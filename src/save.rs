//! Encode + write captures. OWNED BY: Codex.
//! See docs/CODEX_BRIEF.md. Input is BGRA; the `image` crate needs RGBA — swap.

use std::path::{Path, PathBuf};

use crate::model::{CapturedImage, Error, ImageFormat, Result};
use image::{
    codecs::{
        jpeg::JpegEncoder,
        png::{CompressionType, FilterType, PngEncoder},
    },
    ExtendedColorType, ImageEncoder,
};

/// Encode `img` to PNG or JPG bytes. `quality` is 1..=100.
pub fn encode(img: &CapturedImage, format: ImageFormat, quality: u8) -> Result<Vec<u8>> {
    let expected = (img.width as usize)
        .checked_mul(img.height as usize)
        .and_then(|pixels| pixels.checked_mul(4));
    if img.width == 0 || img.height == 0 || expected != Some(img.bgra.len()) {
        return Err(Error::Encode(
            "invalid dimensions or BGRA buffer length".into(),
        ));
    }
    if !(1..=100).contains(&quality) {
        return Err(Error::Encode("quality must be in 1..=100".into()));
    }
    let mut bytes = Vec::new();
    match format {
        ImageFormat::Png => {
            let mut rgba = img.bgra.clone();
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            // Higher quality requests more compression effort; all levels are lossless.
            let compression = match quality {
                1..=33 => CompressionType::Fast,
                34..=66 => CompressionType::Default,
                _ => CompressionType::Best,
            };
            PngEncoder::new_with_quality(&mut bytes, compression, FilterType::Adaptive)
                .write_image(&rgba, img.width, img.height, ExtendedColorType::Rgba8)
                .map_err(|error| Error::Encode(error.to_string()))?;
        }
        ImageFormat::Jpg => {
            // JPEG has no alpha channel. Preserve RGB values and discard alpha.
            let mut rgb = Vec::with_capacity(img.bgra.len() / 4 * 3);
            for pixel in img.bgra.chunks_exact(4) {
                rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            }
            JpegEncoder::new_with_quality(&mut bytes, quality)
                .encode(&rgb, img.width, img.height, ExtendedColorType::Rgb8)
                .map_err(|error| Error::Encode(error.to_string()))?;
        }
    }
    Ok(bytes)
}

/// Encode `img` and write it to `dir/filename`. Returns the full path written.
pub fn save_to(
    img: &CapturedImage,
    dir: &Path,
    filename: &str,
    format: ImageFormat,
    quality: u8,
) -> Result<PathBuf> {
    let bytes = encode(img, format, quality)?;
    std::fs::create_dir_all(dir)?;
    let path = dir.join(filename);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_image() -> CapturedImage {
        CapturedImage::new(
            2,
            2,
            vec![0, 0, 255, 255, 0, 255, 0, 128, 255, 0, 0, 64, 30, 20, 10, 0],
        )
    }

    #[test]
    fn png_preserves_channels_alpha_and_row_order() {
        for quality in [1, 50, 100] {
            let bytes = encode(&tiny_image(), ImageFormat::Png, quality).unwrap();
            assert!(!bytes.is_empty());
            let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
            assert_eq!(decoded.dimensions(), (2, 2));
            assert_eq!(
                decoded.into_raw(),
                vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 10, 20, 30, 0,]
            );
        }
    }

    #[test]
    fn jpeg_encodes_and_decodes() {
        let bytes = encode(&tiny_image(), ImageFormat::Jpg, 90).unwrap();
        assert!(!bytes.is_empty());
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
        let red = CapturedImage::new(2, 2, [0, 0, 255, 255].repeat(4));
        let decoded = image::load_from_memory(&encode(&red, ImageFormat::Jpg, 100).unwrap())
            .unwrap()
            .to_rgb8();
        for pixel in decoded.pixels() {
            assert!(pixel[0] > 240 && pixel[1] < 15 && pixel[2] < 15);
        }
    }

    #[test]
    fn invalid_input_returns_encode_error() {
        let invalid = CapturedImage {
            width: 2,
            height: 2,
            bgra: vec![0; 3],
        };
        for format in [ImageFormat::Png, ImageFormat::Jpg] {
            assert!(matches!(
                encode(&invalid, format, 90),
                Err(Error::Encode(_))
            ));
            assert!(matches!(
                encode(&tiny_image(), format, 0),
                Err(Error::Encode(_))
            ));
            assert!(matches!(
                encode(&tiny_image(), format, 101),
                Err(Error::Encode(_))
            ));
        }
    }
}
