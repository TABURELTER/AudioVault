//! SQLite catalog for AudioVault.
//!
//! The catalog is the authoritative source of knowledge: everything needed for
//! search and browsing lives here so offline disks never need to be touched.

pub mod catalog;
pub mod deletion;
pub mod duplicates;
pub mod ebooks;
pub mod fileops;
pub mod issues;
pub mod jobs;
pub mod library;
pub mod migrations;
pub mod optimize;
pub mod scans;
pub mod search;
pub mod settings;
pub mod torrents;
pub mod volumes;

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags};
use thiserror::Error;

pub use audiovault_core::format::now_ts;
pub use rusqlite;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid state: {0}")]
    Invalid(String),
    #[error("catalog schema version {found} is newer than this application supports ({supported})")]
    TooNew { found: i64, supported: i64 },
}

pub type Result<T> = std::result::Result<T, DbError>;

/// Small connection pool. SQLite in WAL mode allows many readers and one
/// writer; each pooled connection has a busy timeout so writers queue.
pub struct Db {
    path: PathBuf,
    pool: Mutex<Vec<Connection>>,
    max_idle: usize,
}

pub struct PooledConn<'a> {
    db: &'a Db,
    conn: Option<Connection>,
}

impl Deref for PooledConn<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn.as_ref().expect("connection present until drop")
    }
}

impl DerefMut for PooledConn<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        self.conn.as_mut().expect("connection present until drop")
    }
}

impl Drop for PooledConn<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // Never return a connection with an open transaction to the pool.
            if conn.is_autocommit() {
                let mut pool = self.db.pool.lock();
                if pool.len() < self.db.max_idle {
                    pool.push(conn);
                }
            }
        }
    }
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(Duration::from_secs(30))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", -32_000)?;
    Ok(())
}

impl Db {
    /// Open (creating if necessary) and migrate the catalog at `path`.
    pub fn open(path: &Path) -> Result<Arc<Db>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DbError::Invalid(format!("cannot create {parent:?}: {e}")))?;
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        configure(&conn)?;
        // WAL is persistent per database file; survives crashes with no data loss
        // beyond the last committed transaction. Check first before changing journal mode
        // to avoid attempting an exclusive lock when another process is reading/writing in WAL.
        let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if !mode.eq_ignore_ascii_case("wal") {
            let set_mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
            if !set_mode.eq_ignore_ascii_case("wal") {
                tracing::warn!(set_mode, "WAL journal mode unavailable; continuing");
            }
        }
        migrations::migrate(&conn)?;
        let _ = catalog::auto_upgrade_translit(&conn);
        let db = Arc::new(Db { path: path.to_path_buf(), pool: Mutex::new(vec![conn]), max_idle: 8 });
        Ok(db)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Borrow a pooled connection.
    pub fn conn(&self) -> Result<PooledConn<'_>> {
        if let Some(conn) = self.pool.lock().pop() {
            return Ok(PooledConn { db: self, conn: Some(conn) });
        }
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        configure(&conn)?;
        Ok(PooledConn { db: self, conn: Some(conn) })
    }

    /// Run `f` inside an IMMEDIATE transaction (takes the write lock up front,
    /// avoiding deadlock-prone lock upgrades).
    pub fn write<T>(&self, f: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T>) -> Result<T> {
        self.write_with(f)
    }

    /// Like [`Db::write`] but with a caller-defined error type, so higher
    /// layers can mix their own fallible steps into one transaction.
    pub fn write_with<T, E: From<DbError>>(
        &self,
        f: impl FnOnce(&rusqlite::Transaction<'_>) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        let mut conn = self.conn()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(DbError::from)?;
        let out = f(&tx)?;
        tx.commit().map_err(DbError::from)?;
        Ok(out)
    }

    /// Checkpoint WAL into the main DB (called on clean shutdown).
    pub fn checkpoint(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }
}

/// Helper: read an optional row.
pub(crate) trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// JSON array of ids for `json_each(?)` IN-lists (avoids N+1 queries).
pub(crate) fn ids_json(ids: &[i64]) -> String {
    serde_json::to_string(ids).unwrap_or_else(|_| "[]".into())
}

pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::*;

    pub fn temp_db() -> (tempfile::TempDir, Arc<Db>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = Db::open(&dir.path().join("catalog.sqlite")).expect("open db");
        (dir, db)
    }
}
