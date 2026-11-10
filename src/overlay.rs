//! Fullscreen region-select overlay (config only). OWNED BY: integrator (Claude).
//!
//! A modal, topmost, borderless Win32 window that shows a frozen snapshot of the
//! whole virtual desktop and lets the user pick a rectangle. It runs its own
//! message loop (like a native modal dialog), so it is safe to call from inside
//! the egui settings window.
//!
//! Interaction:
//! - Everything outside the selection is dimmed; a crosshair, size label and a
//!   pixel magnifier help with precise placement.
//! - Hovering highlights the UI element under the cursor (web page elements in
//!   browsers, controls, windows) via UI Automation. A plain click selects it;
//!   the mouse wheel walks up (parent) / down (child) the element chain.
//! - Dragging draws a new rectangle. The selection stays editable: drag the 8
//!   handles to resize, drag inside to move, arrows nudge 1px (Shift = 10px),
//!   Ctrl+arrows resize. Enter / double-click confirms; Esc cancels;
//!   right-click clears the selection (or cancels when there is none).

use crate::model::{CapturedImage, Error, Rect, Result};

use windows::core::{w, BOOL, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation, IUIAutomationTreeWalker};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, ReleaseCapture, SetCapture, VK_CONTROL, VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RETURN,
    VK_RIGHT, VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS_NAME: PCWSTR = w!("XerahS_Overlay");
const HOVER_TIMER: usize = 1;
const HANDLE: i32 = 4; // half size of a resize handle
const GRAB: i32 = 7; // hit-test tolerance for handles/edges
const CLICK_SLOP: i32 = 4; // movement below this counts as a click
const MAG_SRC: i32 = 15; // magnifier: source pixels per side (odd)
const MAG_ZOOM: i32 = 8;

const ACCENT: COLORREF = COLORREF(0x00F0_9A2E); // BGR: soft blue
const HOVER: COLORREF = COLORREF(0x0000_C8FF); // BGR: amber
const WHITE: COLORREF = COLORREF(0x00FF_FFFF);
const BLACK: COLORREF = COLORREF(0);
const LABEL_BG: COLORREF = COLORREF(0x0020_2020);

#[derive(Clone, Copy, PartialEq)]
struct Edges {
    l: bool,
    t: bool,
    r: bool,
    b: bool,
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Create { start: POINT, moved: bool },
    Move { grab: POINT, orig: RECT },
    Resize { edges: Edges, orig: RECT },
}

struct OverlayState {
    hwnd: HWND,
    image: CapturedImage,
    bounds: Rect,
    w: i32,
    h: i32,
    bright_dc: HDC,
    bright_bmp: HBITMAP,
    dim_dc: HDC,
    dim_bmp: HBITMAP,
    back_dc: HDC,
    back_bmp: HBITMAP,
    font: HFONT,
    sel: Option<RECT>,
    drag: Drag,
    cursor: POINT,
    // Element hover (UI Automation)
    uia: Option<(IUIAutomation, IUIAutomationTreeWalker)>,
    windows: Vec<(HWND, RECT)>,
    chain: Vec<RECT>, // screen coords, outermost first
    level: usize,
    looked_up_at: Option<POINT>,
    result: Option<Rect>,
    finished: bool,
}

/// Show the overlay over `background` (a snapshot of the whole virtual desktop,
/// covering `bounds`). `initial` pre-selects a region (virtual-desktop coords).
/// Returns the selected region in virtual-desktop physical coordinates, or
/// `None` if the user cancels.
pub fn select_region(
    background: &CapturedImage,
    bounds: Rect,
    initial: Option<Rect>,
) -> Result<Option<Rect>> {
    if bounds.is_empty() {
        return Err(Error::Other("overlay bounds are empty".into()));
    }
    unsafe {
        let com_ok = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let res = run(background, bounds, initial);
        if com_ok {
            CoUninitialize();
        }
        res
    }
}

unsafe fn run(background: &CapturedImage, bounds: Rect, initial: Option<Rect>) -> Result<Option<Rect>> {
    let hmodule = GetModuleHandleW(None).map_err(|e| Error::Other(e.to_string()))?;
    let hinstance = HINSTANCE(hmodule.0);
    ensure_class(hinstance);

    let w = background.width as i32;
    let h = background.height as i32;
    let sel = initial.and_then(|r| {
        let rc = RECT {
            left: (r.x - bounds.x).clamp(0, w),
            top: (r.y - bounds.y).clamp(0, h),
            right: (r.x - bounds.x + r.width as i32).clamp(0, w),
            bottom: (r.y - bounds.y + r.height as i32).clamp(0, h),
        };
        (rc.right > rc.left && rc.bottom > rc.top).then_some(rc)
    });

    let uia = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        .ok()
        .and_then(|a| a.ControlViewWalker().ok().map(|wk| (a, wk)));

    let state = Box::new(OverlayState {
        hwnd: HWND::default(),
        image: background.clone(),
        bounds,
        w,
        h,
        bright_dc: HDC::default(),
        bright_bmp: HBITMAP::default(),
        dim_dc: HDC::default(),
        dim_bmp: HBITMAP::default(),
        back_dc: HDC::default(),
        back_bmp: HBITMAP::default(),
        font: HFONT::default(),
        sel,
        drag: Drag::None,
        cursor: POINT { x: -1000, y: -1000 },
        uia,
        windows: Vec::new(),
        chain: Vec::new(),
        level: 0,
        looked_up_at: None,
        result: None,
        finished: false,
    });
    let ptr = Box::into_raw(state);

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
        CLASS_NAME,
        w!("XerahS Select Region"),
        WS_POPUP,
        bounds.x,
        bounds.y,
        w,
        h,
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
    (*ptr).hwnd = hwnd;
    (*ptr).windows = collect_windows(hwnd);

    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    SetTimer(Some(hwnd), HOVER_TIMER, 60, None);

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

    let state = Box::from_raw(ptr);
    Ok(state.result)
}

unsafe fn ensure_class(hinstance: HINSTANCE) {
    use std::sync::Once;
    static REGISTER: Once = Once::new();
    REGISTER.call_once(|| {
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME,
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

fn norm(a: POINT, b: POINT) -> RECT {
    RECT { left: a.x.min(b.x), top: a.y.min(b.y), right: a.x.max(b.x), bottom: a.y.max(b.y) }
}

fn contains(r: &RECT, p: POINT) -> bool {
    p.x >= r.left && p.x < r.right && p.y >= r.top && p.y < r.bottom
}

fn key_down(vk: u16) -> bool {
    unsafe { GetKeyState(vk as i32) < 0 }
}

/// Which edges of `r` the point grabs (handles/edges), or None.
fn hit_edges(r: &RECT, p: POINT) -> Option<Edges> {
    let near = |a: i32, b: i32| (a - b).abs() <= GRAB;
    let in_x = p.x >= r.left - GRAB && p.x <= r.right + GRAB;
    let in_y = p.y >= r.top - GRAB && p.y <= r.bottom + GRAB;
    let e = Edges {
        l: in_y && near(p.x, r.left),
        r: in_y && near(p.x, r.right),
        t: in_x && near(p.y, r.top),
        b: in_x && near(p.y, r.bottom),
    };
    (e.l || e.r || e.t || e.b).then_some(e)
}

impl OverlayState {
    fn clamp(&self, mut r: RECT) -> RECT {
        r.left = r.left.clamp(0, self.w);
        r.right = r.right.clamp(0, self.w);
        r.top = r.top.clamp(0, self.h);
        r.bottom = r.bottom.clamp(0, self.h);
        r
    }

    /// Hovered element rect in client coords (clipped), if any.
    fn hover_rect(&self) -> Option<RECT> {
        if !matches!(self.drag, Drag::None) {
            return None;
        }
        if let Some(s) = &self.sel {
            if contains(s, self.cursor) || hit_edges(s, self.cursor).is_some() {
                return None;
            }
        }
        let r = self.chain.get(self.level)?;
        let c = self.clamp(RECT {
            left: r.left - self.bounds.x,
            top: r.top - self.bounds.y,
            right: r.right - self.bounds.x,
            bottom: r.bottom - self.bounds.y,
        });
        (c.right > c.left && c.bottom > c.top).then_some(c)
    }

    fn confirm(&mut self) {
        if let Some(r) = self.sel {
            if r.right > r.left && r.bottom > r.top {
                self.result = Some(Rect::new(
                    self.bounds.x + r.left,
                    self.bounds.y + r.top,
                    (r.right - r.left) as u32,
                    (r.bottom - r.top) as u32,
                ));
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
            }
        }
    }

    fn cancel(&mut self) {
        self.result = None;
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CREATE {
        let cs = lparam.0 as *const CREATESTRUCTW;
        if !cs.is_null() {
            let ptr = (*cs).lpCreateParams as *mut OverlayState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, ptr as isize);
            if !ptr.is_null() {
                build_buffers(&mut *ptr);
            }
        }
        return LRESULT(0);
    }
    let st = state_ptr(hwnd);
    if st.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let s = &mut *st;
    let redraw = || {
        let _ = InvalidateRect(Some(hwnd), None, false);
    };

    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_SETCURSOR => {
            let id = match (s.drag, s.sel) {
                (Drag::Move { .. }, _) => IDC_SIZEALL,
                (Drag::Resize { edges, .. }, _) => edge_cursor(edges),
                (Drag::Create { .. }, _) => IDC_CROSS,
                (Drag::None, Some(r)) => match hit_edges(&r, s.cursor) {
                    Some(e) => edge_cursor(e),
                    None if contains(&r, s.cursor) => IDC_SIZEALL,
                    None => IDC_CROSS,
                },
                _ => IDC_CROSS,
            };
            SetCursor(LoadCursorW(None, id).ok());
            LRESULT(1)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            let p = point_from_lparam(lparam);
            s.cursor = p;
            if msg == WM_LBUTTONDBLCLK {
                if let Some(r) = s.sel {
                    if contains(&r, p) {
                        s.confirm();
                        return LRESULT(0);
                    }
                }
            }
            s.drag = match s.sel {
                Some(r) => match hit_edges(&r, p) {
                    Some(edges) => Drag::Resize { edges, orig: r },
                    None if contains(&r, p) => Drag::Move { grab: p, orig: r },
                    None => Drag::Create { start: p, moved: false },
                },
                None => Drag::Create { start: p, moved: false },
            };
            let _ = SetCapture(hwnd);
            redraw();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let p = point_from_lparam(lparam);
            s.cursor = p;
            match &mut s.drag {
                Drag::Create { start, moved } => {
                    if !*moved && ((p.x - start.x).abs() > CLICK_SLOP || (p.y - start.y).abs() > CLICK_SLOP) {
                        *moved = true;
                    }
                    if *moved {
                        let r = norm(*start, p);
                        s.sel = Some(s.clamp(r));
                    }
                }
                Drag::Move { grab, orig } => {
                    let (rw, rh) = (orig.right - orig.left, orig.bottom - orig.top);
                    let nl = (orig.left + p.x - grab.x).clamp(0, (s.w - rw).max(0));
                    let nt = (orig.top + p.y - grab.y).clamp(0, (s.h - rh).max(0));
                    s.sel = Some(RECT { left: nl, top: nt, right: nl + rw, bottom: nt + rh });
                }
                Drag::Resize { edges, orig } => {
                    let mut r = *orig;
                    let q = POINT { x: p.x.clamp(0, s.w), y: p.y.clamp(0, s.h) };
                    if edges.l { r.left = q.x; }
                    if edges.r { r.right = q.x; }
                    if edges.t { r.top = q.y; }
                    if edges.b { r.bottom = q.y; }
                    s.sel = Some(norm(POINT { x: r.left, y: r.top }, POINT { x: r.right, y: r.bottom }));
                }
                Drag::None => {}
            }
            redraw();
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let p = point_from_lparam(lparam);
            if let Drag::Create { moved: false, .. } = s.drag {
                // A click: pick the hovered element, or clear.
                s.drag = Drag::None;
                s.cursor = p;
                s.sel = None;
                if let Some(r) = s.chain.get(s.level).copied() {
                    let c = s.clamp(RECT {
                        left: r.left - s.bounds.x,
                        top: r.top - s.bounds.y,
                        right: r.right - s.bounds.x,
                        bottom: r.bottom - s.bounds.y,
                    });
                    if c.right > c.left && c.bottom > c.top {
                        s.sel = Some(c);
                    }
                }
            }
            s.drag = Drag::None;
            if let Some(r) = s.sel {
                if r.right - r.left < 1 || r.bottom - r.top < 1 {
                    s.sel = None;
                }
            }
            redraw();
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if s.sel.is_some() {
                s.sel = None;
                s.drag = Drag::None;
                redraw();
            } else {
                s.cancel();
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16;
            if !s.chain.is_empty() {
                if delta > 0 {
                    s.level = s.level.saturating_sub(1); // toward parent
                } else {
                    s.level = (s.level + 1).min(s.chain.len() - 1);
                }
                redraw();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == HOVER_TIMER && matches!(s.drag, Drag::None) && s.looked_up_at != Some(s.cursor) {
                s.looked_up_at = Some(s.cursor);
                let sp = POINT { x: s.cursor.x + s.bounds.x, y: s.cursor.y + s.bounds.y };
                let chain = element_chain(s, sp);
                if chain != s.chain {
                    s.level = chain.len().saturating_sub(1);
                    s.chain = chain;
                    redraw();
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let vk = wparam.0 as u16;
            if vk == VK_ESCAPE.0 {
                s.cancel();
            } else if vk == VK_RETURN.0 {
                s.confirm();
            } else if [VK_LEFT.0, VK_RIGHT.0, VK_UP.0, VK_DOWN.0].contains(&vk) {
                if let Some(mut r) = s.sel {
                    let step = if key_down(VK_SHIFT.0) { 10 } else { 1 };
                    let (dx, dy) = match vk {
                        v if v == VK_LEFT.0 => (-step, 0),
                        v if v == VK_RIGHT.0 => (step, 0),
                        v if v == VK_UP.0 => (0, -step),
                        _ => (0, step),
                    };
                    if key_down(VK_CONTROL.0) {
                        r.right = (r.right + dx).clamp(r.left + 1, s.w);
                        r.bottom = (r.bottom + dy).clamp(r.top + 1, s.h);
                    } else {
                        let (rw, rh) = (r.right - r.left, r.bottom - r.top);
                        r.left = (r.left + dx).clamp(0, (s.w - rw).max(0));
                        r.top = (r.top + dy).clamp(0, (s.h - rh).max(0));
                        r.right = r.left + rw;
                        r.bottom = r.top + rh;
                    }
                    s.sel = Some(r);
                    redraw();
                }
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd, s);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            let _ = KillTimer(Some(hwnd), HOVER_TIMER);
            free_buffers(s);
            s.finished = true;
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn edge_cursor(e: Edges) -> PCWSTR {
    match (e.l || e.r, e.t || e.b) {
        (true, true) if (e.l && e.t) || (e.r && e.b) => IDC_SIZENWSE,
        (true, true) => IDC_SIZENESW,
        (true, false) => IDC_SIZEWE,
        _ => IDC_SIZENS,
    }
}

// ---------------------------------------------------------------------------
// Element hover (UI Automation)

struct EnumCtx {
    skip: HWND,
    out: Vec<(HWND, RECT)>,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lp: LPARAM) -> BOOL {
    let ctx = &mut *(lp.0 as *mut EnumCtx);
    if hwnd == ctx.skip || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
        return BOOL(1);
    }
    let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TRANSPARENT.0 != 0 {
        return BOOL(1);
    }
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        &mut cloaked as *mut _ as *mut _,
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked != 0 {
        return BOOL(1);
    }
    let mut r = RECT::default();
    if DwmGetWindowAttribute(
        hwnd,
        DWMWA_EXTENDED_FRAME_BOUNDS,
        &mut r as *mut _ as *mut _,
        std::mem::size_of::<RECT>() as u32,
    )
    .is_err()
    {
        let _ = GetWindowRect(hwnd, &mut r);
    }
    if r.right > r.left && r.bottom > r.top {
        ctx.out.push((hwnd, r));
    }
    BOOL(1)
}

/// Visible top-level windows in Z order (topmost first), excluding `skip`.
unsafe fn collect_windows(skip: HWND) -> Vec<(HWND, RECT)> {
    let mut ctx = EnumCtx { skip, out: Vec::new() };
    let _ = EnumWindows(Some(enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    ctx.out
}

/// Rects (screen coords) from the top-level window under `p` down to the
/// deepest UI element containing it.
unsafe fn element_chain(s: &OverlayState, p: POINT) -> Vec<RECT> {
    let mut chain = Vec::new();
    let Some(&(hwnd, wr)) = s.windows.iter().find(|(_, r)| contains(r, p)) else {
        return chain;
    };
    chain.push(wr);
    let Some((uia, walker)) = &s.uia else { return chain };
    let Ok(mut cur) = uia.ElementFromHandle(hwnd) else { return chain };
    let started = std::time::Instant::now();

    'depth: for _ in 0..64 {
        let mut child = walker.GetFirstChildElement(&cur).ok();
        let mut n = 0;
        while let Some(c) = child {
            if n > 400 || started.elapsed().as_millis() > 200 {
                break 'depth;
            }
            n += 1;
            if let Ok(r) = c.CurrentBoundingRectangle() {
                if r.right > r.left && r.bottom > r.top && contains(&r, p) {
                    if chain.last() != Some(&r) {
                        chain.push(r);
                    }
                    cur = c;
                    continue 'depth;
                }
            }
            child = walker.GetNextSiblingElement(&c).ok();
        }
        break;
    }
    chain
}

// ---------------------------------------------------------------------------
// Drawing

unsafe fn make_bitmap(screen: HDC, w: i32, h: i32, bgra: Option<&[u8]>) -> (HDC, HBITMAP) {
    let mem = CreateCompatibleDC(Some(screen));
    let bmp = CreateCompatibleBitmap(screen, w, h);
    if let Some(px) = bgra {
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
        SetDIBits(Some(screen), bmp, 0, h as u32, px.as_ptr() as *const _, &bmi, DIB_RGB_COLORS);
    }
    SelectObject(mem, bmp.into());
    (mem, bmp)
}

unsafe fn build_buffers(s: &mut OverlayState) {
    let (w, h) = (s.w, s.h);
    let screen = GetDC(None);
    let valid = s.image.bgra.len() == (w as usize) * (h as usize) * 4;
    let dim: Vec<u8> = if valid {
        s.image
            .bgra
            .chunks_exact(4)
            .flat_map(|p| [p[0] / 5 * 2, p[1] / 5 * 2, p[2] / 5 * 2, 255])
            .collect()
    } else {
        Vec::new()
    };
    (s.bright_dc, s.bright_bmp) = make_bitmap(screen, w, h, valid.then_some(&s.image.bgra[..]));
    (s.dim_dc, s.dim_bmp) = make_bitmap(screen, w, h, valid.then_some(&dim[..]));
    (s.back_dc, s.back_bmp) = make_bitmap(screen, w, h, None);
    ReleaseDC(None, screen);

    s.font = CreateFontW(
        -17, 0, 0, 0, 600, 0, 0, 0,
        DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY,
        0, w!("Segoe UI"),
    );
    SelectObject(s.back_dc, s.font.into());
}

unsafe fn free_buffers(s: &mut OverlayState) {
    for (dc, bmp) in [
        (&mut s.bright_dc, &mut s.bright_bmp),
        (&mut s.dim_dc, &mut s.dim_bmp),
        (&mut s.back_dc, &mut s.back_bmp),
    ] {
        if !dc.is_invalid() {
            let _ = DeleteDC(*dc);
            *dc = HDC::default();
        }
        if !bmp.is_invalid() {
            let _ = DeleteObject((*bmp).into());
            *bmp = HBITMAP::default();
        }
    }
    if !s.font.is_invalid() {
        let _ = DeleteObject(s.font.into());
        s.font = HFONT::default();
    }
}

unsafe fn frame(dc: HDC, r: &RECT, color: COLORREF, width: i32) {
    let pen = CreatePen(PS_SOLID, width, color);
    let old_pen = SelectObject(dc, pen.into());
    let old_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
    let _ = Rectangle(dc, r.left, r.top, r.right, r.bottom);
    SelectObject(dc, old_pen);
    SelectObject(dc, old_brush);
    let _ = DeleteObject(pen.into());
}

unsafe fn fill(dc: HDC, r: &RECT, color: COLORREF) {
    let b = CreateSolidBrush(color);
    FillRect(dc, r, b);
    let _ = DeleteObject(b.into());
}

/// Draw a pill label with its top-left at (x, y), kept inside the client area.
/// Returns its rect.
unsafe fn label(s: &OverlayState, x: i32, y: i32, text: &str, accent: COLORREF) -> RECT {
    let dc = s.back_dc;
    let t: Vec<u16> = text.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, &t, &mut sz);
    let (pw, ph) = (sz.cx + 16, sz.cy + 8);
    let x = x.clamp(0, (s.w - pw).max(0));
    let y = y.clamp(0, (s.h - ph).max(0));
    let r = RECT { left: x, top: y, right: x + pw, bottom: y + ph };
    fill(dc, &r, LABEL_BG);
    fill(dc, &RECT { left: x, top: y, right: x + 3, bottom: y + ph }, accent);
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, WHITE);
    let _ = TextOutW(dc, x + 9, y + 4, &t);
    r
}

unsafe fn paint(hwnd: HWND, s: &OverlayState) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let dc = s.back_dc;
    let (w, h) = (s.w, s.h);
    let _ = BitBlt(dc, 0, 0, w, h, Some(s.dim_dc), 0, 0, SRCCOPY);

    let blit_bright = |r: &RECT| {
        let _ = BitBlt(dc, r.left, r.top, r.right - r.left, r.bottom - r.top, Some(s.bright_dc), r.left, r.top, SRCCOPY);
    };

    // Hovered element
    if let Some(hr) = s.hover_rect() {
        if s.sel.is_none() {
            blit_bright(&hr);
        }
        frame(dc, &hr, HOVER, 2);
        let txt = format!(
            "{} × {}   click to select · wheel: parent/child ({}/{})",
            hr.right - hr.left,
            hr.bottom - hr.top,
            s.level + 1,
            s.chain.len()
        );
        let ly = if hr.top >= 34 { hr.top - 32 } else { hr.bottom + 4 };
        label(s, hr.left, ly, &txt, HOVER);
    }

    // Selection
    let creating = matches!(s.drag, Drag::Create { .. });
    if let Some(r) = s.sel {
        blit_bright(&r);
        frame(dc, &RECT { left: r.left - 1, top: r.top - 1, right: r.right + 1, bottom: r.bottom + 1 }, BLACK, 1);
        frame(dc, &r, ACCENT, 1);
        if !creating {
            let (cx, cy) = ((r.left + r.right) / 2, (r.top + r.bottom) / 2);
            for (x, y) in [
                (r.left, r.top), (cx, r.top), (r.right, r.top),
                (r.left, cy), (r.right, cy),
                (r.left, r.bottom), (cx, r.bottom), (r.right, r.bottom),
            ] {
                let hr = RECT { left: x - HANDLE, top: y - HANDLE, right: x + HANDLE + 1, bottom: y + HANDLE + 1 };
                fill(dc, &hr, WHITE);
                frame(dc, &hr, ACCENT, 1);
            }
        }
        let txt = format!(
            "{} × {}   x {}  y {}",
            r.right - r.left,
            r.bottom - r.top,
            r.left + s.bounds.x,
            r.top + s.bounds.y
        );
        let ly = if r.top >= 38 { r.top - 36 } else { r.top + 6 };
        label(s, r.left, ly, &txt, ACCENT);
    }

    // Crosshair + magnifier while placing an edge or with nothing selected
    let precise = creating || matches!(s.drag, Drag::Resize { .. }) || (s.sel.is_none() && matches!(s.drag, Drag::None));
    let c = s.cursor;
    if precise && c.x >= 0 && c.y >= 0 && c.x < w && c.y < h {
        let pen = CreatePen(PS_DOT, 1, WHITE);
        let old = SelectObject(dc, pen.into());
        SetBkMode(dc, OPAQUE);
        SetBkColor(dc, BLACK);
        let _ = MoveToEx(dc, 0, c.y, None);
        let _ = LineTo(dc, w, c.y);
        let _ = MoveToEx(dc, c.x, 0, None);
        let _ = LineTo(dc, c.x, h);
        SelectObject(dc, old);
        let _ = DeleteObject(pen.into());

        draw_magnifier(s, c);
    }

    // Hint bar at top of the monitor under the cursor
    let hint = "Drag to select · Click a highlighted element to snap · Drag handles to adjust · Arrows nudge (Shift ×10, Ctrl resize) · Enter / double-click confirm · Right-click clear · Esc cancel";
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let mon = MonitorFromPoint(POINT { x: c.x + s.bounds.x, y: c.y + s.bounds.y }, MONITOR_DEFAULTTONEAREST);
    let (mx, my, mw, mh) = if GetMonitorInfoW(mon, &mut mi).as_bool() {
        let m = mi.rcMonitor;
        (m.left - s.bounds.x, m.top - s.bounds.y, m.right - m.left, m.bottom - m.top)
    } else {
        (0, 0, w, h)
    };
    let t: Vec<u16> = hint.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(dc, &t, &mut sz);
    let hx = mx + (mw - sz.cx - 16) / 2;
    let hy = my + 12;
    // Don't cover the cursor/selection: move the bar down when it would.
    let bar = RECT { left: hx, top: hy, right: hx + sz.cx + 16, bottom: hy + sz.cy + 8 };
    // Move the bar to the bottom when the cursor is near the top.
    let near_bar = c.y < bar.bottom + 40;
    label(s, hx, if near_bar { my + mh - (bar.bottom - bar.top) - 12 } else { hy }, hint, ACCENT);

    let _ = BitBlt(hdc, 0, 0, w, h, Some(dc), 0, 0, SRCCOPY);
    let _ = EndPaint(hwnd, &ps);
}

unsafe fn draw_magnifier(s: &OverlayState, c: POINT) {
    let dc = s.back_dc;
    let size = MAG_SRC * MAG_ZOOM;
    let mut x = c.x + 24;
    let mut y = c.y + 24;
    if x + size > s.w {
        x = c.x - 24 - size;
    }
    if y + size + 30 > s.h {
        y = c.y - 24 - size - 30;
    }
    let half = MAG_SRC / 2;
    SetStretchBltMode(dc, COLORONCOLOR);
    fill(dc, &RECT { left: x, top: y, right: x + size, bottom: y + size }, BLACK);
    let _ = StretchBlt(
        dc, x, y, size, size,
        Some(s.bright_dc), c.x - half, c.y - half, MAG_SRC, MAG_SRC,
        SRCCOPY,
    );
    // Center pixel marker
    let cx = x + half * MAG_ZOOM;
    let cy = y + half * MAG_ZOOM;
    frame(dc, &RECT { left: cx, top: cy, right: cx + MAG_ZOOM, bottom: cy + MAG_ZOOM }, BLACK, 1);
    frame(dc, &RECT { left: cx - 1, top: cy - 1, right: cx + MAG_ZOOM + 1, bottom: cy + MAG_ZOOM + 1 }, WHITE, 1);
    frame(dc, &RECT { left: x - 1, top: y - 1, right: x + size + 1, bottom: y + size + 1 }, ACCENT, 2);
    label(
        s,
        x,
        y + size + 4,
        &format!("{}, {}", c.x + s.bounds.x, c.y + s.bounds.y),
        ACCENT,
    );
}
