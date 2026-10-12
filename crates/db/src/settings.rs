//! Key/value application settings stored in the catalog (non-secret only;
//! credentials live in the OS credential store).

use rusqlite::{Connection, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::{OptionalExt, Result, now_ts};

pub fn get(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT value FROM app_setting WHERE key = ?1", [key], |r| r.get(0)).optional()?)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO app_setting (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now_ts()],
    )?;
    Ok(())
}

pub fn remove(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM app_setting WHERE key = ?1", [key])?;
    Ok(())
}

pub fn get_json<T: DeserializeOwned>(conn: &Connection, key: &str) -> Result<Option<T>> {
    match get(conn, key)? {
        Some(s) => Ok(serde_json::from_str(&s).ok()),
        None => Ok(None),
    }
}

pub fn set_json<T: Serialize>(conn: &Connection, key: &str, value: &T) -> Result<()> {
    set(conn, key, &serde_json::to_string(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;

    #[test]
    fn roundtrip() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        assert_eq!(get(&conn, "theme").unwrap(), None);
        set(&conn, "theme", "dark").unwrap();
        set(&conn, "theme", "light").unwrap();
        assert_eq!(get(&conn, "theme").unwrap().as_deref(), Some("light"));
        set_json(&conn, "list", &vec![1, 2]).unwrap();
        assert_eq!(get_json::<Vec<i32>>(&conn, "list").unwrap(), Some(vec![1, 2]));
        remove(&conn, "theme").unwrap();
        assert_eq!(get(&conn, "theme").unwrap(), None);
    }
}
