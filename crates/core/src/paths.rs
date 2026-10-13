//! Path safety: sanitizing untrusted names (tags, torrent metadata, FB2) and
//! confining paths to a destination root (no traversal, no absolute paths).

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum PathError {
    #[error("path is absolute or has a drive/root prefix: {0}")]
    Absolute(String),
    #[error("path contains parent-directory traversal: {0}")]
    Traversal(String),
    #[error("path is empty")]
    Empty,
    #[error("path escapes the destination root: {0}")]
    Escapes(String),
}

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Maximum bytes per path component (NTFS/APFS limit is 255 UTF-16/UTF-8 units).
const MAX_COMPONENT_BYTES: usize = 200;

/// Make a single path component safe on Windows and macOS. Never returns
/// an empty string, `.` or `..`.
pub fn sanitize_component(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out.trim().trim_end_matches(['.', ' ']).to_string();
    if out.is_empty() || out == "." || out == ".." || out.chars().all(|c| c == '.') {
        out = "_".into();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if WINDOWS_RESERVED.contains(&stem.as_str()) {
        out = format!("_{out}");
    }
    if out.len() > MAX_COMPONENT_BYTES {
        // Keep the extension when truncating.
        let (base, ext) = match out.rfind('.') {
            Some(i) if out.len() - i <= 10 => (out[..i].to_string(), out[i..].to_string()),
            _ => (out.clone(), String::new()),
        };
        let mut cut = MAX_COMPONENT_BYTES.saturating_sub(ext.len());
        while cut > 0 && !base.is_char_boundary(cut) {
            cut -= 1;
        }
        out = format!("{}{}", base[..cut].trim_end(), ext);
    }
    out
}

/// Validate an untrusted relative path (e.g. from a .torrent) and return it
/// as a sanitized `PathBuf`. Rejects absolute paths, roots, drive prefixes and `..`.
pub fn validate_relative(rel: &str) -> Result<PathBuf, PathError> {
    if rel.trim().is_empty() {
        return Err(PathError::Empty);
    }
    let unified = rel.replace('\\', "/");
    if unified.starts_with('/') || unified.contains(':') && looks_like_drive(&unified) {
        return Err(PathError::Absolute(rel.into()));
    }
    let mut out = PathBuf::new();
    for part in unified.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err(PathError::Traversal(rel.into())),
            p => out.push(sanitize_component(p)),
        }
    }
    if out.as_os_str().is_empty() {
        return Err(PathError::Empty);
    }
    // Double-check with std component parsing.
    for c in out.components() {
        match c {
            Component::Normal(_) => {}
            Component::ParentDir => return Err(PathError::Traversal(rel.into())),
            _ => return Err(PathError::Absolute(rel.into())),
        }
    }
    Ok(out)
}

fn looks_like_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

/// Join an untrusted relative path under `root`, guaranteeing the result stays inside.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, PathError> {
    let rel = validate_relative(rel)?;
    let joined = root.join(&rel);
    if !joined.starts_with(root) {
        return Err(PathError::Escapes(joined.display().to_string()));
    }
    Ok(joined)
}

/// Convert a path relative to a volume/root into the DB representation
/// (forward slashes, no leading slash).
pub fn to_db_rel(rel: &Path) -> String {
    let mut parts = Vec::new();
    for c in rel.components() {
        if let Component::Normal(p) = c {
            parts.push(p.to_string_lossy().into_owned());
        }
    }
    parts.join("/")
}

/// Rebuild an OS path from a mount root and a DB relative path.
pub fn from_db_rel(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty()) {
        p.push(part);
    }
    p
}

/// Render a path template like `{author}/{series}/{index} {title}` into a safe
/// relative path. Empty placeholders collapse; components are sanitized.
pub fn render_template(template: &str, values: &[(&str, &str)]) -> Result<PathBuf, PathError> {
    let mut rendered = template.to_string();
    for (k, v) in values {
        rendered = rendered.replace(&format!("{{{k}}}"), &v.replace(['/', '\\'], "_"));
    }
    // Remove any unknown placeholders.
    while let (Some(s), Some(e)) = (rendered.find('{'), rendered.find('}')) {
        if e > s {
            rendered.replace_range(s..=e, "");
        } else {
            break;
        }
    }
    let cleaned: Vec<String> = rendered
        .split('/')
        .map(|c| crate::normalize::tidy(c))
        .filter(|c| !c.is_empty())
        .map(|c| sanitize_component(&c))
        .collect();
    if cleaned.is_empty() {
        return Err(PathError::Empty);
    }
    validate_relative(&cleaned.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_components() {
        assert_eq!(
            sanitize_component("a/b\\c:d*e?f\"g<h>i|j"),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(sanitize_component("  name.  "), "name");
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("nul.txt"), "_nul.txt");
        assert_eq!(sanitize_component("Метро 2033"), "Метро 2033");
        let long = "я".repeat(300) + ".mp3";
        let s = sanitize_component(&long);
        assert!(s.len() <= 200 && s.ends_with(".mp3"));
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        assert!(matches!(
            validate_relative("../evil"),
            Err(PathError::Traversal(_))
        ));
        assert!(matches!(
            validate_relative("a/../../evil"),
            Err(PathError::Traversal(_))
        ));
        assert!(matches!(
            validate_relative("/etc/passwd"),
            Err(PathError::Absolute(_))
        ));
        assert!(matches!(
            validate_relative("C:\\Windows\\x"),
            Err(PathError::Absolute(_))
        ));
        assert!(matches!(
            validate_relative("\\\\server\\share"),
            Err(PathError::Absolute(_))
        ));
        assert!(matches!(validate_relative(""), Err(PathError::Empty)));
        assert_eq!(
            validate_relative("Book/CD1/01.mp3").unwrap(),
            PathBuf::from("Book").join("CD1").join("01.mp3")
        );
        assert_eq!(
            validate_relative("./a/./b").unwrap(),
            PathBuf::from("a").join("b")
        );
    }

    #[test]
    fn safe_join_stays_inside() {
        let root = Path::new("/dest");
        assert!(safe_join(root, "x/y.mp3").unwrap().starts_with(root));
        assert!(safe_join(root, "../y").is_err());
    }

    #[test]
    fn db_rel_roundtrip() {
        let rel = Path::new("Автор").join("Книга").join("01.mp3");
        let s = to_db_rel(&rel);
        assert_eq!(s, "Автор/Книга/01.mp3");
        assert_eq!(
            from_db_rel(Path::new("root"), &s),
            Path::new("root").join(&rel)
        );
    }

    #[test]
    fn templates() {
        let p = render_template(
            "{author}/{series}/{title}",
            &[
                ("author", "Глуховский"),
                ("series", ""),
                ("title", "Метро: 2033"),
            ],
        )
        .unwrap();
        assert_eq!(p, PathBuf::from("Глуховский").join("Метро_ 2033"));
        let p = render_template("{title}", &[("title", "../..")]).unwrap();
        assert!(
            p.components().all(|c| matches!(c, Component::Normal(_))),
            "{p:?}"
        );
        assert_eq!(p.components().count(), 1);
    }
}
