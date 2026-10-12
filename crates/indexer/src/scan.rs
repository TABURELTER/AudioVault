//! Incremental scan of a library root into the catalog.
//!
//! Invariants:
//! * Only a scan that walked the whole root to the end may mark catalog
//!   entries missing. Cancelled scans and scans interrupted by the storage
//!   disappearing are recorded as `interrupted` and never mark anything.
//! * Subtrees that could not be read are stamped as seen, so permission
//!   errors never turn into "missing".
//! * Unchanged files (same size + mtime) keep their probe results and hashes.
//! * A set whose audio file names and sizes match a missing copy (or a copy
//!   whose old folder no longer exists) is treated as a *move*: the existing
//!   copy is re-pointed instead of creating a new book.

use std::path::Path;

use audiovault_core::identity::book_identity_key;
use audiovault_core::infer::{Inferred, infer_from_path};
use audiovault_core::metadata::Observation;
use audiovault_core::model::{FileKind, MetadataField, MetadataSource};
use audiovault_db::catalog::{self, CopyEnumeration, MediaEnumeration};
use audiovault_db::scans::{self, ScanCounters};
use audiovault_db::volumes::{self, LibraryRoot};
use audiovault_db::{Db, rusqlite};
use rusqlite::params;

use crate::walk::{MediaSet, walk};
use crate::{IndexerError, JobContext, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcome {
    Completed,
    /// Stopped early (cancelled or storage disappeared); nothing marked missing.
    Interrupted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScanSummary {
    pub scan_id: i64,
    pub outcome: ScanOutcome,
    pub counters: ScanCounters,
    pub message: Option<String>,
    /// Copies that are new, moved or whose files changed (need analysis).
    pub changed_copies: Vec<i64>,
}

/// Digest of the sorted (lower-cased name, size) list of a set's audio files.
pub fn quick_signature(set: &MediaSet) -> String {
    let mut items: Vec<(String, u64)> = set.audio().map(|f| (f.file_name.to_lowercase(), f.size)).collect();
    items.sort();
    if items.is_empty() {
        return String::new();
    }
    let mut h = blake3::Hasher::new();
    for (n, s) in &items {
        h.update(n.as_bytes());
        h.update(&[0]);
        h.update(&s.to_le_bytes());
    }
    h.finalize().to_hex()[..32].to_string()
}

fn enumeration(set: &MediaSet, volume_id: i64, root_id: Option<i64>, scan_id: i64, inferred: Option<&Inferred>) -> CopyEnumeration {
    let audio: Vec<_> = set.audio().collect();
    CopyEnumeration {
        volume_id,
        root_id,
        rel_path: set.rel_path.clone(),
        is_single_file: set.is_single_file,
        file_count: set.files.len() as u32,
        audio_file_count: audio.len() as u32,
        total_size: set.files.iter().map(|f| f.size).sum(),
        audio_size: audio.iter().map(|f| f.size).sum(),
        quick_signature: quick_signature(set),
        detection_confidence: inferred.map_or(0.0, |i| i.confidence),
        evidence_json: inferred.map_or_else(|| "[]".into(), |i| serde_json::to_string(&i.evidence).unwrap_or_else(|_| "[]".into())),
        scan_id: Some(scan_id),
    }
}

fn fmt_index(v: f32) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

/// Folder/file-name observations for a copy.
pub fn folder_observations(inf: &Inferred, single_file: bool) -> Vec<Observation> {
    let src = if single_file { MetadataSource::FileName } else { MetadataSource::FolderName };
    let c = inf.confidence;
    let mut obs = Vec::new();
    if let Some(t) = &inf.title {
        obs.push(Observation::new(MetadataField::Title, t.clone(), src, c));
    }
    if !inf.authors.is_empty() {
        obs.push(Observation::new(MetadataField::Author, inf.authors.join("; "), src, c));
    }
    if !inf.narrators.is_empty() {
        obs.push(Observation::new(MetadataField::Narrator, inf.narrators.join("; "), src, c));
    }
    if let Some(s) = &inf.series {
        obs.push(Observation::new(MetadataField::Series, s.clone(), src, c * 0.9));
    }
    if let Some(i) = inf.series_index {
        obs.push(Observation::new(MetadataField::SeriesIndex, fmt_index(i), src, c * 0.9));
    }
    if let Some(y) = inf.year {
        obs.push(Observation::new(MetadataField::Year, y.to_string(), src, c * 0.8));
    }
    obs
}

fn upsert_files(conn: &rusqlite::Connection, copy_id: i64, volume_id: i64, set: &MediaSet, scan_id: i64) -> Result<bool> {
    let mut any_changed = false;
    let mut keep: Vec<&str> = Vec::with_capacity(set.files.len());
    for (i, f) in set.files.iter().enumerate() {
        let m = MediaEnumeration {
            rel_path: f.rel_path.clone(),
            file_name: f.file_name.clone(),
            ext: f.ext.clone(),
            kind: f.kind.as_str().to_string(),
            size: f.size,
            mtime: f.mtime,
            sort_index: i as i64,
        };
        let (_, changed) = catalog::upsert_media(conn, copy_id, volume_id, &m, Some(scan_id))?;
        any_changed |= changed;
        keep.push(&f.rel_path);
    }
    // The set's folder was listed completely, so files of this copy that are
    // not in it were deleted or renamed: drop their rows.
    let removed = conn.execute(
        "DELETE FROM media_file WHERE copy_id = ?1 AND rel_path NOT IN (SELECT value FROM json_each(?2))",
        params![copy_id, serde_json::to_string(&keep).unwrap_or_else(|_| "[]".into())],
    )?;
    Ok(any_changed || removed > 0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestKind {
    Unchanged,
    Changed,
    Moved,
    New,
}

/// Ingest one discovered set inside a write transaction.
pub fn ingest_set(
    conn: &rusqlite::Connection,
    mount: &Path,
    volume_id: i64,
    root_id: Option<i64>,
    scan_id: i64,
    set: &MediaSet,
) -> Result<(i64, IngestKind)> {
    if let Some(copy) = catalog::find_copy(conn, volume_id, &set.rel_path)? {
        let e = enumeration(set, volume_id, root_id, scan_id, None);
        let sig_changed = copy.quick_signature.as_deref() != Some(e.quick_signature.as_str());
        let files_changed = upsert_files(conn, copy.id, volume_id, set, scan_id)? || sig_changed;
        let was_missing = copy.state != "present";
        catalog::update_copy_enumeration(conn, copy.id, &e, files_changed)?;
        if was_missing || files_changed {
            catalog::refresh_book(conn, copy.book_id)?;
        }
        return Ok((copy.id, if files_changed || was_missing { IngestKind::Changed } else { IngestKind::Unchanged }));
    }

    let e = enumeration(set, volume_id, root_id, scan_id, None);
    for cand in catalog::move_candidates(conn, &e.quick_signature, volume_id, scan_id)? {
        if cand.is_single_file != set.is_single_file {
            continue;
        }
        if cand.state == "present" && cand.volume_id == volume_id && crate::abs_path(mount, &cand.rel_path).exists() {
            continue; // genuinely a second copy, not a move
        }
        tracing::info!(from = %cand.rel_path, to = %set.rel_path, "detected moved copy");
        catalog::set_copy_path(conn, cand.id, volume_id, &set.rel_path)?;
        let changed = upsert_files(conn, cand.id, volume_id, set, scan_id)?;
        catalog::update_copy_enumeration(conn, cand.id, &e, changed)?;
        catalog::refresh_book(conn, cand.book_id)?;
        return Ok((cand.id, IngestKind::Moved));
    }

    let comps = set.components();
    let refs: Vec<&str> = comps.iter().map(String::as_str).collect();
    let inferred = infer_from_path(&refs);
    let title = inferred.title.clone().unwrap_or_else(|| comps.last().cloned().unwrap_or_else(|| "Untitled".into()));
    let book_id = match catalog::find_book_by_identity(conn, &book_identity_key(&inferred.authors, &title))? {
        Some(id) => id,
        None => catalog::create_book(conn, &title, &inferred.authors)?,
    };
    let variant_id = catalog::variant_for_new_copy(conn, book_id, &inferred.narrators)?;
    let e = enumeration(set, volume_id, root_id, scan_id, Some(&inferred));
    let copy_id = catalog::insert_copy(conn, variant_id, book_id, &e)?;
    upsert_files(conn, copy_id, volume_id, set, scan_id)?;
    let src = if set.is_single_file { MetadataSource::FileName } else { MetadataSource::FolderName };
    catalog::replace_copy_observations(conn, book_id, copy_id, &[src], &folder_observations(&inferred, set.is_single_file))?;
    catalog::add_book_alias(conn, book_id, comps.last().map_or("", String::as_str), "original", "folder")?;
    catalog::refresh_book(conn, book_id)?;
    Ok((copy_id, IngestKind::New))
}

/// Books that own copies under `prefix` not seen in `scan_id` (about to be marked missing).
fn books_not_seen(conn: &rusqlite::Connection, volume_id: i64, prefix: &str, scan_id: i64) -> Result<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT DISTINCT book_id FROM book_copy WHERE volume_id = ?1 AND state = 'present'
           AND (last_scan_id IS NULL OR last_scan_id <> ?4)
           AND (?2 = '' OR rel_path = ?2 OR substr(rel_path, 1, length(?3)) = ?3)",
    )?;
    let ids = st
        .query_map(params![volume_id, prefix, format!("{prefix}/"), scan_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<i64>>>()?;
    Ok(ids)
}

const BATCH: usize = 48;

/// Scan one library root. Returns `Ok` with [`ScanOutcome::Interrupted`] when
/// cancelled or when the storage went away; other failures are errors.
pub fn scan_root(db: &Db, volume_id: i64, mount: &Path, root: &LibraryRoot, ctx: &dyn JobContext) -> Result<ScanSummary> {
    let scan_id = db.write(|tx| scans::start(tx, volume_id, Some(root.id)))?;
    let mut counters = ScanCounters::default();
    let mut changed: Vec<i64> = Vec::new();
    let mut pending: Vec<MediaSet> = Vec::with_capacity(BATCH);

    let flush = |pending: &mut Vec<MediaSet>, counters: &mut ScanCounters, changed: &mut Vec<i64>| -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let results = db.write_with(|tx| {
            let mut out = Vec::with_capacity(pending.len());
            for set in pending.iter() {
                out.push(ingest_set(tx, mount, volume_id, Some(root.id), scan_id, set)?);
            }
            scans::update_counters(tx, scan_id, counters)?;
            Ok::<_, IndexerError>(out)
        });
        let results = results?;
        for (copy_id, kind) in results {
            counters.copies_found += 1;
            match kind {
                IngestKind::New => {
                    counters.copies_new += 1;
                    changed.push(copy_id);
                }
                IngestKind::Moved | IngestKind::Changed => changed.push(copy_id),
                IngestKind::Unchanged => {}
            }
        }
        pending.clear();
        Ok(())
    };

    let walked = walk(mount, &root.rel_path, ctx, &mut |set| {
        pending.push(set);
        if pending.len() >= BATCH {
            flush(&mut pending, &mut counters, &mut changed)?;
            ctx.progress(-1.0, &format!("{} books indexed", counters.copies_found));
        }
        Ok(())
    });

    match walked {
        Ok(stats) => {
            flush(&mut pending, &mut counters, &mut changed)?;
            counters.dirs_seen = stats.dirs_seen;
            counters.files_seen = stats.files_seen;
            counters.errors = stats.errors.len() as u64;
            // A last sanity check right before the destructive step: if the
            // root vanished at the very end, do not mark anything missing.
            if !crate::abs_path(mount, &root.rel_path).is_dir() {
                return finish_interrupted(db, scan_id, counters, changed, "storage disappeared at the end of the scan".into());
            }
            db.write_with(|tx| {
                for (rel, msg) in &stats.errors {
                    scans::add_error(tx, scan_id, rel, msg)?;
                    catalog::touch_subtree(tx, volume_id, rel, scan_id)?;
                }
                let affected = books_not_seen(tx, volume_id, &root.rel_path, scan_id)?;
                let (copies_missing, _files) = catalog::mark_missing_after_scan(tx, volume_id, &root.rel_path, scan_id)?;
                counters.copies_missing = copies_missing as u64;
                for b in affected {
                    catalog::refresh_book(tx, b)?;
                }
                volumes::mark_root_scanned(tx, root.id)?;
                volumes::mark_scanned(tx, volume_id)?;
                scans::finish(tx, scan_id, "completed", &counters, None)?;
                Ok::<_, IndexerError>(())
            })?;
            Ok(ScanSummary { scan_id, outcome: ScanOutcome::Completed, counters, message: None, changed_copies: changed })
        }
        Err(IndexerError::Cancelled) => {
            // Sets already discovered are valid; keep them.
            flush(&mut pending, &mut counters, &mut changed)?;
            finish_interrupted(db, scan_id, counters, changed, "cancelled".into())
        }
        Err(IndexerError::VolumeUnavailable(msg)) => {
            let _ = flush(&mut pending, &mut counters, &mut changed);
            finish_interrupted(db, scan_id, counters, changed, msg)
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = db.write(|tx| scans::finish(tx, scan_id, "failed", &counters, Some(&msg)));
            Err(e)
        }
    }
}

fn finish_interrupted(db: &Db, scan_id: i64, counters: ScanCounters, changed: Vec<i64>, msg: String) -> Result<ScanSummary> {
    tracing::warn!(scan_id, %msg, "scan interrupted; nothing marked missing");
    db.write(|tx| scans::finish(tx, scan_id, "interrupted", &counters, Some(&msg)))?;
    Ok(ScanSummary { scan_id, outcome: ScanOutcome::Interrupted, counters, message: Some(msg), changed_copies: changed })
}

/// True when a file kind is relevant for metadata (used by analysis).
pub fn is_metadata_file(kind: FileKind) -> bool {
    matches!(kind, FileKind::Audio | FileKind::Ebook | FileKind::Image)
}
