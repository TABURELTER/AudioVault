//! Catalog entities: persons, series, books, variants, copies, media files,
//! observations, covers, FB2 records; plus denormalized summary and FTS upkeep.

use audiovault_core::identity::{book_identity_key, people_key, search_aliases};
use audiovault_core::metadata::{Observation, Resolved};
use audiovault_core::model::{CopyTech, MetadataField, MetadataSource};
use audiovault_core::names::{PersonName, split_people};
use audiovault_core::normalize::{search_norm, skeleton, sort_key, title_key};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{OptionalExt, Result, new_uuid, now_ts};

// ---------------------------------------------------------------- persons

/// Find or create a person for a display name; upgrades the stored display
/// form when a more complete spelling arrives ("Достоевский Ф.М." → "Фёдор Достоевский").
pub fn ensure_person(conn: &Connection, raw: &str, source: &str) -> Result<i64> {
    let (tr, was_tr) = audiovault_core::translit::auto_reverse_translit(raw);
    let effective_raw = if was_tr { tr.as_str() } else { raw };
    let parsed = PersonName::parse(effective_raw);
    let key = parsed.key();
    let surname_key = parsed.surname_key();
    let display = parsed.display();
    let mut found: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, name FROM person WHERE person_key = ?1 ORDER BY id LIMIT 1",
            [&key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if found.is_none() {
        // Compatible initials: "surname|" matches any single person with that surname,
        // and "surname|f" matches a stored "surname|".
        let mut st =
            conn.prepare("SELECT id, name, person_key FROM person WHERE surname_key = ?1 LIMIT 5")?;
        let cands: Vec<(i64, String, String)> = st
            .query_map([&surname_key], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let initial = key.split('|').nth(1).unwrap_or("");
        let compatible: Vec<&(i64, String, String)> = cands
            .iter()
            .filter(|(_, _, k)| {
                let other = k.split('|').nth(1).unwrap_or("");
                initial.is_empty() || other.is_empty() || other == initial
            })
            .collect();
        if compatible.len() == 1 {
            found = Some((compatible[0].0, compatible[0].1.clone()));
        }
    }
    let id = match found {
        Some((id, stored)) => {
            let stored_parsed = PersonName::parse(&stored);
            if parsed.completeness() > stored_parsed.completeness() {
                conn.execute(
                    "UPDATE person SET name = ?2, sort_name = ?3, person_key = ?4 WHERE id = ?1",
                    params![id, display, sort_key(&parsed.sort_name()), key],
                )?;
            }
            id
        }
        None => {
            conn.execute(
                "INSERT INTO person (name, sort_name, person_key, surname_key, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![display, sort_key(&parsed.sort_name()), key, surname_key, now_ts()],
            )?;
            conn.last_insert_rowid()
        }
    };
    let norm = search_norm(raw);
    if !norm.is_empty() {
        conn.execute(
            "INSERT OR IGNORE INTO person_alias (person_id, alias, norm, source) VALUES (?1, ?2, ?3, ?4)",
            params![id, raw.trim(), norm, source],
        )?;
    }
    if was_tr {
        let tr_norm = search_norm(effective_raw);
        if !tr_norm.is_empty() {
            conn.execute(
                "INSERT OR IGNORE INTO person_alias (person_id, alias, norm, source) VALUES (?1, ?2, ?3, ?4)",
                params![id, effective_raw.trim(), tr_norm, source],
            )?;
        }
    }
    Ok(id)
}

pub fn person_name(conn: &Connection, id: i64) -> Result<String> {
    Ok(conn.query_row("SELECT name FROM person WHERE id = ?1", [id], |r| r.get(0))?)
}

// ---------------------------------------------------------------- series

pub fn ensure_series(conn: &Connection, name: &str) -> Result<i64> {
    let (clean_name, _) = audiovault_core::translit::auto_reverse_translit(name);
    let key = title_key(&clean_name);
    if let Some(id) = conn
        .query_row("SELECT id FROM series WHERE series_key = ?1", [&key], |r| {
            r.get(0)
        })
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO series (name, series_key, created_at) VALUES (?1, ?2, ?3)",
        params![clean_name.trim(), key, now_ts()],
    )?;
    Ok(conn.last_insert_rowid())
}

// ---------------------------------------------------------------- books

pub fn find_book_by_identity(conn: &Connection, identity_key: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM book WHERE identity_key = ?1 ORDER BY id LIMIT 1",
            [identity_key],
            |r| r.get(0),
        )
        .optional()?)
}

/// Create a book with the given canonical title and authors.
pub fn create_book(conn: &Connection, title: &str, authors: &[String]) -> Result<i64> {
    let now = now_ts();
    let (clean_title, was_tr) = audiovault_core::translit::auto_reverse_translit(title);
    let effective_title = if was_tr { &clean_title } else { title };
    let authors_cleaned: Vec<String> = authors
        .iter()
        .map(|a| {
            let (ca, _) = audiovault_core::translit::auto_reverse_translit(a);
            ca
        })
        .collect();
    conn.execute(
        "INSERT INTO book (uuid, title, sort_title, title_key, identity_key, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            new_uuid(),
            effective_title,
            sort_key(effective_title),
            title_key(effective_title),
            book_identity_key(&authors_cleaned, effective_title),
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    set_book_authors(conn, id, &authors_cleaned, "inferred")?;
    if was_tr {
        let _ = add_book_alias(conn, id, title, "translit", "reverse_translit");
    }
    Ok(id)
}

pub fn set_book_authors(
    conn: &Connection,
    book_id: i64,
    authors: &[String],
    source: &str,
) -> Result<()> {
    conn.execute(
        "DELETE FROM book_author WHERE book_id = ?1 AND role = 'author'",
        [book_id],
    )?;
    let mut names = Vec::new();
    let mut sorts = Vec::new();
    let people: Vec<String> = authors.iter().flat_map(|a| split_people(a)).collect();
    for (pos, a) in people.iter().enumerate() {
        let pid = ensure_person(conn, a, source)?;
        conn.execute(
            "INSERT OR IGNORE INTO book_author (book_id, person_id, role, position) VALUES (?1, ?2, 'author', ?3)",
            params![book_id, pid, pos as i64],
        )?;
        let (name, sort): (String, String) = conn.query_row(
            "SELECT name, sort_name FROM person WHERE id = ?1",
            [pid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        names.push(name);
        sorts.push(sort);
    }
    conn.execute(
        "UPDATE book SET authors_display = ?2, sort_author = ?3, authors_key = ?4, updated_at = ?5 WHERE id = ?1",
        params![book_id, names.join(", "), sorts.first().cloned().unwrap_or_default(), people_key(&people), now_ts()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BookCore {
    pub id: i64,
    pub title: String,
    pub authors: Vec<String>,
    pub identity_key: String,
    pub series_name: Option<String>,
    pub series_index: Option<f64>,
}

pub fn book_core(conn: &Connection, book_id: i64) -> Result<Option<BookCore>> {
    let row = conn
        .query_row(
            "SELECT id, title, identity_key, series_name, series_index FROM book WHERE id = ?1",
            [book_id],
            |r| {
                Ok(BookCore {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    identity_key: r.get(2)?,
                    series_name: r.get(3)?,
                    series_index: r.get(4)?,
                    authors: vec![],
                })
            },
        )
        .optional()?;
    let Some(mut b) = row else { return Ok(None) };
    b.authors = book_author_names(conn, book_id)?;
    Ok(Some(b))
}

pub fn book_author_names(conn: &Connection, book_id: i64) -> Result<Vec<String>> {
    let mut st = conn.prepare_cached(
        "SELECT p.name FROM book_author ba JOIN person p ON p.id = ba.person_id
         WHERE ba.book_id = ?1 AND ba.role = 'author' ORDER BY ba.position",
    )?;
    Ok(st
        .query_map([book_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?)
}

/// Apply resolved canonical metadata to a book. Returns `true` when the
/// identity key changed (caller may need to merge with another book).
pub fn apply_canonical(conn: &Connection, book_id: i64, r: &Resolved) -> Result<bool> {
    let before: String = conn.query_row(
        "SELECT identity_key FROM book WHERE id = ?1",
        [book_id],
        |x| x.get(0),
    )?;
    if let Some(title) = r.get(MetadataField::Title) {
        let (clean_title, was_tr) = audiovault_core::translit::auto_reverse_translit(title);
        let title_to_use = if was_tr { &clean_title } else { title };
        conn.execute(
            "UPDATE book SET title = ?2, sort_title = ?3, title_key = ?4 WHERE id = ?1",
            params![
                book_id,
                title_to_use,
                sort_key(title_to_use),
                title_key(title_to_use)
            ],
        )?;
        if was_tr {
            let _ = add_book_alias(conn, book_id, title, "translit", "reverse_translit");
        }
    }
    if let Some(authors) = r.get(MetadataField::Author) {
        let list: Vec<String> = split_people(authors);
        let src = r
            .source(MetadataField::Author)
            .map_or("inferred", |s| s.as_str());
        set_book_authors(conn, book_id, &list, src)?;
    }
    match r.get(MetadataField::Series) {
        Some(series) => {
            let sid = ensure_series(conn, series)?;
            conn.execute(
                "UPDATE book SET series_id = ?2, series_name = (SELECT name FROM series WHERE id = ?2) WHERE id = ?1",
                params![book_id, sid],
            )?;
        }
        None => {
            conn.execute(
                "UPDATE book SET series_id = NULL, series_name = NULL WHERE id = ?1",
                [book_id],
            )?;
        }
    }
    let idx: Option<f64> = r
        .get(MetadataField::SeriesIndex)
        .and_then(|s| s.replace(',', ".").parse().ok());
    let year: Option<i64> = r
        .get(MetadataField::Year)
        .and_then(|s| s.get(..4).and_then(|y| y.parse().ok()));
    conn.execute(
        "UPDATE book SET series_index = ?2, year = ?3, language = ?4, genre = ?5, description = ?6, publisher = ?7,
            metadata_confidence = ?8, updated_at = ?9 WHERE id = ?1",
        params![
            book_id,
            idx,
            year,
            r.get(MetadataField::Language),
            r.get(MetadataField::Genre),
            r.get(MetadataField::Description),
            r.get(MetadataField::Publisher),
            r.confidence() as f64,
            now_ts()
        ],
    )?;
    // Recompute identity from the (possibly new) title and authors.
    let core = book_core(conn, book_id)?
        .ok_or_else(|| crate::DbError::NotFound(format!("book {book_id}")))?;
    let identity = book_identity_key(&core.authors, &core.title);
    conn.execute(
        "UPDATE book SET identity_key = ?2 WHERE id = ?1",
        params![book_id, identity],
    )?;
    add_book_alias(conn, book_id, &core.title, "title", "canonical")?;
    Ok(identity != before)
}

pub fn add_book_alias(
    conn: &Connection,
    book_id: i64,
    alias: &str,
    kind: &str,
    source: &str,
) -> Result<()> {
    let norm = search_norm(alias);
    if norm.is_empty() {
        return Ok(());
    }
    conn.execute(
        "INSERT OR IGNORE INTO book_alias (book_id, alias, norm, kind, source) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![book_id, alias.trim(), norm, kind, source],
    )?;
    for a in search_aliases(alias) {
        conn.execute(
            "INSERT OR IGNORE INTO book_alias (book_id, alias, norm, kind, source) VALUES (?1, ?2, ?3, 'translit', ?4)",
            params![book_id, a, search_norm(&a), source],
        )?;
    }
    Ok(())
}

/// Merge book `from` into `into` (variants, copies, observations, aliases move).
pub fn merge_books(conn: &Connection, from: i64, into: i64) -> Result<()> {
    if from == into {
        return Ok(());
    }
    conn.execute(
        "UPDATE audio_variant SET book_id = ?2 WHERE book_id = ?1",
        params![from, into],
    )?;
    conn.execute(
        "UPDATE book_copy SET book_id = ?2 WHERE book_id = ?1",
        params![from, into],
    )?;
    conn.execute(
        "UPDATE metadata_observation SET book_id = ?2 WHERE book_id = ?1",
        params![from, into],
    )?;
    conn.execute(
        "UPDATE OR IGNORE book_alias SET book_id = ?2 WHERE book_id = ?1",
        params![from, into],
    )?;
    conn.execute(
        "UPDATE cover SET book_id = ?2 WHERE book_id = ?1",
        params![from, into],
    )?;
    conn.execute(
        "UPDATE OR IGNORE metadata_issue SET book_id = ?2 WHERE book_id = ?1",
        params![from, into],
    )?;
    conn.execute("DELETE FROM book_fts WHERE rowid = ?1", [from])?;
    conn.execute("DELETE FROM book WHERE id = ?1", [from])?;
    merge_variants_with_same_narrator(conn, into)?;
    Ok(())
}

fn merge_variants_with_same_narrator(conn: &Connection, book_id: i64) -> Result<()> {
    let mut st =
        conn.prepare("SELECT id, narrator_key FROM audio_variant WHERE book_id = ?1 ORDER BY id")?;
    let vars: Vec<(i64, String)> = st
        .query_map([book_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for (id, key) in vars {
        match seen.get(&key) {
            Some(&keep) => {
                conn.execute(
                    "UPDATE book_copy SET variant_id = ?2 WHERE variant_id = ?1",
                    params![id, keep],
                )?;
                conn.execute("DELETE FROM audio_variant WHERE id = ?1", [id])?;
            }
            None => {
                seen.insert(key, id);
            }
        }
    }
    Ok(())
}

/// Delete books (and variants) that no longer have any copy.
pub fn delete_orphan_books(conn: &Connection) -> Result<usize> {
    conn.execute(
        "DELETE FROM audio_variant WHERE id NOT IN (SELECT DISTINCT variant_id FROM book_copy)",
        [],
    )?;
    let mut st = conn
        .prepare("SELECT id FROM book WHERE id NOT IN (SELECT DISTINCT book_id FROM book_copy)")?;
    let ids: Vec<i64> = st
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for id in &ids {
        conn.execute("DELETE FROM book_fts WHERE rowid = ?1", [id])?;
        conn.execute("DELETE FROM book WHERE id = ?1", [id])?;
    }
    Ok(ids.len())
}

// ---------------------------------------------------------------- variants

/// Find or create the variant of `book_id` for the given narrators.
///
/// * no narrators → preferred/first variant (created if none)
/// * matching narrator key → that variant
/// * a variant with unknown narrator exists → it adopts these narrators
/// * otherwise → a new variant (same book, different narrator)
pub fn ensure_variant(conn: &Connection, book_id: i64, narrators: &[String]) -> Result<i64> {
    let key = people_key(narrators);
    let mut st =
        conn.prepare("SELECT id, narrator_key FROM audio_variant WHERE book_id = ?1 ORDER BY id")?;
    let vars: Vec<(i64, String)> = st
        .query_map([book_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if key.is_empty() {
        let preferred: Option<i64> = conn.query_row(
            "SELECT preferred_variant_id FROM book WHERE id = ?1",
            [book_id],
            |r| r.get(0),
        )?;
        if let Some(p) = preferred.filter(|p| vars.iter().any(|(id, _)| id == p)) {
            return Ok(p);
        }
        if let Some((id, _)) = vars.first() {
            return Ok(*id);
        }
        return create_variant(conn, book_id, &[]);
    }
    if let Some((id, _)) = vars.iter().find(|(_, k)| *k == key) {
        return Ok(*id);
    }
    if let Some((id, _)) = vars.iter().find(|(_, k)| k.is_empty()) {
        set_variant_narrators(conn, *id, narrators)?;
        return Ok(*id);
    }
    create_variant(conn, book_id, narrators)
}

fn create_variant(conn: &Connection, book_id: i64, narrators: &[String]) -> Result<i64> {
    conn.execute(
        "INSERT INTO audio_variant (book_id, created_at) VALUES (?1, ?2)",
        params![book_id, now_ts()],
    )?;
    let id = conn.last_insert_rowid();
    set_variant_narrators(conn, id, narrators)?;
    conn.execute(
        "UPDATE book SET preferred_variant_id = COALESCE(preferred_variant_id, ?2) WHERE id = ?1",
        params![book_id, id],
    )?;
    Ok(id)
}

pub fn set_variant_narrators(
    conn: &Connection,
    variant_id: i64,
    narrators: &[String],
) -> Result<()> {
    conn.execute(
        "DELETE FROM variant_narrator WHERE variant_id = ?1",
        [variant_id],
    )?;
    let people: Vec<String> = narrators.iter().flat_map(|n| split_people(n)).collect();
    let mut names = Vec::new();
    for (pos, n) in people.iter().enumerate() {
        let pid = ensure_person(conn, n, "narrator")?;
        conn.execute(
            "INSERT OR IGNORE INTO variant_narrator (variant_id, person_id, position) VALUES (?1, ?2, ?3)",
            params![variant_id, pid, pos as i64],
        )?;
        names.push(person_name(conn, pid)?);
    }
    conn.execute(
        "UPDATE audio_variant SET narrator_display = ?2, narrator_key = ?3 WHERE id = ?1",
        params![variant_id, names.join(", "), people_key(&people)],
    )?;
    Ok(())
}

/// Variant for a newly discovered copy: the variant with the same narrator
/// key (including the empty "unknown narrator" key), or a new one.
pub fn variant_for_new_copy(conn: &Connection, book_id: i64, narrators: &[String]) -> Result<i64> {
    let people: Vec<String> = narrators.iter().flat_map(|n| split_people(n)).collect();
    let key = people_key(&people);
    let found: Option<i64> = conn
        .query_row(
            "SELECT id FROM audio_variant WHERE book_id = ?1 AND narrator_key = ?2 ORDER BY id LIMIT 1",
            params![book_id, key],
            |r| r.get(0),
        )
        .optional()?;
    match found {
        Some(v) => Ok(v),
        None => create_variant(conn, book_id, &people),
    }
}

/// Put `copy_id` into the variant of its book matching `narrators`.
///
/// * a variant with the same narrator key exists → move the copy there;
/// * the copy's own variant has no narrator and holds only this copy → label it;
/// * otherwise → create a new variant for the copy.
///
/// Returns the variant id. Never relabels a variant shared with other copies.
pub fn assign_copy_narrators(conn: &Connection, copy_id: i64, narrators: &[String]) -> Result<i64> {
    let (book_id, current): (i64, i64) = conn.query_row(
        "SELECT book_id, variant_id FROM book_copy WHERE id = ?1",
        [copy_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let key = people_key(narrators);
    if key.is_empty() {
        return Ok(current);
    }
    let target: Option<i64> = conn
        .query_row(
            "SELECT id FROM audio_variant WHERE book_id = ?1 AND narrator_key = ?2 ORDER BY id LIMIT 1",
            params![book_id, key],
            |r| r.get(0),
        )
        .optional()?;
    let target = match target {
        Some(t) => t,
        None => {
            let (cur_key, sharing): (String, i64) = conn.query_row(
                "SELECT v.narrator_key, (SELECT COUNT(*) FROM book_copy WHERE variant_id = v.id AND id <> ?2)
                 FROM audio_variant v WHERE v.id = ?1",
                params![current, copy_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if cur_key.is_empty() && sharing == 0 {
                set_variant_narrators(conn, current, narrators)?;
                current
            } else {
                create_variant(conn, book_id, narrators)?
            }
        }
    };
    if target != current {
        conn.execute(
            "UPDATE book_copy SET variant_id = ?2 WHERE id = ?1",
            params![copy_id, target],
        )?;
        conn.execute("UPDATE audio_variant SET preferred_copy_id = NULL WHERE id = ?1 AND preferred_copy_id = ?2", params![current, copy_id])?;
    }
    Ok(target)
}

// ---------------------------------------------------------------- copies

#[derive(Debug, Clone, Default)]
pub struct CopyEnumeration {
    pub volume_id: i64,
    pub root_id: Option<i64>,
    pub rel_path: String,
    pub is_single_file: bool,
    pub file_count: u32,
    pub audio_file_count: u32,
    pub total_size: u64,
    pub audio_size: u64,
    pub quick_signature: String,
    pub detection_confidence: f32,
    pub evidence_json: String,
    pub scan_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopyRow {
    pub id: i64,
    pub book_id: i64,
    pub variant_id: i64,
    pub volume_id: i64,
    pub rel_path: String,
    pub state: String,
    pub quick_signature: Option<String>,
    pub analysis_level: i64,
    pub is_single_file: bool,
}

const COPY_COLS: &str = "id, book_id, variant_id, volume_id, rel_path, state, quick_signature, analysis_level, is_single_file";

fn row_to_copy(r: &rusqlite::Row<'_>) -> rusqlite::Result<CopyRow> {
    Ok(CopyRow {
        id: r.get(0)?,
        book_id: r.get(1)?,
        variant_id: r.get(2)?,
        volume_id: r.get(3)?,
        rel_path: r.get(4)?,
        state: r.get(5)?,
        quick_signature: r.get(6)?,
        analysis_level: r.get(7)?,
        is_single_file: r.get::<_, i64>(8)? != 0,
    })
}

pub fn find_copy(conn: &Connection, volume_id: i64, rel_path: &str) -> Result<Option<CopyRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COPY_COLS} FROM book_copy WHERE volume_id = ?1 AND rel_path = ?2"),
            params![volume_id, rel_path],
            row_to_copy,
        )
        .optional()?)
}

pub fn get_copy(conn: &Connection, copy_id: i64) -> Result<Option<CopyRow>> {
    Ok(conn
        .query_row(
            &format!("SELECT {COPY_COLS} FROM book_copy WHERE id = ?1"),
            [copy_id],
            row_to_copy,
        )
        .optional()?)
}

pub fn copies_of_book(conn: &Connection, book_id: i64) -> Result<Vec<CopyRow>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COPY_COLS} FROM book_copy WHERE book_id = ?1 ORDER BY id"
    ))?;
    Ok(st
        .query_map([book_id], row_to_copy)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn copies_needing_analysis(
    conn: &Connection,
    volume_id: i64,
    below_level: i64,
    limit: i64,
) -> Result<Vec<CopyRow>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COPY_COLS} FROM book_copy WHERE volume_id = ?1 AND state = 'present' AND analysis_level < ?2 ORDER BY id LIMIT ?3"
    ))?;
    Ok(st
        .query_map(params![volume_id, below_level, limit], row_to_copy)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn insert_copy(
    conn: &Connection,
    variant_id: i64,
    book_id: i64,
    e: &CopyEnumeration,
) -> Result<i64> {
    let now = now_ts();
    conn.execute(
        "INSERT INTO book_copy (uuid, variant_id, book_id, volume_id, root_id, rel_path, is_single_file, state, file_count,
            audio_file_count, total_size, audio_size, quick_signature, detection_confidence, detection_evidence,
            first_seen_at, last_seen_at, last_scan_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'present', ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, ?16)",
        params![
            new_uuid(),
            variant_id,
            book_id,
            e.volume_id,
            e.root_id,
            e.rel_path,
            e.is_single_file as i64,
            e.file_count,
            e.audio_file_count,
            e.total_size,
            e.audio_size,
            e.quick_signature,
            e.detection_confidence as f64,
            e.evidence_json,
            now,
            e.scan_id
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Update a re-enumerated copy. Resets analysis when the file set changed.
pub fn update_copy_enumeration(
    conn: &Connection,
    copy_id: i64,
    e: &CopyEnumeration,
    files_changed: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE book_copy SET state = 'present', missing_since = NULL, last_seen_at = ?2, last_scan_id = ?3,
            file_count = ?4, audio_file_count = ?5, total_size = ?6, audio_size = ?7, quick_signature = ?8,
            root_id = COALESCE(?9, root_id),
            analysis_level = CASE WHEN ?10 = 1 THEN 0 ELSE analysis_level END
         WHERE id = ?1",
        params![
            copy_id,
            now_ts(),
            e.scan_id,
            e.file_count,
            e.audio_file_count,
            e.total_size,
            e.audio_size,
            e.quick_signature,
            e.root_id,
            files_changed as i64
        ],
    )?;
    Ok(())
}

pub fn move_copy(conn: &Connection, copy_id: i64, variant_id: i64, book_id: i64) -> Result<()> {
    conn.execute(
        "UPDATE book_copy SET variant_id = ?2, book_id = ?3 WHERE id = ?1",
        params![copy_id, variant_id, book_id],
    )?;
    conn.execute(
        "UPDATE metadata_observation SET book_id = ?2 WHERE copy_id = ?1",
        params![copy_id, book_id],
    )?;
    conn.execute(
        "UPDATE cover SET book_id = ?2 WHERE copy_id = ?1",
        params![copy_id, book_id],
    )?;
    conn.execute(
        "UPDATE OR IGNORE metadata_issue SET book_id = ?2 WHERE copy_id = ?1",
        params![copy_id, book_id],
    )?;
    Ok(())
}

pub fn update_copy_tech(
    conn: &Connection,
    copy_id: i64,
    t: &CopyTech,
    level: i64,
    narrator_hint: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE book_copy SET total_duration_ms = ?2, primary_codec = ?3, container = ?4, avg_bitrate_kbps = ?5,
            sample_rate = ?6, channels = ?7, is_lossless = ?8, has_cover = ?9, has_fb2 = ?10, has_chapters = ?11,
            probe_errors = ?12, analysis_level = MAX(analysis_level, ?13), narrator_hint = ?14
         WHERE id = ?1",
        params![
            copy_id,
            t.total_duration_ms,
            t.primary_codec,
            t.container,
            t.avg_bitrate_kbps,
            t.sample_rate,
            t.channels,
            t.is_lossless as i64,
            t.has_cover as i64,
            t.has_fb2 as i64,
            t.has_chapters as i64,
            t.probe_errors,
            level,
            narrator_hint
        ],
    )?;
    Ok(())
}

pub fn set_copy_analysis_level(conn: &Connection, copy_id: i64, level: i64) -> Result<()> {
    conn.execute(
        "UPDATE book_copy SET analysis_level = MAX(analysis_level, ?2) WHERE id = ?1",
        params![copy_id, level],
    )?;
    Ok(())
}

pub fn set_copy_quality(conn: &Connection, copy_id: i64, score: f32) -> Result<()> {
    conn.execute(
        "UPDATE book_copy SET quality_score = ?2 WHERE id = ?1",
        params![copy_id, score as f64],
    )?;
    Ok(())
}

pub fn set_copy_path(
    conn: &Connection,
    copy_id: i64,
    volume_id: i64,
    rel_path: &str,
) -> Result<()> {
    let old: (i64, String) = conn.query_row(
        "SELECT volume_id, rel_path FROM book_copy WHERE id = ?1",
        [copy_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    conn.execute(
        "UPDATE book_copy SET volume_id = ?2, rel_path = ?3 WHERE id = ?1",
        params![copy_id, volume_id, rel_path],
    )?;
    // Re-root media files: replace the old prefix with the new one.
    let mut st = conn.prepare("SELECT id, rel_path FROM media_file WHERE copy_id = ?1")?;
    let files: Vec<(i64, String)> = st
        .query_map([copy_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, p) in files {
        let suffix = p.strip_prefix(&old.1).unwrap_or(&p).trim_start_matches('/');
        let np = if suffix.is_empty() {
            rel_path.to_string()
        } else if rel_path.is_empty() {
            suffix.to_string()
        } else {
            format!("{rel_path}/{suffix}")
        };
        conn.execute(
            "UPDATE media_file SET volume_id = ?2, rel_path = ?3 WHERE id = ?1",
            params![id, volume_id, np],
        )?;
    }
    Ok(())
}

pub fn set_preferred_copy(conn: &Connection, book_id: i64, copy_id: i64) -> Result<()> {
    let variant: i64 = conn.query_row(
        "SELECT variant_id FROM book_copy WHERE id = ?1 AND book_id = ?2",
        params![copy_id, book_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE audio_variant SET preferred_copy_id = ?2 WHERE id = ?1",
        params![variant, copy_id],
    )?;
    conn.execute(
        "UPDATE book SET preferred_copy_id = ?2, preferred_variant_id = ?3 WHERE id = ?1",
        params![book_id, copy_id, variant],
    )?;
    refresh_book(conn, book_id)
}

/// Remove a copy from the catalog only (files untouched).
pub fn remove_copy_from_catalog(conn: &Connection, copy_id: i64) -> Result<Option<i64>> {
    let book: Option<i64> = conn
        .query_row(
            "SELECT book_id FROM book_copy WHERE id = ?1",
            [copy_id],
            |r| r.get(0),
        )
        .optional()?;
    conn.execute("DELETE FROM book_copy WHERE id = ?1", [copy_id])?;
    delete_orphan_books(conn)?;
    if let Some(b) = book
        && book_exists(conn, b)?
    {
        conn.execute(
            "UPDATE book SET preferred_copy_id = NULL WHERE id = ?1 AND preferred_copy_id = ?2",
            params![b, copy_id],
        )?;
        refresh_book(conn, b)?;
        return Ok(Some(b));
    }
    Ok(None)
}

pub fn book_exists(conn: &Connection, book_id: i64) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM book WHERE id = ?1)",
        [book_id],
        |r| r.get::<_, i64>(0),
    )? != 0)
}

// ---------------------------------------------------------------- media files

#[derive(Debug, Clone, Default)]
pub struct MediaEnumeration {
    pub rel_path: String,
    pub file_name: String,
    pub ext: String,
    pub kind: String,
    pub size: u64,
    pub mtime: i64,
    pub sort_index: i64,
}

/// Insert or refresh a media file. Returns (id, content_changed).
/// Unchanged (same size + mtime) files keep probe results and hashes.
pub fn upsert_media(
    conn: &Connection,
    copy_id: i64,
    volume_id: i64,
    m: &MediaEnumeration,
    scan_id: Option<i64>,
) -> Result<(i64, bool)> {
    let existing: Option<(i64, i64, i64)> = conn
        .query_row(
            "SELECT id, size, mtime FROM media_file WHERE volume_id = ?1 AND rel_path = ?2",
            params![volume_id, m.rel_path],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    match existing {
        Some((id, size, mtime)) if size as u64 == m.size && mtime == m.mtime => {
            conn.execute(
                "UPDATE media_file SET copy_id = ?2, sort_index = ?3, state = 'present', last_scan_id = ?4 WHERE id = ?1",
                params![id, copy_id, m.sort_index, scan_id],
            )?;
            Ok((id, false))
        }
        Some((id, ..)) => {
            conn.execute(
                "UPDATE media_file SET copy_id = ?2, sort_index = ?3, state = 'present', last_scan_id = ?4, size = ?5, mtime = ?6,
                    duration_ms = NULL, codec = NULL, bitrate_kbps = NULL, probed_at = NULL, probe_error = NULL,
                    quick_hash = NULL, full_hash = NULL, hashed_size = NULL, hashed_mtime = NULL
                 WHERE id = ?1",
                params![id, copy_id, m.sort_index, scan_id, m.size, m.mtime],
            )?;
            Ok((id, true))
        }
        None => {
            conn.execute(
                "INSERT INTO media_file (copy_id, volume_id, rel_path, file_name, ext, kind, size, mtime, sort_index, last_scan_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![copy_id, volume_id, m.rel_path, m.file_name, m.ext, m.kind, m.size, m.mtime, m.sort_index, scan_id],
            )?;
            Ok((conn.last_insert_rowid(), true))
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MediaRow {
    pub id: i64,
    pub copy_id: i64,
    pub rel_path: String,
    pub file_name: String,
    pub ext: String,
    pub kind: String,
    pub size: u64,
    pub mtime: i64,
    pub sort_index: i64,
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub track_no: Option<u32>,
    pub tag_title: Option<String>,
    pub tag_artist: Option<String>,
    pub tag_album: Option<String>,
    pub tag_album_artist: Option<String>,
    pub tag_composer: Option<String>,
    pub tag_genre: Option<String>,
    pub tag_year: Option<i32>,
    pub tag_comment: Option<String>,
    pub has_embedded_cover: bool,
    pub chapter_count: u32,
    pub probed: bool,
    pub probe_error: Option<String>,
    pub quick_hash: Option<String>,
    pub full_hash: Option<String>,
    pub state: String,
    pub tag_narrator: Option<String>,
    pub tag_series: Option<String>,
    pub tag_series_index: Option<String>,
    pub tag_description: Option<String>,
    pub tag_language: Option<String>,
    pub tag_publisher: Option<String>,
    pub container: Option<String>,
}

pub fn media_for_copy(conn: &Connection, copy_id: i64) -> Result<Vec<MediaRow>> {
    let mut st = conn.prepare_cached(
        "SELECT id, copy_id, rel_path, file_name, ext, kind, size, mtime, sort_index, duration_ms, codec, bitrate_kbps,
                sample_rate, channels, track_no, tag_title, tag_artist, tag_album, tag_album_artist, tag_composer, tag_genre,
                tag_year, tag_comment, has_embedded_cover, chapter_count, probed_at IS NOT NULL, probe_error, quick_hash, full_hash, state,
                tag_narrator, tag_series, tag_series_index, tag_description, tag_language, tag_publisher, container
         FROM media_file WHERE copy_id = ?1 ORDER BY sort_index, rel_path",
    )?;
    let rows = st
        .query_map([copy_id], |r| {
            Ok(MediaRow {
                id: r.get(0)?,
                copy_id: r.get(1)?,
                rel_path: r.get(2)?,
                file_name: r.get(3)?,
                ext: r.get(4)?,
                kind: r.get(5)?,
                size: r.get(6)?,
                mtime: r.get(7)?,
                sort_index: r.get(8)?,
                duration_ms: r.get(9)?,
                codec: r.get(10)?,
                bitrate_kbps: r.get(11)?,
                sample_rate: r.get(12)?,
                channels: r.get(13)?,
                track_no: r.get(14)?,
                tag_title: r.get(15)?,
                tag_artist: r.get(16)?,
                tag_album: r.get(17)?,
                tag_album_artist: r.get(18)?,
                tag_composer: r.get(19)?,
                tag_genre: r.get(20)?,
                tag_year: r.get(21)?,
                tag_comment: r.get(22)?,
                has_embedded_cover: r.get::<_, i64>(23)? != 0,
                chapter_count: r.get(24)?,
                probed: r.get(25)?,
                probe_error: r.get(26)?,
                quick_hash: r.get(27)?,
                full_hash: r.get(28)?,
                state: r.get(29)?,
                tag_narrator: r.get(30)?,
                tag_series: r.get(31)?,
                tag_series_index: r.get(32)?,
                tag_description: r.get(33)?,
                tag_language: r.get(34)?,
                tag_publisher: r.get(35)?,
                container: r.get(36)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Default)]
pub struct ProbeUpdate {
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub comment: Option<String>,
    pub has_embedded_cover: bool,
    pub chapter_count: u32,
    pub error: Option<String>,
    pub narrator: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub container: Option<String>,
}

pub fn update_media_probe(conn: &Connection, media_id: i64, p: &ProbeUpdate) -> Result<()> {
    conn.execute(
        "UPDATE media_file SET duration_ms = ?2, codec = ?3, bitrate_kbps = ?4, sample_rate = ?5, channels = ?6,
            track_no = ?7, disc_no = ?8, tag_title = ?9, tag_artist = ?10, tag_album = ?11, tag_album_artist = ?12,
            tag_composer = ?13, tag_genre = ?14, tag_year = ?15, tag_comment = ?16, has_embedded_cover = ?17,
            chapter_count = ?18, probe_error = ?19, probed_at = ?20, tag_narrator = ?21, tag_series = ?22,
            tag_series_index = ?23, tag_description = ?24, tag_language = ?25, tag_publisher = ?26, container = ?27
         WHERE id = ?1",
        params![
            media_id,
            p.duration_ms,
            p.codec,
            p.bitrate_kbps,
            p.sample_rate,
            p.channels,
            p.track_no,
            p.disc_no,
            p.title,
            p.artist,
            p.album,
            p.album_artist,
            p.composer,
            p.genre,
            p.year,
            p.comment,
            p.has_embedded_cover as i64,
            p.chapter_count,
            p.error,
            now_ts(),
            p.narrator,
            p.series,
            p.series_index,
            p.description,
            p.language,
            p.publisher,
            p.container
        ],
    )?;
    Ok(())
}

pub fn update_media_hash(
    conn: &Connection,
    media_id: i64,
    quick: Option<&str>,
    full: Option<&str>,
    size: u64,
    mtime: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE media_file SET quick_hash = COALESCE(?2, quick_hash), full_hash = COALESCE(?3, full_hash),
            hashed_size = ?4, hashed_mtime = ?5 WHERE id = ?1",
        params![media_id, quick, full, size, mtime],
    )?;
    Ok(())
}

/// After a *complete* scan of `root_prefix` on an online volume, mark
/// everything not seen in `scan_id` as missing. Returns (copies, files) marked.
pub fn mark_missing_after_scan(
    conn: &Connection,
    volume_id: i64,
    root_prefix: &str,
    scan_id: i64,
) -> Result<(usize, usize)> {
    let now = now_ts();
    let prefix_slash = format!("{root_prefix}/");
    let under = "(?2 = '' OR rel_path = ?2 OR substr(rel_path, 1, length(?3)) = ?3)";
    let files = conn.execute(
        &format!(
            "UPDATE media_file SET state = 'missing' WHERE volume_id = ?1 AND state = 'present'
             AND (last_scan_id IS NULL OR last_scan_id <> ?4) AND {under}"
        ),
        params![volume_id, root_prefix, prefix_slash, scan_id],
    )?;
    let copies = conn.execute(
        &format!(
            "UPDATE book_copy SET state = 'missing', missing_since = ?5 WHERE volume_id = ?1 AND state = 'present'
             AND (last_scan_id IS NULL OR last_scan_id <> ?4) AND {under}"
        ),
        params![volume_id, root_prefix, prefix_slash, scan_id, now],
    )?;
    Ok((copies, files))
}

/// Missing copies on a volume (for confirmation UI / move inference).
pub fn missing_copies(conn: &Connection, volume_id: Option<i64>) -> Result<Vec<CopyRow>> {
    let mut st = conn.prepare(&format!(
        "SELECT {COPY_COLS} FROM book_copy WHERE state = 'missing' AND (?1 IS NULL OR volume_id = ?1) ORDER BY missing_since"
    ))?;
    Ok(st
        .query_map([volume_id], row_to_copy)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Stamp everything under `prefix` as seen in `scan_id` without changing its
/// state. Used for subtrees that could not be read during a scan so they are
/// never marked missing because of a transient error.
pub fn touch_subtree(conn: &Connection, volume_id: i64, prefix: &str, scan_id: i64) -> Result<()> {
    let prefix_slash = format!("{prefix}/");
    for table in ["book_copy", "media_file"] {
        conn.execute(
            &format!(
                "UPDATE {table} SET last_scan_id = ?4 WHERE volume_id = ?1
                 AND (?2 = '' OR rel_path = ?2 OR substr(rel_path, 1, length(?3)) = ?3)"
            ),
            params![volume_id, prefix, prefix_slash, scan_id],
        )?;
    }
    Ok(())
}

/// Copies that may be the previous location of a set with `signature`:
/// confirmed-missing copies anywhere, plus not-yet-seen copies on the volume
/// being scanned (the caller checks whether their old path still exists).
pub fn move_candidates(
    conn: &Connection,
    signature: &str,
    volume_id: i64,
    scan_id: i64,
) -> Result<Vec<CopyRow>> {
    if signature.is_empty() {
        return Ok(Vec::new());
    }
    let mut st = conn.prepare_cached(&format!(
        "SELECT {COPY_COLS} FROM book_copy WHERE quick_signature = ?1
           AND (state = 'missing' OR (volume_id = ?2 AND state = 'present' AND (last_scan_id IS NULL OR last_scan_id <> ?3)))
         ORDER BY (state = 'missing') DESC, id LIMIT 8"
    ))?;
    Ok(st
        .query_map(params![signature, volume_id, scan_id], row_to_copy)?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Quality-heuristic inputs for many copies in one query.
pub fn quality_inputs(
    conn: &Connection,
    copy_ids: &[i64],
) -> Result<std::collections::HashMap<i64, audiovault_core::quality::QualityInput>> {
    let mut out = std::collections::HashMap::new();
    if copy_ids.is_empty() {
        return Ok(out);
    }
    let mut st = conn.prepare(
        "SELECT c.id, c.primary_codec, c.avg_bitrate_kbps, c.is_lossless, b.title <> '', b.authors_display <> '',
                v.narrator_key <> '', c.has_chapters, c.has_cover, c.has_fb2, c.audio_file_count, c.probe_errors,
                (b.preferred_copy_id = c.id), s.storage_class
         FROM book_copy c JOIN book b ON b.id = c.book_id JOIN audio_variant v ON v.id = c.variant_id
         JOIN storage_volume s ON s.id = c.volume_id
         WHERE c.id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = st.query_map([crate::ids_json(copy_ids)], |r| {
        let class: String = r.get(13)?;
        Ok((
            r.get::<_, i64>(0)?,
            audiovault_core::quality::QualityInput {
                codec: r.get(1)?,
                bitrate_kbps: r.get(2)?,
                is_lossless: r.get::<_, i64>(3)? != 0,
                has_title: r.get::<_, i64>(4)? != 0,
                has_author: r.get::<_, i64>(5)? != 0,
                has_narrator: r.get::<_, i64>(6)? != 0,
                has_chapters: r.get::<_, i64>(7)? != 0,
                has_cover: r.get::<_, i64>(8)? != 0,
                has_fb2: r.get::<_, i64>(9)? != 0,
                audio_file_count: r.get(10)?,
                probe_errors: r.get(11)?,
                user_preferred: r.get::<_, Option<i64>>(12)?.unwrap_or(0) != 0,
                storage_bias: if class == "removable" || class == "network" {
                    -1.0
                } else {
                    0.0
                },
            },
        ))
    })?;
    for row in rows {
        let (id, q) = row?;
        out.insert(id, q);
    }
    Ok(out)
}

// ---------------------------------------------------------------- observations

/// Replace all observations of the given sources recorded for a copy.
pub fn replace_copy_observations(
    conn: &Connection,
    book_id: i64,
    copy_id: i64,
    sources: &[MetadataSource],
    obs: &[Observation],
) -> Result<()> {
    for s in sources {
        conn.execute(
            "DELETE FROM metadata_observation WHERE copy_id = ?1 AND source = ?2",
            params![copy_id, s.as_str()],
        )?;
    }
    let now = now_ts();
    let mut st = conn.prepare_cached(
        "INSERT INTO metadata_observation (book_id, copy_id, field, value, source, confidence, observed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for o in obs {
        st.execute(params![
            book_id,
            copy_id,
            o.field.as_str(),
            o.value,
            o.source.as_str(),
            o.confidence as f64,
            now
        ])?;
    }
    Ok(())
}

/// Record a manual edit (highest authority, survives rescans: not tied to a copy).
pub fn set_manual_value(
    conn: &Connection,
    book_id: i64,
    field: MetadataField,
    value: Option<&str>,
) -> Result<()> {
    conn.execute(
        "DELETE FROM metadata_observation WHERE book_id = ?1 AND field = ?2 AND source = 'manual'",
        params![book_id, field.as_str()],
    )?;
    if let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) {
        conn.execute(
            "INSERT INTO metadata_observation (book_id, copy_id, field, value, source, confidence, observed_at)
             VALUES (?1, NULL, ?2, ?3, 'manual', 1.0, ?4)",
            params![book_id, field.as_str(), v, now_ts()],
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredObservation {
    pub copy_id: Option<i64>,
    pub observation: Observation,
    pub observed_at: i64,
}

pub fn observations_for_book(conn: &Connection, book_id: i64) -> Result<Vec<StoredObservation>> {
    let mut st = conn.prepare_cached(
        "SELECT mo.copy_id, mo.field, mo.value, mo.source, mo.confidence, mo.observed_at 
         FROM metadata_observation mo 
         LEFT JOIN book_copy c ON c.id = mo.copy_id 
         LEFT JOIN storage_volume v ON v.id = c.volume_id 
         WHERE mo.book_id = ?1 
           AND (mo.copy_id IS NULL OR (c.state <> 'removed' AND NOT (v.is_online = 1 AND c.state = 'missing')))
         ORDER BY mo.id",
    )?;
    let rows = st
        .query_map([book_id], |r| {
            let field: String = r.get(1)?;
            let source: String = r.get(3)?;
            Ok((
                r.get::<_, Option<i64>>(0)?,
                field,
                r.get::<_, String>(2)?,
                source,
                r.get::<_, f64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(copy_id, f, v, s, c, at)| {
            Some(StoredObservation {
                copy_id,
                observation: Observation::new(
                    MetadataField::parse(&f)?,
                    v,
                    MetadataSource::parse(&s)?,
                    c as f32,
                ),
                observed_at: at,
            })
        })
        .collect())
}

/// Observations relevant for resolving the canonical metadata of `book_id`:
/// manual (book-level) observations plus those of the given copies.
pub fn observations_for_resolution(conn: &Connection, book_id: i64) -> Result<Vec<Observation>> {
    Ok(observations_for_book(conn, book_id)?
        .into_iter()
        .map(|o| o.observation)
        .collect())
}

// ---------------------------------------------------------------- ebooks & covers

#[derive(Debug, Clone, Default)]
pub struct EbookRecord {
    pub format: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub genres: Vec<String>,
    pub annotation: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub year: Option<i32>,
    pub document_id: Option<String>,
    pub has_cover: bool,
    pub match_score: Option<f32>,
    pub error: Option<String>,
}

pub fn upsert_ebook(
    conn: &Connection,
    copy_id: i64,
    media_file_id: i64,
    e: &EbookRecord,
) -> Result<()> {
    conn.execute(
        "INSERT INTO ebook_file (copy_id, media_file_id, format, title, authors, series, series_index, genres, annotation,
            language, publisher, year, document_id, has_cover, match_score, parsed_at, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(media_file_id) DO UPDATE SET copy_id = excluded.copy_id, title = excluded.title, authors = excluded.authors,
            series = excluded.series, series_index = excluded.series_index, genres = excluded.genres,
            annotation = excluded.annotation, language = excluded.language, publisher = excluded.publisher,
            year = excluded.year, document_id = excluded.document_id, has_cover = excluded.has_cover,
            match_score = excluded.match_score, parsed_at = excluded.parsed_at, error = excluded.error",
        params![
            copy_id,
            media_file_id,
            e.format,
            e.title,
            e.authors.join("; "),
            e.series,
            e.series_index.map(|v| v as f64),
            e.genres.join(", "),
            e.annotation,
            e.language,
            e.publisher,
            e.year,
            e.document_id,
            e.has_cover as i64,
            e.match_score.map(|v| v as f64),
            now_ts(),
            e.error
        ],
    )?;
    Ok(())
}

pub fn set_copy_cover(
    conn: &Connection,
    book_id: i64,
    copy_id: Option<i64>,
    source: &str,
    source_ref: Option<&str>,
    dims: Option<(u32, u32)>,
    content_hash: Option<&str>,
    thumb_path: Option<&str>,
) -> Result<()> {
    match copy_id {
        Some(c) => conn.execute(
            "DELETE FROM cover WHERE copy_id = ?1 AND source <> 'manual'",
            [c],
        )?,
        None => conn.execute(
            "DELETE FROM cover WHERE book_id = ?1 AND source = 'manual'",
            [book_id],
        )?,
    };
    conn.execute(
        "INSERT INTO cover (book_id, copy_id, source, source_ref, width, height, content_hash, thumb_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![book_id, copy_id, source, source_ref, dims.map(|d| d.0), dims.map(|d| d.1), content_hash, thumb_path, now_ts()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------- summary & FTS

/// Recompute the denormalized book summary and its FTS row.
pub fn refresh_book(conn: &Connection, book_id: i64) -> Result<()> {
    if !book_exists(conn, book_id)? {
        conn.execute("DELETE FROM book_fts WHERE rowid = ?1", [book_id])?;
        return Ok(());
    }
    // Preferred copy: keep if still present, else best present copy by quality.
    let preferred: Option<i64> = conn
        .query_row(
            "SELECT preferred_copy_id FROM book WHERE id = ?1 AND preferred_copy_id IN
            (SELECT id FROM book_copy WHERE book_id = ?1)",
            [book_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let preferred = match preferred {
        Some(p) => Some(p),
        None => conn
            .query_row(
                "SELECT id FROM book_copy WHERE book_id = ?1
                 ORDER BY (state = 'present') DESC, COALESCE(quality_score, 0) DESC, id LIMIT 1",
                [book_id],
                |r| r.get(0),
            )
            .optional()?,
    };
    conn.execute(
        "UPDATE book SET
            preferred_copy_id = ?2,
            preferred_variant_id = COALESCE((SELECT variant_id FROM book_copy WHERE id = ?2), preferred_variant_id),
            variant_count = (SELECT COUNT(*) FROM audio_variant WHERE book_id = ?1),
            copy_count = (SELECT COUNT(*) FROM book_copy WHERE book_id = ?1),
            present_copy_count = (SELECT COUNT(*) FROM book_copy WHERE book_id = ?1 AND state = 'present'),
            narrators_display = COALESCE((SELECT v.narrator_display FROM audio_variant v JOIN book_copy c ON c.variant_id = v.id WHERE c.id = ?2), ''),
            total_duration_ms = COALESCE((SELECT total_duration_ms FROM book_copy WHERE id = ?2), 0),
            total_size = COALESCE((SELECT total_size FROM book_copy WHERE id = ?2), 0),
            format = COALESCE((SELECT UPPER(COALESCE(primary_codec, '')) FROM book_copy WHERE id = ?2), ''),
            has_fb2 = EXISTS(SELECT 1 FROM book_copy WHERE book_id = ?1 AND has_fb2 = 1),
            has_duplicates = EXISTS(SELECT 1 FROM duplicate_candidate d JOIN book_copy c ON c.id IN (d.copy_a, d.copy_b)
                                    WHERE c.book_id = ?1 AND d.status = 'open' AND d.verdict IN ('exact','same_recording','possible')),
            issue_count = (SELECT COUNT(*) FROM metadata_issue WHERE book_id = ?1 AND state = 'open'),
            cover_thumb = (SELECT thumb_path FROM cover WHERE book_id = ?1 AND thumb_path IS NOT NULL
                           ORDER BY (source = 'manual') DESC, (copy_id = ?2) DESC, id DESC LIMIT 1),
            updated_at = ?3
         WHERE id = ?1",
        params![book_id, preferred, now_ts()],
    )?;
    refresh_fts(conn, book_id)
}

pub fn refresh_fts(conn: &Connection, book_id: i64) -> Result<()> {
    let (title, series): (String, Option<String>) = conn.query_row(
        "SELECT title, series_name FROM book WHERE id = ?1",
        [book_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let collect = |sql: &str| -> Result<Vec<String>> {
        let mut st = conn.prepare_cached(sql)?;
        Ok(st
            .query_map([book_id], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    };
    let authors = collect(
        "SELECT p.name FROM book_author ba JOIN person p ON p.id = ba.person_id WHERE ba.book_id = ?1
         UNION SELECT pa.alias FROM book_author ba JOIN person_alias pa ON pa.person_id = ba.person_id WHERE ba.book_id = ?1",
    )?;
    let narrators = collect(
        "SELECT p.name FROM audio_variant v JOIN variant_narrator vn ON vn.variant_id = v.id JOIN person p ON p.id = vn.person_id
         WHERE v.book_id = ?1",
    )?;
    let aliases = collect("SELECT alias FROM book_alias WHERE book_id = ?1")?;
    let norm_join = |v: &[String]| {
        v.iter()
            .map(|s| search_norm(s))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let author_aliases: Vec<String> = authors.iter().flat_map(|a| search_aliases(a)).collect();
    let mut skel_parts: Vec<String> = Vec::new();
    skel_parts.push(skeleton(&title));
    skel_parts.extend(authors.iter().map(|a| skeleton(a)));
    skel_parts.extend(narrators.iter().map(|a| skeleton(a)));
    skel_parts.extend(aliases.iter().map(|a| skeleton(a)));
    if let Some(s) = &series {
        skel_parts.push(skeleton(s));
    }
    conn.execute("DELETE FROM book_fts WHERE rowid = ?1", [book_id])?;
    conn.execute(
        "INSERT INTO book_fts (rowid, title, authors, series, narrators, aliases, skel) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            book_id,
            search_norm(&title),
            format!("{} {}", norm_join(&authors), author_aliases.join(" ")),
            series.as_deref().map(search_norm).unwrap_or_default(),
            norm_join(&narrators),
            norm_join(&aliases),
            skel_parts.join(" ")
        ],
    )?;
    Ok(())
}

/// Upgrades existing database records, automatically reverse-transliterating Latin
/// representations of Russian book titles, authors, series, and audio track titles / ebooks
/// into Russian Cyrillic. Also registers transliteration aliases so search still works with both.
pub fn auto_upgrade_translit(conn: &Connection) -> Result<usize> {
    let mut updated_total = 0;

    // 1. Upgrade books (title, authors_display, series_name)
    let mut book_stmt = conn.prepare(
        "SELECT id, title, authors_display, series_name FROM book
         WHERE title GLOB '*[a-zA-Z]*' OR authors_display GLOB '*[a-zA-Z]*' OR series_name GLOB '*[a-zA-Z]*'",
    )?;
    let books: Vec<(i64, String, String, Option<String>)> = book_stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (id, title, authors_display, series_name) in books {
        let (new_title, tr_t) = audiovault_core::translit::auto_reverse_translit(&title);
        let (new_series, tr_s) = match series_name {
            Some(s) => {
                let (ns, was) = audiovault_core::translit::auto_reverse_translit(&s);
                (Some(ns), was)
            }
            None => (None, false),
        };

        let authors = book_author_names(conn, id).unwrap_or_default();
        let mut tr_a = false;
        let mut new_authors = Vec::new();
        for a in &authors {
            let (na, was) = audiovault_core::translit::auto_reverse_translit(a);
            if was {
                tr_a = true;
            }
            new_authors.push(na);
        }

        let authors_disp_has_latin = authors_display.chars().any(|c| c.is_ascii_alphabetic());
        let (disp_tr, disp_was) =
            audiovault_core::translit::auto_reverse_translit(&authors_display);
        if disp_was {
            tr_a = true;
        }

        if tr_t || tr_s || tr_a || authors_disp_has_latin {
            if tr_t {
                let _ = add_book_alias(conn, id, &title, "translit", "reverse_translit");
                conn.execute(
                    "UPDATE book SET title = ?2, sort_title = ?3, title_key = ?4 WHERE id = ?1",
                    params![id, new_title, sort_key(&new_title), title_key(&new_title)],
                )?;
            }
            if tr_s && new_series.is_some() {
                let s_name = new_series.as_deref().unwrap();
                let sid = ensure_series(conn, s_name)?;
                conn.execute(
                    "UPDATE book SET series_id = ?2, series_name = ?3 WHERE id = ?1",
                    params![id, sid, s_name],
                )?;
            }
            if !authors.is_empty() {
                let _ = set_book_authors(conn, id, &new_authors, "upgrade");
            } else if !authors_display.trim().is_empty() {
                let effective_disp = if disp_was { &disp_tr } else { &authors_display };
                let raw_list = split_people(effective_disp);
                let _ = set_book_authors(conn, id, &raw_list, "upgrade");
            }
            let cur_authors = book_author_names(conn, id).unwrap_or(new_authors);
            let cur_title: String =
                conn.query_row("SELECT title FROM book WHERE id = ?1", [id], |r| r.get(0))?;
            let identity = book_identity_key(&cur_authors, &cur_title);
            conn.execute(
                "UPDATE book SET identity_key = ?2 WHERE id = ?1",
                params![id, identity],
            )?;

            let _ = refresh_book(conn, id);
            updated_total += 1;
        }
    }

    // 2. Upgrade media_file track titles and tags
    let mut media_stmt = conn.prepare(
        "SELECT id, file_name, tag_title, tag_artist, tag_album, tag_narrator, tag_series FROM media_file
         WHERE kind = 'audio' AND (tag_title IS NULL OR tag_title GLOB '*[a-zA-Z]*' OR tag_artist GLOB '*[a-zA-Z]*' OR tag_album GLOB '*[a-zA-Z]*')",
    )?;
    let media_rows: Vec<(
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = media_stmt
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (id, file_name, tag_title, tag_artist, tag_album, tag_narrator, tag_series) in media_rows {
        let mut changed = false;
        let mut new_title = tag_title.clone();
        if let Some(ref tt) = tag_title {
            let (tr, was) = audiovault_core::translit::auto_reverse_translit(tt);
            if was {
                new_title = Some(tr);
                changed = true;
            }
        } else {
            let stem = file_name
                .rsplit_once('.')
                .map_or(file_name.as_str(), |(s, _)| s);
            let (tr, was) = audiovault_core::translit::auto_reverse_translit(stem);
            if was {
                new_title = Some(tr);
                changed = true;
            }
        }

        let new_artist = tag_artist
            .as_ref()
            .and_then(|a| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(a);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(tag_artist);

        let new_album = tag_album
            .as_ref()
            .and_then(|a| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(a);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(tag_album);

        let new_narrator = tag_narrator
            .as_ref()
            .and_then(|n| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(n);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(tag_narrator);

        let new_series = tag_series
            .as_ref()
            .and_then(|s| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(s);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(tag_series);

        if changed {
            conn.execute(
                "UPDATE media_file SET tag_title = ?2, tag_artist = ?3, tag_album = ?4, tag_narrator = ?5, tag_series = ?6 WHERE id = ?1",
                params![id, new_title, new_artist, new_album, new_narrator, new_series],
            )?;
            updated_total += 1;
        }
    }

    // 3. Upgrade ebook_entry
    let mut ebook_stmt = conn.prepare(
        "SELECT id, title, authors, series, file_name FROM ebook_entry
         WHERE title GLOB '*[a-zA-Z]*' OR authors GLOB '*[a-zA-Z]*' OR series GLOB '*[a-zA-Z]*'",
    )?;
    let ebooks: Vec<(i64, String, String, Option<String>, String)> = ebook_stmt
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (id, title, authors, series, file_name) in ebooks {
        let (new_title, tr_t) = audiovault_core::translit::auto_reverse_translit(&title);
        let title_to_set = if tr_t {
            new_title
        } else {
            let stem = file_name
                .rsplit_once('.')
                .map_or(file_name.as_str(), |(s, _)| s);
            let (stem_tr, stem_was) = audiovault_core::translit::auto_reverse_translit(stem);
            if stem_was { stem_tr } else { title.clone() }
        };
        let tr_t_final = title_to_set != title;

        let mut tr_a = false;
        let new_authors = if !authors.is_empty() {
            authors
                .split(';')
                .map(|a| {
                    let trimmed = a.trim();
                    let (ca, was) = audiovault_core::translit::auto_reverse_translit(trimmed);
                    if was {
                        tr_a = true;
                    }
                    ca
                })
                .collect::<Vec<_>>()
                .join("; ")
        } else {
            authors.clone()
        };

        let (new_series, tr_s) = match series {
            Some(s) => {
                let (ns, was) = audiovault_core::translit::auto_reverse_translit(&s);
                (Some(ns), was)
            }
            None => (None, false),
        };

        if tr_t_final || tr_a || tr_s {
            conn.execute(
                "UPDATE ebook_entry SET title = ?2, authors = ?3, series = ?4, updated_at = ?5 WHERE id = ?1",
                params![id, title_to_set, new_authors, new_series, now_ts()],
            )?;
            updated_total += 1;
        }
    }

    // 4. Upgrade ebook_file
    let mut ef_stmt = conn.prepare(
        "SELECT id, title, authors, series FROM ebook_file
         WHERE title GLOB '*[a-zA-Z]*' OR authors GLOB '*[a-zA-Z]*' OR series GLOB '*[a-zA-Z]*'",
    )?;
    let ebook_files: Vec<(i64, Option<String>, Option<String>, Option<String>)> = ef_stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (id, title, authors, series) in ebook_files {
        let mut changed = false;
        let new_title = title
            .as_ref()
            .and_then(|t| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(t);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(title);

        let new_authors = authors
            .as_ref()
            .and_then(|a| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(a);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(authors);

        let new_series = series
            .as_ref()
            .and_then(|s| {
                let (tr, was) = audiovault_core::translit::auto_reverse_translit(s);
                if was {
                    changed = true;
                    Some(tr)
                } else {
                    None
                }
            })
            .or(series);

        if changed {
            conn.execute(
                "UPDATE ebook_file SET title = ?2, authors = ?3, series = ?4 WHERE id = ?1",
                params![id, new_title, new_authors, new_series],
            )?;
            updated_total += 1;
        }
    }

    if updated_total > 0 {
        tracing::info!(
            updated_total,
            "completed automatic reverse-transliteration database upgrade"
        );
    }

    Ok(updated_total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_db;

    #[test]
    fn person_upgrade_and_variant_logic() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let p1 = ensure_person(&conn, "Достоевский Ф.М.", "folder").unwrap();
        let p2 = ensure_person(&conn, "Фёдор Достоевский", "fb2").unwrap();
        assert_eq!(p1, p2);
        assert_eq!(person_name(&conn, p1).unwrap(), "Фёдор Достоевский");

        let book = create_book(
            &conn,
            "Братья Карамазовы",
            &["Фёдор Достоевский".to_string()],
        )
        .unwrap();
        let v_unknown = ensure_variant(&conn, book, &[]).unwrap();
        let v_a = ensure_variant(&conn, book, &["Александр Клюквин".to_string()]).unwrap();
        assert_eq!(
            v_unknown, v_a,
            "unknown-narrator variant adopts the first narrator"
        );
        let v_b = ensure_variant(&conn, book, &["Сергей Кузнецов".to_string()]).unwrap();
        assert_ne!(v_a, v_b, "different narrator → new variant");
        assert_eq!(
            ensure_variant(&conn, book, &["Клюквин Александр".to_string()]).unwrap(),
            v_a
        );
    }

    #[test]
    fn identity_lookup_and_merge() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let a = create_book(&conn, "Метро 2033", &["Глуховский Дмитрий".to_string()]).unwrap();
        let key = book_identity_key(&["Dmitry Glukhovsky"], "Metro 2033");
        assert_eq!(find_book_by_identity(&conn, &key).unwrap(), Some(a));
        let b = create_book(&conn, "Metro 2033", &["Dmitry Glukhovsky".to_string()]).unwrap();
        let va = ensure_variant(&conn, a, &[]).unwrap();
        let vb = ensure_variant(&conn, b, &[]).unwrap();
        let vol = crate::volumes::insert(&conn, &Default::default(), "V").unwrap();
        let ca = insert_copy(
            &conn,
            va,
            a,
            &CopyEnumeration {
                volume_id: vol,
                rel_path: "a".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let cb = insert_copy(
            &conn,
            vb,
            b,
            &CopyEnumeration {
                volume_id: vol,
                rel_path: "b".into(),
                ..Default::default()
            },
        )
        .unwrap();
        merge_books(&conn, b, a).unwrap();
        refresh_book(&conn, a).unwrap();
        assert!(!book_exists(&conn, b).unwrap());
        let copies = copies_of_book(&conn, a).unwrap();
        assert_eq!(copies.len(), 2);
        assert!(
            copies.iter().all(|c| c.variant_id == copies[0].variant_id),
            "unknown-narrator variants merge"
        );
        let _ = (ca, cb);
    }

    #[test]
    fn media_incremental_identity() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let vol = crate::volumes::insert(&conn, &Default::default(), "V").unwrap();
        let b = create_book(&conn, "X", &[]).unwrap();
        let v = ensure_variant(&conn, b, &[]).unwrap();
        let c = insert_copy(
            &conn,
            v,
            b,
            &CopyEnumeration {
                volume_id: vol,
                rel_path: "x".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let m = MediaEnumeration {
            rel_path: "x/01.mp3".into(),
            file_name: "01.mp3".into(),
            ext: "mp3".into(),
            kind: "audio".into(),
            size: 100,
            mtime: 5,
            sort_index: 0,
        };
        let (id, changed) = upsert_media(&conn, c, vol, &m, Some(1)).unwrap();
        assert!(changed);
        update_media_hash(&conn, id, Some("q"), Some("f"), 100, 5).unwrap();
        let (id2, changed2) = upsert_media(&conn, c, vol, &m, Some(2)).unwrap();
        assert_eq!(id, id2);
        assert!(!changed2);
        assert_eq!(
            media_for_copy(&conn, c).unwrap()[0].full_hash.as_deref(),
            Some("f")
        );
        let m2 = MediaEnumeration { size: 101, ..m };
        let (_, changed3) = upsert_media(&conn, c, vol, &m2, Some(3)).unwrap();
        assert!(changed3);
        assert_eq!(
            media_for_copy(&conn, c).unwrap()[0].full_hash,
            None,
            "hash invalidated on change"
        );
    }

    #[test]
    fn test_auto_upgrade_translit() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let vol = crate::volumes::insert(&conn, &Default::default(), "V").unwrap();

        // Directly insert transliterated book (simulating existing older DB state)
        conn.execute(
            "INSERT INTO book (uuid, title, sort_title, title_key, identity_key, authors_display, created_at, updated_at)
             VALUES ('u-1', 'Metro 2033', 'metro 2033', 'metro 2033', 'id-1', 'Glukhovskiy Dmitriy', 1, 1)",
            [],
        ).unwrap();
        let b = conn.last_insert_rowid();
        let pid = ensure_person(&conn, "Glukhovskiy Dmitriy", "test").unwrap();
        conn.execute(
            "INSERT INTO book_author (book_id, person_id, role, position) VALUES (?1, ?2, 'author', 0)",
            params![b, pid],
        ).unwrap();

        let v = ensure_variant(&conn, b, &[]).unwrap();
        let c = insert_copy(
            &conn,
            v,
            b,
            &CopyEnumeration {
                volume_id: vol,
                rel_path: "dmitriy_glukhovskiy_metro_2033".into(),
                ..Default::default()
            },
        )
        .unwrap();

        // Audio files with translit tag_title and null tag_title
        let m1 = MediaEnumeration {
            rel_path: "m/01.mp3".into(),
            file_name: "01.mp3".into(),
            ext: "mp3".into(),
            kind: "audio".into(),
            size: 100,
            mtime: 5,
            sort_index: 0,
        };
        let (mid1, _) = upsert_media(&conn, c, vol, &m1, Some(1)).unwrap();
        conn.execute(
            "UPDATE media_file SET tag_title = '01 - Vstuplenie' WHERE id = ?1",
            [mid1],
        )
        .unwrap();

        let m2 = MediaEnumeration {
            rel_path: "m/02 - Glava 01.mp3".into(),
            file_name: "02 - Glava 01.mp3".into(),
            ext: "mp3".into(),
            kind: "audio".into(),
            size: 100,
            mtime: 5,
            sort_index: 1,
        };
        let (mid2, _) = upsert_media(&conn, c, vol, &m2, Some(1)).unwrap();
        conn.execute(
            "UPDATE media_file SET tag_title = NULL WHERE id = ?1",
            [mid2],
        )
        .unwrap();

        // Ebook entry with translit
        conn.execute(
            "INSERT INTO ebook_entry (volume_id, rel_path, file_name, format, size, mtime, title, authors, created_at, updated_at)
             VALUES (?1, 'books/voyna.fb2', 'voyna.fb2', 'fb2', 500, 10, 'Voyna i mir', 'Tolstoy Lev', 1, 1)",
            [vol],
        ).unwrap();
        let eid = conn.last_insert_rowid();

        // Run auto_upgrade_translit
        let count = auto_upgrade_translit(&conn).unwrap();
        assert!(count > 0, "should have upgraded rows");

        // Verify book title and authors
        let (title, authors): (String, String) = conn
            .query_row(
                "SELECT title, authors_display FROM book WHERE id = ?1",
                [b],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "Метро 2033");
        assert_eq!(authors, "Дмитрий Глуховский");

        // Verify media files
        let tag1: String = conn
            .query_row(
                "SELECT tag_title FROM media_file WHERE id = ?1",
                [mid1],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tag1, "01 - Вступление");

        let tag2: String = conn
            .query_row(
                "SELECT tag_title FROM media_file WHERE id = ?1",
                [mid2],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tag2, "02 - Глава 01");

        // Verify ebook entry
        let (etitle, eauthors): (String, String) = conn
            .query_row(
                "SELECT title, authors FROM ebook_entry WHERE id = ?1",
                [eid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(etitle, "Война и мир");
        assert_eq!(eauthors, "Толстой Лев");
    }
}
