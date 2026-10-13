//! Per-copy analysis: probe audio, read tags and FB2, choose a cover,
//! aggregate technical data, record observations, resolve canonical metadata
//! (merging books whose identity now matches), assign the narrator variant,
//! score quality, compute issues and storage-optimizer recommendations.
//!
//! File I/O happens *outside* the write transaction; all catalog updates for
//! a copy are applied atomically afterwards.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use audiovault_core::duplicate::blocking_keys;
use audiovault_core::identity::{people_surname_key, title_similarity};
use audiovault_core::infer::{extract_volume, infer_from_path, parse_release_title};
use audiovault_core::metadata::{Observation, Resolved, resolve};
use audiovault_core::model::{
    CopyTech, FileKind, IssueKind, MetadataField, MetadataSource, is_lossless_codec,
};
use audiovault_core::names::split_people;
use audiovault_core::normalize::{clean_title, is_junk_value, title_key};
use audiovault_core::optimize::{OptimizeInput, OptimizePolicy, recommend};
use audiovault_core::quality;
use audiovault_db::catalog::{self, EbookRecord, MediaRow, ProbeUpdate};
use audiovault_db::issues::{self, NewIssue};
use audiovault_db::{Db, duplicates as dbdup, optimize as dbopt, rusqlite};
use audiovault_media::cover::{self, Thumbnail};
use audiovault_media::fb2::{Fb2Info, parse_fb2_file};
use audiovault_media::hash::quick_hash;
use audiovault_media::probe::{ProbeResult, probe_file};
use rusqlite::params;

use crate::{IndexerError, JobContext, Result, check, media_err};

/// Analysis level written by [`analyze_copy`] (probed + quick-hashed).
pub const LEVEL_ANALYZED: i64 = 2;

#[derive(Debug, Clone)]
pub struct AnalyzeEnv {
    /// Local thumbnail cache directory.
    pub covers_dir: PathBuf,
    pub policy: OptimizePolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzeOutcome {
    pub copy_id: i64,
    pub book_id: i64,
    /// Set when the copy's book was merged into another existing book.
    pub merged: bool,
}

struct Fb2Parsed {
    media_id: i64,
    info: Option<Fb2Info>,
    error: Option<String>,
}

struct CoverFound {
    source: &'static str,
    source_ref: String,
    thumb: Thumbnail,
}

fn to_update(p: ProbeResult, file_name: &str) -> ProbeUpdate {
    let t = p.tags;
    let title = t
        .title
        .as_deref()
        .or_else(|| {
            let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
            Some(stem)
        })
        .map(|raw| {
            let (clean, _) = audiovault_core::translit::auto_reverse_translit(raw);
            clean
        });
    let artist = t.artist.map(|raw| {
        let (clean, _) = audiovault_core::translit::auto_reverse_translit(&raw);
        clean
    });
    let album = t.album.map(|raw| {
        let (clean, _) = audiovault_core::translit::auto_reverse_translit(&raw);
        clean
    });
    let album_artist = t.album_artist.map(|raw| {
        let (clean, _) = audiovault_core::translit::auto_reverse_translit(&raw);
        clean
    });
    let composer = t.composer.map(|raw| {
        let (clean, _) = audiovault_core::translit::auto_reverse_translit(&raw);
        clean
    });
    let narrator = t.narrator.map(|raw| {
        let (clean, _) = audiovault_core::translit::auto_reverse_translit(&raw);
        clean
    });
    let series = t.series.map(|raw| {
        let (clean, _) = audiovault_core::translit::auto_reverse_translit(&raw);
        clean
    });

    ProbeUpdate {
        duration_ms: p.duration_ms,
        codec: p.codec,
        bitrate_kbps: p.bitrate_kbps,
        sample_rate: p.sample_rate,
        channels: p.channels,
        track_no: t.track_no,
        disc_no: t.disc_no,
        title,
        artist,
        album,
        album_artist,
        composer,
        genre: t.genre,
        year: t.year,
        comment: t.comment,
        has_embedded_cover: p.has_embedded_cover,
        chapter_count: p.chapter_count,
        error: p.warning.filter(|_| p.duration_ms.is_none()),
        narrator,
        series,
        series_index: t.series_index,
        description: t.description,
        language: t.language,
        publisher: t.publisher,
        container: p.container,
    }
}

/// Most common value by normalized key; returns (display, share of `total`).
fn majority<'a>(
    vals: impl Iterator<Item = Option<&'a str>>,
    total: usize,
) -> Option<(String, f32)> {
    let mut groups: HashMap<String, (usize, &'a str)> = HashMap::new();
    for v in vals.flatten() {
        let v = v.trim();
        let k = title_key(v);
        if k.is_empty() {
            continue;
        }
        let e = groups.entry(k).or_insert((0, v));
        e.0 += 1;
    }
    let distinct = groups.len();
    let (count, display) = groups
        .into_values()
        .max_by_key(|(c, d)| (*c, std::cmp::Reverse(d.len())))?;
    let _ = distinct;
    Some((display.to_string(), count as f32 / total.max(1) as f32))
}

fn distinct_keys<'a>(vals: impl Iterator<Item = Option<&'a str>>) -> usize {
    let mut set = std::collections::HashSet::new();
    for v in vals.flatten() {
        let k = title_key(v);
        if !k.is_empty() {
            set.insert(k);
        }
    }
    set.len()
}

/// Observations derived from embedded tags of a copy's audio files.
pub fn tag_observations(audio: &[&MediaRow]) -> Vec<Observation> {
    let n = audio.len();
    let mut obs = Vec::new();
    if n == 0 {
        return obs;
    }
    let src = MetadataSource::EmbeddedTag;
    let single = n == 1;
    let mut title = majority(audio.iter().map(|m| m.tag_album.as_deref()), n).or_else(|| {
        if single {
            majority(audio.iter().map(|m| m.tag_title.as_deref()), n)
        } else {
            None
        }
    });
    let mut tag_detected_vol = None;
    if let Some((t, share)) = &title {
        if extract_volume(t).is_none() {
            let vol_candidate = audio
                .iter()
                .find_map(|m| m.tag_title.as_deref().and_then(extract_volume))
                .or_else(|| audio.iter().find_map(|m| extract_volume(&m.file_name)));
            if let Some(vol) = vol_candidate {
                let combined = format!("{t}. {}", vol.raw);
                title = Some((combined, *share));
                tag_detected_vol = Some(vol);
            }
        }
    }
    let is_valid_author = |a: &str| {
        let trimmed = a.trim();
        !trimmed.is_empty()
            && !is_junk_value(trimmed)
            && !trimmed
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '_' | ' '))
            && trimmed.len() >= 2
    };
    let author = majority(
        audio
            .iter()
            .map(|m| m.tag_album_artist.as_deref().filter(|a| is_valid_author(a))),
        n,
    )
    .or_else(|| {
        majority(
            audio
                .iter()
                .map(|m| m.tag_artist.as_deref().filter(|a| is_valid_author(a))),
            n,
        )
    });
    if let Some((t, share)) = &title {
        let conf = 0.55 + 0.4 * share;
        let parsed = parse_release_title(t);
        if author.is_none()
            && !parsed.authors.is_empty()
            && parsed.title.is_some()
            && t.contains(" - ")
        {
            // Album tag like "Author - Title [release info]".
            let t_clean = parsed.title.clone().unwrap_or_default();
            let (tr_t, _) = audiovault_core::translit::auto_reverse_translit(&t_clean);
            obs.push(Observation::new(
                MetadataField::Title,
                tr_t,
                src,
                conf * 0.85,
            ));
            let a_clean = parsed
                .authors
                .iter()
                .map(|a| {
                    let (ca, _) = audiovault_core::translit::auto_reverse_translit(a);
                    ca
                })
                .collect::<Vec<_>>()
                .join("; ");
            obs.push(Observation::new(
                MetadataField::Author,
                a_clean,
                src,
                conf * 0.8,
            ));
            if !parsed.narrators.is_empty() {
                let n_clean = parsed
                    .narrators
                    .iter()
                    .map(|n| {
                        let (cn, _) = audiovault_core::translit::auto_reverse_translit(n);
                        cn
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                obs.push(Observation::new(
                    MetadataField::Narrator,
                    n_clean,
                    src,
                    conf * 0.7,
                ));
            }
        } else {
            let cleaned = clean_title(t);
            let raw = if cleaned.is_empty() {
                t.clone()
            } else {
                cleaned
            };
            let (tr, _) = audiovault_core::translit::auto_reverse_translit(&raw);
            obs.push(Observation::new(MetadataField::Title, tr, src, conf));
        }
    }
    if let Some(vol) = &tag_detected_vol {
        if let Some(idx) = vol.index {
            let idx_str = if idx.fract() == 0.0 {
                format!("{}", idx as i64)
            } else {
                format!("{idx}")
            };
            obs.push(Observation::new(
                MetadataField::SeriesIndex,
                idx_str,
                src,
                0.75,
            ));
        }
    }
    if let Some((a, share)) = &author {
        let (clean_a, _) = audiovault_core::translit::auto_reverse_translit(a);
        obs.push(Observation::new(
            MetadataField::Author,
            clean_a,
            src,
            0.5 + 0.4 * share,
        ));
    }
    let author_key = author.as_ref().map(|(a, _)| title_key(a));
    if let Some((nar, share)) = majority(audio.iter().map(|m| m.tag_narrator.as_deref()), n) {
        let (clean_nar, _) = audiovault_core::translit::auto_reverse_translit(&nar);
        obs.push(Observation::new(
            MetadataField::Narrator,
            clean_nar,
            src,
            0.6 + 0.3 * share,
        ));
    } else if let Some((comp, share)) = majority(audio.iter().map(|m| m.tag_composer.as_deref()), n)
        && author_key.as_deref() != Some(title_key(&comp).as_str())
    {
        // Russian audiobook rips commonly put the reader into "composer".
        let (clean_comp, _) = audiovault_core::translit::auto_reverse_translit(&comp);
        obs.push(Observation::new(
            MetadataField::Narrator,
            clean_comp,
            src,
            0.35 + 0.2 * share,
        ));
    }
    let simple = [
        (
            MetadataField::Series,
            audio
                .iter()
                .map(|m| m.tag_series.as_deref())
                .collect::<Vec<_>>(),
            0.8,
        ),
        (
            MetadataField::SeriesIndex,
            audio
                .iter()
                .map(|m| m.tag_series_index.as_deref())
                .collect(),
            0.75,
        ),
        (
            MetadataField::Genre,
            audio.iter().map(|m| m.tag_genre.as_deref()).collect(),
            0.6,
        ),
        (
            MetadataField::Language,
            audio.iter().map(|m| m.tag_language.as_deref()).collect(),
            0.6,
        ),
        (
            MetadataField::Publisher,
            audio.iter().map(|m| m.tag_publisher.as_deref()).collect(),
            0.6,
        ),
    ];
    for (field, vals, conf) in simple {
        if let Some((v, share)) = majority(vals.into_iter(), n) {
            let val = if field == MetadataField::Series {
                let (clean_s, _) = audiovault_core::translit::auto_reverse_translit(&v);
                clean_s
            } else {
                v
            };
            obs.push(Observation::new(
                field,
                val,
                src,
                conf * (0.6 + 0.4 * share),
            ));
        }
    }
    let years: Vec<Option<String>> = audio
        .iter()
        .map(|m| m.tag_year.map(|y| y.to_string()))
        .collect();
    if let Some((y, _)) = majority(years.iter().map(|y| y.as_deref()), n) {
        obs.push(Observation::new(MetadataField::Year, y, src, 0.6));
    }
    let desc = audio
        .iter()
        .filter_map(|m| m.tag_description.as_deref().or(m.tag_comment.as_deref()))
        .filter(|d| d.chars().count() >= 40)
        .max_by_key(|d| d.len());
    if let Some(d) = desc {
        obs.push(Observation::new(
            MetadataField::Description,
            d.trim().to_string(),
            src,
            0.5,
        ));
    }
    obs
}

/// Observations from a parsed FB2 file. `reference_title` lowers confidence
/// when the FB2 clearly belongs to a different book.
pub fn fb2_observations(info: &Fb2Info, reference_title: Option<&str>) -> (Vec<Observation>, f32) {
    let src = MetadataSource::Fb2;
    let score = match (info.title.as_deref(), reference_title) {
        (Some(t), Some(r)) => title_similarity(t, r),
        _ => 1.0,
    };
    let shares_word = match (info.title.as_deref(), reference_title) {
        (Some(t), Some(r)) => {
            let words = |s: &str| -> std::collections::HashSet<String> {
                title_key(s)
                    .split_whitespace()
                    .filter(|w| w.chars().count() >= 3)
                    .map(str::to_string)
                    .collect()
            };
            !words(t).is_disjoint(&words(r))
        }
        _ => true,
    };
    let mismatch = score < 0.75 && !shares_word;
    let score = if mismatch { score.min(0.29) } else { score };
    let c = if mismatch { 0.35 } else { 0.95 };
    let mut obs = Vec::new();
    if let Some(t) = &info.title {
        let (clean_t, _) = audiovault_core::translit::auto_reverse_translit(t);
        obs.push(Observation::new(MetadataField::Title, clean_t, src, c));
    }
    if !info.authors.is_empty() {
        let clean_a = info
            .authors
            .iter()
            .map(|a| {
                let (ca, _) = audiovault_core::translit::auto_reverse_translit(a);
                ca
            })
            .collect::<Vec<_>>()
            .join("; ");
        obs.push(Observation::new(MetadataField::Author, clean_a, src, c));
    }
    if let Some((s, idx)) = info.sequences.first() {
        let (clean_s, _) = audiovault_core::translit::auto_reverse_translit(s);
        obs.push(Observation::new(
            MetadataField::Series,
            clean_s,
            src,
            c * 0.95,
        ));
        if let Some(i) = idx {
            let v = if i.fract() == 0.0 {
                format!("{}", *i as i64)
            } else {
                format!("{i}")
            };
            obs.push(Observation::new(
                MetadataField::SeriesIndex,
                v,
                src,
                c * 0.95,
            ));
        }
    }
    if let Some(y) = info.year {
        obs.push(Observation::new(
            MetadataField::Year,
            y.to_string(),
            src,
            c * 0.8,
        ));
    }
    if !info.genres.is_empty() {
        obs.push(Observation::new(
            MetadataField::Genre,
            info.genres.join(", "),
            src,
            c * 0.6,
        ));
    }
    if let Some(a) = &info.annotation {
        obs.push(Observation::new(
            MetadataField::Description,
            a.clone(),
            src,
            c * 0.85,
        ));
    }
    if let Some(l) = &info.language {
        obs.push(Observation::new(
            MetadataField::Language,
            l.clone(),
            src,
            c * 0.8,
        ));
    }
    if let Some(p) = &info.publisher {
        obs.push(Observation::new(
            MetadataField::Publisher,
            p.clone(),
            src,
            c * 0.8,
        ));
    }
    (obs, score)
}

fn container_for(ext: &str) -> String {
    match ext {
        "m4b" | "m4a" | "mp4" | "aac" => "mp4".into(),
        "oga" | "opus" => "ogg".into(),
        other => other.to_string(),
    }
}

/// Aggregate technical summary of a copy from its present media rows.
pub fn aggregate_tech(rows: &[MediaRow], has_cover_thumb: bool) -> CopyTech {
    let audio: Vec<&MediaRow> = rows
        .iter()
        .filter(|m| m.kind == "audio" && m.state == "present")
        .collect();
    let total_size: u64 = rows
        .iter()
        .filter(|m| m.state == "present")
        .map(|m| m.size)
        .sum();
    let audio_size: u64 = audio.iter().map(|m| m.size).sum();
    let duration: u64 = audio.iter().filter_map(|m| m.duration_ms).sum();
    let mut by_codec: HashMap<String, u64> = HashMap::new();
    for m in &audio {
        if let Some(c) = &m.codec {
            *by_codec.entry(c.to_ascii_lowercase()).or_default() += m.size;
        }
    }
    let primary_codec = by_codec.into_iter().max_by_key(|(_, s)| *s).map(|(c, _)| c);
    let largest = audio.iter().max_by_key(|m| m.size);
    let container = largest
        .and_then(|m| m.container.clone())
        .or_else(|| largest.map(|m| container_for(&m.ext)));
    let avg_bitrate = if duration > 0 {
        Some(((audio_size as f64 * 8.0) / (duration as f64 / 1000.0) / 1000.0).round() as u32)
    } else {
        let rates: Vec<u32> = audio.iter().filter_map(|m| m.bitrate_kbps).collect();
        if rates.is_empty() {
            None
        } else {
            Some(rates.iter().sum::<u32>() / rates.len() as u32)
        }
    };
    let first = audio.first();
    CopyTech {
        audio_file_count: audio.len() as u32,
        total_size,
        audio_size,
        total_duration_ms: duration,
        is_lossless: primary_codec.as_deref().is_some_and(is_lossless_codec),
        primary_codec,
        container,
        avg_bitrate_kbps: avg_bitrate,
        sample_rate: first.and_then(|m| m.sample_rate),
        channels: first.and_then(|m| m.channels),
        has_cover: has_cover_thumb
            || rows
                .iter()
                .any(|m| m.kind == "image" && cover::cover_rank(&m.file_name).is_some())
            || audio.iter().any(|m| m.has_embedded_cover),
        has_fb2: rows.iter().any(|m| m.ext == "fb2" && m.state == "present"),
        has_chapters: audio.iter().any(|m| m.chapter_count > 0) || audio.len() > 1,
        probe_errors: audio.iter().filter(|m| m.probe_error.is_some()).count() as u32,
    }
}

fn find_other_book(
    conn: &rusqlite::Connection,
    identity: &str,
    not_id: i64,
) -> Result<Option<i64>> {
    let mut st = conn.prepare_cached(
        "SELECT id FROM book WHERE identity_key = ?1 AND id <> ?2 ORDER BY id LIMIT 1",
    )?;
    let mut rows = st.query(params![identity, not_id])?;
    Ok(match rows.next()? {
        Some(r) => Some(r.get(0)?),
        None => None,
    })
}

/// Best narrator observation recorded for one copy (tags beat folder names).
fn copy_narrators(conn: &rusqlite::Connection, book_id: i64, copy_id: i64) -> Result<Vec<String>> {
    let best = catalog::observations_for_book(conn, book_id)?
        .into_iter()
        .filter(|o| o.copy_id == Some(copy_id) && o.observation.field == MetadataField::Narrator)
        .max_by(|a, b| {
            let sa = a.observation.source.authority() * a.observation.confidence;
            let sb = b.observation.source.authority() * b.observation.confidence;
            sa.total_cmp(&sb)
        });
    Ok(best
        .map(|o| split_people(&o.observation.value))
        .unwrap_or_default())
}

fn can_merge_books(conn: &rusqlite::Connection, from: i64, into: i64) -> Result<bool> {
    let core_from = match catalog::book_core(conn, from)? {
        Some(c) => c,
        None => return Ok(false),
    };
    let core_into = match catalog::book_core(conn, into)? {
        Some(c) => c,
        None => return Ok(false),
    };

    // 1. Both titles must be non-empty and non-junk
    if core_from.title.trim().is_empty() || core_into.title.trim().is_empty() {
        return Ok(false);
    }
    if is_junk_value(&core_from.title) || is_junk_value(&core_into.title) {
        return Ok(false);
    }

    // 2. Volume mismatch check:
    let vol_from = extract_volume(&core_from.title);
    let vol_into = extract_volume(&core_into.title);
    if let (Some(vf), Some(vi)) = (vol_from, vol_into) {
        if vf.index != vi.index || vf.raw.to_lowercase() != vi.raw.to_lowercase() {
            tracing::warn!(
                from,
                into,
                "merge forbidden: different volume numbers {:?} vs {:?}",
                vf.raw,
                vi.raw
            );
            return Ok(false);
        }
    } else if core_from.series_index.is_some()
        && core_into.series_index.is_some()
        && core_from.series_index != core_into.series_index
    {
        tracing::warn!(from, into, "merge forbidden: different series indices");
        return Ok(false);
    }

    // 3. Title similarity: normalized titles must be at least 0.85 similar
    let sim = title_similarity(&core_from.title, &core_into.title);
    if sim < 0.85 {
        tracing::warn!(
            from,
            into,
            "merge forbidden: title similarity too low ({sim:.2}) between {:?} and {:?}",
            core_from.title,
            core_into.title
        );
        return Ok(false);
    }

    // 4. Authors: if both have authors, surname keys must match
    if !core_from.authors.is_empty() && !core_into.authors.is_empty() {
        let key_from = people_surname_key(&core_from.authors);
        let key_into = people_surname_key(&core_into.authors);
        if key_from != key_into {
            tracing::warn!(
                from,
                into,
                "merge forbidden: author surname mismatch {key_from:?} vs {key_into:?}"
            );
            return Ok(false);
        }
    }

    Ok(true)
}

fn resolve_book(conn: &rusqlite::Connection, book_id: i64) -> Result<(i64, Resolved, bool)> {
    let resolved = resolve(&catalog::observations_for_resolution(conn, book_id)?);
    let changed = catalog::apply_canonical(conn, book_id, &resolved)?;
    if changed {
        let core = catalog::book_core(conn, book_id)?
            .ok_or_else(|| IndexerError::Invalid(format!("book {book_id} vanished")))?;
        let has_identity = !core.title.trim().is_empty() && !core.authors.is_empty();
        if has_identity && let Some(other) = find_other_book(conn, &core.identity_key, book_id)? {
            let (from, into) = if other < book_id {
                (book_id, other)
            } else {
                (other, book_id)
            };
            if can_merge_books(conn, from, into)? {
                tracing::info!(from, into, "merging books with identical identity");
                catalog::merge_books(conn, from, into)?;
                let resolved = resolve(&catalog::observations_for_resolution(conn, into)?);
                catalog::apply_canonical(conn, into, &resolved)?;
                return Ok((into, resolved, true));
            } else {
                tracing::info!(
                    from,
                    into,
                    "skipped merge: books failed merge safety checks"
                );
            }
        }
    }
    Ok((book_id, resolved, false))
}

fn conflict_between(
    resolved: &Resolved,
    field: MetadataField,
    a: MetadataSource,
    b: MetadataSource,
) -> Option<(String, String)> {
    let v = resolved.fields.get(&field)?;
    if v.source != a && v.source != b {
        return None;
    }
    let other = if v.source == a { b } else { a };
    v.conflicts
        .iter()
        .find(|(s, _)| *s == other)
        .map(|(_, val)| (v.value.clone(), val.clone()))
}

#[allow(clippy::too_many_arguments)]
fn compute_issues(
    conn: &rusqlite::Connection,
    book_id: i64,
    copy_id: i64,
    copy_rel: &str,
    is_single_file: bool,
    resolved: &Resolved,
    audio: &[&MediaRow],
) -> Result<Vec<NewIssue>> {
    let mut out = Vec::new();
    let issue =
        |kind, msg: String, field: Option<&str>, cur: Option<String>, prop: Option<String>| {
            NewIssue {
                kind,
                copy_id: Some(copy_id),
                message: msg,
                field: field.map(str::to_string),
                current_value: cur,
                proposed_value: prop,
            }
        };
    let title = resolved.get(MetadataField::Title).map(str::to_string);
    if resolved.get(MetadataField::Author).is_none() {
        out.push(issue(
            IssueKind::UnknownAuthor,
            "No author could be determined from tags, FB2 or folder names.".into(),
            Some("author"),
            None,
            None,
        ));
    }
    match (&title, resolved.fields.get(&MetadataField::Title)) {
        (None, _) => out.push(issue(
            IssueKind::MissingTitle,
            "No title found.".into(),
            Some("title"),
            None,
            None,
        )),
        (Some(t), Some(v))
            if matches!(
                v.source,
                MetadataSource::FolderName | MetadataSource::FileName
            ) && v.score < 30.0 =>
        {
            out.push(issue(
                IssueKind::MissingTitle,
                format!("Title \"{t}\" was guessed from the folder name with low confidence."),
                Some("title"),
                Some(t.clone()),
                None,
            ));
        }
        _ => {}
    }
    if let Some(t) = &title {
        let cleaned = clean_title(t);
        if !cleaned.is_empty() && cleaned != *t {
            out.push(issue(
                IssueKind::ReleaseGarbage,
                format!("Title contains release information: \"{t}\"."),
                Some("title"),
                Some(t.clone()),
                Some(cleaned),
            ));
        }
    }
    // Book-level: any copy whose folder name is transliterated.
    let mut copies = catalog::copies_of_book(conn, book_id)?;
    copies.sort_by_key(|c| (c.id != copy_id, c.id));
    for c in copies.iter().filter(|c| c.state == "present") {
        let single = if c.id == copy_id {
            is_single_file
        } else {
            c.is_single_file
        };
        let comps: Vec<String> = c
            .rel_path
            .split('/')
            .map(|s| {
                if single {
                    s.rsplit_once('.').map_or(s, |(a, _)| a).to_string()
                } else {
                    s.to_string()
                }
            })
            .collect();
        let refs: Vec<&str> = comps.iter().map(String::as_str).collect();
        if infer_from_path(&refs).transliterated {
            let folder = comps.last().cloned().unwrap_or_default();
            let mut i = issue(
                IssueKind::TransliteratedFolder,
                format!(
                    "Folder \"{folder}\" uses Latin transliteration; shown as \"{}\".",
                    title.clone().unwrap_or_default()
                ),
                None,
                Some(folder),
                title.clone(),
            );
            i.copy_id = Some(c.id);
            out.push(i);
            break;
        }
    }
    let _ = copy_rel;
    for field in [MetadataField::Title, MetadataField::Author] {
        if let Some((chosen, other)) = conflict_between(
            resolved,
            field,
            MetadataSource::Fb2,
            MetadataSource::EmbeddedTag,
        ) {
            out.push(issue(
                IssueKind::Fb2TagConflict,
                format!(
                    "FB2 and audio tags disagree on {}: \"{chosen}\" vs \"{other}\".",
                    field.as_str()
                ),
                Some(field.as_str()),
                Some(chosen),
                Some(other),
            ));
        }
    }
    if audio.len() > 1 {
        let albums = distinct_keys(audio.iter().map(|m| m.tag_album.as_deref()));
        if albums > 1
            && let Some((top, share)) =
                majority(audio.iter().map(|m| m.tag_album.as_deref()), audio.len())
            && share < 0.8
        {
            out.push(issue(
                IssueKind::TracksDisagree,
                format!("Tracks carry {albums} different album tags (most common: \"{top}\")."),
                Some("album"),
                None,
                Some(top),
            ));
        }
    }
    let narrators: i64 = conn.query_row(
        "SELECT COUNT(*) FROM audio_variant WHERE book_id = ?1 AND narrator_key <> ''",
        [book_id],
        |r| r.get(0),
    )?;
    if narrators == 0 {
        out.push(issue(
            IssueKind::MissingNarrator,
            "Narrator is unknown for all copies.".into(),
            Some("narrator"),
            None,
            None,
        ));
    }
    let probe_errors: i64 = conn.query_row(
        "SELECT COALESCE(SUM(probe_errors), 0) FROM book_copy WHERE book_id = ?1",
        [book_id],
        |r| r.get(0),
    )?;
    if probe_errors > 0 {
        out.push(issue(
            IssueKind::ProbeErrors,
            format!("{probe_errors} audio file(s) could not be read."),
            None,
            None,
            None,
        ));
    }
    Ok(out)
}

/// Analyze one copy. The copy's storage must be mounted at `mount`.
pub fn analyze_copy(
    db: &Db,
    copy_id: i64,
    mount: &Path,
    env: &AnalyzeEnv,
    ctx: &dyn JobContext,
) -> Result<AnalyzeOutcome> {
    let (copy, rows) = {
        let conn = db.conn()?;
        let copy = catalog::get_copy(&conn, copy_id)?
            .ok_or_else(|| IndexerError::Invalid(format!("copy {copy_id} not found")))?;
        let rows = catalog::media_for_copy(&conn, copy_id)?;
        (copy, rows)
    };
    if !mount.exists() {
        return Err(IndexerError::VolumeUnavailable(mount.display().to_string()));
    }

    // ---------------------------------------------------------- I/O phase
    let mut probes: Vec<(i64, ProbeUpdate)> = Vec::new();
    let mut hashes: Vec<(i64, String, u64, i64)> = Vec::new();
    let mut fb2s: Vec<Fb2Parsed> = Vec::new();
    let present: Vec<&MediaRow> = rows.iter().filter(|m| m.state == "present").collect();
    let audio_count = present.iter().filter(|m| m.kind == "audio").count().max(1);
    for (i, m) in present.iter().enumerate() {
        check(ctx)?;
        let path = crate::abs_path(mount, &m.rel_path);
        match FileKind::parse(&m.kind) {
            Some(FileKind::Audio) => {
                if !m.probed {
                    let upd = match probe_file(&path, false) {
                        Ok(p) => to_update(p, &m.file_name),
                        Err(e) => {
                            if !mount.exists() {
                                return Err(IndexerError::VolumeUnavailable(
                                    mount.display().to_string(),
                                ));
                            }
                            ProbeUpdate {
                                error: Some(e.to_string()),
                                ..Default::default()
                            }
                        }
                    };
                    probes.push((m.id, upd));
                }
                if m.quick_hash.is_none() {
                    match quick_hash(&path) {
                        Ok(h) => hashes.push((m.id, h, m.size, m.mtime)),
                        Err(e) => {
                            let e = media_err(e, mount);
                            if matches!(e, IndexerError::VolumeUnavailable(_)) {
                                return Err(e);
                            }
                            tracing::debug!(path = %path.display(), error = %e, "quick hash failed");
                        }
                    }
                }
                ctx.progress(i as f64 / audio_count as f64, &m.file_name);
            }
            Some(FileKind::Ebook) if m.ext == "fb2" => match parse_fb2_file(&path) {
                Ok(info) => fb2s.push(Fb2Parsed {
                    media_id: m.id,
                    info: Some(info),
                    error: None,
                }),
                Err(e) => fb2s.push(Fb2Parsed {
                    media_id: m.id,
                    info: None,
                    error: Some(e.to_string()),
                }),
            },
            _ => {}
        }
    }

    // Cover: folder image → embedded picture → FB2 cover.
    let mut cover_found: Option<CoverFound> = None;
    let images: Vec<&MediaRow> = present
        .iter()
        .copied()
        .filter(|m| m.kind == "image")
        .collect();
    if let Some(name) = cover::pick_folder_cover(images.iter().map(|m| m.file_name.as_str()))
        && let Some(m) = images.iter().find(|m| m.file_name == name)
    {
        match cover::thumbnail_from_file(&crate::abs_path(mount, &m.rel_path), &env.covers_dir) {
            Ok(t) => {
                cover_found = Some(CoverFound {
                    source: "folder",
                    source_ref: m.rel_path.clone(),
                    thumb: t,
                })
            }
            Err(e) => tracing::debug!(error = %e, "folder cover unusable"),
        }
    }
    if cover_found.is_none() {
        let embedded = present.iter().filter(|m| m.kind == "audio").find(|m| {
            m.has_embedded_cover
                || probes
                    .iter()
                    .any(|(id, p)| *id == m.id && p.has_embedded_cover)
        });
        if let Some(m) = embedded
            && let Some(bytes) = cover::embedded_cover(&crate::abs_path(mount, &m.rel_path))
            && let Ok(t) = cover::thumbnail_from_bytes(&bytes, &env.covers_dir)
        {
            cover_found = Some(CoverFound {
                source: "embedded",
                source_ref: m.rel_path.clone(),
                thumb: t,
            });
        }
    }
    if cover_found.is_none() {
        for f in &fb2s {
            if let Some(c) = f.info.as_ref().and_then(|i| i.cover.as_ref())
                && let Ok(t) = cover::thumbnail_from_bytes(&c.data, &env.covers_dir)
            {
                let rel = present
                    .iter()
                    .find(|m| m.id == f.media_id)
                    .map(|m| m.rel_path.clone())
                    .unwrap_or_default();
                cover_found = Some(CoverFound {
                    source: "fb2",
                    source_ref: rel,
                    thumb: t,
                });
                break;
            }
        }
    }
    check(ctx)?;

    // ---------------------------------------------------------- write phase
    let covers_dir = env.covers_dir.clone();
    let policy = env.policy;
    db.write_with(|tx| {
        // The copy may have been removed/merged while we were reading files.
        let Some(copy_now) = catalog::get_copy(tx, copy_id)? else {
            return Ok(AnalyzeOutcome {
                copy_id,
                book_id: copy.book_id,
                merged: false,
            });
        };
        for (id, upd) in &probes {
            catalog::update_media_probe(tx, *id, upd)?;
        }
        for (id, h, size, mtime) in &hashes {
            catalog::update_media_hash(tx, *id, Some(h), None, *size, *mtime)?;
        }
        let rows = catalog::media_for_copy(tx, copy_id)?;
        let audio: Vec<&MediaRow> = rows
            .iter()
            .filter(|m| m.kind == "audio" && m.state == "present")
            .collect();
        let mut obs = tag_observations(&audio);
        let reference_title = obs
            .iter()
            .find(|o| o.field == MetadataField::Title)
            .map(|o| o.value.clone())
            .or_else(|| {
                catalog::book_core(tx, copy_now.book_id)
                    .ok()
                    .flatten()
                    .map(|b| b.title)
            });
        for f in &fb2s {
            let mut rec = EbookRecord {
                format: "fb2".into(),
                error: f.error.clone(),
                ..Default::default()
            };
            if let Some(info) = &f.info {
                let (fobs, score) = fb2_observations(info, reference_title.as_deref());
                obs.extend(fobs);
                let title_clean = info.title.as_ref().map(|t| {
                    let (res, _) = audiovault_core::translit::auto_reverse_translit(t);
                    res
                });
                let authors_clean: Vec<String> = info
                    .authors
                    .iter()
                    .map(|a| {
                        let (res, _) = audiovault_core::translit::auto_reverse_translit(a);
                        res
                    })
                    .collect();
                let series_clean = info.sequences.first().map(|s| {
                    let (res, _) = audiovault_core::translit::auto_reverse_translit(&s.0);
                    res
                });
                rec = EbookRecord {
                    format: "fb2".into(),
                    title: title_clean.or_else(|| info.title.clone()),
                    authors: authors_clean,
                    series: series_clean,
                    series_index: info.sequences.first().and_then(|s| s.1),
                    genres: info.genres.clone(),
                    annotation: info.annotation.clone(),
                    language: info.language.clone(),
                    publisher: info.publisher.clone(),
                    year: info.year,
                    document_id: info.document_id.clone(),
                    has_cover: info.cover.is_some(),
                    match_score: Some(score),
                    error: None,
                };
            }
            catalog::upsert_ebook(tx, copy_id, f.media_id, &rec)?;
        }
        catalog::replace_copy_observations(
            tx,
            copy_now.book_id,
            copy_id,
            &[MetadataSource::EmbeddedTag, MetadataSource::Fb2],
            &obs,
        )?;

        let (book_id, resolved, merged) = resolve_book(tx, copy_now.book_id)?;

        let narrators = copy_narrators(tx, book_id, copy_id)?;
        if !narrators.is_empty() {
            catalog::assign_copy_narrators(tx, copy_id, &narrators)?;
        }

        let tech = aggregate_tech(&rows, cover_found.is_some());
        catalog::update_copy_tech(
            tx,
            copy_id,
            &tech,
            LEVEL_ANALYZED,
            narrators.first().map(String::as_str),
        )?;

        if let Some(c) = &cover_found {
            let rel = c
                .thumb
                .path
                .strip_prefix(&covers_dir)
                .unwrap_or(&c.thumb.path);
            let rel = audiovault_core::paths::to_db_rel(rel);
            catalog::set_copy_cover(
                tx,
                book_id,
                Some(copy_id),
                c.source,
                Some(&c.source_ref),
                Some((c.thumb.width, c.thumb.height)),
                Some(&c.thumb.content_hash),
                Some(&rel),
            )?;
        }

        if let Some(q) = catalog::quality_inputs(tx, &[copy_id])?.get(&copy_id) {
            catalog::set_copy_quality(tx, copy_id, quality::score(q).score)?;
        }

        let recs = recommend(
            &OptimizeInput {
                codec: tech.primary_codec.clone(),
                is_lossless: tech.is_lossless,
                bitrate_kbps: tech.avg_bitrate_kbps,
                channels: tech.channels,
                total_size: tech.total_size,
                audio_size: tech.audio_size,
                duration_ms: tech.total_duration_ms,
                audio_file_count: tech.audio_file_count,
            },
            &policy,
        );
        dbopt::replace_for_copy(tx, copy_id, &recs)?;

        catalog::delete_orphan_books(tx)?;
        let issues = compute_issues(
            tx,
            book_id,
            copy_id,
            &copy_now.rel_path,
            copy_now.is_single_file,
            &resolved,
            &audio,
        )?;
        issues::replace_for_book(tx, book_id, &issues)?;

        // Duplicate blocking keys reflect the freshly resolved metadata.
        if let Some(p) = dbdup::load_profiles(tx, &[copy_id])?.get(&copy_id) {
            dbdup::replace_block_keys(tx, copy_id, &blocking_keys(p))?;
        }
        catalog::refresh_book(tx, book_id)?;
        Ok::<_, IndexerError>(AnalyzeOutcome {
            copy_id,
            book_id,
            merged,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(album: Option<&str>, artist: Option<&str>, composer: Option<&str>) -> MediaRow {
        MediaRow {
            kind: "audio".into(),
            state: "present".into(),
            tag_album: album.map(str::to_string),
            tag_artist: artist.map(str::to_string),
            tag_composer: composer.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn tag_majority_and_narrator_from_composer() {
        let rows = [
            row(
                Some("Метро 2033"),
                Some("Дмитрий Глуховский"),
                Some("Сергей Кузнецов"),
            ),
            row(
                Some("Метро 2033"),
                Some("Дмитрий Глуховский"),
                Some("Сергей Кузнецов"),
            ),
            row(
                Some("Метро 2033 (часть 2)"),
                Some("Дмитрий Глуховский"),
                None,
            ),
        ];
        let refs: Vec<&MediaRow> = rows.iter().collect();
        let obs = tag_observations(&refs);
        let title = obs
            .iter()
            .find(|o| o.field == MetadataField::Title)
            .unwrap();
        assert_eq!(title.value, "Метро 2033");
        assert!(title.confidence > 0.8);
        assert!(
            obs.iter()
                .any(|o| o.field == MetadataField::Author && o.value == "Дмитрий Глуховский")
        );
        assert!(
            obs.iter()
                .any(|o| o.field == MetadataField::Narrator && o.value == "Сергей Кузнецов")
        );
    }

    #[test]
    fn album_with_author_dash_title() {
        let rows = [row(
            Some("Глуховский Дмитрий - Метро 2033 [Кузнецов Сергей]"),
            None,
            None,
        )];
        let refs: Vec<&MediaRow> = rows.iter().collect();
        let obs = tag_observations(&refs);
        assert!(
            obs.iter()
                .any(|o| o.field == MetadataField::Title && o.value == "Метро 2033"),
            "{obs:?}"
        );
        assert!(
            obs.iter()
                .any(|o| o.field == MetadataField::Author && o.value.contains("Глуховский"))
        );
    }

    #[test]
    fn fb2_for_other_book_is_down_weighted() {
        let info = Fb2Info {
            title: Some("Совсем другая книга".into()),
            ..Default::default()
        };
        let (obs, score) = fb2_observations(&info, Some("Метро 2033"));
        assert!(score < 0.3);
        assert!(obs[0].confidence < 0.5);
    }

    #[test]
    fn tech_aggregation() {
        let mut a = row(None, None, None);
        a.size = 1_000_000;
        a.duration_ms = Some(60_000);
        a.codec = Some("MP3".into());
        a.ext = "mp3".into();
        let mut b = a.clone();
        b.chapter_count = 0;
        let mut img = MediaRow {
            kind: "image".into(),
            state: "present".into(),
            file_name: "cover.jpg".into(),
            size: 5000,
            ..Default::default()
        };
        img.ext = "jpg".into();
        let t = aggregate_tech(&[a, b, img], false);
        assert_eq!(t.audio_file_count, 2);
        assert_eq!(t.total_duration_ms, 120_000);
        assert_eq!(t.primary_codec.as_deref(), Some("mp3"));
        assert_eq!(t.avg_bitrate_kbps, Some(133));
        assert!(t.has_cover);
        assert!(!t.is_lossless);
        assert_eq!(t.total_size, 2_005_000);
    }
}
