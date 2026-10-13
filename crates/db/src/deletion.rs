//! Book and e-book deletion operations, offline volume deletion queues, and audit history.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::{Result, now_ts};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDeletion {
    pub id: i64,
    pub volume_id: i64,
    pub volume_name: String,
    pub volume_online: bool,
    pub item_type: String,
    pub item_id: i64,
    pub rel_path: String,
    pub title: String,
    pub authors: String,
    pub size: u64,
    pub status: String,
    pub created_at: i64,
    pub executed_at: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionHistoryEntry {
    pub id: i64,
    pub item_type: String,
    pub title: String,
    pub authors: String,
    pub narrator: String,
    pub format: String,
    pub total_size: u64,
    pub duration_ms: i64,
    pub volume_name: String,
    pub mount_path: String,
    pub rel_path: String,
    pub status: String,
    pub deleted_at: i64,
}

pub fn set_book_listened(conn: &Connection, book_id: i64, listened: bool) -> Result<()> {
    conn.execute(
        "UPDATE book SET is_listened = ?2, updated_at = ?3 WHERE id = ?1",
        params![book_id, if listened { 1 } else { 0 }, now_ts()],
    )?;
    Ok(())
}

pub fn set_book_marked_for_deletion(conn: &Connection, book_id: i64, marked: bool) -> Result<()> {
    conn.execute(
        "UPDATE book SET is_marked_for_deletion = ?2, updated_at = ?3 WHERE id = ?1",
        params![book_id, if marked { 1 } else { 0 }, now_ts()],
    )?;
    Ok(())
}

pub fn set_ebook_read(conn: &Connection, ebook_id: i64, read: bool) -> Result<()> {
    conn.execute(
        "UPDATE ebook_entry SET is_read = ?2, updated_at = ?3 WHERE id = ?1",
        params![ebook_id, if read { 1 } else { 0 }, now_ts()],
    )?;
    Ok(())
}

pub fn set_ebook_marked_for_deletion(conn: &Connection, ebook_id: i64, marked: bool) -> Result<()> {
    conn.execute(
        "UPDATE ebook_entry SET is_marked_for_deletion = ?2, updated_at = ?3 WHERE id = ?1",
        params![ebook_id, if marked { 1 } else { 0 }, now_ts()],
    )?;
    Ok(())
}

pub fn queue_pending_deletion(
    conn: &Connection,
    volume_id: i64,
    item_type: &str,
    item_id: i64,
    rel_path: &str,
    title: &str,
    authors: &str,
    size: u64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO pending_deletion (volume_id, item_type, item_id, rel_path, title, authors, size, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
        params![volume_id, item_type, item_id, rel_path, title, authors, size as i64, now_ts()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn record_history(
    conn: &Connection,
    item_type: &str,
    title: &str,
    authors: &str,
    narrator: &str,
    format: &str,
    total_size: u64,
    duration_ms: i64,
    volume_name: &str,
    mount_path: &str,
    rel_path: &str,
    status: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO deletion_history (item_type, title, authors, narrator, format, total_size, duration_ms, volume_name, mount_path, rel_path, status, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            item_type,
            title,
            authors,
            narrator,
            format,
            total_size as i64,
            duration_ms,
            volume_name,
            mount_path,
            rel_path,
            status,
            now_ts()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_pending_for_volume(conn: &Connection, volume_id: i64) -> Result<Vec<PendingDeletion>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.volume_id, v.display_name, v.is_online, p.item_type, p.item_id, p.rel_path,
                p.title, p.authors, p.size, p.status, p.created_at, p.executed_at, p.error_message
         FROM pending_deletion p
         JOIN storage_volume v ON v.id = p.volume_id
         WHERE p.volume_id = ?1 AND p.status = 'pending'
         ORDER BY p.created_at ASC",
    )?;
    let rows = stmt.query_map([volume_id], |r| {
        Ok(PendingDeletion {
            id: r.get(0)?,
            volume_id: r.get(1)?,
            volume_name: r.get(2)?,
            volume_online: r.get::<_, i64>(3)? != 0,
            item_type: r.get(4)?,
            item_id: r.get(5)?,
            rel_path: r.get(6)?,
            title: r.get(7)?,
            authors: r.get(8)?,
            size: r.get::<_, i64>(9)? as u64,
            status: r.get(10)?,
            created_at: r.get(11)?,
            executed_at: r.get(12)?,
            error_message: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn list_all_pending(conn: &Connection) -> Result<Vec<PendingDeletion>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.volume_id, v.display_name, v.is_online, p.item_type, p.item_id, p.rel_path,
                p.title, p.authors, p.size, p.status, p.created_at, p.executed_at, p.error_message
         FROM pending_deletion p
         JOIN storage_volume v ON v.id = p.volume_id
         WHERE p.status = 'pending'
         ORDER BY p.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PendingDeletion {
            id: r.get(0)?,
            volume_id: r.get(1)?,
            volume_name: r.get(2)?,
            volume_online: r.get::<_, i64>(3)? != 0,
            item_type: r.get(4)?,
            item_id: r.get(5)?,
            rel_path: r.get(6)?,
            title: r.get(7)?,
            authors: r.get(8)?,
            size: r.get::<_, i64>(9)? as u64,
            status: r.get(10)?,
            created_at: r.get(11)?,
            executed_at: r.get(12)?,
            error_message: r.get(13)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn mark_pending_completed(conn: &Connection, id: i64) -> Result<()> {
    conn.execute(
        "UPDATE pending_deletion SET status = 'completed', executed_at = ?2 WHERE id = ?1",
        params![id, now_ts()],
    )?;
    Ok(())
}

pub fn mark_pending_failed(conn: &Connection, id: i64, error: &str) -> Result<()> {
    conn.execute(
        "UPDATE pending_deletion SET status = 'failed', error_message = ?2 WHERE id = ?1",
        params![id, error],
    )?;
    Ok(())
}

pub fn cancel_pending(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM pending_deletion WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn update_history_status(conn: &Connection, rel_path: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE deletion_history SET status = ?2 WHERE rel_path = ?1 AND status = 'pending_volume'",
        params![rel_path, status],
    )?;
    Ok(())
}

pub fn list_history(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<DeletionHistoryEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, item_type, title, authors, narrator, format, total_size, duration_ms,
                volume_name, mount_path, rel_path, status, deleted_at
         FROM deletion_history
         ORDER BY deleted_at DESC
         LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map([limit, offset], |r| {
        Ok(DeletionHistoryEntry {
            id: r.get(0)?,
            item_type: r.get(1)?,
            title: r.get(2)?,
            authors: r.get(3)?,
            narrator: r.get(4)?,
            format: r.get(5)?,
            total_size: r.get::<_, i64>(6)? as u64,
            duration_ms: r.get(7)?,
            volume_name: r.get(8)?,
            mount_path: r.get(9)?,
            rel_path: r.get(10)?,
            status: r.get(11)?,
            deleted_at: r.get(12)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn clear_history(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM deletion_history", [])?;
    Ok(())
}

pub fn counts_summary(conn: &Connection) -> Result<DeletionCounts> {
    let listened_books: i64 = conn
        .query_row("SELECT COUNT(*) FROM book WHERE is_listened = 1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    let read_ebooks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ebook_entry WHERE is_read = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let candidates_books: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM book WHERE is_marked_for_deletion = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let candidates_ebooks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ebook_entry WHERE is_marked_for_deletion = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let pending_tasks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pending_deletion WHERE status = 'pending'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let deleted_history_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM deletion_history", [], |r| r.get(0))
        .unwrap_or(0);

    Ok(DeletionCounts {
        listened_books,
        read_ebooks,
        candidates_books,
        candidates_ebooks,
        pending_tasks,
        deleted_history_count,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionCounts {
    pub listened_books: i64,
    pub read_ebooks: i64,
    pub candidates_books: i64,
    pub candidates_ebooks: i64,
    pub pending_tasks: i64,
    pub deleted_history_count: i64,
}
