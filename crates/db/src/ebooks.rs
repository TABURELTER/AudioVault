//! E-books database operations (independent text/electronic books catalog).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEbook {
    pub rel_path: String,
    pub file_name: String,
    pub format: String,
    pub size: u64,
    pub mtime: i64,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub genres: Option<String>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub cover_thumb: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbookSummary {
    pub id: i64,
    pub volume_id: i64,
    pub volume_name: String,
    pub volume_online: bool,
    pub rel_path: String,
    pub file_name: String,
    pub format: String,
    pub size: u64,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub year: Option<i32>,
    pub cover_thumb: Option<String>,
    pub full_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbookEntry {
    pub id: i64,
    pub volume_id: i64,
    pub root_id: Option<i64>,
    pub volume_name: String,
    pub volume_online: bool,
    pub mount_path: Option<String>,
    pub rel_path: String,
    pub file_name: String,
    pub format: String,
    pub size: u64,
    pub mtime: i64,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub genres: Option<String>,
    pub year: Option<i32>,
    pub language: Option<String>,
    pub description: Option<String>,
    pub cover_thumb: Option<String>,
    pub state: String,
    pub full_path: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbookStats {
    pub total_count: i64,
    pub total_size_bytes: u64,
    pub formats: Vec<(String, i64)>,
    pub authors_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EbookSearchResult {
    pub items: Vec<EbookSummary>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct ExistingEbookHeader {
    pub id: i64,
    pub size: u64,
    pub mtime: i64,
    pub state: &'static str,
}

/// Check if an e-book already exists with matching size and mtime.
pub fn find_existing_ebook(conn: &Connection, volume_id: i64, rel_path: &str) -> Result<Option<(i64, u64, i64, String)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, size, mtime, state FROM ebook_entry WHERE volume_id = ?1 AND rel_path = ?2",
    )?;
    let mut rows = stmt.query(params![volume_id, rel_path])?;
    if let Some(r) = rows.next()? {
        Ok(Some((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
    } else {
        Ok(None)
    }
}

/// Fast touch: update last_scan_id and state if present, without rewriting metadata.
pub fn touch_ebook(conn: &Connection, id: i64, scan_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE ebook_entry SET state = 'present', last_scan_id = ?1 WHERE id = ?2",
        params![scan_id, id],
    )?;
    Ok(())
}

/// Upsert an e-book record.
pub fn upsert_ebook(
    conn: &Connection,
    volume_id: i64,
    root_id: Option<i64>,
    scan_id: i64,
    entry: &NewEbook,
) -> Result<i64> {
    let now = crate::now_ts();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO ebook_entry (
            volume_id, root_id, rel_path, file_name, format, size, mtime,
            title, authors, series, series_index, genres, year, language,
            description, cover_thumb, state, last_scan_id, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, 'present', ?17, ?18, ?18
         )
         ON CONFLICT(volume_id, rel_path) DO UPDATE SET
            root_id = excluded.root_id,
            file_name = excluded.file_name,
            format = excluded.format,
            size = excluded.size,
            mtime = excluded.mtime,
            title = excluded.title,
            authors = excluded.authors,
            series = excluded.series,
            series_index = excluded.series_index,
            genres = excluded.genres,
            year = excluded.year,
            language = excluded.language,
            description = coalesce(excluded.description, ebook_entry.description),
            cover_thumb = coalesce(excluded.cover_thumb, ebook_entry.cover_thumb),
            state = 'present',
            last_scan_id = excluded.last_scan_id,
            updated_at = excluded.updated_at
         RETURNING id",
    )?;
    let id: i64 = stmt.query_row(
        params![
            volume_id,
            root_id,
            entry.rel_path,
            entry.file_name,
            entry.format,
            entry.size,
            entry.mtime,
            entry.title,
            entry.authors,
            entry.series,
            entry.series_index,
            entry.genres,
            entry.year,
            entry.language,
            entry.description,
            entry.cover_thumb,
            scan_id,
            now
        ],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Mark missing e-books that were not seen during the latest scan under `prefix`.
pub fn mark_missing_ebooks(conn: &Connection, volume_id: i64, prefix: &str, scan_id: i64) -> Result<usize> {
    let now = crate::now_ts();
    let prefix_slash = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
    let n = conn.execute(
        "UPDATE ebook_entry SET state = 'missing', updated_at = ?1
         WHERE volume_id = ?2 AND state = 'present'
           AND (last_scan_id IS NULL OR last_scan_id <> ?3)
           AND (?4 = '' OR rel_path = ?4 OR substr(rel_path, 1, length(?5)) = ?5)",
        params![now, volume_id, scan_id, prefix, prefix_slash],
    )?;
    Ok(n)
}

/// Fetch a single e-book by ID with volume information.
pub fn get_ebook(conn: &Connection, id: i64) -> Result<Option<EbookEntry>> {
    let mut stmt = conn.prepare_cached(
        "SELECT e.id, e.volume_id, e.root_id, v.display_name, v.is_online, v.last_mount_path,
                e.rel_path, e.file_name, e.format, e.size, e.mtime,
                e.title, e.authors, e.series, e.series_index, e.genres,
                e.year, e.language, e.description, e.cover_thumb, e.state,
                e.created_at, e.updated_at
         FROM ebook_entry e
         JOIN storage_volume v ON v.id = e.volume_id
         WHERE e.id = ?1",
    )?;
    let res = stmt.query_row(params![id], |r| {
        let volume_id: i64 = r.get(1)?;
        let root_id: Option<i64> = r.get(2)?;
        let volume_name: String = r.get(3)?;
        let volume_online: bool = r.get(4)?;
        let mount_path: Option<String> = r.get(5)?;
        let rel_path: String = r.get(6)?;
        let full_path = mount_path.as_deref().map(|m| {
            audiovault_core::paths::from_db_rel(std::path::Path::new(m), &rel_path)
                .display()
                .to_string()
        });
        Ok(EbookEntry {
            id: r.get(0)?,
            volume_id,
            root_id,
            volume_name,
            volume_online,
            mount_path,
            rel_path,
            file_name: r.get(7)?,
            format: r.get(8)?,
            size: r.get(9)?,
            mtime: r.get(10)?,
            title: r.get(11)?,
            authors: r.get(12)?,
            series: r.get(13)?,
            series_index: r.get(14)?,
            genres: r.get(15)?,
            year: r.get(16)?,
            language: r.get(17)?,
            description: r.get(18)?,
            cover_thumb: r.get(19)?,
            state: r.get(20)?,
            full_path,
            created_at: r.get(21)?,
            updated_at: r.get(22)?,
        })
    }).optional()?;
    Ok(res)
}

/// Search and filter e-books with pagination and optional FTS5 matching.
pub fn search_ebooks(
    conn: &Connection,
    query: Option<&str>,
    format_filter: Option<&str>,
    sort_by: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<EbookSearchResult> {
    let q = query.map(str::trim).filter(|s| !s.is_empty());
    let fmt = format_filter.map(str::trim).filter(|s| !s.is_empty() && *s != "all");

    let is_archive_filter = fmt.map(|s| s == "archives" || s == "zip" || s == "fb2.zip").unwrap_or(false);

    // Build FTS tokens if query is present
    let (items, total_count) = if let Some(query_str) = q {
        // Sanitize for FTS5
        let tokens: Vec<String> = query_str
            .split_whitespace()
            .map(|t| t.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').collect::<String>())
            .filter(|s| !s.is_empty())
            .map(|s| format!("\"{s}\"*"))
            .collect();

        if tokens.is_empty() {
            return Ok(EbookSearchResult { items: vec![], total_count: 0 });
        }
        let fts_expr = tokens.join(" AND ");

        let fmt_filter_clause = if is_archive_filter {
            "AND e.format IN ('fb2.zip', 'zip')"
        } else if fmt.is_some() {
            "AND e.format = ?2"
        } else {
            ""
        };

        let count_sql = format!(
            "SELECT count(*)
             FROM ebook_entry e
             JOIN ebook_fts f ON f.rowid = e.id
             WHERE e.state = 'present'
               AND ebook_fts MATCH ?1
               {fmt_filter_clause}"
        );

        let total: i64 = if is_archive_filter || fmt.is_none() {
            conn.query_row(&count_sql, params![fts_expr], |r| r.get(0))?
        } else {
            conn.query_row(&count_sql, params![fts_expr, fmt.unwrap()], |r| r.get(0))?
        };

        let needs_param = !is_archive_filter && fmt.is_some();
        let select_sql = format!(
            "SELECT e.id, e.volume_id, v.display_name, v.is_online, v.last_mount_path,
                    e.rel_path, e.file_name, e.format, e.size,
                    e.title, e.authors, e.series, e.series_index, e.year, e.cover_thumb
             FROM ebook_entry e
             JOIN ebook_fts f ON f.rowid = e.id
             JOIN storage_volume v ON v.id = e.volume_id
             WHERE e.state = 'present'
               AND ebook_fts MATCH ?1
               {fmt_filter_clause}
             ORDER BY bm25(ebook_fts) ASC, e.title ASC
             LIMIT ?{} OFFSET ?{}",
            if needs_param { 3 } else { 2 },
            if needs_param { 4 } else { 3 },
        );

        let mut stmt = conn.prepare(&select_sql)?;
        let rows = if needs_param {
            stmt.query_map(params![fts_expr, fmt.unwrap(), limit, offset], row_to_summary)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![fts_expr, limit, offset], row_to_summary)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };

        (rows, total)
    } else {
        let fmt_filter_clause = if is_archive_filter {
            "AND e.format IN ('fb2.zip', 'zip')"
        } else if fmt.is_some() {
            "AND e.format = ?1"
        } else {
            ""
        };

        let count_sql = format!(
            "SELECT count(*) FROM ebook_entry e WHERE e.state = 'present' {fmt_filter_clause}"
        );
        let total: i64 = if is_archive_filter || fmt.is_none() {
            conn.query_row(&count_sql, [], |r| r.get(0))?
        } else {
            conn.query_row(&count_sql, params![fmt.unwrap()], |r| r.get(0))?
        };

        let order_clause = match sort_by.unwrap_or("author") {
            "title" => "ORDER BY e.title COLLATE NOCASE ASC, e.series_index ASC",
            "series" => "ORDER BY CASE WHEN e.series IS NULL OR e.series = '' THEN 1 ELSE 0 END, e.series COLLATE NOCASE ASC, e.series_index ASC",
            "year" => "ORDER BY e.year DESC, e.title ASC",
            "size" => "ORDER BY e.size DESC",
            "added" => "ORDER BY e.created_at DESC",
            _ => "ORDER BY CASE WHEN e.authors IS NULL OR e.authors = '' THEN 1 ELSE 0 END, e.authors COLLATE NOCASE ASC, e.series COLLATE NOCASE ASC, e.series_index ASC, e.title ASC",
        };

        let needs_param = !is_archive_filter && fmt.is_some();
        let select_sql = format!(
            "SELECT e.id, e.volume_id, v.display_name, v.is_online, v.last_mount_path,
                    e.rel_path, e.file_name, e.format, e.size,
                    e.title, e.authors, e.series, e.series_index, e.year, e.cover_thumb
             FROM ebook_entry e
             JOIN storage_volume v ON v.id = e.volume_id
             WHERE e.state = 'present'
               {fmt_filter_clause}
             {order_clause}
             LIMIT ?{} OFFSET ?{}",
            if needs_param { 2 } else { 1 },
            if needs_param { 3 } else { 2 },
        );

        let mut stmt = conn.prepare(&select_sql)?;
        let rows = if needs_param {
            stmt.query_map(params![fmt.unwrap(), limit, offset], row_to_summary)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            stmt.query_map(params![limit, offset], row_to_summary)?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };

        (rows, total)
    };

    Ok(EbookSearchResult { items, total_count })
}

fn row_to_summary(r: &rusqlite::Row<'_>) -> rusqlite::Result<EbookSummary> {
    let mount_path: Option<String> = r.get(4)?;
    let rel_path: String = r.get(5)?;
    let full_path = mount_path.as_deref().map(|m| {
        audiovault_core::paths::from_db_rel(std::path::Path::new(m), &rel_path)
            .display()
            .to_string()
    });
    Ok(EbookSummary {
        id: r.get(0)?,
        volume_id: r.get(1)?,
        volume_name: r.get(2)?,
        volume_online: r.get(3)?,
        rel_path,
        file_name: r.get(6)?,
        format: r.get(7)?,
        size: r.get(8)?,
        title: r.get(9)?,
        authors: r.get(10)?,
        series: r.get(11)?,
        series_index: r.get(12)?,
        year: r.get(13)?,
        cover_thumb: r.get(14)?,
        full_path,
    })
}

/// Statistics for electronic books.
pub fn get_ebook_stats(conn: &Connection) -> Result<EbookStats> {
    let (total_count, total_size_bytes): (i64, Option<i64>) = conn.query_row(
        "SELECT count(*), sum(size) FROM ebook_entry WHERE state = 'present'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let authors_count: i64 = conn.query_row(
        "SELECT count(DISTINCT authors) FROM ebook_entry WHERE state = 'present' AND authors <> ''",
        [],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT format, count(*) FROM ebook_entry WHERE state = 'present' GROUP BY format ORDER BY 2 DESC",
    )?;
    let formats = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(EbookStats {
        total_count,
        total_size_bytes: total_size_bytes.unwrap_or(0) as u64,
        formats,
        authors_count,
    })
}
