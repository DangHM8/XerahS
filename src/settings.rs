//! Settings persistence (JSON). OWNED BY: Codex.
//! See docs/CODEX_BRIEF.md. Implement the bodies; keep the signatures.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::model::{Error, Result, Settings};

/// Returns the settings file path: `%APPDATA%\REA_ShareX\settings.json`.
pub fn settings_path() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("REA_ShareX")
        .join("settings.json")
}

/// Load settings; return `Settings::default()` if the file is missing.
pub fn load() -> Result<Settings> {
    load_from(&settings_path())
}

/// Save settings as pretty JSON, creating parent dirs.
pub fn save(settings: &Settings) -> Result<()> {
    save_at(&settings_path(), settings)
}

fn load_from(path: &Path) -> Result<Settings> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Settings::default());
        }
        Err(error) => return Err(error.into()),
    };
    serde_json::from_slice(&bytes)
        .map_err(|error| Error::Config(format!("{}: {error}", path.display())))
}

fn save_at(path: &Path, settings: &Settings) -> Result<()> {
    let bytes =
        serde_json::to_vec_pretty(settings).map_err(|error| Error::Config(error.to_string()))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "rea-sharex-settings-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("cannot create test directory: {error}"),
                }
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn settings_round_trip() {
        let dir = TempDir::new();
        let path = dir.0.join("nested/settings.json");
        let settings = Settings {
            default_quality: 73,
            default_filename_template: "Ảnh_%n".into(),
            ..Settings::default()
        };
        save_at(&path, &settings).unwrap();
        assert_eq!(
            serde_json::to_value(load_from(&path).unwrap()).unwrap(),
            serde_json::to_value(&settings).unwrap()
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            serde_json::to_string_pretty(&settings).unwrap()
        );
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = TempDir::new();
        assert_eq!(
            serde_json::to_value(load_from(&dir.0.join("missing.json")).unwrap()).unwrap(),
            serde_json::to_value(Settings::default()).unwrap()
        );
    }

    #[test]
    fn corrupt_file_returns_config_error() {
        let dir = TempDir::new();
        let path = dir.0.join("settings.json");
        for bytes in [b"not json".as_slice(), b"{}", &[0xff]] {
            fs::write(&path, bytes).unwrap();
            assert!(matches!(load_from(&path), Err(Error::Config(_))));
        }
    }
}
