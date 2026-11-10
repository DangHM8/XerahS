<p align="center">
  <img src="assets/logo.png" width="128" alt="XerahS logo">
</p>

<h1 align="center">XerahS</h1>

<p align="center">Fast, pre-configured region screenshot tool for Windows — written in Rust.</p>

---

## About

XerahS runs in the background from the system tray. You create **presets** — each one is a fixed screen region bound to a global hotkey. Press the hotkey and that region is captured instantly, saved with your filename template, and a ShareX-style toast appears in the corner of the screen.

Ideal when you repeatedly capture the same area (a data table, a game window, a chat panel, a dashboard…) and don't want to drag-select every time.

## Features

- **Region presets + global hotkeys** (e.g. `Ctrl+Shift+1`, `Alt+F2`) that work even when XerahS isn't in the foreground.
- **Visual region picker** with a drag-to-select screen overlay.
- **Multi-monitor & mixed DPI**: uses physical pixel coordinates, including monitors left of / above the primary one.
- **PNG or JPG** with adjustable quality; output folder, format and filename template can be overridden per preset.
- **Flexible filename templates** (see below).
- **Customizable toast**: corner, size, duration, fade, thumbnail, and left/right/middle-click actions — open file, open folder, copy path, delete file.
- **Settings window** (egui) opened from the tray menu, with hotkey validation.
- Lightweight single `.exe`, no installer required.

## Build

Requirements: Windows 10/11, [Rust](https://rustup.rs) (MSVC toolchain).

```bash
git clone https://github.com/DangHM8/XerahS.git
```

```bash
cd XerahS
```

```bash
cargo build --release
```

The executable is `target/release/xerahs.exe`. Run it and the XerahS icon appears in the system tray.

## Usage

1. Right-click the tray icon → **Settings**.
2. Add a preset, pick the capture region, set a hotkey.
3. Save, then press the hotkey to capture.
4. Click the toast to open the image / folder (depending on configuration).

To exit: right-click the tray icon → **Quit**.

### Filename template

Default: `XerahS_%Y-%m-%d_%H-%M-%S_%n`

| Token | Meaning |
|---|---|
| `%Y` `%m` `%d` | Year, month, day |
| `%H` `%M` `%S` | Hour, minute, second |
| `%n` | Incrementing counter (3 digits) |
| `%name` | Preset name |
| `%w` `%h` | Image width / height |
| `%title` | Active window title |

The extension (`.png` / `.jpg`) is appended automatically. Characters invalid in Windows filenames are stripped.

## Paths

| Item | Location |
|---|---|
| Settings | `%APPDATA%\XerahS\settings.json` |
| Captures (default) | `%USERPROFILE%\Pictures\XerahS` |
| Diagnostic log | `%TEMP%\xerahs.log` |

## Source layout

```
src/
  main.rs       main loop, hotkey handling and capture
  capture.rs    screen capture (GDI BitBlt)
  overlay.rs    region selection overlay
  hotkey.rs     global hotkey registration
  save.rs       PNG/JPG encoding & saving
  naming.rs     filename templates
  toast.rs      post-capture notification
  tray.rs       system tray icon & menu
  config_ui.rs  settings window (egui)
  settings.rs   settings.json load/save
  model.rs      shared data types
assets/         logo & icons
```

> ⚠️ The **DeleteFile** toast action permanently deletes the file without confirmation.
