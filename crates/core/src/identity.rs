//! Book identity keys and search alias generation.
//!
//! Identity is independent of physical location: two copies belong to the
//! same `Book` when their author set and normalized title agree.

use crate::names::{PersonName, split_people};
use crate::normalize::{cyr_to_lat, has_cyrillic, is_latin_only, search_norm, skeleton, title_key, tokens};
use crate::translit::{reverse_phrase, transliteration_likelihood};

/// Order-independent key for a set of people.
pub fn people_key<S: AsRef<str>>(people: &[S]) -> String {
    let mut keys: Vec<String> = people
        .iter()
        .flat_map(|p| split_people(p.as_ref()))
        .map(|p| PersonName::parse(&p).key())
        .filter(|k| k != "|")
        .collect();
    keys.sort();
    keys.dedup();
    keys.join(";")
}

/// Surname-only key (more tolerant: ignores initials).
pub fn people_surname_key<S: AsRef<str>>(people: &[S]) -> String {
    let mut keys: Vec<String> = people
        .iter()
        .flat_map(|p| split_people(p.as_ref()))
        .map(|p| PersonName::parse(&p).surname_key())
        .filter(|k| !k.is_empty())
        .collect();
    keys.sort();
    keys.dedup();
    keys.join(";")
}

/// `authors#title` identity key. Uses surnames only so that
/// "Достоевский Ф.М." and "Фёдор Достоевский" collide deliberately.
pub fn book_identity_key<S: AsRef<str>>(authors: &[S], title: &str) -> String {
    format!("{}#{}", people_surname_key(authors), title_key(title))
}

/// Similarity of two titles in 0..1 (on skeleton keys).
pub fn title_similarity(a: &str, b: &str) -> f32 {
    let (ka, kb) = (title_key(a), title_key(b));
    if ka.is_empty() || kb.is_empty() {
        return 0.0;
    }
    if ka == kb {
        return 1.0;
    }
    strsim::jaro_winkler(&ka, &kb) as f32
}

/// Alternative spellings to index for search: Latin transliteration of
/// Cyrillic text and reverse transliteration of Latin text.
pub fn search_aliases(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let norm = search_norm(text);
    if norm.is_empty() {
        return out;
    }
    if has_cyrillic(&norm) {
        out.push(cyr_to_lat(&norm));
    } else if is_latin_only(&norm) {
        let toks = tokens(&norm);
        if transliteration_likelihood(&toks) > 0.3 {
            let rec = reverse_phrase(&toks);
            if rec.confidence > 0.3 {
                out.push(rec.text());
            }
        }
    }
    out.retain(|a| a != &norm && !a.is_empty());
    out
}

/// Skeleton for a whole text block (used by the FTS skeleton column).
pub fn skeleton_text(parts: &[&str]) -> String {
    parts.iter().map(|p| skeleton(p)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_keys_match_across_spellings() {
        let a = book_identity_key(&["Фёдор Достоевский"], "Братья Карамазовы");
        let b = book_identity_key(&["Достоевский Ф.М."], "Братья Карамазовы [2010, MP3]");
        let c = book_identity_key(&["Fyodor Dostoevsky"], "Bratya Karamazovy");
        assert_eq!(a, b);
        assert_eq!(a, c);
        let d = book_identity_key(&["Лев Толстой"], "Братья Карамазовы");
        assert_ne!(a, d);
    }

    #[test]
    fn people_keys_order_independent() {
        assert_eq!(
            people_key(&["Аркадий Стругацкий", "Борис Стругацкий"]),
            people_key(&["Стругацкий Борис; Стругацкий Аркадий"])
        );
    }

    #[test]
    fn aliases() {
        let a = search_aliases("Метро 2033");
        assert_eq!(a, vec!["metro 2033".to_string()]);
        let b = search_aliases("gluhovskiy dmitriy");
        assert_eq!(b, vec!["глуховский дмитрий".to_string()]);
        assert!(search_aliases("The Shining").is_empty());
    }
}
