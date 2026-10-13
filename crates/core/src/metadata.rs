//! Metadata observations and canonical-value resolution.
//!
//! Observations from different sources (tags, FB2, folder names, manual edits,
//! providers) are collected and never overwritten. Canonical values are
//! *derived* by [`resolve`]. Manual edits always win.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::{MetadataField, MetadataSource};
use crate::normalize::{is_junk_value, skeleton};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub field: MetadataField,
    pub value: String,
    pub source: MetadataSource,
    /// 0..1, how much the source itself trusts the value.
    pub confidence: f32,
}

impl Observation {
    pub fn new(
        field: MetadataField,
        value: impl Into<String>,
        source: MetadataSource,
        confidence: f32,
    ) -> Self {
        Observation {
            field,
            value: value.into(),
            source,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedValue {
    pub value: String,
    pub source: MetadataSource,
    pub score: f32,
    /// Distinct sources that agree with the chosen value.
    pub agreeing_sources: Vec<MetadataSource>,
    /// Competing values from other sources that disagree.
    pub conflicts: Vec<(MetadataSource, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Resolved {
    pub fields: HashMap<MetadataField, ResolvedValue>,
}

impl Resolved {
    pub fn get(&self, f: MetadataField) -> Option<&str> {
        self.fields.get(&f).map(|v| v.value.as_str())
    }
    pub fn source(&self, f: MetadataField) -> Option<MetadataSource> {
        self.fields.get(&f).map(|v| v.source)
    }
    /// Overall confidence 0..1 for the identity-relevant fields.
    pub fn confidence(&self) -> f32 {
        let t = self
            .fields
            .get(&MetadataField::Title)
            .map_or(0.0, |v| v.score / 100.0);
        let a = self
            .fields
            .get(&MetadataField::Author)
            .map_or(0.0, |v| v.score / 100.0);
        ((t + a) / 2.0).clamp(0.0, 1.0)
    }
}

fn compare_key(field: MetadataField, v: &str) -> String {
    match field {
        MetadataField::Author | MetadataField::Narrator => {
            // Compare people by identity key set, order-independent.
            let mut keys: Vec<String> = crate::names::split_people(v)
                .iter()
                .map(|p| crate::names::PersonName::parse(p).key())
                .collect();
            keys.sort();
            keys.join(";")
        }
        MetadataField::Title | MetadataField::Series => crate::normalize::title_key(v),
        _ => skeleton(v),
    }
}

/// Resolve canonical values. Rules:
/// * junk values ("Unknown Artist", "Track 01") are ignored;
/// * `Manual` always wins;
/// * otherwise score = authority × confidence, + 8 per additional agreeing source.
pub fn resolve(observations: &[Observation]) -> Resolved {
    let mut by_field: HashMap<MetadataField, Vec<&Observation>> = HashMap::new();
    for o in observations {
        if o.value.trim().is_empty()
            || (o.field != MetadataField::Description && is_junk_value(&o.value))
        {
            continue;
        }
        by_field.entry(o.field).or_default().push(o);
    }
    let mut out = Resolved::default();
    for (field, obs) in by_field {
        // Group by comparison key.
        let mut groups: HashMap<String, Vec<&Observation>> = HashMap::new();
        for o in &obs {
            groups
                .entry(compare_key(field, &o.value))
                .or_default()
                .push(o);
        }
        let mut best: Option<(f32, &Observation, Vec<MetadataSource>, String)> = None;
        for (key, members) in &groups {
            let manual = members.iter().any(|o| o.source == MetadataSource::Manual);
            let top = members
                .iter()
                .max_by(|a, b| {
                    (a.source.authority() * a.confidence)
                        .total_cmp(&(b.source.authority() * b.confidence))
                })
                .copied();
            let Some(top) = top else { continue };
            let mut sources: Vec<MetadataSource> = members.iter().map(|o| o.source).collect();
            sources.sort_by_key(|s| s.as_str());
            sources.dedup();
            let mut score = top.source.authority() * top.confidence
                + 8.0 * (sources.len().saturating_sub(1)) as f32;
            if manual {
                score = 1000.0;
            }
            // Prefer the most complete display form within the agreeing group
            // (e.g. "Фёдор Достоевский" over "Достоевский Ф.М.").
            let display = members
                .iter()
                .filter(|o| o.source == top.source || !manual)
                .max_by_key(|o| display_quality(field, &o.value, o.source == top.source))
                .copied()
                .unwrap_or(top);
            let display = if manual {
                members
                    .iter()
                    .find(|o| o.source == MetadataSource::Manual)
                    .copied()
                    .unwrap_or(top)
            } else {
                display
            };
            if best.as_ref().is_none_or(|(s, ..)| score > *s) {
                best = Some((score, display, sources, key.clone()));
            }
        }
        if let Some((score, chosen, sources, key)) = best {
            let mut conflicts: Vec<(MetadataSource, String)> = obs
                .iter()
                .filter(|o| compare_key(field, &o.value) != key)
                .map(|o| (o.source, o.value.clone()))
                .collect();
            conflicts.dedup();
            out.fields.insert(
                field,
                ResolvedValue {
                    value: chosen.value.trim().to_string(),
                    source: if sources.contains(&MetadataSource::Manual) {
                        MetadataSource::Manual
                    } else {
                        chosen.source
                    },
                    score: score.min(100.0),
                    agreeing_sources: sources,
                    conflicts,
                },
            );
        }
    }
    out
}

fn display_quality(field: MetadataField, value: &str, is_top_source: bool) -> (u8, u8, usize) {
    let completeness = match field {
        MetadataField::Author | MetadataField::Narrator => crate::names::split_people(value)
            .iter()
            .map(|p| crate::names::PersonName::parse(p).completeness())
            .max()
            .unwrap_or(0),
        _ => {
            let has_cyr = crate::normalize::has_cyrillic(value) as u8;
            let has_upper = value.chars().any(char::is_uppercase) as u8;
            has_cyr * 2 + has_upper
        }
    };
    (completeness, is_top_source as u8, value.len().min(200))
}

#[cfg(test)]
mod tests {
    use super::*;
    use MetadataField::*;
    use MetadataSource::*;

    #[test]
    fn manual_edit_wins() {
        let obs = vec![
            Observation::new(Title, "Metro 2033", EmbeddedTag, 0.9),
            Observation::new(Title, "Метро 2033", Fb2, 0.95),
            Observation::new(Title, "Метро-2033 (моя правка)", Manual, 1.0),
        ];
        let r = resolve(&obs);
        assert_eq!(r.get(Title), Some("Метро-2033 (моя правка)"));
        assert_eq!(r.source(Title), Some(Manual));
    }

    #[test]
    fn fb2_beats_tags_and_folder_and_junk_ignored() {
        let obs = vec![
            Observation::new(Author, "Unknown Artist", EmbeddedTag, 0.9),
            Observation::new(Author, "Глуховский Д.", FolderName, 0.6),
            Observation::new(Author, "Дмитрий Глуховский", Fb2, 0.95),
            Observation::new(Title, "Track 01", EmbeddedTag, 0.9),
            Observation::new(Title, "Метро 2033", FolderName, 0.6),
        ];
        let r = resolve(&obs);
        assert_eq!(r.get(Author), Some("Дмитрий Глуховский"));
        assert_eq!(r.get(Title), Some("Метро 2033"));
        let a = &r.fields[&Author];
        assert!(
            a.agreeing_sources.contains(&FolderName),
            "Глуховский Д. agrees by identity key"
        );
    }

    #[test]
    fn good_tags_not_overwritten_by_folder() {
        let obs = vec![
            Observation::new(Title, "Пикник на обочине", EmbeddedTag, 0.9),
            Observation::new(Title, "piknik na obochine", FolderName, 0.5),
        ];
        let r = resolve(&obs);
        assert_eq!(r.get(Title), Some("Пикник на обочине"));
    }

    #[test]
    fn conflicts_are_reported() {
        let obs = vec![
            Observation::new(Narrator, "Сергей Кузнецов", EmbeddedTag, 0.8),
            Observation::new(Narrator, "Александр Клюквин", FolderName, 0.5),
        ];
        let r = resolve(&obs);
        assert_eq!(r.get(Narrator), Some("Сергей Кузнецов"));
        assert_eq!(r.fields[&Narrator].conflicts.len(), 1);
    }
}
