//! System tray icon + menu (tray-icon). OWNED BY: integrator (Claude).

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::model::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    OpenSettings,
    Quit,
}

pub struct Tray {
    _tray: TrayIcon,
    settings_id: MenuId,
    quit_id: MenuId,
}

impl Tray {
    /// Create the tray icon with a "Settings" and "Quit" menu.
    pub fn new() -> Result<Self> {
        let menu = Menu::new();
        let settings_item = MenuItem::new("Settings", true, None);
        let quit_item = MenuItem::new("Quit", true, None);
        menu.append(&settings_item)
            .map_err(|e| Error::Other(format!("tray menu: {e}")))?;
        menu.append(&quit_item)
            .map_err(|e| Error::Other(format!("tray menu: {e}")))?;

        let settings_id = settings_item.id().clone();
        let quit_id = quit_item.id().clone();

        let tray = TrayIconBuilder::new()
            .with_tooltip("XerahS")
            .with_menu(Box::new(menu))
            .with_icon(default_icon())
            .build()
            .map_err(|e| Error::Other(format!("tray icon: {e}")))?;

        Ok(Self {
            _tray: tray,
            settings_id,
            quit_id,
        })
    }

    /// Non-blocking: return a pending tray menu event, if any.
    pub fn poll(&self) -> Option<TrayEvent> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == self.settings_id {
                return Some(TrayEvent::OpenSettings);
            }
            if event.id == self.quit_id {
                return Some(TrayEvent::Quit);
            }
        }
        None
    }
}

/// XerahS logo (assets/tray32.png), decoded at startup.
fn default_icon() -> Icon {
    let img = image::load_from_memory(include_bytes!("../assets/tray32.png"))
        .expect("embedded tray icon")
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).expect("valid icon dimensions")
}
