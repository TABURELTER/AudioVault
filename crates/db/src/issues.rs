//! Metadata issues surfaced in the "Metadata Issues" view.

use audiovault_core::model::IssueKind;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{Result, now_ts};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NewIssue {
    pub kind: IssueKind,
    pub copy_id: Option<i64>,
    pub message: String,
    pub field: Option<String>,
    pub current_value: Option<String>,
    pub proposed_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IssueRow {
    pub id: i64,
    pub book_id: i64,
    pub copy_id: Option<i64>,
    pub kind: IssueKind,
    pub message: String,
    pub field: Option<String>,
    pub current_value: Option<String>,
    pub proposed_value: Option<String>,
    pub state: String,
    pub book_title: String,
    pub book_authors: String,
}

/// Replace the open issues of a book with a freshly computed set. Issues the
/// user dismissed stay dismissed as long as the same (kind, field) recurs.
pub fn replace_for_book(conn: &Connection, book_id: i64, issues: &[NewIssue]) -> Result<()> {
    let now = now_ts();
    let keys: Vec<String> =
        issues.iter().map(|i| format!("{}|{}", i.kind.as_str(), i.field.as_deref().unwrap_or(""))).collect();
    // Drop stale open/applied issues that no longer apply.
    {
        let mut st = conn.prepare_cached("SELECT id, kind, COALESCE(field, ''), state FROM metadata_issue WHERE book_id = ?1")?;
        let existing: Vec<(i64, String, String, String)> = st
            .query_map([book_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (id, kind, field, state) in existing {
            if state != "dismissed" && !keys.contains(&format!("{kind}|{field}")) {
                conn.execute("DELETE FROM metadata_issue WHERE id = ?1", [id])?;
            }
        }
    }
    let mut st = conn.prepare_cached(
        "INSERT INTO metadata_issue (book_id, copy_id, kind, message, field, current_value, proposed_value, state, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8)
         ON CONFLICT (book_id, kind, field) DO UPDATE SET copy_id = excluded.copy_id, message = excluded.message,
            current_value = excluded.current_value, proposed_value = excluded.proposed_value,
            state = CASE WHEN metadata_issue.state = 'dismissed' THEN 'dismissed' ELSE 'open' END",
    )?;
    for i in issues {
        // UNIQUE treats NULLs as distinct, so store an empty field instead of NULL.
        st.execute(params![
            book_id,
            i.copy_id,
            i.kind.as_str(),
            i.message,
            i.field.clone().unwrap_or_default(),
            i.current_value,
            i.proposed_value,
            now
        ])?;
    }
    Ok(())
}

const SELECT: &str = "SELECT i.id, i.book_id, i.copy_id, i.kind, i.message, NULLIF(i.field, ''), i.current_value, i.proposed_value,
        i.state, b.title, b.authors_display
     FROM metadata_issue i JOIN book b ON b.id = i.book_id";

fn row_to_issue(r: &rusqlite::Row<'_>) -> rusqlite::Result<IssueRow> {
    let kind: String = r.get(3)?;
    Ok(IssueRow {
        id: r.get(0)?,
        book_id: r.get(1)?,
        copy_id: r.get(2)?,
        kind: IssueKind::parse(&kind).unwrap_or(IssueKind::ProbeErrors),
        message: r.get(4)?,
        field: r.get(5)?,
        current_value: r.get(6)?,
        proposed_value: r.get(7)?,
        state: r.get(8)?,
        book_title: r.get(9)?,
        book_authors: r.get(10)?,
    })
}

/// Open issues, optionally filtered by kind, paged.
pub fn list_open(conn: &Connection, kind: Option<IssueKind>, offset: i64, limit: i64) -> Result<Vec<IssueRow>> {
    let mut st = conn.prepare_cached(&format!(
        "{SELECT} WHERE i.state = 'open' AND (?1 IS NULL OR i.kind = ?1) ORDER BY b.sort_author, b.sort_title, i.id LIMIT ?2 OFFSET ?3"
    ))?;
    Ok(st
        .query_map(params![kind.map(|k| k.as_str()), limit, offset], row_to_issue)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_many(conn: &Connection, ids: &[i64]) -> Result<Vec<IssueRow>> {
    let mut st = conn.prepare(&format!("{SELECT} WHERE i.id IN (SELECT value FROM json_each(?1))"))?;
    Ok(st.query_map([crate::ids_json(ids)], row_to_issue)?.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn counts_by_kind(conn: &Connection) -> Result<Vec<(IssueKind, i64)>> {
    let mut st = conn.prepare_cached("SELECT kind, COUNT(*) FROM metadata_issue WHERE state = 'open' GROUP BY kind ORDER BY 2 DESC")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (k, n) = row?;
        if let Some(k) = IssueKind::parse(&k) {
            out.push((k, n));
        }
    }
    Ok(out)
}

/// Set state ('open' | 'applied' | 'dismissed'); returns affected book ids.
pub fn set_state(conn: &Connection, ids: &[i64], state: &str) -> Result<Vec<i64>> {
    let json = crate::ids_json(ids);
    conn.execute("UPDATE metadata_issue SET state = ?2 WHERE id IN (SELECT value FROM json_each(?1))", params![json, state])?;
    let mut st = conn.prepare("SELECT DISTINCT book_id FROM metadata_issue WHERE id IN (SELECT value FROM json_each(?1))")?;
    Ok(st.query_map([json], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;
    use crate::test_util::temp_db;

    fn issue(kind: IssueKind, field: &str) -> NewIssue {
        NewIssue {
            kind,
            copy_id: None,
            message: kind.label().into(),
            field: Some(field.into()),
            current_value: Some("Unknown Artist".into()),
            proposed_value: Some("Дмитрий Глуховский".into()),
        }
    }

    #[test]
    fn dismissed_issues_stay_dismissed() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let book = catalog::create_book(&conn, "Метро 2033", &[]).unwrap();
        replace_for_book(&conn, book, &[issue(IssueKind::UnknownAuthor, "author"), issue(IssueKind::MissingNarrator, "narrator")])
            .unwrap();
        let open = list_open(&conn, None, 0, 10).unwrap();
        assert_eq!(open.len(), 2);
        let unknown = open.iter().find(|i| i.kind == IssueKind::UnknownAuthor).unwrap().id;
        set_state(&conn, &[unknown], "dismissed").unwrap();
        // Recompute: author issue still detected, narrator issue fixed.
        replace_for_book(&conn, book, &[issue(IssueKind::UnknownAuthor, "author")]).unwrap();
        assert!(list_open(&conn, None, 0, 10).unwrap().is_empty());
        assert_eq!(counts_by_kind(&conn).unwrap().len(), 0);
    }
}
