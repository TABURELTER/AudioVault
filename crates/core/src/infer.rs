//! Inference of book metadata from folder names, file names and release titles.
//!
//! Every inferred value carries confidence and human-readable evidence so the
//! UI can explain *why* a book is displayed the way it is.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::names::{PersonName, all_tokens_namelike, person_likelihood, split_people};
use crate::normalize::{capitalize_first, clean_title, is_junk_value, is_latin_only, search_norm, tidy, tokens};
use crate::translit::{WordClass, reverse_phrase, transliteration_likelihood};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Inferred {
    pub authors: Vec<String>,
    pub title: Option<String>,
    pub series: Option<String>,
    pub series_index: Option<f32>,
    pub narrators: Vec<String>,
    pub year: Option<i32>,
    pub bitrate_kbps: Option<u32>,
    pub format: Option<String>,
    pub transliterated: bool,
    pub confidence: f32,
    pub evidence: Vec<String>,
}

static BRACKET_GROUP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\[\(\{]([^\]\)\}]*)[\]\)\}]").expect("valid regex"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19[0-9]{2}|20[0-9]{2})\b").expect("valid regex"));
static BITRATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{2,4})\s*(kbps|kbit|кбит|кбпс|k)\b").expect("valid regex"));
static FORMAT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(mp3|m4b|m4a|aac|flac|ogg|opus|wav|ape|alac|wma)\b").expect("valid regex"));
static NARRATOR_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(читает|читают|чит\.?|исп\.?|исполнитель|исполняет|read\s+by|narrated\s+by|narrator)\s*[:\-]?\s*(.+)$")
        .expect("valid regex")
});
static SERIES_HINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(.+?)\s*(?:#|№|-|,)?\s*(?:книга|кн\.|том|book|vol\.?|часть)?\s*(\d{1,3}(?:\.\d)?)\s*$")
        .expect("valid regex")
});
static LEADING_INDEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(?:\(|\[)?(\d{1,3}(?:\.\d)?)(?:\)|\])?\s*(?:[.\-_)]+\s*|\s+-\s+)(.+)$").expect("valid regex")
});
static LEADING_ZERO_INDEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(0\d{1,2})\s+(.+)$").expect("valid regex"));
static BOOK_N: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:книга|кн\.|book|том|vol\.?)\s*(\d{1,3})\b").expect("valid regex"));
static DASH_SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+[-–—]\s+|_-_|\s+[-–—]|[-–—]\s+").expect("valid regex"));

#[derive(Debug, Clone, PartialEq)]
pub struct VolumeInfo {
    pub raw: String,
    pub index: Option<f32>,
}

static VOLUME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(том|vol(?:ume|\.)?|книга|book|часть|part)\s*[-_ .#№]?\s*([0-9]{1,3}(?:\.[0-9]+)?|[ivxlcdm]+)\b")
        .expect("valid regex")
});

fn parse_roman_or_arabic(s: &str) -> Option<f32> {
    if let Ok(num) = s.parse::<f32>() {
        return Some(num);
    }
    let lower = s.to_ascii_lowercase();
    let val = match lower.as_str() {
        "i" => 1.0,
        "ii" => 2.0,
        "iii" => 3.0,
        "iv" => 4.0,
        "v" => 5.0,
        "vi" => 6.0,
        "vii" => 7.0,
        "viii" => 8.0,
        "ix" => 9.0,
        "x" => 10.0,
        "xi" => 11.0,
        "xii" => 12.0,
        _ => return None,
    };
    Some(val)
}

/// Extract volume/tome/part information (e.g. "Том 1", "Vol. 2", "Book 3").
pub fn extract_volume(s: &str) -> Option<VolumeInfo> {
    let caps = VOLUME_REGEX.captures(s)?;
    let word = &caps[1];
    let num_str = &caps[2];
    let index = parse_roman_or_arabic(num_str);
    let cap_word = crate::normalize::capitalize_first(word);
    let formatted_num = if num_str.chars().all(|c| c.is_ascii_digit()) {
        num_str.trim_start_matches('0').to_string()
    } else {
        num_str.to_uppercase()
    };
    let num_display = if formatted_num.is_empty() { "0".to_string() } else { formatted_num };
    Some(VolumeInfo {
        raw: format!("{cap_word} {num_display}"),
        index,
    })
}

/// Container folder names that never carry book identity.
pub fn is_generic_folder(name: &str) -> bool {
    let base = name.rsplit_once('/').map_or(name, |(_, b)| b);
    let base = base.rsplit_once('\\').map_or(base, |(_, b)| b);
    const GENERIC: &[&str] = &[
        "аудиокниги", "аудиокнига", "audiobooks", "audiobook", "audio books", "books", "книги", "downloads", "загрузки",
        "torrents", "торренты", "library", "библиотека", "media", "music", "музыка", "новая папка", "new folder", "mp3",
        "unsorted", "разное", "misc", "temp", "tmp", "мои книги", "все книги", "книги от юли", "новые", "классика",
    ];
    let norm = search_norm(base);
    if GENERIC.contains(&norm.as_str()) {
        return true;
    }
    if let Ok(year) = norm.parse::<u32>() {
        if (1800..=2100).contains(&year) {
            return true;
        }
    }
    if norm.starts_with("лучшие ") {
        return true;
    }
    false
}

/// Folder names that denote a disc/part of a multi-folder book ("CD1", "Disc 2", "Диск 3", "..._CD1").
pub fn is_disc_folder(name: &str) -> bool {
    static DISC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(^\s*(cd|disc|disk|диск|сд|part|часть|ч\.?)\s*[-_ .]?\s*\d{1,4}\s*$|.*[-_ .\(](cd|disc|disk|диск|сд)\s*[-_ .]?\s*\d{1,4}\s*\)?$)").expect("valid regex")
    });
    DISC.is_match(name.trim())
}

/// Folder names that denote a disc, chapter, part, or track subfolder within an audiobook.
pub fn is_part_or_chapter_folder(name: &str) -> bool {
    let trimmed = name.trim();
    if is_generic_folder(trimmed) {
        return false;
    }
    // 4-digit years (e.g. 1800..2100) are collection years or publication years, not chapters/parts
    if let Ok(year) = trimmed.parse::<u32>() {
        if (1800..=2100).contains(&year) {
            return false;
        }
    }
    if is_disc_folder(trimmed) {
        return true;
    }
    static PART_OR_CHAPTER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^((day|день)\s*[-_ .]?\s*\d{1,4}|(mp3|m4b|flac|aac|ogg|wav|wma|audio|аудио)|\d{1,4}\s*[-_ .]+\s*(glava|глава|chapter|part|часть|ч)\b.*|(glava|глава|chapter|part|часть|том|vol|volume|book|книга|ch|gl)\.?\s*[-_ .]?\s*\d{1,4}\b.*|0\d{1,3}|\d{1,3}|\d{1,4}[-_.]\d{1,4}|.*[-_ .\(](cd|disc|disk|диск|сд|part|часть|том|vol|volume)\s*[-_ .]?\s*\d{1,4}\s*\)?|[a-zа-я0-9_-]{2,25}[_-](part|cd|disc|диск|часть|tom|том)?\d{1,3})$"
        ).expect("valid regex")
    });
    if PART_OR_CHAPTER.is_match(trimmed) {
        return true;
    }
    const NON_BOOK_FOLDERS: &[&str] = &[
        "plugins", "plugin", "wa", "avs", "autorun", "scans", "scan", "covers", "cover", "artwork", "art", "bookplayer", "cue",
        "mp3", "m4b", "flac", "aac", "ogg", "wav", "wma", "audio", "аудио",
    ];
    NON_BOOK_FOLDERS.contains(&trimmed.to_lowercase().as_str())
}

struct Partial {
    authors: Vec<String>,
    title: Option<String>,
    series: Option<String>,
    series_index: Option<f32>,
    narrators: Vec<String>,
    year: Option<i32>,
    bitrate: Option<u32>,
    format: Option<String>,
    translit: Option<f32>,
    evidence: Vec<String>,
}

fn looks_personish(s: &str) -> f32 {
    let s = s.trim();
    if s.is_empty() {
        return 0.0;
    }
    if is_latin_only(s) {
        let toks = tokens(s);
        if transliteration_likelihood(&toks) > 0.4 {
            return person_likelihood(&reverse_phrase(&toks).text());
        }
    }
    person_likelihood(s)
}

fn convert_person(s: &str, evidence: &mut Vec<String>) -> String {
    let s = tidy(&s.replace('_', " "));
    if is_latin_only(&s) {
        let (tr, was_tr) = crate::translit::auto_reverse_translit(&s);
        if was_tr {
            evidence.push(format!("name \"{s}\" reverse-transliterated"));
            return PersonName::parse(&tr).display();
        }
    }
    PersonName::parse(&s).display()
}

fn convert_title(s: &str, evidence: &mut Vec<String>) -> (String, Option<f32>) {
    let cleaned = clean_title(s);
    if is_latin_only(&cleaned) {
        let (tr, was_tr) = crate::translit::auto_reverse_translit(&cleaned);
        if was_tr {
            evidence.push(format!("title \"{cleaned}\" reverse-transliterated"));
            return (tr, Some(0.85));
        }
    }
    (cleaned, None)
}

#[allow(dead_code)]
fn title_case_ru(rec: &crate::translit::Reconstruction) -> String {
    let mut out = Vec::with_capacity(rec.words.len());
    for (i, w) in rec.words.iter().enumerate() {
        let is_name = matches!(w.class, Some(WordClass::Given | WordClass::Surname));
        out.push(if i == 0 || is_name { capitalize_first(&w.text) } else { w.text.clone() });
    }
    out.join(" ")
}

fn parse_bracket(inner: &str, p: &mut Partial) {
    let inner = inner.trim();
    if inner.is_empty() {
        return;
    }
    if let Some(c) = YEAR.captures(inner) {
        p.year = p.year.or_else(|| c[1].parse().ok());
    }
    if let Some(c) = BITRATE.captures(inner) {
        p.bitrate = p.bitrate.or_else(|| c[1].parse().ok());
    }
    if let Some(c) = FORMAT.captures(inner) {
        p.format = p.format.clone().or_else(|| Some(c[1].to_uppercase()));
    }
    if let Some(c) = NARRATOR_PREFIX.captures(inner) {
        for n in split_people(c[2].trim()) {
            let disp = convert_person(&n, &mut p.evidence);
            p.evidence.push(format!("narrator \"{disp}\" from bracket \"{inner}\""));
            p.narrators.push(disp);
        }
        return;
    }
    // RuTracker style: [Narrator, 2010, 128 kbps, MP3]
    let parts: Vec<&str> = inner.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.len() >= 2 {
        if let Some(first) = parts.first()
            && !crate::normalize::is_release_noise(first)
            && looks_personish(first) >= 0.3
        {
            for n in split_people(first) {
                let disp = convert_person(&n, &mut p.evidence);
                p.evidence.push(format!("narrator \"{disp}\" from release info"));
                p.narrators.push(disp);
            }
        }
        return;
    }
    if crate::normalize::is_release_noise(inner) {
        return;
    }
    // "(Метро 2033 #1)" / "(Книга 2)"
    if let Some(c) = BOOK_N.captures(inner) {
        p.series_index = p.series_index.or_else(|| c[1].parse().ok());
    }
    if looks_personish(inner) >= 0.45 {
        let disp = convert_person(inner, &mut p.evidence);
        p.evidence.push(format!("narrator \"{disp}\" from parenthesized name"));
        p.narrators.push(disp);
    } else if p.series.is_none()
        && let Some(c) = SERIES_HINT.captures(inner)
        && inner.chars().any(char::is_alphabetic)
        && (inner.contains('#') || inner.contains('№') || BOOK_N.is_match(inner))
    {
        let name = clean_title(c[1].trim_end_matches(['-', ',', '#', '№', ' ']));
        if !name.is_empty() {
            p.series = Some(name);
            p.series_index = c[2].parse().ok();
            p.evidence.push(format!("series from bracket \"{inner}\""));
        }
    }
}

fn split_author_title_tokens(raw_tokens: &[String], p: &mut Partial) {
    // Works on Cyrillic or reverse-transliterated tokens without separators:
    // "gluhovskiy dmitriy metro 2033" → author (2 tokens) + title.
    let latin = raw_tokens.iter().all(|t| !crate::normalize::has_cyrillic(t));
    let lik = if latin { transliteration_likelihood(raw_tokens) } else { 0.0 };
    let (words, conf): (Vec<String>, Option<f32>) = if latin && lik > 0.45 {
        let rec = reverse_phrase(raw_tokens);
        p.translit = Some(rec.confidence);
        (rec.words.iter().map(|w| w.text.clone()).collect(), Some(rec.confidence))
    } else {
        (raw_tokens.to_vec(), None)
    };
    if words.len() >= 3 {
        for take in [3usize, 2] {
            if words.len() <= take {
                continue;
            }
            // Author at the start.
            let head = words[..take].join(" ");
            let head_name = PersonName::parse(&head);
            let head_ok = person_likelihood(&head) >= 0.75
                && all_tokens_namelike(&head)
                && head_name.given.is_some()
                && head_name.surname.is_some();
            if head_ok {
                p.authors.push(head_name.display());
                p.evidence.push(format!("author \"{}\" detected at start of name", head_name.display()));
                let rest = &words[take..];
                p.title = Some(title_from_words(rest, latin && conf.is_some()));
                return;
            }
            // Author at the end.
            let tail = words[words.len() - take..].join(" ");
            let tail_name = PersonName::parse(&tail);
            if person_likelihood(&tail) >= 0.75
                && all_tokens_namelike(&tail)
                && tail_name.given.is_some()
                && tail_name.surname.is_some()
            {
                p.authors.push(tail_name.display());
                p.evidence.push(format!("author \"{}\" detected at end of name", tail_name.display()));
                p.title = Some(title_from_words(&words[..words.len() - take], latin && conf.is_some()));
                return;
            }
        }
    }
    p.title = Some(title_from_words(&words, latin && conf.is_some()));
}

fn title_from_words(words: &[String], reconstructed: bool) -> String {
    let joined = words.join(" ");
    if reconstructed {
        let lex = crate::translit::lexicon();
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let n = search_norm(w);
                let is_name = lex.classes_of(&n).iter().any(|c| matches!(c, WordClass::Given | WordClass::Surname));
                if i == 0 || is_name { capitalize_first(w) } else { w.clone() }
            })
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        joined
    }
}

fn parse_name(name: &str) -> Partial {
    let mut p = Partial {
        authors: vec![],
        title: None,
        series: None,
        series_index: None,
        narrators: vec![],
        year: None,
        bitrate: None,
        format: None,
        translit: None,
        evidence: vec![],
    };
    let mut core = name.to_string();
    for cap in BRACKET_GROUP.captures_iter(name) {
        parse_bracket(&cap[1], &mut p);
    }
    core = BRACKET_GROUP.replace_all(&core, " ").into_owned();
    core = core.replace('_', " ");
    core = tidy(&core);
    // Release suffix noise outside brackets.
    core = clean_title(&core);

    // Leading index "01 - Title" / "01. Title".
    if let Some(c) = LEADING_INDEX.captures(&core).or_else(|| LEADING_ZERO_INDEX.captures(&core)) {
        let rest = c[2].trim().to_string();
        if rest.chars().any(char::is_alphabetic) {
            p.series_index = c[1].parse().ok();
            p.evidence.push(format!("index {} from leading number", &c[1]));
            core = rest;
        }
    }

    let parts: Vec<String> =
        DASH_SPLIT.split(&core).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => {}
        1 => {
            let toks: Vec<String> = parts[0].split_whitespace().map(str::to_string).collect();
            split_author_title_tokens(&toks, &mut p);
        }
        _ => {
            let a = &parts[0];
            let rest = &parts[1..];
            let pa = looks_personish(a);
            let pl = looks_personish(rest.last().map(String::as_str).unwrap_or(""));
            if pa >= 0.3 && pa >= pl {
                for person in split_people(a) {
                    let d = convert_person(&person, &mut p.evidence);
                    p.evidence.push(format!("author \"{d}\" from \"Author - Title\" pattern"));
                    p.authors.push(d);
                }
                assign_title_parts(rest, &mut p);
            } else if pl >= 0.5 && rest.len() == 1 {
                for person in split_people(&rest[0]) {
                    let d = convert_person(&person, &mut p.evidence);
                    p.evidence.push(format!("author \"{d}\" from \"Title - Author\" pattern"));
                    p.authors.push(d);
                }
                assign_title_parts(&parts[..1], &mut p);
            } else {
                // Unknown: assume "Author - Title" convention with low confidence.
                for person in split_people(a) {
                    let d = convert_person(&person, &mut p.evidence);
                    p.evidence.push(format!("author \"{d}\" assumed from \"X - Y\" pattern (low confidence)"));
                    p.authors.push(d);
                }
                assign_title_parts(rest, &mut p);
            }
        }
    }
    if let Some(t) = &p.title
        && is_junk_value(t)
    {
        p.title = None;
    }
    p
}

fn assign_title_parts(rest: &[String], p: &mut Partial) {
    match rest.len() {
        0 => {}
        1 => {
            let (t, c) = convert_title(&rest[0], &mut p.evidence);
            if c.is_some() {
                p.translit = c;
            }
            p.title = Some(t);
        }
        _ => {
            // "Author - Series 01 - Title" or "Author - Title - Narrator"
            let last = rest.last().map(String::as_str).unwrap_or("");
            if rest.len() == 2 && looks_personish(last) >= 0.6 && !last.chars().any(|c| c.is_ascii_digit()) {
                let d = convert_person(last, &mut p.evidence);
                p.evidence.push(format!("narrator \"{d}\" from trailing name"));
                p.narrators.push(d);
                let (t, _) = convert_title(&rest[0], &mut p.evidence);
                p.title = Some(t);
                return;
            }
            let series_part = &rest[0];
            if let Some(c) = SERIES_HINT.captures(series_part)
                && series_part.chars().any(char::is_alphabetic)
            {
                let (s, _) = convert_title(c[1].trim(), &mut p.evidence);
                p.series = Some(s);
                p.series_index = c[2].parse().ok();
                p.evidence.push(format!("series from \"{series_part}\""));
            } else if rest.len() >= 2 && rest[1].chars().all(|c| c.is_ascii_digit() || c == '.') {
                // "Author - Series - 01 - Title"
                let (s, _) = convert_title(series_part, &mut p.evidence);
                p.series = Some(s);
                p.series_index = rest[1].parse().ok();
                p.evidence.push(format!("series from \"{series_part}\""));
                if rest.len() > 2 {
                    let (t, _) = convert_title(&rest[2..].join(" - "), &mut p.evidence);
                    p.title = Some(t);
                }
                return;
            } else {
                let (s, _) = convert_title(series_part, &mut p.evidence);
                p.series = Some(s);
                p.evidence.push(format!("series assumed from \"{series_part}\""));
            }
            let (t, _) = convert_title(&rest[1..].join(" - "), &mut p.evidence);
            p.title = Some(t);
        }
    }
}

/// Parse a single release-style name (folder name, torrent title, file stem).
pub fn parse_release_title(s: &str) -> Inferred {
    let p = parse_name(s);
    finish(p, 0.0)
}

fn finish(p: Partial, bonus: f32) -> Inferred {
    let mut conf: f32 = 0.2 + bonus;
    if !p.authors.is_empty() {
        conf += 0.3;
    }
    if p.title.is_some() {
        conf += 0.2;
    }
    if let Some(t) = p.translit {
        conf *= 0.6 + 0.4 * t;
    }
    let mut authors = p.authors;
    authors.retain(|a| !a.trim().is_empty() && !is_junk_value(a));
    authors.dedup();
    let mut narrators = p.narrators;
    narrators.retain(|a| !a.trim().is_empty());
    narrators.dedup();
    Inferred {
        authors,
        title: p.title.filter(|t| !t.trim().is_empty()),
        series: p.series.filter(|s| !s.trim().is_empty()),
        series_index: p.series_index,
        narrators,
        year: p.year,
        bitrate_kbps: p.bitrate,
        format: p.format,
        transliterated: p.translit.is_some(),
        confidence: conf.clamp(0.0, 0.85),
        evidence: p.evidence,
    }
}

/// Infer metadata from path components below the library root; the last
/// component is the media-set folder (or file stem for single-file books).
pub fn infer_from_path(components: &[&str]) -> Inferred {
    let comps: Vec<&str> = components.iter().copied().filter(|c| !c.trim().is_empty()).collect();
    if comps.is_empty() {
        return Inferred::default();
    }
    // Filter out trailing components that are only chapters, parts, discs, track numbers or utility folders.
    // The actual literary work title resides on the containing book folder.
    let mut book_comps = comps.clone();
    let mut detected_volume: Option<VolumeInfo> = None;
    while book_comps.len() > 1 {
        let last = book_comps.last().unwrap();
        if let Some(vol) = extract_volume(last) {
            if detected_volume.is_none() {
                detected_volume = Some(vol);
            }
            book_comps.pop();
        } else if is_part_or_chapter_folder(last) || is_disc_folder(last) || is_junk_value(last) {
            book_comps.pop();
        } else {
            break;
        }
    }
    let leaf = book_comps.last().unwrap();
    let mut p = parse_name(leaf);
    p.evidence.insert(0, format!("parsed folder name \"{leaf}\""));
    let parents: Vec<&str> = book_comps[..book_comps.len() - 1].iter().rev().copied().filter(|c| !is_generic_folder(c)).collect();

    if p.authors.is_empty() {
        if let Some(parent) = parents.first() {
            let parent_parsed = parse_name(parent);
            if looks_personish(parent) >= 0.45 {
                for person in split_people(parent) {
                    let d = convert_person(&person, &mut p.evidence);
                    p.evidence.push(format!("author \"{d}\" from parent folder \"{parent}\""));
                    p.authors.push(d);
                }
            } else if !parent_parsed.authors.is_empty() && parent_parsed.title.is_some() && p.series.is_none() {
                // Parent "Author - Series"
                p.authors = parent_parsed.authors.clone();
                p.series = parent_parsed.title.clone();
                p.evidence.push(format!("author and series from parent folder \"{parent}\""));
            } else if let Some(grand) = parents.get(1)
                && looks_personish(grand) >= 0.45
            {
                for person in split_people(grand) {
                    let d = convert_person(&person, &mut p.evidence);
                    p.evidence.push(format!("author \"{d}\" from folder \"{grand}\""));
                    p.authors.push(d);
                }
                if p.series.is_none() {
                    let (s, _) = convert_title(parent, &mut p.evidence);
                    p.series = Some(s);
                    p.evidence.push(format!("series from folder \"{parent}\""));
                }
            }
        }
    } else if p.series.is_none()
        && p.series_index.is_some()
        && let Some(parent) = parents.first()
        && looks_personish(parent) < 0.45
    {
        let (s, _) = convert_title(parent, &mut p.evidence);
        p.series = Some(s);
        p.evidence.push(format!("series from parent folder \"{parent}\""));
    }
    if p.series.is_none()
        && p.series_index.is_some()
        && !p.authors.is_empty()
        && parents.len() >= 2
        && let Some(parent) = parents.first()
        && looks_personish(parent) < 0.45
    {
        let (s, _) = convert_title(parent, &mut p.evidence);
        p.series = Some(s);
        p.evidence.push(format!("series from parent folder \"{parent}\""));
    }
    if p.title.is_none()
        && let Some(parent) = parents.first()
    {
        let (t, _) = convert_title(parent, &mut p.evidence);
        p.title = Some(t);
    }
    if let Some(vol) = detected_volume {
        let already_has_vol = extract_volume(leaf).is_some()
            || p.title.as_ref().and_then(|t| extract_volume(t)).is_some();
        if !already_has_vol {
            let base_title = p.title.clone().unwrap_or_else(|| clean_title(leaf));
            if !base_title.is_empty() && !is_junk_value(&base_title) {
                p.title = Some(format!("{base_title}. {}", vol.raw));
                if p.series.is_none() {
                    p.series = Some(base_title);
                }
                if p.series_index.is_none() {
                    p.series_index = vol.index;
                }
                p.evidence.push(format!("volume \"{}\" from path component", vol.raw));
            }
        }
    }
    finish(p, 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translit_folder_name() {
        let i = infer_from_path(&["gluhovskiy_dmitriy_metro_2033"]);
        assert_eq!(i.authors, vec!["Дмитрий Глуховский".to_string()], "{i:?}");
        assert_eq!(i.title.as_deref(), Some("Метро 2033"));
        assert!(i.transliterated);
        assert!(i.confidence > 0.4);
    }

    #[test]
    fn author_dash_title_with_release_info() {
        let i = parse_release_title("Глуховский Дмитрий - Метро 2033 [Кузнецов Сергей, 2007, 128 kbps, MP3]");
        assert_eq!(i.authors, vec!["Дмитрий Глуховский"]);
        assert_eq!(i.title.as_deref(), Some("Метро 2033"));
        assert_eq!(i.narrators, vec!["Сергей Кузнецов"]);
        assert_eq!(i.year, Some(2007));
        assert_eq!(i.bitrate_kbps, Some(128));
        assert_eq!(i.format.as_deref(), Some("MP3"));
    }

    #[test]
    fn hierarchy_author_series_book() {
        let i = infer_from_path(&["Аудиокниги", "Стругацкий Аркадий", "Мир Полудня", "01 - Полдень, XXII век"]);
        assert_eq!(i.authors, vec!["Аркадий Стругацкий"]);
        assert_eq!(i.series.as_deref(), Some("Мир Полудня"));
        assert_eq!(i.series_index, Some(1.0));
        assert_eq!(i.title.as_deref(), Some("Полдень, XXII век"));
    }

    #[test]
    fn narrator_in_parentheses() {
        let i = infer_from_path(&["Достоевский Фёдор - Братья Карамазовы (читает Клюквин Александр)"]);
        assert_eq!(i.authors, vec!["Фёдор Достоевский"]);
        assert_eq!(i.title.as_deref(), Some("Братья Карамазовы"));
        assert_eq!(i.narrators, vec!["Александр Клюквин"]);
    }

    #[test]
    fn title_only_with_parent_author() {
        let i = infer_from_path(&["Лукьяненко Сергей", "Ночной дозор"]);
        assert_eq!(i.authors, vec!["Сергей Лукьяненко"]);
        assert_eq!(i.title.as_deref(), Some("Ночной дозор"));
    }

    #[test]
    fn numeric_titles_are_not_indices() {
        let i = infer_from_path(&["Ильф Илья", "12 стульев"]);
        assert_eq!(i.title.as_deref(), Some("12 стульев"));
        assert_eq!(i.series_index, None);
    }

    #[test]
    fn disc_and_generic_folders() {
        assert!(is_disc_folder("CD1"));
        assert!(is_disc_folder("Disc 02"));
        assert!(is_disc_folder("Диск 3"));
        assert!(!is_disc_folder("Метро 2033"));
        assert!(is_generic_folder("Audiobooks"));
    }

    #[test]
    fn english_is_not_transliterated() {
        let i = parse_release_title("Stephen King - The Shining");
        assert_eq!(i.authors, vec!["Stephen King"]);
        assert_eq!(i.title.as_deref(), Some("The Shining"));
        assert!(!i.transliterated);
    }

    #[test]
    fn test_volume_extraction() {
        let v1 = extract_volume("Том 1").unwrap();
        assert_eq!(v1.raw, "Том 1");
        assert_eq!(v1.index, Some(1.0));

        let v2 = extract_volume("Том 02.m4b").unwrap();
        assert_eq!(v2.raw, "Том 2");
        assert_eq!(v2.index, Some(2.0));

        let v3 = extract_volume("Горький квест. Том 3").unwrap();
        assert_eq!(v3.raw, "Том 3");
        assert_eq!(v3.index, Some(3.0));

        let v4 = extract_volume("Vol. IV").unwrap();
        assert_eq!(v4.raw, "Vol. IV");
        assert_eq!(v4.index, Some(4.0));

        assert!(extract_volume("CD1").is_none());
        assert!(extract_volume("Track 01").is_none());
        assert!(extract_volume("Глава 5").is_none());
    }

    #[test]
    fn test_path_with_volume_preservation() {
        let i1 = infer_from_path(&["аудио", "Горький КВЕСТ", "Том 1"]);
        assert_eq!(i1.title.as_deref(), Some("Горький КВЕСТ. Том 1"));
        assert_eq!(i1.series.as_deref(), Some("Горький КВЕСТ"));
        assert_eq!(i1.series_index, Some(1.0));

        let i2 = infer_from_path(&["аудио", "Горький КВЕСТ", "Том 2"]);
        assert_eq!(i2.title.as_deref(), Some("Горький КВЕСТ. Том 2"));
        assert_eq!(i2.series_index, Some(2.0));
    }
}
