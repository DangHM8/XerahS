//! Settings persistence (JSON). OWNED BY: Codex.
//! See docs/CODEX_BRIEF.md. Implement the bodies; keep the signatures.

use std::path::PathBuf;

use crate::model::{Result, Settings};

/// Returns the settings file path: `%APPDATA%\REA_ShareX\settings.json`.
pub fn settings_path() -> PathBuf {
    todo!("build %APPDATA%/REA_ShareX/settings.json")
}

/// Load settings; return `Settings::default()` if the file is missing.
pub fn load() -> Result<Settings> {
    todo!("read + deserialize, default on missing")
}

/// Save settings as pretty JSON, creating parent dirs.
pub fn save(settings: &Settings) -> Result<()> {
    let _ = settings;
    todo!("serialize pretty + write")
}
