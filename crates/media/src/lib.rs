//! Media infrastructure: probing (lofty + Symphonia fallback), FB2 parsing,
//! BLAKE3 hashing, acoustic fingerprinting, FFmpeg transcoding, cover
//! thumbnails and explicit tag writing.
//!
//! Everything here is synchronous and meant to run on blocking worker threads
//! scheduled by the service layer's storage-aware I/O scheduler. All inputs
//! (files, tags, FB2 XML) are treated as untrusted.

pub mod cover;
pub mod ebook;
pub mod epub;
pub mod fb2;
pub mod fingerprint;
pub mod hash;
pub mod mp4chapters;
pub mod probe;
pub mod tags_write;
pub mod text;
pub mod transcode;

use std::sync::atomic::{AtomicBool, Ordering};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported or unreadable media: {0}")]
    Unsupported(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("cancelled")]
    Cancelled,
    #[error("tool not available: {0}")]
    ToolMissing(String),
    #[error("{0}")]
    Failed(String),
}

pub type Result<T> = std::result::Result<T, MediaError>;

/// Cooperative cancellation flag checked by long-running operations.
pub trait Cancel: Sync {
    fn is_cancelled(&self) -> bool;
}

impl Cancel for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Relaxed)
    }
}

impl<F: Fn() -> bool + Sync> Cancel for F {
    fn is_cancelled(&self) -> bool {
        self()
    }
}

/// A cancel source that never fires.
pub struct NeverCancel;

impl Cancel for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}
