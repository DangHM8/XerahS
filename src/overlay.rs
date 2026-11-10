//! Fullscreen drag-select overlay (config only). OWNED BY: integrator (Claude).
//!
//! A modal, topmost, borderless Win32 window that shows a frozen snapshot of the
//! whole virtual desktop and lets the user drag a rectangle. It runs its own
//! message loop (like a native modal dialog), so it is safe to call from inside
//! the egui settings window.

use crate::model::{CapturedImage, Error, Rect, Result};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreatePen, DeleteDC,
    DeleteObject, GetDC, GetStockObject, InvalidateRect, ReleaseDC, Rectangle, SelectObject,
    SetDIBits, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HBRUSH, HDC,
    NULL_BRUSH, PS_SOLID, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{SetCapture, ReleaseCapture, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, LoadCursorW,
    PostQuitMessage, RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, GetWindowLongPtrW,
    ShowWindow, TranslateMessage, CREATESTRUCTW, GWLP_USERDATA, IDC_CROSS, MSG, SW_SHOW,
    WM_CREATE, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCDESTROY,
    WM_PAINT, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::Foundation::{HINSTANCE, COLORREF};

const CLASS_NAME: PCWSTR = w!("XerahS_Overlay");

struct OverlayState {
    image: CapturedImage,
    bounds: Rect,
    mem_dc: HDC,
    bmp: HBITMAP,
    selecting: bool,
    start: POINT,
    cur: POINT,
    result: Option<Rect>,
    finished: bool,
}

/// Show the overlay over `background` (a snapshot of the whole virtual desktop,
/// covering `bounds`). Returns the selected region in virtual-desktop physical
/// coordinates, or `None` if the user cancels (Esc / right-click / empty drag).
pub fn select_region(background: &CapturedImage, bounds: Rect) -> Result<Option<Rect>> {
    if bounds.is_empty() {
        return Err(Error::Other("overlay bounds are empty".into()));
    }
    unsafe {
        let hmodule = GetModuleHandleW(None).map_err(|e| Error::Other(e.to_string()))?;
        let hinstance = HINSTANCE(hmodule.0);
        ensure_class(hinstance);

        let state = Box::new(OverlayState {
            image: background.clone(),
            bounds,
            mem_dc: HDC::default(),
            bmp: HBITMAP::default(),
            selecting: false,
            start: POINT::default(),
            cur: POINT::default(),
            result: None,
            finished: false,
        });
        let ptr = Box::into_raw(state);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS_NAME,
            w!("XerahS Select Region"),
            WS_POPUP | WS_VISIBLE,
            bounds.x,
            bounds.y,
            bounds.width as i32,
            bounds.height as i32,
            None,
            None,
            Some(hinstance),
            Some(ptr as *const _),
        );
        let hwnd = match hwnd {
            Ok(h) => h,
            Err(e) => {
                drop(Box::from_raw(ptr));
                return Err(Error::Other(format!("overlay window: {e}")));
            }
        };

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0).0;
            if ret == -1 {
                break;
            }
            if ret == 0 {
                // External WM_QUIT: tear down our window, then re-post it for the outer loop.
                let _ = DestroyWindow(hwnd);
                PostQuitMessage(0);
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if (*ptr).finished {
                break;
            }
        }

        // Reclaim the box (WM_NCDESTROY only freed GDI handles) and read the result.
        let state = Box::from_raw(ptr);
        Ok(state.result)
    }
}

unsafe fn ensure_class(hinstance: HINSTANCE) {
    use std::sync::Once;
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_CROSS).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    });
}

unsafe fn state_ptr(hwnd: HWND) -> *mut OverlayState {
    GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut OverlayState
}

fn point_from_lparam(lp: LPARAM) -> POINT {
    let v = lp.0 as u32;
    POINT {
        x: (v & 0xFFFF) as i16 as i32,
        y: ((v >> 16) & 0xFFFF) as i16 as i32,
    }
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
                let ptr = (*cs).lpCreateParams as *mut OverlayState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
                if !ptr.is_null() {
                    build_background(&mut *ptr);
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1), // we paint everything ourselves
        WM_LBUTTONDOWN => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                let s = &mut *st;
                let p = point_from_lparam(lparam);
                s.selecting = true;
                s.start = p;
                s.cur = p;
                let _ = SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                let s = &mut *st;
                if s.selecting {
                    s.cur = point_from_lparam(lparam);
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                let s = &mut *st;
                s.selecting = false;
                let _ = ReleaseCapture();
                s.cur = point_from_lparam(lparam);
                s.result = finalize(s);
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                (*st).result = None;
            }
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 as u16 == VK_ESCAPE.0 {
                let st = state_ptr(hwnd);
                if !st.is_null() {
                    (*st).result = None;
                }
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let st = state_ptr(hwnd);
            if !st.is_null() {
                let s = &mut *st;
                if !s.mem_dc.is_invalid() {
                    let _ = DeleteDC(s.mem_dc);
                    s.mem_dc = HDC::default();
                }
                if !s.bmp.is_invalid() {
                    let _ = DeleteObject(s.bmp.into());
                    s.bmp = HBITMAP::default();
                }
                s.finished = true;
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Build a memory DC holding the background image for fast repaint.
unsafe fn build_background(s: &mut OverlayState) {
    let w = s.image.width as i32;
    let h = s.image.height as i32;
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, w, h);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SetDIBits requires the bitmap NOT be selected into a DC; fill it before selecting.
    if s.image.bgra.len() == (s.image.width as usize) * (s.image.height as usize) * 4 {
        SetDIBits(
            Some(screen),
            bmp,
            0,
            h as u32,
            s.image.bgra.as_ptr() as *const _,
            &bmi,
            DIB_RGB_COLORS,
        );
    }
    SelectObject(mem, bmp.into());
    ReleaseDC(None, screen);

    s.mem_dc = mem;
    s.bmp = bmp;
}

unsafe fn paint(hwnd: HWND) {
    let st = state_ptr(hwnd);
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    if !st.is_null() {
        let s = &*st;
        let w = s.image.width as i32;
        let h = s.image.height as i32;
        let _ = BitBlt(hdc, 0, 0, w, h, Some(s.mem_dc), 0, 0, SRCCOPY);

        if s.selecting || s.start.x != s.cur.x || s.start.y != s.cur.y {
            let (l, t, r, b) = norm_rect(s.start, s.cur);
            let pen = CreatePen(PS_SOLID, 2, COLORREF(0x0000_40FF)); // red-ish (BGR)
            let hollow = HBRUSH(GetStockObject(NULL_BRUSH).0);
            let old_pen = SelectObject(hdc, pen.into());
            let old_brush = SelectObject(hdc, hollow.into());
            let _ = Rectangle(hdc, l, t, r, b);
            SelectObject(hdc, old_pen);
            SelectObject(hdc, old_brush);
            let _ = DeleteObject(pen.into());
        }
    }
    let _ = EndPaint(hwnd, &ps);
}

fn norm_rect(a: POINT, b: POINT) -> (i32, i32, i32, i32) {
    (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y))
}

/// Compute the selected region in virtual-desktop coordinates, or None if empty.
fn finalize(s: &OverlayState) -> Option<Rect> {
    let (l, t, r, b) = norm_rect(s.start, s.cur);
    let w = (r - l).max(0) as u32;
    let h = (b - t).max(0) as u32;
    if w == 0 || h == 0 {
        return None;
    }
    Some(Rect::new(s.bounds.x + l, s.bounds.y + t, w, h))
}
