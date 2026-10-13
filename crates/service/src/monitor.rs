//! Volume detection: matches mounted volumes to catalog storages by identity
//! (marker UUID > platform id > serial+capacity, never drive letter), keeps
//! online/offline state current and registers new library folders.

use std::path::{Path, PathBuf};

use audiovault_core::model::StorageClass;
use audiovault_core::paths::to_db_rel;
use audiovault_db::volumes::{self, ObservedVolume, VolumeIdentity, VolumeRecord};
use audiovault_db::{Db, DbError};
use audiovault_platform::marker;
use audiovault_platform::volumes::{
    MediaClass, VolumeEnumerator, VolumeInfo, normalize_for_compare,
};

use crate::{Result, ServiceError};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct MonitorChange {
    pub came_online: Vec<i64>,
    pub went_offline: Vec<i64>,
}

impl MonitorChange {
    pub fn is_empty(&self) -> bool {
        self.came_online.is_empty() && self.went_offline.is_empty()
    }
}

pub fn storage_class(m: MediaClass) -> StorageClass {
    match m {
        MediaClass::Ssd => StorageClass::Ssd,
        MediaClass::Hdd => StorageClass::Hdd,
        MediaClass::Removable => StorageClass::Removable,
        MediaClass::Network => StorageClass::Network,
        MediaClass::Unknown => StorageClass::Unknown,
    }
}

pub fn observe(info: &VolumeInfo) -> ObservedVolume {
    let marker_uuid = marker::read_marker(&info.mount_path).map(|m| m.uuid);
    ObservedVolume {
        identity: VolumeIdentity {
            platform_id: info.platform_id.clone(),
            serial: info.serial.clone(),
            marker_uuid,
        },
        label: info.label.clone(),
        fs_type: info.fs_type.clone(),
        capacity_bytes: info.capacity_bytes,
        free_bytes: info.free_bytes,
        mount_path: info.mount_path.to_string_lossy().into_owned(),
        storage_class: storage_class(info.media),
    }
}

/// One detection pass. Never removes catalog data.
pub fn refresh(db: &Db, enumerator: &dyn VolumeEnumerator) -> Result<MonitorChange> {
    let mounted = enumerator.list();
    let observed: Vec<ObservedVolume> = mounted.iter().map(observe).collect();
    db.write(|tx| {
        let mut change = MonitorChange::default();
        let mut online = Vec::new();
        for obs in &observed {
            if let Some(rec) = volumes::find_by_identity(tx, &obs.identity, obs.capacity_bytes)? {
                if !rec.is_online {
                    change.came_online.push(rec.id);
                }
                volumes::mark_seen(tx, rec.id, obs)?;
                online.push(rec.id);
            }
        }
        change.went_offline = volumes::set_offline_except(tx, &online)?;
        Ok(change)
    })
    .map_err(ServiceError::from)
}

/// Mount path of an online volume, verified to exist right now.
pub fn mount_of(rec: &VolumeRecord) -> Result<PathBuf> {
    if !rec.is_online {
        return Err(ServiceError::Offline(rec.display_name.clone()));
    }
    let p = rec
        .last_mount_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| ServiceError::Offline(rec.display_name.clone()))?;
    if !p.exists() {
        return Err(ServiceError::Offline(rec.display_name.clone()));
    }
    Ok(p)
}

pub fn volume_mount(db: &Db, volume_id: i64) -> Result<(VolumeRecord, PathBuf)> {
    let rec = {
        let conn = db.conn()?;
        volumes::get(&conn, volume_id)?
            .ok_or_else(|| ServiceError::NotFound(format!("storage {volume_id}")))?
    };
    let m = mount_of(&rec)?;
    Ok((rec, m))
}

/// Relative path of `path` under `mount` in catalog form, case-insensitive on
/// Windows/macOS.
pub fn relative_to_mount(mount: &Path, path: &Path) -> Option<String> {
    if let Ok(rel) = path.strip_prefix(mount) {
        return Some(to_db_rel(rel));
    }
    let m = normalize_for_compare(mount);
    let p = normalize_for_compare(path);
    if !p.starts_with(&m) {
        return None;
    }
    // Same prefix ignoring case: cut the original string by component count.
    let skip = mount.components().count();
    let rel: PathBuf = path.components().skip(skip).collect();
    Some(to_db_rel(&rel))
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegisteredFolder {
    pub volume_id: i64,
    pub root_id: i64,
    pub created_volume: bool,
}

/// Register `path` as a library folder: identify (or create) its storage and
/// add a library root. Writes the identity marker only if `write_marker`.
pub fn register_folder(
    db: &Db,
    enumerator: &dyn VolumeEnumerator,
    path: &Path,
    write_marker: bool,
) -> Result<RegisteredFolder> {
    if !path.is_dir() {
        return Err(ServiceError::Invalid(format!(
            "{} is not a folder",
            path.display()
        )));
    }
    let info = enumerator.volume_for_path(path).ok_or_else(|| {
        ServiceError::Invalid(format!("{} is not on a mounted storage", path.display()))
    })?;
    if write_marker && !info.read_only {
        marker::write_marker(&info.mount_path)?;
    }
    let obs = observe(&info);
    let rel = relative_to_mount(&info.mount_path, path).ok_or_else(|| {
        ServiceError::Invalid(format!(
            "{} is outside {}",
            path.display(),
            info.mount_path.display()
        ))
    })?;
    if obs.identity.platform_id.is_none()
        && obs.identity.serial.is_none()
        && obs.identity.marker_uuid.is_none()
    {
        return Err(ServiceError::Invalid(
            "this storage reports no stable identity; enable the identity marker to add it".into(),
        ));
    }
    let display = info.display_name();
    db.write(|tx| {
        let (volume_id, created) =
            match volumes::find_by_identity(tx, &obs.identity, obs.capacity_bytes)? {
                Some(rec) => {
                    volumes::mark_seen(tx, rec.id, &obs)?;
                    if let Some(m) = &obs.identity.marker_uuid
                        && rec.marker_uuid.as_deref() != Some(m)
                    {
                        volumes::set_marker(tx, rec.id, m)?;
                    }
                    (rec.id, false)
                }
                None => (volumes::insert(tx, &obs, &display)?, true),
            };
        if write_marker {
            volumes::set_marker_policy(tx, volume_id, "allowed")?;
        }
        let root_id = volumes::add_root(tx, volume_id, &rel)?;
        Ok::<_, DbError>(RegisteredFolder {
            volume_id,
            root_id,
            created_volume: created,
        })
    })
    .map_err(ServiceError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use audiovault_platform::volumes::{StaticVolumes, synthetic_volume};

    #[test]
    fn register_and_track_online_state() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("c.sqlite")).unwrap();
        let disk = dir.path().join("disk");
        std::fs::create_dir_all(disk.join("Books")).unwrap();
        let vols = StaticVolumes::new(vec![synthetic_volume("usb1", &disk)]);
        let reg = register_folder(&db, &vols, &disk.join("Books"), false).unwrap();
        assert!(reg.created_volume);
        let again = register_folder(&db, &vols, &disk.join("Books"), false).unwrap();
        assert_eq!(again.volume_id, reg.volume_id);
        assert_eq!(again.root_id, reg.root_id);

        vols.set(vec![]);
        let ch = refresh(&db, &vols).unwrap();
        assert_eq!(ch.went_offline, vec![reg.volume_id]);
        assert!(volume_mount(&db, reg.volume_id).is_err());

        // Reconnected at a different mount path: same storage by identity.
        let disk2 = dir.path().join("elsewhere");
        std::fs::create_dir_all(&disk2).unwrap();
        vols.set(vec![synthetic_volume("usb1", &disk2)]);
        let ch = refresh(&db, &vols).unwrap();
        assert_eq!(ch.came_online, vec![reg.volume_id]);
        let (_, m) = volume_mount(&db, reg.volume_id).unwrap();
        assert_eq!(m, disk2);
    }

    #[test]
    fn marker_identity_written_only_with_consent() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("c.sqlite")).unwrap();
        let disk = dir.path().join("disk");
        std::fs::create_dir_all(&disk).unwrap();
        let vols = StaticVolumes::new(vec![synthetic_volume("d", &disk)]);
        register_folder(&db, &vols, &disk, false).unwrap();
        assert!(!disk.join(marker::MARKER_FILE).exists());
        register_folder(&db, &vols, &disk, true).unwrap();
        assert!(disk.join(marker::MARKER_FILE).exists());
    }

    #[test]
    fn relative_paths() {
        assert_eq!(
            relative_to_mount(Path::new("/mnt/a"), Path::new("/mnt/a/x/y")).as_deref(),
            Some("x/y")
        );
        assert_eq!(
            relative_to_mount(Path::new("/mnt/a"), Path::new("/mnt/a")).as_deref(),
            Some("")
        );
        assert!(relative_to_mount(Path::new("/mnt/a"), Path::new("/mnt/b")).is_none());
    }
}
