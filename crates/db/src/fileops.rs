//! Journal of filesystem-changing operations.
//!
//! Every rename/move/copy/delete/metadata-write is recorded *before* it is
//! executed (`planned`), marked `in_progress` immediately before touching the
//! filesystem and `completed`/`failed` afterwards. The catalog is only updated
//! once the filesystem change has succeeded. Operations found `in_progress` at
//! startup are flagged `needs_verification` and checked against the disk.

use audiovault_core::model::{FileOpKind, FileOpState};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{DbError, OptionalExt, Result, now_ts};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FileOpRecord {
    pub id: i64,
    pub kind: FileOpKind,
    pub state: FileOpState,
    pub volume_id: Option<i64>,
    pub dest_volume_id: Option<i64>,
    pub copy_id: Option<i64>,
    pub source_path: String,
    pub dest_path: Option<String>,
    pub payload: serde_json::Value,
    pub undo: Option<serde_json::Value>,
    pub confirmed: bool,
    pub error: Option<String>,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct NewFileOp {
    pub kind: FileOpKind,
    pub volume_id: Option<i64>,
    pub dest_volume_id: Option<i64>,
    pub copy_id: Option<i64>,
    pub source_path: String,
    pub dest_path: Option<String>,
    pub payload: serde_json::Value,
    /// Explicit user confirmation. Destructive operations cannot start without it.
    pub confirmed: bool,
}

const COLS: &str = "id, kind, state, volume_id, dest_volume_id, copy_id, source_path, dest_path, payload, undo, confirmed, \
    error, created_at, started_at, finished_at";

fn row_to_op(r: &rusqlite::Row<'_>) -> rusqlite::Result<FileOpRecord> {
    let kind: String = r.get(1)?;
    let state: String = r.get(2)?;
    let payload: String = r.get(8)?;
    let undo: Option<String> = r.get(9)?;
    Ok(FileOpRecord {
        id: r.get(0)?,
        kind: FileOpKind::parse(&kind).unwrap_or(FileOpKind::Rename),
        state: FileOpState::parse(&state).unwrap_or(FileOpState::NeedsVerification),
        volume_id: r.get(3)?,
        dest_volume_id: r.get(4)?,
        copy_id: r.get(5)?,
        source_path: r.get(6)?,
        dest_path: r.get(7)?,
        payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
        undo: undo.and_then(|u| serde_json::from_str(&u).ok()),
        confirmed: r.get::<_, i64>(10)? != 0,
        error: r.get(11)?,
        created_at: r.get(12)?,
        started_at: r.get(13)?,
        finished_at: r.get(14)?,
    })
}

pub fn plan(conn: &Connection, op: &NewFileOp) -> Result<i64> {
    conn.execute(
        "INSERT INTO file_operation (kind, state, volume_id, dest_volume_id, copy_id, source_path, dest_path, payload, confirmed, created_at)
         VALUES (?1, 'planned', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            op.kind.as_str(),
            op.volume_id,
            op.dest_volume_id,
            op.copy_id,
            op.source_path,
            op.dest_path,
            serde_json::to_string(&op.payload)?,
            op.confirmed as i64,
            now_ts()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<FileOpRecord>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COLS} FROM file_operation WHERE id = ?1"),
            [id],
            row_to_op,
        )
        .optional()?)
}

/// Record user confirmation for a planned operation.
pub fn confirm(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE file_operation SET confirmed = 1 WHERE id = ?1 AND state = 'planned'",
        [id],
    )?;
    Ok(())
}

/// Validated state transition. Destructive operations may only start when confirmed.
pub fn transition(
    conn: &Connection,
    id: i64,
    next: FileOpState,
    error: Option<&str>,
) -> Result<()> {
    let op = get(conn, id)?.ok_or_else(|| DbError::NotFound(format!("file operation {id}")))?;
    if !op.state.can_transition_to(next) {
        return Err(DbError::Invalid(format!(
            "file operation {id}: {} → {} not allowed",
            op.state, next
        )));
    }
    if next == FileOpState::InProgress && op.kind.is_destructive() && !op.confirmed {
        return Err(DbError::Invalid(format!(
            "file operation {id} ({}) is destructive and has not been confirmed by the user",
            op.kind
        )));
    }
    let now = now_ts();
    conn.execute(
        "UPDATE file_operation SET state = ?2, error = COALESCE(?3, error),
            started_at = CASE WHEN ?2 = 'in_progress' THEN ?4 ELSE started_at END,
            finished_at = CASE WHEN ?2 IN ('completed','failed','rolled_back','undone') THEN ?4 ELSE finished_at END
         WHERE id = ?1",
        params![id, next.as_str(), error, now],
    )?;
    Ok(())
}

pub fn set_undo(conn: &Connection, id: i64, undo: &serde_json::Value) -> Result<()> {
    conn.execute(
        "UPDATE file_operation SET undo = ?2 WHERE id = ?1",
        params![id, serde_json::to_string(undo)?],
    )?;
    Ok(())
}

pub fn set_dest(conn: &Connection, id: i64, dest: &str) -> Result<()> {
    conn.execute(
        "UPDATE file_operation SET dest_path = ?2 WHERE id = ?1",
        params![id, dest],
    )?;
    Ok(())
}

/// Recent operations, newest first.
pub fn recent(conn: &Connection, limit: i64) -> Result<Vec<FileOpRecord>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM file_operation ORDER BY id DESC LIMIT ?1"
    ))?;
    Ok(st
        .query_map([limit], row_to_op)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn by_state(conn: &Connection, state: FileOpState) -> Result<Vec<FileOpRecord>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM file_operation WHERE state = ?1 ORDER BY id"
    ))?;
    Ok(st
        .query_map([state.as_str()], row_to_op)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Startup: anything left `in_progress` must be verified against the disk;
/// never assume it finished.
pub fn flag_incomplete_on_startup(conn: &Connection) -> Result<Vec<FileOpRecord>> {
    conn.execute(
        "UPDATE file_operation SET state = 'needs_verification', error = 'application exited during this operation'
         WHERE state = 'in_progress'",
        [],
    )?;
    by_state(conn, FileOpState::NeedsVerification)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;
    use serde_json::json;

    fn new_op(kind: FileOpKind, confirmed: bool) -> NewFileOp {
        NewFileOp {
            kind,
            volume_id: None,
            dest_volume_id: None,
            copy_id: None,
            source_path: "C:/a".into(),
            dest_path: Some("C:/b".into()),
            payload: json!({}),
            confirmed,
        }
    }

    #[test]
    fn state_transitions_validated() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let id = plan(&conn, &new_op(FileOpKind::Rename, false)).unwrap();
        assert!(
            transition(&conn, id, FileOpState::Completed, None).is_err(),
            "planned → completed is illegal"
        );
        transition(&conn, id, FileOpState::InProgress, None).unwrap();
        transition(&conn, id, FileOpState::Completed, None).unwrap();
        let op = get(&conn, id).unwrap().unwrap();
        assert_eq!(op.state, FileOpState::Completed);
        assert!(op.finished_at.is_some());
        assert!(transition(&conn, id, FileOpState::InProgress, None).is_err());
        transition(&conn, id, FileOpState::Undone, None).unwrap();
    }

    #[test]
    fn destructive_requires_confirmation() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let id = plan(&conn, &new_op(FileOpKind::DeletePermanent, false)).unwrap();
        let err = transition(&conn, id, FileOpState::InProgress, None).unwrap_err();
        assert!(err.to_string().contains("not been confirmed"));
        confirm(&conn, id).unwrap();
        transition(&conn, id, FileOpState::InProgress, None).unwrap();
    }

    #[test]
    fn crash_leaves_needs_verification() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let id = plan(&conn, &new_op(FileOpKind::Move, true)).unwrap();
        transition(&conn, id, FileOpState::InProgress, None).unwrap();
        let pending = flag_incomplete_on_startup(&conn).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].state, FileOpState::NeedsVerification);
        transition(&conn, id, FileOpState::Completed, None).unwrap();
    }
}
