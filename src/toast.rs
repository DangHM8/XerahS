//! ShareX-style custom-drawn toast notification. OWNED BY: integrator (Claude).
//!
//! A topmost, borderless, layered window shown in a configurable screen corner,
//! with a thumbnail of the capture + text, fade in/out, and configurable
//! left/middle/right click actions.

use std::path::Path;

use crate::model::{CapturedImage, ToastConfig};

/// Show a toast for a just-saved capture. Non-fatal: failures are swallowed so a
/// toast problem never blocks the capture pipeline.
pub fn show_toast(image: &CapturedImage, saved_path: &Path, config: &ToastConfig) {
    let _ = (image, saved_path, config);
    // TODO: layered window + thumbnail + fade + click actions.
}
