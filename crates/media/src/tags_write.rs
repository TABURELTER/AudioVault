//! Explicit audio tag writing (never automatic; driven by user-approved
//! file operations). Writes go to a temporary copy which atomically replaces
//! the original only after a successful save and re-read.

use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::{Deserialize, Serialize};

use crate::{MediaError, Result};

/// Fields to write; `None` leaves the existing value untouched.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TagEdit {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub narrator: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub comment: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
}

impl TagEdit {
    pub fn is_empty(&self) -> bool {
        *self == TagEdit::default()
    }
}

fn apply(tag: &mut Tag, e: &TagEdit) {
    if let Some(v) = &e.title {
        tag.set_title(v.clone());
    }
    if let Some(v) = &e.artist {
        tag.set_artist(v.clone());
    }
    if let Some(v) = &e.album {
        tag.set_album(v.clone());
    }
    if let Some(v) = &e.genre {
        tag.set_genre(v.clone());
    }
    if let Some(v) = &e.comment {
        tag.set_comment(v.clone());
    }
    if let Some(v) = &e.album_artist {
        tag.insert_text(ItemKey::AlbumArtist, v.clone());
    }
    if let Some(v) = &e.narrator {
        // Composer is the most widely displayed field for the reader in audiobook players.
        tag.insert_text(ItemKey::Composer, v.clone());
    }
    if let Some(y) = e.year {
        tag.insert_text(ItemKey::Year, y.to_string());
    }
    if let Some(v) = &e.series {
        tag.insert_text(ItemKey::Movement, v.clone());
    }
    if let Some(v) = &e.series_index {
        tag.insert_text(ItemKey::MovementNumber, v.clone());
    }
}

/// Write tags into `path` safely: copy → modify copy → verify → atomic rename.
pub fn write_tags(path: &Path, edit: &TagEdit) -> Result<()> {
    if edit.is_empty() {
        return Ok(());
    }
    let file_name = path.file_name().ok_or_else(|| MediaError::Failed("invalid path".into()))?;
    let tmp = path.with_file_name(format!(".{}.avtmp", file_name.to_string_lossy()));
    std::fs::copy(path, &tmp)?;
    let result = (|| -> Result<()> {
        let mut tagged = Probe::open(&tmp)
            .map_err(|e| MediaError::Failed(e.to_string()))?
            .options(ParseOptions::new())
            .guess_file_type()?
            .read()
            .map_err(|e| MediaError::Failed(e.to_string()))?;
        if tagged.primary_tag().is_none() {
            let tt = tagged.primary_tag_type();
            tagged.insert_tag(Tag::new(tt));
        }
        let tag = tagged.primary_tag_mut().ok_or_else(|| MediaError::Failed("format does not support tags".into()))?;
        apply(tag, edit);
        tagged.save_to_path(&tmp, WriteOptions::default()).map_err(|e| MediaError::Failed(e.to_string()))?;
        // Verify the result is still a readable audio file.
        Probe::open(&tmp)
            .map_err(|e| MediaError::Failed(e.to_string()))?
            .guess_file_type()?
            .read()
            .map_err(|e| MediaError::Failed(format!("verification failed: {e}")))?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            std::fs::rename(&tmp, path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_wav_tags_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.wav");
        crate::probe::tests::write_wav(&p, 0.5, 300.0);
        write_tags(
            &p,
            &TagEdit { title: Some("Метро 2033".into()), artist: Some("Дмитрий Глуховский".into()), ..Default::default() },
        )
        .unwrap();
        let r = crate::probe::probe_file(&p, false).unwrap();
        assert_eq!(r.tags.title.as_deref(), Some("Метро 2033"));
        assert_eq!(r.tags.artist.as_deref(), Some("Дмитрий Глуховский"));
        assert!(std::fs::read_dir(dir.path()).unwrap().count() == 1, "no temp files left");
    }
}
