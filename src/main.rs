//! XerahS — fast pre-configured region screenshot utility.
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

use crate::model::{NameContext, Preset, Rect, Settings};
use crate::tray::TrayEvent;

fn main() {
    // Physical-pixel coordinates across monitors, correct on mixed-DPI setups.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    if std::env::args().any(|a| a == "--selftest-hotkey") {
        selftest_hotkey();
        return;
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
    log_line(&format!(
        "startup: {} preset(s), output_dir={}",
        settings.presets.len(),
        settings.default_output_dir.display()
    ));
    match hotkeys.register_presets(&settings.presets) {
        Ok(()) => log_line("hotkeys registered OK"),
        Err(e) => {
            eprintln!("failed to register hotkeys: {e}");
            log_line(&format!("hotkey register error: {e}"));
        }
    }

    let mut workers: Vec<std::thread::JoinHandle<()>> = Vec::new();

    loop {
        pump_messages();
        workers.retain(|h| !h.is_finished());

        if let Some(id) = hotkeys.poll() {
            log_line(&format!("hotkey fired: preset id={id}"));
            if let Some(preset) = settings.presets.iter().find(|p| p.id == id) {
                if let Some(handle) = capture_preset(preset, &settings, &counter) {
                    workers.push(handle);
                }
            } else {
                log_line(&format!("no preset matches fired id={id}"));
            }
        }

        match tray.poll() {
            Some(TrayEvent::OpenSettings) => match config_ui::run_settings(settings.clone()) {
                Ok(updated) => {
                    settings = updated;
                    if let Err(e) = settings::save(&settings) {
                        eprintln!("failed to save settings: {e}");
                    }
                    match hotkeys.register_presets(&settings.presets) {
                        Ok(()) => log_line(&format!(
                            "re-registered hotkeys ({} preset(s))",
                            settings.presets.len()
                        )),
                        Err(e) => {
                            eprintln!("failed to re-register hotkeys: {e}");
                            log_line(&format!("hotkey re-register error: {e}"));
                        }
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
            log_line(&format!("capture error: {e}"));
            return None;
        }
    };
    log_line(&format!(
        "captured {}x{} for preset '{}'",
        image.width, image.height, preset.name
    ));

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
    let path = dir.join(&filename);

    // Show the toast immediately from the in-memory image so it feels instant;
    // encode + write to disk happens in parallel on the worker thread.
    toast::show_toast(&image, &path, &toast_cfg);

    let handle =
        thread::spawn(move || match save::save_to(&image, &dir, &filename, format, quality) {
            Ok(path) => log_line(&format!("saved: {}", path.display())),
            Err(e) => {
                eprintln!("save failed: {e}");
                log_line(&format!("save error: {e}"));
            }
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

/// Headless end-to-end check of the hotkey pipeline: register a combo, simulate
/// the keypress with SendInput, and confirm `poll()` reports it. Run with
/// `xerahs.exe --selftest-hotkey` (debug build, so stdout is visible).
fn selftest_hotkey() {
    use std::time::Instant;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_CONTROL, VK_MENU,
    };

    unsafe fn send_key(vk: u16, up: bool) {
        let input = INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: 0,
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        KEYBD_EVENT_FLAGS(0)
                    },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }

    // Probe registration of several hotkey strings (fresh manager each, dropped
    // after so the window + registration are released).
    fn probe(spec: &str) {
        match hotkey::HotkeyManager::new() {
            Ok(mut hk) => {
                let p = Preset {
                    id: "probe".into(),
                    name: "probe".into(),
                    region: Rect::new(0, 0, 10, 10),
                    hotkey: spec.into(),
                    format: None,
                    quality: None,
                    output_dir: None,
                    filename_template: None,
                };
                match hk.register_presets(std::slice::from_ref(&p)) {
                    Ok(()) => println!("[reg] {:?} -> OK", spec),
                    Err(e) => println!("[reg] {:?} -> FAIL: {e}", spec),
                }
            }
            Err(e) => println!("[reg] manager fail: {e}"),
        }
    }
    for spec in ["\\", "'", "Backslash", "Quote", "Shift+\\", "Ctrl+Shift+'"] {
        probe(spec);
    }

    let mut hk = match hotkey::HotkeyManager::new() {
        Ok(h) => h,
        Err(e) => {
            println!("[selftest] HotkeyManager::new FAILED: {e}");
            return;
        }
    };
    let preset = Preset {
        id: "selftest".into(),
        name: "selftest".into(),
        region: Rect::new(0, 0, 10, 10),
        hotkey: "Ctrl+Alt+KeyK".into(),
        format: None,
        quality: None,
        output_dir: None,
        filename_template: None,
    };
    match hk.register_presets(std::slice::from_ref(&preset)) {
        Ok(()) => println!("[selftest] register OK (Ctrl+Alt+KeyK)"),
        Err(e) => {
            println!("[selftest] register FAILED: {e}");
            return;
        }
    }

    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(800));
        unsafe {
            send_key(VK_CONTROL.0, false);
            send_key(VK_MENU.0, false);
            send_key(0x4B, false); // K
            send_key(0x4B, true);
            send_key(VK_MENU.0, true);
            send_key(VK_CONTROL.0, true);
        }
        println!("[selftest] simulated Ctrl+Alt+K");
    });

    let start = Instant::now();
    let mut fired = false;
    while start.elapsed() < Duration::from_secs(3) {
        pump_messages();
        if let Some(id) = hk.poll() {
            println!("[selftest] HOTKEY FIRED: preset id={id}");
            fired = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    println!(
        "[selftest] RESULT: {}",
        if fired {
            "PASS — register -> WM_HOTKEY -> poll pipeline works"
        } else {
            "FAIL — no event received"
        }
    );

    // (A) Does a BARE key (no modifier) actually fire? Register "\\" and press VK_OEM_5.
    {
        let mut hk2 = match hotkey::HotkeyManager::new() {
            Ok(h) => h,
            Err(e) => {
                println!("[bare] manager fail: {e}");
                return;
            }
        };
        let p = Preset {
            id: "bare".into(),
            name: "bare".into(),
            region: Rect::new(0, 0, 10, 10),
            hotkey: "\\".into(),
            format: None,
            quality: None,
            output_dir: None,
            filename_template: None,
        };
        let _ = hk2.register_presets(std::slice::from_ref(&p));
        std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(600));
            unsafe {
                send_key(0xDC, false); // VK_OEM_5 == backslash on US layout
                send_key(0xDC, true);
            }
        });
        let start = Instant::now();
        let mut bare_fired = false;
        while start.elapsed() < Duration::from_secs(2) {
            pump_messages();
            if hk2.poll().is_some() {
                bare_fired = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        println!(
            "[bare] '\\' no-modifier fire: {}",
            if bare_fired { "PASS" } else { "FAIL (bare key did not fire)" }
        );
    }

    // (B) Does capture + save actually produce a file, and where?
    match capture::capture_rect(Rect::new(0, 0, 120, 80)) {
        Ok(img) => {
            println!("[cap] captured {}x{} ({} bytes)", img.width, img.height, img.bgra.len());
            let dir = std::env::temp_dir();
            match save::save_to(&img, &dir, "rea_selftest.png", crate::model::ImageFormat::Png, 90) {
                Ok(path) => println!("[cap] saved OK: {}", path.display()),
                Err(e) => println!("[cap] save FAIL: {e}"),
            }
        }
        Err(e) => println!("[cap] capture FAIL: {e}"),
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

/// Diagnostic log at `%TEMP%\xerahs.log`. Best-effort; never fails the app.
fn log_line(msg: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("xerahs.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let _ = writeln!(f, "{ts}  {msg}");
    }
}
