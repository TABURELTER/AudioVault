//! Unified e-book metadata extraction and cover thumbnail caching.

use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::cover::thumbnail_from_bytes;
use crate::epub::parse_epub_file;
use crate::fb2::parse_fb2_file;
use crate::Result;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExtractedEbook {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub genres: Vec<String>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub cover_thumb: Option<String>,
}

pub fn extract_ebook(path: &Path, cache_dir: Option<&Path>) -> Result<ExtractedEbook> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let name_lower = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();

    if name_lower.ends_with(".fb2")
        || name_lower.ends_with(".fb2.zip")
        || (ext == "zip" && crate::fb2::is_fb2_zip(path))
    {
        if let Ok(info) = parse_fb2_file(path) {
            let mut thumb_hash = None;
            if let (Some(cache), Some(cov)) = (cache_dir, info.cover) {
                if let Ok(t) = thumbnail_from_bytes(&cov.data, cache) {
                    thumb_hash = Some(t.content_hash);
                }
            }
            return Ok(ExtractedEbook {
                title: info.title,
                authors: info.authors,
                series: info.sequences.first().map(|(s, _)| s.clone()),
                series_index: info.sequences.first().and_then(|(_, idx)| *idx),
                genres: info.genres,
                year: info.year,
                language: info.language,
                description: info.annotation,
                cover_thumb: thumb_hash,
            });
        }
    } else if ext == "epub" {
        if let Ok(info) = parse_epub_file(path) {
            let mut thumb_hash = None;
            if let (Some(cache), Some(data)) = (cache_dir, info.cover_data) {
                if let Ok(t) = thumbnail_from_bytes(&data, cache) {
                    thumb_hash = Some(t.content_hash);
                }
            }
            return Ok(ExtractedEbook {
                title: info.title,
                authors: info.authors,
                series: info.series,
                series_index: info.series_index,
                genres: info.genres,
                year: info.year,
                language: info.language,
                description: info.description,
                cover_thumb: thumb_hash,
            });
        }
    }

    Ok(ExtractedEbook::default())
}
