//! Encode + write captures. OWNED BY: Codex.
//! See docs/CODEX_BRIEF.md. Input is BGRA; the `image` crate needs RGBA — swap.

use std::path::{Path, PathBuf};

use crate::model::{CapturedImage, ImageFormat, Result};

/// Encode `img` to PNG or JPG bytes. `quality` is 1..=100.
pub fn encode(img: &CapturedImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>> {
    let _ = (img, format, quality);
    todo!("BGRA->RGBA, encode png/jpg")
}

/// Encode `img` and write it to `dir/filename`. Returns the full path written.
pub fn save_to(
    img: &CapturedImage,
    dir: &Path,
    filename: &str,
    format: ImageFormat,
    quality: u8,
) -> Result<PathBuf> {
    let _ = (img, dir, filename, format, quality);
    todo!("mkdir -p, encode, write, return path")
}
