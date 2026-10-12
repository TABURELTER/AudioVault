//! Persistent torrent transfer records (the engine keeps its own resume state;
//! this table links transfers to storage volumes and library import).

use audiovault_core::model::TorrentState;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{OptionalExt, Result, now_ts};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TorrentRecord {
    pub id: i64,
    pub info_hash: Option<String>,
    pub name: String,
    pub source_kind: String,
    pub magnet: Option<String>,
    #[serde(skip)]
    pub torrent_data: Option<Vec<u8>>,
    pub provider: Option<String>,
    pub provider_ref: Option<String>,
    pub volume_id: Option<i64>,
    pub dest_rel_path: String,
    pub selected_files: Option<Vec<usize>>,
    pub state: TorrentState,
    pub progress: f64,
    pub total_bytes: i64,
    pub downloaded_bytes: i64,
    pub uploaded_bytes: i64,
    pub engine_id: Option<i64>,
    pub error: Option<String>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub imported_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct NewTorrent {
    pub name: String,
    pub source_kind: String,
    pub magnet: Option<String>,
    pub torrent_data: Option<Vec<u8>>,
    pub provider: Option<String>,
    pub provider_ref: Option<String>,
    pub volume_id: Option<i64>,
    pub dest_rel_path: String,
    pub selected_files: Option<Vec<usize>>,
    pub info_hash: Option<String>,
}

const COLS: &str = "id, info_hash, name, source_kind, magnet, torrent_data, provider, provider_ref, volume_id, dest_rel_path, \
    selected_files, state, progress, total_bytes, downloaded_bytes, uploaded_bytes, engine_id, error, created_at, completed_at, imported_at";

fn row_to_torrent(r: &rusqlite::Row<'_>) -> rusqlite::Result<TorrentRecord> {
    let selected: Option<String> = r.get(10)?;
    let state: String = r.get(11)?;
    Ok(TorrentRecord {
        id: r.get(0)?,
        info_hash: r.get(1)?,
        name: r.get(2)?,
        source_kind: r.get(3)?,
        magnet: r.get(4)?,
        torrent_data: r.get(5)?,
        provider: r.get(6)?,
        provider_ref: r.get(7)?,
        volume_id: r.get(8)?,
        dest_rel_path: r.get(9)?,
        selected_files: selected.and_then(|s| serde_json::from_str(&s).ok()),
        state: TorrentState::parse(&state).unwrap_or(TorrentState::Error),
        progress: r.get(12)?,
        total_bytes: r.get(13)?,
        downloaded_bytes: r.get(14)?,
        uploaded_bytes: r.get(15)?,
        engine_id: r.get(16)?,
        error: r.get(17)?,
        created_at: r.get(18)?,
        completed_at: r.get(19)?,
        imported_at: r.get(20)?,
    })
}

pub fn insert(conn: &Connection, t: &NewTorrent) -> Result<i64> {
    let selected = match &t.selected_files {
        Some(v) => Some(serde_json::to_string(v)?),
        None => None,
    };
    conn.execute(
        "INSERT INTO torrent_transfer (info_hash, name, source_kind, magnet, torrent_data, provider, provider_ref, volume_id,
            dest_rel_path, selected_files, state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'queued', ?11)",
        params![
            t.info_hash,
            t.name,
            t.source_kind,
            t.magnet,
            t.torrent_data,
            t.provider,
            t.provider_ref,
            t.volume_id,
            t.dest_rel_path,
            selected,
            now_ts()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<TorrentRecord>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM torrent_transfer WHERE id = ?1"), [id], row_to_torrent).optional()?)
}

pub fn find_by_info_hash(conn: &Connection, info_hash: &str) -> Result<Option<TorrentRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM torrent_transfer WHERE info_hash = ?1 AND state <> 'removed' ORDER BY id DESC LIMIT 1"),
            [info_hash],
            row_to_torrent,
        )
        .optional()?)
}

/// All non-removed transfers, newest first.
pub fn list(conn: &Connection) -> Result<Vec<TorrentRecord>> {
    let mut st =
        conn.prepare_cached(&format!("SELECT {COLS} FROM torrent_transfer WHERE state <> 'removed' ORDER BY id DESC"))?;
    Ok(st.query_map([], row_to_torrent)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn set_engine(conn: &Connection, id: i64, engine_id: Option<i64>, info_hash: Option<&str>, name: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE torrent_transfer SET engine_id = ?2, info_hash = COALESCE(?3, info_hash), name = COALESCE(?4, name) WHERE id = ?1",
        params![id, engine_id, info_hash, name],
    )?;
    Ok(())
}

pub fn set_state(conn: &Connection, id: i64, state: TorrentState, error: Option<&str>) -> Result<()> {
    let now = now_ts();
    conn.execute(
        "UPDATE torrent_transfer SET state = ?2, error = ?3,
            completed_at = CASE WHEN ?2 IN ('completed','seeding') AND completed_at IS NULL THEN ?4 ELSE completed_at END
         WHERE id = ?1",
        params![id, state.as_str(), error, now],
    )?;
    Ok(())
}

pub fn update_stats(conn: &Connection, id: i64, progress: f64, total: u64, downloaded: u64, uploaded: u64) -> Result<()> {
    conn.execute(
        "UPDATE torrent_transfer SET progress = ?2, total_bytes = ?3, downloaded_bytes = ?4, uploaded_bytes = ?5 WHERE id = ?1",
        params![id, progress.clamp(0.0, 1.0), total, downloaded, uploaded],
    )?;
    Ok(())
}

pub fn mark_imported(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("UPDATE torrent_transfer SET imported_at = ?2 WHERE id = ?1", params![id, now_ts()])?;
    Ok(())
}

/// Completed transfers whose content has not yet been imported into the library.
pub fn pending_import(conn: &Connection) -> Result<Vec<TorrentRecord>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM torrent_transfer WHERE state IN ('completed','seeding') AND imported_at IS NULL ORDER BY id"
    ))?;
    Ok(st.query_map([], row_to_torrent)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Volumes with actively downloading transfers (used by the I/O scheduler).
pub fn volumes_downloading(conn: &Connection) -> Result<Vec<i64>> {
    let mut st = conn.prepare_cached(
        "SELECT DISTINCT volume_id FROM torrent_transfer WHERE state IN ('downloading','initializing') AND volume_id IS NOT NULL",
    )?;
    Ok(st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn mark_removed(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("UPDATE torrent_transfer SET state = 'removed', engine_id = NULL WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;

    #[test]
    fn crud_and_import_queue() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let id = insert(
            &conn,
            &NewTorrent {
                name: "Метро 2033".into(),
                source_kind: "magnet".into(),
                magnet: Some("magnet:?xt=urn:btih:abc".into()),
                dest_rel_path: "Downloads".into(),
                selected_files: Some(vec![0, 2]),
                ..Default::default()
            },
        )
        .unwrap();
        let t = get(&conn, id).unwrap().unwrap();
        assert_eq!(t.state, TorrentState::Queued);
        assert_eq!(t.selected_files, Some(vec![0, 2]));
        update_stats(&conn, id, 0.5, 100, 50, 0).unwrap();
        set_state(&conn, id, TorrentState::Completed, None).unwrap();
        assert_eq!(pending_import(&conn).unwrap().len(), 1);
        mark_imported(&conn, id).unwrap();
        assert!(pending_import(&conn).unwrap().is_empty());
        mark_removed(&conn, id).unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }
}
