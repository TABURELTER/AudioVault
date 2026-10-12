//! Deterministic Latin → Russian reverse transliteration.
//!
//! Strategy:
//! 1. Dictionary recovery: every lexicon word is indexed by its transliteration
//!    [`skeleton`](crate::normalize::skeleton_token). A Latin token whose skeleton
//!    matches gets the exact Cyrillic spelling (including `ь`, `ё`, `й`).
//! 2. Out-of-lexicon tokens are decoded with a beam search over competing
//!    transliteration conventions (`ya/ja/ia`, `kh/h`, `shch/sch`, `ts/c`,
//!    `iy/ij/y`, ...). Candidates are scored by rule cost plus a character bigram
//!    language model trained on the embedded lexicon.
//!
//! The output is a *candidate* with a confidence value; nothing here renames files.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::normalize::{search_norm, skeleton_token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WordClass {
    Given,
    Surname,
    Word,
}

#[derive(Debug)]
pub struct Lexicon {
    by_skeleton: HashMap<String, Vec<(String, WordClass)>>,
    by_cyrillic: HashMap<String, Vec<WordClass>>,
    bigram: HashMap<(char, char), u32>,
    unigram: HashMap<char, u32>,
}

const LEXICON_SRC: &str = include_str!("../data/lexicon_ru.txt");

static LEXICON: LazyLock<Lexicon> = LazyLock::new(|| Lexicon::parse(LEXICON_SRC));

pub fn lexicon() -> &'static Lexicon {
    &LEXICON
}

impl Lexicon {
    fn parse(src: &str) -> Self {
        let mut lex = Lexicon {
            by_skeleton: HashMap::new(),
            by_cyrillic: HashMap::new(),
            bigram: HashMap::new(),
            unigram: HashMap::new(),
        };
        let mut class = WordClass::Word;
        for line in src.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line {
                "[given]" => {
                    class = WordClass::Given;
                    continue;
                }
                "[surname]" => {
                    class = WordClass::Surname;
                    continue;
                }
                "[word]" => {
                    class = WordClass::Word;
                    continue;
                }
                _ => {}
            }
            for word in line.split_whitespace() {
                let lower = word.to_lowercase();
                if !lower.chars().any(crate::normalize::is_cyrillic) {
                    continue;
                }
                lex.add(&lower, class);
            }
        }
        lex
    }

    fn add(&mut self, lower: &str, class: WordClass) {
        let norm = search_norm(lower);
        let classes = self.by_cyrillic.entry(norm.clone()).or_default();
        if classes.contains(&class) {
            return;
        }
        classes.push(class);
        let skel = skeleton_token(&norm);
        let entry = self.by_skeleton.entry(skel).or_default();
        if !entry.iter().any(|(w, c)| w == lower && *c == class) {
            entry.push((lower.to_string(), class));
        }
        // Train the bigram model on the ё-less normalized form plus boundaries.
        let mut prev = '^';
        for c in norm.chars().chain(std::iter::once('$')) {
            *self.bigram.entry((prev, c)).or_insert(0) += 1;
            *self.unigram.entry(prev).or_insert(0) += 1;
            prev = c;
        }
    }

    /// Exact lexicon lookup of a normalized Cyrillic word.
    pub fn classes_of(&self, cyr_norm: &str) -> &[WordClass] {
        self.by_cyrillic.get(cyr_norm).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn is_given_name(&self, word: &str) -> bool {
        let n = search_norm(word);
        if crate::normalize::has_cyrillic(&n) {
            return self.classes_of(&n).contains(&WordClass::Given);
        }
        self.lookup_skeleton(&n).iter().any(|(_, c)| *c == WordClass::Given)
    }

    pub fn is_known_surname(&self, word: &str) -> bool {
        let n = search_norm(word);
        if crate::normalize::has_cyrillic(&n) {
            return self.classes_of(&n).contains(&WordClass::Surname);
        }
        self.lookup_skeleton(&n).iter().any(|(_, c)| *c == WordClass::Surname)
    }

    /// Lexicon words whose transliteration skeleton equals that of `latin_token`.
    pub fn lookup_skeleton(&self, latin_token: &str) -> &[(String, WordClass)] {
        let skel = skeleton_token(&search_norm(latin_token));
        self.by_skeleton.get(&skel).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Log-probability of a Cyrillic word under the bigram model (add-α smoothing).
    pub fn log_prob(&self, word: &str) -> f32 {
        const ALPHA: f32 = 0.2;
        const VOCAB: f32 = 40.0;
        let mut lp = 0.0;
        let mut prev = '^';
        let normalized: String = word.chars().map(|c| if c == 'ё' { 'е' } else { c }).collect();
        for c in normalized.chars().chain(std::iter::once('$')) {
            let bi = *self.bigram.get(&(prev, c)).unwrap_or(&0) as f32;
            let uni = *self.unigram.get(&prev).unwrap_or(&0) as f32;
            lp += ((bi + ALPHA) / (uni + ALPHA * VOCAB)).ln();
            prev = c;
        }
        lp
    }
}

/// A reverse-transliteration candidate for a single word.
#[derive(Debug, Clone, PartialEq)]
pub struct WordCandidate {
    pub text: String,
    pub class: Option<WordClass>,
    /// 0..1
    pub confidence: f32,
    pub from_lexicon: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    Any,
    End,
    Start,
    AfterVowel,
    AfterConsonant,
    EndAfterConsonant,
}

struct Rule {
    lat: &'static str,
    ctx: Ctx,
    outs: &'static [(&'static str, f32)],
}

macro_rules! r {
    ($lat:expr, $ctx:expr, [$(($o:expr, $c:expr)),* $(,)?]) => {
        Rule { lat: $lat, ctx: $ctx, outs: &[$(($o, $c)),*] }
    };
}

static RULES: &[Rule] = &[
    r!("shch", Ctx::Any, [("щ", 0.0)]),
    r!("sch", Ctx::Any, [("щ", 0.2), ("сч", 0.9), ("ш", 1.2)]),
    r!("tch", Ctx::Any, [("ч", 0.4), ("тч", 1.0)]),
    r!("zh", Ctx::Any, [("ж", 0.0), ("зх", 2.5)]),
    r!("kh", Ctx::Any, [("х", 0.0), ("кх", 2.0)]),
    r!("ch", Ctx::Any, [("ч", 0.0)]),
    r!("sh", Ctx::Any, [("ш", 0.0), ("сх", 1.6)]),
    r!("ts", Ctx::Any, [("ц", 0.5), ("тс", 0.7)]),
    r!("tz", Ctx::Any, [("ц", 0.6)]),
    r!("iy", Ctx::End, [("ий", 0.0), ("ый", 0.9)]),
    r!("yy", Ctx::End, [("ый", 0.0), ("ий", 0.8)]),
    r!("ij", Ctx::End, [("ий", 0.2)]),
    r!("ii", Ctx::End, [("ий", 0.3), ("ии", 0.8)]),
    r!("yi", Ctx::End, [("ый", 0.5), ("ий", 0.6)]),
    r!("ya", Ctx::AfterConsonant, [("ья", 0.7), ("я", 0.3)]),
    r!("ya", Ctx::Any, [("я", 0.0)]),
    r!("ja", Ctx::Any, [("я", 0.3)]),
    r!("ia", Ctx::Any, [("ия", 0.3), ("я", 0.9)]),
    r!("yu", Ctx::AfterConsonant, [("ью", 0.7), ("ю", 0.3)]),
    r!("yu", Ctx::Any, [("ю", 0.0)]),
    r!("ju", Ctx::Any, [("ю", 0.3)]),
    r!("iu", Ctx::Any, [("ию", 0.4), ("ю", 1.0)]),
    r!("yo", Ctx::Any, [("ё", 0.2), ("йо", 1.0), ("ьо", 1.5)]),
    r!("jo", Ctx::Any, [("ё", 0.4)]),
    r!("ye", Ctx::AfterConsonant, [("ье", 0.5), ("е", 0.6)]),
    r!("ye", Ctx::Any, [("е", 0.3)]),
    r!("je", Ctx::Any, [("е", 0.6)]),
    r!("ie", Ctx::Any, [("ие", 0.2), ("ье", 0.7)]),
    r!("ph", Ctx::Any, [("ф", 0.5)]),
    r!("th", Ctx::Any, [("т", 0.8)]),
    r!("ck", Ctx::Any, [("к", 0.5)]),
    r!("x", Ctx::Any, [("кс", 0.0)]),
    r!("w", Ctx::Any, [("в", 0.5), ("у", 1.5)]),
    r!("q", Ctx::Any, [("к", 1.0)]),
    r!("c", Ctx::Any, [("ц", 0.6), ("к", 0.7), ("с", 1.0)]),
    r!("h", Ctx::Any, [("х", 0.0), ("г", 1.6)]),
    r!("y", Ctx::AfterVowel, [("й", 0.1), ("ы", 1.2)]),
    r!("y", Ctx::EndAfterConsonant, [("ий", 0.5), ("ы", 0.5), ("и", 1.2)]),
    r!("y", Ctx::Start, [("й", 0.8), ("и", 1.0)]),
    r!("y", Ctx::AfterConsonant, [("ы", 0.3), ("и", 0.9), ("ь", 1.4)]),
    r!("i", Ctx::AfterVowel, [("и", 0.2), ("й", 0.6)]),
    r!("i", Ctx::Any, [("и", 0.0), ("ы", 1.3)]),
    r!("e", Ctx::Start, [("э", 0.6), ("е", 0.2)]),
    r!("e", Ctx::Any, [("е", 0.0), ("э", 1.2)]),
    r!("'", Ctx::Any, [("ь", 0.0), ("ъ", 1.0)]),
    r!("j", Ctx::Any, [("й", 0.5), ("дж", 1.2)]),
    r!("a", Ctx::Any, [("а", 0.0)]),
    r!("b", Ctx::Any, [("б", 0.0)]),
    r!("v", Ctx::Any, [("в", 0.0)]),
    r!("g", Ctx::Any, [("г", 0.0)]),
    r!("d", Ctx::Any, [("д", 0.0)]),
    r!("z", Ctx::Any, [("з", 0.0)]),
    r!("k", Ctx::Any, [("к", 0.0)]),
    r!("l", Ctx::Any, [("л", 0.0)]),
    r!("m", Ctx::Any, [("м", 0.0)]),
    r!("n", Ctx::Any, [("н", 0.0)]),
    r!("o", Ctx::Any, [("о", 0.0)]),
    r!("p", Ctx::Any, [("п", 0.0)]),
    r!("r", Ctx::Any, [("р", 0.0)]),
    r!("s", Ctx::Any, [("с", 0.0)]),
    r!("t", Ctx::Any, [("т", 0.0)]),
    r!("u", Ctx::Any, [("у", 0.0)]),
    r!("f", Ctx::Any, [("ф", 0.0)]),
];

fn is_lat_vowel(c: u8) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u' | b'y')
}

const BEAM: usize = 48;
const LM_WEIGHT: f32 = 0.9;

/// Decode a single Latin token into ranked Cyrillic candidates.
pub fn reverse_word(token: &str) -> Vec<WordCandidate> {
    let lower = token.to_ascii_lowercase();
    if lower.is_empty() || !lower.bytes().all(|b| b.is_ascii_alphabetic() || b == b'\'') {
        return vec![];
    }
    let lex = lexicon();
    let mut out: Vec<WordCandidate> = Vec::new();
    for (word, class) in lex.lookup_skeleton(&lower) {
        out.push(WordCandidate { text: word.clone(), class: Some(*class), confidence: 0.95, from_lexicon: true });
    }
    if !out.is_empty() {
        return out;
    }

    let bytes = lower.as_bytes();
    // (position, output, cost)
    let mut beam: Vec<(usize, String, f32)> = vec![(0, String::new(), 0.0)];
    let mut finished: Vec<(String, f32)> = Vec::new();
    while !beam.is_empty() {
        let mut next: Vec<(usize, String, f32)> = Vec::with_capacity(beam.len() * 3);
        for (pos, text, cost) in beam {
            if pos >= bytes.len() {
                finished.push((text, cost));
                continue;
            }
            let prev = if pos == 0 { None } else { Some(bytes[pos - 1]) };
            let mut matched_any = false;
            for rule in RULES {
                let lb = rule.lat.as_bytes();
                if !bytes[pos..].starts_with(lb) {
                    continue;
                }
                let end = pos + lb.len();
                let at_end = end == bytes.len();
                let ok = match rule.ctx {
                    Ctx::Any => true,
                    Ctx::End => at_end,
                    Ctx::Start => pos == 0,
                    Ctx::AfterVowel => prev.is_some_and(is_lat_vowel),
                    Ctx::AfterConsonant => prev.is_some_and(|p| p.is_ascii_alphabetic() && !is_lat_vowel(p)),
                    Ctx::EndAfterConsonant => {
                        at_end && prev.is_some_and(|p| p.is_ascii_alphabetic() && !is_lat_vowel(p))
                    }
                };
                if !ok {
                    continue;
                }
                matched_any = true;
                for (o, c) in rule.outs {
                    let mut t = text.clone();
                    t.push_str(o);
                    next.push((end, t, cost + c));
                }
            }
            if !matched_any {
                // Unknown character: skip it with a penalty.
                next.push((pos + 1, text, cost + 2.0));
            }
        }
        // Score with the LM on the partial output and keep the best states.
        next.sort_by(|a, b| {
            let sa = a.2 - LM_WEIGHT * lex.log_prob_partial(&a.1);
            let sb = b.2 - LM_WEIGHT * lex.log_prob_partial(&b.1);
            sa.total_cmp(&sb)
        });
        next.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
        next.truncate(BEAM);
        beam = next;
    }

    // Optional final soft sign after л/т/н/с/з/д (Latin rarely marks it).
    let mut extra = Vec::new();
    for (t, c) in &finished {
        if t.ends_with(['л', 'т', 'н', 'с', 'з', 'д']) {
            extra.push((format!("{t}ь"), c + 1.2));
        }
    }
    finished.extend(extra);

    let mut scored: Vec<(String, f32)> = finished
        .into_iter()
        .map(|(t, c)| {
            let lp = lex.log_prob(&t);
            (t, c - LM_WEIGHT * lp)
        })
        .collect();
    scored.sort_by(|a, b| a.1.total_cmp(&b.1));
    scored.dedup_by(|a, b| a.0 == b.0);
    for (i, (text, score)) in scored.into_iter().take(5).enumerate() {
        let chars = text.chars().count().max(1) as f32;
        let per_char = score / chars;
        // Map average per-character cost (≈1.5 good … ≈4 bad) into a confidence.
        let mut conf = (1.0 - (per_char - 1.2) / 3.5).clamp(0.15, 0.8);
        if i > 0 {
            conf *= 0.8;
        }
        if has_russian_suffix(&lower) {
            conf = (conf + 0.15).min(0.85);
        }
        out.push(WordCandidate { text, class: None, confidence: conf, from_lexicon: false });
    }
    out
}

impl Lexicon {
    fn log_prob_partial(&self, word: &str) -> f32 {
        const ALPHA: f32 = 0.2;
        const VOCAB: f32 = 40.0;
        let mut lp = 0.0;
        let mut prev = '^';
        for c in word.chars() {
            let c = if c == 'ё' { 'е' } else { c };
            let bi = *self.bigram.get(&(prev, c)).unwrap_or(&0) as f32;
            let uni = *self.unigram.get(&prev).unwrap_or(&0) as f32;
            lp += ((bi + ALPHA) / (uni + ALPHA * VOCAB)).ln();
            prev = c;
        }
        lp
    }
}

const RU_SUFFIXES: &[&str] = &[
    "skiy", "skii", "skij", "sky", "skaya", "tskiy", "tsky", "ovich", "evich", "ovna", "evna", "ichna", "enko", "ov",
    "ev", "ova", "eva", "yov", "yev", "ina", "in", "yn", "uk", "yuk", "ykh", "ikh", "iy", "yy", "aya", "ogo", "shvili",
    "dze", "ich",
];

pub fn has_russian_suffix(lower_latin: &str) -> bool {
    lower_latin.len() > 3 && RU_SUFFIXES.iter().any(|s| lower_latin.ends_with(s))
}

const EN_STOPWORDS: &[&str] = &[
    "the", "of", "and", "an", "to", "for", "with", "from", "by", "my", "your", "his", "her", "is", "are", "was", "this",
    "that", "it", "at", "as", "or", "not", "all", "book", "volume", "chapter", "edition", "story", "stories", "tales",
    "house", "man", "woman", "love", "night", "dark", "world", "life", "who", "what", "when", "how", "s",
];

/// Estimate (0..1) whether a sequence of Latin tokens is transliterated Russian.
pub fn transliteration_likelihood(tokens: &[String]) -> f32 {
    let words: Vec<&str> = tokens
        .iter()
        .map(String::as_str)
        .filter(|t| t.chars().any(|c| c.is_ascii_alphabetic()))
        .collect();
    if words.is_empty() {
        return 0.0;
    }
    if words.iter().any(|w| crate::normalize::has_cyrillic(w)) {
        return 0.0;
    }
    let lex = lexicon();
    let mut score = 0.0f32;
    for w in &words {
        let lw = w.to_ascii_lowercase();
        if EN_STOPWORDS.contains(&lw.as_str()) {
            score -= 1.5;
            continue;
        }
        if ["th", "ph", "wh", "ee", "oo", "ck", "ou", "ea", "w", "q", "x"].iter().any(|p| lw.contains(p)) {
            score -= 0.8;
            continue;
        }
        if !lex.lookup_skeleton(&lw).is_empty() && lw.len() > 2 {
            score += 1.0;
        } else if has_russian_suffix(&lw) {
            score += 0.8;
        } else if ["zh", "kh", "shch", "ya", "yu", "yo", "iy", "ts"].iter().any(|d| lw.contains(d)) {
            score += 0.4;
        }
    }
    (score / words.len() as f32).clamp(0.0, 1.0)
}

/// A reconstructed Cyrillic phrase.
#[derive(Debug, Clone, PartialEq)]
pub struct Reconstruction {
    pub words: Vec<WordCandidate>,
    pub confidence: f32,
}

impl Reconstruction {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// Reverse-transliterate a whole phrase token by token. Digits and
/// non-alphabetic tokens pass through unchanged.
pub fn reverse_phrase(tokens: &[String]) -> Reconstruction {
    let mut words = Vec::with_capacity(tokens.len());
    let mut conf_sum = 0.0;
    let mut n = 0.0;
    for t in tokens {
        if t.chars().all(|c| c.is_ascii_alphabetic() || c == '\'') && !t.is_empty() {
            let best = reverse_word(t).into_iter().next();
            match best {
                Some(c) => {
                    conf_sum += c.confidence;
                    n += 1.0;
                    words.push(c);
                }
                None => words.push(WordCandidate {
                    text: t.clone(),
                    class: None,
                    confidence: 0.0,
                    from_lexicon: false,
                }),
            }
        } else {
            words.push(WordCandidate { text: t.clone(), class: None, confidence: 1.0, from_lexicon: false });
        }
    }
    let confidence = if n > 0.0 { conf_sum / n } else { 0.0 };
    Reconstruction { words, confidence }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn best(t: &str) -> String {
        reverse_word(t).into_iter().next().map(|c| c.text).unwrap_or_default()
    }

    #[test]
    fn lexicon_recovery_handles_conventions() {
        assert_eq!(best("dmitriy"), "дмитрий");
        assert_eq!(best("dmitry"), "дмитрий");
        assert_eq!(best("fyodor"), "фёдор");
        assert_eq!(best("fedor"), "фёдор");
        assert_eq!(best("dostoevskiy"), "достоевский");
        assert_eq!(best("dostoyevsky"), "достоевский");
        assert_eq!(best("gluhovskiy"), "глуховский");
        assert_eq!(best("glukhovsky"), "глуховский");
        assert_eq!(best("olga"), "ольга");
        assert_eq!(best("ilya"), "илья");
        assert_eq!(best("ilja"), "илья");
        assert_eq!(best("yuriy"), "юрий");
        assert_eq!(best("jurij"), "юрий");
        assert_eq!(best("metro"), "метро");
        assert_eq!(best("gogol"), "гоголь");
        assert_eq!(best("shchedrin"), "щедрин");
        assert_eq!(best("tsvetaeva"), "цветаева");
    }

    #[test]
    fn beam_search_decodes_unknown_words() {
        // Not in lexicon: decoded by rules + bigram LM.
        assert_eq!(best("zhilinskiy"), "жилинский");
        assert_eq!(best("kharitonchik"), "харитончик");
        assert_eq!(best("shchepkin"), "щепкин");
        assert_eq!(best("yakushev"), "якушев");
        let c = reverse_word("zhilinskiy");
        assert!(!c[0].from_lexicon);
        assert!(c[0].confidence > 0.4, "{c:?}");
    }

    #[test]
    fn likelihood_distinguishes_english() {
        let ru: Vec<String> = ["gluhovskiy", "dmitriy", "metro", "2033"].iter().map(|s| s.to_string()).collect();
        let en: Vec<String> = ["stephen", "king", "the", "shining"].iter().map(|s| s.to_string()).collect();
        assert!(transliteration_likelihood(&ru) > 0.6);
        assert!(transliteration_likelihood(&en) < 0.3);
    }
}
