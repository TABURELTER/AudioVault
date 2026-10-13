//! Duplicate candidates, blocking keys and acoustic fingerprints.

use std::collections::HashMap;

use audiovault_core::duplicate::{Assessment, CopyProfile, Evidence};
use audiovault_core::model::{DuplicateStatus, DuplicateVerdict};
use audiovault_core::normalize::title_key;
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::{OptionalExt, Result, ids_json, now_ts};

// ---------------------------------------------------------------- blocking keys

pub fn replace_block_keys(conn: &Connection, copy_id: i64, keys: &[String]) -> Result<()> {
    conn.execute("DELETE FROM copy_block_key WHERE copy_id = ?1", [copy_id])?;
    let mut st =
        conn.prepare_cached("INSERT OR IGNORE INTO copy_block_key (key, copy_id) VALUES (?1, ?2)")?;
    for k in keys {
        st.execute(params![k, copy_id])?;
    }
    Ok(())
}

/// Copies sharing at least one blocking key with `copy_id` (bounded; never a full scan).
pub fn candidate_copies(conn: &Connection, copy_id: i64, limit: i64) -> Result<Vec<i64>> {
    let mut st = conn.prepare_cached(
        "SELECT DISTINCT b.copy_id FROM copy_block_key a
         JOIN copy_block_key b ON b.key = a.key AND b.copy_id <> a.copy_id
         JOIN book_copy c ON c.id = b.copy_id AND c.state <> 'removed'
         WHERE a.copy_id = ?1
         LIMIT ?2",
    )?;
    Ok(st
        .query_map(params![copy_id, limit], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------- profiles

/// Build scoring profiles for many copies with a fixed number of queries.
pub fn load_profiles(conn: &Connection, copy_ids: &[i64]) -> Result<HashMap<i64, CopyProfile>> {
    let mut out: HashMap<i64, CopyProfile> = HashMap::new();
    if copy_ids.is_empty() {
        return Ok(out);
    }
    let ids = ids_json(copy_ids);
    let mut st = conn.prepare(
        "SELECT c.id, c.book_id, c.variant_id, b.authors_key, b.title_key, v.narrator_key, b.series_name, b.series_index,
                b.language, c.total_duration_ms, c.audio_file_count, c.total_size, c.primary_codec, c.avg_bitrate_kbps,
                c.is_lossless, c.rel_path
         FROM book_copy c JOIN book b ON b.id = c.book_id JOIN audio_variant v ON v.id = c.variant_id
         WHERE c.id IN (SELECT value FROM json_each(?1))",
    )?;
    let rows = st.query_map([&ids], |r| {
        let rel: String = r.get(15)?;
        let folder = rel.rsplit('/').next().unwrap_or("").to_string();
        let series: Option<String> = r.get(6)?;
        let narrator: String = r.get(5)?;
        Ok(CopyProfile {
            copy_id: r.get(0)?,
            book_id: r.get(1)?,
            variant_id: r.get(2)?,
            author_key: r.get(3)?,
            title_key: r.get(4)?,
            narrator_key: if narrator.is_empty() {
                None
            } else {
                Some(narrator)
            },
            series_key: series.as_deref().map(title_key).filter(|s| !s.is_empty()),
            series_index: r.get::<_, Option<f64>>(7)?.map(|f| f as f32),
            language: r.get(8)?,
            total_duration_ms: r.get(9)?,
            audio_file_count: r.get(10)?,
            total_size: r.get(11)?,
            codec: r.get(12)?,
            bitrate_kbps: r.get(13)?,
            is_lossless: r.get::<_, i64>(14)? != 0,
            file_durations_ms: Vec::new(),
            full_hashes: Vec::new(),
            quick_hashes: Vec::new(),
            folder_key: title_key(&folder),
        })
    })?;
    for p in rows {
        let p = p?;
        out.insert(p.copy_id, p);
    }
    // Per-file data in one query.
    let mut st = conn.prepare(
        "SELECT copy_id, duration_ms, quick_hash, full_hash FROM media_file
         WHERE copy_id IN (SELECT value FROM json_each(?1)) AND kind = 'audio' AND state = 'present'
         ORDER BY copy_id, sort_index, file_name",
    )?;
    let mut all_full: HashMap<i64, bool> = HashMap::new();
    let rows = st.query_map([&ids], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<i64>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    for row in rows {
        let (cid, dur, quick, full) = row?;
        let Some(p) = out.get_mut(&cid) else { continue };
        if let Some(d) = dur {
            p.file_durations_ms.push(d.max(0) as u64);
        }
        if let Some(q) = quick {
            p.quick_hashes.push(q);
        }
        let entry = all_full.entry(cid).or_insert(true);
        match full {
            Some(f) => p.full_hashes.push(f),
            None => *entry = false,
        }
    }
    for (cid, p) in out.iter_mut() {
        if p.file_durations_ms.len() != p.audio_file_count as usize {
            p.file_durations_ms.clear();
        }
        if !all_full.get(cid).copied().unwrap_or(false) {
            p.full_hashes.clear();
        }
        p.full_hashes.sort();
        p.quick_hashes.sort();
    }
    Ok(out)
}

// ---------------------------------------------------------------- candidates

/// Store (or refresh) a scored pair. User decisions (non-open status) are kept.
/// Unrelated pairs are removed unless the user already decided on them.
pub fn upsert_candidate(
    conn: &Connection,
    a: i64,
    b: i64,
    asm: &Assessment,
    recommended_keep: Option<i64>,
) -> Result<Option<i64>> {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    if lo == hi {
        return Ok(None);
    }
    let now = now_ts();
    if asm.verdict == DuplicateVerdict::Unrelated {
        conn.execute(
            "DELETE FROM duplicate_candidate WHERE copy_a = ?1 AND copy_b = ?2 AND status = 'open'",
            params![lo, hi],
        )?;
        return Ok(None);
    }
    let evidence = serde_json::to_string(&asm.evidence)?;
    conn.execute(
        "INSERT INTO duplicate_candidate (copy_a, copy_b, score, verdict, evidence, acoustic_checked, recommended_keep, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, ?8)
         ON CONFLICT (copy_a, copy_b) DO UPDATE SET score = excluded.score, verdict = excluded.verdict,
            evidence = excluded.evidence, acoustic_checked = excluded.acoustic_checked,
            recommended_keep = excluded.recommended_keep, updated_at = excluded.updated_at",
        params![lo, hi, asm.score as f64, asm.verdict.as_str(), evidence, asm.acoustic_checked as i64, recommended_keep, now],
    )?;
    let id: i64 = conn.query_row(
        "SELECT id FROM duplicate_candidate WHERE copy_a = ?1 AND copy_b = ?2",
        params![lo, hi],
        |r| r.get(0),
    )?;
    Ok(Some(id))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DupCopy {
    pub copy_id: i64,
    pub book_id: i64,
    pub variant_id: i64,
    pub title: String,
    pub authors: String,
    pub narrator: String,
    pub volume_id: i64,
    pub volume_name: String,
    pub volume_online: bool,
    pub mount_path: Option<String>,
    pub rel_path: String,
    pub state: String,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub total_size: i64,
    pub duration_ms: i64,
    pub audio_file_count: i64,
    pub has_fb2: bool,
    pub has_cover: bool,
    pub is_lossless: bool,
    pub quality_score: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicatePair {
    pub id: i64,
    pub score: f64,
    pub verdict: DuplicateVerdict,
    pub evidence: Vec<Evidence>,
    pub acoustic_checked: bool,
    pub recommended_keep: Option<i64>,
    pub status: DuplicateStatus,
    pub a: DupCopy,
    pub b: DupCopy,
}

const COPY_SELECT: &str = "SELECT c.id, c.book_id, c.variant_id, b.title, b.authors_display, v.narrator_display, c.volume_id,
        s.display_name, s.is_online, s.last_mount_path, c.rel_path, c.state, c.primary_codec, c.avg_bitrate_kbps,
        c.total_size, c.total_duration_ms, c.audio_file_count, c.has_fb2, c.has_cover, c.is_lossless, c.quality_score
     FROM book_copy c JOIN book b ON b.id = c.book_id JOIN audio_variant v ON v.id = c.variant_id
     JOIN storage_volume s ON s.id = c.volume_id";

fn row_to_dupcopy(r: &rusqlite::Row<'_>) -> rusqlite::Result<DupCopy> {
    Ok(DupCopy {
        copy_id: r.get(0)?,
        book_id: r.get(1)?,
        variant_id: r.get(2)?,
        title: r.get(3)?,
        authors: r.get(4)?,
        narrator: r.get(5)?,
        volume_id: r.get(6)?,
        volume_name: r.get(7)?,
        volume_online: r.get::<_, i64>(8)? != 0,
        mount_path: r.get(9)?,
        rel_path: r.get(10)?,
        state: r.get(11)?,
        codec: r.get(12)?,
        bitrate_kbps: r.get(13)?,
        total_size: r.get(14)?,
        duration_ms: r.get(15)?,
        audio_file_count: r.get(16)?,
        has_fb2: r.get::<_, i64>(17)? != 0,
        has_cover: r.get::<_, i64>(18)? != 0,
        is_lossless: r.get::<_, i64>(19)? != 0,
        quality_score: r.get(20)?,
    })
}

fn copies_by_id(conn: &Connection, ids: &[i64]) -> Result<HashMap<i64, DupCopy>> {
    let mut st = conn.prepare(&format!(
        "{COPY_SELECT} WHERE c.id IN (SELECT value FROM json_each(?1))"
    ))?;
    let rows = st.query_map([ids_json(ids)], row_to_dupcopy)?;
    let mut out = HashMap::new();
    for r in rows {
        let c = r?;
        out.insert(c.copy_id, c);
    }
    Ok(out)
}

type RawPair = (
    i64,
    i64,
    i64,
    f64,
    String,
    String,
    bool,
    Option<i64>,
    String,
);

fn assemble(conn: &Connection, raw: Vec<RawPair>) -> Result<Vec<DuplicatePair>> {
    let ids: Vec<i64> = raw.iter().flat_map(|p| [p.1, p.2]).collect();
    let copies = copies_by_id(conn, &ids)?;
    let mut out = Vec::with_capacity(raw.len());
    for (id, a, b, score, verdict, evidence, acoustic, keep, status) in raw {
        let (Some(ca), Some(cb)) = (copies.get(&a), copies.get(&b)) else {
            continue;
        };
        out.push(DuplicatePair {
            id,
            score,
            verdict: DuplicateVerdict::parse(&verdict)
                .unwrap_or(DuplicateVerdict::PossibleDuplicate),
            evidence: serde_json::from_str(&evidence).unwrap_or_default(),
            acoustic_checked: acoustic,
            recommended_keep: keep,
            status: DuplicateStatus::parse(&status).unwrap_or(DuplicateStatus::Open),
            a: ca.clone(),
            b: cb.clone(),
        });
    }
    Ok(out)
}

fn raw_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawPair> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get::<_, i64>(6)? != 0,
        r.get(7)?,
        r.get(8)?,
    ))
}

const PAIR_COLS: &str =
    "id, copy_a, copy_b, score, verdict, evidence, acoustic_checked, recommended_keep, status";

/// Open candidates, most confident first. `include_variants` also lists
/// "same book, different narrator" pairs (shown separately in the UI).
pub fn list_open(
    conn: &Connection,
    include_variants: bool,
    offset: i64,
    limit: i64,
) -> Result<Vec<DuplicatePair>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {PAIR_COLS} FROM duplicate_candidate
         WHERE status = 'open' AND (?1 OR verdict <> 'different_narrator')
         ORDER BY CASE verdict WHEN 'exact' THEN 0 WHEN 'same_recording' THEN 1 WHEN 'possible' THEN 2 ELSE 3 END, score DESC, id
         LIMIT ?2 OFFSET ?3"
    ))?;
    let raw = st
        .query_map(params![include_variants, limit, offset], raw_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assemble(conn, raw)
}

pub fn count_open(conn: &Connection) -> Result<(i64, i64)> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(verdict <> 'different_narrator'), 0), COALESCE(SUM(verdict = 'different_narrator'), 0)
         FROM duplicate_candidate WHERE status = 'open'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

pub fn get(conn: &Connection, id: i64) -> Result<Option<DuplicatePair>> {
    let raw = conn
        .query_row(
            &format!("SELECT {PAIR_COLS} FROM duplicate_candidate WHERE id = ?1"),
            [id],
            raw_row,
        )
        .optional()?;
    Ok(match raw {
        Some(r) => assemble(conn, vec![r])?.into_iter().next(),
        None => None,
    })
}

/// Candidates involving any copy of `book_id`.
pub fn for_book(conn: &Connection, book_id: i64) -> Result<Vec<DuplicatePair>> {
    let mut st = conn.prepare_cached(&format!(
        "SELECT {PAIR_COLS} FROM duplicate_candidate WHERE copy_a IN (SELECT id FROM book_copy WHERE book_id = ?1)
            OR copy_b IN (SELECT id FROM book_copy WHERE book_id = ?1) ORDER BY score DESC"
    ))?;
    let raw = st
        .query_map([book_id], raw_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    assemble(conn, raw)
}

/// Record the user's decision; returns the book ids whose summary must be refreshed.
pub fn set_status(conn: &Connection, id: i64, status: DuplicateStatus) -> Result<Vec<i64>> {
    conn.execute(
        "UPDATE duplicate_candidate SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, status.as_str(), now_ts()],
    )?;
    let mut st = conn.prepare(
        "SELECT DISTINCT c.book_id FROM duplicate_candidate d JOIN book_copy c ON c.id IN (d.copy_a, d.copy_b) WHERE d.id = ?1",
    )?;
    Ok(st
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------- fingerprints

#[derive(Debug, Clone, PartialEq)]
pub struct StoredFingerprint {
    pub segment: i64,
    pub position_ms: i64,
    pub length_ms: i64,
    pub algorithm: String,
    pub data: Vec<u32>,
}

pub fn save_fingerprints(
    conn: &Connection,
    copy_id: i64,
    segs: &[StoredFingerprint],
) -> Result<()> {
    conn.execute("DELETE FROM copy_fingerprint WHERE copy_id = ?1", [copy_id])?;
    let now = now_ts();
    let mut st = conn.prepare_cached(
        "INSERT INTO copy_fingerprint (copy_id, segment, position_ms, length_ms, algorithm, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    for s in segs {
        let bytes: Vec<u8> = s.data.iter().flat_map(|v| v.to_le_bytes()).collect();
        st.execute(params![
            copy_id,
            s.segment,
            s.position_ms,
            s.length_ms,
            s.algorithm,
            bytes,
            now
        ])?;
    }
    Ok(())
}

pub fn load_fingerprints(conn: &Connection, copy_id: i64) -> Result<Vec<StoredFingerprint>> {
    let mut st = conn.prepare_cached(
        "SELECT segment, position_ms, length_ms, algorithm, data FROM copy_fingerprint WHERE copy_id = ?1 ORDER BY segment",
    )?;
    let rows = st.query_map([copy_id], |r| {
        let blob: Vec<u8> = r.get(4)?;
        let data = blob
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(StoredFingerprint {
            segment: r.get(0)?,
            position_ms: r.get(1)?,
            length_ms: r.get(2)?,
            algorithm: r.get(3)?,
            data,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{self, CopyEnumeration};
    use crate::test_util::temp_db;
    use crate::volumes;

    fn setup(conn: &Connection) -> (i64, i64, i64) {
        let vol = volumes::insert(
            conn,
            &volumes::ObservedVolume::synthetic("T", "/mnt/t"),
            "Test",
        )
        .unwrap();
        let book =
            catalog::create_book(conn, "Метро 2033", &["Дмитрий Глуховский".into()]).unwrap();
        let var = catalog::ensure_variant(conn, book, &[]).unwrap();
        let mk = |rel: &str| {
            catalog::insert_copy(
                conn,
                var,
                book,
                &CopyEnumeration {
                    volume_id: vol,
                    rel_path: rel.into(),
                    audio_file_count: 2,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        (book, mk("a/Metro"), mk("b/Metro"))
    }

    #[test]
    fn candidates_and_decisions() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let (book, a, b) = setup(&conn);
        replace_block_keys(&conn, a, &["t:metro 2033".into()]).unwrap();
        replace_block_keys(&conn, b, &["t:metro 2033".into(), "d0:5".into()]).unwrap();
        assert_eq!(candidate_copies(&conn, a, 10).unwrap(), vec![b]);

        let profiles = load_profiles(&conn, &[a, b]).unwrap();
        let asm = audiovault_core::duplicate::assess(&profiles[&a], &profiles[&b], None);
        assert!(asm.score > 0.3, "same book scores: {asm:?}");
        let asm = Assessment {
            verdict: DuplicateVerdict::SameRecording,
            ..asm
        };
        let id = upsert_candidate(&conn, b, a, &asm, Some(a))
            .unwrap()
            .unwrap();
        let pair = get(&conn, id).unwrap().unwrap();
        assert_eq!(pair.a.copy_id, a.min(b));
        assert_eq!(list_open(&conn, false, 0, 10).unwrap().len(), 1);

        let books = set_status(&conn, id, DuplicateStatus::KeptBoth).unwrap();
        assert_eq!(books, vec![book]);
        // Re-scoring keeps the user's decision.
        upsert_candidate(&conn, a, b, &asm, Some(a)).unwrap();
        assert_eq!(
            get(&conn, id).unwrap().unwrap().status,
            DuplicateStatus::KeptBoth
        );
        assert!(list_open(&conn, true, 0, 10).unwrap().is_empty());
    }

    #[test]
    fn fingerprint_roundtrip() {
        let (_d, db) = temp_db();
        let conn = db.conn().unwrap();
        let (_, a, _) = setup(&conn);
        let seg = StoredFingerprint {
            segment: 0,
            position_ms: 1000,
            length_ms: 30000,
            algorithm: "chromaprint2".into(),
            data: vec![1, u32::MAX, 42],
        };
        save_fingerprints(&conn, a, std::slice::from_ref(&seg)).unwrap();
        assert_eq!(load_fingerprints(&conn, a).unwrap(), vec![seg]);
    }
}
