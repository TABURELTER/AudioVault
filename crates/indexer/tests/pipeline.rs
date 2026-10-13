//! End-to-end indexing tests on synthetic libraries with real (generated)
//! WAV files: scan → analyze → hash → duplicates → search, incremental
//! rescans, moves, deletions, storage disappearing mid-scan and offline search.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use audiovault_core::optimize::OptimizePolicy;
use audiovault_db::volumes::{self, ObservedVolume};
use audiovault_db::{Db, catalog, duplicates as dbdup, library, search};
use audiovault_indexer::analyze::{AnalyzeEnv, analyze_copy};
use audiovault_indexer::duplicates::detect_all;
use audiovault_indexer::hashing::full_hash_copy;
use audiovault_indexer::scan::{ScanOutcome, scan_root};
use audiovault_indexer::{JobContext, NoopContext};

/// Mono 16-bit PCM WAV with a deterministic chirp + noise signal.
fn write_wav(path: &Path, seconds: f32, seed: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let rate = 8000u32;
    let n = (seconds * rate as f32) as u32;
    let mut data = Vec::with_capacity(n as usize * 2);
    let mut lcg = seed.wrapping_mul(2654435761).wrapping_add(1);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let f = 200.0 + 150.0 * (seed % 7) as f32 + 300.0 * t;
        lcg = lcg.wrapping_mul(1664525).wrapping_add(1013904223);
        let noise = ((lcg >> 16) as f32 / 65536.0 - 0.5) * 0.2;
        let v = ((t * f * std::f32::consts::TAU).sin() * 0.6 + noise) * i16::MAX as f32;
        data.extend_from_slice(&(v as i16).to_le_bytes());
    }
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    fs::write(path, out).unwrap();
}

struct Env {
    _dir: tempfile::TempDir,
    mount: PathBuf,
    covers: PathBuf,
    db: Arc<Db>,
    volume_id: i64,
    root: volumes::LibraryRoot,
}

fn setup() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("disk");
    let lib = mount.join("Аудиокниги");
    write_wav(&lib.join("Глуховский Дмитрий/Метро 2033/01.wav"), 3.0, 1);
    write_wav(&lib.join("Глуховский Дмитрий/Метро 2033/02.wav"), 3.0, 2);
    // Same audio under a transliterated folder name → exact duplicate of the same book.
    write_wav(&lib.join("gluhovskiy_dmitriy_metro_2033/01.wav"), 3.0, 1);
    write_wav(&lib.join("gluhovskiy_dmitriy_metro_2033/02.wav"), 3.0, 2);
    // Same book, two narrators → two variants.
    write_wav(&lib.join("Достоевский Фёдор - Братья Карамазовы (читает Клюквин Александр)/01.wav"), 2.0, 3);
    write_wav(&lib.join("Достоевский Фёдор - Братья Карамазовы (читает Кузнецов Сергей)/01.wav"), 2.5, 4);
    // Disc folders merge into one copy.
    write_wav(&lib.join("Толстой Лев - Война и мир/CD1/01.wav"), 1.0, 5);
    write_wav(&lib.join("Толстой Лев - Война и мир/CD2/01.wav"), 1.0, 6);
    let db = Db::open(&dir.path().join("catalog.sqlite")).unwrap();
    let (volume_id, root) = {
        let conn = db.conn().unwrap();
        let vol = volumes::insert(&conn, &ObservedVolume::synthetic("Archive", &mount.to_string_lossy()), "Archive").unwrap();
        let root_id = volumes::add_root(&conn, vol, "Аудиокниги").unwrap();
        (vol, volumes::get_root(&conn, root_id).unwrap().unwrap())
    };
    let covers = dir.path().join("covers");
    Env { mount, covers, db, volume_id, root, _dir: dir }
}

fn analyze_all(env: &Env) {
    let ids: Vec<i64> = {
        let conn = env.db.conn().unwrap();
        catalog::copies_needing_analysis(&conn, env.volume_id, 2, 1000).unwrap().into_iter().map(|c| c.id).collect()
    };
    let aenv = AnalyzeEnv { covers_dir: env.covers.clone(), policy: OptimizePolicy::default() };
    for id in ids {
        analyze_copy(&env.db, id, &env.mount, &aenv, &NoopContext).unwrap();
    }
}

fn count(db: &Db, sql: &str) -> i64 {
    db.conn().unwrap().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn full_pipeline_and_incremental_rescans() {
    let env = setup();
    let s = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    assert_eq!(s.outcome, ScanOutcome::Completed);
    assert_eq!(s.counters.copies_found, 5, "{s:?}");
    assert_eq!(s.counters.copies_new, 5);

    // Translit folder joins the Cyrillic book; narrators split variants.
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book"), 3);
    let metro = count(&env.db, "SELECT id FROM book WHERE title = 'Метро 2033'");
    assert_eq!(count(&env.db, &format!("SELECT COUNT(*) FROM book_copy WHERE book_id = {metro}")), 2);
    let karamazov = count(&env.db, "SELECT id FROM book WHERE title = 'Братья Карамазовы'");
    assert_eq!(count(&env.db, &format!("SELECT COUNT(*) FROM audio_variant WHERE book_id = {karamazov}")), 2);
    let war = count(&env.db, "SELECT COUNT(*) FROM media_file WHERE rel_path LIKE '%Война и мир/CD%'");
    assert_eq!(war, 2, "disc folders merged into one copy");

    analyze_all(&env);
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE analysis_level < 2"), 0);
    let dur = count(&env.db, "SELECT total_duration_ms FROM book_copy WHERE rel_path LIKE '%Метро 2033'");
    assert!((5900..=6100).contains(&dur), "duration {dur}");
    assert!(count(&env.db, "SELECT COUNT(*) FROM metadata_issue WHERE kind = 'transliterated_folder'") >= 1);

    // Duplicates: full hashes make the translit copy an exact duplicate.
    let copies: Vec<i64> = {
        let conn = env.db.conn().unwrap();
        let mut st = conn.prepare("SELECT id FROM book_copy").unwrap();
        st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    for c in &copies {
        full_hash_copy(&env.db, *c, &env.mount, &NoopContext).unwrap();
    }
    detect_all(&env.db, None, &NoopContext).unwrap();
    let pairs = dbdup::list_open(&env.db.conn().unwrap(), true, 0, 50).unwrap();
    assert!(
        pairs.iter().any(|p| p.verdict == audiovault_core::model::DuplicateVerdict::ExactCopy),
        "expected an exact duplicate, got {:?}",
        pairs.iter().map(|p| (p.verdict, p.score)).collect::<Vec<_>>()
    );
    assert!(library::details(&env.db.conn().unwrap(), metro).unwrap().unwrap().copies.len() == 2);

    // Search in Cyrillic, Latin and transliteration.
    let conn = env.db.conn().unwrap();
    for q in ["метро", "Metro 2033", "glukhovsky", "глуховский метро", "karamazovy"] {
        let hits = search::search(&conn, q, 10).unwrap();
        assert!(!hits.is_empty(), "no hits for {q}");
    }
    drop(conn);

    // Unchanged rescan: nothing new, nothing changed.
    let s2 = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    assert_eq!(s2.counters.copies_new, 0);
    assert!(s2.changed_copies.is_empty(), "{:?}", s2.changed_copies);
    assert_eq!(s2.counters.copies_missing, 0);

    // Move a book folder: same copy, new path, nothing missing, hashes kept.
    let lib = env.mount.join("Аудиокниги");
    fs::create_dir_all(lib.join("Перемещено")).unwrap();
    fs::rename(lib.join("Глуховский Дмитрий/Метро 2033"), lib.join("Перемещено/Метро 2033")).unwrap();
    let s3 = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    assert_eq!(s3.counters.copies_new, 0, "move must not create a new copy");
    assert_eq!(s3.counters.copies_missing, 0);
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE rel_path = 'Аудиокниги/Перемещено/Метро 2033'"), 1);
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM media_file WHERE full_hash IS NULL AND kind = 'audio'"), 0);

    // Delete a copy: marked missing, book kept.
    fs::remove_dir_all(lib.join("Толстой Лев - Война и мир")).unwrap();
    let s4 = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    assert_eq!(s4.counters.copies_missing, 1);
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE state = 'missing'"), 1);
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book"), 3, "missing copies keep their book");

    // Modified file: copy re-analyzed.
    write_wav(&lib.join("Перемещено/Метро 2033/02.wav"), 4.0, 9);
    let s5 = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    assert_eq!(s5.changed_copies.len(), 1);
    analyze_all(&env);
    let dur = count(&env.db, "SELECT total_duration_ms FROM book_copy WHERE rel_path LIKE '%Перемещено/Метро 2033'");
    assert!((6900..=7100).contains(&dur), "duration after change {dur}");
}

/// Context that unplugs (renames away) the storage after a few checks.
struct Unplug {
    calls: AtomicUsize,
    root: PathBuf,
    away: PathBuf,
}

impl JobContext for Unplug {
    fn is_cancelled(&self) -> bool {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 2 {
            fs::rename(&self.root, &self.away).unwrap();
        }
        false
    }
    fn progress(&self, _: f64, _: &str) {}
}

#[test]
fn storage_disappearing_mid_scan_never_marks_missing() {
    let env = setup();
    scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    let before = count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE state = 'present'");
    let ctx = Unplug { calls: AtomicUsize::new(0), root: env.mount.join("Аудиокниги"), away: env.mount.with_extension("gone") };
    let s = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &ctx, None).unwrap();
    assert_eq!(s.outcome, ScanOutcome::Interrupted, "{s:?}");
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE state = 'missing'"), 0);
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE state = 'present'"), before);
    assert_eq!(count(&env.db, &format!("SELECT COUNT(*) FROM scan_run WHERE id = {} AND state = 'interrupted'", s.scan_id)), 1);
}

#[test]
fn offline_storage_remains_searchable() {
    let env = setup();
    scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None).unwrap();
    analyze_all(&env);
    // "Unplug" the disk entirely.
    let away = env.mount.with_extension("unplugged");
    fs::rename(&env.mount, &away).unwrap();
    {
        let conn = env.db.conn().unwrap();
        volumes::set_online(&conn, env.volume_id, false).unwrap();
        let hits = search::search(&conn, "братья карамазовы", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].copies.iter().all(|c| !c.online));
        assert_eq!(hits[0].variant_count, 2);
    }
    // Scanning an absent root is an interruption, not mass deletion.
    let s = scan_root(&env.db, env.volume_id, &env.mount, &env.root, &NoopContext, None);
    assert!(matches!(s, Err(audiovault_indexer::IndexerError::VolumeUnavailable(_))) || s.is_ok_and(|s| s.outcome == ScanOutcome::Interrupted));
    assert_eq!(count(&env.db, "SELECT COUNT(*) FROM book_copy WHERE state = 'missing'"), 0);
}

#[test]
fn acoustic_fingerprints_confirm_same_recording() {
    use audiovault_media::fingerprint::ChromaprintService;
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("disk");
    // Same audio, different files (one re-encoded would differ in bytes; here
    // we change only the file layout so hashes differ but audio matches).
    write_wav(&mount.join("A/Автор Иван - Книга/01.wav"), 40.0, 11);
    write_wav(&mount.join("B/Автор Иван - Книга (копия)/part1.wav"), 40.0, 11);
    let db = Db::open(&dir.path().join("c.sqlite")).unwrap();
    let (vol, root) = {
        let conn = db.conn().unwrap();
        let vol = volumes::insert(&conn, &ObservedVolume::synthetic("V", &mount.to_string_lossy()), "V").unwrap();
        let r = volumes::add_root(&conn, vol, "").unwrap();
        (vol, volumes::get_root(&conn, r).unwrap().unwrap())
    };
    scan_root(&db, vol, &mount, &root, &NoopContext, None).unwrap();
    let aenv = AnalyzeEnv { covers_dir: dir.path().join("covers"), policy: OptimizePolicy::default() };
    let ids: Vec<i64> = catalog::copies_needing_analysis(&db.conn().unwrap(), vol, 2, 10).unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids.len(), 2);
    for id in &ids {
        analyze_copy(&db, *id, &mount, &aenv, &NoopContext).unwrap();
    }
    let svc = ChromaprintService::new(None);
    for id in &ids {
        audiovault_indexer::hashing::fingerprint_copy(&db, *id, &mount, &svc, &NoopContext).unwrap();
    }
    detect_all(&db, Some(&svc), &NoopContext).unwrap();
    let pairs = dbdup::list_open(&db.conn().unwrap(), true, 0, 10).unwrap();
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    assert!(pairs[0].acoustic_checked);
    assert!(matches!(
        pairs[0].verdict,
        audiovault_core::model::DuplicateVerdict::SameRecording | audiovault_core::model::DuplicateVerdict::ExactCopy
    ));
}

#[test]
fn nested_discs_and_chapters_merged_into_single_book() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("mnt");
    // Emulates "Крестный отец/CD1/01_Glava_1/01.wav" and "Крестный отец/CD2/14_Glava_14/01.wav"
    write_wav(&mount.join("Крестный отец/CD1/01_Glava_1/01.wav"), 1.0, 1);
    write_wav(&mount.join("Крестный отец/CD1/02_Glava_2/01.wav"), 1.0, 2);
    write_wav(&mount.join("Крестный отец/CD2/14_Glava_14/01.wav"), 1.0, 3);
    let db = Db::open(&dir.path().join("c.sqlite")).unwrap();
    let (vol, root) = {
        let conn = db.conn().unwrap();
        let vol = volumes::insert(&conn, &ObservedVolume::synthetic("V", &mount.to_string_lossy()), "V").unwrap();
        let r = volumes::add_root(&conn, vol, "").unwrap();
        (vol, volumes::get_root(&conn, r).unwrap().unwrap())
    };
    let summary = scan_root(&db, vol, &mount, &root, &NoopContext, None).unwrap();
    assert_eq!(summary.outcome, ScanOutcome::Completed);
    let conn = db.conn().unwrap();
    let books: Vec<(i64, String)> = conn.prepare("SELECT id, title FROM book")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    // Exactly 1 book, with title "Крестный отец"
    assert_eq!(books.len(), 1, "Expected 1 book but found {books:?}");
    assert_eq!(books[0].1, "Крестный отец");
    let copy: (i64, u32) = conn.query_row("SELECT id, audio_file_count FROM book_copy", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(copy.1, 3, "Expected 3 audio files merged into copy");
}
