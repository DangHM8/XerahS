//! REA_ShareX — fast pre-configured region screenshot utility.
//!
//! Background tray app. Each preset binds a global hotkey to a fixed screen
//! region; pressing it captures that region, saves it with the configured name,
//! and shows a ShareX-style toast. A tray menu opens the settings window.

#![allow(dead_code)] // some module fns are used only across worktrees / by the UI
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod config_ui;
mod hotkey;
mod model;
mod naming;
mod overlay;
mod save;
mod settings;
mod toast;
mod tray;

use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, PeekMessageW,
    TranslateMessage, MSG, PM_REMOVE,
};

use crate::model::{NameContext, Preset, Settings};
use crate::tray::TrayEvent;

fn main() {
    // Physical-pixel coordinates across monitors, correct on mixed-DPI setups.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let mut settings = match settings::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("settings load error: {e}; starting with defaults (existing file left intact)");
            Settings::default()
        }
    };
    let counter = AtomicU32::new(1);

    let tray = match tray::Tray::new() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to create tray icon: {e}");
            return;
        }
    };

    let mut hotkeys = match hotkey::HotkeyManager::new() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("failed to init hotkeys: {e}");
            return;
        }
    };
    if let Err(e) = hotkeys.register_presets(&settings.presets) {
        eprintln!("failed to register hotkeys: {e}");
    }

    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();

    loop {
        pump_messages();
        workers.retain(|h| !h.is_finished());

        if let Some(id) = hotkeys.poll() {
            if let Some(preset) = settings.presets.iter().find(|p| p.id == id) {
                if let Some(handle) = capture_preset(preset, &settings, &counter) {
                    workers.push(handle);
                }
            }
        }

        match tray.poll() {
            Some(TrayEvent::OpenSettings) => match config_ui::run_settings(settings.clone()) {
                Ok(updated) => {
                    settings = updated;
                    if let Err(e) = settings::save(&settings) {
                        eprintln!("failed to save settings: {e}");
                    }
                    if let Err(e) = hotkeys.register_presets(&settings.presets) {
                        eprintln!("failed to re-register hotkeys: {e}");
                    }
                }
                Err(e) => eprintln!("settings window error: {e}"),
            },
            Some(TrayEvent::Quit) => break,
            None => {}
        }

        thread::sleep(Duration::from_millis(10));
    }

    // Drain pending save workers so an in-flight capture is not lost on quit.
    for handle in workers {
        let _ = handle.join();
    }
}

/// Capture one preset, then encode + save + toast on a worker thread so the
/// hotkey loop stays responsive.
fn capture_preset(
    preset: &Preset,
    settings: &Settings,
    counter: &AtomicU32,
) -> Option<std::thread::JoinHandle<()>> {
    let image = match capture::capture_rect(preset.region) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("capture failed: {e}");
            return None;
        }
    };

    let format = preset.format.unwrap_or(settings.default_format);
    let quality = preset.quality.unwrap_or(settings.default_quality);
    let dir = preset
        .output_dir
        .clone()
        .unwrap_or_else(|| settings.default_output_dir.clone());
    let template = preset
        .filename_template
        .clone()
        .unwrap_or_else(|| settings.default_filename_template.clone());

    let ctx = NameContext {
        preset_name: preset.name.clone(),
        counter: counter.fetch_add(1, Ordering::Relaxed),
        window_title: foreground_window_title(),
        width: image.width,
        height: image.height,
        ext: format.ext().to_string(),
    };
    let filename = naming::render_filename(&template, &ctx);
    let toast_cfg = settings.toast.clone();

    let handle =
        thread::spawn(move || match save::save_to(&image, &dir, &filename, format, quality) {
            Ok(path) => toast::show_toast(&image, &path, &toast_cfg),
            Err(e) => eprintln!("save failed: {e}"),
        });
    Some(handle)
}

/// Drain the thread message queue so tray-icon and global-hotkey deliver events.
fn pump_messages() {
    unsafe {
        let mut msg = MSG::default();
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Title of the current foreground window, for the `%title` filename token.
fn foreground_window_title() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return None;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        if n <= 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..n as usize]))
    }
}
