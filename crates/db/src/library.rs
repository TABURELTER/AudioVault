//! Library browsing (paged, indexed) and book details.

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::catalog::{StoredObservation, observations_for_book};
use crate::search::{CopyBrief, copies_for_books};
use crate::{OptionalExt, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LibraryFilter {
    All,
    Online,
    Offline,
    Duplicates,
    Issues,
    Missing,
    MultipleVariants,
    Volume(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LibrarySort {
    Title,
    TitleDesc,
    Author,
    AuthorDesc,
    Narrator,
    NarratorDesc,
    Duration,
    DurationAsc,
    Size,
    SizeAsc,
    Format,
    FormatDesc,
    Volume,
    VolumeDesc,
    Recent,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibraryRow {
    pub book_id: i64,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub narrators: String,
    pub duration_ms: u64,
    pub size: u64,
    pub format: String,
    pub copy_count: i64,
    pub variant_count: i64,
    pub has_duplicates: bool,
    pub issue_count: i64,
    pub cover_thumb: Option<String>,
    pub volume_name: Option<String>,
    pub online: bool,
    pub copy_state: Option<String>,
    pub any_online: bool,
}

fn filter_sql(f: LibraryFilter) -> (&'static str, Option<i64>) {
    match f {
        LibraryFilter::All => ("1", None),
        LibraryFilter::Online => (
            "EXISTS (SELECT 1 FROM book_copy c2 JOIN storage_volume v2 ON v2.id = c2.volume_id WHERE c2.book_id = b.id AND c2.state = 'present' AND v2.is_online = 1)",
            None,
        ),
        LibraryFilter::Offline => (
            "NOT EXISTS (SELECT 1 FROM book_copy c2 JOIN storage_volume v2 ON v2.id = c2.volume_id WHERE c2.book_id = b.id AND c2.state = 'present' AND v2.is_online = 1)",
            None,
        ),
        LibraryFilter::Duplicates => ("b.has_duplicates = 1", None),
        LibraryFilter::Issues => ("b.issue_count > 0", None),
        LibraryFilter::Missing => ("EXISTS (SELECT 1 FROM book_copy c2 WHERE c2.book_id = b.id AND c2.state = 'missing')", None),
        LibraryFilter::MultipleVariants => ("b.variant_count > 1", None),
        LibraryFilter::Volume(id) => ("EXISTS (SELECT 1 FROM book_copy c2 WHERE c2.book_id = b.id AND c2.volume_id = ?9)", Some(id)),
    }
}

fn order_sql(s: LibrarySort) -> &'static str {
    match s {
        LibrarySort::Title => "b.sort_title ASC, b.id ASC",
        LibrarySort::TitleDesc => "b.sort_title DESC, b.id DESC",
        LibrarySort::Author => "b.sort_author ASC, b.sort_title ASC, b.id ASC",
        LibrarySort::AuthorDesc => "b.sort_author DESC, b.sort_title DESC, b.id DESC",
        LibrarySort::Narrator => "b.narrators_display ASC, b.sort_title ASC, b.id ASC",
        LibrarySort::NarratorDesc => "b.narrators_display DESC, b.sort_title DESC, b.id DESC",
        LibrarySort::Duration => "b.total_duration_ms DESC, b.id ASC",
        LibrarySort::DurationAsc => "b.total_duration_ms ASC, b.id ASC",
        LibrarySort::Size => "b.total_size DESC, b.id ASC",
        LibrarySort::SizeAsc => "b.total_size ASC, b.id ASC",
        LibrarySort::Format => "b.format ASC, b.sort_title ASC, b.id ASC",
        LibrarySort::FormatDesc => "b.format DESC, b.sort_title DESC, b.id DESC",
        LibrarySort::Volume => "v.display_name ASC, b.sort_title ASC, b.id ASC",
        LibrarySort::VolumeDesc => "v.display_name DESC, b.sort_title DESC, b.id DESC",
        LibrarySort::Recent => "b.created_at DESC, b.id DESC",
    }
}

pub fn count(conn: &Connection, filter: LibraryFilter) -> Result<i64> {
    let (w, vol) = filter_sql(filter);
    let sql = format!("SELECT COUNT(*) FROM book b WHERE {}", w.replace("?9", "?1"));
    Ok(match vol {
        Some(v) => conn.query_row(&sql, [v], |r| r.get(0))?,
        None => conn.query_row(&sql, [], |r| r.get(0))?,
    })
}

pub fn page(conn: &Connection, filter: LibraryFilter, sort: LibrarySort, offset: i64, limit: i64) -> Result<Vec<LibraryRow>> {
    let (w, vol) = filter_sql(filter);
    let sql = format!(
        "SELECT b.id, b.title, b.authors_display, b.series_name, b.series_index, b.narrators_display, b.total_duration_ms,
                b.total_size, b.format, b.copy_count, b.variant_count, b.has_duplicates, b.issue_count, b.cover_thumb,
                v.display_name, COALESCE(v.is_online, 0), c.state,
                EXISTS (SELECT 1 FROM book_copy c3 JOIN storage_volume v3 ON v3.id = c3.volume_id
                        WHERE c3.book_id = b.id AND c3.state = 'present' AND v3.is_online = 1)
         FROM book b
         LEFT JOIN book_copy c ON c.id = b.preferred_copy_id
         LEFT JOIN storage_volume v ON v.id = c.volume_id
         WHERE {w}
         ORDER BY {}
         LIMIT ?1 OFFSET ?2",
        order_sql(sort)
    );
    let mut st = conn.prepare_cached(&sql)?;
    let map = |r: &rusqlite::Row<'_>| {
        Ok(LibraryRow {
            book_id: r.get(0)?,
            title: r.get(1)?,
            authors: r.get(2)?,
            series: r.get(3)?,
            series_index: r.get(4)?,
            narrators: r.get(5)?,
            duration_ms: r.get(6)?,
            size: r.get(7)?,
            format: r.get(8)?,
            copy_count: r.get(9)?,
            variant_count: r.get(10)?,
            has_duplicates: r.get::<_, i64>(11)? != 0,
            issue_count: r.get(12)?,
            cover_thumb: r.get(13)?,
            volume_name: r.get(14)?,
            online: r.get::<_, i64>(15)? != 0,
            copy_state: r.get(16)?,
            any_online: r.get::<_, i64>(17)? != 0,
        })
    };
    let rows = match vol {
        Some(v) => st.query_map(rusqlite::params_from_iter([limit, offset, 0, 0, 0, 0, 0, 0, v]), map)?.collect::<rusqlite::Result<Vec<_>>>()?,
        None => st.query_map(params![limit, offset], map)?.collect::<rusqlite::Result<Vec<_>>>()?,
    };
    Ok(rows)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EbookInfo {
    pub rel_path: String,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub series: Option<String>,
    pub annotation: Option<String>,
    pub language: Option<String>,
    pub year: Option<i32>,
    pub match_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopyDetail {
    pub brief: CopyBrief,
    pub variant_id: i64,
    pub file_count: i64,
    pub audio_file_count: i64,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub is_lossless: bool,
    pub has_cover: bool,
    pub has_chapters: bool,
    pub probe_errors: i64,
    pub quality_score: Option<f64>,
    pub missing_since: Option<i64>,
    pub first_seen_at: i64,
    pub detection_confidence: f64,
    pub evidence: Vec<String>,
    pub is_single_file: bool,
    pub ebook: Option<EbookInfo>,
    pub storage_class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VariantInfo {
    pub id: i64,
    pub narrator: String,
    pub label: Option<String>,
    pub is_preferred: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IssueInfo {
    pub id: i64,
    pub kind: String,
    pub message: String,
    pub field: Option<String>,
    pub current_value: Option<String>,
    pub proposed_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicateBrief {
    pub id: i64,
    pub other_copy_id: i64,
    pub other_book_id: i64,
    pub other_title: String,
    pub score: f64,
    pub verdict: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BookDetails {
    pub id: i64,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub year: Option<i64>,
    pub language: Option<String>,
    pub genre: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub narrators: String,
    pub duration_ms: u64,
    pub size: u64,
    pub format: String,
    pub cover_thumb: Option<String>,
    pub metadata_confidence: f64,
    pub preferred_copy_id: Option<i64>,
    pub variants: Vec<VariantInfo>,
    pub copies: Vec<CopyDetail>,
    pub observations: Vec<StoredObservation>,
    pub issues: Vec<IssueInfo>,
    pub duplicates: Vec<DuplicateBrief>,
    pub manual_fields: Vec<String>,
}

pub fn details(conn: &Connection, book_id: i64) -> Result<Option<BookDetails>> {
    let base = conn
        .query_row(
            "SELECT id, title, authors_display, series_name, series_index, year, language, genre, description, publisher,
                    narrators_display, total_duration_ms, total_size, format, cover_thumb, metadata_confidence,
                    preferred_copy_id, preferred_variant_id
             FROM book WHERE id = ?1",
            [book_id],
            |r| {
                Ok((
                    BookDetails {
                        id: r.get(0)?,
                        title: r.get(1)?,
                        authors: r.get(2)?,
                        series: r.get(3)?,
                        series_index: r.get(4)?,
                        year: r.get(5)?,
                        language: r.get(6)?,
                        genre: r.get(7)?,
                        description: r.get(8)?,
                        publisher: r.get(9)?,
                        narrators: r.get(10)?,
                        duration_ms: r.get(11)?,
                        size: r.get(12)?,
                        format: r.get(13)?,
                        cover_thumb: r.get(14)?,
                        metadata_confidence: r.get(15)?,
                        preferred_copy_id: r.get(16)?,
                        variants: vec![],
                        copies: vec![],
                        observations: vec![],
                        issues: vec![],
                        duplicates: vec![],
                        manual_fields: vec![],
                    },
                    r.get::<_, Option<i64>>(17)?,
                ))
            },
        )
        .optional()?;
    let Some((mut d, pref_variant)) = base else { return Ok(None) };

    let mut st = conn.prepare("SELECT id, narrator_display, label FROM audio_variant WHERE book_id = ?1 ORDER BY id")?;
    d.variants = st
        .query_map([book_id], |r| {
            let id: i64 = r.get(0)?;
            Ok(VariantInfo { id, narrator: r.get(1)?, label: r.get(2)?, is_preferred: Some(id) == pref_variant })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let briefs = copies_for_books(conn, &[book_id])?.remove(&book_id).unwrap_or_default();
    let mut st = conn.prepare_cached(
        "SELECT c.variant_id, c.file_count, c.audio_file_count, c.sample_rate, c.channels, c.is_lossless, c.has_cover,
                c.has_chapters, c.probe_errors, c.quality_score, c.missing_since, c.first_seen_at, c.detection_confidence,
                c.detection_evidence, c.is_single_file, v.storage_class
         FROM book_copy c JOIN storage_volume v ON v.id = c.volume_id WHERE c.id = ?1",
    )?;
    let mut est = conn.prepare_cached(
        "SELECT m.rel_path, e.title, e.authors, e.series, e.annotation, e.language, e.year, e.match_score
         FROM ebook_file e JOIN media_file m ON m.id = e.media_file_id WHERE e.copy_id = ?1 ORDER BY e.match_score DESC LIMIT 1",
    )?;
    for b in briefs {
        let ebook = est
            .query_row([b.copy_id], |r| {
                Ok(EbookInfo {
                    rel_path: r.get(0)?,
                    title: r.get(1)?,
                    authors: r.get(2)?,
                    series: r.get(3)?,
                    annotation: r.get(4)?,
                    language: r.get(5)?,
                    year: r.get(6)?,
                    match_score: r.get(7)?,
                })
            })
            .optional()?;
        let detail = st.query_row([b.copy_id], |r| {
            let ev: String = r.get(13)?;
            Ok(CopyDetail {
                variant_id: r.get(0)?,
                file_count: r.get(1)?,
                audio_file_count: r.get(2)?,
                sample_rate: r.get(3)?,
                channels: r.get(4)?,
                is_lossless: r.get::<_, i64>(5)? != 0,
                has_cover: r.get::<_, i64>(6)? != 0,
                has_chapters: r.get::<_, i64>(7)? != 0,
                probe_errors: r.get(8)?,
                quality_score: r.get(9)?,
                missing_since: r.get(10)?,
                first_seen_at: r.get(11)?,
                detection_confidence: r.get(12)?,
                evidence: serde_json::from_str(&ev).unwrap_or_default(),
                is_single_file: r.get::<_, i64>(14)? != 0,
                storage_class: r.get(15)?,
                ebook: None,
                brief: b.clone(),
            })
        })?;
        d.copies.push(CopyDetail { ebook, ..detail });
    }

    d.observations = observations_for_book(conn, book_id)?;
    d.manual_fields = d
        .observations
        .iter()
        .filter(|o| o.observation.source == audiovault_core::model::MetadataSource::Manual)
        .map(|o| o.observation.field.as_str().to_string())
        .collect();

    let mut st = conn.prepare(
        "SELECT id, kind, message, field, current_value, proposed_value FROM metadata_issue WHERE book_id = ?1 AND state = 'open'",
    )?;
    d.issues = st
        .query_map([book_id], |r| {
            Ok(IssueInfo {
                id: r.get(0)?,
                kind: r.get(1)?,
                message: r.get(2)?,
                field: r.get(3)?,
                current_value: r.get(4)?,
                proposed_value: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut st = conn.prepare(
        "SELECT d.id, CASE WHEN ca.book_id = ?1 THEN d.copy_b ELSE d.copy_a END AS other,
                CASE WHEN ca.book_id = ?1 THEN cb.book_id ELSE ca.book_id END AS other_book,
                d.score, d.verdict, d.status
         FROM duplicate_candidate d
         JOIN book_copy ca ON ca.id = d.copy_a JOIN book_copy cb ON cb.id = d.copy_b
         WHERE ca.book_id = ?1 OR cb.book_id = ?1 ORDER BY d.score DESC",
    )?;
    let dups: Vec<(i64, i64, i64, f64, String, String)> = st
        .query_map([book_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, other, other_book, score, verdict, status) in dups {
        let other_title: String =
            conn.query_row("SELECT title FROM book WHERE id = ?1", [other_book], |r| r.get(0)).optional()?.unwrap_or_default();
        d.duplicates.push(DuplicateBrief { id, other_copy_id: other, other_book_id: other_book, other_title, score, verdict, status });
    }
    Ok(Some(d))
}

/// Overall catalog statistics for the status bar.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CatalogStats {
    pub books: i64,
    pub copies: i64,
    pub volumes: i64,
    pub online_volumes: i64,
    pub open_duplicates: i64,
    pub open_issues: i64,
    pub total_bytes: u64,
}

pub fn stats(conn: &Connection) -> Result<CatalogStats> {
    Ok(conn.query_row(
        "SELECT (SELECT COUNT(*) FROM book), (SELECT COUNT(*) FROM book_copy WHERE state = 'present'),
                (SELECT COUNT(*) FROM storage_volume), (SELECT COUNT(*) FROM storage_volume WHERE is_online = 1),
                (SELECT COUNT(*) FROM duplicate_candidate WHERE status = 'open' AND verdict IN ('exact','same_recording','possible')),
                (SELECT COUNT(*) FROM metadata_issue WHERE state = 'open'),
                (SELECT COALESCE(SUM(total_size), 0) FROM book_copy WHERE state = 'present')",
        [],
        |r| {
            Ok(CatalogStats {
                books: r.get(0)?,
                copies: r.get(1)?,
                volumes: r.get(2)?,
                online_volumes: r.get(3)?,
                open_duplicates: r.get(4)?,
                open_issues: r.get(5)?,
                total_bytes: r.get(6)?,
            })
        },
    )?)
}
