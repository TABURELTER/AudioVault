//! Job execution: maps a [`JobRecord`] to indexer / file / transcode / import work.
//!
//! Payload formats (JSON):
//! * `scan`            `{"root_id": i64, "sub_path"?: "rel/path"}` — volume set
//! * `metadata`        `{}` — analyze every unanalyzed copy on the job's volume
//! * `duplicates`      `{"copy_ids"?: [i64], "no_fp_followup"?: bool}` — catalog only
//! * `fingerprint`     `{"copy_ids": [i64]}` — volume set
//! * `full_hash`       `{"copy_ids": [i64]}` — volume set
//! * `file_operation`  `{"op_id": i64}`
//! * `transcode`       `{"optimization_id": i64, ...}`
//! * `torrent_import`  `{"torrent_id": i64}`
//! * `reconcile`       `{}` — verify interrupted file operations

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use audiovault_core::model::JobKind;
use audiovault_core::paths::validate_relative;
use audiovault_db::jobs::{self, JobRecord, NewJob};
use audiovault_db::volumes::{self, LibraryRoot};
use audiovault_db::{Db, catalog};
use audiovault_indexer::analyze::{AnalyzeEnv, LEVEL_ANALYZED, analyze_copy};
use audiovault_indexer::hashing::{LEVEL_FINGERPRINTED, fingerprint_copy, full_hash_copy};
use audiovault_indexer::scan::{ScanOutcome, scan_root};
use audiovault_indexer::{IndexerError, JobContext, duplicates};
use audiovault_media::fingerprint::ChromaprintService;
use parking_lot::RwLock;
use serde_json::{Value, json};

use crate::events::{AppEvent, EventBus};
use crate::monitor::volume_mount;
use crate::settings::AppSettings;
use crate::{Result, ServiceError};

pub struct Executor {
    pub db: Arc<Db>,
    pub settings: Arc<RwLock<AppSettings>>,
    pub covers_dir: PathBuf,
    pub temp_dir: PathBuf,
    pub bundled_dirs: Vec<PathBuf>,
    pub events: EventBus,
}

fn ids_of(v: &Value, key: &str) -> Vec<i64> {
    v.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default()
}

fn req_i64(v: &Value, key: &str) -> Result<i64> {
    v.get(key).and_then(Value::as_i64).ok_or_else(|| ServiceError::Invalid(format!("job payload lacks {key}")))
}

fn cancelled_or(ctx: &dyn JobContext, e: IndexerError) -> ServiceError {
    if ctx.is_cancelled() { ServiceError::Cancelled } else { ServiceError::Indexer(e) }
}

impl Executor {
    pub fn ffmpeg(&self) -> Option<PathBuf> {
        let configured = self.settings.read().ffmpeg_path.clone();
        let configured = (!configured.trim().is_empty()).then(|| PathBuf::from(configured.trim()));
        audiovault_media::transcode::locate_ffmpeg(configured.as_deref(), &self.bundled_dirs)
    }

    fn fp_service(&self) -> ChromaprintService {
        ChromaprintService::new(self.ffmpeg())
    }

    pub fn enqueue(&self, job: &NewJob) -> Result<i64> {
        let conn = self.db.conn()?;
        let id = jobs::enqueue(&conn, job)?;
        self.events.emit(AppEvent::JobsChanged);
        Ok(id)
    }

    pub fn run(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        match job.kind {
            JobKind::Scan => self.scan(job, ctx),
            JobKind::Metadata | JobKind::QuickHash | JobKind::Thumbnails | JobKind::Optimize => self.analyze(job, ctx),
            JobKind::Duplicates => self.duplicates(job, ctx),
            JobKind::Fingerprint => self.fingerprint(job, ctx),
            JobKind::FullHash => self.full_hash(job, ctx),
            JobKind::FileOperation => crate::fileops::execute(self, req_i64(&job.payload, "op_id")?, ctx),
            JobKind::Transcode => crate::transcode::execute(self, job, ctx),
            JobKind::TorrentImport => self.torrent_import(job, ctx),
            JobKind::Reconcile => crate::fileops::reconcile(self, ctx),
        }
    }

    fn job_volume(&self, job: &JobRecord) -> Result<i64> {
        job.volume_id.ok_or_else(|| ServiceError::Invalid("job has no storage".into()))
    }

    /// Scan a library root (or a sub-folder of it).
    fn scan(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        let root_id = req_i64(&job.payload, "root_id")?;
        let root = {
            let conn = self.db.conn()?;
            volumes::get_root(&conn, root_id)?.ok_or_else(|| ServiceError::NotFound(format!("library folder {root_id}")))?
        };
        let (vol, mount) = volume_mount(&self.db, root.volume_id)?;
        let target = match job.payload.get("sub_path").and_then(Value::as_str) {
            Some(sub) => {
                validate_relative(sub).map_err(|e| ServiceError::Invalid(e.to_string()))?;
                LibraryRoot { rel_path: sub.to_string(), ..root.clone() }
            }
            None => root.clone(),
        };
        ctx.progress(-1.0, &format!("Scanning {}", if target.rel_path.is_empty() { "/" } else { &target.rel_path }));
        let summary = scan_root(&self.db, vol.id, &mount, &target, ctx, Some(&self.covers_dir)).map_err(|e| cancelled_or(ctx, e))?;
        self.events.emit(AppEvent::LibraryChanged);
        self.events.emit(AppEvent::VolumesChanged);
        if summary.outcome == ScanOutcome::Interrupted {
            return Err(if ctx.is_cancelled() {
                ServiceError::Cancelled
            } else {
                ServiceError::Offline(vol.display_name.clone())
            });
        }
        {
            let conn = self.db.conn()?;
            volumes::mark_scanned(&conn, vol.id)?;
        }
        let c = &summary.counters;
        ctx.progress(
            1.0,
            &format!("{} books found, {} new, {} missing, {} errors", c.copies_found, c.copies_new, c.copies_missing, c.errors),
        );
        self.enqueue(
            &NewJob::new(JobKind::Metadata, format!("Analyze files on {}", vol.display_name), json!({}))
                .volume(vol.id)
                .priority(5)
                .dedupe(format!("metadata:{}", vol.id)),
        )?;
        Ok(())
    }

    /// Probe/tags/FB2/covers/quality for every copy below the analyzed level.
    fn analyze(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        let vid = self.job_volume(job)?;
        let (vol, mount) = volume_mount(&self.db, vid)?;
        let env = AnalyzeEnv { covers_dir: self.covers_dir.clone(), policy: self.settings.read().optimize_policy() };
        let total: i64 = {
            let conn = self.db.conn()?;
            conn.query_row(
                "SELECT COUNT(*) FROM book_copy WHERE volume_id = ?1 AND state = 'present' AND analysis_level < ?2",
                rusqlite::params![vid, LEVEL_ANALYZED],
                |r| r.get(0),
            )?
        };
        let mut analyzed = Vec::new();
        let mut done = 0i64;
        let mut failures = 0usize;
        loop {
            if ctx.is_cancelled() {
                return Err(ServiceError::Cancelled);
            }
            let batch = {
                let conn = self.db.conn()?;
                catalog::copies_needing_analysis(&conn, vid, LEVEL_ANALYZED, 64)?
            };
            if batch.is_empty() {
                break;
            }
            for c in batch {
                ctx.progress(done as f64 / total.max(1) as f64, &format!("{done} of {total}: {}", c.rel_path));
                match analyze_copy(&self.db, c.id, &mount, &env, ctx) {
                    Ok(o) => analyzed.push(o.copy_id),
                    Err(IndexerError::Cancelled) => return Err(ServiceError::Cancelled),
                    Err(IndexerError::VolumeUnavailable(m)) => return Err(ServiceError::Offline(m)),
                    Err(e) => {
                        failures += 1;
                        tracing::warn!(copy = c.id, path = %c.rel_path, error = %e, "analysis failed");
                        // Never loop on the same broken copy: record it as analyzed (issues carry the problem).
                        let conn = self.db.conn()?;
                        catalog::set_copy_analysis_level(&conn, c.id, LEVEL_ANALYZED)?;
                    }
                }
                done += 1;
                if done % 16 == 0 {
                    self.events.emit(AppEvent::LibraryChanged);
                }
            }
        }
        self.events.emit(AppEvent::LibraryChanged);
        self.events.emit(AppEvent::IssuesChanged);
        if failures > 0 {
            self.events.warn(format!("{failures} folder(s) on {} could not be fully analyzed", vol.display_name));
        }
        ctx.progress(1.0, &format!("{} analyzed", analyzed.len()));
        if !analyzed.is_empty() {
            self.enqueue(
                &NewJob::new(JobKind::Duplicates, "Find duplicates", json!({ "copy_ids": analyzed })).priority(-1),
            )?;
        }
        Ok(())
    }

    fn duplicates(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        let svc = self.fp_service();
        let ids = ids_of(&job.payload, "copy_ids");
        let stats = if ids.is_empty() {
            duplicates::detect_all(&self.db, Some(&svc), ctx)
        } else {
            duplicates::detect(&self.db, &ids, Some(&svc), ctx)
        }
        .map_err(|e| cancelled_or(ctx, e))?;
        ctx.progress(1.0, &format!("{} candidate pair(s)", stats.candidates));
        self.events.emit(AppEvent::DuplicatesChanged);
        self.events.emit(AppEvent::LibraryChanged);
        let followup = !job.payload.get("no_fp_followup").and_then(Value::as_bool).unwrap_or(false);
        if followup && self.settings.read().acoustic_fingerprints {
            self.schedule_fingerprints()?;
        }
        Ok(())
    }

    /// Queue acoustic fingerprinting (per storage) for ambiguous pairs.
    pub fn schedule_fingerprints(&self) -> Result<usize> {
        let pairs = duplicates::pairs_needing_fingerprints(&self.db, 200)?;
        if pairs.is_empty() {
            return Ok(0);
        }
        let mut by_volume: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        {
            let conn = self.db.conn()?;
            for id in pairs.iter().flat_map(|(a, b)| [*a, *b]) {
                if let Some(c) = catalog::get_copy(&conn, id)?
                    && c.analysis_level < LEVEL_FINGERPRINTED
                {
                    let v = by_volume.entry(c.volume_id).or_default();
                    if !v.contains(&id) {
                        v.push(id);
                    }
                }
            }
        }
        let n = by_volume.len();
        for (vid, ids) in by_volume {
            self.enqueue(
                &NewJob::new(JobKind::Fingerprint, format!("Acoustic check of {} cop(ies)", ids.len()), json!({ "copy_ids": ids }))
                    .volume(vid)
                    .priority(-2),
            )?;
        }
        Ok(n)
    }

    fn fingerprint(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        let vid = self.job_volume(job)?;
        let (_, mount) = volume_mount(&self.db, vid)?;
        let svc = self.fp_service();
        let ids = ids_of(&job.payload, "copy_ids");
        for (i, id) in ids.iter().enumerate() {
            ctx.progress(i as f64 / ids.len().max(1) as f64, &format!("{} of {}", i + 1, ids.len()));
            match fingerprint_copy(&self.db, *id, &mount, &svc, ctx) {
                Ok(_) => {}
                Err(IndexerError::Cancelled) => return Err(ServiceError::Cancelled),
                Err(IndexerError::VolumeUnavailable(m)) => return Err(ServiceError::Offline(m)),
                Err(e) => tracing::warn!(copy = id, error = %e, "fingerprinting failed"),
            }
        }
        duplicates::detect(&self.db, &ids, Some(&svc), ctx).map_err(|e| cancelled_or(ctx, e))?;
        self.events.emit(AppEvent::DuplicatesChanged);
        Ok(())
    }

    fn full_hash(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        let vid = self.job_volume(job)?;
        let (_, mount) = volume_mount(&self.db, vid)?;
        let ids = ids_of(&job.payload, "copy_ids");
        for (i, id) in ids.iter().enumerate() {
            ctx.progress(i as f64 / ids.len().max(1) as f64, &format!("{} of {}", i + 1, ids.len()));
            full_hash_copy(&self.db, *id, &mount, ctx).map_err(|e| cancelled_or(ctx, e))?;
        }
        let svc = self.fp_service();
        duplicates::detect(&self.db, &ids, Some(&svc), ctx).map_err(|e| cancelled_or(ctx, e))?;
        self.events.emit(AppEvent::DuplicatesChanged);
        Ok(())
    }

    /// The library root that should own `rel` on a volume: an existing root
    /// containing it, otherwise a new root for `rel` itself.
    pub fn root_covering(&self, volume_id: i64, rel: &str) -> Result<LibraryRoot> {
        let conn = self.db.conn()?;
        let roots = volumes::roots(&conn, Some(volume_id))?;
        let covering = roots
            .into_iter()
            .filter(|r| r.rel_path.is_empty() || rel == r.rel_path || rel.starts_with(&format!("{}/", r.rel_path)))
            .max_by_key(|r| r.rel_path.len());
        if let Some(r) = covering {
            return Ok(r);
        }
        let id = volumes::add_root(&conn, volume_id, rel)?;
        volumes::get_root(&conn, id)?.ok_or_else(|| ServiceError::NotFound("library folder".into()))
    }

    /// Queue a scan of one folder (relative to the volume) so new content is cataloged.
    pub fn enqueue_subtree_scan(&self, volume_id: i64, rel: &str, title: &str) -> Result<i64> {
        let root = self.root_covering(volume_id, rel)?;
        let payload = if root.rel_path == rel { json!({ "root_id": root.id }) } else { json!({ "root_id": root.id, "sub_path": rel }) };
        self.enqueue(&NewJob::new(JobKind::Scan, title, payload).volume(volume_id).priority(8))
    }

    fn torrent_import(&self, job: &JobRecord, ctx: &dyn JobContext) -> Result<()> {
        let tid = req_i64(&job.payload, "torrent_id")?;
        let rec = {
            let conn = self.db.conn()?;
            audiovault_db::torrents::get(&conn, tid)?.ok_or_else(|| ServiceError::NotFound(format!("download {tid}")))?
        };
        let vid = rec.volume_id.ok_or_else(|| ServiceError::Invalid("download has no destination storage".into()))?;
        let (vol, mount) = volume_mount(&self.db, vid)?;
        let dest = audiovault_core::paths::safe_join(&mount, &rec.dest_rel_path).map_err(|e| ServiceError::Invalid(e.to_string()))?;
        if !dest.exists() {
            return Err(ServiceError::NotFound(format!("downloaded folder {}", dest.display())));
        }
        let root = self.root_covering(vid, &rec.dest_rel_path)?;
        let target = LibraryRoot { rel_path: rec.dest_rel_path.clone(), ..root };
        ctx.progress(-1.0, &format!("Importing {}", rec.name));
        let summary = scan_root(&self.db, vid, &mount, &target, ctx, Some(&self.covers_dir)).map_err(|e| cancelled_or(ctx, e))?;
        if summary.outcome == ScanOutcome::Interrupted {
            return Err(if ctx.is_cancelled() { ServiceError::Cancelled } else { ServiceError::Offline(vol.display_name) });
        }
        {
            let conn = self.db.conn()?;
            audiovault_db::torrents::mark_imported(&conn, tid)?;
        }
        self.enqueue(
            &NewJob::new(JobKind::Metadata, format!("Analyze files on {}", vol.display_name), json!({}))
                .volume(vid)
                .priority(6)
                .dedupe(format!("metadata:{vid}")),
        )?;
        self.events.emit(AppEvent::LibraryChanged);
        self.events.emit(AppEvent::TorrentsChanged);
        self.events.info(format!("Imported \"{}\" into the library", rec.name));
        Ok(())
    }
}
