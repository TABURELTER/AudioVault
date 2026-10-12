//! Optional `.audiovault-id` marker file in the root of a storage volume.
//!
//! The marker contains only a random UUID and a format version. It is never
//! written without explicit user consent (per-volume `marker_policy`).

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::Result;

pub const MARKER_FILE: &str = ".audiovault-id";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMarker {
    pub format: String,
    pub version: u32,
    pub uuid: String,
}

/// Read the marker of a mounted volume root (None if absent or malformed).
pub fn read_marker(root: &Path) -> Option<VolumeMarker> {
    let path = root.join(MARKER_FILE);
    let meta = std::fs::metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > 4096 {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let m: VolumeMarker = serde_json::from_str(&text).ok()?;
    (m.format == "audiovault-volume" && uuid::Uuid::parse_str(&m.uuid).is_ok()).then_some(m)
}

/// Create a marker (only called after the user agreed). Existing valid markers are kept.
pub fn write_marker(root: &Path) -> Result<VolumeMarker> {
    if let Some(m) = read_marker(root) {
        return Ok(m);
    }
    let m = VolumeMarker { format: "audiovault-volume".into(), version: 1, uuid: uuid::Uuid::new_v4().to_string() };
    let tmp = root.join(format!("{MARKER_FILE}.tmp"));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(serde_json::to_string_pretty(&m).map_err(|e| crate::PlatformError::Other(e.to_string()))?.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, root.join(MARKER_FILE))?;
    hide(&root.join(MARKER_FILE));
    Ok(m)
}

#[cfg(windows)]
fn hide(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a valid NUL-terminated UTF-16 string that outlives the call.
    unsafe {
        SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
fn hide(_path: &Path) {
    // Dot-files are hidden by convention on Unix.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_marker(dir.path()).is_none());
        let m = write_marker(dir.path()).unwrap();
        assert_eq!(read_marker(dir.path()), Some(m.clone()));
        assert_eq!(write_marker(dir.path()).unwrap(), m, "existing marker reused");
    }

    #[test]
    fn malformed_marker_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(MARKER_FILE), "garbage").unwrap();
        assert!(read_marker(dir.path()).is_none());
    }
}
