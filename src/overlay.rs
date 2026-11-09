//! Fullscreen drag-select overlay (config only). OWNED BY: Antigravity.
//! See docs/ANTIGRAVITY_BRIEF.md. Implement the body; keep the signature.

use crate::model::{CapturedImage, Rect, Result};

/// Freeze `background`, let the user drag a rectangle, return it in
/// virtual-desktop physical coords (`bounds` maps the snapshot to those coords).
/// Returns `None` if the user cancels (Esc).
pub fn select_region(background: &CapturedImage, bounds: Rect) -> Result<Option<Rect>> {
    let _ = (background, bounds);
    todo!("fullscreen topmost window, drag-select, map to virtual coords")
}
