//! Global hotkeys for presets. OWNED BY: Antigravity.
//! See docs/ANTIGRAVITY_BRIEF.md. Implement the bodies; keep the API.

use crate::model::{Preset, Result};

pub struct HotkeyManager {
    // Keep the GlobalHotKeyManager + id->preset_id map here.
}

impl HotkeyManager {
    pub fn new() -> Result<Self> {
        todo!("create GlobalHotKeyManager")
    }

    /// Unregister all, then register each preset's hotkey (skip empty strings).
    pub fn register_presets(&mut self, presets: &[Preset]) -> Result<()> {
        let _ = presets;
        todo!("parse + register hotkeys, map accelerator id -> preset id")
    }

    /// Non-blocking: return the `id` of the preset whose hotkey just fired.
    pub fn poll(&self) -> Option<String> {
        None
    }
}
