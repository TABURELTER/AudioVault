//! Deterministic text normalization used for search keys and identity matching.
//!
//! Original strings are never modified in the catalog; these functions only
//! derive *keys* that are persisted next to the originals.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

/// Lowercase, Unicode-normalized form with punctuation collapsed to single spaces.
///
/// * NFKD + removal of combining marks (Latin diacritics: `ï` → `i`)
/// * Cyrillic `й` is preserved (it is a separate letter, not `и` + breve)
/// * `ё` → `е` (search alias)
/// * underscores, dots, dashes and any other punctuation → space
pub fn search_norm(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev: Option<char> = None;
    let mut pending_space = false;
    for ch in input.nfkd() {
        if is_combining(ch) {
            // Re-compose и + U+0306 (breve) into й; drop every other mark.
            if ch == '\u{0306}' && matches!(prev, Some('и')) {
                out.pop();
                out.push('й');
                prev = Some('й');
            }
            continue;
        }
        for lc in ch.to_lowercase() {
            let lc = if lc == 'ё' { 'е' } else { lc };
            if lc.is_alphanumeric() {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(lc);
                prev = Some(lc);
            } else if lc == '\'' || lc == '’' || lc == 'ʼ' || lc == '`' {
                // Apostrophes inside words are soft-sign markers in transliteration
                // ("Gogol'"); drop them without splitting the word.
                prev = Some(lc);
            } else {
                pending_space = true;
                prev = Some(' ');
            }
        }
    }
    out
}

fn is_combining(ch: char) -> bool {
    matches!(ch as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Split normalized text into tokens.
pub fn tokens(input: &str) -> Vec<String> {
    search_norm(input)
        .split(' ')
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect()
}

/// True when the string contains at least one Cyrillic letter.
pub fn has_cyrillic(s: &str) -> bool {
    s.chars().any(is_cyrillic)
}

pub fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}')
}

/// True when the string has Latin letters and no Cyrillic ones.
pub fn is_latin_only(s: &str) -> bool {
    !has_cyrillic(s) && s.chars().any(|c| c.is_ascii_alphabetic())
}

/// Simplified Cyrillic → Latin transliteration (one canonical convention).
pub fn cyr_to_lat(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for ch in s.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        let rep = match lower {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' => "e",
            'ё' => "yo",
            'ж' => "zh",
            'з' => "z",
            'и' => "i",
            'й' => "y",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ъ' => "",
            'ы' => "y",
            'ь' => "",
            'э' => "e",
            'ю' => "yu",
            'я' => "ya",
            // Ukrainian / Belarusian letters that show up in Russian libraries.
            'і' => "i",
            'ї' => "yi",
            'є' => "ye",
            'ґ' => "g",
            'ў' => "u",
            _ => {
                out.push(lower);
                continue;
            }
        };
        out.push_str(rep);
    }
    out
}

/// Phonetic "skeleton" key that collapses competing transliteration
/// conventions so that Cyrillic and Latin spellings of the same name meet.
///
/// `Фёдор Достоевский`, `Fyodor Dostoevsky`, `Fedor Dostoevskiy` and
/// `Dostoevskij Fedor` all produce the tokens `fedor` / `dostoevski`.
pub fn skeleton(input: &str) -> String {
    let norm = search_norm(input);
    let mut out = String::with_capacity(norm.len() + 8);
    for (i, tok) in norm.split(' ').filter(|t| !t.is_empty()).enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&skeleton_token(tok));
    }
    out
}

/// Skeleton of a single already-normalized token.
pub fn skeleton_token(tok: &str) -> String {
    let lat = cyr_to_lat(tok);
    if lat.chars().all(|c| c.is_ascii_digit()) {
        return lat;
    }
    let mut s = lat;
    // Order matters: longer sequences first.
    const RULES: &[(&str, &str)] = &[
        ("shch", "sh"),
        ("tsch", "ch"),
        ("sch", "sh"),
        ("tch", "ch"),
        ("kh", "h"),
        ("ph", "f"),
        ("ck", "k"),
        ("tz", "c"),
        ("ts", "c"),
        ("x", "ks"),
        ("w", "v"),
        ("q", "k"),
        ("j", "y"),
        ("yo", "e"),
        ("ye", "e"),
    ];
    for (from, to) in RULES {
        if s.contains(from) {
            s = s.replace(from, to);
        }
    }
    s = s.replace('y', "i");
    // Collapse doubled letters ("ii" from "iy", "nn", "ss", ...).
    let mut collapsed = String::with_capacity(s.len());
    let mut last: Option<char> = None;
    for c in s.chars() {
        if Some(c) == last && c.is_alphabetic() {
            continue;
        }
        collapsed.push(c);
        last = Some(c);
    }
    // Trailing "ii"/"iy" were collapsed into "i"; "ia" vs "iia" handled by collapse too.
    collapsed
}

static BRACKETS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\[\(\{【]([^\]\)\}】]*)[\]\)\}】]").expect("valid regex"));

static NOISE_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        \b(19|20)\d\d\b
        | \d{2,3}\s*(kbps|kbit|кбит|кбпс|k)\b
        | \b(mp3|m4b|m4a|aac|flac|ogg|opus|wav|ape|alac|wma|vbr|cbr|abr|lossless)\b
        | \b(аудиокнига|аудиокниги|audiobook|audio\s*book|unabridged|abridged|полная\s+версия|сокращ\w*|копия|copy|дубликат)\b
        | \b(читает|читают|чит|исп|исполнитель|исполняет|narrated|narrator|read\s+by)\b
        | \b(rip|ripped|webrip|rutracker|torrent|kinozal|nnm|кhz|khz|hz|stereo|mono|моно|стерео)\b
        | \b(\d+\s*(ч|час|часов|мин|h|hr|min))\b
        ",
    )
    .expect("valid regex")
});

static RELEASE_SUFFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\s_\-–—.]*(аудиокнига|audiobook|audio\s*book|\(?unabridged\)?)\s*$")
        .expect("valid regex")
});

static TRACK_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(track|трек|глава|chapter|часть|part|cd|disc|диск)?\s*\d{1,4}\s*([\.\-_)\]:]+|\s+)\s*")
        .expect("valid regex")
});

static MULTISPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s{2,}").expect("valid regex"));

/// Whether a bracketed fragment looks like release information rather than
/// part of the title (year, bitrate, codec, "читает ...", etc.).
pub fn is_release_noise(fragment: &str) -> bool {
    let f = fragment.trim();
    if f.is_empty() {
        return true;
    }
    NOISE_WORDS.is_match(f)
}

/// Remove release garbage from a title-like string while keeping meaningful
/// bracketed content (e.g. subtitles). Underscores become spaces.
pub fn clean_title(input: &str) -> String {
    let mut s = input.replace('_', " ");
    s = BRACKETS
        .replace_all(&s, |caps: &regex::Captures<'_>| {
            let inner = caps.get(1).map_or("", |m| m.as_str());
            if is_release_noise(inner) {
                String::from(" ")
            } else {
                caps[0].to_string()
            }
        })
        .into_owned();
    s = RELEASE_SUFFIX.replace(&s, "").into_owned();
    s = MULTISPACE.replace_all(&s, " ").into_owned();
    s.trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '–' | '—' | '.' | ',' | '_'))
        .to_string()
}

/// Strip a leading track/chapter number ("01 - ", "001_", "Track 03 ").
pub fn strip_track_noise(input: &str) -> String {
    let stripped = TRACK_PREFIX.replace(input, "");
    let s = stripped.trim();
    if s.is_empty() {
        input.trim().to_string()
    } else {
        s.to_string()
    }
}

/// Normalized key for titles: cleaned, normalized, `ё`→`е`, leading articles kept.
pub fn title_key(title: &str) -> String {
    skeleton(&clean_title(title))
}

/// Sort key: normalized, leading punctuation removed, numbers zero-padded so
/// "Книга 2" sorts before "Книга 10".
pub fn sort_key(input: &str) -> String {
    let n = search_norm(input);
    let mut out = String::with_capacity(n.len() + 8);
    let mut digits = String::new();
    for c in n.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                out.push_str(&format!("{:0>6}", digits));
                digits.clear();
            }
            out.push(c);
        }
    }
    if !digits.is_empty() {
        out.push_str(&format!("{:0>6}", digits));
    }
    out
}

/// Values that carry no information and must never win metadata resolution.
pub fn is_junk_value(value: &str) -> bool {
    let n = search_norm(value);
    if n.is_empty() {
        return true;
    }
    const JUNK: &[&str] = &[
        "unknown",
        "unknown artist",
        "unknown album",
        "unknown title",
        "various",
        "various artists",
        "artist",
        "album",
        "title",
        "untitled",
        "неизвестен",
        "неизвестный",
        "неизвестный исполнитель",
        "неизвестный альбом",
        "без названия",
        "аудиокнига",
        "audiobook",
        "track",
        "трек",
        "new folder",
        "новая папка",
        "cd",
        "disc",
        "диск",
        "том",
        "часть",
        "mp3",
        "m4b",
        "flac",
        "aac",
        "audio",
        "аудио",
        "book",
        "книга",
        "<unknown>",
    ];
    if JUNK.contains(&n.as_str()) {
        return true;
    }
    // "чит.В.Стельмащук", "читает Иванов", "исп. Петров" are narrator noise, not titles or authors.
    static NARRATOR_NOISE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(чит|читает|исп|исполняет|read\s+by|narrated\s+by)[. :\-].*")
            .expect("valid regex")
    });
    if NARRATOR_NOISE.is_match(&n) {
        return true;
    }
    // "Track 01", "01", "Трек 5", "Глава 3", "13 глава 13", "1-01", "GL_041", "Day 2" are track noise, not titles.
    static TRACKISH: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(\d{1,4}[-_ ]*(track|трек|глава|chapter|part|часть|cd|disc|диск|file|файл|glava|day)[-_ ]*\d*|(track|трек|глава|chapter|part|часть|cd|disc|диск|file|файл|glava|day)[-_ .]*\d{1,4}|\d{1,4}[-_.]\d{1,4}|(gl|ch|cd|диск|day)[-_ ]*\d{1,4}|\d{1,4})$",
        )
        .expect("valid regex")
    });
    TRACKISH.is_match(&n)
}

/// Capitalize the first letter of each word (for names) — preserves the rest.
pub fn capitalize_words(s: &str) -> String {
    s.split(' ')
        .filter(|w| !w.is_empty())
        .map(capitalize_first)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn capitalize_first(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Collapse whitespace and trim.
pub fn tidy(s: &str) -> String {
    MULTISPACE.replace_all(s.trim(), " ").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_norm_basics() {
        assert_eq!(search_norm("Фёдор  ДОСТОЕВСКИЙ"), "федор достоевский");
        assert_eq!(search_norm("Dostoevskiy_Fedor"), "dostoevskiy fedor");
        assert_eq!(search_norm("Достоевский Ф.М."), "достоевский ф м");
        assert_eq!(search_norm("Dostoïevski"), "dostoievski");
        assert_eq!(search_norm("Йошкар-Ола"), "йошкар ола");
        assert_eq!(search_norm("Gogol'"), "gogol");
        assert_eq!(search_norm("___Метро---2033___"), "метро 2033");
    }

    #[test]
    fn skeleton_unifies_transliteration_conventions() {
        let forms = [
            "Фёдор Достоевский",
            "Федор Достоевский",
            "Fedor Dostoevskiy",
            "Fyodor Dostoevsky",
            "Fedor Dostoevskij",
        ];
        let keys: Vec<_> = forms.iter().map(|f| skeleton(f)).collect();
        for k in &keys {
            assert_eq!(k, "fedor dostoevski", "{keys:?}");
        }
        assert_eq!(skeleton("Глуховский"), skeleton("gluhovskiy"));
        assert_eq!(skeleton("Глуховский"), skeleton("Glukhovsky"));
        assert_eq!(skeleton("Чехов"), skeleton("Chekhov"));
        assert_eq!(skeleton("Щукин"), skeleton("Schukin"));
        assert_eq!(skeleton("Щукин"), skeleton("Shchukin"));
        assert_eq!(skeleton("Максим"), skeleton("Maxim"));
        assert_eq!(skeleton("Мария"), skeleton("Maria"));
        assert_eq!(skeleton("Юрий"), skeleton("Yuri"));
        assert_eq!(skeleton("Юрий"), skeleton("Jurij"));
        assert_eq!(skeleton("Цветаева"), skeleton("Tsvetaeva"));
        assert_eq!(skeleton("Метро 2033"), "metro 2033");
    }

    #[test]
    fn clean_title_removes_release_noise() {
        assert_eq!(
            clean_title("Метро 2033 [2007, 128 kbps, MP3]"),
            "Метро 2033"
        );
        assert_eq!(clean_title("Метро_2033_(аудиокнига)"), "Метро 2033");
        assert_eq!(
            clean_title("Пикник на обочине (читает Иванов)"),
            "Пикник на обочине"
        );
        assert_eq!(clean_title("Война и мир (Том 1)"), "Война и мир (Том 1)");
        assert_eq!(clean_title("Солярис - аудиокнига"), "Солярис");
    }

    #[test]
    fn track_noise() {
        assert_eq!(strip_track_noise("01 - Глава первая"), "Глава первая");
        assert_eq!(strip_track_noise("Track 03 Intro"), "Intro");
        assert_eq!(strip_track_noise("2033"), "2033");
        assert!(is_junk_value("Track 01"));
        assert!(is_junk_value("Unknown Artist"));
        assert!(is_junk_value("  "));
        assert!(!is_junk_value("Метро 2033"));
    }

    #[test]
    fn sort_keys_pad_numbers() {
        assert!(sort_key("Книга 2") < sort_key("Книга 10"));
    }
}
