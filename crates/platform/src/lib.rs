//! Platform abstraction layer.
//!
//! Everything OS-specific lives here behind small, explicit APIs: volume
//! enumeration with stable identity, application paths (incl. portable mode),
//! volume marker files, reveal/open/trash, the OS credential store, sleep
//! inhibition and single-instance enforcement.

pub mod credentials;
pub mod instance;
pub mod marker;
pub mod paths;
pub mod power;
pub mod shell;
pub mod volumes;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("credential store: {0}")]
    Credentials(String),
    #[error("trash: {0}")]
    Trash(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PlatformError>;
