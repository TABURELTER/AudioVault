//! Indexing pipeline: filesystem discovery, incremental catalog ingestion,
//! per-copy analysis (probe, tags, FB2, covers, quality, issues, optimizer),
//! content hashing, acoustic fingerprinting and duplicate detection.
//!
//! All functions are synchronous and designed to run on worker threads owned
//! by the service layer's I/O scheduler. Long operations poll a [`JobContext`]
//! for cancellation and report progress through it.

pub mod analyze;
pub mod duplicates;
pub mod hashing;
pub mod scan;
pub mod walk;

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IndexerError {
    #[error(transparent)]
    Db(#[from] audiovault_db::DbError),
    #[error(transparent)]
    Media(#[from] audiovault_media::MediaError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancelled")]
    Cancelled,
    /// The storage disappeared (unplugged, unmounted) during the operation.
    #[error("storage unavailable: {0}")]
    VolumeUnavailable(String),
    #[error("{0}")]
    Invalid(String),
}

impl From<rusqlite::Error> for IndexerError {
    fn from(e: rusqlite::Error) -> Self {
        IndexerError::Db(audiovault_db::DbError::Sqlite(e))
    }
}

pub type Result<T> = std::result::Result<T, IndexerError>;

/// Execution context supplied by the job runner.
pub trait JobContext: Sync {
    fn is_cancelled(&self) -> bool;
    /// `fraction` in 0..=1 when known, negative for indeterminate.
    fn progress(&self, fraction: f64, text: &str);
}

/// Context for tests / one-off calls: never cancelled, ignores progress.
pub struct NoopContext;

impl JobContext for NoopContext {
    fn is_cancelled(&self) -> bool {
        false
    }
    fn progress(&self, _fraction: f64, _text: &str) {}
}

/// Adapts a [`JobContext`] to the media crate's cancellation trait.
pub struct CancelOf<'a>(pub &'a dyn JobContext);

impl audiovault_media::Cancel for CancelOf<'_> {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

pub(crate) fn check(ctx: &dyn JobContext) -> Result<()> {
    if ctx.is_cancelled() { Err(IndexerError::Cancelled) } else { Ok(()) }
}

/// Absolute path of a catalog-relative path under a mount root.
pub fn abs_path(mount: &Path, rel: &str) -> PathBuf {
    audiovault_core::paths::from_db_rel(mount, rel)
}

/// Map a media-layer error to an indexer error, turning "file vanished" into
/// a volume check when the whole mount is gone.
pub(crate) fn media_err(e: audiovault_media::MediaError, mount: &Path) -> IndexerError {
    match e {
        audiovault_media::MediaError::Cancelled => IndexerError::Cancelled,
        other if !mount.exists() => IndexerError::VolumeUnavailable(format!("{} is no longer reachable ({other})", mount.display())),
        other => IndexerError::Media(other),
    }
}
