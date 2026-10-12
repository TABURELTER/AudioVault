//! AudioVault core domain: entities, normalization, transliteration, identity
//! matching, metadata resolution, duplicate scoring, quality heuristics and
//! path safety. This crate performs no I/O and has no UI dependencies.

pub mod duplicate;
pub mod format;
pub mod identity;
pub mod infer;
pub mod metadata;
pub mod model;
pub mod names;
pub mod normalize;
pub mod optimize;
pub mod paths;
pub mod quality;
pub mod translit;
