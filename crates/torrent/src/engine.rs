//! Embedded BitTorrent engine (librqbit) wrapper.
//!
//! Transfers are addressed by their lowercase hex info hash, which is stable
//! across restarts (librqbit's numeric ids are session-local). The session
//! persists its torrent list and fast-resume data under `<data_dir>/session`,
//! and DHT state under `<data_dir>/dht.json`.

use std::collections::HashSet;
use std::net::{Ipv4Addr, SocketAddr};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use audiovault_core::model::TorrentState;
use librqbit::api::TorrentIdOrHash;
use librqbit::dht::{DhtPersistenceConfig, Id20};
use librqbit::limits::LimitsConfig;
use librqbit::{
    AddTorrent, AddTorrentOptions, AddTorrentResponse, DhtSessionConfig, ListenerOptions,
    ManagedTorrent, Session, SessionOptions, SessionPersistenceConfig, TorrentStatsState,
};
use serde::Serialize;

use crate::magnet::{normalize_hash, parse_magnet};
use crate::{Result, TorrentError};

/// Where torrent content comes from.
#[derive(Debug, Clone)]
pub enum TorrentSource {
    Magnet(String),
    TorrentFile(Vec<u8>),
}

impl TorrentSource {
    fn to_add(&self) -> Result<AddTorrent<'static>> {
        match self {
            TorrentSource::Magnet(m) => {
                if parse_magnet(m).is_none() {
                    return Err(TorrentError::InvalidSource(
                        "not a valid magnet link".into(),
                    ));
                }
                Ok(AddTorrent::from_url(m.clone()))
            }
            TorrentSource::TorrentFile(b) => {
                if b.first() != Some(&b'd') {
                    return Err(TorrentError::InvalidSource(
                        "not a bencoded .torrent file".into(),
                    ));
                }
                Ok(AddTorrent::from_bytes(b.clone()))
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Engine state directory (session persistence, DHT state).
    pub data_dir: PathBuf,
    /// Fallback output folder; every add call passes an explicit folder anyway.
    pub default_download_dir: PathBuf,
    /// TCP listen port for incoming peers (0 = random).
    pub listen_port: u16,
    pub enable_upnp: bool,
    pub enable_dht: bool,
    /// Bytes per second; `None` = unlimited.
    pub download_limit_bps: Option<u32>,
    pub upload_limit_bps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreviewFile {
    pub index: usize,
    /// Path inside the torrent, `/`-separated.
    pub path: String,
    pub len: u64,
    /// BEP-47 padding files are never shown nor downloaded.
    pub padding: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TorrentPreview {
    pub info_hash: String,
    pub name: String,
    pub files: Vec<PreviewFile>,
    pub total_bytes: u64,
    #[serde(skip)]
    pub torrent_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AddedTorrent {
    pub info_hash: String,
    pub engine_id: usize,
    pub name: Option<String>,
    pub already_managed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EngineStats {
    pub state: TorrentState,
    pub progress_bytes: u64,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub finished: bool,
    pub download_bps: f64,
    pub upload_bps: f64,
    pub eta_secs: Option<u64>,
    pub error: Option<String>,
    pub file_progress: Vec<u64>,
}

impl EngineStats {
    pub fn fraction(&self) -> f64 {
        if self.total_bytes == 0 {
            if self.finished { 1.0 } else { 0.0 }
        } else {
            (self.progress_bytes as f64 / self.total_bytes as f64).clamp(0.0, 1.0)
        }
    }
}

pub struct TorrentEngine {
    session: Arc<Session>,
    data_dir: PathBuf,
}

fn eng<E: std::fmt::Display>(e: E) -> TorrentError {
    TorrentError::Engine(e.to_string())
}

fn parse_id(info_hash: &str) -> Result<Id20> {
    let hex = normalize_hash(info_hash)
        .ok_or_else(|| TorrentError::InvalidSource(format!("bad info hash {info_hash}")))?;
    Id20::from_str(&hex).map_err(eng)
}

impl TorrentEngine {
    /// Starts the session. Must be called inside a tokio runtime.
    pub async fn start(cfg: EngineConfig) -> Result<Self> {
        std::fs::create_dir_all(&cfg.data_dir).map_err(eng)?;
        let session_dir = cfg.data_dir.join("session");
        std::fs::create_dir_all(&session_dir).map_err(eng)?;
        std::fs::create_dir_all(&cfg.default_download_dir).map_err(eng)?;
        let dht = if cfg.enable_dht {
            Some(DhtSessionConfig {
                persistence: Some(DhtPersistenceConfig {
                    dump_interval: None,
                    config_filename: Some(cfg.data_dir.join("dht.json")),
                }),
                ..Default::default()
            })
        } else {
            None
        };
        let listen = ListenerOptions {
            listen_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, cfg.listen_port)),
            enable_upnp_port_forwarding: cfg.enable_upnp,
            ..Default::default()
        };
        let opts = SessionOptions {
            dht,
            fastresume: true,
            persistence: Some(SessionPersistenceConfig::Json {
                folder: Some(session_dir),
            }),
            listen: Some(listen),
            ratelimits: LimitsConfig {
                download_bps: cfg.download_limit_bps.and_then(NonZeroU32::new),
                upload_bps: cfg.upload_limit_bps.and_then(NonZeroU32::new),
            },
            client_name_and_version: Some(format!("AudioVault {}", env!("CARGO_PKG_VERSION"))),
            ..Default::default()
        };
        let session = Session::new_with_opts(cfg.default_download_dir.clone(), opts)
            .await
            .map_err(eng)?;
        Ok(Self {
            session,
            data_dir: cfg.data_dir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Resolves metadata (for magnets this contacts DHT/trackers) and lists files
    /// without starting a download.
    pub async fn preview(
        &self,
        source: &TorrentSource,
        timeout: Duration,
    ) -> Result<TorrentPreview> {
        let add = source.to_add()?;
        let fut = self.session.add_torrent(
            add,
            Some(AddTorrentOptions {
                list_only: true,
                ..Default::default()
            }),
        );
        let resp = tokio::time::timeout(timeout, fut)
            .await
            .map_err(|_| TorrentError::Timeout)?
            .map_err(eng)?;
        match resp {
            AddTorrentResponse::ListOnly(r) => {
                let mut files = Vec::new();
                let mut total = 0u64;
                for (index, fd) in r.info.iter_file_details().enumerate() {
                    let padding = fd.attrs().padding;
                    if !padding {
                        total += fd.len;
                    }
                    files.push(PreviewFile {
                        index,
                        path: fd.filename.to_vec().join("/"),
                        len: fd.len,
                        padding,
                    });
                }
                let name = r
                    .info
                    .name()
                    .map(|n| n.into_owned())
                    .or_else(|| match source {
                        TorrentSource::Magnet(m) => parse_magnet(m).and_then(|m| m.display_name),
                        _ => None,
                    })
                    .unwrap_or_else(|| r.info_hash.as_string());
                Ok(TorrentPreview {
                    info_hash: r.info_hash.as_string(),
                    name,
                    files,
                    total_bytes: total,
                    torrent_bytes: r.torrent_bytes.to_vec(),
                })
            }
            AddTorrentResponse::AlreadyManaged(_, h) | AddTorrentResponse::Added(_, h) => {
                // list_only never adds; treat an already-managed torrent as previewable.
                let files = Self::files_of(&h)?;
                let total = files.iter().filter(|f| !f.padding).map(|f| f.len).sum();
                Ok(TorrentPreview {
                    info_hash: h.info_hash().as_string(),
                    name: h.name().unwrap_or_default(),
                    files,
                    total_bytes: total,
                    torrent_bytes: Vec::new(),
                })
            }
        }
    }

    /// Adds a transfer writing into `output_folder` (an absolute, already
    /// validated directory). `only_files` restricts the download to the given
    /// file indices.
    pub async fn add(
        &self,
        source: &TorrentSource,
        output_folder: &Path,
        only_files: Option<Vec<usize>>,
        paused: bool,
    ) -> Result<AddedTorrent> {
        let add = source.to_add()?;
        let folder = output_folder
            .to_str()
            .ok_or_else(|| TorrentError::InvalidSource("output folder is not valid UTF-8".into()))?
            .to_string();
        let opts = AddTorrentOptions {
            paused,
            only_files,
            overwrite: true,
            output_folder: Some(folder),
            ..Default::default()
        };
        let resp = self
            .session
            .add_torrent(add, Some(opts))
            .await
            .map_err(eng)?;
        match resp {
            AddTorrentResponse::Added(id, h) => Ok(AddedTorrent {
                info_hash: h.info_hash().as_string(),
                engine_id: id,
                name: h.name(),
                already_managed: false,
            }),
            AddTorrentResponse::AlreadyManaged(id, h) => Ok(AddedTorrent {
                info_hash: h.info_hash().as_string(),
                engine_id: id,
                name: h.name(),
                already_managed: true,
            }),
            AddTorrentResponse::ListOnly(_) => Err(TorrentError::Engine(
                "engine returned list-only response".into(),
            )),
        }
    }

    fn handle(&self, info_hash: &str) -> Result<Arc<ManagedTorrent>> {
        let id = parse_id(info_hash)?;
        self.session
            .get(TorrentIdOrHash::Hash(id))
            .ok_or_else(|| TorrentError::NotFound(info_hash.to_string()))
    }

    pub fn contains(&self, info_hash: &str) -> bool {
        self.handle(info_hash).is_ok()
    }

    /// Info hashes of all torrents managed by the session.
    pub fn managed(&self) -> Vec<String> {
        self.session
            .with_torrents(|it| it.map(|(_, h)| h.info_hash().as_string()).collect())
    }

    pub fn stats(&self, info_hash: &str) -> Result<EngineStats> {
        let h = self.handle(info_hash)?;
        let s = h.stats();
        let state = match s.state {
            TorrentStatsState::Initializing { .. } => TorrentState::Initializing,
            TorrentStatsState::Paused => {
                if s.finished {
                    TorrentState::Completed
                } else {
                    TorrentState::Paused
                }
            }
            TorrentStatsState::Error => TorrentState::Error,
            TorrentStatsState::Live => {
                if s.finished {
                    TorrentState::Seeding
                } else {
                    TorrentState::Downloading
                }
            }
        };
        let (down, up, eta) = match &s.live {
            Some(l) => (
                l.download_speed.mbps * 1024.0 * 1024.0,
                l.upload_speed.mbps * 1024.0 * 1024.0,
                l.time_remaining
                    .as_ref()
                    .and_then(|d| serde_json::to_value(d).ok())
                    .and_then(eta_from_json),
            ),
            None => (0.0, 0.0, None),
        };
        Ok(EngineStats {
            state,
            progress_bytes: s.progress_bytes,
            total_bytes: s.total_bytes,
            uploaded_bytes: s.uploaded_bytes,
            finished: s.finished,
            download_bps: down,
            upload_bps: up,
            eta_secs: eta,
            error: s.error.clone(),
            file_progress: s.file_progress.clone(),
        })
    }

    fn files_of(h: &Arc<ManagedTorrent>) -> Result<Vec<PreviewFile>> {
        h.with_metadata(|m| {
            m.file_infos
                .iter()
                .enumerate()
                .map(|(index, f)| PreviewFile {
                    index,
                    path: f
                        .relative_filename
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("/"),
                    len: f.len,
                    padding: f.attrs.padding,
                })
                .collect()
        })
        .map_err(eng)
    }

    pub fn files(&self, info_hash: &str) -> Result<Vec<PreviewFile>> {
        Self::files_of(&self.handle(info_hash)?)
    }

    /// The on-disk folder the transfer writes into.
    pub fn output_folder(&self, info_hash: &str) -> Result<PathBuf> {
        Ok(self.handle(info_hash)?.output_folder().to_path_buf())
    }

    pub async fn pause(&self, info_hash: &str) -> Result<()> {
        let h = self.handle(info_hash)?;
        self.session.pause(&h).await.map_err(eng)
    }

    pub async fn resume(&self, info_hash: &str) -> Result<()> {
        let h = self.handle(info_hash)?;
        self.session.unpause(&h).await.map_err(eng)
    }

    pub async fn set_only_files(&self, info_hash: &str, files: &[usize]) -> Result<()> {
        let h = self.handle(info_hash)?;
        let set: HashSet<usize> = files.iter().copied().collect();
        self.session.update_only_files(&h, &set).await.map_err(eng)
    }

    /// Removes the transfer from the engine. Downloaded files are deleted only
    /// when `delete_files` is set (the caller must have obtained explicit consent).
    pub async fn remove(&self, info_hash: &str, delete_files: bool) -> Result<()> {
        let id = parse_id(info_hash)?;
        self.session
            .delete(TorrentIdOrHash::Hash(id), delete_files)
            .await
            .map_err(eng)
    }

    pub async fn shutdown(&self) {
        self.session.stop().await;
    }
}

/// `DurationWithHumanReadable` serializes as `{"duration": {"secs":..,"nanos":..}, "human_readable": ".."}`.
fn eta_from_json(v: serde_json::Value) -> Option<u64> {
    v.get("duration")
        .and_then(|d| d.get("secs"))
        .and_then(|s| s.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_sources() {
        assert!(TorrentSource::Magnet("http://x".into()).to_add().is_err());
        assert!(
            TorrentSource::TorrentFile(b"<html>".to_vec())
                .to_add()
                .is_err()
        );
        assert!(
            TorrentSource::TorrentFile(b"d4:infod".to_vec())
                .to_add()
                .is_ok()
        );
    }

    #[test]
    fn stats_fraction() {
        let s = EngineStats {
            state: TorrentState::Downloading,
            progress_bytes: 50,
            total_bytes: 200,
            uploaded_bytes: 0,
            finished: false,
            download_bps: 0.0,
            upload_bps: 0.0,
            eta_secs: None,
            error: None,
            file_progress: vec![],
        };
        assert!((s.fraction() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn eta_json_shape() {
        let v = serde_json::json!({"duration": {"secs": 42, "nanos": 0}, "human_readable": "42s"});
        assert_eq!(eta_from_json(v), Some(42));
    }
}
