# Antigravity task — REA_ShareX "UI + input" package

You are one of three agents building a Rust Windows screenshot utility. Work ONLY
on the four files assigned to you. The shared type contract lives in
`src/model.rs` — read it first; treat it as read-only.

## Hard rules (parallel-safety)
- Edit ONLY: `src/overlay.rs`, `src/config_ui.rs`, `src/tray.rs`, `src/hotkey.rs`.
- Do NOT edit `src/model.rs`, `src/main.rs`, `src/capture.rs`, `src/save.rs`,
  `src/settings.rs`, `src/naming.rs`, `src/toast.rs`, or `Cargo.toml`.
- Do NOT add dependencies. Already present: `eframe`/`egui`, `global-hotkey`,
  `tray-icon`, `rfd`, `image`, `serde`. If you need another, STOP and leave a
  `// NEEDS-DEP: <crate>` comment.
- Keep the public signatures below EXACTLY. The integrator (Claude) owns
  `main.rs` and the top-level event loop; your modules are driven from there, so
  make them poll-based / return cleanly as specified — do NOT spawn your own
  global event loop except inside `run_settings` and `select_region`.
- `cargo build` must pass. UI need not be unit-tested, but must compile and run.

## Available from other modules (already stubbed; call them)
- `crate::capture::capture_fullscreen() -> Result<CapturedImage>` — whole virtual
  desktop, BGRA top-down.
- `crate::capture::virtual_screen_bounds() -> Result<Rect>` — the virtual desktop
  origin+size, so you can map overlay pixel selection → virtual coords.

## File 1 — `src/overlay.rs`
Fullscreen "freeze and drag-select" used ONLY when configuring a preset region.

```rust
use crate::model::{CapturedImage, Rect, Result};

/// Show a borderless, topmost, fullscreen window displaying `background` (a
/// snapshot of the whole virtual desktop). The user drags a rectangle.
/// `bounds` is the virtual-desktop rect that `background` covers, used to convert
/// the selection into virtual-desktop physical coordinates.
/// Returns Some(rect) on confirm, None if the user presses Esc / cancels.
pub fn select_region(background: &CapturedImage, bounds: Rect) -> Result<Option<Rect>>;
```
Draw the frozen image, dim it, draw the live selection rectangle, show the
selection size in px. Esc cancels, Enter / mouse-up confirms.

## File 2 — `src/config_ui.rs`
The egui Settings window.

```rust
use crate::model::{Result, Settings};

/// Open the settings window (blocking). Let the user edit everything in
/// `Settings`, then return the edited copy when the window closes. The caller
/// persists it via `settings::save`.
pub fn run_settings(initial: Settings) -> Result<Settings>;
```
Must let the user:
- Add / remove / rename presets; edit each preset's hotkey (text field) and
  format (Png/Jpg) + quality slider (1–100) + optional output dir (via `rfd`
  folder picker) + optional filename template.
- Set the region of a preset with a **"Set region"** button that calls
  `capture::capture_fullscreen()` + `capture::virtual_screen_bounds()` and then
  `overlay::select_region(...)`, storing the returned `Rect`.
- Edit global defaults (output dir, filename template, format, quality).
- Edit the full `ToastConfig` (corner dropdown, size, margin, duration, fade,
  show-thumbnail toggle, and the 3 click-action dropdowns).

## File 3 — `src/tray.rs`
System tray icon (uses `tray-icon`).

```rust
use crate::model::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent { OpenSettings, Quit }

pub struct Tray { /* keep the icon + menu alive here */ }

impl Tray {
    /// Create the tray icon with a menu: "Settings" and "Quit".
    pub fn new() -> Result<Self>;
    /// Non-blocking: return a pending tray menu event, if any.
    pub fn poll(&self) -> Option<TrayEvent>;
}
```

## File 4 — `src/hotkey.rs`
Global hotkeys for presets (uses `global-hotkey`).

```rust
use crate::model::{Preset, Result};

pub struct HotkeyManager { /* manager + id->preset_id map */ }

impl HotkeyManager {
    pub fn new() -> Result<Self>;
    /// Unregister all, then register each preset's `hotkey` string
    /// (skip empty strings). Parse errors -> Error::Hotkey.
    pub fn register_presets(&mut self, presets: &[Preset]) -> Result<()>;
    /// Non-blocking: return the `id` of the preset whose hotkey just fired.
    pub fn poll(&self) -> Option<String>;
}
```

## Definition of done
- All four files compile; `run_settings` opens a usable window; tray shows with a
  working menu; hotkeys register without error.
- No `todo!()`/`unimplemented!()` left in your files.
- No edits outside your four files.
