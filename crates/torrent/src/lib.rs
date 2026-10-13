//! BitTorrent support for AudioVault.
//!
//! * [`engine`] wraps the embedded `librqbit` session: previewing torrent
//!   contents, adding transfers with selected files into a validated folder,
//!   pause/resume/remove and stats polling. The engine persists its own resume
//!   state in the application data directory; the catalog database keeps the
//!   link between a transfer and a storage volume (see `audiovault_db::torrents`).
//! * [`provider`] defines the search-provider abstraction and
//!   [`rutracker`] implements it for RuTracker (login with user-supplied
//!   credentials, search, `.torrent` download). CAPTCHA challenges are reported
//!   to the user; they are never bypassed.

pub mod engine;
pub mod magnet;
pub mod provider;
pub mod rutracker;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TorrentError {
    #[error("torrent engine error: {0}")]
    Engine(String),
    #[error("torrent not found in engine: {0}")]
    NotFound(String),
    #[error("invalid torrent source: {0}")]
    InvalidSource(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("not logged in to {0}")]
    NotLoggedIn(String),
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("the site requires a CAPTCHA; open it in a browser, log in once there, then retry")]
    CaptchaRequired,
    #[error(
        "RuTracker требует проверки Cloudflare / капчи. Вставьте значение cookie bb_data из браузера в Настройках."
    )]
    CloudflareChallenge,
    #[error("unexpected response from {site}: {detail}")]
    UnexpectedResponse { site: String, detail: String },
    #[error("operation timed out")]
    Timeout,
}

impl From<reqwest::Error> for TorrentError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() {
            TorrentError::Timeout
        } else {
            // reqwest errors may include the URL; our URLs never contain credentials.
            TorrentError::Network(e.to_string())
        }
    }
}

pub type Result<T> = std::result::Result<T, TorrentError>;
