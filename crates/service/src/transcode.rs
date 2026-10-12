//! Storage optimizer transcoding execution.
//!
//! Takes an [`OptimizationRow`], prepares inputs and metadata, invokes FFmpeg
//! via [`audiovault_media::transcode::run_transcode`], and indexes the resulting
//! transcoded file into the catalog. The original source files are never deleted
//! unless explicit replacement was requested and confirmed by the user.

use std::path::{Path, PathBuf};

use audiovault_core::paths::{safe_join, validate_relative};
use audiovault_db::catalog;
use audiovault_db::jobs::JobRecord;
use audiovault_db::optimize;
use audiovault_indexer::{CancelOf, JobContext};
use audiovault_media::transcode::{TranscodeInput, TranscodeMetadata, TranscodeRequest, run_transcode};
use serde_json::Value;

use crate::events::AppEvent;
use crate::exec::Executor;
use crate::monitor::volume_mount;
use crate::{Result, ServiceError};

pub fn execute(exec: &Executor, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
    let opt_id = job
        .payload
        .get("optimization_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| ServiceError::Invalid("transcode job lacks optimization_id".into()))?;

    let replace_source = job.payload.get("replace_source").and_then(Value::as_bool).unwrap_or(false);
    let custom_dest = job.payload.get("dest_rel_path").and_then(Value::as_str).map(str::to_string);

    let (opt, copy, media_rows) = {
        let conn = exec.db.conn()?;
        let opt = optimize::get(&conn, opt_id)?.ok_or_else(|| ServiceError::NotFound(format!("optimization {opt_id}")))?;
        let copy = catalog::get_copy(&conn, opt.copy_id)?.ok_or_else(|| ServiceError::NotFound(format!("copy {}", opt.copy_id)))?;
        let media = catalog::media_for_copy(&conn, copy.id)?;
        (opt, copy, media)
    };

    if media_rows.is_empty() {
        return Err(ServiceError::Invalid("copy has no audio files".into()));
    }

    let (vol, mount) = volume_mount(&exec.db, copy.volume_id)?;

    let ffmpeg = exec.ffmpeg().ok_or_else(|| {
        ServiceError::Invalid(
            "FFmpeg executable not found. Please install FFmpeg or set its path in Settings.".into(),
        )
    })?;

    // Determine output destination path relative to storage volume
    let dest_rel = if let Some(d) = custom_dest {
        validate_relative(&d).map_err(|e| ServiceError::Invalid(e.to_string()))?;
        d
    } else {
        default_dest_rel(&copy.rel_path, opt.target.extension(), copy.is_single_file)
    };

    let dest_abs = safe_join(&mount, &dest_rel).map_err(|e| ServiceError::Invalid(e.to_string()))?;
    if dest_abs.exists() {
        return Err(ServiceError::Invalid(format!("destination already exists: {}", dest_abs.display())));
    }

    // Build transcode inputs
    let mut inputs = Vec::new();
    for m in &media_rows {
        let abs_path = safe_join(&mount, &m.rel_path).map_err(|e| ServiceError::Invalid(e.to_string()))?;
        if !abs_path.is_file() {
            return Err(ServiceError::NotFound(format!("source file missing: {}", abs_path.display())));
        }
        let chapter_title = m.tag_title.clone().or_else(|| {
            abs_path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
        });
        inputs.push(TranscodeInput {
            path: abs_path,
            duration_ms: m.duration_ms.unwrap_or(0),
            chapter_title,
        });
    }

    let book_core = {
        let conn = exec.db.conn()?;
        catalog::book_core(&conn, copy.book_id)?
    };

    let authors_display = {
        let conn = exec.db.conn()?;
        let authors = catalog::book_author_names(&conn, copy.book_id)?;
        if authors.is_empty() { None } else { Some(authors.join(", ")) }
    };

    let metadata = TranscodeMetadata {
        title: book_core.as_ref().map(|b| b.title.clone()),
        author: authors_display,
        narrator: None,
        album: book_core.as_ref().and_then(|b| b.series_name.clone()),
        year: None,
        genre: Some("Audiobook".into()),
        comment: None,
    };

    let cover_path = find_cover_art(&mount, &copy.rel_path, &media_rows);

    let req = TranscodeRequest {
        inputs,
        output: dest_abs.clone(),
        target: opt.target,
        bitrate_kbps: opt.target_bitrate_kbps.map(|b| b as u32),
        reencode: opt.reencode,
        mono: true,
        metadata,
        cover: cover_path,
    };

    ctx.progress(0.0, &format!("Transcoding \"{}\" to {}", opt.title, opt.target.label()));

    let cancel = CancelOf(ctx);
    let mut last_reported = 0;
    let outcome = run_transcode(&ffmpeg, &req, &exec.temp_dir, &cancel, &mut |frac| {
        let pct = (frac * 100.0) as u32;
        if pct != last_reported {
            last_reported = pct;
            ctx.progress(frac as f64, &format!("Transcoding \"{}\" ({pct}%)", opt.title));
        }
    })
    .map_err(|e| {
        if ctx.is_cancelled() {
            ServiceError::Cancelled
        } else {
            ServiceError::Media(e)
        }
    })?;

    // Mark optimization candidate completed
    {
        let conn = exec.db.conn()?;
        optimize::set_state(&conn, opt.id, "done")?;
    }

    // Index the newly generated file
    exec.enqueue_subtree_scan(vol.id, &dest_rel, &format!("Catalog transcoded {}", opt.title))?;

    // If source replacement was explicitly requested and confirmed, trash the original copy
    if replace_source {
        let (op_id, fileop_job) = crate::fileops::plan_delete(&exec.db, copy.id, false, true)?;
        let _ = op_id;
        exec.enqueue(&fileop_job)?;
    }

    exec.events.emit(AppEvent::LibraryChanged);
    exec.events.info(format!("Transcoded \"{}\" ({:.1} MB saved)", opt.title, (opt.savings() as f64) / 1024.0 / 1024.0));
    tracing::info!(output = %outcome.output.display(), size = outcome.size, "transcode complete");

    Ok(())
}

fn default_dest_rel(source_rel: &str, ext: &str, is_single_file: bool) -> String {
    let clean = source_rel.trim_matches(['/', '\\']).replace('\\', "/");
    if is_single_file {
        if let Some((stem, _)) = clean.rsplit_once('.') {
            format!("{stem}.{ext}")
        } else {
            format!("{clean}.{ext}")
        }
    } else {
        // Multi-file folder: create a single file next to or inside the folder
        // If folder is "Author/Title", place "Author/Title.m4b"
        format!("{clean}.{ext}")
    }
}

fn find_cover_art(mount: &Path, rel_path: &str, media: &[audiovault_db::catalog::MediaRow]) -> Option<PathBuf> {
    // 1. Standalone image in the book directory
    let book_dir = safe_join(mount, rel_path).ok()?;
    let dir = if book_dir.is_dir() { book_dir } else { book_dir.parent()?.to_path_buf() };

    for name in ["cover.jpg", "cover.png", "folder.jpg", "front.jpg", "Cover.jpg"] {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Embedded cover extracted from one of the media files
    for m in media {
        if let Ok(p) = safe_join(mount, &m.rel_path)
            && let Some(bytes) = audiovault_media::cover::embedded_cover(&p)
        {
            if let Ok(mut tmp) = tempfile::Builder::new().suffix(".jpg").tempfile() {
                use std::io::Write;
                if tmp.write_all(&bytes).is_ok() {
                    return Some(tmp.into_temp_path().keep().ok()?);
                }
            }
        }
    }
    None
}
