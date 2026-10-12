//! Audio probing: technical properties and embedded tags.
//!
//! lofty is the primary reader (fast, reads tags of all common formats);
//! Symphonia is the fallback for files lofty rejects (partially corrupt or
//! unusual streams) so we still get a duration/codec where possible.

use std::fs::File;
use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag};
use serde::{Deserialize, Serialize};

use crate::text::clean_tag;
use crate::{MediaError, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TagInfo {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub composer: Option<String>,
    /// Explicit narrator/performer/reader tag when present.
    pub narrator: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i32>,
    pub comment: Option<String>,
    pub description: Option<String>,
    pub track_no: Option<u32>,
    pub disc_no: Option<u32>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub duration_ms: Option<u64>,
    pub codec: Option<String>,
    pub container: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bit_depth: Option<u8>,
    pub tags: TagInfo,
    pub has_embedded_cover: bool,
    pub chapter_count: u32,
    /// Set when probing only partially succeeded (e.g. Symphonia fallback).
    pub warning: Option<String>,
}

fn container_for(ext: &str) -> String {
    match ext {
        "m4b" | "m4a" | "mp4" => "mp4".into(),
        other => other.to_string(),
    }
}

/// Probe one audio file. `read_cover` loads embedded pictures (only needed
/// for the first file of a set; skipping it keeps big scans fast).
pub fn probe_file(path: &Path, read_cover: bool) -> Result<ProbeResult> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match probe_lofty(path, &ext, read_cover) {
        Ok(r) => Ok(r),
        Err(lofty_err) => match probe_symphonia(path, &ext) {
            Ok(mut r) => {
                r.warning = Some(format!("tags unreadable: {lofty_err}"));
                Ok(r)
            }
            Err(sym_err) => Err(MediaError::Unsupported(format!("{lofty_err}; fallback: {sym_err}"))),
        },
    }
}

fn probe_lofty(path: &Path, ext: &str, read_cover: bool) -> std::result::Result<ProbeResult, String> {
    let opts = ParseOptions::new().read_cover_art(read_cover);
    let tagged: TaggedFile = Probe::open(path)
        .map_err(|e| e.to_string())?
        .options(opts)
        .guess_file_type()
        .map_err(|e| e.to_string())?
        .read()
        .map_err(|e| e.to_string())?;
    let props = tagged.properties();
    let ftype = tagged.file_type();
    let mut codec = codec_name(ftype).map(str::to_string);
    if ftype == FileType::Mp4 {
        codec = mp4_codec(path).or(codec);
    }
    let duration_ms = props.duration().as_millis() as u64;
    let mut r = ProbeResult {
        duration_ms: (duration_ms > 0).then_some(duration_ms),
        codec,
        container: Some(container_for(ext)),
        bitrate_kbps: props.audio_bitrate().or(props.overall_bitrate()).filter(|b| *b > 0),
        sample_rate: props.sample_rate(),
        channels: props.channels(),
        bit_depth: props.bit_depth(),
        ..Default::default()
    };
    // Prefer the primary tag; fill gaps from the others (e.g. ID3v1 + ID3v2).
    let mut tags: Vec<&Tag> = Vec::new();
    if let Some(t) = tagged.primary_tag() {
        tags.push(t);
    }
    for t in tagged.tags() {
        if !tags.iter().any(|x| std::ptr::eq(*x, t)) {
            tags.push(t);
        }
    }
    for t in tags {
        merge_tag(&mut r.tags, t);
        if t.picture_count() > 0 {
            r.has_embedded_cover = true;
        }
    }
    if matches!(ext, "m4b" | "m4a" | "mp4") {
        r.chapter_count = crate::mp4chapters::count_chapters(path);
    }
    Ok(r)
}

fn codec_name(t: FileType) -> Option<&'static str> {
    Some(match t {
        FileType::Aac => "aac",
        FileType::Aiff => "pcm",
        FileType::Ape => "ape",
        FileType::Flac => "flac",
        FileType::Mpeg => "mp3",
        FileType::Mp4 => "aac",
        FileType::Mpc => "musepack",
        FileType::Opus => "opus",
        FileType::Vorbis => "vorbis",
        FileType::Speex => "speex",
        FileType::Wav => "pcm",
        FileType::WavPack => "wavpack",
        _ => return None,
    })
}

fn mp4_codec(path: &Path) -> Option<String> {
    use lofty::mp4::{Mp4Codec, Mp4File};
    let mut f = File::open(path).ok()?;
    let mp4 = Mp4File::read_from(&mut f, ParseOptions::new().read_tags(false).read_cover_art(false)).ok()?;
    Some(
        match mp4.properties().codec()? {
            Mp4Codec::AAC => "aac",
            Mp4Codec::ALAC => "alac",
            Mp4Codec::MP3 => "mp3",
            Mp4Codec::FLAC => "flac",
            _ => return None,
        }
        .to_string(),
    )
}

fn set_if_none(slot: &mut Option<String>, v: Option<std::borrow::Cow<'_, str>>) {
    if slot.is_none()
        && let Some(v) = v
    {
        *slot = clean_tag(&v);
    }
}

fn merge_tag(out: &mut TagInfo, t: &Tag) {
    set_if_none(&mut out.title, t.title());
    set_if_none(&mut out.artist, t.artist());
    set_if_none(&mut out.album, t.album());
    set_if_none(&mut out.genre, t.genre());
    set_if_none(&mut out.comment, t.comment());
    let get = |k: ItemKey| t.get_string(k).map(std::borrow::Cow::Borrowed);
    set_if_none(&mut out.album_artist, get(ItemKey::AlbumArtist));
    set_if_none(&mut out.composer, get(ItemKey::Composer));
    // Narrator conventions: dedicated keys, then "performer"/"conductor"-style fields
    // that audiobook taggers commonly misuse for the reader.
    for key in [ItemKey::Performer, ItemKey::Conductor] {
        if out.narrator.is_none() {
            set_if_none(&mut out.narrator, get(key));
        }
    }
    // lofty's generic tag drops unmapped custom keys (e.g. TXXX:NARRATOR), so
    // custom narrator frames are not visible here; folder/comment inference
    // in the indexer covers the common cases.
    set_if_none(&mut out.description, get(ItemKey::Description));
    set_if_none(&mut out.language, get(ItemKey::Language));
    set_if_none(&mut out.publisher, get(ItemKey::Publisher).or_else(|| get(ItemKey::Label)));
    set_if_none(&mut out.series, get(ItemKey::Movement).or_else(|| get(ItemKey::ShowName)));
    set_if_none(&mut out.series_index, get(ItemKey::MovementNumber));
    if out.year.is_none() {
        out.year = t.date().map(|d| d.year as i32).filter(|y| (1000..=2100).contains(y)).or_else(|| {
            get(ItemKey::Year)
                .and_then(|s| s.get(..4).and_then(|y| y.parse::<i32>().ok()))
                .filter(|y| (1000..=2100).contains(y))
        });
    }
    if out.track_no.is_none() {
        out.track_no = t.track();
    }
    if out.disc_no.is_none() {
        out.disc_no = t.disk();
    }
}

/// Symphonia fallback: container/codec probing and duration from the track header.
fn probe_symphonia(path: &Path, ext: &str) -> std::result::Result<ProbeResult, String> {
    use symphonia::core::formats::probe::Hint;
    use symphonia::core::formats::{FormatOptions, TrackType};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::units::Timestamp;

    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);
    let format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| e.to_string())?;
    let track = format.default_track(TrackType::Audio).ok_or("no audio track")?;
    let mut r = ProbeResult { container: Some(container_for(ext)), ..Default::default() };
    if let (Some(tb), Some(dur)) = (track.time_base, track.duration) {
        let t = tb.calc_time_saturating(Timestamp::new(dur.get().min(i64::MAX as u64) as i64));
        let ms = (t.as_secs_f64() * 1000.0) as u64;
        r.duration_ms = (ms > 0).then_some(ms);
    }
    if let Some(params) = track.codec_params.as_ref().and_then(|p| p.audio()) {
        r.sample_rate = params.sample_rate;
        r.channels = params.channels.as_ref().map(|c| c.count() as u8);
        let id = format!("{:?}", params.codec).to_ascii_lowercase();
        r.codec = Some(
            if id.contains("mp3") {
                "mp3"
            } else if id.contains("aac") {
                "aac"
            } else if id.contains("flac") {
                "flac"
            } else if id.contains("vorbis") {
                "vorbis"
            } else if id.contains("opus") {
                "opus"
            } else if id.contains("pcm") {
                "pcm"
            } else if id.contains("alac") {
                "alac"
            } else {
                "unknown"
            }
            .to_string(),
        );
    }
    if let (Some(d), Ok(meta)) = (r.duration_ms, std::fs::metadata(path))
        && d > 0
    {
        r.bitrate_kbps = Some(((meta.len() * 8) / d) as u32);
    }
    Ok(r)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Write a valid PCM WAV file (sine wave) for tests.
    pub fn write_wav(path: &Path, seconds: f32, freq: f32) {
        let rate = 8000u32;
        let n = (seconds * rate as f32) as u32;
        let mut data = Vec::with_capacity(n as usize * 2);
        for i in 0..n {
            let s = ((i as f32 / rate as f32) * freq * std::f32::consts::TAU).sin();
            data.extend_from_slice(&((s * 12000.0) as i16).to_le_bytes());
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

    #[test]
    fn probes_wav() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Глава 01.wav");
        write_wav(&p, 2.0, 440.0);
        let r = probe_file(&p, true).unwrap();
        assert_eq!(r.codec.as_deref(), Some("pcm"));
        let d = r.duration_ms.unwrap();
        assert!((1900..=2100).contains(&d), "duration {d}");
        assert_eq!(r.sample_rate, Some(8000));
    }

    #[test]
    fn corrupt_file_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("broken.mp3");
        std::fs::write(&p, b"ID3\x04\x00\x00\x00\x00\x00\x10garbagegarbage").unwrap();
        assert!(probe_file(&p, false).is_err());
    }
}
