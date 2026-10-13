//! Duplicate detection: cheap blocking keys and progressive evidence scoring.
//!
//! Candidate generation never compares all pairs; copies are bucketed by
//! [`blocking_keys`] (title key, author+title key, duration buckets, content
//! hash) and only copies sharing a bucket are scored with [`assess`].

use serde::{Deserialize, Serialize};

use crate::model::DuplicateVerdict;

/// Everything the scorer needs to know about a physical copy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CopyProfile {
    pub copy_id: i64,
    pub book_id: i64,
    pub variant_id: i64,
    /// Sorted identity keys of authors (`skeleton|initial`).
    pub author_key: String,
    pub title_key: String,
    pub narrator_key: Option<String>,
    pub series_key: Option<String>,
    pub series_index: Option<f32>,
    pub language: Option<String>,
    pub total_duration_ms: u64,
    pub audio_file_count: u32,
    pub total_size: u64,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub is_lossless: bool,
    /// Per-file durations in playback order (may be empty when unknown).
    pub file_durations_ms: Vec<u64>,
    /// Sorted full BLAKE3 hashes of audio files (only when *all* are hashed).
    pub full_hashes: Vec<String>,
    /// Sorted quick hashes (size + head/tail) of audio files.
    pub quick_hashes: Vec<String>,
    pub folder_key: String,
}

/// Result of comparing acoustic fingerprints of two copies.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FingerprintComparison {
    pub segments_compared: u32,
    pub segments_matched: u32,
    /// Mean similarity of compared segments, 0..1.
    pub similarity: f32,
}

impl FingerprintComparison {
    pub fn is_match(&self) -> bool {
        self.segments_compared > 0
            && self.segments_matched * 2 > self.segments_compared
            && self.similarity >= 0.6
    }
    pub fn is_mismatch(&self) -> bool {
        self.segments_compared >= 2 && self.segments_matched == 0 && self.similarity < 0.25
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub code: String,
    pub text: String,
    /// Contribution to the score (may be negative).
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assessment {
    pub score: f32,
    pub verdict: DuplicateVerdict,
    pub evidence: Vec<Evidence>,
    /// False when no acoustic fingerprint comparison was available.
    pub acoustic_checked: bool,
}

/// Duration bucket width for blocking.
const BUCKET_MS: u64 = 120_000;

/// Blocking keys; two copies are compared only if they share at least one.
/// Two staggered duration grids guarantee that copies within 60 s of each
/// other always share a duration key.
pub fn blocking_keys(p: &CopyProfile) -> Vec<String> {
    let mut keys = Vec::with_capacity(6);
    if !p.title_key.is_empty() {
        keys.push(format!("t:{}", p.title_key));
        if !p.author_key.is_empty() {
            keys.push(format!("at:{}#{}", p.author_key, p.title_key));
        }
    }
    if p.total_duration_ms > 10 * 60_000 && !p.author_key.is_empty() {
        keys.push(format!(
            "ad0:{}#{}",
            p.author_key,
            p.total_duration_ms / BUCKET_MS
        ));
        keys.push(format!(
            "ad1:{}#{}",
            p.author_key,
            (p.total_duration_ms + BUCKET_MS / 2) / BUCKET_MS
        ));
    }
    if !p.full_hashes.is_empty() {
        keys.push(format!("h:{}", combined_hash_key(&p.full_hashes)));
    }
    if let Some(first) = p.quick_hashes.first() {
        keys.push(format!("q:{first}"));
    }
    keys
}

/// Stable key for a set of hashes (they are expected to be sorted).
pub fn combined_hash_key(hashes: &[String]) -> String {
    // Short, order-independent digest without pulling a hash crate into core:
    // FNV-1a 64 over the sorted list is plenty for bucketing (not security).
    let mut sorted: Vec<&String> = hashes.iter().collect();
    sorted.sort();
    let mut h: u64 = 0xcbf29ce484222325;
    for s in sorted {
        for b in s.bytes().chain(std::iter::once(b'|')) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

fn ratio_diff(a: u64, b: u64) -> f64 {
    if a == 0 || b == 0 {
        return f64::INFINITY;
    }
    (a as f64 - b as f64).abs() / (a.max(b) as f64)
}

fn fmt_size(b: u64) -> String {
    crate::format::bytes(b)
}

/// Score a pair of copies.
pub fn assess(a: &CopyProfile, b: &CopyProfile, fp: Option<FingerprintComparison>) -> Assessment {
    let mut ev: Vec<Evidence> = Vec::new();
    let mut push = |code: &str, text: String, weight: f32| {
        ev.push(Evidence {
            code: code.into(),
            text,
            weight,
        })
    };

    // 1. Byte-identical content.
    if !a.full_hashes.is_empty() && a.full_hashes == b.full_hashes {
        push(
            "exact",
            format!(
                "all {} audio files are byte-identical (BLAKE3)",
                a.full_hashes.len()
            ),
            1.0,
        );
        return Assessment {
            score: 1.0,
            verdict: DuplicateVerdict::ExactCopy,
            evidence: ev,
            acoustic_checked: fp.is_some(),
        };
    }

    // Volume/part mismatch: distinct books/volumes are never duplicates!
    let vol_a = crate::infer::extract_volume(&a.title_key)
        .or_else(|| crate::infer::extract_volume(&a.folder_key));
    let vol_b = crate::infer::extract_volume(&b.title_key)
        .or_else(|| crate::infer::extract_volume(&b.folder_key));
    let diff_volume = match (&vol_a, &vol_b) {
        (Some(va), Some(vb)) => {
            va.index != vb.index || va.raw.to_lowercase() != vb.raw.to_lowercase()
        }
        _ => false,
    };
    let diff_series_idx =
        a.series_index.is_some() && b.series_index.is_some() && a.series_index != b.series_index;
    if diff_volume || diff_series_idx {
        push(
            "volume_differs",
            "different volumes or book numbers — distinct releases".into(),
            -1.0,
        );
        return Assessment {
            score: 0.0,
            verdict: DuplicateVerdict::Unrelated,
            evidence: ev,
            acoustic_checked: fp.is_some(),
        };
    }

    let mut score: f32 = 0.0;
    // 2. Identity.
    let same_book = a.book_id != 0 && a.book_id == b.book_id;
    let same_title = !a.title_key.is_empty() && a.title_key == b.title_key;
    let same_author = !a.author_key.is_empty() && a.author_key == b.author_key;
    if same_book {
        score += 0.35;
        push("same_book", "same canonical book".into(), 0.35);
    } else if same_title && same_author {
        score += 0.35;
        push(
            "same_identity",
            "same canonical title and author".into(),
            0.35,
        );
    } else if same_title {
        score += 0.2;
        push(
            "same_title",
            "same normalized title (author differs or unknown)".into(),
            0.2,
        );
    } else {
        let sim = strsim::jaro_winkler(&a.title_key, &b.title_key) as f32;
        if sim >= 0.92 && !a.title_key.is_empty() {
            score += 0.12;
            push(
                "similar_title",
                format!("similar titles ({:.0}% similar)", sim * 100.0),
                0.12,
            );
        }
    }
    if let (Some(sa), Some(sb)) = (&a.series_key, &b.series_key)
        && sa == sb
        && a.series_index.is_some()
        && a.series_index != b.series_index
    {
        score -= 0.4;
        push(
            "series_index_differs",
            "same series but different book number".into(),
            -0.4,
        );
    }

    // 3. Narrator.
    let narrators_differ = match (&a.narrator_key, &b.narrator_key) {
        (Some(na), Some(nb)) if !na.is_empty() && !nb.is_empty() => {
            if na == nb {
                score += 0.15;
                push("same_narrator", "same narrator".into(), 0.15);
                false
            } else {
                push("narrator_differs", "narrators differ".into(), 0.0);
                true
            }
        }
        _ => {
            push(
                "narrator_unknown",
                "narrator unknown for at least one copy".into(),
                0.0,
            );
            false
        }
    };

    // 4. Duration.
    let dr = ratio_diff(a.total_duration_ms, b.total_duration_ms);
    let duration_far = dr.is_finite() && dr > 0.05;
    if dr.is_finite() {
        let pct = dr * 100.0;
        let (w, txt) = if dr <= 0.002 {
            (0.25, format!("duration differs by only {pct:.2}%"))
        } else if dr <= 0.01 {
            (0.15, format!("duration differs by {pct:.2}%"))
        } else if dr <= 0.03 {
            (0.05, format!("duration differs by {pct:.1}%"))
        } else if dr <= 0.05 {
            (0.0, format!("duration differs by {pct:.1}%"))
        } else {
            (
                -0.25,
                format!("duration differs by {pct:.0}% — likely a different recording"),
            )
        };
        score += w;
        push("duration", txt, w);
    } else {
        push(
            "duration_unknown",
            "duration unknown for at least one copy".into(),
            0.0,
        );
    }

    // 5. Per-file structure.
    if a.audio_file_count == b.audio_file_count && a.audio_file_count > 1 {
        let per_file_match = a.file_durations_ms.len() == b.file_durations_ms.len()
            && !a.file_durations_ms.is_empty()
            && a.file_durations_ms
                .iter()
                .zip(&b.file_durations_ms)
                .all(|(x, y)| x.abs_diff(*y) <= 1500.max(x / 200));
        if per_file_match {
            score += 0.1;
            push(
                "per_file",
                format!(
                    "same {} files with matching per-file durations",
                    a.audio_file_count
                ),
                0.1,
            );
        } else {
            score += 0.03;
            push(
                "file_count",
                format!("same file count ({})", a.audio_file_count),
                0.03,
            );
        }
    }

    // 6. Partial byte overlap.
    if !a.full_hashes.is_empty() && !b.full_hashes.is_empty() {
        let shared = a
            .full_hashes
            .iter()
            .filter(|h| b.full_hashes.contains(h))
            .count();
        if shared > 0 {
            let w = 0.2;
            score += w;
            push(
                "partial_hash",
                format!("{shared} audio file(s) are byte-identical"),
                w,
            );
        }
    } else if !a.quick_hashes.is_empty() && !b.quick_hashes.is_empty() {
        let shared = a
            .quick_hashes
            .iter()
            .filter(|h| b.quick_hashes.contains(h))
            .count();
        if shared > 0 {
            score += 0.12;
            push(
                "quick_hash",
                format!("{shared} file(s) share size and head/tail hash"),
                0.12,
            );
        }
    }

    // 7. Acoustic fingerprint.
    let mut acoustic_same = false;
    match fp {
        Some(f) if f.is_match() => {
            score += 0.35;
            acoustic_same = true;
            push(
                "acoustic_match",
                format!(
                    "acoustic fingerprints match ({}/{} segments, {:.0}% similar)",
                    f.segments_matched,
                    f.segments_compared,
                    f.similarity * 100.0
                ),
                0.35,
            );
        }
        Some(f) if f.is_mismatch() => {
            score -= 0.4;
            push(
                "acoustic_mismatch",
                "acoustic fingerprints do not match — different recording".into(),
                -0.4,
            );
        }
        Some(f) => {
            push(
                "acoustic_inconclusive",
                format!(
                    "acoustic comparison inconclusive ({}/{} segments)",
                    f.segments_matched, f.segments_compared
                ),
                0.0,
            );
        }
        None => push(
            "acoustic_unavailable",
            "acoustic fingerprint not available — confidence reduced".into(),
            0.0,
        ),
    }

    // 8. Technical difference description (informational).
    let fa = a.codec.clone().unwrap_or_else(|| "?".into()).to_uppercase();
    let fb = b.codec.clone().unwrap_or_else(|| "?".into()).to_uppercase();
    if fa != fb {
        push(
            "formats",
            format!(
                "one is {fa} ({}) and one is {fb} ({})",
                fmt_size(a.total_size),
                fmt_size(b.total_size)
            ),
            0.0,
        );
    }

    if fp.is_none() {
        // Without acoustic evidence we cap confidence below "certain".
        score = score.min(0.92);
    }
    let score = score.clamp(0.0, 1.0);
    let identity = same_book || same_title;
    let verdict = if diff_volume || diff_series_idx {
        DuplicateVerdict::Unrelated
    } else if identity && (narrators_differ || duration_far) && !acoustic_same {
        if same_title && narrators_differ && dr <= 0.20 {
            DuplicateVerdict::DifferentNarrator
        } else {
            DuplicateVerdict::Unrelated
        }
    } else if score >= 0.75 || (acoustic_same && dr <= 0.02) {
        DuplicateVerdict::SameRecording
    } else if score >= 0.5 {
        DuplicateVerdict::PossibleDuplicate
    } else {
        DuplicateVerdict::Unrelated
    };
    Assessment {
        score,
        verdict,
        evidence: ev,
        acoustic_checked: fp.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> CopyProfile {
        CopyProfile {
            copy_id: 1,
            book_id: 10,
            variant_id: 100,
            author_key: "gluhovski|d".into(),
            title_key: "metro 2033".into(),
            narrator_key: Some("kuznecov|s".into()),
            total_duration_ms: 16 * 3600 * 1000,
            audio_file_count: 40,
            total_size: 2_400_000_000,
            codec: Some("mp3".into()),
            bitrate_kbps: Some(320),
            ..Default::default()
        }
    }

    #[test]
    fn different_encodes_of_same_narration_are_duplicates() {
        let a = base();
        let mut b = base();
        b.copy_id = 2;
        b.audio_file_count = 1;
        b.total_size = 870_000_000;
        b.codec = Some("aac".into());
        b.total_duration_ms = a.total_duration_ms + (a.total_duration_ms as f64 * 0.0008) as u64;
        let fp = FingerprintComparison {
            segments_compared: 3,
            segments_matched: 3,
            similarity: 0.9,
        };
        let r = assess(&a, &b, Some(fp));
        assert_eq!(r.verdict, DuplicateVerdict::SameRecording, "{r:?}");
        assert!(r.score >= 0.95);
        assert!(r.evidence.iter().any(|e| e.code == "acoustic_match"));
        assert!(
            r.evidence
                .iter()
                .any(|e| e.text.contains("MP3") && e.text.contains("AAC"))
        );

        let r2 = assess(&a, &b, None);
        assert_eq!(r2.verdict, DuplicateVerdict::SameRecording);
        assert!(r2.score <= 0.92, "capped without acoustic evidence");
    }

    #[test]
    fn different_narrator_is_variant_not_duplicate() {
        let a = base();
        let mut b = base();
        b.copy_id = 2;
        b.narrator_key = Some("kluykvin|a".into());
        b.total_duration_ms = (a.total_duration_ms as f64 * 1.12) as u64;
        let r = assess(&a, &b, None);
        assert_eq!(r.verdict, DuplicateVerdict::DifferentNarrator);
    }

    #[test]
    fn byte_identical() {
        let mut a = base();
        a.full_hashes = vec!["aa".into(), "bb".into()];
        let mut b = a.clone();
        b.copy_id = 2;
        let r = assess(&a, &b, None);
        assert_eq!(r.verdict, DuplicateVerdict::ExactCopy);
        assert_eq!(r.score, 1.0);
    }

    #[test]
    fn unrelated_books_with_similar_duration() {
        let a = base();
        let mut b = base();
        b.book_id = 11;
        b.title_key = "piknik na obochine".into();
        b.author_key = "strugacki|a".into();
        b.narrator_key = None;
        let r = assess(&a, &b, None);
        assert_eq!(r.verdict, DuplicateVerdict::Unrelated, "{r:?}");
    }

    #[test]
    fn series_neighbours_not_duplicates() {
        let mut a = base();
        a.series_key = Some("metro".into());
        a.series_index = Some(1.0);
        let mut b = a.clone();
        b.copy_id = 2;
        b.book_id = 0;
        b.series_index = Some(2.0);
        let r = assess(&a, &b, None);
        assert!(r.score < 0.75, "{r:?}");
    }

    #[test]
    fn blocking_keys_overlap_for_close_durations() {
        let mut a = base();
        let mut b = base();
        for d in [0u64, 59_000, 30_000, 1] {
            a.total_duration_ms = 7_199_000;
            b.total_duration_ms = 7_199_000 + d;
            let ka = blocking_keys(&a);
            let kb = blocking_keys(&b);
            assert!(
                ka.iter()
                    .filter(|k| k.starts_with("ad") || k.starts_with('d'))
                    .any(|k| kb.contains(k)),
                "durations {d}ms apart must share a bucket"
            );
        }
    }

    #[test]
    fn different_volumes_are_unrelated() {
        let mut a = base();
        a.title_key = "gorki kvest tom 1".into();
        a.folder_key = "Том 1".into();

        let mut b = base();
        b.copy_id = 2;
        b.title_key = "gorki kvest tom 2".into();
        b.folder_key = "Том 2".into();

        let r = assess(&a, &b, None);
        assert_eq!(r.verdict, DuplicateVerdict::Unrelated, "{r:?}");
        assert_eq!(r.score, 0.0);
    }
}
