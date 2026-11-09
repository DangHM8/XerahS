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
            .with_tooltip("REA_ShareX")
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

/// A simple 32x32 RGBA icon: a blue tile with a lighter "lens" square.
fn default_icon() -> Icon {
    const N: usize = 32;
    let mut rgba = vec![0u8; N * N * 4];
    for y in 0..N {
        for x in 0..N {
            let i = (y * N + x) * 4;
            let border = x < 2 || x >= N - 2 || y < 2 || y >= N - 2;
            let lens = (8..24).contains(&x) && (8..24).contains(&y);
            let (r, g, b) = if border {
                (20, 40, 70)
            } else if lens {
                (220, 230, 245)
            } else {
                (40, 110, 210)
            };
            rgba[i] = r;
            rgba[i + 1] = g;
            rgba[i + 2] = b;
            rgba[i + 3] = 255;
        }
    }
    Icon::from_rgba(rgba, N as u32, N as u32).expect("valid icon dimensions")
}
