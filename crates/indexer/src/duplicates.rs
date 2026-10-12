//! Duplicate detection driven by blocking keys (never all-pairs).

use std::collections::{BTreeSet, HashMap};

use audiovault_core::duplicate::{FingerprintComparison, assess, blocking_keys};
use audiovault_core::model::DuplicateVerdict;
use audiovault_core::quality::recommend_keep;
use audiovault_db::duplicates::{self as dbdup, StoredFingerprint};
use audiovault_db::{Db, catalog, rusqlite};
use audiovault_media::fingerprint::{AudioFingerprintService, SegmentFingerprint};

use crate::{IndexerError, JobContext, Result, check};

/// Max candidates considered per copy (blocking buckets are small in practice).
const MAX_CANDIDATES: i64 = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DetectStats {
    pub copies: u64,
    pub pairs_scored: u64,
    pub candidates: u64,
}

fn to_segments(v: Vec<StoredFingerprint>, algorithm: &str) -> Vec<SegmentFingerprint> {
    v.into_iter()
        .filter(|s| s.algorithm == algorithm)
        .map(|s| SegmentFingerprint {
            segment: s.segment.max(0) as u32,
            position_ms: s.position_ms.max(0) as u64,
            length_ms: s.length_ms.max(0) as u64,
            data: s.data,
        })
        .collect()
}

fn fingerprint_cmp(
    conn: &rusqlite::Connection,
    a: i64,
    b: i64,
    fp: Option<&dyn AudioFingerprintService>,
    cache: &mut HashMap<i64, Vec<SegmentFingerprint>>,
) -> Result<Option<FingerprintComparison>> {
    let Some(svc) = fp else { return Ok(None) };
    for id in [a, b] {
        if let std::collections::hash_map::Entry::Vacant(e) = cache.entry(id) {
            e.insert(to_segments(dbdup::load_fingerprints(conn, id)?, svc.algorithm()));
        }
    }
    let (sa, sb) = (&cache[&a], &cache[&b]);
    if sa.is_empty() || sb.is_empty() {
        return Ok(None);
    }
    Ok(Some(svc.compare(sa, sb)))
}

/// Re-score `copy_ids` against their blocking-key neighbours.
pub fn detect(db: &Db, copy_ids: &[i64], fp: Option<&dyn AudioFingerprintService>, ctx: &dyn JobContext) -> Result<DetectStats> {
    let mut stats = DetectStats::default();
    let total = copy_ids.len().max(1);
    for (ci, chunk) in copy_ids.chunks(32).enumerate() {
        check(ctx)?;
        ctx.progress((ci * 32) as f64 / total as f64, &format!("{} of {} copies", ci * 32, copy_ids.len()));
        let chunk_stats = db.write_with(|tx| {
            let mut s = DetectStats::default();
            let mut books: BTreeSet<i64> = BTreeSet::new();
            let mut fp_cache: HashMap<i64, Vec<SegmentFingerprint>> = HashMap::new();
            for &copy_id in chunk {
                let own = dbdup::load_profiles(tx, &[copy_id])?;
                let Some(p) = own.get(&copy_id) else { continue };
                dbdup::replace_block_keys(tx, copy_id, &blocking_keys(p))?;
                let cands = dbdup::candidate_copies(tx, copy_id, MAX_CANDIDATES)?;
                s.copies += 1;
                if cands.is_empty() {
                    continue;
                }
                let mut ids = cands.clone();
                ids.push(copy_id);
                let profiles = dbdup::load_profiles(tx, &ids)?;
                let quality = catalog::quality_inputs(tx, &ids)?;
                for other in cands {
                    let Some(po) = profiles.get(&other) else { continue };
                    let fpc = fingerprint_cmp(tx, copy_id, other, fp, &mut fp_cache)?;
                    let asm = assess(p, po, fpc);
                    s.pairs_scored += 1;
                    let keep = match (quality.get(&copy_id), quality.get(&other)) {
                        (Some(qa), Some(qb))
                            if matches!(
                                asm.verdict,
                                DuplicateVerdict::ExactCopy | DuplicateVerdict::SameRecording | DuplicateVerdict::PossibleDuplicate
                            ) =>
                        {
                            Some(if recommend_keep(qa, qb).0 == 0 { copy_id } else { other })
                        }
                        _ => None,
                    };
                    if dbdup::upsert_candidate(tx, copy_id, other, &asm, keep)?.is_some() {
                        s.candidates += 1;
                    }
                    books.insert(p.book_id);
                    books.insert(po.book_id);
                }
            }
            for b in books {
                catalog::refresh_book(tx, b)?;
            }
            Ok::<_, IndexerError>(s)
        })?;
        stats.copies += chunk_stats.copies;
        stats.pairs_scored += chunk_stats.pairs_scored;
        stats.candidates += chunk_stats.candidates;
    }
    ctx.progress(1.0, "done");
    Ok(stats)
}

/// Re-score every present copy (paged; memory bounded).
pub fn detect_all(db: &Db, fp: Option<&dyn AudioFingerprintService>, ctx: &dyn JobContext) -> Result<DetectStats> {
    let ids: Vec<i64> = {
        let conn = db.conn()?;
        let mut st = conn.prepare("SELECT id FROM book_copy WHERE state = 'present' ORDER BY id")?;
        st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<i64>>>()?
    };
    detect(db, &ids, fp, ctx)
}

/// Open "possible duplicate" pairs that would benefit from acoustic
/// verification: (copy_a, copy_b) where at least one side lacks fingerprints.
pub fn pairs_needing_fingerprints(db: &Db, limit: i64) -> Result<Vec<(i64, i64)>> {
    let conn = db.conn()?;
    let mut st = conn.prepare(
        "SELECT d.copy_a, d.copy_b FROM duplicate_candidate d
         JOIN book_copy a ON a.id = d.copy_a JOIN book_copy b ON b.id = d.copy_b
         WHERE d.status = 'open' AND d.acoustic_checked = 0 AND d.verdict IN ('possible', 'same_recording')
           AND a.state = 'present' AND b.state = 'present'
           AND (a.analysis_level < 4 OR b.analysis_level < 4)
         ORDER BY d.score DESC LIMIT ?1",
    )?;
    Ok(st.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?)
}
