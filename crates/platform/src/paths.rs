//! OS-standard application directories, with an optional portable mode.
//!
//! Portable mode is enabled when a file named `audiovault.portable` exists
//! next to the executable; all data then lives in `<exe dir>/AudioVaultData`.
//! `AUDIOVAULT_DATA_DIR` overrides the base directory (development/tests).

use std::path::{Path, PathBuf};

use crate::{PlatformError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub portable: bool,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub cover_cache_dir: PathBuf,
    pub temp_dir: PathBuf,
    /// Helper binaries copied/extracted by the app (e.g. bundled FFmpeg sidecar).
    pub bin_dir: PathBuf,
    pub torrent_state_dir: PathBuf,
}

pub const PORTABLE_MARKER: &str = "audiovault.portable";

impl AppPaths {
    pub fn resolve() -> Result<AppPaths> {
        if let Some(base) = std::env::var_os("AUDIOVAULT_DATA_DIR") {
            return Ok(Self::under(Path::new(&base), true));
        }
        if let Some(exe_dir) = exe_dir()
            && exe_dir.join(PORTABLE_MARKER).is_file()
        {
            return Ok(Self::under(&exe_dir.join("AudioVaultData"), true));
        }
        let dirs = directories::ProjectDirs::from("app", "AudioVault", "AudioVault")
            .ok_or_else(|| PlatformError::Other("cannot determine the user's home directory".into()))?;
        let data = dirs.data_local_dir().to_path_buf();
        let cache = dirs.cache_dir().to_path_buf();
        Ok(AppPaths {
            portable: false,
            config_dir: dirs.config_dir().to_path_buf(),
            log_dir: data.join("logs"),
            cover_cache_dir: cache.join("covers"),
            temp_dir: cache.join("tmp"),
            bin_dir: data.join("bin"),
            torrent_state_dir: data.join("torrents"),
            data_dir: data,
            cache_dir: cache,
        })
    }

    /// All directories under a single base (portable mode and tests).
    pub fn under(base: &Path, portable: bool) -> AppPaths {
        AppPaths {
            portable,
            config_dir: base.join("config"),
            data_dir: base.join("data"),
            log_dir: base.join("logs"),
            cache_dir: base.join("cache"),
            cover_cache_dir: base.join("cache").join("covers"),
            temp_dir: base.join("cache").join("tmp"),
            bin_dir: base.join("bin"),
            torrent_state_dir: base.join("data").join("torrents"),
        }
    }

    pub fn ensure(&self) -> Result<()> {
        for d in [
            &self.config_dir,
            &self.data_dir,
            &self.log_dir,
            &self.cache_dir,
            &self.cover_cache_dir,
            &self.temp_dir,
            &self.bin_dir,
            &self.torrent_state_dir,
        ] {
            std::fs::create_dir_all(d)?;
        }
        Ok(())
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.data_dir.join("catalog.sqlite")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.data_dir.join("instance.lock")
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
}

pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Directory with resources bundled next to / inside the application
/// (Windows: exe dir; macOS: `Contents/Resources` of the `.app`).
pub fn bundled_resource_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = exe_dir() {
        #[cfg(target_os = "macos")]
        if let Some(contents) = dir.parent() {
            out.push(contents.join("Resources"));
            out.push(contents.join("Helpers"));
        }
        out.push(dir.join("helpers"));
        out.push(dir);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_layout_is_self_contained() {
        let p = AppPaths::under(Path::new("/x/AudioVaultData"), true);
        for d in [&p.config_dir, &p.data_dir, &p.log_dir, &p.cover_cache_dir, &p.temp_dir, &p.bin_dir] {
            assert!(d.starts_with("/x/AudioVaultData"));
        }
        assert!(p.catalog_path().ends_with("catalog.sqlite"));
    }

    #[test]
    fn system_paths_resolve() {
        if std::env::var_os("AUDIOVAULT_DATA_DIR").is_none() {
            let p = AppPaths::resolve().unwrap();
            assert_ne!(p.data_dir, p.cache_dir);
        }
    }
}
