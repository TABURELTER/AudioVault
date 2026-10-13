//! Person-name parsing and identity keys (authors, narrators).
//!
//! Handles both orders ("Дмитрий Глуховский" / "Глуховский Дмитрий"),
//! initials ("Достоевский Ф.М."), patronymics, comma form ("Surname, Given")
//! and transliterated Latin forms.

use crate::normalize::{capitalize_first, search_norm, skeleton_token};
use crate::translit::{has_russian_suffix, lexicon};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PersonName {
    pub given: Option<String>,
    pub patronymic: Option<String>,
    pub surname: Option<String>,
    /// Initials when only abbreviated given/patronymic are known.
    pub initials: Vec<char>,
    pub original: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum TokKind {
    Initials,
    Patronymic,
    Given,
    Surname,
    Unknown,
}

const CYR_SURNAME_SUFFIXES: &[&str] = &[
    "ский",
    "цкий",
    "ская",
    "цкая",
    "ов",
    "ев",
    "ёв",
    "ова",
    "ева",
    "ёва",
    "ин",
    "ина",
    "ын",
    "ына",
    "енко",
    "ук",
    "юк",
    "ых",
    "их",
    "ой",
    "ая",
    "швили",
    "дзе",
    "ян",
    "ич",
];

fn is_patronymic(lower: &str) -> bool {
    const P: &[&str] = &[
        "ович",
        "евич",
        "ьич",
        "овна",
        "евна",
        "ична",
        "инична",
        "ovich",
        "evich",
        "ovna",
        "evna",
        "ichna",
    ];
    lower.chars().count() > 5 && P.iter().any(|p| lower.ends_with(p))
}

fn has_surname_suffix(lower: &str) -> bool {
    if crate::normalize::has_cyrillic(lower) {
        lower.chars().count() > 3 && CYR_SURNAME_SUFFIXES.iter().any(|s| lower.ends_with(s))
    } else {
        has_russian_suffix(lower)
    }
}

fn classify(tok: &str) -> TokKind {
    let stripped: String = tok.chars().filter(|c| *c != '.').collect();
    if stripped.chars().count() <= 2
        && stripped.chars().all(char::is_alphabetic)
        && (tok.contains('.') || stripped.chars().count() == 1)
    {
        return TokKind::Initials;
    }
    let lower = search_norm(tok);
    if is_patronymic(&lower) {
        return TokKind::Patronymic;
    }
    let lex = lexicon();
    let given = lex.is_given_name(&lower);
    let surname = lex.is_known_surname(&lower);
    match (given, surname) {
        (true, false) => TokKind::Given,
        (false, true) => TokKind::Surname,
        (true, true) => {
            if has_surname_suffix(&lower) {
                TokKind::Surname
            } else {
                TokKind::Given
            }
        }
        (false, false) => {
            if has_surname_suffix(&lower) {
                TokKind::Surname
            } else {
                TokKind::Unknown
            }
        }
    }
}

fn split_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in s.split(|c: char| c.is_whitespace() || c == '_') {
        if raw.is_empty() {
            continue;
        }
        // "Ф.М." → initials token; "Ф.Достоевский" → "Ф." + "Достоевский"
        if raw.contains('.') {
            let parts: Vec<&str> = raw.split('.').filter(|p| !p.is_empty()).collect();
            let all_short = parts.iter().all(|p| p.chars().count() == 1);
            if all_short {
                out.push(raw.to_string());
                continue;
            }
            for p in parts {
                if p.chars().count() == 1 {
                    out.push(format!("{p}."));
                } else {
                    out.push(p.to_string());
                }
            }
            continue;
        }
        out.push(raw.to_string());
    }
    out
}

impl PersonName {
    pub fn parse(input: &str) -> PersonName {
        let original = input.trim().to_string();
        let mut name = PersonName {
            original: original.clone(),
            ..Default::default()
        };
        if original.is_empty() {
            return name;
        }
        // Comma form: "Достоевский, Фёдор Михайлович"
        if let Some((left, right)) = original.split_once(',') {
            let l = left.trim();
            let r = right.trim();
            if !l.is_empty() && !r.is_empty() && !l.contains(' ') {
                name.surname = Some(tidy_case(l));
                name.assign_given_part(&split_tokens(r));
                return name;
            }
        }
        let toks = split_tokens(&original);
        let kinds: Vec<TokKind> = toks.iter().map(|t| classify(t)).collect();
        match toks.len() {
            0 => {}
            1 => name.surname = Some(tidy_case(&toks[0])),
            _ => {
                let mut surname_idx: Option<usize> = None;
                if let Some(i) = kinds.iter().position(|k| *k == TokKind::Surname) {
                    surname_idx = Some(i);
                } else if kinds
                    .iter()
                    .any(|k| *k == TokKind::Given || *k == TokKind::Initials)
                {
                    // Surname is the first unknown token that isn't given/initials/patronymic.
                    surname_idx = kinds.iter().position(|k| *k == TokKind::Unknown);
                }
                let surname_idx = surname_idx.unwrap_or_else(|| {
                    // Fallback: "Given Patronymic Surname" / "Given Surname".
                    if toks.len() == 3 && kinds[2] == TokKind::Patronymic {
                        0
                    } else {
                        toks.len() - 1
                    }
                });
                name.surname = Some(tidy_case(&toks[surname_idx]));
                let rest: Vec<String> = toks
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != surname_idx)
                    .map(|(_, t)| t.clone())
                    .collect();
                name.assign_given_part(&rest);
            }
        }
        name
    }

    fn assign_given_part(&mut self, toks: &[String]) {
        for t in toks {
            match classify(t) {
                TokKind::Initials => {
                    for c in t.chars().filter(|c| c.is_alphabetic()) {
                        self.initials.extend(c.to_uppercase());
                    }
                }
                TokKind::Patronymic if self.given.is_some() || !self.initials.is_empty() => {
                    self.patronymic = Some(tidy_case(t));
                }
                _ => {
                    if self.given.is_none() {
                        self.given = Some(tidy_case(t));
                    } else if self.patronymic.is_none() {
                        self.patronymic = Some(tidy_case(t));
                    }
                }
            }
        }
    }

    /// First initial of the given name, from the full given name or initials.
    pub fn first_initial(&self) -> Option<char> {
        self.given
            .as_ref()
            .and_then(|g| g.chars().next())
            .or_else(|| self.initials.first().copied())
    }

    /// Canonical display: "Фёдор Достоевский", "Ф. М. Достоевский", "Тэффи".
    pub fn display(&self) -> String {
        let surname = self.surname.clone().unwrap_or_default();
        if let Some(g) = &self.given {
            return format!("{g} {surname}").trim().to_string();
        }
        if !self.initials.is_empty() {
            let ini: Vec<String> = self.initials.iter().map(|c| format!("{c}.")).collect();
            return format!("{} {surname}", ini.join(" ")).trim().to_string();
        }
        surname
    }

    /// "Достоевский Фёдор" style sort name.
    pub fn sort_name(&self) -> String {
        let mut s = self.surname.clone().unwrap_or_default();
        if let Some(g) = &self.given {
            s.push(' ');
            s.push_str(g);
        } else if let Some(i) = self.initials.first() {
            s.push(' ');
            s.push(*i);
        }
        s
    }

    /// Identity key: `skeleton(surname)|first-initial-skeleton`.
    pub fn key(&self) -> String {
        let surname = self
            .surname
            .as_deref()
            .map(|s| skeleton_token(&search_norm(s).replace(' ', "")))
            .unwrap_or_default();
        let initial = self
            .first_initial()
            .map(|c| {
                skeleton_token(&search_norm(&c.to_string()))
                    .chars()
                    .next()
                    .unwrap_or(c)
            })
            .map(|c| c.to_string())
            .unwrap_or_default();
        format!("{surname}|{initial}")
    }

    /// Surname-only key, for blocking.
    pub fn surname_key(&self) -> String {
        self.key().split('|').next().unwrap_or_default().to_string()
    }

    /// How "complete" this form is; used to choose the canonical display.
    pub fn completeness(&self) -> u8 {
        let mut s = 0;
        if self.surname.is_some() {
            s += 2;
        }
        if self.given.is_some() {
            s += 3;
        } else if !self.initials.is_empty() {
            s += 1;
        }
        if crate::normalize::has_cyrillic(&self.original) {
            s += 1;
        }
        s
    }
}

fn tidy_case(t: &str) -> String {
    let t = t.trim_matches(|c: char| c == ',' || c == ';');
    let all_lower = t.chars().all(|c| !c.is_uppercase());
    let all_upper =
        t.chars().filter(|c| c.is_alphabetic()).count() > 1 && t.chars().all(|c| !c.is_lowercase());
    if all_lower || all_upper {
        // Preserve hyphenated double surnames: "мамин-сибиряк" → "Мамин-Сибиряк"
        t.to_lowercase()
            .split('-')
            .map(capitalize_first)
            .collect::<Vec<_>>()
            .join("-")
    } else {
        t.to_string()
    }
}

/// Whether two parsed names plausibly refer to the same person.
pub fn same_person(a: &PersonName, b: &PersonName) -> bool {
    let (ka, kb) = (a.key(), b.key());
    let (sa, ia) = ka.split_once('|').unwrap_or((&ka, ""));
    let (sb, ib) = kb.split_once('|').unwrap_or((&kb, ""));
    if sa.is_empty() || sa != sb {
        // Allow swapped order when surname/given were misclassified.
        let swap_a = a.given.as_deref().map(|g| skeleton_token(&search_norm(g)));
        let swap_b = b
            .surname
            .as_deref()
            .map(|s| skeleton_token(&search_norm(s)));
        return swap_a.is_some()
            && swap_a == swap_b
            && a.surname
                .as_deref()
                .map(|s| skeleton_token(&search_norm(s)))
                == b.given.as_deref().map(|g| skeleton_token(&search_norm(g)));
    }
    ia.is_empty() || ib.is_empty() || ia == ib
}

/// Split a multi-person credit string into individual names.
pub fn split_people(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let normalized = s
        .replace(" и ", ";")
        .replace(" & ", ";")
        .replace(" and ", ";")
        .replace(" / ", ";")
        .replace('/', ";");
    for part in normalized.split(';') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        // "A, B" with both multi-word → two people; "Surname, Given" → one.
        let commas: Vec<&str> = p
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .collect();
        if commas.len() > 1 && commas.iter().all(|c| c.contains(' ')) {
            out.extend(commas.iter().map(|c| c.to_string()));
        } else {
            out.push(p.to_string());
        }
    }
    out
}

/// True when every token is a given name, surname, patronymic or initials.
pub fn all_tokens_namelike(s: &str) -> bool {
    let toks = split_tokens(s);
    !toks.is_empty() && toks.iter().all(|t| classify(t) != TokKind::Unknown)
}

/// Confidence (0..1) that a string is a person name rather than a title.
pub fn person_likelihood(s: &str) -> f32 {
    let toks = split_tokens(s);
    if toks.is_empty() || toks.len() > 4 {
        return 0.0;
    }
    if toks.iter().any(|t| t.chars().any(|c| c.is_ascii_digit())) {
        return 0.0;
    }
    let kinds: Vec<TokKind> = toks.iter().map(|t| classify(t)).collect();
    let mut score: f32 = 0.0;
    for k in &kinds {
        score += match k {
            TokKind::Given => 0.5,
            TokKind::Surname => 0.4,
            TokKind::Initials => 0.35,
            TokKind::Patronymic => 0.4,
            TokKind::Unknown => -0.1,
        };
    }
    if toks.len() == 1 {
        score *= 0.6;
    }
    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_orders_and_initials() {
        let a = PersonName::parse("Фёдор Достоевский");
        assert_eq!(a.surname.as_deref(), Some("Достоевский"));
        assert_eq!(a.given.as_deref(), Some("Фёдор"));
        let b = PersonName::parse("Достоевский Ф.М.");
        assert_eq!(b.surname.as_deref(), Some("Достоевский"));
        assert_eq!(b.initials, vec!['Ф', 'М']);
        let c = PersonName::parse("Глуховский Дмитрий");
        assert_eq!(c.display(), "Дмитрий Глуховский");
        let d = PersonName::parse("Достоевский Фёдор Михайлович");
        assert_eq!(d.given.as_deref(), Some("Фёдор"));
        assert_eq!(d.patronymic.as_deref(), Some("Михайлович"));
        let e = PersonName::parse("Достоевский, Фёдор");
        assert_eq!(e.display(), "Фёдор Достоевский");
        let f = PersonName::parse("Stephen King");
        assert_eq!(f.surname.as_deref(), Some("King"));
    }

    #[test]
    fn identity_keys_unify_variants() {
        let forms = [
            "Фёдор Достоевский",
            "Федор Достоевский",
            "Fedor Dostoevskiy",
            "Fyodor Dostoevsky",
            "Dostoevskiy_Fedor",
            "Достоевский Ф.М.",
            "ДОСТОЕВСКИЙ ФЁДОР",
        ];
        let base = PersonName::parse(forms[0]);
        for f in forms {
            let p = PersonName::parse(f);
            assert!(
                same_person(&base, &p),
                "{f}: {:?} vs {:?}",
                p.key(),
                base.key()
            );
        }
        assert!(!same_person(&base, &PersonName::parse("Лев Толстой")));
    }

    #[test]
    fn splits_people() {
        assert_eq!(
            split_people("Аркадий Стругацкий, Борис Стругацкий"),
            vec!["Аркадий Стругацкий", "Борис Стругацкий"]
        );
        assert_eq!(
            split_people("Ильф И. и Петров Е."),
            vec!["Ильф И.", "Петров Е."]
        );
        assert_eq!(
            split_people("Достоевский, Фёдор"),
            vec!["Достоевский, Фёдор"]
        );
    }

    #[test]
    fn likelihood() {
        assert!(person_likelihood("Дмитрий Глуховский") > 0.6);
        assert!(person_likelihood("Метро 2033") < 0.2);
    }
}
