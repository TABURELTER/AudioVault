//! Storage optimization recommendations. Pure analysis — nothing here
//! transcodes; the user must approve every conversion.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetFormat {
    /// AAC-LC in an M4B container: widely compatible audiobook format.
    M4bAac,
    /// Opus in Ogg: best compression, weaker player support (Apple Books etc.).
    Opus,
    /// Universal MP3: plays on all hardware players, car stereos, and legacy devices.
    Mp3,
}

impl TargetFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetFormat::M4bAac => "m4b_aac",
            TargetFormat::Opus => "opus",
            TargetFormat::Mp3 => "mp3",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "m4b_aac" | "m4b" => Some(TargetFormat::M4bAac),
            "opus" => Some(TargetFormat::Opus),
            "mp3" => Some(TargetFormat::Mp3),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            TargetFormat::M4bAac => "M4B (AAC)",
            TargetFormat::Opus => "Opus",
            TargetFormat::Mp3 => "MP3",
        }
    }
    pub fn codec(self) -> &'static str {
        match self {
            TargetFormat::M4bAac => "aac",
            TargetFormat::Opus => "opus",
            TargetFormat::Mp3 => "libmp3lame",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            TargetFormat::M4bAac => "m4b",
            TargetFormat::Opus => "opus",
            TargetFormat::Mp3 => "mp3",
        }
    }
    pub fn compatibility_note(self) -> &'static str {
        match self {
            TargetFormat::M4bAac => {
                "M4B/AAC воспроизводится на большинстве современных плееров и сохраняет встроенные главы и обложку."
            }
            TargetFormat::Opus => {
                "Opus обеспечивает наименьший размер для речи, но не поддерживается многими аппаратными плеерами и автомагнитолами."
            }
            TargetFormat::Mp3 => {
                "MP3 воспроизводится абсолютно на любых плеерах, автомагнитолах, устаревших устройствах и аудиосистемах."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversionKind {
    /// Lossless source → lossy target. Usually large savings, low risk.
    LosslessToLossy,
    /// Lossy → lossy re-encode. Generation loss; offered only when savings are large.
    LossyToLossy,
    /// Container repack without re-encoding (e.g. AAC tracks → single M4B).
    Repack,
}

impl ConversionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConversionKind::LosslessToLossy => "lossless_to_lossy",
            ConversionKind::LossyToLossy => "lossy_to_lossy",
            ConversionKind::Repack => "repack",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "lossless_to_lossy" => Some(ConversionKind::LosslessToLossy),
            "lossy_to_lossy" => Some(ConversionKind::LossyToLossy),
            "repack" => Some(ConversionKind::Repack),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ConversionKind::LosslessToLossy => "Lossless → lossy",
            ConversionKind::LossyToLossy => "Lossy → lossy (re-encode)",
            ConversionKind::Repack => "Repack (no re-encoding)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizeInput {
    pub codec: Option<String>,
    pub is_lossless: bool,
    pub bitrate_kbps: Option<u32>,
    pub channels: Option<u8>,
    pub total_size: u64,
    pub audio_size: u64,
    pub duration_ms: u64,
    pub audio_file_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub kind: ConversionKind,
    pub target: TargetFormat,
    pub current_codec: String,
    pub current_bitrate_kbps: Option<u32>,
    pub target_bitrate_kbps: Option<u32>,
    pub current_size: u64,
    pub estimated_size: u64,
    pub reencode: bool,
    pub preserves_chapters: bool,
    pub notes: String,
}

impl Recommendation {
    pub fn savings(&self) -> u64 {
        self.current_size.saturating_sub(self.estimated_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OptimizePolicy {
    pub aac_kbps: u32,
    pub opus_kbps: u32,
    /// Minimum savings ratio for lossy→lossy suggestions.
    pub lossy_min_savings: f32,
    /// Minimum source bitrate before lossy→lossy is even considered.
    pub lossy_min_source_kbps: u32,
}

impl Default for OptimizePolicy {
    fn default() -> Self {
        OptimizePolicy {
            aac_kbps: 64,
            opus_kbps: 32,
            lossy_min_savings: 0.45,
            lossy_min_source_kbps: 160,
        }
    }
}

fn estimate(duration_ms: u64, kbps: u32) -> u64 {
    // bitrate × duration + ~1.5 % container overhead
    let bytes = duration_ms as f64 / 1000.0 * kbps as f64 * 1000.0 / 8.0;
    (bytes * 1.015) as u64
}

pub fn recommend(input: &OptimizeInput, policy: &OptimizePolicy) -> Vec<Recommendation> {
    let mut out = Vec::new();
    if input.duration_ms < 60_000 || input.audio_size == 0 {
        return out;
    }
    let codec = input
        .codec
        .clone()
        .unwrap_or_else(|| "unknown".into())
        .to_ascii_lowercase();
    let src_kbps = input.bitrate_kbps.unwrap_or(
        ((input.audio_size as f64 * 8.0) / (input.duration_ms as f64 / 1000.0) / 1000.0) as u32,
    );
    let non_audio = input.total_size.saturating_sub(input.audio_size);
    let mk =
        |kind, target: TargetFormat, kbps: Option<u32>, est_audio: u64, reencode, notes: String| {
            Recommendation {
                kind,
                target,
                current_codec: codec.to_uppercase(),
                current_bitrate_kbps: Some(src_kbps),
                target_bitrate_kbps: kbps,
                current_size: input.total_size,
                estimated_size: est_audio + non_audio,
                reencode,
                preserves_chapters: true,
                notes,
            }
        };

    if input.is_lossless {
        let aac = estimate(input.duration_ms, policy.aac_kbps);
        out.push(mk(
            ConversionKind::LosslessToLossy,
            TargetFormat::M4bAac,
            Some(policy.aac_kbps),
            aac,
            true,
            "Lossless speech compresses extremely well; AAC at this bitrate is transparent for narration.".into(),
        ));
        let opus = estimate(input.duration_ms, policy.opus_kbps);
        out.push(mk(
            ConversionKind::LosslessToLossy,
            TargetFormat::Opus,
            Some(policy.opus_kbps),
            opus,
            true,
            TargetFormat::Opus.compatibility_note().into(),
        ));
        return out;
    }

    let is_aac = matches!(codec.as_str(), "aac" | "mp4a" | "alac");
    if is_aac && input.audio_file_count > 1 {
        out.push(mk(
            ConversionKind::Repack,
            TargetFormat::M4bAac,
            Some(src_kbps),
            input.audio_size,
            false,
            "Joins AAC tracks into one M4B with chapters per source file; audio is copied bit-for-bit.".into(),
        ));
    }

    if src_kbps >= policy.lossy_min_source_kbps && codec != "opus" {
        let target_kbps = policy.aac_kbps.min(src_kbps);
        let est = estimate(input.duration_ms, target_kbps);
        let ratio = 1.0 - est as f32 / input.audio_size.max(1) as f32;
        if ratio >= policy.lossy_min_savings {
            out.push(mk(
                ConversionKind::LossyToLossy,
                TargetFormat::M4bAac,
                Some(target_kbps),
                est,
                true,
                format!(
                    "Re-encoding lossy {} causes some generation loss; worthwhile only because the source bitrate ({src_kbps} kbps) is far above what speech needs.",
                    codec.to_uppercase()
                ),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hours(h: u64) -> u64 {
        h * 3600 * 1000
    }

    #[test]
    fn flac_gets_lossless_to_lossy() {
        let input = OptimizeInput {
            codec: Some("flac".into()),
            is_lossless: true,
            bitrate_kbps: Some(700),
            channels: Some(2),
            total_size: 5_000_000_000,
            audio_size: 5_000_000_000,
            duration_ms: hours(15),
            audio_file_count: 30,
        };
        let r = recommend(&input, &OptimizePolicy::default());
        assert_eq!(r[0].kind, ConversionKind::LosslessToLossy);
        assert!(r[0].reencode);
        // 15h at 64kbps ≈ 432 MB
        assert!(
            r[0].estimated_size > 400_000_000 && r[0].estimated_size < 470_000_000,
            "{}",
            r[0].estimated_size
        );
        assert!(r[0].savings() > 4_000_000_000);
    }

    #[test]
    fn modest_mp3_not_recompressed() {
        let input = OptimizeInput {
            codec: Some("mp3".into()),
            is_lossless: false,
            bitrate_kbps: Some(96),
            channels: Some(1),
            total_size: 650_000_000,
            audio_size: 650_000_000,
            duration_ms: hours(15),
            audio_file_count: 50,
        };
        assert!(recommend(&input, &OptimizePolicy::default()).is_empty());
    }

    #[test]
    fn mp3_320_offered_as_lossy_to_lossy() {
        let input = OptimizeInput {
            codec: Some("mp3".into()),
            is_lossless: false,
            bitrate_kbps: Some(320),
            channels: Some(2),
            total_size: 2_160_000_000,
            audio_size: 2_160_000_000,
            duration_ms: hours(15),
            audio_file_count: 50,
        };
        let r = recommend(&input, &OptimizePolicy::default());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, ConversionKind::LossyToLossy);
    }

    #[test]
    fn aac_tracks_repack() {
        let input = OptimizeInput {
            codec: Some("aac".into()),
            is_lossless: false,
            bitrate_kbps: Some(64),
            channels: Some(1),
            total_size: 450_000_000,
            audio_size: 440_000_000,
            duration_ms: hours(15),
            audio_file_count: 20,
        };
        let r = recommend(&input, &OptimizePolicy::default());
        assert_eq!(r[0].kind, ConversionKind::Repack);
        assert!(!r[0].reencode);
    }
}
