//! Global hotkeys for presets (global-hotkey). OWNED BY: integrator (Claude).

use std::collections::HashMap;
use std::str::FromStr;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::model::{Error, Preset, Result};

pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    registered: Vec<HotKey>,
    id_to_preset: HashMap<u32, String>,
}

impl HotkeyManager {
    pub fn new() -> Result<Self> {
        let manager = GlobalHotKeyManager::new().map_err(|e| Error::Hotkey(e.to_string()))?;
        Ok(Self {
            manager,
            registered: Vec::new(),
            id_to_preset: HashMap::new(),
        })
    }

    /// Unregister all, then register each preset's hotkey (skipping empty strings).
    /// Hotkey strings follow the global-hotkey format, e.g. `"Ctrl+Shift+KeyA"`,
    /// `"Alt+Digit1"`, `"ControlLeft+F2"`.
    pub fn register_presets(&mut self, presets: &[Preset]) -> Result<()> {
        if !self.registered.is_empty() {
            let _ = self.manager.unregister_all(&self.registered);
            self.registered.clear();
        }
        self.id_to_preset.clear();

        for preset in presets {
            let spec = preset.hotkey.trim();
            if spec.is_empty() {
                continue;
            }
            let hotkey = HotKey::from_str(spec)
                .map_err(|e| Error::Hotkey(format!("'{spec}': {e}")))?;
            self.manager
                .register(hotkey)
                .map_err(|e| Error::Hotkey(format!("'{spec}': {e}")))?;
            self.id_to_preset.insert(hotkey.id(), preset.id.clone());
            self.registered.push(hotkey);
        }
        Ok(())
    }

    /// Non-blocking: return the `id` of the preset whose hotkey just fired.
    pub fn poll(&self) -> Option<String> {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                if let Some(preset_id) = self.id_to_preset.get(&event.id) {
                    return Some(preset_id.clone());
                }
            }
        }
        None
    }
}
