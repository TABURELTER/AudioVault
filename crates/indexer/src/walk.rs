//! Filesystem discovery of audiobook media sets.
//!
//! * Iterative traversal with an explicit stack (no recursion limits).
//! * Symlinks, junctions and other reparse points are never followed, so
//!   cycles are impossible.
//! * A directory containing audio files is one *media set* (one physical copy
//!   of a book). Disc sub-folders (`CD1`, `Disc 2`, `Диск 3`) are merged into
//!   their parent's set. A directory holding several `.m4b` files yields one
//!   single-file set per file, and loose audio files directly in the library
//!   root are always single-file sets.
//! * Unreadable directories are reported, not fatal; if the root itself or the
//!   mount disappears the walk stops with [`IndexerError::VolumeUnavailable`].

use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use audiovault_core::infer::{is_generic_folder, is_part_or_chapter_folder};
use audiovault_core::model::FileKind;

use crate::{IndexerError, JobContext, Result, check};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Path relative to the volume root, `/`-separated.
    pub rel_path: String,
    pub file_name: String,
    pub ext: String,
    pub kind: FileKind,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSet {
    /// Folder (or single file) relative to the volume root.
    pub rel_path: String,
    pub is_single_file: bool,
    /// Audio files in playback order first, then other files.
    pub files: Vec<FileEntry>,
}

impl MediaSet {
    pub fn audio(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.iter().filter(|f| f.kind == FileKind::Audio)
    }
    /// Path components used for folder-name inference (file stem for single files).
    pub fn components(&self) -> Vec<String> {
        let mut parts: Vec<String> = self.rel_path.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect();
        if self.is_single_file
            && let Some(last) = parts.last_mut()
            && let Some((stem, _)) = last.rsplit_once('.')
        {
            *last = stem.to_string();
        }
        parts
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkStats {
    pub dirs_seen: u64,
    pub files_seen: u64,
    pub sets_found: u64,
    pub ebooks_found: u64,
    /// (relative path, message) of entries that could not be read.
    pub errors: Vec<(String, String)>,
}

const SKIP_DIRS: &[&str] = &[
    "$recycle.bin", "system volume information", "@eadir", "#recycle", "lost+found", "recycler",
    "plugins", "plugin", "wa", "__macosx"
];
const SKIP_FILES: &[&str] = &["thumbs.db", "desktop.ini"];
const PARTIAL_EXTS: &[&str] = &["part", "!qb", "bc!", "crdownload", "tmp", "avpart"];

fn skip_name(name: &str, is_dir: bool) -> bool {
    if name.starts_with('.') {
        return true; // hidden on Unix; also .audiovault-id, .Trashes, temp outputs
    }
    let lower = name.to_lowercase();
    if is_dir {
        SKIP_DIRS.contains(&lower.as_str())
    } else {
        SKIP_FILES.contains(&lower.as_str())
            || lower.rsplit_once('.').is_some_and(|(_, e)| PARTIAL_EXTS.contains(&e))
    }
}

#[cfg(windows)]
fn is_hidden_system(md: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    md.file_attributes() & (HIDDEN | SYSTEM) == (HIDDEN | SYSTEM)
}

#[cfg(not(windows))]
fn is_hidden_system(_md: &fs::Metadata) -> bool {
    false
}

/// Case-insensitive "natural" ordering: `2.mp3` < `10.mp3`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let (lx, ly) = (x.to_lowercase().next().unwrap_or(x), y.to_lowercase().next().unwrap_or(y));
                if lx != ly {
                    return lx.cmp(&ly);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

fn join_rel(parent: &str, name: &str) -> String {
    if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") }
}

struct DirListing {
    files: Vec<FileEntry>,
    dirs: Vec<String>,
}

fn list_dir(abs: &Path, rel: &str, stats: &mut WalkStats) -> std::io::Result<DirListing> {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(abs)? {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                stats.errors.push((rel.to_string(), e.to_string()));
                continue;
            }
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        // symlink_metadata: never follow links/junctions.
        let md = match entry.path().symlink_metadata() {
            Ok(m) => m,
            Err(e) => {
                stats.errors.push((join_rel(rel, &name), e.to_string()));
                continue;
            }
        };
        let ft = md.file_type();
        if ft.is_symlink() || is_hidden_system(&md) {
            continue;
        }
        if ft.is_dir() {
            if !skip_name(&name, true) {
                dirs.push(name);
            }
        } else if ft.is_file() {
            if skip_name(&name, false) {
                continue;
            }
            stats.files_seen += 1;
            let name_lower = name.to_ascii_lowercase();
            let (ext, mut kind) = if name_lower.ends_with(".fb2.zip") {
                ("fb2.zip".to_string(), FileKind::Ebook)
            } else {
                let e = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
                let k = FileKind::from_extension(&e);
                (e, k)
            };
            if ext == "zip" && kind != FileKind::Ebook && audiovault_media::fb2::is_fb2_zip(&entry.path()) {
                kind = FileKind::Ebook;
            }
            let mtime = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs() as i64);
            files.push(FileEntry {
                rel_path: join_rel(rel, &name),
                kind,
                file_name: name,
                ext,
                size: md.len(),
                mtime,
            });
        }
    }
    files.sort_by(|a, b| natural_cmp(&a.file_name, &b.file_name));
    dirs.sort_by(|a, b| natural_cmp(a, b));
    Ok(DirListing { files, dirs })
}

fn order_files(mut audio: Vec<FileEntry>, mut other: Vec<FileEntry>) -> Vec<FileEntry> {
    // Natural order across folders (e.g. CD1 < CD2, Glava 1 < Glava 2)
    audio.sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
    other.sort_by(|a, b| natural_cmp(&a.rel_path, &b.rel_path));
    audio.append(&mut other);
    audio
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

fn collect_part_subtree(
    mount: &Path,
    sub_rel: &str,
    audio: &mut Vec<FileEntry>,
    other: &mut Vec<FileEntry>,
    stats: &mut WalkStats,
) {
    let sub_abs = crate::abs_path(mount, sub_rel);
    if let Ok(sub) = list_dir(&sub_abs, sub_rel, stats) {
        stats.dirs_seen += 1;
        let (a, o): (Vec<FileEntry>, Vec<FileEntry>) =
            sub.files.into_iter().partition(|f| f.kind == FileKind::Audio);
        audio.extend(a);
        other.extend(o);
        for sd in sub.dirs {
            let nested_rel = join_rel(sub_rel, &sd);
            collect_part_subtree(mount, &nested_rel, audio, other, stats);
        }
    }
}

/// Walk `root_rel` (relative to `mount`) and report each media set.
pub fn walk(
    mount: &Path,
    root_rel: &str,
    ctx: &dyn JobContext,
    on_set: &mut dyn FnMut(MediaSet) -> Result<()>,
    on_ebook: &mut dyn FnMut(FileEntry) -> Result<()>,
) -> Result<WalkStats> {
    let root_abs: PathBuf = crate::abs_path(mount, root_rel);
    let mut stats = WalkStats::default();
    match fs::metadata(&root_abs) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(IndexerError::Invalid(format!("{} is not a folder", root_abs.display()))),
        Err(e) => return Err(IndexerError::VolumeUnavailable(format!("{}: {e}", root_abs.display()))),
    }
    let mut stack: Vec<String> = vec![root_rel.to_string()];
    while let Some(rel) = stack.pop() {
        check(ctx)?;
        let abs = crate::abs_path(mount, &rel);
        stats.dirs_seen += 1;
        if stats.dirs_seen % 200 == 0 {
            ctx.progress(-1.0, &format!("{} folders, {} books found", stats.dirs_seen, stats.sets_found));
        }
        let listing = match list_dir(&abs, &rel, &mut stats) {
            Ok(l) => l,
            Err(e) => {
                if !root_abs.exists() || !mount.exists() {
                    return Err(IndexerError::VolumeUnavailable(format!("{} disappeared during scan", root_abs.display())));
                }
                stats.errors.push((rel.clone(), e.to_string()));
                continue;
            }
        };
        let is_root = rel == root_rel;
        let (mut audio, mut other): (Vec<FileEntry>, Vec<FileEntry>) =
            listing.files.into_iter().partition(|f| f.kind == FileKind::Audio);

        // Report any e-book files discovered in this folder
        for f in &other {
            if f.kind == FileKind::Ebook {
                stats.ebooks_found += 1;
                on_ebook(f.clone())?;
            }
        }

        // Merge disc/part/chapter folders into this directory's set.
        let mut rest_dirs = Vec::new();
        let parent_is_generic = is_root || is_generic_folder(&rel) || rel.split('/').last().is_some_and(is_generic_folder);
        for d in listing.dirs {
            if is_part_or_chapter_folder(&d) && !parent_is_generic {
                let sub_rel = join_rel(&rel, &d);
                collect_part_subtree(mount, &sub_rel, &mut audio, &mut other, &mut stats);
            } else {
                rest_dirs.push(d);
            }
        }
        for d in rest_dirs.into_iter().rev() {
            stack.push(join_rel(&rel, &d));
        }
        if audio.is_empty() {
            continue;
        }

        let all_m4b = audio.len() >= 2 && audio.iter().all(|f| f.ext == "m4b");
        if is_root || all_m4b {
            for a in audio {
                let s = stem(&a.file_name).to_lowercase();
                let companions: Vec<FileEntry> = other
                    .iter()
                    .filter(|o| stem(&o.file_name).to_lowercase() == s)
                    .cloned()
                    .collect();
                stats.sets_found += 1;
                let rel_path = a.rel_path.clone();
                on_set(MediaSet { rel_path, is_single_file: true, files: order_files(vec![a], companions) })?;
            }
        } else {
            stats.sets_found += 1;
            on_set(MediaSet { rel_path: rel.clone(), is_single_file: false, files: order_files(audio, other) })?;
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NoopContext;

    fn touch(p: &Path, bytes: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![0u8; bytes]).unwrap();
    }

    fn collect(mount: &Path, root: &str) -> (Vec<MediaSet>, WalkStats) {
        let mut sets = Vec::new();
        let stats = walk(mount, root, &NoopContext, &mut |s| {
            sets.push(s);
            Ok(())
        }, &mut |_| Ok(()))
        .unwrap();
        (sets, stats)
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["10.mp3", "2.mp3", "1.mp3", "Глава 11.mp3", "глава 3.mp3"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["1.mp3", "2.mp3", "10.mp3", "глава 3.mp3", "Глава 11.mp3"]);
    }

    #[test]
    fn discovers_sets_discs_and_m4b_collections() {
        let d = tempfile::tempdir().unwrap();
        let m = d.path();
        touch(&m.join("Книги/Глуховский Дмитрий/Метро 2033/01.mp3"), 10);
        touch(&m.join("Книги/Глуховский Дмитрий/Метро 2033/02.mp3"), 10);
        touch(&m.join("Книги/Глуховский Дмитрий/Метро 2033/cover.jpg"), 10);
        touch(&m.join("Книги/Толстой/Война и мир/CD1/01.mp3"), 10);
        touch(&m.join("Книги/Толстой/Война и мир/CD2/01.mp3"), 10);
        touch(&m.join("Книги/Толстой/Война и мир/CD10/01.mp3"), 10);
        touch(&m.join("Книги/m4b/Book A.m4b"), 10);
        touch(&m.join("Книги/m4b/Book B.m4b"), 10);
        touch(&m.join("Книги/m4b/Book B.jpg"), 10);
        touch(&m.join("Книги/loose.m4b"), 10);
        touch(&m.join("Книги/.hidden/x.mp3"), 10);
        touch(&m.join("Книги/empty/readme.txt"), 10);
        touch(&m.join("Книги/partial/01.mp3.part"), 10);

        let (sets, stats) = collect(m, "Книги");
        let paths: Vec<&str> = sets.iter().map(|s| s.rel_path.as_str()).collect();
        assert!(paths.contains(&"Книги/Глуховский Дмитрий/Метро 2033"), "{paths:?}");
        assert!(paths.contains(&"Книги/Толстой/Война и мир"));
        assert!(paths.contains(&"Книги/m4b/Book A.m4b"));
        assert!(paths.contains(&"Книги/m4b/Book B.m4b"));
        assert!(paths.contains(&"Книги/loose.m4b"));
        assert_eq!(sets.len(), 5, "{paths:?}");
        assert!(stats.errors.is_empty());

        let war = sets.iter().find(|s| s.rel_path.ends_with("Война и мир")).unwrap();
        let order: Vec<&str> = war.audio().map(|f| f.rel_path.as_str()).collect();
        assert_eq!(
            order,
            vec!["Книги/Толстой/Война и мир/CD1/01.mp3", "Книги/Толстой/Война и мир/CD2/01.mp3", "Книги/Толстой/Война и мир/CD10/01.mp3"]
        );
        let b = sets.iter().find(|s| s.rel_path.ends_with("Book B.m4b")).unwrap();
        assert!(b.is_single_file);
        assert_eq!(b.files.len(), 2, "companion cover attached");
        assert_eq!(b.components().last().map(String::as_str), Some("Book B"));
        let metro = sets.iter().find(|s| s.rel_path.ends_with("Метро 2033")).unwrap();
        assert_eq!(metro.files.len(), 3);
    }

    #[test]
    fn missing_root_is_volume_unavailable() {
        let d = tempfile::tempdir().unwrap();
        let err = walk(d.path(), "nope", &NoopContext, &mut |_| Ok(()), &mut |_| Ok(())).unwrap_err();
        assert!(matches!(err, IndexerError::VolumeUnavailable(_)));
    }

    #[test]
    fn cancellation_stops_walk() {
        struct Cancelled;
        impl JobContext for Cancelled {
            fn is_cancelled(&self) -> bool {
                true
            }
            fn progress(&self, _: f64, _: &str) {}
        }
        let d = tempfile::tempdir().unwrap();
        touch(&d.path().join("a/1.mp3"), 1);
        let err = walk(d.path(), "", &Cancelled, &mut |_| Ok(()), &mut |_| Ok(())).unwrap_err();
        assert!(matches!(err, IndexerError::Cancelled));
    }
}
