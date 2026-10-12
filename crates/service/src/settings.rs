//! User settings, stored as one JSON document in the catalog's `app_setting`
//! table (key `app_settings`). Secrets are never part of this struct; the
//! RuTracker password lives in the OS credential store.

use audiovault_core::optimize::OptimizePolicy;
use audiovault_db::{Db, settings};
use serde::{Deserialize, Serialize};

use crate::Result;

pub const KEY: &str = "app_settings";
pub const RUTRACKER_ACCOUNT_PREFIX: &str = "rutracker:";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Explicit FFmpeg executable; empty = bundled sidecar or PATH.
    pub ffmpeg_path: String,
    /// Rescan library roots automatically when their storage comes online.
    pub auto_scan_on_connect: bool,
    /// Compute acoustic fingerprints for ambiguous duplicate candidates.
    pub acoustic_fingerprints: bool,
    /// Max background jobs across all storages.
    pub max_parallel_jobs: usize,
    /// Prevent system sleep while heavy jobs run.
    pub inhibit_sleep: bool,

    pub optimizer_aac_kbps: u32,
    pub optimizer_opus_kbps: u32,

    pub torrent_listen_port: u16,
    pub torrent_enable_dht: bool,
    pub torrent_enable_upnp: bool,
    /// KiB/s, 0 = unlimited.
    pub torrent_download_limit_kib: u32,
    pub torrent_upload_limit_kib: u32,
    /// Default download destination (volume + relative folder).
    pub download_volume_id: Option<i64>,
    pub download_rel_path: String,
    /// Import completed downloads into the library automatically.
    pub auto_import_downloads: bool,

    pub rutracker_base_url: String,
    pub rutracker_username: String,
    /// Comma-separated forum ids to restrict searches to (empty = all).
    pub rutracker_categories: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            theme: "system".into(),
            ffmpeg_path: String::new(),
            auto_scan_on_connect: true,
            acoustic_fingerprints: true,
            max_parallel_jobs: 4,
            inhibit_sleep: true,
            optimizer_aac_kbps: 64,
            optimizer_opus_kbps: 32,
            torrent_listen_port: 0,
            torrent_enable_dht: true,
            torrent_enable_upnp: false,
            torrent_download_limit_kib: 0,
            torrent_upload_limit_kib: 0,
            download_volume_id: None,
            download_rel_path: "Audiobooks/Downloads".into(),
            auto_import_downloads: true,
            rutracker_base_url: audiovault_torrent::rutracker::DEFAULT_BASE_URL.into(),
            rutracker_username: String::new(),
            rutracker_categories: String::new(),
        }
    }
}

impl AppSettings {
    pub fn load(db: &Db) -> Result<AppSettings> {
        let conn = db.conn()?;
        Ok(settings::get_json::<AppSettings>(&conn, KEY)?.unwrap_or_default().sanitized())
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        let conn = db.conn()?;
        settings::set_json(&conn, KEY, &self.clone().sanitized())?;
        Ok(())
    }

    /// Clamp values into supported ranges.
    pub fn sanitized(mut self) -> Self {
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        self.max_parallel_jobs = self.max_parallel_jobs.clamp(1, 16);
        self.optimizer_aac_kbps = self.optimizer_aac_kbps.clamp(24, 256);
        self.optimizer_opus_kbps = self.optimizer_opus_kbps.clamp(16, 192);
        self.download_rel_path = self.download_rel_path.trim().trim_matches(['/', '\\']).replace('\\', "/");
        self
    }

    pub fn optimize_policy(&self) -> OptimizePolicy {
        OptimizePolicy { aac_kbps: self.optimizer_aac_kbps, opus_kbps: self.optimizer_opus_kbps, ..Default::default() }
    }

    pub fn rutracker_categories(&self) -> Vec<String> {
        self.rutracker_categories
            .split([',', ' ', ';'])
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .map(str::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_sanitize() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("c.sqlite")).unwrap();
        assert_eq!(AppSettings::load(&db).unwrap(), AppSettings::default());
        let s = AppSettings {
            theme: "neon".into(),
            max_parallel_jobs: 99,
            download_rel_path: "\\Books\\New\\".into(),
            rutracker_categories: "2387, 2389;x".into(),
            ..Default::default()
        };
        s.save(&db).unwrap();
        let l = AppSettings::load(&db).unwrap();
        assert_eq!(l.theme, "system");
        assert_eq!(l.max_parallel_jobs, 16);
        assert_eq!(l.download_rel_path, "Books/New");
        assert_eq!(l.rutracker_categories(), vec!["2387", "2389"]);
    }

    #[test]
    fn tolerates_unknown_and_missing_fields() {
        let s: AppSettings = serde_json::from_str(r#"{"theme":"dark","future_field":1}"#).unwrap();
        assert_eq!(s.theme, "dark");
        assert!(s.auto_scan_on_connect);
    }
}
