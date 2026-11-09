//! Screen capture via GDI BitBlt. OWNED BY: integrator (Claude).
//!
//! Backend choice: for on-demand single-shot capture of a small fixed region,
//! GDI `BitBlt` of just the target rectangle gives the lowest latency. DXGI
//! Desktop Duplication is intentionally NOT used (it is tuned for continuous
//! high-FPS capture and returns no frame on a static desktop).

use crate::model::{CapturedImage, Rect, Result};

/// Capture exactly `region` (virtual-desktop physical pixels) into a BGRA image.
pub fn capture_rect(region: Rect) -> Result<CapturedImage> {
    let _ = region;
    todo!("BitBlt the region into a BGRA top-down buffer")
}

/// Capture the entire virtual desktop (all monitors) into one BGRA image.
pub fn capture_fullscreen() -> Result<CapturedImage> {
    todo!("BitBlt the full virtual screen")
}

/// Bounds of the virtual desktop (origin + total size), in physical pixels.
pub fn virtual_screen_bounds() -> Result<Rect> {
    todo!("GetSystemMetrics SM_XVIRTUALSCREEN / CX/CYVIRTUALSCREEN")
}
