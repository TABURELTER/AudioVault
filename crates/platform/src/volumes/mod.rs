//! Mounted volume enumeration with stable identity.
//!
//! Identity is never the drive letter or mount point:
//! - Windows: Volume GUID path (`\\?\Volume{…}\`) + volume serial number.
//! - macOS: volume UUID from DiskArbitration + `f_fsid`-independent BSD info.
//! - Linux: filesystem UUID from `/dev/disk/by-uuid`.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// Physical/transport class as far as the OS reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MediaClass {
    Ssd,
    Hdd,
    Removable,
    Network,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeInfo {
    pub mount_path: PathBuf,
    pub label: Option<String>,
    pub fs_type: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    /// Stable platform identity (Volume GUID path / volume UUID / fs UUID).
    pub platform_id: Option<String>,
    /// Secondary identity: volume serial number.
    pub serial: Option<String>,
    pub media: MediaClass,
    /// Connected through USB/Thunderbolt/SD or otherwise external.
    pub is_external: bool,
    pub is_system: bool,
    pub read_only: bool,
}

impl VolumeInfo {
    pub fn display_name(&self) -> String {
        let label = self.label.clone().filter(|l| !l.trim().is_empty());
        match label {
            Some(l) => l,
            None => {
                let p = self.mount_path.display().to_string();
                if self.is_system { format!("System ({p})") } else { format!("Volume ({p})") }
            }
        }
    }
}

/// Source of currently mounted volumes. A trait so the scanner/monitor can be
/// tested with folder-backed fake volumes.
pub trait VolumeEnumerator: Send + Sync {
    fn list(&self) -> Vec<VolumeInfo>;

    /// The mounted volume containing `path` (longest mount-path prefix).
    fn volume_for_path(&self, path: &Path) -> Option<VolumeInfo> {
        let canon = normalize_for_compare(path);
        self.list()
            .into_iter()
            .filter(|v| canon.starts_with(&normalize_for_compare(&v.mount_path)))
            .max_by_key(|v| v.mount_path.as_os_str().len())
    }
}

/// Real OS volumes.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemVolumes;

impl VolumeEnumerator for SystemVolumes {
    fn list(&self) -> Vec<VolumeInfo> {
        #[cfg(windows)]
        {
            windows::list()
        }
        #[cfg(target_os = "macos")]
        {
            macos::list()
        }
        #[cfg(target_os = "linux")]
        {
            linux::list()
        }
        #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
        {
            Vec::new()
        }
    }
}

/// Free/total bytes of the filesystem containing `path` (online check + space display).
pub fn space_for_path(path: &Path) -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        windows::space(path)
    }
    #[cfg(unix)]
    {
        unix_space(path)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        None
    }
}

#[cfg(unix)]
fn unix_space(path: &Path) -> Option<(u64, u64)> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let c = CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `c` is a valid NUL-terminated path and `st` is a properly sized out-parameter.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    if rc != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)]
    let frsize = st.f_frsize as u64;
    #[allow(clippy::unnecessary_cast)]
    Some((st.f_bavail as u64 * frsize, st.f_blocks as u64 * frsize))
}

/// Case-insensitive comparison form on Windows/macOS, exact elsewhere.
pub fn normalize_for_compare(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(any(windows, target_os = "macos")) { s.to_lowercase() + "/" } else { s + "/" }
}

/// A mutable list of volumes (tests, folder-backed "mock" storages).
#[derive(Debug, Default, Clone)]
pub struct StaticVolumes(std::sync::Arc<parking_lot::RwLock<Vec<VolumeInfo>>>);

impl StaticVolumes {
    pub fn new(vols: Vec<VolumeInfo>) -> Self {
        StaticVolumes(std::sync::Arc::new(parking_lot::RwLock::new(vols)))
    }
    pub fn set(&self, vols: Vec<VolumeInfo>) {
        *self.0.write() = vols;
    }
}

impl VolumeEnumerator for StaticVolumes {
    fn list(&self) -> Vec<VolumeInfo> {
        self.0.read().clone()
    }
}

/// Folder-backed fake volume (tests and "add folder" on unidentifiable mounts).
pub fn synthetic_volume(id: &str, mount: &Path) -> VolumeInfo {
    VolumeInfo {
        mount_path: mount.to_path_buf(),
        label: Some(id.to_string()),
        fs_type: None,
        capacity_bytes: None,
        free_bytes: None,
        platform_id: Some(format!("synthetic:{id}")),
        serial: None,
        media: MediaClass::Unknown,
        is_external: true,
        is_system: false,
        read_only: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_volumes_include_a_root() {
        let vols = SystemVolumes.list();
        assert!(!vols.is_empty(), "at least the system volume is mounted");
        let exe = std::env::current_exe().unwrap();
        let v = SystemVolumes.volume_for_path(&exe).expect("exe lives on a mounted volume");
        assert!(v.capacity_bytes.unwrap_or(0) > 0);
        assert!(v.platform_id.is_some(), "stable identity available: {v:?}");
    }

    #[test]
    fn longest_prefix_wins() {
        let a = synthetic_volume("a", Path::new("/mnt"));
        let b = synthetic_volume("b", Path::new("/mnt/disk2"));
        let s = StaticVolumes::new(vec![a, b]);
        assert_eq!(s.volume_for_path(Path::new("/mnt/disk2/books/x")).unwrap().label.as_deref(), Some("b"));
        assert_eq!(s.volume_for_path(Path::new("/mnt/disk")).unwrap().label.as_deref(), Some("a"));
        assert!(s.volume_for_path(Path::new("/other")).is_none());
    }
}
