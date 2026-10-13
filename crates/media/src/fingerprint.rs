//! Acoustic fingerprinting behind [`AudioFingerprintService`].
//!
//! The implementation uses the pure-Rust Chromaprint port (`rusty-chromaprint`)
//! fed by Symphonia-decoded PCM, with FFmpeg as a decoding fallback for codecs
//! Symphonia lacks (e.g. Opus). No other part of the application touches
//! Chromaprint directly.
//!
//! Long audiobooks are never fingerprinted in full: a few strategically placed
//! segments (≈10 %, 50 %, 90 % of the timeline) are fingerprinted and compared
//! with a small offset search to absorb encoder delay differences.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use audiovault_core::duplicate::FingerprintComparison;
use rusty_chromaprint::{Configuration, Fingerprinter};
use serde::{Deserialize, Serialize};

use crate::{Cancel, MediaError, Result};

pub const ALGORITHM: &str = "chromaprint-rs-test2";

/// One audio file of a copy in playback order.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineFile {
    pub path: PathBuf,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SegmentFingerprint {
    pub segment: u32,
    /// Position within the whole copy timeline.
    pub position_ms: u64,
    pub length_ms: u64,
    pub data: Vec<u32>,
}

pub trait AudioFingerprintService: Send + Sync {
    fn algorithm(&self) -> &'static str;
    /// Fingerprint selected segments of a copy.
    fn fingerprint_copy(
        &self,
        files: &[TimelineFile],
        cancel: &dyn Cancel,
    ) -> Result<Vec<SegmentFingerprint>>;
    fn compare(&self, a: &[SegmentFingerprint], b: &[SegmentFingerprint]) -> FingerprintComparison;
}

pub struct ChromaprintService {
    config: Configuration,
    segment_ms: u64,
    ffmpeg: Option<PathBuf>,
}

impl ChromaprintService {
    pub fn new(ffmpeg: Option<PathBuf>) -> Self {
        ChromaprintService {
            config: Configuration::preset_test2(),
            segment_ms: 30_000,
            ffmpeg,
        }
    }

    /// Seconds of audio represented by one fingerprint item.
    fn item_ms(&self) -> f64 {
        self.config.item_duration_in_seconds() as f64 * 1000.0
    }
}

/// Segment positions on the copy timeline.
pub fn segment_positions(total_ms: u64, segment_ms: u64) -> Vec<u64> {
    if total_ms <= segment_ms + 2_000 {
        return vec![0];
    }
    if total_ms < 4 * segment_ms {
        return vec![(total_ms - segment_ms) / 2];
    }
    [0.1, 0.5, 0.9]
        .iter()
        .map(|f| ((total_ms as f64 * f) as u64).min(total_ms - segment_ms))
        .collect()
}

/// Map a timeline position to (file index, offset inside file).
fn locate(files: &[TimelineFile], pos: u64) -> Option<(usize, u64)> {
    let mut acc = 0u64;
    for (i, f) in files.iter().enumerate() {
        if pos < acc + f.duration_ms {
            return Some((i, pos - acc));
        }
        acc += f.duration_ms;
    }
    None
}

impl AudioFingerprintService for ChromaprintService {
    fn algorithm(&self) -> &'static str {
        ALGORITHM
    }

    fn fingerprint_copy(
        &self,
        files: &[TimelineFile],
        cancel: &dyn Cancel,
    ) -> Result<Vec<SegmentFingerprint>> {
        let total: u64 = files.iter().map(|f| f.duration_ms).sum();
        if total == 0 {
            return Err(MediaError::Unsupported("unknown duration".into()));
        }
        let mut out = Vec::new();
        for (i, pos) in segment_positions(total, self.segment_ms)
            .into_iter()
            .enumerate()
        {
            if cancel.is_cancelled() {
                return Err(MediaError::Cancelled);
            }
            let Some((fi, offset)) = locate(files, pos) else {
                continue;
            };
            let file = &files[fi];
            let len = self
                .segment_ms
                .min(file.duration_ms.saturating_sub(offset))
                .max(1);
            if len < 5_000 {
                continue; // segment crosses into a tiny tail; skip rather than fingerprint noise
            }
            let data = match fingerprint_symphonia(&file.path, offset, len, &self.config, cancel) {
                Ok(d) if !d.is_empty() => d,
                Ok(_) | Err(MediaError::Unsupported(_)) | Err(MediaError::Parse(_)) => {
                    match &self.ffmpeg {
                        Some(ff) => {
                            fingerprint_ffmpeg(ff, &file.path, offset, len, &self.config, cancel)?
                        }
                        None => {
                            return Err(MediaError::Unsupported(format!(
                                "cannot decode {}",
                                file.path.display()
                            )));
                        }
                    }
                }
                Err(e) => return Err(e),
            };
            if !data.is_empty() {
                out.push(SegmentFingerprint {
                    segment: i as u32,
                    position_ms: pos,
                    length_ms: len,
                    data,
                });
            }
        }
        if out.is_empty() {
            return Err(MediaError::Unsupported(
                "no segment could be fingerprinted".into(),
            ));
        }
        Ok(out)
    }

    fn compare(&self, a: &[SegmentFingerprint], b: &[SegmentFingerprint]) -> FingerprintComparison {
        compare_segments(a, b, self.item_ms())
    }
}

/// Compare corresponding segments (same index, nearby timeline positions).
pub fn compare_segments(
    a: &[SegmentFingerprint],
    b: &[SegmentFingerprint],
    item_ms: f64,
) -> FingerprintComparison {
    let mut compared = 0u32;
    let mut matched = 0u32;
    let mut sum = 0f32;
    for sa in a {
        let Some(sb) = b.iter().find(|s| s.segment == sa.segment) else {
            continue;
        };
        // Positions are relative fractions of each timeline; allow a shift up to ~4 s
        // plus the position difference (different total durations).
        let pos_diff = sa.position_ms.abs_diff(sb.position_ms) as f64;
        let max_shift = (((4_000.0 + pos_diff.min(20_000.0)) / item_ms) as i64).max(8);
        let sim = best_similarity(&sa.data, &sb.data, max_shift);
        compared += 1;
        sum += sim;
        if sim >= 0.6 {
            matched += 1;
        }
    }
    FingerprintComparison {
        segments_compared: compared,
        segments_matched: matched,
        similarity: if compared > 0 {
            sum / compared as f32
        } else {
            0.0
        },
    }
}

/// Similarity in 0..1 from the bit error rate at the best alignment.
/// Identical audio gives BER ≈ 0–0.15, unrelated audio ≈ 0.5.
pub fn best_similarity(a: &[u32], b: &[u32], max_shift: i64) -> f32 {
    let min_overlap = (a.len().min(b.len()) / 2).max(16);
    let mut best_ber = 1.0f64;
    for shift in -max_shift..=max_shift {
        let (sa, sb) = if shift >= 0 {
            (shift as usize, 0usize)
        } else {
            (0usize, (-shift) as usize)
        };
        if sa >= a.len() || sb >= b.len() {
            continue;
        }
        let n = (a.len() - sa).min(b.len() - sb);
        if n < min_overlap {
            continue;
        }
        let bits: u32 = a[sa..sa + n]
            .iter()
            .zip(&b[sb..sb + n])
            .map(|(x, y)| (x ^ y).count_ones())
            .sum();
        let ber = bits as f64 / (n as f64 * 32.0);
        if ber < best_ber {
            best_ber = ber;
        }
    }
    ((1.0 - 2.0 * best_ber) / 0.7).clamp(0.0, 1.0) as f32
}

fn fingerprint_symphonia(
    path: &Path,
    offset_ms: u64,
    len_ms: u64,
    config: &Configuration,
    cancel: &dyn Cancel,
) -> Result<Vec<u32>> {
    use symphonia::core::codecs::audio::AudioDecoderOptions;
    use symphonia::core::errors::Error as SymErr;
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::units::Time;

    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension() {
        hint.with_extension(&ext.to_string_lossy());
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| MediaError::Unsupported(e.to_string()))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| MediaError::Unsupported("no audio track".into()))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| MediaError::Unsupported("no codec parameters".into()))?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(&params, &AudioDecoderOptions::default())
        .map_err(|e| MediaError::Unsupported(e.to_string()))?;
    if offset_ms > 0
        && let Some(t) = Time::try_from_secs_f64(offset_ms as f64 / 1000.0)
    {
        // Coarse seek is fine: segment alignment is absorbed by the offset search.
        if format
            .seek(
                SeekMode::Coarse,
                SeekTo::Time {
                    time: t,
                    track_id: Some(track_id),
                },
            )
            .is_err()
        {
            return Err(MediaError::Unsupported("stream not seekable".into()));
        }
        decoder.reset();
    }
    let mut printer: Option<(Fingerprinter, u32, u32)> = None;
    let mut consumed_frames: u64 = 0;
    let mut samples: Vec<i16> = Vec::new();
    loop {
        if cancel.is_cancelled() {
            return Err(MediaError::Cancelled);
        }
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymErr::IoError(_)) => break,
            Err(e) => return Err(MediaError::Parse(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buf = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(SymErr::DecodeError(_)) => continue,
            Err(e) => return Err(MediaError::Parse(e.to_string())),
        };
        let rate = buf.spec().rate();
        let channels = buf.spec().channels().count() as u32;
        if rate == 0 || channels == 0 {
            continue;
        }
        if printer.is_none() {
            let mut fp = Fingerprinter::new(config);
            fp.start(rate, channels)
                .map_err(|e| MediaError::Failed(format!("{e:?}")))?;
            printer = Some((fp, rate, channels));
        }
        let Some((fp, prate, _)) = printer.as_mut() else {
            continue;
        };
        buf.copy_to_vec_interleaved::<i16>(&mut samples);
        fp.consume(&samples);
        consumed_frames += buf.frames() as u64;
        if consumed_frames * 1000 >= len_ms * *prate as u64 {
            break;
        }
    }
    match printer {
        Some((mut fp, _, _)) => {
            fp.finish();
            Ok(fp.fingerprint().to_vec())
        }
        None => Ok(Vec::new()),
    }
}

fn fingerprint_ffmpeg(
    ffmpeg: &Path,
    path: &Path,
    offset_ms: u64,
    len_ms: u64,
    config: &Configuration,
    cancel: &dyn Cancel,
) -> Result<Vec<u32>> {
    const RATE: u32 = 11025;
    let mut child = Command::new(ffmpeg)
        .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-ss"])
        .arg(format!("{:.3}", offset_ms as f64 / 1000.0))
        .arg("-t")
        .arg(format!("{:.3}", len_ms as f64 / 1000.0))
        .arg("-i")
        .arg(path)
        .args([
            "-vn",
            "-ac",
            "1",
            "-ar",
            &RATE.to_string(),
            "-f",
            "s16le",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| MediaError::ToolMissing(format!("ffmpeg: {e}")))?;
    let mut fp = Fingerprinter::new(config);
    fp.start(RATE, 1)
        .map_err(|e| MediaError::Failed(format!("{e:?}")))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| MediaError::Failed("no ffmpeg stdout".into()))?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut carry: Option<u8> = None;
    let mut samples = Vec::with_capacity(buf.len() / 2);
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(MediaError::Cancelled);
        }
        let n = stdout.read(&mut buf)?;
        if n == 0 {
            break;
        }
        samples.clear();
        let mut bytes = buf[..n].iter().copied();
        if let Some(lo) = carry.take()
            && let Some(hi) = bytes.next()
        {
            samples.push(i16::from_le_bytes([lo, hi]));
        }
        let rest: Vec<u8> = bytes.collect();
        let mut chunks = rest.chunks_exact(2);
        for c in &mut chunks {
            samples.push(i16::from_le_bytes([c[0], c[1]]));
        }
        carry = chunks.remainder().first().copied();
        fp.consume(&samples);
    }
    let _ = child.wait();
    fp.finish();
    Ok(fp.fingerprint().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NeverCancel;
    use crate::probe::tests::write_wav;

    #[test]
    fn positions() {
        assert_eq!(segment_positions(20_000, 30_000), vec![0]);
        let p = segment_positions(10 * 3600 * 1000, 30_000);
        assert_eq!(p.len(), 3);
        assert!(p[2] + 30_000 <= 10 * 3600 * 1000);
    }

    #[test]
    fn same_audio_matches_different_audio_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.wav");
        let b = dir.path().join("b.wav");
        let c = dir.path().join("c.wav");
        // A chirp-like signal: sum of two tones changing over time gives chroma variation.
        write_chirp(&a, 1.0);
        write_chirp(&b, 1.0);
        write_wav(&c, 20.0, 97.0);
        let svc = ChromaprintService::new(None);
        let tl = |p: &Path| {
            vec![TimelineFile {
                path: p.to_path_buf(),
                duration_ms: 20_000,
            }]
        };
        let fa = svc.fingerprint_copy(&tl(&a), &NeverCancel).unwrap();
        let fb = svc.fingerprint_copy(&tl(&b), &NeverCancel).unwrap();
        let fc = svc.fingerprint_copy(&tl(&c), &NeverCancel).unwrap();
        let same = svc.compare(&fa, &fb);
        assert!(same.is_match(), "{same:?}");
        let diff = svc.compare(&fa, &fc);
        assert!(!diff.is_match(), "{diff:?}");
    }

    fn write_chirp(path: &Path, gain: f32) {
        let rate = 11025u32;
        let n = 20 * rate;
        let mut data = Vec::with_capacity(n as usize * 2);
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let f1 = 220.0 * 2f32.powf(((t * 1.7) as u32 % 12) as f32 / 12.0);
            let f2 = 330.0 * 2f32.powf(((t * 2.3) as u32 % 12) as f32 / 12.0);
            let s = (t * f1 * std::f32::consts::TAU).sin() * 0.5
                + (t * f2 * std::f32::consts::TAU).sin() * 0.3;
            data.extend_from_slice(&((s * 12000.0 * gain) as i16).to_le_bytes());
        }
        let mut f = Vec::new();
        f.extend_from_slice(b"RIFF");
        f.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        f.extend_from_slice(b"WAVEfmt ");
        f.extend_from_slice(&16u32.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&1u16.to_le_bytes());
        f.extend_from_slice(&rate.to_le_bytes());
        f.extend_from_slice(&(rate * 2).to_le_bytes());
        f.extend_from_slice(&2u16.to_le_bytes());
        f.extend_from_slice(&16u16.to_le_bytes());
        f.extend_from_slice(b"data");
        f.extend_from_slice(&(data.len() as u32).to_le_bytes());
        f.extend_from_slice(&data);
        std::fs::write(path, f).unwrap();
    }
}
