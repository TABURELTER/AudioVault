//! Scan runs and per-path scan errors (history shown in the Storages view).

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{OptionalExt, Result, now_ts};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ScanCounters {
    pub dirs_seen: u64,
    pub files_seen: u64,
    pub copies_found: u64,
    pub copies_new: u64,
    pub copies_missing: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanRun {
    pub id: i64,
    pub volume_id: i64,
    pub root_id: Option<i64>,
    pub state: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub counters: ScanCounters,
    pub message: Option<String>,
}

pub fn start(conn: &Connection, volume_id: i64, root_id: Option<i64>) -> Result<i64> {
    conn.execute(
        "INSERT INTO scan_run (volume_id, root_id, state, started_at) VALUES (?1, ?2, 'running', ?3)",
        params![volume_id, root_id, now_ts()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_counters(conn: &Connection, scan_id: i64, c: &ScanCounters) -> Result<()> {
    conn.execute(
        "UPDATE scan_run SET dirs_seen = ?2, files_seen = ?3, copies_found = ?4, copies_new = ?5,
            copies_missing = ?6, errors = ?7 WHERE id = ?1",
        params![
            scan_id,
            c.dirs_seen,
            c.files_seen,
            c.copies_found,
            c.copies_new,
            c.copies_missing,
            c.errors
        ],
    )?;
    Ok(())
}

/// `state`: completed | interrupted | failed.
pub fn finish(
    conn: &Connection,
    scan_id: i64,
    state: &str,
    c: &ScanCounters,
    message: Option<&str>,
) -> Result<()> {
    update_counters(conn, scan_id, c)?;
    conn.execute(
        "UPDATE scan_run SET state = ?2, finished_at = ?3, message = ?4 WHERE id = ?1",
        params![scan_id, state, now_ts(), message],
    )?;
    Ok(())
}

/// Record a per-path error. Bounded: at most 1000 rows per scan are kept.
pub fn add_error(conn: &Connection, scan_id: i64, rel_path: &str, message: &str) -> Result<()> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM scan_error WHERE scan_id = ?1",
        [scan_id],
        |r| r.get(0),
    )?;
    if n < 1000 {
        conn.execute(
            "INSERT INTO scan_error (scan_id, rel_path, message, at) VALUES (?1, ?2, ?3, ?4)",
            params![scan_id, rel_path, message, now_ts()],
        )?;
    }
    Ok(())
}

pub fn errors(conn: &Connection, scan_id: i64, limit: i64) -> Result<Vec<(String, String)>> {
    let mut st = conn.prepare(
        "SELECT rel_path, message FROM scan_error WHERE scan_id = ?1 ORDER BY id LIMIT ?2",
    )?;
    Ok(st
        .query_map(params![scan_id, limit], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ScanRun> {
    Ok(ScanRun {
        id: r.get(0)?,
        volume_id: r.get(1)?,
        root_id: r.get(2)?,
        state: r.get(3)?,
        started_at: r.get(4)?,
        finished_at: r.get(5)?,
        counters: ScanCounters {
            dirs_seen: r.get(6)?,
            files_seen: r.get(7)?,
            copies_found: r.get(8)?,
            copies_new: r.get(9)?,
            copies_missing: r.get(10)?,
            errors: r.get(11)?,
        },
        message: r.get(12)?,
    })
}

const COLS: &str = "id, volume_id, root_id, state, started_at, finished_at, dirs_seen, files_seen, copies_found, \
    copies_new, copies_missing, errors, message";

pub fn get(conn: &Connection, id: i64) -> Result<Option<ScanRun>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM scan_run WHERE id = ?1"),
            [id],
            row,
        )
        .optional()?)
}

pub fn recent(conn: &Connection, volume_id: Option<i64>, limit: i64) -> Result<Vec<ScanRun>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COLS} FROM scan_run WHERE (?1 IS NULL OR volume_id = ?1) ORDER BY id DESC LIMIT ?2"
    ))?;
    Ok(st
        .query_map(params![volume_id, limit], row)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Scans left `running` by a crash become `interrupted` (nothing was marked missing).
pub fn interrupt_running_on_startup(conn: &Connection) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE scan_run SET state = 'interrupted', finished_at = ?1, message = 'application closed during scan'
         WHERE state = 'running'",
        [now_ts()],
    )?)
}

/// Keep history bounded: drop all but the newest `keep` runs per volume.
pub fn prune(conn: &Connection, keep: i64) -> Result<usize> {
    Ok(conn.execute(
        "DELETE FROM scan_run WHERE id IN (
            SELECT id FROM (SELECT id, ROW_NUMBER() OVER (PARTITION BY volume_id ORDER BY id DESC) AS rn FROM scan_run)
            WHERE rn > ?1)",
        [keep],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;

    #[test]
    fn lifecycle() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let vol = crate::volumes::insert(&conn, &Default::default(), "V").unwrap();
        let id = start(&conn, vol, None).unwrap();
        add_error(&conn, id, "a/b", "permission denied").unwrap();
        let c = ScanCounters {
            dirs_seen: 3,
            files_seen: 10,
            copies_found: 2,
            errors: 1,
            ..Default::default()
        };
        finish(&conn, id, "completed", &c, None).unwrap();
        let run = get(&conn, id).unwrap().unwrap();
        assert_eq!(run.state, "completed");
        assert_eq!(run.counters, c);
        assert_eq!(errors(&conn, id, 10).unwrap().len(), 1);
        let id2 = start(&conn, vol, None).unwrap();
        assert_eq!(interrupt_running_on_startup(&conn).unwrap(), 1);
        assert_eq!(get(&conn, id2).unwrap().unwrap().state, "interrupted");
        assert_eq!(recent(&conn, Some(vol), 10).unwrap().len(), 2);
        assert_eq!(prune(&conn, 1).unwrap(), 1);
    }
}
