//! Forward-only schema migrations tracked with `PRAGMA user_version`.

use rusqlite::Connection;

use crate::{DbError, Result};

/// Ordered list of migrations; index + 1 = schema version after applying.
pub const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_initial.sql"),
    include_str!("../migrations/0002_fingerprints_optimizer.sql"),
    include_str!("../migrations/0003_media_tags.sql"),
    include_str!("../migrations/0004_ebooks.sql"),
    include_str!("../migrations/0005_reading_status_and_deletion.sql"),
];

pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn current_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Apply all pending migrations, each in its own transaction.
pub fn migrate(conn: &Connection) -> Result<()> {
    migrate_to(conn, LATEST_VERSION)
}

/// Apply migrations up to `target` (used by migration tests).
pub fn migrate_to(conn: &Connection, target: i64) -> Result<()> {
    let current = current_version(conn)?;
    if current > LATEST_VERSION {
        return Err(DbError::TooNew { found: current, supported: LATEST_VERSION });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate() {
        let version = i as i64 + 1;
        if version <= current || version > target {
            continue;
        }
        tracing::info!(version, "applying catalog migration");
        conn.execute_batch(&format!("BEGIN IMMEDIATE;\n{sql}\nPRAGMA user_version = {version};\nCOMMIT;"))
            .inspect_err(|_| {
                let _ = conn.execute_batch("ROLLBACK;");
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_database_reaches_latest() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        // Idempotent.
        migrate(&conn).unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        // FTS works.
        conn.execute("INSERT INTO book_fts(rowid, title, authors, series, narrators, aliases, skel) VALUES (1,'метро 2033','','','','','metro 2033')", []).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM book_fts WHERE book_fts MATCH 'метро*'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn upgrade_from_v1_preserves_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            migrate_to(&conn, 1).unwrap();
            assert_eq!(current_version(&conn).unwrap(), 1);
            conn.execute(
                "INSERT INTO storage_volume(uuid, display_name, created_at) VALUES ('u1', 'Archive HDD 4', 1)",
                [],
            )
            .unwrap();
            // v2 table must not exist yet.
            let has: i64 = conn
                .query_row("SELECT count(*) FROM sqlite_master WHERE name='copy_fingerprint'", [], |r| r.get(0))
                .unwrap();
            assert_eq!(has, 0);
        }
        let db = crate::Db::open(&path).unwrap();
        let conn = db.conn().unwrap();
        assert_eq!(current_version(&conn).unwrap(), LATEST_VERSION);
        let (name, policy): (String, String) = conn
            .query_row("SELECT display_name, marker_policy FROM storage_volume WHERE uuid='u1'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(name, "Archive HDD 4");
        assert_eq!(policy, "ask");
    }

    #[test]
    fn refuses_newer_schema() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 999;").unwrap();
        assert!(matches!(migrate(&conn), Err(DbError::TooNew { .. })));
    }
}
