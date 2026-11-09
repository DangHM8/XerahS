//! System tray icon + menu. OWNED BY: Antigravity.
//! See docs/ANTIGRAVITY_BRIEF.md. Implement the bodies; keep the API.

use crate::model::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    OpenSettings,
    Quit,
}

pub struct Tray {
    // Keep the tray icon + menu handles alive here.
}

impl Tray {
    /// Create the tray icon with a "Settings" and "Quit" menu.
    pub fn new() -> Result<Self> {
        todo!("build tray-icon with menu")
    }

    /// Non-blocking: return a pending tray menu event, if any.
    pub fn poll(&self) -> Option<TrayEvent> {
        None
    }
}
