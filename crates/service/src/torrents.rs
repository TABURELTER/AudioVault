//! Torrent management service.
//!
//! Bridges the catalog database, the embedded [`TorrentEngine`], and torrent
//! search providers (RuTracker). Handles adding torrents, monitoring transfer
//! progress, import of completed downloads into the library, and secure
//! credential storage.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use audiovault_core::model::{JobKind, TorrentState};
use audiovault_core::paths::{safe_join, validate_relative};
use audiovault_db::Db;
use audiovault_db::jobs::NewJob;
use audiovault_db::torrents::{self, NewTorrent};
use audiovault_platform::credentials::CredentialStore;
use audiovault_torrent::engine::{EngineConfig, TorrentEngine, TorrentPreview, TorrentSource};
use audiovault_torrent::provider::{SearchProvider, SearchQuery, SearchResult};
use audiovault_torrent::rutracker::RutrackerClient;
use parking_lot::RwLock;
use serde_json::json;

use crate::events::{AppEvent, EventBus};
use crate::monitor::volume_mount;
use crate::settings::{AppSettings, RUTRACKER_ACCOUNT_PREFIX};
use crate::{Result, ServiceError};

pub struct TorrentService {
    db: Arc<Db>,
    engine: Arc<RwLock<Option<Arc<TorrentEngine>>>>,
    rutracker: Arc<RwLock<Arc<RutrackerClient>>>,
    settings: Arc<RwLock<AppSettings>>,
    credentials: Arc<dyn CredentialStore>,
    events: EventBus,
    state_dir: PathBuf,
    default_download_dir: PathBuf,
}

impl TorrentService {
    pub fn new(
        db: Arc<Db>,
        settings: Arc<RwLock<AppSettings>>,
        credentials: Arc<dyn CredentialStore>,
        events: EventBus,
        state_dir: PathBuf,
        default_download_dir: PathBuf,
    ) -> Result<Self> {
        let (base, proxy, cookie) = {
            let s = settings.read();
            (
                s.rutracker_base_url.clone(),
                s.rutracker_proxy.clone(),
                s.rutracker_cookie.clone(),
            )
        };
        let rutracker = Arc::new(RutrackerClient::new(
            Some(&base),
            Some(&proxy),
            Some(&cookie),
        )?);

        Ok(TorrentService {
            db,
            engine: Arc::new(RwLock::new(None)),
            rutracker: Arc::new(RwLock::new(rutracker)),
            settings,
            credentials,
            events,
            state_dir,
            default_download_dir,
        })
    }

    pub fn rutracker_client(&self) -> Arc<RutrackerClient> {
        self.rutracker.read().clone()
    }

    pub fn reload_rutracker(&self) -> Result<()> {
        let (base, proxy, cookie) = {
            let s = self.settings.read();
            (
                s.rutracker_base_url.clone(),
                s.rutracker_proxy.clone(),
                s.rutracker_cookie.clone(),
            )
        };
        let client = Arc::new(RutrackerClient::new(
            Some(&base),
            Some(&proxy),
            Some(&cookie),
        )?);
        *self.rutracker.write() = client;
        Ok(())
    }

    pub async fn check_rutracker_session(&self) -> Result<String> {
        self.rutracker_client()
            .check_session()
            .await
            .map_err(ServiceError::from)
    }

    fn engine(&self) -> Option<Arc<TorrentEngine>> {
        self.engine.read().clone()
    }

    /// Initializes and starts the embedded BitTorrent engine.
    pub async fn start(&self) -> Result<()> {
        let s = self.settings.read().clone();
        let cfg = EngineConfig {
            data_dir: self.state_dir.clone(),
            default_download_dir: self.default_download_dir.clone(),
            listen_port: s.torrent_listen_port,
            enable_upnp: s.torrent_enable_upnp,
            enable_dht: s.torrent_enable_dht,
            download_limit_bps: if s.torrent_download_limit_kib > 0 {
                Some(s.torrent_download_limit_kib * 1024)
            } else {
                None
            },
            upload_limit_bps: if s.torrent_upload_limit_kib > 0 {
                Some(s.torrent_upload_limit_kib * 1024)
            } else {
                None
            },
        };

        let engine = Arc::new(TorrentEngine::start(cfg).await?);
        *self.engine.write() = Some(engine);
        tracing::info!("torrent engine initialized");

        // Re-attach active transfers from DB that may not be loaded yet
        self.restore_active_transfers().await?;

        Ok(())
    }

    async fn restore_active_transfers(&self) -> Result<()> {
        let transfers = {
            let conn = self.db.conn()?;
            torrents::list(&conn)?
        };

        let Some(engine) = self.engine() else {
            return Ok(());
        };

        for t in transfers {
            if matches!(
                t.state,
                TorrentState::Queued | TorrentState::Downloading | TorrentState::Initializing
            ) {
                if let Some(h) = &t.info_hash
                    && engine.contains(h)
                {
                    continue;
                }

                let source = if let Some(mag) = &t.magnet {
                    Some(TorrentSource::Magnet(mag.clone()))
                } else if let Some(bytes) = &t.torrent_data {
                    Some(TorrentSource::TorrentFile(bytes.clone()))
                } else {
                    None
                };

                if let Some(src) = source
                    && let Some(vid) = t.volume_id
                    && let Ok((_, mount)) = volume_mount(&self.db, vid)
                    && let Ok(out_folder) = safe_join(&mount, &t.dest_rel_path)
                {
                    let paused = t.state == TorrentState::Paused;
                    let _ = engine
                        .add(&src, &out_folder, t.selected_files.clone(), paused)
                        .await;
                }
            }
        }
        Ok(())
    }

    /// Polls transfer statistics from the engine, updating DB rows and triggering library import on completion.
    pub async fn poll_stats(&self) -> Result<()> {
        let transfers = {
            let conn = self.db.conn()?;
            torrents::list(&conn)?
        };

        let Some(engine) = self.engine() else {
            return Ok(());
        };

        let mut changed = false;
        let mut completed_transfers = Vec::new();

        for t in transfers {
            let Some(info_hash) = &t.info_hash else {
                continue;
            };

            if let Ok(stats) = engine.stats(info_hash) {
                let frac = stats.fraction();
                let is_completed = stats.finished
                    || matches!(stats.state, TorrentState::Completed | TorrentState::Seeding);

                let conn = self.db.conn()?;
                torrents::update_stats(
                    &conn,
                    t.id,
                    frac,
                    stats.total_bytes,
                    stats.progress_bytes,
                    stats.uploaded_bytes,
                )?;

                let new_state = stats.state;
                if new_state != t.state {
                    torrents::set_state(&conn, t.id, new_state, stats.error.as_deref())?;
                    changed = true;
                }

                if is_completed && t.imported_at.is_none() {
                    completed_transfers.push(t);
                }
            }
        }

        if changed {
            self.events.emit(AppEvent::TorrentsChanged);
        }

        // Automatic library import for finished downloads
        if self.settings.read().auto_import_downloads {
            for comp in completed_transfers {
                if let Some(vid) = comp.volume_id {
                    let job = NewJob::new(
                        JobKind::TorrentImport,
                        format!("Import download: {}", comp.name),
                        json!({ "torrent_id": comp.id }),
                    )
                    .volume(vid)
                    .priority(7)
                    .dedupe(format!("torrent_import:{}", comp.id));

                    let conn = self.db.conn()?;
                    if audiovault_db::jobs::enqueue(&conn, &job).is_ok() {
                        self.events.emit(AppEvent::JobsChanged);
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolves metadata and previews the files within a torrent before downloading.
    pub async fn preview(&self, source: &TorrentSource) -> Result<TorrentPreview> {
        let engine = self
            .engine()
            .ok_or_else(|| ServiceError::Invalid("torrent engine not running".into()))?;
        engine
            .preview(source, Duration::from_secs(25))
            .await
            .map_err(ServiceError::from)
    }

    /// Adds a torrent for downloading to a specified storage volume and relative directory.
    pub async fn add(
        &self,
        source: &TorrentSource,
        volume_id: i64,
        dest_rel_path: &str,
        selected_files: Option<Vec<usize>>,
        paused: bool,
    ) -> Result<i64> {
        let (vol, mount) = volume_mount(&self.db, volume_id)?;
        validate_relative(dest_rel_path).map_err(|e| ServiceError::Invalid(e.to_string()))?;
        let output_folder =
            safe_join(&mount, dest_rel_path).map_err(|e| ServiceError::Invalid(e.to_string()))?;

        std::fs::create_dir_all(&output_folder)?;

        let engine = self
            .engine()
            .ok_or_else(|| ServiceError::Invalid("torrent engine not running".into()))?;

        let added = engine
            .add(source, &output_folder, selected_files.clone(), paused)
            .await?;

        let (src_kind, magnet, torrent_data) = match source {
            TorrentSource::Magnet(m) => ("magnet".to_string(), Some(m.clone()), None),
            TorrentSource::TorrentFile(b) => ("file".to_string(), None, Some(b.clone())),
        };

        let new_t = NewTorrent {
            name: added
                .name
                .clone()
                .unwrap_or_else(|| added.info_hash.clone()),
            source_kind: src_kind,
            magnet,
            torrent_data,
            provider: None,
            provider_ref: None,
            volume_id: Some(volume_id),
            dest_rel_path: dest_rel_path.to_string(),
            selected_files,
            info_hash: Some(added.info_hash.clone()),
        };

        let id = {
            let conn = self.db.conn()?;
            let id = torrents::insert(&conn, &new_t)?;
            torrents::set_engine(
                &conn,
                id,
                Some(added.engine_id as i64),
                Some(&added.info_hash),
                added.name.as_deref(),
            )?;
            torrents::set_state(
                &conn,
                id,
                if paused {
                    TorrentState::Paused
                } else {
                    TorrentState::Downloading
                },
                None,
            )?;
            id
        };

        self.events.emit(AppEvent::TorrentsChanged);
        self.events
            .info(format!("Added download to {}", vol.display_name));

        Ok(id)
    }

    pub async fn pause(&self, id: i64) -> Result<()> {
        let rec = {
            let conn = self.db.conn()?;
            torrents::get(&conn, id)?
                .ok_or_else(|| ServiceError::NotFound(format!("download {id}")))?
        };

        if let Some(h) = &rec.info_hash
            && let Some(engine) = self.engine()
        {
            let _ = engine.pause(h).await;
        }

        let conn = self.db.conn()?;
        torrents::set_state(&conn, id, TorrentState::Paused, None)?;
        self.events.emit(AppEvent::TorrentsChanged);
        Ok(())
    }

    pub async fn resume(&self, id: i64) -> Result<()> {
        let rec = {
            let conn = self.db.conn()?;
            torrents::get(&conn, id)?
                .ok_or_else(|| ServiceError::NotFound(format!("download {id}")))?
        };

        if let Some(h) = &rec.info_hash
            && let Some(engine) = self.engine()
        {
            let _ = engine.resume(h).await;
        }

        let conn = self.db.conn()?;
        torrents::set_state(&conn, id, TorrentState::Downloading, None)?;
        self.events.emit(AppEvent::TorrentsChanged);
        Ok(())
    }

    /// Removes a transfer. Downloaded files are deleted only if `delete_files` AND `confirmed` are both true.
    pub async fn remove(&self, id: i64, delete_files: bool, confirmed: bool) -> Result<()> {
        if delete_files && !confirmed {
            return Err(ServiceError::Invalid(
                "deleting downloaded files requires confirmation".into(),
            ));
        }

        let rec = {
            let conn = self.db.conn()?;
            torrents::get(&conn, id)?
                .ok_or_else(|| ServiceError::NotFound(format!("download {id}")))?
        };

        if let Some(h) = &rec.info_hash
            && let Some(engine) = self.engine()
        {
            let _ = engine.remove(h, delete_files).await;
        }

        let conn = self.db.conn()?;
        torrents::mark_removed(&conn, id)?;
        self.events.emit(AppEvent::TorrentsChanged);
        Ok(())
    }

    /// Authenticates with RuTracker using credentials saved in the OS keyring.
    pub async fn login_rutracker(&self, username: &str, password: &str) -> Result<()> {
        let client = self.rutracker_client();
        client
            .login(username, password)
            .await
            .map_err(ServiceError::from)?;

        let account = format!("{RUTRACKER_ACCOUNT_PREFIX}{username}");
        self.credentials.set(&account, password)?;

        {
            let mut s = self.settings.write();
            s.rutracker_username = username.to_string();
            let _ = s.save(&self.db);
        }

        self.events.info("Logged in to RuTracker");
        Ok(())
    }

    /// Searches RuTracker for audiobooks. Automatically logs in if a saved password exists.
    pub async fn search_rutracker(&self, text: &str) -> Result<Vec<SearchResult>> {
        let client = self.rutracker_client();
        if !client.is_logged_in().await {
            let user = self.settings.read().rutracker_username.clone();
            if !user.is_empty() {
                let account = format!("{RUTRACKER_ACCOUNT_PREFIX}{user}");
                if let Ok(Some(pass)) = self.credentials.get(&account) {
                    let _ = client.login(&user, &pass).await;
                }
            }
        }

        let cats = self.settings.read().rutracker_categories();
        let q = SearchQuery {
            text: text.to_string(),
            categories: cats,
        };

        client.search(&q).await.map_err(ServiceError::from)
    }

    /// Fetches the `.torrent` file for a RuTracker topic.
    pub async fn fetch_rutracker_torrent(&self, topic_id: &str) -> Result<Vec<u8>> {
        self.rutracker_client()
            .fetch_torrent(topic_id)
            .await
            .map_err(ServiceError::from)
    }

    pub async fn shutdown(&self) {
        let engine = self.engine.write().take();
        if let Some(e) = engine {
            e.shutdown().await;
        }
    }
}
