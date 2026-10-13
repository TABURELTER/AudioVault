//! Heavy per-copy content analysis: full BLAKE3 hashes and acoustic
//! fingerprints. Both are resumable per file / per copy and cancellable.

use std::path::Path;

use audiovault_db::Db;
use audiovault_db::catalog;
use audiovault_db::duplicates::{StoredFingerprint, save_fingerprints};
use audiovault_media::fingerprint::{AudioFingerprintService, TimelineFile};
use audiovault_media::hash::full_hash;

use crate::{CancelOf, IndexerError, JobContext, Result, check, media_err};

pub const LEVEL_FULL_HASHED: i64 = 3;
pub const LEVEL_FINGERPRINTED: i64 = 4;

/// Compute missing full hashes of a copy's audio files. Each file's hash is
/// committed immediately, so an interrupted job resumes where it stopped.
pub fn full_hash_copy(db: &Db, copy_id: i64, mount: &Path, ctx: &dyn JobContext) -> Result<u32> {
    let rows = {
        let conn = db.conn()?;
        catalog::media_for_copy(&conn, copy_id)?
    };
    let todo: Vec<_> = rows
        .iter()
        .filter(|m| m.kind == "audio" && m.state == "present" && m.full_hash.is_none())
        .collect();
    let total: u64 = todo.iter().map(|m| m.size).sum::<u64>().max(1);
    let mut done: u64 = 0;
    let mut hashed = 0u32;
    for m in todo {
        check(ctx)?;
        let path = crate::abs_path(mount, &m.rel_path);
        let base = done;
        let mut last_report = 0u64;
        let h = full_hash(&path, &CancelOf(ctx), &mut |bytes| {
            if bytes - last_report >= 16 * 1024 * 1024 {
                last_report = bytes;
                ctx.progress((base + bytes) as f64 / total as f64, &m.file_name);
            }
        })
        .map_err(|e| media_err(e, mount))?;
        db.write(|tx| catalog::update_media_hash(tx, m.id, None, Some(&h), m.size, m.mtime))?;
        done += m.size;
        hashed += 1;
    }
    db.write(|tx| catalog::set_copy_analysis_level(tx, copy_id, LEVEL_FULL_HASHED))?;
    Ok(hashed)
}

/// Fingerprint representative segments of a copy and store them.
pub fn fingerprint_copy(
    db: &Db,
    copy_id: i64,
    mount: &Path,
    service: &dyn AudioFingerprintService,
    ctx: &dyn JobContext,
) -> Result<usize> {
    let rows = {
        let conn = db.conn()?;
        catalog::media_for_copy(&conn, copy_id)?
    };
    let files: Vec<TimelineFile> = rows
        .iter()
        .filter(|m| m.kind == "audio" && m.state == "present")
        .map(|m| TimelineFile {
            path: crate::abs_path(mount, &m.rel_path),
            duration_ms: m.duration_ms.unwrap_or(0),
        })
        .collect();
    if files.is_empty() || files.iter().any(|f| f.duration_ms == 0) {
        return Err(IndexerError::Invalid(
            "copy has not been analyzed yet (durations unknown)".into(),
        ));
    }
    ctx.progress(-1.0, "fingerprinting");
    let segs = service
        .fingerprint_copy(&files, &CancelOf(ctx))
        .map_err(|e| media_err(e, mount))?;
    let stored: Vec<StoredFingerprint> = segs
        .iter()
        .map(|s| StoredFingerprint {
            segment: s.segment as i64,
            position_ms: s.position_ms as i64,
            length_ms: s.length_ms as i64,
            algorithm: service.algorithm().to_string(),
            data: s.data.clone(),
        })
        .collect();
    db.write(|tx| {
        save_fingerprints(tx, copy_id, &stored)?;
        catalog::set_copy_analysis_level(tx, copy_id, LEVEL_FINGERPRINTED)
    })?;
    Ok(stored.len())
}
