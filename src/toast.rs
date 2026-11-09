//! ShareX-style custom-drawn toast notification. OWNED BY: integrator (Claude).
//!
//! A topmost, borderless, layered window shown in a configurable screen corner,
//! with a thumbnail of the capture + the file name, fade in / hold / fade out,
//! and configurable left/middle/right click actions. Each toast runs on its own
//! thread with its own message loop so it never blocks the capture hot path.

use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::thread;
use std::time::Instant;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, SetBkMode,
    SetStretchBltMode, SetTextColor, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, DT_END_ELLIPSIS, DT_LEFT, DT_SINGLELINE, DT_VCENTER, HALFTONE, HDC,
    PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetMessageW, GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW,
    SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, ShowWindow, SystemParametersInfoW,
    TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, IDC_ARROW, LWA_ALPHA, MSG, SPI_GETWORKAREA,
    SW_SHOWNOACTIVATE, SW_SHOWNORMAL, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_CREATE, WM_LBUTTONUP,
    WM_MBUTTONUP, WM_NCDESTROY, WM_PAINT, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::model::{CapturedImage, ToastAction, ToastConfig, ToastCorner};

const CLASS_NAME: PCWSTR = w!("REA_ShareX_Toast");
const TIMER_ID: usize = 1;

struct ToastState {
    image: CapturedImage,
    path: PathBuf,
    config: ToastConfig,
    start: Instant,
}

/// Show a toast for a just-saved capture. Non-fatal: failures are swallowed so a
/// toast problem never blocks the capture pipeline.
pub fn show_toast(image: &CapturedImage, saved_path: &Path, config: &ToastConfig) {
    if !config.enabled {
        return;
    }
    let state = ToastState {
        image: image.clone(),
        path: saved_path.to_path_buf(),
        config: config.clone(),
        start: Instant::now(),
    };
    thread::spawn(move || unsafe { run(state) });
}

unsafe fn run(state: ToastState) {
    let hmodule = match GetModuleHandleW(None) {
        Ok(h) => h,
        Err(_) => return,
    };
    let hinstance = HINSTANCE(hmodule.0);
    ensure_class(hinstance);

    let (x, y) = position(&state.config);
    let w = state.config.width as i32;
    let h = state.config.height as i32;

    let ptr = Box::into_raw(Box::new(state));
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        CLASS_NAME,
        w!("REA_ShareX"),
        WS_POPUP,
        x,
        y,
        w,
        h,
        None,
        None,
        Some(hinstance),
        Some(ptr as *const _),
    );
    let hwnd = match hwnd {
        Ok(h) => h,
        Err(_) => {
            drop(Box::from_raw(ptr));
            return;
        }
    };

    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
    let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(Some(hwnd), TIMER_ID, 15, None);

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe fn ensure_class(hinstance: HINSTANCE) {
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
}

unsafe fn work_area() -> RECT {
    let mut rc = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut rc as *mut _ as *mut _),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    rc
}

unsafe fn position(cfg: &ToastConfig) -> (i32, i32) {
    let wa = work_area();
    let w = cfg.width as i32;
    let h = cfg.height as i32;
    let m = cfg.margin;
    match cfg.corner {
        ToastCorner::TopLeft => (wa.left + m, wa.top + m),
        ToastCorner::TopRight => (wa.right - w - m, wa.top + m),
        ToastCorner::BottomLeft => (wa.left + m, wa.bottom - h - m),
        ToastCorner::BottomRight => (wa.right - w - m, wa.bottom - h - m),
    }
}

unsafe fn state_ptr(hwnd: HWND) -> *mut ToastState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut ToastState
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = lparam.0 as *const CREATESTRUCTW;
            if !cs.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*cs).lpCreateParams as isize);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                let s = &*st;
                let elapsed = s.start.elapsed().as_millis() as u32;
                let fade = s.config.fade_ms.max(1);
                let total = fade + s.config.duration_ms + fade;
                if elapsed >= total {
                    let _ = DestroyWindow(hwnd);
                } else {
                    let alpha = if elapsed < fade {
                        (elapsed * 255 / fade) as u8
                    } else if elapsed < fade + s.config.duration_ms {
                        255
                    } else {
                        ((total - elapsed) * 255 / fade) as u8
                    };
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            click(hwnd, |c| c.left_click);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            click(hwnd, |c| c.right_click);
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            click(hwnd, |c| c.middle_click);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                drop(Box::from_raw(st));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn click(hwnd: HWND, pick: impl Fn(&ToastConfig) -> ToastAction) {
    let st = state_ptr(hwnd);
    if !st.is_null() {
        let s = &*st;
        perform(pick(&s.config), &s.path);
    }
    let _ = DestroyWindow(hwnd);
}

unsafe fn perform(action: ToastAction, path: &Path) {
    match action {
        ToastAction::None => {}
        ToastAction::OpenFile => shell_open(path),
        ToastAction::OpenFolder => {
            if let Some(dir) = path.parent() {
                shell_open(dir);
            }
        }
        ToastAction::CopyPath => copy_to_clipboard(&path.to_string_lossy()),
    }
}

unsafe fn shell_open(p: &Path) {
    let wide = to_wide(&p.to_string_lossy());
    ShellExecuteW(
        None,
        w!("open"),
        PCWSTR(wide.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        SW_SHOWNORMAL,
    );
}

unsafe fn copy_to_clipboard(text: &str) {
    let wide = to_wide(text);
    let bytes = wide.len() * 2;
    if OpenClipboard(None).is_err() {
        return;
    }
    let _ = EmptyClipboard();
    if let Ok(hglobal) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
        let dst = GlobalLock(hglobal) as *mut u16;
        if !dst.is_null() {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            let _ = GlobalUnlock(hglobal);
            let _ = SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hglobal.0)));
        }
    }
    let _ = CloseClipboard();
}

unsafe fn paint(hwnd: HWND) {
    let st = state_ptr(hwnd);
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    let bg = CreateSolidBrush(COLORREF(0x0020_2020)); // dark gray, 0x00BBGGRR
    FillRect(hdc, &rc, bg);
    let _ = DeleteObject(bg.into());

    if !st.is_null() {
        let s = &*st;
        let pad = 8;
        let text_h = 28;

        if s.config.show_thumbnail {
            let area = RECT {
                left: pad,
                top: pad,
                right: rc.right - pad,
                bottom: rc.bottom - text_h - pad,
            };
            draw_thumbnail(hdc, &s.image, &area);
        }

        let name = s
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // DrawTextW uses the slice length, so do NOT include a null terminator.
        let mut text: Vec<u16> = name.encode_utf16().collect();
        let mut trc = RECT {
            left: pad,
            top: rc.bottom - text_h,
            right: rc.right - pad,
            bottom: rc.bottom,
        };
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, COLORREF(0x00FF_FFFF));
        DrawTextW(
            hdc,
            &mut text[..],
            &mut trc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
    }

    let _ = EndPaint(hwnd, &ps);
}

unsafe fn draw_thumbnail(hdc: HDC, img: &CapturedImage, area: &RECT) {
    let aw = (area.right - area.left).max(1);
    let ah = (area.bottom - area.top).max(1);
    let iw = img.width as i32;
    let ih = img.height as i32;
    if iw <= 0 || ih <= 0 {
        return;
    }

    // Fit preserving aspect ratio, centered in `area`.
    let scale = f64::min(aw as f64 / iw as f64, ah as f64 / ih as f64);
    let dw = ((iw as f64 * scale).round() as i32).max(1);
    let dh = ((ih as f64 * scale).round() as i32).max(1);
    let dx = area.left + (aw - dw) / 2;
    let dy = area.top + (ah - dh) / 2;

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: iw,
            biHeight: -ih, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    SetStretchBltMode(hdc, HALFTONE);
    StretchDIBits(
        hdc,
        dx,
        dy,
        dw,
        dh,
        0,
        0,
        iw,
        ih,
        Some(img.bgra.as_ptr() as *const _),
        &bmi,
        DIB_RGB_COLORS,
        SRCCOPY,
    );
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
