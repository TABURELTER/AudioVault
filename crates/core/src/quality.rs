//! Transparent quality heuristic used to recommend which copy to keep.
//!
//! Larger is *not* automatically better: codec efficiency and speech
//! saturation are modelled so a 64 kbps AAC M4B can beat a 320 kbps MP3.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QualityInput {
    pub codec: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub is_lossless: bool,
    pub has_title: bool,
    pub has_author: bool,
    pub has_narrator: bool,
    pub has_chapters: bool,
    pub has_cover: bool,
    pub has_fb2: bool,
    pub audio_file_count: u32,
    pub probe_errors: u32,
    pub user_preferred: bool,
    /// Storage policy bonus/penalty (e.g. -1 for slow/removable archive).
    pub storage_bias: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualityScore {
    pub score: f32,
    pub reasons: Vec<String>,
}

fn codec_efficiency(codec: &str) -> f32 {
    match codec.to_ascii_lowercase().as_str() {
        "opus" => 2.0,
        "aac" | "mp4a" | "aac-lc" | "he-aac" => 1.4,
        "vorbis" => 1.3,
        "mp3" | "mpeg" => 1.0,
        "wma" => 0.9,
        _ => 1.0,
    }
}

/// Compute a 0..100 score with human-readable reasons.
pub fn score(q: &QualityInput) -> QualityScore {
    let mut s: f32 = 0.0;
    let mut reasons = Vec::new();
    // Audio fidelity: effective (MP3-equivalent) bitrate, saturating for speech.
    if q.is_lossless {
        s += 40.0;
        reasons.push("lossless source (+40)".into());
    } else if let Some(br) = q.bitrate_kbps {
        let eff = br as f32 * q.codec.as_deref().map_or(1.0, codec_efficiency);
        // Speech is transparent around ~128 kbps MP3-equivalent.
        let fidelity = (eff / 128.0).min(1.0) * 35.0;
        s += fidelity;
        reasons.push(format!(
            "{} {} kbps ≈ {:.0} kbps MP3-equivalent (+{:.0})",
            q.codec.as_deref().unwrap_or("?").to_uppercase(),
            br,
            eff,
            fidelity
        ));
    } else {
        s += 15.0;
        reasons.push("bitrate unknown (+15)".into());
    }
    let meta = [q.has_title, q.has_author, q.has_narrator].iter().filter(|b| **b).count() as f32 * 5.0;
    if meta > 0.0 {
        s += meta;
        reasons.push(format!("metadata completeness (+{meta:.0})"));
    }
    if q.has_chapters {
        s += 8.0;
        reasons.push("chapter markers (+8)".into());
    }
    if q.has_cover {
        s += 4.0;
        reasons.push("cover art (+4)".into());
    }
    if q.has_fb2 {
        s += 4.0;
        reasons.push("FB2 text included (+4)".into());
    }
    if q.audio_file_count == 1 {
        s += 3.0;
        reasons.push("single-file book (+3)".into());
    }
    if q.probe_errors > 0 {
        let p = (q.probe_errors as f32 * 10.0).min(40.0);
        s -= p;
        reasons.push(format!("{} unreadable/corrupt file(s) (−{p:.0})", q.probe_errors));
    }
    if q.storage_bias != 0.0 {
        s += q.storage_bias * 5.0;
        reasons.push(format!("storage policy ({:+.0})", q.storage_bias * 5.0));
    }
    if q.user_preferred {
        s += 30.0;
        reasons.push("marked preferred by you (+30)".into());
    }
    QualityScore { score: s.clamp(0.0, 100.0), reasons }
}

/// Returns 0 to keep `a`, 1 to keep `b`, plus the scores.
pub fn recommend_keep(a: &QualityInput, b: &QualityInput) -> (usize, QualityScore, QualityScore) {
    let sa = score(a);
    let sb = score(b);
    let idx = if sb.score > sa.score { 1 } else { 0 };
    (idx, sa, sb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn efficient_m4b_beats_big_mp3() {
        let mp3 = QualityInput {
            codec: Some("mp3".into()),
            bitrate_kbps: Some(320),
            audio_file_count: 40,
            has_title: true,
            has_author: true,
            ..Default::default()
        };
        let m4b = QualityInput {
            codec: Some("aac".into()),
            bitrate_kbps: Some(96),
            audio_file_count: 1,
            has_chapters: true,
            has_cover: true,
            has_title: true,
            has_author: true,
            has_narrator: true,
            ..Default::default()
        };
        let (keep, _, _) = recommend_keep(&mp3, &m4b);
        assert_eq!(keep, 1);
    }

    #[test]
    fn corruption_penalized() {
        let ok = QualityInput { codec: Some("mp3".into()), bitrate_kbps: Some(64), ..Default::default() };
        let broken = QualityInput { probe_errors: 3, ..ok.clone() };
        assert!(score(&ok).score > score(&broken).score);
    }
}
