//! Main application service facade.
//!
//! [`AppService`] is the central hub of AudioVault: it coordinates the database,
//! background jobs, volume monitoring, I/O scheduling, torrent transfers, and
//! UI events.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use audiovault_core::model::{DuplicateStatus, IssueKind, JobKind};
use audiovault_db::duplicates::{DuplicatePair, count_open, list_open};
use audiovault_db::issues::{IssueRow, counts_by_kind};
use audiovault_db::jobs::{JobRecord, NewJob};
use audiovault_db::library::{BookDetails, CatalogStats, LibraryFilter, LibraryRow, LibrarySort};
use audiovault_db::optimize::{OptimizationRow, OptimizationTotals, totals};
use audiovault_db::search::SearchHit;
use audiovault_db::volumes::VolumeSummary;
use audiovault_db::{Db, catalog, duplicates, fileops, issues, library, optimize, scans, search, volumes};
use audiovault_platform::credentials::{CredentialStore, KeyringStore};
use audiovault_platform::paths::AppPaths;
use audiovault_platform::volumes::{SystemVolumes, VolumeEnumerator};
use parking_lot::RwLock;
use serde_json::json;

use crate::events::{AppEvent, EventBus};
use crate::exec::Executor;
use crate::monitor::{RegisteredFolder, refresh, register_folder};
use crate::runner::Runner;
use crate::settings::AppSettings;
use crate::torrents::TorrentService;
use crate::{Result, ServiceError};

pub struct AppService {
    paths: AppPaths,
    db: Arc<Db>,
    settings: Arc<RwLock<AppSettings>>,
    events: EventBus,
    enumerator: Arc<dyn VolumeEnumerator>,
    credentials: Arc<dyn CredentialStore>,
    runner: Runner,
    exec: Arc<Executor>,
    torrents: Arc<TorrentService>,
    stop: Arc<AtomicBool>,
}

impl AppService {
    /// Opens the application catalog, runs crash-recovery, starts background tasks,
    /// and initializes the torrent engine.
    pub async fn open(
        paths: AppPaths,
        enumerator: Option<Arc<dyn VolumeEnumerator>>,
        credentials: Option<Arc<dyn CredentialStore>>,
    ) -> Result<Arc<Self>> {
        paths.ensure().map_err(ServiceError::from)?;

        let db = Db::open(&paths.catalog_path())?;

        // 1. Crash recovery on catalog startup
        {
            let conn = db.conn()?;
            let _ = scans::interrupt_running_on_startup(&conn);
            let (requeued, interrupted) = audiovault_db::jobs::recover_on_startup(&conn)?;
            if requeued > 0 || interrupted > 0 {
                tracing::info!(requeued, interrupted, "recovered background jobs from previous session");
            }
            let incomplete_ops = fileops::flag_incomplete_on_startup(&conn)?;
            if !incomplete_ops.is_empty() {
                tracing::warn!(count = incomplete_ops.len(), "found incomplete file operations; scheduling reconciliation");
                let _ = audiovault_db::jobs::enqueue(
                    &conn,
                    &NewJob::new(JobKind::Reconcile, "Verify file operations after restart", json!({})).priority(50),
                );
            }
        }

        let settings = Arc::new(RwLock::new(AppSettings::load(&db)?));
        let events = EventBus::default();
        let enumerator = enumerator.unwrap_or_else(|| Arc::new(SystemVolumes));
        let credentials = credentials.unwrap_or_else(|| Arc::new(KeyringStore::default()));

        let exec = Arc::new(Executor {
            db: db.clone(),
            settings: settings.clone(),
            covers_dir: paths.cover_cache_dir.clone(),
            temp_dir: paths.temp_dir.clone(),
            bundled_dirs: audiovault_platform::paths::bundled_resource_dirs(),
            events: events.clone(),
        });

        let runner = Runner::new(db.clone(), exec.clone(), events.clone(), settings.read().max_parallel_jobs);
        runner.start()?;

        let torrent_download_dir = paths.data_dir.join("downloads");
        let torrents = Arc::new(TorrentService::new(
            db.clone(),
            settings.clone(),
            credentials.clone(),
            events.clone(),
            paths.torrent_state_dir.clone(),
            torrent_download_dir,
        )?);

        // Start torrent engine in background task
        let torrents_clone = torrents.clone();
        tokio::spawn(async move {
            if let Err(e) = torrents_clone.start().await {
                tracing::error!(error = %e, "failed to start torrent engine");
            }
        });

        let stop = Arc::new(AtomicBool::new(false));

        let service = Arc::new(AppService {
            paths,
            db,
            settings,
            events,
            enumerator,
            credentials,
            runner,
            exec,
            torrents,
            stop: stop.clone(),
        });

        // Initial volume detection
        let _ = refresh(&service.db, service.enumerator.as_ref());

        // Spawn periodic background polling loop (volume monitor + torrent stats)
        let svc_weak = Arc::downgrade(&service);
        tokio::spawn(async move {
            let mut tick_counter = 0u64;
            while !stop.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let Some(svc) = svc_weak.upgrade() else {
                    break;
                };

                // Poll torrent stats every 1.5s
                let _ = svc.torrents.poll_stats().await;

                // Check volumes every 3.0s (every second tick)
                tick_counter += 1;
                if tick_counter % 2 == 0 {
                    if let Ok(ch) = refresh(&svc.db, svc.enumerator.as_ref()) {
                        if !ch.is_empty() {
                            svc.events.emit(AppEvent::VolumesChanged);
                            if !ch.came_online.is_empty() && svc.settings.read().auto_scan_on_connect {
                                for vid in ch.came_online {
                                    let _ = svc.scan_volume(vid);
                                }
                            }
                        }
                    }
                }
            }
        });

        Ok(service)
    }

    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }

    pub fn credentials(&self) -> &Arc<dyn CredentialStore> {
        &self.credentials
    }

    pub fn exec(&self) -> &Arc<Executor> {
        &self.exec
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    pub fn runner(&self) -> &Runner {
        &self.runner
    }

    pub fn torrents(&self) -> &Arc<TorrentService> {
        &self.torrents
    }

    // --- Settings ---

    pub fn settings(&self) -> AppSettings {
        self.settings.read().clone()
    }

    pub fn update_settings(&self, s: AppSettings) -> Result<()> {
        let s = s.sanitized();
        s.save(&self.db)?;
        self.runner.set_max_parallel(s.max_parallel_jobs);
        self.runner.set_inhibit_sleep(s.inhibit_sleep);
        *self.settings.write() = s;
        self.events.emit(AppEvent::JobsChanged);
        Ok(())
    }

    // --- Library Browsing & Details ---

    pub fn library_count(&self, filter: LibraryFilter) -> Result<i64> {
        let conn = self.db.conn()?;
        Ok(library::count(&conn, filter)?)
    }

    pub fn library_page(&self, filter: LibraryFilter, sort: LibrarySort, offset: i64, limit: i64) -> Result<Vec<LibraryRow>> {
        let conn = self.db.conn()?;
        Ok(library::page(&conn, filter, sort, offset, limit)?)
    }

    pub fn book_details(&self, book_id: i64) -> Result<Option<BookDetails>> {
        let conn = self.db.conn()?;
        Ok(library::details(&conn, book_id)?)
    }

    pub fn catalog_stats(&self) -> Result<CatalogStats> {
        let conn = self.db.conn()?;
        Ok(library::stats(&conn)?)
    }

    // --- Search ---

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let conn = self.db.conn()?;
        Ok(search::search(&conn, query, limit)?)
    }

    // --- Storage Volumes & Library Folders ---

    pub fn volume_summaries(&self) -> Result<Vec<VolumeSummary>> {
        let conn = self.db.conn()?;
        Ok(volumes::summaries(&conn)?)
    }

    pub fn add_storage_folder(&self, path: &Path, write_marker: bool) -> Result<RegisteredFolder> {
        let reg = register_folder(&self.db, self.enumerator.as_ref(), path, write_marker)?;
        self.events.emit(AppEvent::VolumesChanged);

        // Queue scan of the newly added folder
        let conn = self.db.conn()?;
        if let Some(root) = volumes::get_root(&conn, reg.root_id)? {
            let _ = self.runner.enqueue(
                &NewJob::new(JobKind::Scan, format!("Scan {}", root.rel_path), json!({ "root_id": root.id }))
                    .volume(reg.volume_id)
                    .priority(10),
            );
        }
        Ok(reg)
    }

    pub fn scan_volume(&self, volume_id: i64) -> Result<usize> {
        let roots = {
            let conn = self.db.conn()?;
            volumes::roots(&conn, Some(volume_id))?
        };
        let mut queued = 0;
        for r in roots {
            if r.enabled {
                let _ = self.runner.enqueue(
                    &NewJob::new(JobKind::Scan, format!("Scan {}", r.rel_path), json!({ "root_id": r.id }))
                        .volume(volume_id)
                        .priority(8),
                )?;
                queued += 1;
            }
        }
        Ok(queued)
    }

    pub fn scan_root(&self, root_id: i64) -> Result<i64> {
        let conn = self.db.conn()?;
        let root = volumes::get_root(&conn, root_id)?.ok_or_else(|| ServiceError::NotFound(format!("folder {root_id}")))?;
        self.runner.enqueue(
            &NewJob::new(JobKind::Scan, format!("Scan {}", root.rel_path), json!({ "root_id": root.id }))
                .volume(root.volume_id)
                .priority(10),
        )
    }

    pub fn delete_volume(&self, volume_id: i64) -> Result<()> {
        let conn = self.db.conn()?;
        volumes::delete_volume(&conn, volume_id)?;
        self.events.emit(AppEvent::VolumesChanged);
        self.events.emit(AppEvent::LibraryChanged);
        Ok(())
    }

    // --- Duplicates ---

    pub fn duplicate_pairs(&self, include_variants: bool, offset: i64, limit: i64) -> Result<Vec<DuplicatePair>> {
        let conn = self.db.conn()?;
        Ok(list_open(&conn, include_variants, offset, limit)?)
    }

    pub fn duplicate_counts(&self) -> Result<(i64, i64)> {
        let conn = self.db.conn()?;
        Ok(count_open(&conn)?)
    }

    pub fn resolve_duplicate(&self, pair_id: i64, status: DuplicateStatus) -> Result<()> {
        let conn = self.db.conn()?;
        let affected_books = duplicates::set_status(&conn, pair_id, status)?;
        for b in affected_books {
            let _ = catalog::refresh_book(&conn, b);
        }
        self.events.emit(AppEvent::DuplicatesChanged);
        self.events.emit(AppEvent::LibraryChanged);
        Ok(())
    }

    pub fn find_all_duplicates(&self) -> Result<i64> {
        self.runner.enqueue(&NewJob::new(JobKind::Duplicates, "Catalog duplicate search", json!({})).priority(2))
    }

    // --- Metadata Issues ---

    pub fn issues(&self, kind: Option<IssueKind>, offset: i64, limit: i64) -> Result<Vec<IssueRow>> {
        let conn = self.db.conn()?;
        Ok(issues::list_open(&conn, kind, offset, limit)?)
    }

    pub fn issue_counts(&self) -> Result<Vec<(IssueKind, i64)>> {
        let conn = self.db.conn()?;
        Ok(counts_by_kind(&conn)?)
    }

    pub fn ignore_issue(&self, issue_id: i64) -> Result<()> {
        let conn = self.db.conn()?;
        let affected_books = issues::set_state(&conn, &[issue_id], "ignored")?;
        for b in affected_books {
            let _ = catalog::refresh_book(&conn, b);
        }
        self.events.emit(AppEvent::IssuesChanged);
        self.events.emit(AppEvent::LibraryChanged);
        Ok(())
    }

    // --- Storage Optimizer ---

    pub fn optimizations(&self, states: &[&str], volume_id: Option<i64>, offset: i64, limit: i64) -> Result<Vec<OptimizationRow>> {
        let conn = self.db.conn()?;
        Ok(optimize::list(&conn, states, volume_id, offset, limit)?)
    }

    pub fn optimization_totals(&self) -> Result<OptimizationTotals> {
        let conn = self.db.conn()?;
        Ok(totals(&conn)?)
    }

    pub fn start_transcode(&self, opt_id: i64, replace_source: bool, dest_rel: Option<String>) -> Result<i64> {
        let opt = {
            let conn = self.db.conn()?;
            optimize::get(&conn, opt_id)?.ok_or_else(|| ServiceError::NotFound(format!("optimization {opt_id}")))?
        };

        let mut payload = json!({
            "optimization_id": opt_id,
            "replace_source": replace_source,
        });
        if let Some(d) = dest_rel {
            payload["dest_rel_path"] = json!(d);
        }

        let job_id = self.runner.enqueue(
            &NewJob::new(JobKind::Transcode, format!("Optimize: {}", opt.title), payload)
                .volume(opt.volume_id)
                .priority(5),
        )?;

        let conn = self.db.conn()?;
        optimize::set_state(&conn, opt_id, "queued")?;

        Ok(job_id)
    }

    // --- Background Tasks ---

    pub fn jobs(&self, recent_limit: i64) -> Result<Vec<JobRecord>> {
        let conn = self.db.conn()?;
        Ok(audiovault_db::jobs::list(&conn, recent_limit)?)
    }

    pub fn clear_finished_jobs(&self) -> Result<usize> {
        let conn = self.db.conn()?;
        let count = audiovault_db::jobs::clear_finished(&conn)?;
        self.events.emit(AppEvent::JobsChanged);
        Ok(count)
    }

    // --- File Operations ---

    pub fn plan_delete_copy(&self, copy_id: i64, permanent: bool, confirmed: bool) -> Result<i64> {
        let (op_id, job) = crate::fileops::plan_delete(&self.db, copy_id, permanent, confirmed)?;
        if confirmed {
            self.runner.enqueue(&job)?;
        }
        Ok(op_id)
    }

    pub fn plan_rename_copy(&self, copy_id: i64, new_name: &str) -> Result<i64> {
        let (op_id, job) = crate::fileops::plan_rename(&self.db, copy_id, new_name)?;
        self.runner.enqueue(&job)?;
        Ok(op_id)
    }

    pub fn plan_move_copy(&self, copy_id: i64, dest_volume_id: i64, dest_parent_rel: &str, keep_source: bool) -> Result<i64> {
        let (op_id, job) = crate::fileops::plan_move(&self.db, copy_id, dest_volume_id, dest_parent_rel, keep_source)?;
        self.runner.enqueue(&job)?;
        Ok(op_id)
    }

    // --- Graceful Shutdown ---

    pub async fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        self.runner.shutdown(Duration::from_secs(3));
        self.torrents.shutdown().await;
        let _ = self.db.checkpoint();
        tracing::info!("AudioVault service shutdown complete");
    }
}
