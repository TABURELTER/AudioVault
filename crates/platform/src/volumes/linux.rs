//! Linux volume enumeration (basic): `/proc/self/mounts` + `/dev/disk/by-uuid`.
//! Linux is not a first-class target yet; this keeps the domain portable.

use std::collections::HashMap;
use std::path::PathBuf;

use super::{MediaClass, VolumeInfo};

fn unescape(s: &str) -> String {
    // /proc/mounts escapes spaces etc. as \040 octal sequences.
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    let mut bytes = Vec::new();
    while i < b.len() {
        if b[i] == b'\\' && i + 3 < b.len() && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)) {
            let v = (b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0');
            bytes.push(v);
            i += 4;
        } else {
            bytes.push(b[i]);
            i += 1;
        }
    }
    out.push_str(&String::from_utf8_lossy(&bytes));
    out
}

pub fn list() -> Vec<VolumeInfo> {
    let mut uuids: HashMap<PathBuf, String> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir("/dev/disk/by-uuid") {
        for e in rd.flatten() {
            if let Ok(target) = std::fs::canonicalize(e.path()) {
                uuids.insert(target, e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else { return Vec::new() };
    let mut out = Vec::new();
    for line in text.lines() {
        let mut f = line.split_whitespace();
        let (Some(dev), Some(mnt), Some(fs), Some(opts)) = (f.next(), f.next(), f.next(), f.next()) else { continue };
        let network = matches!(fs, "nfs" | "nfs4" | "cifs" | "smb3" | "sshfs" | "fuse.sshfs");
        if !(dev.starts_with("/dev/") || network) || dev.starts_with("/dev/loop") {
            continue;
        }
        let mount = unescape(mnt);
        let path = PathBuf::from(&mount);
        let dev_path = std::fs::canonicalize(dev).unwrap_or_else(|_| PathBuf::from(dev));
        let (free, total) = super::space_for_path(&path).unzip();
        let removable = mount.starts_with("/media/") || mount.starts_with("/run/media/");
        out.push(VolumeInfo {
            label: path.file_name().map(|s| s.to_string_lossy().into_owned()),
            mount_path: path,
            fs_type: Some(fs.to_string()),
            capacity_bytes: total,
            free_bytes: free,
            platform_id: uuids.get(&dev_path).cloned().or_else(|| network.then(|| format!("net:{dev}"))),
            serial: None,
            media: if network { MediaClass::Network } else if removable { MediaClass::Removable } else { MediaClass::Unknown },
            is_external: removable,
            is_system: mount == "/",
            read_only: opts.split(',').any(|o| o == "ro"),
        });
    }
    out
}
