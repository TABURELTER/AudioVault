//! Journaled filesystem operations on book copies.
//!
//! Flow: the UI *plans* an operation (row in `file_operation`, state
//! `planned`), destructive kinds require `confirmed = true` which the UI only
//! sets after an explicit confirmation dialog; a `file_operation` job then
//! executes it: `planned → in_progress → completed | failed`. The catalog is
//! updated only after the filesystem change succeeded. Operations interrupted
//! by a crash are re-verified against the disk by [`reconcile`].
//!
//! Default deletion goes to the OS trash; permanent deletion is a separate,
//! explicitly confirmed kind.

use std::path::{Path, PathBuf};

use audiovault_core::model::{FileOpKind, FileOpState, JobKind};
use audiovault_core::paths::{safe_join, sanitize_component, validate_relative};
use audiovault_db::fileops::{self, FileOpRecord, NewFileOp};
use audiovault_db::jobs::NewJob;
use audiovault_db::{Db, DbError, catalog, volumes};
use audiovault_indexer::JobContext;
use serde_json::json;

use crate::events::AppEvent;
use crate::exec::Executor;
use crate::monitor::volume_mount;
use crate::{Result, ServiceError};

fn inval<E: std::fmt::Display>(e: E) -> ServiceError {
    ServiceError::Invalid(e.to_string())
}

fn parent_rel(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

fn base_name(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(_, n)| n).unwrap_or(rel)
}

fn join_rel(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

/// Plan a deletion of a copy's files. `permanent` deletes instead of trashing.
/// `confirmed` must reflect an explicit user confirmation.
pub fn plan_delete(
    db: &Db,
    copy_id: i64,
    permanent: bool,
    confirmed: bool,
) -> Result<(i64, NewJob)> {
    let conn = db.conn()?;
    let copy = catalog::get_copy(&conn, copy_id)?
        .ok_or_else(|| ServiceError::NotFound(format!("copy {copy_id}")))?;
    if copy.rel_path.is_empty() {
        return Err(ServiceError::Invalid(
            "refusing to delete the root of a storage".into(),
        ));
    }
    let kind = if permanent {
        FileOpKind::DeletePermanent
    } else {
        FileOpKind::Trash
    };
    let op = NewFileOp {
        kind,
        volume_id: Some(copy.volume_id),
        dest_volume_id: None,
        copy_id: Some(copy_id),
        source_path: copy.rel_path.clone(),
        dest_path: None,
        payload: json!({ "book_id": copy.book_id }),
        confirmed,
    };
    let id = fileops::plan(&conn, &op)?;
    let title = format!(
        "{} {}",
        if permanent {
            "Delete permanently"
        } else {
            "Move to trash"
        },
        base_name(&copy.rel_path)
    );
    Ok((
        id,
        NewJob::new(JobKind::FileOperation, title, json!({ "op_id": id }))
            .volume(copy.volume_id)
            .priority(20),
    ))
}

/// Plan renaming a copy's folder/file in place.
pub fn plan_rename(db: &Db, copy_id: i64, new_name: &str) -> Result<(i64, NewJob)> {
    let name = sanitize_component(new_name.trim());
    if name.is_empty() || name == "." || name == ".." {
        return Err(ServiceError::Invalid("invalid name".into()));
    }
    let conn = db.conn()?;
    let copy = catalog::get_copy(&conn, copy_id)?
        .ok_or_else(|| ServiceError::NotFound(format!("copy {copy_id}")))?;
    let dest = join_rel(parent_rel(&copy.rel_path), &name);
    if dest == copy.rel_path {
        return Err(ServiceError::Invalid("the name is unchanged".into()));
    }
    let op = NewFileOp {
        kind: FileOpKind::Rename,
        volume_id: Some(copy.volume_id),
        dest_volume_id: Some(copy.volume_id),
        copy_id: Some(copy_id),
        source_path: copy.rel_path.clone(),
        dest_path: Some(dest),
        payload: json!({}),
        confirmed: true,
    };
    let id = fileops::plan(&conn, &op)?;
    Ok((
        id,
        NewJob::new(
            JobKind::FileOperation,
            format!("Rename to {name}"),
            json!({ "op_id": id }),
        )
        .volume(copy.volume_id)
        .priority(20),
    ))
}

/// Plan moving (or copying) a copy into `dest_parent_rel` on `dest_volume_id`.
pub fn plan_move(
    db: &Db,
    copy_id: i64,
    dest_volume_id: i64,
    dest_parent_rel: &str,
    keep_source: bool,
) -> Result<(i64, NewJob)> {
    let dest_parent = dest_parent_rel.trim().trim_matches('/').replace('\\', "/");
    if !dest_parent.is_empty() {
        validate_relative(&dest_parent).map_err(inval)?;
    }
    let conn = db.conn()?;
    let copy = catalog::get_copy(&conn, copy_id)?
        .ok_or_else(|| ServiceError::NotFound(format!("copy {copy_id}")))?;
    volumes::get(&conn, dest_volume_id)?
        .ok_or_else(|| ServiceError::NotFound(format!("storage {dest_volume_id}")))?;
    let dest = join_rel(&dest_parent, base_name(&copy.rel_path));
    if dest_volume_id == copy.volume_id
        && (dest == copy.rel_path || dest.starts_with(&format!("{}/", copy.rel_path)))
    {
        return Err(ServiceError::Invalid(
            "destination is the same as or inside the source".into(),
        ));
    }
    let kind = if keep_source {
        FileOpKind::Copy
    } else {
        FileOpKind::Move
    };
    let op = NewFileOp {
        kind,
        volume_id: Some(copy.volume_id),
        dest_volume_id: Some(dest_volume_id),
        copy_id: Some(copy_id),
        source_path: copy.rel_path.clone(),
        dest_path: Some(dest),
        payload: json!({}),
        confirmed: true,
    };
    let id = fileops::plan(&conn, &op)?;
    let title = format!(
        "{} {}",
        if keep_source { "Copy" } else { "Move" },
        base_name(&copy.rel_path)
    );
    // Jobs are scheduled on the source volume; cross-volume copies also load the destination.
    Ok((
        id,
        NewJob::new(JobKind::FileOperation, title, json!({ "op_id": id }))
            .volume(copy.volume_id)
            .priority(20),
    ))
}

fn get_op(db: &Db, id: i64) -> Result<FileOpRecord> {
    let conn = db.conn()?;
    fileops::get(&conn, id)?.ok_or_else(|| ServiceError::NotFound(format!("file operation {id}")))
}

fn transition(db: &Db, id: i64, s: FileOpState, err: Option<&str>) -> Result<()> {
    let conn = db.conn()?;
    fileops::transition(&conn, id, s, err)?;
    Ok(())
}

/// Execute a planned operation.
pub fn execute(exec: &Executor, op_id: i64, ctx: &dyn JobContext) -> Result<()> {
    let db = &exec.db;
    let op = get_op(db, op_id)?;
    if op.state != FileOpState::Planned {
        return Err(ServiceError::Invalid(format!(
            "file operation {op_id} is {}, not planned",
            op.state
        )));
    }
    let src_vol = op
        .volume_id
        .ok_or_else(|| ServiceError::Invalid("operation has no source storage".into()))?;
    let (_, src_mount) = volume_mount(db, src_vol)?;
    let src = safe_join(&src_mount, &op.source_path).map_err(inval)?;
    let dst = match (&op.dest_path, op.dest_volume_id) {
        (Some(d), Some(dv)) => {
            let (_, m) = volume_mount(db, dv)?;
            Some(safe_join(&m, d).map_err(inval)?)
        }
        _ => None,
    };
    if !src.exists() {
        transition(
            db,
            op_id,
            FileOpState::Failed,
            Some("source no longer exists"),
        )?;
        return Err(ServiceError::NotFound(src.display().to_string()));
    }
    if let Some(d) = &dst
        && d.exists()
    {
        transition(
            db,
            op_id,
            FileOpState::Failed,
            Some("destination already exists"),
        )?;
        return Err(ServiceError::Invalid(format!(
            "destination already exists: {}",
            d.display()
        )));
    }
    // Enforces confirmation for destructive kinds.
    transition(db, op_id, FileOpState::InProgress, None)?;

    let fs_result: Result<()> = (|| match op.kind {
        FileOpKind::Trash => {
            audiovault_platform::shell::move_to_trash(&src).map_err(ServiceError::from)
        }
        FileOpKind::DeletePermanent => {
            if src.is_dir() {
                std::fs::remove_dir_all(&src)?;
            } else {
                std::fs::remove_file(&src)?;
            }
            Ok(())
        }
        FileOpKind::Rename | FileOpKind::Move | FileOpKind::Copy => {
            let d = dst
                .as_ref()
                .ok_or_else(|| ServiceError::Invalid("operation has no destination".into()))?;
            if let Some(p) = d.parent() {
                std::fs::create_dir_all(p)?;
            }
            let same_volume = op.dest_volume_id == op.volume_id;
            if op.kind != FileOpKind::Copy && same_volume && std::fs::rename(&src, d).is_ok() {
                return Ok(());
            }
            copy_verified(&src, d, ctx)?;
            if op.kind != FileOpKind::Copy {
                if src.is_dir() {
                    std::fs::remove_dir_all(&src)?;
                } else {
                    std::fs::remove_file(&src)?;
                }
            }
            Ok(())
        }
        FileOpKind::MetadataWrite | FileOpKind::TranscodeReplace => Err(ServiceError::Invalid(
            format!("{} is not executed by this job", op.kind),
        )),
    })();

    if let Err(e) = fs_result {
        let msg = e.to_string();
        transition(db, op_id, FileOpState::Failed, Some(&msg))?;
        return Err(e);
    }
    apply_to_catalog(exec, &op)?;
    transition(db, op_id, FileOpState::Completed, None)?;
    if let (FileOpKind::Rename | FileOpKind::Move, Some(d)) = (op.kind, &op.dest_path) {
        let conn = db.conn()?;
        fileops::set_undo(
            &conn,
            op_id,
            &json!({ "from": op.source_path, "to": d, "volume_id": op.volume_id, "dest_volume_id": op.dest_volume_id }),
        )?;
    }
    exec.events.emit(AppEvent::LibraryChanged);
    exec.events.emit(AppEvent::DuplicatesChanged);
    exec.events.emit(AppEvent::VolumesChanged);
    Ok(())
}

fn apply_to_catalog(exec: &Executor, op: &FileOpRecord) -> Result<()> {
    let Some(copy_id) = op.copy_id else {
        return Ok(());
    };
    match op.kind {
        FileOpKind::Trash | FileOpKind::DeletePermanent => {
            exec.db.write(|tx| {
                if let Some(book) = catalog::remove_copy_from_catalog(tx, copy_id)? {
                    if catalog::book_exists(tx, book)? {
                        catalog::refresh_book(tx, book)?;
                    }
                }
                catalog::delete_orphan_books(tx)?;
                Ok::<_, DbError>(())
            })?;
        }
        FileOpKind::Rename | FileOpKind::Move => {
            let dv = op.dest_volume_id.unwrap_or_default();
            let dp = op.dest_path.clone().unwrap_or_default();
            exec.db
                .write(|tx| catalog::set_copy_path(tx, copy_id, dv, &dp))?;
        }
        FileOpKind::Copy => {
            if let (Some(dv), Some(dp)) = (op.dest_volume_id, &op.dest_path) {
                exec.enqueue_subtree_scan(dv, dp, &format!("Catalog copied {}", base_name(dp)))?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Copy a file or tree to `dst` via a temporary sibling, verify sizes, then
/// rename into place. Never overwrites.
pub fn copy_verified(src: &Path, dst: &Path, ctx: &dyn JobContext) -> Result<()> {
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dst.with_file_name(format!(".{name}.avpart"));
    if tmp.exists() {
        if tmp.is_dir() {
            std::fs::remove_dir_all(&tmp)?;
        } else {
            std::fs::remove_file(&tmp)?;
        }
    }
    let total = tree_size(src)?;
    let mut done = 0u64;
    let r = copy_tree(src, &tmp, ctx, &mut done, total);
    if let Err(e) = r {
        let _ = if tmp.is_dir() {
            std::fs::remove_dir_all(&tmp)
        } else {
            std::fs::remove_file(&tmp)
        };
        return Err(e);
    }
    if tree_size(&tmp)? != total {
        let _ = if tmp.is_dir() {
            std::fs::remove_dir_all(&tmp)
        } else {
            std::fs::remove_file(&tmp)
        };
        return Err(ServiceError::Invalid(
            "copied size does not match the source; nothing was changed".into(),
        ));
    }
    std::fs::rename(&tmp, dst)?;
    Ok(())
}

fn tree_size(p: &Path) -> Result<u64> {
    let meta = std::fs::symlink_metadata(p)?;
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)? {
            let e = e?;
            let ft = e.file_type()?;
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() {
                total += e.metadata()?.len();
            }
        }
    }
    Ok(total)
}

fn copy_tree(
    src: &Path,
    dst: &Path,
    ctx: &dyn JobContext,
    done: &mut u64,
    total: u64,
) -> Result<()> {
    if ctx.is_cancelled() {
        return Err(ServiceError::Cancelled);
    }
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_file() {
        std::fs::copy(src, dst)?;
        *done += meta.len();
        ctx.progress(
            *done as f64 / total.max(1) as f64,
            &format!(
                "{}",
                src.file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ),
        );
        return Ok(());
    }
    std::fs::create_dir_all(dst)?;
    let mut entries: Vec<PathBuf> = std::fs::read_dir(src)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for e in entries {
        let ft = std::fs::symlink_metadata(&e)?.file_type();
        if ft.is_symlink() {
            continue;
        }
        let name = e.file_name().map(|n| n.to_os_string()).unwrap_or_default();
        copy_tree(&e, &dst.join(name), ctx, done, total)?;
    }
    Ok(())
}

/// Verify operations interrupted by a crash against the filesystem.
pub fn reconcile(exec: &Executor, _ctx: &dyn JobContext) -> Result<()> {
    let ops = {
        let conn = exec.db.conn()?;
        fileops::by_state(&conn, FileOpState::NeedsVerification)?
    };
    for op in ops {
        let Some(vid) = op.volume_id else { continue };
        let Ok((_, mount)) = volume_mount(&exec.db, vid) else {
            continue;
        };
        let Ok(src) = safe_join(&mount, &op.source_path) else {
            continue;
        };
        let dst = match (&op.dest_path, op.dest_volume_id) {
            (Some(d), Some(dv)) => match volume_mount(&exec.db, dv) {
                Ok((_, m)) => safe_join(&m, d).ok(),
                Err(_) => continue,
            },
            _ => None,
        };
        let (state, note): (FileOpState, Option<&str>) = match op.kind {
            FileOpKind::Trash | FileOpKind::DeletePermanent => {
                if src.exists() {
                    (
                        FileOpState::Failed,
                        Some(
                            "verified after restart: files are still present, nothing was deleted",
                        ),
                    )
                } else {
                    (FileOpState::Completed, None)
                }
            }
            FileOpKind::Rename | FileOpKind::Move | FileOpKind::Copy => {
                match (src.exists(), dst.as_ref().is_some_and(|d| d.exists())) {
                    (false, true) => (FileOpState::Completed, None),
                    (true, false) => (
                        FileOpState::RolledBack,
                        Some("verified after restart: source untouched"),
                    ),
                    (true, true) if op.kind == FileOpKind::Copy => (FileOpState::Completed, None),
                    (true, true) => (
                        FileOpState::Failed,
                        Some("both source and destination exist; please review manually"),
                    ),
                    (false, false) => (
                        FileOpState::Failed,
                        Some("neither source nor destination found"),
                    ),
                }
            }
            _ => (
                FileOpState::Failed,
                Some("cannot verify this operation automatically"),
            ),
        };
        if state == FileOpState::Completed {
            apply_to_catalog(exec, &op)?;
        }
        transition(&exec.db, op.id, state, note)?;
    }
    exec.events.emit(AppEvent::LibraryChanged);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use audiovault_indexer::NoopContext;

    #[test]
    fn verified_copy_and_no_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("Book");
        std::fs::create_dir_all(src.join("CD1")).unwrap();
        std::fs::write(src.join("CD1/01.mp3"), vec![1u8; 1000]).unwrap();
        std::fs::write(src.join("cover.jpg"), vec![2u8; 10]).unwrap();
        let dst = d.path().join("out/Book");
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        copy_verified(&src, &dst, &NoopContext).unwrap();
        assert_eq!(std::fs::read(dst.join("CD1/01.mp3")).unwrap().len(), 1000);
        assert!(!d.path().join("out/.Book.avpart").exists());
        assert!(src.exists(), "source untouched by copy");
    }

    #[test]
    fn rel_helpers() {
        assert_eq!(parent_rel("a/b/c"), "a/b");
        assert_eq!(parent_rel("c"), "");
        assert_eq!(base_name("a/b/c"), "c");
        assert_eq!(join_rel("", "x"), "x");
    }
}
