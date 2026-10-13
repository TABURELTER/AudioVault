use anyhow::Result;
use audiovault_db::Db;
use audiovault_indexer::NoopContext;
use audiovault_platform::paths::AppPaths;
use rusqlite::{Connection, OptionalExtension};

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

    // Ensure DB migrations (including 0004_ebooks) are applied
    let _app_db = Db::open(&db_path)?;
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS ebook_ad;
         DROP TRIGGER IF EXISTS ebook_au;
         CREATE TRIGGER ebook_ad AFTER DELETE ON ebook_entry BEGIN
             DELETE FROM ebook_fts WHERE rowid = old.id;
         END;
         CREATE TRIGGER ebook_au AFTER UPDATE ON ebook_entry BEGIN
             DELETE FROM ebook_fts WHERE rowid = old.id;
             INSERT INTO ebook_fts(rowid, title, authors, series, genres, description)
             VALUES (new.id, new.title, new.authors, coalesce(new.series, ''), coalesce(new.genres, ''), coalesce(new.description, ''));
         END;"
    )?;

    println!("\n=== COUNTS ===");
    let books: i64 = conn.query_row("SELECT COUNT(*) FROM book", [], |r| r.get(0)).unwrap_or(0);
    let copies: i64 = conn.query_row("SELECT COUNT(*) FROM book_copy", [], |r| r.get(0)).unwrap_or(0);
    let media: i64 = conn.query_row("SELECT COUNT(*) FROM media_file", [], |r| r.get(0)).unwrap_or(0);
    let dups: i64 = conn.query_row("SELECT COUNT(*) FROM duplicate_candidate", [], |r| r.get(0)).unwrap_or(0);
    let opts: i64 = conn.query_row("SELECT COUNT(*) FROM optimization_candidate", [], |r| r.get(0)).unwrap_or(0);
    let ebooks: i64 = conn.query_row("SELECT COUNT(*) FROM ebook_entry WHERE state = 'present'", [], |r| r.get(0)).unwrap_or(0);
    println!("Books: {books}, Book Copies: {copies}, Media Files: {media}, Duplicate Candidates: {dups}, Optimization Candidates: {opts}, E-books: {ebooks}");

    if args.iter().any(|a| a == "--vol1") {
        println!("\n=== COPIES ON VOLUME #1 (F:\\) ===");
        let mut stmt = conn.prepare(
            "SELECT c.id, c.rel_path, b.title, b.authors_display, c.file_count, c.total_size 
             FROM book_copy c 
             JOIN book b ON c.book_id = b.id 
             WHERE c.volume_id = 1 
             ORDER BY c.id"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?;
        for r in rows {
            let (id, rel_path, title, author, files, size) = r?;
            let size_mb = size / 1024 / 1024;
            println!("  Copy #{id}: \"{title}\" (author={author:?}, files={files}, size={size_mb} MB)\n    Path: {rel_path}");
        }
    }

    if args.iter().any(|a| a == "--cleanup-spurious") {
        conn.execute("DELETE FROM book_copy WHERE rel_path = 'МОИ КНИГИ'", [])?;
        conn.execute("DELETE FROM book WHERE id NOT IN (SELECT book_id FROM book_copy)", [])?;
        println!("Cleaned up spurious entries.");
    }

    if args.iter().any(|a| a == "--diagnose-copies") {
        println!("\n=== DIAGNOSING BOOKS WITH MULTIPLE COPIES ===");
        let mut stmt = conn.prepare(
            "SELECT b.id, b.title, b.authors_display, COUNT(c.id) as cnt
             FROM book b
             JOIN book_copy c ON c.book_id = b.id AND c.state = 'present'
             GROUP BY b.id
             HAVING cnt > 1
             ORDER BY cnt DESC"
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?))
        })?;

        let mut mismatched = 0;
        for r in rows {
            let (bid, btitle, bauthor, cnt) = r?;
            let mut c_stmt = conn.prepare(
                "SELECT c.id, c.rel_path, c.total_duration_ms, c.total_size, c.primary_codec
                 FROM book_copy c WHERE c.book_id = ?1 AND c.state = 'present'"
            )?;
            let copies: Vec<(i64, String, i64, i64, Option<String>)> = c_stmt.query_map([bid], |cr| {
                Ok((cr.get(0)?, cr.get(1)?, cr.get(2)?, cr.get(3)?, cr.get(4)?))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;

            // Check if any copy has a volume indicator that differs from others
            let vols: Vec<Option<audiovault_core::infer::VolumeInfo>> = copies.iter().map(|c| {
                audiovault_core::infer::extract_volume(&c.1)
            }).collect();

            let has_different_vols = vols.iter().flatten().map(|v| v.index.map(|i| (i * 10.0) as i64)).collect::<std::collections::HashSet<_>>().len() > 1;
            let mut dur_mismatch = false;
            let first_dur = copies[0].2;
            for c in &copies[1..] {
                if first_dur > 0 && c.2 > 0 {
                    let diff = (first_dur - c.2).abs() as f64 / (first_dur.max(c.2) as f64);
                    if diff > 0.20 {
                        dur_mismatch = true;
                    }
                }
            }

            if has_different_vols || dur_mismatch {
                mismatched += 1;
                println!("\n[MISMATCH] Book #{bid}: \"{btitle}\" (author={bauthor:?}, present_copies={cnt}):");
                for (c, v) in copies.iter().zip(&vols) {
                    println!("    Copy #{}: dur={}s, vol={:?}, path={}", c.0, c.2 / 1000, v.as_ref().map(|v| &v.raw), c.1);
                }
            }
        }
        println!("\nTotal mismatched multi-copy books: {mismatched}");
    }

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

    if args.iter().any(|a| a == "--repair-catalog") {
        println!("\n=== REPAIRING CATALOG ===");

        // 1. Clean up missing copies on online volumes
        println!("1. Cleaning up stale missing copies on online volumes...");
        let stale_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM book_copy c JOIN storage_volume v ON v.id = c.volume_id WHERE v.is_online = 1 AND c.state = 'missing'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);
        if stale_count > 0 {
            conn.execute(
                "DELETE FROM metadata_observation WHERE copy_id IN (
                    SELECT c.id FROM book_copy c JOIN storage_volume v ON v.id = c.volume_id WHERE v.is_online = 1 AND c.state = 'missing'
                )",
                [],
            )?;
            conn.execute(
                "DELETE FROM media_file WHERE copy_id IN (
                    SELECT c.id FROM book_copy c JOIN storage_volume v ON v.id = c.volume_id WHERE v.is_online = 1 AND c.state = 'missing'
                )",
                [],
            )?;
            conn.execute(
                "DELETE FROM book_copy WHERE id IN (
                    SELECT c.id FROM book_copy c JOIN storage_volume v ON v.id = c.volume_id WHERE v.is_online = 1 AND c.state = 'missing'
                )",
                [],
            )?;
            println!("  Deleted {stale_count} stale missing copies from online volumes.");
        }

        // 2. Identify and reassign copies whose title/author/volume mismatches their book
        println!("2. Re-evaluating copy assignments...");
        let mut copy_stmt = conn.prepare(
            "SELECT c.id, c.book_id, c.volume_id, c.rel_path, c.is_single_file FROM book_copy c WHERE c.state = 'present'"
        )?;
        let copies: Vec<(i64, i64, i64, String, bool)> = copy_stmt.query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get::<_, i64>(4)? != 0))
        })?.collect::<rusqlite::Result<Vec<_>>>()?;

        let mut moved_count = 0;
        let mut created_count = 0;

        for (copy_id, current_book_id, _vol_id, rel_path, _is_single) in copies {
            // Get media rows for this copy
            let mut m_stmt = conn.prepare(
                "SELECT tag_title, tag_album, tag_artist, tag_album_artist FROM media_file WHERE copy_id = ?1 AND state = 'present'"
            )?;
            let media_tags: Vec<(Option<String>, Option<String>, Option<String>, Option<String>)> = m_stmt.query_map([copy_id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?.collect::<rusqlite::Result<Vec<_>>>()?;

            // Infer from path
            let comps: Vec<String> = rel_path.split(['/', '\\']).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect();
            let refs: Vec<&str> = comps.iter().map(String::as_str).collect();
            let inferred = audiovault_core::infer::infer_from_path(&refs);

            // Determine volume from path or tags
            let vol_from_path = audiovault_core::infer::extract_volume(&rel_path);
            let vol_from_tags = media_tags.iter().find_map(|(tt, _, _, _)| {
                tt.as_deref().and_then(audiovault_core::infer::extract_volume)
            });
            let vol = vol_from_path.or(vol_from_tags);

            // Check tag album & title
            let mut tag_title = None;
            let mut tag_author = None;
            for (_tt, ta, tar, talb) in &media_tags {
                if tag_author.is_none() {
                    if let Some(a) = talb.as_deref().or(tar.as_deref()) {
                        let trimmed = a.trim();
                        if !trimmed.is_empty() && !audiovault_core::normalize::is_junk_value(trimmed) && trimmed.len() >= 2 {
                            tag_author = Some(trimmed.to_string());
                        }
                    }
                }
                if tag_title.is_none() {
                    if let Some(alb) = ta {
                        let trimmed = alb.trim();
                        if !trimmed.is_empty() && !audiovault_core::normalize::is_junk_value(trimmed) {
                            tag_title = Some(trimmed.to_string());
                        }
                    }
                }
            }

            // Combine tag title with volume if needed
            let mut final_title = tag_title.or_else(|| inferred.title.clone()).unwrap_or_else(|| {
                comps.last().cloned().unwrap_or_else(|| "Untitled".into())
            });

            if let Some(v) = &vol {
                if audiovault_core::infer::extract_volume(&final_title).is_none() {
                    final_title = format!("{final_title}. {}", v.raw);
                }
            }

            let final_author = tag_author.or_else(|| inferred.authors.first().cloned()).unwrap_or_default();
            let authors_vec = if final_author.is_empty() { vec![] } else { vec![final_author.clone()] };

            let identity = audiovault_core::identity::book_identity_key(&authors_vec, &final_title);
            if identity.is_empty() || identity.starts_with('#') {
                continue; // Skip generic identity
            }

            // Check current book identity
            let curr_book_ident: Option<String> = conn.query_row(
                "SELECT identity_key FROM book WHERE id = ?1",
                [current_book_id],
                |r| r.get(0),
            ).optional()?.flatten();

            let current_book_copy_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM book_copy WHERE book_id = ?1 AND state = 'present'",
                [current_book_id],
                |r| r.get(0),
            ).unwrap_or(0);

            if curr_book_ident.as_deref() == Some(&identity) {
                // Already matches!
                continue;
            }

            // If a book with this identity already exists, move copy to that book!
            let target_book_id = conn.query_row(
                "SELECT id FROM book WHERE identity_key = ?1 AND id <> ?2 LIMIT 1",
                rusqlite::params![identity, current_book_id],
                |r| r.get::<_, i64>(0),
            ).optional()?;

            if let Some(target) = target_book_id {
                let var_id = audiovault_db::catalog::variant_for_new_copy(&conn, target, &inferred.narrators)?;
                conn.execute(
                    "UPDATE book_copy SET book_id = ?1, variant_id = ?2 WHERE id = ?3",
                    rusqlite::params![target, var_id, copy_id],
                )?;
                conn.execute(
                    "UPDATE metadata_observation SET book_id = ?1 WHERE copy_id = ?2",
                    rusqlite::params![target, copy_id],
                )?;
                moved_count += 1;
                continue;
            }

            // If current book only has THIS single copy and no other book has this identity, update current book!
            if current_book_copy_count <= 1 {
                conn.execute(
                    "UPDATE book SET title = ?2, identity_key = ?3 WHERE id = ?1",
                    rusqlite::params![current_book_id, final_title, identity],
                )?;
                continue;
            }

            // Otherwise, this book had multiple different copies; create a new book for this copy
            let new_id = audiovault_db::catalog::create_book(&conn, &final_title, &authors_vec)?;
            created_count += 1;

            let var_id = audiovault_db::catalog::variant_for_new_copy(&conn, new_id, &inferred.narrators)?;
            conn.execute(
                "UPDATE book_copy SET book_id = ?1, variant_id = ?2 WHERE id = ?3",
                rusqlite::params![new_id, var_id, copy_id],
            )?;
            conn.execute(
                "UPDATE metadata_observation SET book_id = ?1 WHERE copy_id = ?2",
                rusqlite::params![new_id, copy_id],
            )?;
            moved_count += 1;
        }

        println!("  Moved {moved_count} copies, created {created_count} books.");

        // 3. Delete orphan books
        println!("3. Deleting orphan books without copies...");
        let deleted_orphans = audiovault_db::catalog::delete_orphan_books(&conn)?;
        println!("  Deleted {deleted_orphans} orphan books.");

        // 4. Refresh all remaining books
        println!("4. Refreshing metadata and canonical titles for all books...");
        let mut b_stmt = conn.prepare("SELECT id FROM book")?;
        let book_ids: Vec<i64> = b_stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for bid in book_ids {
            let obs = audiovault_db::catalog::observations_for_resolution(&conn, bid)?;
            let resolved = audiovault_core::metadata::resolve(&obs);
            audiovault_db::catalog::apply_canonical(&conn, bid, &resolved)?;
            audiovault_db::catalog::refresh_book(&conn, bid)?;
        }
        println!("  Refreshed all books.");

        // 5. Clean and regenerate duplicate candidates
        println!("5. Regenerating duplicate candidates with new duplicate algorithm...");
        conn.execute("DELETE FROM duplicate_candidate", [])?;
        let db = Db::open(&db_path)?;
        let stats = audiovault_indexer::duplicates::detect_all(&db, None, &NoopContext)?;
        println!("  Duplicate detection complete: copies={}, pairs_scored={}, candidates={}", stats.copies, stats.pairs_scored, stats.candidates);
        println!("\n=== REPAIR COMPLETE ===");
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

    if let Some(pos) = args.iter().position(|a| a == "--inspect-book") {
        if let Some(query) = args.get(pos + 1) {
            println!("\n=== INSPECT BOOK: \"{query}\" ===");
            let mut stmt = conn.prepare(
                "SELECT id, title, authors_display, identity_key FROM book WHERE id = ?1 OR title LIKE ?2"
            )?;
            let book_rows = stmt.query_map(rusqlite::params![query.parse::<i64>().unwrap_or(-1), format!("%{query}%")], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, String>(3)?))
            })?;
            for b in book_rows {
                let (bid, title, authors, identity) = b?;
                println!("\n--- BOOK #{bid}: \"{title}\" (author={authors:?}, identity={identity}) ---");

                println!("  Copies:");
                let mut c_stmt = conn.prepare(
                    "SELECT id, rel_path, state, coalesce(container, primary_codec, ''), total_duration_ms, file_count, total_size, variant_id FROM book_copy WHERE book_id = ?1"
                )?;
                let c_rows = c_stmt.query_map([bid], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                        r.get::<_, i64>(6)?,
                        r.get::<_, i64>(7)?,
                    ))
                })?;
                let mut copy_ids = Vec::new();
                for c in c_rows {
                    let (cid, rel_path, state, fmt, dur, fc, sz, vid) = c?;
                    copy_ids.push(cid);
                    println!("    Copy #{cid} (var={vid}, state={state}, fmt={fmt}, dur={}s, files={fc}, size={}MB):\n      {rel_path}", dur / 1000, sz / 1024 / 1024);
                    let mut m_stmt = conn.prepare(
                        "SELECT id, rel_path, tag_title, tag_album, tag_artist, track_no, disc_no FROM media_file WHERE copy_id = ?1 LIMIT 3"
                    )?;
                    let m_rows = m_stmt.query_map([cid], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, Option<i64>>(5)?,
                            r.get::<_, Option<i64>>(6)?,
                        ))
                    })?;
                    for m in m_rows {
                        let (mid, mpath, tt, ta, tar, tr, dno) = m?;
                        println!("        Media #{mid}: \"{mpath}\" (tag_title={tt:?}, tag_album={ta:?}, tag_artist={tar:?}, track={tr:?}, disc={dno:?})");
                    }
                }

                println!("  Aliases:");
                let mut a_stmt = conn.prepare(
                    "SELECT alias, kind, source FROM book_alias WHERE book_id = ?1"
                )?;
                let a_rows = a_stmt.query_map([bid], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
                })?;
                for a in a_rows {
                    let (alias, kind, source) = a?;
                    println!("    Alias: \"{alias}\" (kind={kind}, source={source})");
                }

                println!("  Observations:");
                let mut o_stmt = conn.prepare(
                    "SELECT id, copy_id, field, value, source, confidence FROM metadata_observation WHERE book_id = ?1 ORDER BY field, id"
                )?;
                let o_rows = o_stmt.query_map([bid], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, f64>(5)?,
                    ))
                })?;
                for o in o_rows {
                    let (oid, cid, field, val, src, conf) = o?;
                    println!("    Obs #{oid} [cid={cid:?}] {field}={val:?} (src={src}, conf={conf:.2})");
                }

                println!("  Duplicate Candidates for these copies (sample):");
                for cid in copy_ids.iter().take(2) {
                    let mut d_stmt = conn.prepare(
                        "SELECT id, copy_a, copy_b, score, verdict, evidence FROM duplicate_candidate WHERE copy_a = ?1 OR copy_b = ?1 LIMIT 3"
                    )?;
                    let d_rows = d_stmt.query_map([cid], |r| {
                        Ok((
                            r.get::<_, i64>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, f64>(3)?,
                            r.get::<_, String>(4)?,
                            r.get::<_, String>(5)?,
                        ))
                    })?;
                    for d in d_rows {
                        let (did, ca, cb, score, verdict, ev) = d?;
                        println!("    Dup #{did}: copy {ca} vs {cb} => {verdict} (score={score:.2}, evidence={ev})");
                    }
                }
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--scan-volume") {
        let vol_id: i64 = args.get(pos + 1).and_then(|s| s.parse().ok()).unwrap_or(1);
        println!("\n=== SCANNING VOLUME #{vol_id} ===");
        let db = Db::open(&db_path)?;
        let roots = {
            let conn = db.conn()?;
            audiovault_db::volumes::roots(&conn, Some(vol_id))?
        };
        let mount_str = {
            let conn = db.conn()?;
            audiovault_db::volumes::get(&conn, vol_id)?
                .and_then(|v| v.last_mount_path)
                .ok_or_else(|| anyhow::anyhow!("Volume not found or has no mount path"))?
        };
        let mount = std::path::PathBuf::from(&mount_str);

        let covers = paths.cover_cache_dir;
        for root in roots {
            if !root.enabled {
                continue;
            }
            println!("Scanning root #{} (rel_path={:?}) on {}...", root.id, root.rel_path, mount.display());
            let summary = audiovault_indexer::scan::scan_root(&db, vol_id, &mount, &root, &NoopContext, Some(&covers))?;
            println!(
                "Scan complete! Outcome: {:?}, dirs={}, files={}, copies_found={}, copies_new={}",
                summary.outcome,
                summary.counters.dirs_seen,
                summary.counters.files_seen,
                summary.counters.copies_found,
                summary.counters.copies_new
            );
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--search-ebooks") {
        let query = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("");
        println!("\n=== SEARCH EBOOKS: \"{query}\" ===");
        let conn = Connection::open(&db_path)?;
        let start = std::time::Instant::now();
        let query_opt = if query.is_empty() { None } else { Some(query) };
        let results = audiovault_db::ebooks::search_ebooks(&conn, query_opt, None, None, 20, 0)?;
        let elapsed = start.elapsed();
        println!("Found {} total matching e-books in {:.2?}:", results.total_count, elapsed);
        for (idx, b) in results.items.iter().enumerate() {
            println!(
                "  #{}: \"{}\" by {} (format: {}, series: {:?} #{:?}, size: {} KB, path: {})",
                idx + 1,
                b.title,
                b.authors,
                b.format,
                b.series,
                b.series_index,
                b.size / 1024,
                b.rel_path
            );
        }
    }

    if args.iter().any(|a| a == "--ebook-stats") {
        println!("\n=== EBOOK STATS ===");
        let conn = Connection::open(&db_path)?;
        let stats = audiovault_db::ebooks::get_ebook_stats(&conn)?;
        println!("Total E-books: {}", stats.total_count);
        println!("Total Authors: {}", stats.authors_count);
        println!("Formats breakdown: {:?}", stats.formats);
        println!("Total size: {} MB", stats.total_size_bytes / 1024 / 1024);
    }

    Ok(())
}
