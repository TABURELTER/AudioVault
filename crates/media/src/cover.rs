//! Cover art: choosing a folder image, extracting embedded pictures and
//! producing small cached JPEG thumbnails (the UI never decodes full covers
//! for list rows).

use std::path::{Path, PathBuf};

use image::ImageReader;
use image::imageops::FilterType;

use crate::{MediaError, Result};

pub const THUMB_SIZE: u32 = 256;
/// Refuse to decode giant images (decompression bombs).
const MAX_IMAGE_BYTES: u64 = 40 * 1024 * 1024;
const MAX_DIMENSION: u32 = 12_000;

/// Rank image file names; lower is better. `None` = not a cover candidate.
pub fn cover_rank(file_name: &str) -> Option<u32> {
    let lower = file_name.to_lowercase();
    let stem = lower.rsplit_once('.').map(|(s, _)| s).unwrap_or(&lower);
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if !matches!(ext, "jpg" | "jpeg" | "png" | "webp" | "bmp" | "gif") {
        return None;
    }
    let rank = match stem {
        "cover" | "folder" | "front" | "обложка" => 0,
        s if s.starts_with("cover") || s.starts_with("front") || s.contains("обложк") => 1,
        s if s.contains("cover") || s.contains("folder") => 2,
        s if s.contains("back") || s.contains("cd") || s.contains("disc") => 9,
        _ => 5,
    };
    Some(rank)
}

/// Best cover image among file names of a media set.
pub fn pick_folder_cover<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    names
        .into_iter()
        .filter_map(|n| cover_rank(n).map(|r| (r, n)))
        .min_by_key(|(r, n)| (*r, n.len()))
        .map(|(_, n)| n)
}

pub struct Thumbnail {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub content_hash: String,
}

/// Decode `bytes`, downscale and store a JPEG thumbnail named by content hash
/// in `cache_dir` (idempotent).
pub fn thumbnail_from_bytes(bytes: &[u8], cache_dir: &Path) -> Result<Thumbnail> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(MediaError::Unsupported("image too large".into()));
    }
    let hash = blake3::hash(bytes).to_hex()[..24].to_string();
    let sub = cache_dir.join(&hash[..2]);
    let out = sub.join(format!("{hash}.jpg"));
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| MediaError::Unsupported(e.to_string()))?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| MediaError::Unsupported(e.to_string()))?;
    if w > MAX_DIMENSION || h > MAX_DIMENSION || w == 0 || h == 0 {
        return Err(MediaError::Unsupported(format!(
            "unreasonable image size {w}x{h}"
        )));
    }
    if !out.exists() {
        let img = ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| MediaError::Unsupported(e.to_string()))?
            .decode()
            .map_err(|e| MediaError::Unsupported(e.to_string()))?;
        let thumb = img
            .resize(THUMB_SIZE, THUMB_SIZE, FilterType::Triangle)
            .to_rgb8();
        std::fs::create_dir_all(&sub)?;
        let tmp = sub.join(format!("{hash}.tmp"));
        thumb
            .save_with_format(&tmp, image::ImageFormat::Jpeg)
            .map_err(|e| MediaError::Failed(e.to_string()))?;
        std::fs::rename(&tmp, &out)?;
    }
    Ok(Thumbnail {
        path: out,
        width: w,
        height: h,
        content_hash: hash,
    })
}

pub fn thumbnail_from_file(path: &Path, cache_dir: &Path) -> Result<Thumbnail> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_IMAGE_BYTES {
        return Err(MediaError::Unsupported("image too large".into()));
    }
    thumbnail_from_bytes(&std::fs::read(path)?, cache_dir)
}

/// First embedded picture of an audio file (front cover preferred).
pub fn embedded_cover(path: &Path) -> Option<Vec<u8>> {
    use lofty::config::ParseOptions;
    use lofty::file::TaggedFileExt;
    use lofty::picture::PictureType;
    use lofty::probe::Probe;
    let tagged = Probe::open(path)
        .ok()?
        .options(ParseOptions::new().read_cover_art(true))
        .guess_file_type()
        .ok()?
        .read()
        .ok()?;
    let mut best: Option<Vec<u8>> = None;
    for tag in tagged.tags() {
        if let Some(p) = tag.get_picture_type(PictureType::CoverFront) {
            return Some(p.data().to_vec());
        }
        if best.is_none() {
            best = tag.pictures().first().map(|p| p.data().to_vec());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_covers() {
        assert_eq!(
            pick_folder_cover(["back.jpg", "cover.jpg", "scan01.png"]),
            Some("cover.jpg")
        );
        assert_eq!(
            pick_folder_cover(["Обложка.jpg", "x.txt"]),
            Some("Обложка.jpg")
        );
        assert_eq!(pick_folder_cover(["a.mp3", "b.txt"]), None);
    }

    #[test]
    fn thumbnails_png() {
        let dir = tempfile::tempdir().unwrap();
        let img = image::RgbImage::from_pixel(600, 900, image::Rgb([200, 30, 30]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let t = thumbnail_from_bytes(&png, dir.path()).unwrap();
        assert_eq!((t.width, t.height), (600, 900));
        let thumb = image::open(&t.path).unwrap();
        assert!(thumb.width() <= THUMB_SIZE && thumb.height() <= THUMB_SIZE);
        assert!(thumbnail_from_bytes(b"not an image", dir.path()).is_err());
    }
}
