//! Persistent background job queue.

use audiovault_core::model::{JobKind, JobState};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{OptionalExt, Result, now_ts};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JobRecord {
    pub id: i64,
    pub kind: JobKind,
    pub state: JobState,
    pub priority: i64,
    pub volume_id: Option<i64>,
    pub payload: serde_json::Value,
    pub title: String,
    pub progress: f64,
    pub progress_text: String,
    pub error: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub cancel_requested: bool,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct NewJob {
    pub kind: JobKind,
    pub priority: i64,
    pub volume_id: Option<i64>,
    pub payload: serde_json::Value,
    pub title: String,
    /// When set, an active job with the same key is reused instead of duplicated.
    pub dedupe_key: Option<String>,
    pub max_attempts: i64,
}

impl NewJob {
    pub fn new(kind: JobKind, title: impl Into<String>, payload: serde_json::Value) -> Self {
        NewJob { kind, priority: 0, volume_id: None, payload, title: title.into(), dedupe_key: None, max_attempts: 3 }
    }
    pub fn volume(mut self, v: i64) -> Self {
        self.volume_id = Some(v);
        self
    }
    pub fn priority(mut self, p: i64) -> Self {
        self.priority = p;
        self
    }
    pub fn dedupe(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }
}

const COLS: &str = "id, kind, state, priority, volume_id, payload, title, progress, progress_text, error, attempts, max_attempts, \
    cancel_requested, created_at, started_at, finished_at, updated_at";

fn row_to_job(r: &rusqlite::Row<'_>) -> rusqlite::Result<JobRecord> {
    let kind: String = r.get(1)?;
    let state: String = r.get(2)?;
    let payload: String = r.get(5)?;
    Ok(JobRecord {
        id: r.get(0)?,
        kind: JobKind::parse(&kind).unwrap_or(JobKind::Reconcile),
        state: JobState::parse(&state).unwrap_or(JobState::Failed),
        priority: r.get(3)?,
        volume_id: r.get(4)?,
        payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
        title: r.get(6)?,
        progress: r.get(7)?,
        progress_text: r.get(8)?,
        error: r.get(9)?,
        attempts: r.get(10)?,
        max_attempts: r.get(11)?,
        cancel_requested: r.get::<_, i64>(12)? != 0,
        created_at: r.get(13)?,
        started_at: r.get(14)?,
        finished_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

pub fn enqueue(conn: &Connection, j: &NewJob) -> Result<i64> {
    if let Some(key) = &j.dedupe_key
        && let Some(id) = conn
            .query_row(
                "SELECT id FROM background_job WHERE dedupe_key = ?1 AND state IN ('queued','paused') LIMIT 1",
                [key],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
    {
        return Ok(id);
    }
    let now = now_ts();
    conn.execute(
        "INSERT INTO background_job (kind, state, priority, volume_id, payload, title, max_attempts, dedupe_key, created_at, updated_at)
         VALUES (?1, 'queued', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            j.kind.as_str(),
            j.priority,
            j.volume_id,
            serde_json::to_string(&j.payload)?,
            j.title,
            j.max_attempts,
            j.dedupe_key,
            now
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<JobRecord>> {
    Ok(conn.query_row(&format!("SELECT {COLS} FROM background_job WHERE id = ?1"), [id], row_to_job).optional()?)
}

/// Queued jobs in dispatch order.
pub fn queued(conn: &Connection, limit: i64) -> Result<Vec<JobRecord>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM background_job WHERE state = 'queued' ORDER BY priority DESC, id LIMIT ?1"
    ))?;
    Ok(st.query_map([limit], row_to_job)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Atomically claim a queued job. Returns false if someone else took it or it was cancelled.
pub fn mark_running(conn: &Connection, id: i64) -> Result<bool> {
    let now = now_ts();
    let n = conn.execute(
        "UPDATE background_job SET state = 'running', started_at = ?2, updated_at = ?2, attempts = attempts + 1,
            cancel_requested = 0, error = NULL
         WHERE id = ?1 AND state = 'queued'",
        params![id, now],
    )?;
    Ok(n == 1)
}

pub fn set_progress(conn: &Connection, id: i64, progress: f64, text: &str) -> Result<()> {
    conn.execute(
        "UPDATE background_job SET progress = ?2, progress_text = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, progress.clamp(0.0, 1.0), text, now_ts()],
    )?;
    Ok(())
}

pub fn finish(conn: &Connection, id: i64, state: JobState, error: Option<&str>) -> Result<()> {
    let now = now_ts();
    // A job paused while running ends up 'paused' (it will resume from scratch, all jobs are idempotent).
    conn.execute(
        "UPDATE background_job SET state = CASE WHEN state = 'paused' THEN 'paused' ELSE ?2 END,
            error = ?3, finished_at = CASE WHEN ?2 IN ('completed','failed','cancelled') THEN ?4 ELSE finished_at END,
            progress = CASE WHEN ?2 = 'completed' THEN 1.0 ELSE progress END, updated_at = ?4
         WHERE id = ?1",
        params![id, state.as_str(), error, now],
    )?;
    Ok(())
}

/// Retry a failed job later (keeps attempts counter).
pub fn requeue(conn: &Connection, id: i64, error: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE background_job SET state = 'queued', error = ?2, updated_at = ?3 WHERE id = ?1 AND state IN ('running','failed','interrupted')",
        params![id, error, now_ts()],
    )?;
    Ok(())
}

pub fn request_cancel(conn: &Connection, id: i64) -> Result<()> {
    let now = now_ts();
    conn.execute(
        "UPDATE background_job SET state = 'cancelled', finished_at = ?2, updated_at = ?2 WHERE id = ?1 AND state IN ('queued','paused','interrupted')",
        params![id, now],
    )?;
    conn.execute(
        "UPDATE background_job SET cancel_requested = 1, updated_at = ?2 WHERE id = ?1 AND state = 'running'",
        params![id, now],
    )?;
    Ok(())
}

pub fn is_cancel_requested(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn
        .query_row("SELECT cancel_requested, state FROM background_job WHERE id = ?1", [id], |r| {
            Ok(r.get::<_, i64>(0)? != 0 || r.get::<_, String>(1)? == "paused")
        })
        .optional()?
        .unwrap_or(true))
}

/// Pause: queued jobs stop being dispatched; running jobs are interrupted
/// cooperatively and stay paused until resumed.
pub fn pause(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE background_job SET state = 'paused', updated_at = ?2 WHERE id = ?1 AND state IN ('queued','running')",
        params![id, now_ts()],
    )?;
    Ok(())
}

pub fn resume(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE background_job SET state = 'queued', cancel_requested = 0, updated_at = ?2 WHERE id = ?1 AND state IN ('paused','interrupted')",
        params![id, now_ts()],
    )?;
    Ok(())
}

pub fn retry(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE background_job SET state = 'queued', attempts = 0, error = NULL, cancel_requested = 0, updated_at = ?2
         WHERE id = ?1 AND state IN ('failed','cancelled','interrupted')",
        params![id, now_ts()],
    )?;
    Ok(())
}

/// Active jobs plus the most recent finished ones.
pub fn list(conn: &Connection, recent_finished: i64) -> Result<Vec<JobRecord>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {COLS} FROM (
            SELECT * FROM background_job WHERE state IN ('running','queued','paused','interrupted')
            UNION ALL
            SELECT * FROM (SELECT * FROM background_job WHERE state IN ('completed','failed','cancelled') ORDER BY updated_at DESC LIMIT ?1)
         ) ORDER BY CASE state WHEN 'running' THEN 0 WHEN 'paused' THEN 1 WHEN 'interrupted' THEN 2 WHEN 'queued' THEN 3 ELSE 4 END,
                    priority DESC, updated_at DESC, id"
    ))?;
    Ok(st.query_map([recent_finished], row_to_job)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn active_count(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM background_job WHERE state IN ('running','queued')",
        [],
        |r| r.get(0),
    )?)
}

pub fn clear_finished(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM background_job WHERE state IN ('completed','cancelled')", [])?)
}

/// Startup recovery: jobs left 'running' by a crash become queued again when
/// restartable (scans/hashing are idempotent), otherwise 'interrupted' for the
/// user to inspect (file operations, transcodes).
pub fn recover_on_startup(conn: &Connection) -> Result<(usize, usize)> {
    let mut st = conn.prepare("SELECT id, kind, attempts, max_attempts FROM background_job WHERE state = 'running'")?;
    let rows: Vec<(i64, String, i64, i64)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let (mut requeued, mut interrupted) = (0, 0);
    let now = now_ts();
    for (id, kind, attempts, max) in rows {
        let restartable = JobKind::parse(&kind).is_some_and(|k| k.restartable());
        if restartable && attempts < max {
            conn.execute(
                "UPDATE background_job SET state = 'queued', progress_text = 'resumed after restart', updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            requeued += 1;
        } else {
            conn.execute(
                "UPDATE background_job SET state = 'interrupted', error = 'application exited while this job was running', updated_at = ?2 WHERE id = ?1",
                params![id, now],
            )?;
            interrupted += 1;
        }
    }
    // Purge very old finished jobs (bounded table growth).
    conn.execute(
        "DELETE FROM background_job WHERE state IN ('completed','cancelled') AND updated_at < ?1",
        [now - 14 * 86400],
    )?;
    Ok((requeued, interrupted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;
    use serde_json::json;

    #[test]
    fn lifecycle_and_dedupe() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let a = enqueue(&conn, &NewJob::new(JobKind::Scan, "scan", json!({"root": 1})).dedupe("scan:1")).unwrap();
        let b = enqueue(&conn, &NewJob::new(JobKind::Scan, "scan", json!({"root": 1})).dedupe("scan:1")).unwrap();
        assert_eq!(a, b);
        let hi = enqueue(&conn, &NewJob::new(JobKind::Metadata, "meta", json!({})).priority(10)).unwrap();
        let q = queued(&conn, 10).unwrap();
        assert_eq!(q[0].id, hi, "priority order");
        assert!(mark_running(&conn, a).unwrap());
        assert!(!mark_running(&conn, a).unwrap(), "cannot claim twice");
        request_cancel(&conn, a).unwrap();
        assert!(is_cancel_requested(&conn, a).unwrap());
        finish(&conn, a, JobState::Cancelled, None).unwrap();
        assert_eq!(get(&conn, a).unwrap().unwrap().state, JobState::Cancelled);
    }

    #[test]
    fn startup_recovery() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let scan = enqueue(&conn, &NewJob::new(JobKind::Scan, "scan", json!({}))).unwrap();
        let tc = enqueue(&conn, &NewJob::new(JobKind::Transcode, "tc", json!({}))).unwrap();
        mark_running(&conn, scan).unwrap();
        mark_running(&conn, tc).unwrap();
        let (r, i) = recover_on_startup(&conn).unwrap();
        assert_eq!((r, i), (1, 1));
        assert_eq!(get(&conn, scan).unwrap().unwrap().state, JobState::Queued);
        assert_eq!(get(&conn, tc).unwrap().unwrap().state, JobState::Interrupted, "transcode never assumed complete");
    }

    #[test]
    fn pause_resume() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let j = enqueue(&conn, &NewJob::new(JobKind::FullHash, "hash", json!({}))).unwrap();
        mark_running(&conn, j).unwrap();
        pause(&conn, j).unwrap();
        assert!(is_cancel_requested(&conn, j).unwrap(), "running job sees pause as stop signal");
        finish(&conn, j, JobState::Cancelled, None).unwrap();
        assert_eq!(get(&conn, j).unwrap().unwrap().state, JobState::Paused);
        resume(&conn, j).unwrap();
        assert_eq!(get(&conn, j).unwrap().unwrap().state, JobState::Queued);
    }
}
