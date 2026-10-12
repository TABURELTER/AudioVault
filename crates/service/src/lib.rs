//! Application layer of AudioVault.
//!
//! * [`app::AppService`] — the facade used by the UI: opens the catalog,
//!   performs startup recovery, owns the job runner, volume monitor and
//!   torrent service, and exposes queries/actions.
//! * [`runner`] — persistent background job runner. Jobs live in SQLite and
//!   survive restarts; dispatch goes through the storage-aware
//!   [`scheduler::IoScheduler`].
//! * [`monitor`] — volume detection (online/offline, identity matching).
//! * [`fileops`] — journaled, confirmation-gated filesystem operations.
//! * [`torrents`] — embedded BitTorrent engine + RuTracker integration and
//!   import of completed downloads into the library.

pub mod app;
pub mod events;
pub mod exec;
pub mod fileops;
pub mod monitor;
pub mod runner;
pub mod scheduler;
pub mod settings;
pub mod torrents;
pub mod transcode;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error(transparent)]
    Db(#[from] audiovault_db::DbError),
    #[error(transparent)]
    Indexer(#[from] audiovault_indexer::IndexerError),
    #[error(transparent)]
    Media(#[from] audiovault_media::MediaError),
    #[error(transparent)]
    Platform(#[from] audiovault_platform::PlatformError),
    #[error(transparent)]
    Torrent(#[from] audiovault_torrent::TorrentError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("storage \"{0}\" is offline")]
    Offline(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<rusqlite::Error> for ServiceError {
    fn from(e: rusqlite::Error) -> Self {
        ServiceError::Db(audiovault_db::DbError::Sqlite(e))
    }
}

pub type Result<T> = std::result::Result<T, ServiceError>;

pub use app::AppService;
pub use events::AppEvent;
