//! Storage volumes and library roots.
//!
//! A volume is identified by (in order of trust): the `.audiovault-id` marker
//! UUID, the platform volume identity (Windows Volume GUID / macOS volume UUID),
//! then serial number + capacity. Mount paths and drive letters are *never*
//! used as identity — only remembered as the last known location.

use audiovault_core::model::StorageClass;
use rusqlite::{Connection, Row, params};
use serde::Serialize;

use crate::{OptionalExt, Result, new_uuid, now_ts};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct VolumeIdentity {
    pub platform_id: Option<String>,
    pub serial: Option<String>,
    pub marker_uuid: Option<String>,
}

/// A volume as currently observed by the platform layer.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ObservedVolume {
    pub identity: VolumeIdentity,
    pub label: Option<String>,
    pub fs_type: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub mount_path: String,
    pub storage_class: StorageClass,
}

impl ObservedVolume {
    /// A volume with a synthetic identity (tests, seed tool, folder-backed mocks).
    pub fn synthetic(label: &str, mount_path: &str) -> Self {
        ObservedVolume {
            identity: VolumeIdentity {
                platform_id: Some(format!("synthetic:{label}")),
                serial: None,
                marker_uuid: None,
            },
            label: Some(label.to_string()),
            fs_type: None,
            capacity_bytes: None,
            free_bytes: None,
            mount_path: mount_path.to_string(),
            storage_class: StorageClass::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VolumeRecord {
    pub id: i64,
    pub uuid: String,
    pub display_name: String,
    pub volume_label: Option<String>,
    pub fs_type: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub platform_id: Option<String>,
    pub serial: Option<String>,
    pub marker_uuid: Option<String>,
    pub last_mount_path: Option<String>,
    pub storage_class: StorageClass,
    pub is_online: bool,
    pub first_indexed_at: Option<i64>,
    pub last_seen_at: Option<i64>,
    pub last_scanned_at: Option<i64>,
    pub last_error: Option<String>,
    pub marker_policy: String,
}

const COLS: &str = "id, uuid, display_name, volume_label, fs_type, capacity_bytes, free_bytes, platform_id, serial, \
    marker_uuid, last_mount_path, storage_class, is_online, first_indexed_at, last_seen_at, last_scanned_at, last_error, marker_policy";

fn row_to_volume(r: &Row<'_>) -> rusqlite::Result<VolumeRecord> {
    Ok(VolumeRecord {
        id: r.get(0)?,
        uuid: r.get(1)?,
        display_name: r.get(2)?,
        volume_label: r.get(3)?,
        fs_type: r.get(4)?,
        capacity_bytes: r.get(5)?,
        free_bytes: r.get(6)?,
        platform_id: r.get(7)?,
        serial: r.get(8)?,
        marker_uuid: r.get(9)?,
        last_mount_path: r.get(10)?,
        storage_class: StorageClass::parse(&r.get::<_, String>(11)?)
            .unwrap_or(StorageClass::Unknown),
        is_online: r.get::<_, i64>(12)? != 0,
        first_indexed_at: r.get(13)?,
        last_seen_at: r.get(14)?,
        last_scanned_at: r.get(15)?,
        last_error: r.get(16)?,
        marker_policy: r.get(17)?,
    })
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<VolumeRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM storage_volume WHERE id = ?1"),
            [id],
            row_to_volume,
        )
        .optional()?)
}

pub fn all(conn: &Connection) -> Result<Vec<VolumeRecord>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM storage_volume ORDER BY display_name COLLATE NOCASE"
    ))?;
    let rows = st
        .query_map([], row_to_volume)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Find a known volume matching the observed identity.
pub fn find_by_identity(
    conn: &Connection,
    ident: &VolumeIdentity,
    capacity: Option<u64>,
) -> Result<Option<VolumeRecord>> {
    if let Some(m) = ident
        .marker_uuid
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        && let Some(v) = conn
            .query_row(
                &format!("SELECT {COLS} FROM storage_volume WHERE marker_uuid = ?1 COLLATE NOCASE"),
                [m.trim()],
                row_to_volume,
            )
            .optional()?
    {
        return Ok(Some(v));
    }
    if let Some(p) = ident
        .platform_id
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        let clean_p = p.trim().trim_end_matches(['\\', '/']);
        if let Some(v) = conn
            .query_row(
                &format!("SELECT {COLS} FROM storage_volume WHERE rtrim(platform_id, '\\/') = ?1 COLLATE NOCASE OR platform_id = ?2 COLLATE NOCASE"),
                [clean_p, p.trim()],
                row_to_volume,
            )
            .optional()?
        {
            return Ok(Some(v));
        }
    }
    if let Some(s) = ident.serial.as_deref().filter(|s| !s.trim().is_empty()) {
        // Serial numbers can collide (cloned disks); require matching capacity too.
        let mut st = conn.prepare(&format!(
            "SELECT {COLS} FROM storage_volume WHERE serial = ?1 COLLATE NOCASE"
        ))?;
        let cands = st
            .query_map([s.trim()], row_to_volume)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for c in cands {
            let cap_ok = match (c.capacity_bytes, capacity) {
                (Some(a), Some(b)) => a.abs_diff(b) <= a / 1000,
                _ => true,
            };
            if cap_ok {
                return Ok(Some(c));
            }
        }
    }
    Ok(None)
}

pub fn insert(conn: &Connection, obs: &ObservedVolume, display_name: &str) -> Result<i64> {
    let now = now_ts();
    conn.execute(
        "INSERT INTO storage_volume (uuid, display_name, volume_label, fs_type, capacity_bytes, free_bytes, platform_id, serial,
            marker_uuid, last_mount_path, storage_class, is_online, first_indexed_at, last_seen_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 1, NULL, ?12, ?12)",
        params![
            new_uuid(),
            display_name,
            obs.label,
            obs.fs_type,
            obs.capacity_bytes,
            obs.free_bytes,
            obs.identity.platform_id,
            obs.identity.serial,
            obs.identity.marker_uuid,
            obs.mount_path,
            obs.storage_class.as_str(),
            now
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Record that a known volume is currently mounted (possibly at a new path).
pub fn mark_seen(conn: &Connection, id: i64, obs: &ObservedVolume) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET is_online = 1, last_seen_at = ?2, last_mount_path = ?3, free_bytes = ?4,
            capacity_bytes = COALESCE(?5, capacity_bytes), volume_label = COALESCE(?6, volume_label),
            fs_type = COALESCE(?7, fs_type),
            platform_id = COALESCE(platform_id, ?8), serial = COALESCE(serial, ?9),
            storage_class = CASE WHEN storage_class = 'unknown' THEN ?10 ELSE storage_class END,
            marker_uuid = COALESCE(marker_uuid, ?11),
            last_error = NULL
         WHERE id = ?1",
        params![
            id,
            now_ts(),
            obs.mount_path,
            obs.free_bytes,
            obs.capacity_bytes,
            obs.label,
            obs.fs_type,
            obs.identity.platform_id,
            obs.identity.serial,
            obs.storage_class.as_str(),
            obs.identity.marker_uuid,
        ],
    )?;
    Ok(())
}

/// Mark every volume not in `online_ids` offline. Returns ids that changed.
/// Catalog entries are never removed because a volume went offline.
pub fn set_offline_except(conn: &Connection, online_ids: &[i64]) -> Result<Vec<i64>> {
    let mut st = conn.prepare(
        "SELECT id FROM storage_volume WHERE is_online = 1 AND id NOT IN (SELECT value FROM json_each(?1))",
    )?;
    let ids: Vec<i64> = st
        .query_map([crate::ids_json(online_ids)], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in &ids {
        conn.execute(
            "UPDATE storage_volume SET is_online = 0 WHERE id = ?1",
            [id],
        )?;
    }
    Ok(ids)
}

pub fn set_online(conn: &Connection, id: i64, online: bool) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET is_online = ?2, last_seen_at = CASE WHEN ?2 = 1 THEN ?3 ELSE last_seen_at END WHERE id = ?1",
        params![id, online as i64, now_ts()],
    )?;
    Ok(())
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET display_name = ?2 WHERE id = ?1",
        params![id, name.trim()],
    )?;
    Ok(())
}

pub fn set_marker(conn: &Connection, id: i64, marker: &str) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET marker_uuid = ?2 WHERE id = ?1",
        params![id, marker],
    )?;
    Ok(())
}

pub fn set_marker_policy(conn: &Connection, id: i64, policy: &str) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET marker_policy = ?2 WHERE id = ?1",
        params![id, policy],
    )?;
    Ok(())
}

pub fn set_storage_class(conn: &Connection, id: i64, class: StorageClass) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET storage_class = ?2 WHERE id = ?1",
        params![id, class.as_str()],
    )?;
    Ok(())
}

pub fn set_error(conn: &Connection, id: i64, err: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE storage_volume SET last_error = ?2 WHERE id = ?1",
        params![id, err],
    )?;
    Ok(())
}

pub fn mark_scanned(conn: &Connection, id: i64) -> Result<()> {
    let now = now_ts();
    conn.execute(
        "UPDATE storage_volume SET last_scanned_at = ?2, first_indexed_at = COALESCE(first_indexed_at, ?2) WHERE id = ?1",
        params![id, now],
    )?;
    Ok(())
}

/// Remove a volume and all its catalog entries. Explicit user action only.
pub fn delete_volume(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM storage_volume WHERE id = ?1", [id])?;
    crate::catalog::delete_orphan_books(conn)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibraryRoot {
    pub id: i64,
    pub volume_id: i64,
    pub rel_path: String,
    pub enabled: bool,
    pub last_scan_at: Option<i64>,
}

pub fn add_root(conn: &Connection, volume_id: i64, rel_path: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO library_root (volume_id, rel_path, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(volume_id, rel_path) DO UPDATE SET enabled = 1",
        params![volume_id, rel_path, now_ts()],
    )?;
    Ok(conn.query_row(
        "SELECT id FROM library_root WHERE volume_id = ?1 AND rel_path = ?2",
        params![volume_id, rel_path],
        |r| r.get(0),
    )?)
}

pub fn roots(conn: &Connection, volume_id: Option<i64>) -> Result<Vec<LibraryRoot>> {
    let mut st = conn.prepare(
        "SELECT id, volume_id, rel_path, enabled, last_scan_at FROM library_root
         WHERE (?1 IS NULL OR volume_id = ?1) ORDER BY volume_id, rel_path",
    )?;
    let rows = st
        .query_map([volume_id], |r| {
            Ok(LibraryRoot {
                id: r.get(0)?,
                volume_id: r.get(1)?,
                rel_path: r.get(2)?,
                enabled: r.get::<_, i64>(3)? != 0,
                last_scan_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_root(conn: &Connection, root_id: i64) -> Result<Option<LibraryRoot>> {
    Ok(conn
        .query_row(
            "SELECT id, volume_id, rel_path, enabled, last_scan_at FROM library_root WHERE id = ?1",
            [root_id],
            |r| {
                Ok(LibraryRoot {
                    id: r.get(0)?,
                    volume_id: r.get(1)?,
                    rel_path: r.get(2)?,
                    enabled: r.get::<_, i64>(3)? != 0,
                    last_scan_at: r.get(4)?,
                })
            },
        )
        .optional()?)
}

pub fn remove_root(conn: &Connection, root_id: i64) -> Result<()> {
    conn.execute("DELETE FROM library_root WHERE id = ?1", [root_id])?;
    Ok(())
}

pub fn mark_root_scanned(conn: &Connection, root_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE library_root SET last_scan_at = ?2 WHERE id = ?1",
        params![root_id, now_ts()],
    )?;
    Ok(())
}

/// Volume with aggregate catalog statistics for the Storages view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VolumeSummary {
    pub volume: VolumeRecord,
    pub book_count: i64,
    pub copy_count: i64,
    pub missing_copy_count: i64,
    pub indexed_bytes: u64,
    pub root_count: i64,
    pub active_jobs: i64,
    pub potential_savings: u64,
    pub scan_errors: i64,
}

pub fn summaries(conn: &Connection) -> Result<Vec<VolumeSummary>> {
    let vols = all(conn)?;
    // Aggregate in one pass per table (no N+1).
    let mut stats: std::collections::HashMap<i64, (i64, i64, i64, u64)> =
        std::collections::HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT volume_id, COUNT(DISTINCT CASE WHEN state='present' THEN book_id END),
                    SUM(state='present'), SUM(state='missing'), COALESCE(SUM(CASE WHEN state='present' THEN total_size END),0)
             FROM book_copy GROUP BY volume_id",
        )?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?),
            ))
        })?;
        for row in rows {
            let (id, s) = row?;
            stats.insert(id, s);
        }
    }
    let mut roots_n = std::collections::HashMap::new();
    {
        let mut st =
            conn.prepare("SELECT volume_id, COUNT(*) FROM library_root GROUP BY volume_id")?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (k, v) = row?;
            roots_n.insert(k, v);
        }
    }
    let mut jobs_n = std::collections::HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT volume_id, COUNT(*) FROM background_job WHERE state IN ('queued','running','paused') AND volume_id IS NOT NULL GROUP BY volume_id",
        )?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (k, v) = row?;
            jobs_n.insert(k, v);
        }
    }
    let mut savings = std::collections::HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT c.volume_id, SUM(o.current_size - o.estimated_size) FROM optimization_candidate o
             JOIN book_copy c ON c.id = o.copy_id
             WHERE o.state = 'open' AND o.id = (SELECT o2.id FROM optimization_candidate o2 WHERE o2.copy_id = o.copy_id AND o2.state='open'
                                                ORDER BY (o2.current_size - o2.estimated_size) DESC LIMIT 1)
             GROUP BY c.volume_id",
        )?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (k, v) = row?;
            savings.insert(k, v.max(0) as u64);
        }
    }
    let mut errs = std::collections::HashMap::new();
    {
        let mut st = conn.prepare(
            "SELECT s.volume_id, COUNT(e.id) FROM scan_run s JOIN scan_error e ON e.scan_id = s.id
             WHERE s.id IN (SELECT MAX(id) FROM scan_run GROUP BY volume_id) GROUP BY s.volume_id",
        )?;
        for row in st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))? {
            let (k, v) = row?;
            errs.insert(k, v);
        }
    }
    Ok(vols
        .into_iter()
        .map(|v| {
            let (books, copies, missing, bytes) = stats.get(&v.id).copied().unwrap_or_default();
            VolumeSummary {
                book_count: books,
                copy_count: copies,
                missing_copy_count: missing,
                indexed_bytes: bytes,
                root_count: roots_n.get(&v.id).copied().unwrap_or(0),
                active_jobs: jobs_n.get(&v.id).copied().unwrap_or(0),
                potential_savings: savings.get(&v.id).copied().unwrap_or(0),
                scan_errors: errs.get(&v.id).copied().unwrap_or(0),
                volume: v,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;

    fn observed(guid: &str, path: &str) -> ObservedVolume {
        ObservedVolume {
            identity: VolumeIdentity {
                platform_id: Some(guid.into()),
                serial: Some("ABCD-1234".into()),
                marker_uuid: None,
            },
            label: Some("ARCHIVE4".into()),
            fs_type: Some("NTFS".into()),
            capacity_bytes: Some(12_000_000_000_000),
            free_bytes: Some(1_000_000_000_000),
            mount_path: path.into(),
            storage_class: StorageClass::Hdd,
        }
    }

    #[test]
    fn reconnect_with_new_drive_letter_matches_same_volume() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let obs = observed("\\\\?\\Volume{1111}\\", "E:\\");
        let id = insert(&conn, &obs, "Archive HDD 4").unwrap();
        set_offline_except(&conn, &[]).unwrap();
        assert!(!get(&conn, id).unwrap().unwrap().is_online);

        let moved = observed("\\\\?\\Volume{1111}\\", "G:\\");
        let found = find_by_identity(&conn, &moved.identity, moved.capacity_bytes)
            .unwrap()
            .unwrap();
        assert_eq!(found.id, id);
        mark_seen(&conn, id, &moved).unwrap();
        let v = get(&conn, id).unwrap().unwrap();
        assert!(v.is_online);
        assert_eq!(v.last_mount_path.as_deref(), Some("G:\\"));
    }

    #[test]
    fn marker_takes_precedence_and_serial_needs_capacity() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let mut obs = observed("guid-a", "/Volumes/A");
        obs.identity.marker_uuid = Some("marker-1".into());
        let id = insert(&conn, &obs, "A").unwrap();
        let by_marker = VolumeIdentity {
            platform_id: Some("other".into()),
            serial: None,
            marker_uuid: Some("marker-1".into()),
        };
        assert_eq!(
            find_by_identity(&conn, &by_marker, None)
                .unwrap()
                .unwrap()
                .id,
            id
        );
        let serial_only = VolumeIdentity {
            platform_id: None,
            serial: Some("ABCD-1234".into()),
            marker_uuid: None,
        };
        assert!(
            find_by_identity(&conn, &serial_only, Some(500))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            find_by_identity(&conn, &serial_only, Some(12_000_000_000_000))
                .unwrap()
                .unwrap()
                .id,
            id
        );
    }
}
