use anyhow::Result;
use audiovault_db::Db;
use audiovault_indexer::NoopContext;
use audiovault_platform::paths::AppPaths;
use rusqlite::Connection;

fn main() -> Result<()> {
    let paths = AppPaths::resolve()?;
    let db_path = paths.catalog_path();
    println!("Database path: {}", db_path.display());

    if !db_path.exists() {
        println!("Database does not exist yet.");
        return Ok(());
    }

    let args: Vec<String> = std::env::args().collect();
    let conn = Connection::open(&db_path)?;

    println!("\n=== VOLUMES ===");
    let mut stmt = conn.prepare("SELECT id, display_name, last_mount_path, is_online, storage_class, marker_uuid, serial FROM storage_volume")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    for r in rows {
        let (id, name, mount, online, sc, marker, serial) = r?;
        println!("Volume #{id}: {name} (mount={mount:?}, online={online}, class={sc}, marker={marker:?}, serial={serial:?})");
    }

    println!("\n=== LIBRARY ROOTS ===");
    let mut stmt = conn.prepare("SELECT id, volume_id, rel_path, enabled FROM library_root")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
        ))
    })?;

    for r in rows {
        let (id, vol_id, rel_path, active) = r?;
        println!("Root #{id}: vol_id={vol_id}, rel_path={rel_path:?}, enabled={active}");
    }

    println!("\n=== BACKGROUND JOBS ===");
    let mut stmt = conn.prepare("SELECT id, kind, state, title, attempts, error FROM background_job ORDER BY id DESC LIMIT 10")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;

    for r in rows {
        let (id, kind, state, title, attempts, err) = r?;
        println!("Job #{id}: [{state}] {kind} - {title} (attempts={attempts}, err={err:?})");
    }

    println!("\n=== SCAN RUNS ===");
    let mut stmt = conn.prepare("SELECT id, volume_id, state, dirs_seen, files_seen, copies_found, copies_new, errors, started_at, finished_at FROM scan_run ORDER BY id DESC LIMIT 10")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, Option<i64>>(9)?,
        ))
    })?;

    for r in rows {
        let (id, vol, state, d_seen, f_seen, c_found, c_new, errs, started, finished) = r?;
        println!("ScanRun #{id}: vol={vol} state={state} dirs={d_seen} files={f_seen} copies={c_found} new={c_new} errs={errs} (started={started}, finished={finished:?})");
    }

    println!("\n=== COUNTS ===");
    let books: i64 = conn.query_row("SELECT COUNT(*) FROM book", [], |r| r.get(0)).unwrap_or(0);
    let copies: i64 = conn.query_row("SELECT COUNT(*) FROM book_copy", [], |r| r.get(0)).unwrap_or(0);
    let media: i64 = conn.query_row("SELECT COUNT(*) FROM media_file", [], |r| r.get(0)).unwrap_or(0);
    let dups: i64 = conn.query_row("SELECT COUNT(*) FROM duplicate_candidate", [], |r| r.get(0)).unwrap_or(0);
    let opts: i64 = conn.query_row("SELECT COUNT(*) FROM optimization_candidate", [], |r| r.get(0)).unwrap_or(0);
    println!("Books: {books}, Book Copies: {copies}, Media Files: {media}, Duplicate Candidates: {dups}, Optimization Candidates: {opts}");

    if args.iter().any(|a| a == "--run-duplicates") {
        println!("\n=== RUNNING DUPLICATE DETECTION ===");
        let db = Db::open(&db_path)?;
        let stats = audiovault_indexer::duplicates::detect_all(&db, None, &NoopContext)?;
        println!("Duplicate detection complete: copies={}, pairs_scored={}, candidates={}", stats.copies, stats.pairs_scored, stats.candidates);

        let mut stmt = conn.prepare(
            "SELECT dc.id, dc.verdict, dc.score, b1.title, c1.rel_path, c2.rel_path
             FROM duplicate_candidate dc
             JOIN book_copy c1 ON dc.copy_a = c1.id
             JOIN book_copy c2 ON dc.copy_b = c2.id
             JOIN book b1 ON c1.book_id = b1.id
             LIMIT 10"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        println!("\nTop duplicates found:");
        for r in rows {
            let (id, verdict, score, title, p1, p2) = r?;
            println!("  Pair #{id} [{verdict}, score={:.2}]: \"{}\"", score, title);
            println!("    A: {}", p1);
            println!("    B: {}", p2);
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--search") {
        if let Some(query) = args.get(pos + 1) {
            println!("\n=== SEARCH TEST: \"{query}\" ===");
            let start = std::time::Instant::now();
            let hits = audiovault_db::search::search(&conn, query, 20)?;
            let elapsed = start.elapsed();
            println!("Found {} results in {:.2?}:", hits.len(), elapsed);
            for (idx, hit) in hits.iter().enumerate() {
                println!(
                    "  #{}: \"{}\" by {} (narrator: \"{}\", format: {}, duration: {} ms, score: {:.2})",
                    idx + 1,
                    hit.title,
                    hit.authors,
                    hit.narrators,
                    hit.format,
                    hit.duration_ms,
                    hit.score
                );
                for copy in &hit.copies {
                    println!("      Path: {}", copy.display_path());
                }
            }
        }
    }

    Ok(())
}
