# Codex task — REA_ShareX "pipeline" package

You are one of three agents building a Rust Windows screenshot utility. Work ONLY
on the three files assigned to you. The shared type contract lives in
`src/model.rs` — read it first; treat it as read-only.

## Hard rules (parallel-safety)
- Edit ONLY: `src/settings.rs`, `src/naming.rs`, `src/save.rs`.
- Do NOT edit `src/model.rs`, `src/main.rs`, `Cargo.toml`, or any other file.
- Do NOT add dependencies. Everything you need is already in `Cargo.toml`
  (`serde`, `serde_json`, `thiserror`, `image`, `chrono`, `dirs`). If you think you
  need another crate, STOP and leave a `// NEEDS-DEP: <crate>` comment instead.
- Keep the public function signatures below EXACTLY as written — other modules
  call them.
- `cargo build` and `cargo test` must pass when you finish.

## Context
- All image buffers are `model::CapturedImage`: **BGRA8, top-down**, length
  `width*height*4`. The `image` crate wants RGBA — you must swap B/R channels.
- Errors use `model::Error` / `model::Result<T>`.

## File 1 — `src/settings.rs`
Persist and load `model::Settings` as pretty JSON.

```rust
use crate::model::{Result, Settings};
use std::path::PathBuf;

/// Returns the settings file path: %APPDATA%\REA_ShareX\settings.json
pub fn settings_path() -> PathBuf;

/// Load settings from disk. If the file does not exist, return
/// `Settings::default()` (do NOT error). If it exists but is corrupt, return
/// `Err(Error::Config(..))`.
pub fn load() -> Result<Settings>;

/// Write settings to disk as pretty JSON, creating parent dirs as needed.
pub fn save(settings: &Settings) -> Result<()>;
```
Add `#[cfg(test)]` tests: round-trip save→load into a temp dir; missing-file
returns default.

## File 2 — `src/naming.rs`
Render a filename from a template + `model::NameContext`. You supply the current
local time with `chrono::Local::now()`.

```rust
use crate::model::NameContext;

/// Expand tokens in `template` and append `.<ext>` from `ctx.ext`.
/// The returned string is a FILE NAME only (no directory).
/// Supported tokens (case-sensitive):
///   %Y %m %d  -> year(4), month(2), day(2)
///   %H %M %S  -> hour(2), minute(2), second(2)
///   %n        -> ctx.counter, zero-padded to 3 (e.g. 007)
///   %name     -> ctx.preset_name (sanitized)
///   %w %h     -> ctx.width, ctx.height
///   %title    -> ctx.window_title or "" if None (sanitized)
/// Any unknown %x is left literally. The final name is sanitized so it is a
/// valid Windows filename (strip / \ : * ? " < > | and control chars).
pub fn render_filename(template: &str, ctx: &NameContext) -> String;
```
Tests: fixed context + template → expected string; sanitization of illegal chars;
unknown token passthrough.

## File 3 — `src/save.rs`
Encode a `CapturedImage` and write it to disk.

```rust
use crate::model::{CapturedImage, ImageFormat, Result};
use std::path::{Path, PathBuf};

/// Encode `img` to PNG or JPG bytes. `quality` is 1..=100 (JPG quality; for PNG
/// map to a compression level — lossless, so quality only affects size/speed).
/// Remember: input is BGRA, `image` needs RGBA — swap channels.
pub fn encode(img: &CapturedImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>>;

/// Encode `img` and write it to `dir/filename`. Creates `dir` if missing.
/// Returns the full path written.
pub fn save_to(
    img: &CapturedImage,
    dir: &Path,
    filename: &str,
    format: ImageFormat,
    quality: u8,
) -> Result<PathBuf>;
```
Tests: encode a tiny 2x2 image to PNG and JPG, assert non-empty and that the PNG
decodes back to the right dimensions with channels correctly swapped.

## Definition of done
- The three files compile, all your tests pass (`cargo test`).
- No `todo!()`/`unimplemented!()` left in your files.
- No edits outside your three files.
