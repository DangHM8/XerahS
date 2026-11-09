//! Screen capture via GDI BitBlt. OWNED BY: integrator (Claude).
//!
//! Backend choice: for on-demand single-shot capture of a small fixed region,
//! GDI `BitBlt` of just the target rectangle gives the lowest latency. DXGI
//! Desktop Duplication is intentionally NOT used (it is tuned for continuous
//! high-FPS capture and returns no frame on a static desktop).

use crate::model::{CapturedImage, Error, Rect, Result};

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HDC,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

/// Bounds of the virtual desktop (origin + total size), in physical pixels.
pub fn virtual_screen_bounds() -> Result<Rect> {
    // SAFETY: GetSystemMetrics has no preconditions.
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    if w <= 0 || h <= 0 {
        return Err(Error::Capture("virtual screen has zero size".into()));
    }
    Ok(Rect::new(x, y, w as u32, h as u32))
}

/// Capture the entire virtual desktop (all monitors) into one BGRA image.
pub fn capture_fullscreen() -> Result<CapturedImage> {
    capture_rect(virtual_screen_bounds()?)
}

/// Capture exactly `region` (virtual-desktop physical pixels) into a BGRA image.
pub fn capture_rect(region: Rect) -> Result<CapturedImage> {
    if region.is_empty() {
        return Err(Error::Capture("capture region is empty".into()));
    }
    let w = region.width as i32;
    let h = region.height as i32;

    // SAFETY: each GDI object is released/deleted before returning on every path.
    unsafe {
        let screen_dc: HDC = GetDC(None);
        if screen_dc.is_invalid() {
            return Err(Error::Capture("GetDC failed".into()));
        }

        let result = (|| {
            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                return Err(Error::Capture("CreateCompatibleDC failed".into()));
            }

            let bitmap = CreateCompatibleBitmap(screen_dc, w, h);
            if bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                return Err(Error::Capture("CreateCompatibleBitmap failed".into()));
            }

            let old = SelectObject(mem_dc, bitmap.into());

            let blt = BitBlt(
                mem_dc,
                0,
                0,
                w,
                h,
                Some(screen_dc),
                region.x,
                region.y,
                SRCCOPY | CAPTUREBLT,
            );

            let out = if blt.is_ok() {
                read_pixels(mem_dc, bitmap, w, h)
            } else {
                Err(Error::Capture("BitBlt failed".into()))
            };

            // Cleanup GDI objects for this scope.
            SelectObject(mem_dc, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(mem_dc);
            out
        })();

        ReleaseDC(None, screen_dc);
        result
    }
}

/// Extract the selected bitmap as a BGRA8, top-down buffer and set alpha to 255.
unsafe fn read_pixels(
    mem_dc: HDC,
    bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    w: i32,
    h: i32,
) -> Result<CapturedImage> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // negative => top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    let scanlines = GetDIBits(
        mem_dc,
        bitmap,
        0,
        h as u32,
        Some(buf.as_mut_ptr() as *mut _),
        &mut bmi,
        DIB_RGB_COLORS,
    );
    if scanlines == 0 {
        return Err(Error::Capture("GetDIBits returned 0 scanlines".into()));
    }

    // GDI leaves the 4th byte undefined for BI_RGB 32bpp; force opaque alpha.
    for px in buf.chunks_exact_mut(4) {
        px[3] = 0xFF;
    }

    Ok(CapturedImage::new(w as u32, h as u32, buf))
}
