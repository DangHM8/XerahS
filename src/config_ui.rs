//! egui Settings window. OWNED BY: Antigravity.
//! See docs/ANTIGRAVITY_BRIEF.md. Implement the body; keep the signature.

use crate::model::{Result, Settings};

/// Open the settings window (blocking) and return the edited settings on close.
pub fn run_settings(initial: Settings) -> Result<Settings> {
    let _ = initial;
    todo!("eframe window: presets CRUD, set-region, defaults, toast config")
}
