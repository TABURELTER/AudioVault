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

/// Container folder names that never carry book identity.
pub fn is_generic_folder(name: &str) -> bool {
    const GENERIC: &[&str] = &[
        "аудиокниги", "аудиокнига", "audiobooks", "audiobook", "audio books", "books", "книги", "downloads", "загрузки",
        "torrents", "торренты", "library", "библиотека", "media", "music", "музыка", "новая папка", "new folder", "mp3",
        "unsorted", "разное", "misc", "temp", "tmp",
    ];
    GENERIC.contains(&search_norm(name).as_str())
}

/// Folder names that denote a disc/part of a multi-folder book ("CD1", "Disc 2", "Диск 3").
pub fn is_disc_folder(name: &str) -> bool {
    static DISC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*(cd|disc|disk|диск|сд|part|часть|ч\.?)\s*[-_ .]?\s*\d{1,3}\s*$").expect("valid regex")
    });
    DISC.is_match(name)
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
        let toks = tokens(&s);
        let lik = transliteration_likelihood(&toks);
        if lik > 0.4 {
            let rec = reverse_phrase(&toks);
            if rec.confidence > 0.35 {
                evidence.push(format!("name \"{s}\" reverse-transliterated (confidence {:.0}%)", rec.confidence * 100.0));
                return PersonName::parse(&rec.text()).display();
            }
        }
    }
    PersonName::parse(&s).display()
}

fn convert_title(s: &str, evidence: &mut Vec<String>) -> (String, Option<f32>) {
    let cleaned = clean_title(s);
    if is_latin_only(&cleaned) {
        let toks = tokens(&cleaned);
        let lik = transliteration_likelihood(&toks);
        if lik > 0.45 {
            let rec = reverse_phrase(&toks);
            if rec.confidence > 0.35 {
                evidence.push(format!("title \"{cleaned}\" reverse-transliterated (confidence {:.0}%)", rec.confidence * 100.0));
                return (title_case_ru(&rec), Some(rec.confidence));
            }
        }
    }
    (cleaned, None)
}

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
    let Some(leaf) = comps.last() else {
        return Inferred::default();
    };
    let mut p = parse_name(leaf);
    p.evidence.insert(0, format!("parsed folder name \"{leaf}\""));
    let parents: Vec<&str> = comps[..comps.len() - 1].iter().rev().copied().filter(|c| !is_generic_folder(c)).collect();

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
}
