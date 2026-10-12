//! Catalog search: FTS5 over normalized text and transliteration skeletons,
//! typo tolerance via the FTS vocabulary, and explicit re-ranking.
//!
//! Search never touches the physical disks — everything comes from SQLite.

use std::collections::HashMap;

use audiovault_core::normalize::{search_norm, skeleton, skeleton_token};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MatchKind {
    Exact,
    Prefix,
    Fuzzy,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CopyBrief {
    pub copy_id: i64,
    pub volume_id: i64,
    pub volume_name: String,
    pub online: bool,
    pub last_seen_at: Option<i64>,
    pub mount_path: Option<String>,
    pub rel_path: String,
    pub state: String,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub total_size: u64,
    pub duration_ms: u64,
    pub narrator: String,
    pub has_fb2: bool,
    pub is_preferred: bool,
}

impl CopyBrief {
    /// Full path using the last known mount location.
    pub fn display_path(&self) -> String {
        match &self.mount_path {
            Some(m) => audiovault_core::paths::from_db_rel(std::path::Path::new(m), &self.rel_path).display().to_string(),
            None => self.rel_path.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchHit {
    pub book_id: i64,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub narrators: String,
    pub duration_ms: u64,
    pub size: u64,
    pub format: String,
    pub has_fb2: bool,
    pub has_duplicates: bool,
    pub variant_count: i64,
    pub cover_thumb: Option<String>,
    pub copies: Vec<CopyBrief>,
    pub match_kind: MatchKind,
    pub score: f64,
}

fn fts_query(tokens: &[String], skel_tokens: &[String]) -> String {
    // Tokens are alphanumeric after search_norm, so quoting is safe.
    let mut parts = Vec::with_capacity(tokens.len());
    for (t, s) in tokens.iter().zip(skel_tokens) {
        let mut alts = vec![format!("{{title authors series narrators aliases}} : \"{t}\"*")];
        if !s.is_empty() {
            alts.push(format!("skel : \"{s}\"*"));
        }
        parts.push(format!("({})", alts.join(" OR ")));
    }
    parts.join(" AND ")
}

fn fts_query_alternatives(alternatives: &[Vec<String>]) -> String {
    alternatives
        .iter()
        .map(|alts| {
            let inner: Vec<String> = alts.iter().map(|a| format!("\"{a}\"")).collect();
            format!("({})", inner.join(" OR "))
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Edit distance with adjacent transpositions (optimal string alignment).
fn osa_distance(a: &str, b: &str) -> usize {
    strsim::osa_distance(a, b)
}

/// Similar vocabulary terms for typo tolerance (bounded scan by first char and length).
fn similar_terms(conn: &Connection, term: &str) -> Result<Vec<String>> {
    let len = term.chars().count();
    if len < 4 {
        return Ok(vec![]);
    }
    let max_dist = if len >= 8 { 2 } else { 1 };
    let first: String = term.chars().take(1).collect();
    let mut upper = first.clone();
    upper.push(char::MAX);
    let mut st = conn.prepare_cached(
        "SELECT term FROM book_fts_vocab WHERE term >= ?1 AND term < ?2 AND length(term) BETWEEN ?3 AND ?4 LIMIT 20000",
    )?;
    let rows = st.query_map(
        params![first, upper, (len.saturating_sub(max_dist)) as i64, (len + max_dist + 3) as i64],
        |r| r.get::<_, String>(0),
    )?;
    let mut out: Vec<(usize, String)> = Vec::new();
    for t in rows {
        let t = t?;
        // Compare against the same-length prefix too, so partially typed words match.
        let tl = t.chars().count();
        let cand_prefix: String = t.chars().take(len).collect();
        let d = osa_distance(term, &t).min(if tl > len { osa_distance(term, &cand_prefix) } else { usize::MAX });
        if d <= max_dist && t != term {
            out.push((d, t));
        }
    }
    out.sort();
    out.truncate(8);
    Ok(out.into_iter().map(|(_, t)| t).collect())
}

struct Candidate {
    id: i64,
    bm25: f64,
    kind: MatchKind,
}

fn run_fts(conn: &Connection, query: &str, kind: MatchKind, limit: i64) -> Result<Vec<Candidate>> {
    let mut st = conn.prepare_cached(
        "SELECT rowid, bm25(book_fts, 10.0, 7.0, 3.0, 3.0, 2.0, 1.5) FROM book_fts WHERE book_fts MATCH ?1 ORDER BY 2 LIMIT ?2",
    )?;
    let rows = st
        .query_map(params![query, limit], |r| Ok(Candidate { id: r.get(0)?, bm25: r.get(1)?, kind }))?
        .collect::<rusqlite::Result<Vec<_>>>();
    match rows {
        Ok(v) => Ok(v),
        Err(e) => {
            // A malformed query must never break the UI; log and return nothing.
            tracing::warn!(error = %e, query, "FTS query failed");
            Ok(vec![])
        }
    }
}

/// Search the catalog. Returns up to `limit` ranked hits with their copies.
pub fn search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let norm = search_norm(query);
    let tokens: Vec<String> = norm.split(' ').filter(|t| !t.is_empty()).map(str::to_owned).collect();
    if tokens.is_empty() {
        return Ok(vec![]);
    }
    let skel_tokens: Vec<String> = tokens.iter().map(|t| skeleton_token(t)).collect();
    let mut cands = run_fts(conn, &fts_query(&tokens, &skel_tokens), MatchKind::Prefix, 400)?;

    if cands.len() < 5 {
        // Typo tolerance: expand each token with similar vocabulary terms.
        let mut alternatives: Vec<Vec<String>> = Vec::new();
        let mut any_expansion = false;
        for (t, s) in tokens.iter().zip(&skel_tokens) {
            let mut alts = vec![t.clone()];
            if s != t {
                alts.push(s.clone());
            }
            for x in similar_terms(conn, t)?.into_iter().chain(similar_terms(conn, s)?) {
                if !alts.contains(&x) {
                    alts.push(x);
                    any_expansion = true;
                }
            }
            alternatives.push(alts);
        }
        if any_expansion {
            let seen: std::collections::HashSet<i64> = cands.iter().map(|c| c.id).collect();
            for c in run_fts(conn, &fts_query_alternatives(&alternatives), MatchKind::Fuzzy, 200)? {
                if !seen.contains(&c.id) {
                    cands.push(c);
                }
            }
        }
    }
    if cands.is_empty() {
        return Ok(vec![]);
    }

    // Load ranking fields for all candidates in one query.
    let ids: Vec<i64> = cands.iter().map(|c| c.id).collect();
    let mut st = conn.prepare(
        "SELECT b.id, b.title, b.authors_display, b.series_name, b.series_index, b.narrators_display, b.total_duration_ms,
                b.total_size, b.format, b.has_fb2, b.has_duplicates, b.variant_count, b.cover_thumb
         FROM book b WHERE b.id IN (SELECT value FROM json_each(?1))",
    )?;
    let mut rows: HashMap<i64, SearchHit> = HashMap::new();
    for r in st.query_map([crate::ids_json(&ids)], |r| {
        Ok(SearchHit {
            book_id: r.get(0)?,
            title: r.get(1)?,
            authors: r.get(2)?,
            series: r.get(3)?,
            series_index: r.get(4)?,
            narrators: r.get(5)?,
            duration_ms: r.get(6)?,
            size: r.get(7)?,
            format: r.get(8)?,
            has_fb2: r.get::<_, i64>(9)? != 0,
            has_duplicates: r.get::<_, i64>(10)? != 0,
            variant_count: r.get(11)?,
            cover_thumb: r.get(12)?,
            copies: vec![],
            match_kind: MatchKind::Prefix,
            score: 0.0,
        })
    })? {
        let h = r?;
        rows.insert(h.book_id, h);
    }

    let q_skel = skeleton(&norm);
    let mut hits: Vec<SearchHit> = Vec::with_capacity(cands.len());
    for c in cands {
        let Some(mut h) = rows.remove(&c.id) else { continue };
        let t_norm = search_norm(&h.title);
        let a_norm = search_norm(&h.authors);
        let t_skel = skeleton(&h.title);
        let mut score = -c.bm25; // bm25 is negative; larger is better after negation
        let mut kind = c.kind;
        if t_norm == norm || t_skel == q_skel {
            score += 100.0;
            kind = MatchKind::Exact;
        } else if format!("{a_norm} {t_norm}") == norm || format!("{t_norm} {a_norm}") == norm {
            score += 90.0;
            kind = MatchKind::Exact;
        } else {
            let a_skel = skeleton(&h.authors);
            let combined = format!("{a_skel} {t_skel}");
            let combo_words: Vec<&str> = combined.split(' ').collect();
            let title_words: Vec<&str> = t_skel.split(' ').collect();
            let all_prefix_title = skel_tokens.iter().all(|q| title_words.iter().any(|w| w.starts_with(q.as_str())));
            let all_prefix_any = skel_tokens.iter().all(|q| combo_words.iter().any(|w| w.starts_with(q.as_str())));
            if t_skel.starts_with(&q_skel) {
                score += 50.0;
            } else if all_prefix_title {
                score += 35.0;
            } else if all_prefix_any {
                score += 25.0;
            }
            // Canonical spelling bonus: query matched the original-script title directly.
            if t_norm.contains(&norm) {
                score += 10.0;
            }
        }
        if kind == MatchKind::Fuzzy {
            score -= 30.0;
        }
        h.match_kind = kind;
        h.score = score;
        hits.push(h);
    }
    hits.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);

    let ids: Vec<i64> = hits.iter().map(|h| h.book_id).collect();
    let mut copies = copies_for_books(conn, &ids)?;
    for h in &mut hits {
        h.copies = copies.remove(&h.book_id).unwrap_or_default();
    }
    Ok(hits)
}

/// Copies (with storage state) for many books in one query.
pub fn copies_for_books(conn: &Connection, book_ids: &[i64]) -> Result<HashMap<i64, Vec<CopyBrief>>> {
    let mut st = conn.prepare_cached(
        "SELECT c.book_id, c.id, c.volume_id, v.display_name, v.is_online, v.last_seen_at, v.last_mount_path, c.rel_path,
                c.state, c.primary_codec, c.avg_bitrate_kbps, c.total_size, c.total_duration_ms, av.narrator_display,
                c.has_fb2, (b.preferred_copy_id = c.id)
         FROM book_copy c
         JOIN storage_volume v ON v.id = c.volume_id
         JOIN audio_variant av ON av.id = c.variant_id
         JOIN book b ON b.id = c.book_id
         WHERE c.book_id IN (SELECT value FROM json_each(?1)) AND c.state <> 'removed'
         ORDER BY c.book_id, (b.preferred_copy_id = c.id) DESC, v.is_online DESC, c.id",
    )?;
    let mut out: HashMap<i64, Vec<CopyBrief>> = HashMap::new();
    for r in st.query_map([crate::ids_json(book_ids)], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            CopyBrief {
                copy_id: r.get(1)?,
                volume_id: r.get(2)?,
                volume_name: r.get(3)?,
                online: r.get::<_, i64>(4)? != 0,
                last_seen_at: r.get(5)?,
                mount_path: r.get(6)?,
                rel_path: r.get(7)?,
                state: r.get(8)?,
                codec: r.get(9)?,
                bitrate_kbps: r.get(10)?,
                total_size: r.get(11)?,
                duration_ms: r.get(12)?,
                narrator: r.get(13)?,
                has_fb2: r.get::<_, i64>(14)? != 0,
                is_preferred: r.get::<_, Option<i64>>(15)?.unwrap_or(0) != 0,
            },
        ))
    })? {
        let (book, c) = r?;
        out.entry(book).or_default().push(c);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::*;
    use crate::test_util::temp_db;

    fn add(conn: &Connection, vol: i64, title: &str, author: &str, narrator: &str, path: &str) -> i64 {
        let b = create_book(conn, title, &[author.to_string()]).unwrap();
        let v = ensure_variant(conn, b, &[narrator.to_string()]).unwrap();
        insert_copy(conn, v, b, &CopyEnumeration { volume_id: vol, rel_path: path.into(), ..Default::default() }).unwrap();
        add_book_alias(conn, b, title, "title", "test").unwrap();
        refresh_book(conn, b).unwrap();
        b
    }

    fn setup() -> (tempfile::TempDir, std::sync::Arc<crate::Db>, Vec<i64>) {
        let (d, db) = temp_db();
        let ids = {
            let conn = db.conn().unwrap();
            let vol = crate::volumes::insert(&conn, &Default::default(), "Archive HDD 4").unwrap();
            vec![
                add(&conn, vol, "Братья Карамазовы", "Фёдор Достоевский", "Александр Клюквин", "dost/bk"),
                add(&conn, vol, "Преступление и наказание", "Фёдор Достоевский", "Владимир Самойлов", "dost/pn"),
                add(&conn, vol, "Метро 2033", "Дмитрий Глуховский", "Сергей Кузнецов", "gl/m2033"),
                add(&conn, vol, "Анна Каренина", "Лев Толстой", "", "tolstoy/ak"),
                add(&conn, vol, "The Shining", "Stephen King", "", "king/shining"),
            ]
        };
        (d, db, ids)
    }

    #[test]
    fn prefix_query_across_author_and_title() {
        let (_d, db, ids) = setup();
        let conn = db.conn().unwrap();
        let hits = search(&conn, "досто кар", 10).unwrap();
        assert_eq!(hits.first().map(|h| h.book_id), Some(ids[0]), "{hits:?}");
    }

    #[test]
    fn latin_query_finds_cyrillic() {
        let (_d, db, ids) = setup();
        let conn = db.conn().unwrap();
        let hits = search(&conn, "Dostoevsky karamazov", 10).unwrap();
        assert_eq!(hits.first().map(|h| h.book_id), Some(ids[0]));
        let hits = search(&conn, "metro 2033", 10).unwrap();
        assert_eq!(hits.first().map(|h| h.book_id), Some(ids[2]));
        assert_eq!(hits[0].match_kind, MatchKind::Exact);
        let hits = search(&conn, "glukhovsky", 10).unwrap();
        assert_eq!(hits.first().map(|h| h.book_id), Some(ids[2]));
    }

    #[test]
    fn exact_title_ranks_first_and_narrator_searchable() {
        let (_d, db, ids) = setup();
        let conn = db.conn().unwrap();
        let hits = search(&conn, "анна каренина", 10).unwrap();
        assert_eq!(hits[0].book_id, ids[3]);
        let hits = search(&conn, "клюквин", 10).unwrap();
        assert_eq!(hits[0].book_id, ids[0]);
    }

    #[test]
    fn typo_tolerance() {
        let (_d, db, ids) = setup();
        let conn = db.conn().unwrap();
        let hits = search(&conn, "карамазавы", 10).unwrap();
        assert_eq!(hits.first().map(|h| h.book_id), Some(ids[0]), "{hits:?}");
        assert_eq!(hits[0].match_kind, MatchKind::Fuzzy);
    }

    #[test]
    fn results_include_offline_copy_storage() {
        let (_d, db, ids) = setup();
        let conn = db.conn().unwrap();
        crate::volumes::set_offline_except(&conn, &[]).unwrap();
        let hits = search(&conn, "метро", 10).unwrap();
        assert_eq!(hits[0].book_id, ids[2]);
        let c = &hits[0].copies[0];
        assert_eq!(c.volume_name, "Archive HDD 4");
        assert!(!c.online);
        assert_eq!(c.rel_path, "gl/m2033");
    }

    #[test]
    fn hostile_queries_do_not_error() {
        let (_d, db, _) = setup();
        let conn = db.conn().unwrap();
        for q in ["\"", "AND OR NOT", "*", "a\"b", "{title}:x", "NEAR(a b)", "  "] {
            assert!(search(&conn, q, 10).is_ok(), "{q}");
        }
    }
}
