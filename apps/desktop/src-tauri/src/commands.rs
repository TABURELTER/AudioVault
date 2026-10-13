use std::path::{Path, PathBuf};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tauri::State;

use audiovault_core::model::{DuplicateStatus, IssueKind};
use audiovault_db::duplicates::count_open;
use audiovault_db::library::{LibraryFilter, LibrarySort};
use audiovault_platform::shell;
use audiovault_service::AppService;
use audiovault_torrent::engine::TorrentSource;

pub struct AppState {
    pub service: Arc<AppService>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookDto {
    pub id: i64,
    pub title: String,
    pub authors: String,
    pub narrators: String,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub duration_ms: i64,
    pub size: i64,
    pub format: String,
    pub copies_count: i64,
    pub online: bool,
    pub volume_name: String,
    pub cover_thumb: Option<String>,
    pub is_listened: bool,
    pub is_marked_for_deletion: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CopyDetailDto {
    pub id: i64,
    pub volume_name: String,
    pub volume_online: bool,
    pub rel_path: String,
    pub folder_name: Option<String>,
    pub full_path: Option<String>,
    pub format: String,
    pub codec: Option<String>,
    pub bitrate_kbps: Option<i64>,
    pub total_size: i64,
    pub duration_ms: i64,
    pub audio_file_count: i64,
    pub is_single_file: bool,
    pub quality_score: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaFileDetailDto {
    pub id: i64,
    pub file_name: String,
    pub rel_path: String,
    pub full_path: Option<String>,
    pub track_no: Option<i64>,
    pub duration_ms: Option<i64>,
    pub size: i64,
    pub bitrate_kbps: Option<i64>,
    pub title: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookDetailDto {
    pub id: i64,
    pub title: String,
    pub authors: String,
    pub narrators: String,
    pub series: Option<String>,
    pub series_index: Option<f64>,
    pub duration_ms: i64,
    pub description: Option<String>,
    pub cover_thumb: Option<String>,
    pub copies: Vec<CopyDetailDto>,
    pub media: Vec<MediaFileDetailDto>,
    pub is_listened: bool,
    pub is_marked_for_deletion: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeDto {
    pub id: i64,
    pub label: String,
    pub mount_path: String,
    pub online: bool,
    pub storage_class: String,
    pub fs_type: String,
    pub capacity_bytes: i64,
    pub free_bytes: i64,
    pub book_count: i64,
    pub roots_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CopySideDto {
    pub copy_id: i64,
    pub book_id: i64,
    pub volume_id: i64,
    pub volume_name: String,
    pub narrator: String,
    pub format: String,
    pub size: i64,
    pub duration_ms: i64,
    pub online: bool,
    pub rel_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DuplicatePairDto {
    pub id: i64,
    pub score: f64,
    pub verdict: String,
    pub evidence: String,
    pub title: String,
    pub authors: String,
    pub a: CopySideDto,
    pub b: CopySideDto,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IssueDto {
    pub id: i64,
    pub book_id: i64,
    pub copy_id: Option<i64>,
    pub kind: String,
    pub kind_label: String,
    pub book_title: String,
    pub book_authors: String,
    pub message: String,
    pub field: Option<String>,
    pub current_val: Option<String>,
    pub proposed_val: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OptimizerDto {
    pub copy_id: i64,
    pub book_title: String,
    pub authors: String,
    pub current_codec: String,
    pub current_bitrate_kbps: Option<i64>,
    pub current_size: i64,
    pub target_format: String,
    pub target_bitrate_kbps: Option<i64>,
    pub estimated_size: i64,
    pub savings_bytes: i64,
    pub savings_pct: i32,
    pub notes: String,
    pub reencode: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskDto {
    pub id: i64,
    pub kind: String,
    pub label: String,
    pub description: String,
    pub progress: f64,
    pub state: String,
    pub started_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TorrentResultDto {
    pub id: String,
    pub title: String,
    pub author: String,
    pub reader: String,
    pub size_bytes: i64,
    pub seeds: i32,
    pub leeches: i32,
    pub bitrate: String,
    pub category: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadDto {
    pub id: i64,
    pub name: String,
    pub state: String,
    pub progress: f64,
    pub total_bytes: i64,
    pub downloaded_bytes: i64,
    pub down_speed: String,
    pub up_speed: String,
    pub eta_str: String,
    pub seeds: i32,
    pub peers: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsDto {
    pub rutracker_username: String,
    pub rutracker_cookie: String,
    pub rutracker_proxy: String,
    pub rutracker_base_url: String,
    pub ffmpeg_path: String,
    pub ffmpeg_version: String,
    pub ffmpeg_ok: bool,
    pub theme: String,
    pub db_path: String,
    pub logs_path: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StatsDto {
    pub total_books: i64,
    pub total_copies: i64,
    pub total_duration_ms: i64,
    pub total_size_bytes: i64,
    pub online_volumes: i64,
    pub offline_volumes: i64,
    pub duplicates_count: i64,
    pub issues_count: i64,
    pub total_ebooks: i64,
}

#[tauri::command]
pub async fn get_library_books(
    state: State<'_, AppState>,
    filter: String,
    sort: String,
    search: String,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<BookDto>, String> {
    let f = match filter.as_str() {
        "online" => LibraryFilter::Online,
        "offline" => LibraryFilter::Offline,
        "multiple" => LibraryFilter::MultipleVariants,
        "duplicates" => LibraryFilter::Duplicates,
        "issues" => LibraryFilter::Issues,
        "listened" => LibraryFilter::Listened,
        "candidates" => LibraryFilter::Candidates,
        value if value.starts_with("volume:") => value[7..].parse::<i64>().map(LibraryFilter::Volume).unwrap_or(LibraryFilter::All),
        _ => LibraryFilter::All,
    };
    let s = match sort.as_str() {
        "title" | "title_asc" => LibrarySort::Title,
        "title_desc" => LibrarySort::TitleDesc,
        "author" | "author_asc" => LibrarySort::Author,
        "author_desc" => LibrarySort::AuthorDesc,
        "narrator" | "narrator_asc" => LibrarySort::Narrator,
        "narrator_desc" => LibrarySort::NarratorDesc,
        "duration" | "duration_desc" => LibrarySort::Duration,
        "duration_asc" => LibrarySort::DurationAsc,
        "size" | "size_desc" => LibrarySort::Size,
        "size_asc" => LibrarySort::SizeAsc,
        "format" | "format_asc" => LibrarySort::Format,
        "format_desc" => LibrarySort::FormatDesc,
        "volume" | "volume_asc" | "storage" | "storage_asc" => LibrarySort::Volume,
        "volume_desc" | "storage_desc" => LibrarySort::VolumeDesc,
        "recent" => LibrarySort::Recent,
        _ => LibrarySort::Author,
    };

    let lim = limit.unwrap_or(5000);
    let off = offset.unwrap_or(0);

    let items = state
        .service
        .library_page(f, s, off, lim)
        .map_err(|e| e.to_string())?;

    let q_lower = search.trim().to_lowercase();

    Ok(items
        .into_iter()
        .filter(|b| {
            if q_lower.is_empty() {
                true
            } else {
                b.title.to_lowercase().contains(&q_lower)
                    || b.authors.to_lowercase().contains(&q_lower)
                    || b.narrators.to_lowercase().contains(&q_lower)
            }
        })
        .map(|b| BookDto {
            id: b.book_id,
            title: b.title,
            authors: b.authors,
            narrators: b.narrators,
            series: b.series,
            series_index: b.series_index,
            duration_ms: b.duration_ms as i64,
            size: b.size as i64,
            format: b.format,
            copies_count: b.copy_count,
            online: b.any_online,
            volume_name: b.volume_name.unwrap_or_else(|| "Архив".into()),
            cover_thumb: b.cover_thumb,
            is_listened: b.is_listened,
            is_marked_for_deletion: b.is_marked_for_deletion,
        })
        .collect())
}

#[tauri::command]
pub async fn get_book_details(
    state: State<'_, AppState>,
    book_id: i64,
) -> Result<Option<BookDetailDto>, String> {
    let detail = state
        .service
        .book_details(book_id)
        .map_err(|e| e.to_string())?;

    let Some(d) = detail else { return Ok(None) };

    let copies: Vec<CopyDetailDto> = d
        .copies
        .iter()
        .map(|c| {
            let full = if c.brief.online {
                Some(c.brief.display_path())
            } else {
                c.brief.mount_path.as_ref().map(|m| {
                    audiovault_core::paths::from_db_rel(Path::new(m), &c.brief.rel_path)
                        .display()
                        .to_string()
                })
            };
            let folder_stem = c.brief.rel_path.split(['/', '\\']).filter(|s| !s.is_empty()).last().unwrap_or(&c.brief.rel_path);
            let (tr_folder, was_tr) = audiovault_core::translit::auto_reverse_translit(folder_stem);
            let folder_name = if was_tr { Some(tr_folder) } else { None };
            CopyDetailDto {
                id: c.brief.copy_id,
                volume_name: c.brief.volume_name.clone(),
                volume_online: c.brief.online,
                rel_path: c.brief.rel_path.clone(),
                folder_name,
                full_path: full,
                format: c.brief.codec.clone().unwrap_or_else(|| "MP3".into()).to_uppercase(),
                codec: c.brief.codec.clone(),
                bitrate_kbps: c.brief.bitrate_kbps.map(|b| b as i64),
                total_size: c.brief.total_size as i64,
                duration_ms: c.brief.duration_ms as i64,
                audio_file_count: c.audio_file_count,
                is_single_file: c.is_single_file,
                quality_score: c.quality_score.unwrap_or(80.0) as i64,
            }
        })
        .collect();

    let first_copy = d.copies.first();
    let first_copy_id = first_copy.map(|c| c.brief.copy_id).unwrap_or(0);
    let copy_mount = first_copy.and_then(|c| c.brief.mount_path.as_ref());
    let copy_rel = first_copy.map(|c| c.brief.rel_path.as_str()).unwrap_or("");

    let media: Vec<MediaFileDetailDto> = if first_copy_id > 0 {
        let conn = state.service.db().conn().map_err(|e| e.to_string())?;
        audiovault_db::catalog::media_for_copy(&conn, first_copy_id)
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                let full = copy_mount.map(|mnt| {
                    let rel_clean = copy_rel.trim_matches(['/', '\\']);
                    let sub = if rel_clean.is_empty() {
                        m.file_name.clone()
                    } else {
                        format!("{}/{}", rel_clean, m.file_name)
                    };
                    audiovault_core::paths::from_db_rel(Path::new(mnt), &sub)
                        .display()
                        .to_string()
                });
                let display_title = m.tag_title.or_else(|| {
                    let stem = m.file_name.rsplit_once('.').map_or(m.file_name.as_str(), |(s, _)| s);
                    let (tr, was_tr) = audiovault_core::translit::auto_reverse_translit(stem);
                    if was_tr { Some(tr) } else { None }
                });
                MediaFileDetailDto {
                    id: m.id,
                    file_name: m.file_name,
                    rel_path: m.rel_path,
                    full_path: full,
                    track_no: m.track_no.map(|t| t as i64),
                    duration_ms: m.duration_ms.map(|d| d as i64),
                    size: m.size as i64,
                    bitrate_kbps: m.bitrate_kbps.map(|b| b as i64),
                    title: display_title,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    let (is_listened, is_marked_for_deletion): (i64, i64) = conn
        .query_row(
            "SELECT COALESCE(is_listened, 0), COALESCE(is_marked_for_deletion, 0) FROM book WHERE id = ?1",
            rusqlite::params![book_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 0));

    Ok(Some(BookDetailDto {
        id: d.id,
        title: d.title,
        authors: d.authors,
        narrators: d.narrators,
        series: d.series,
        series_index: d.series_index,
        duration_ms: d.duration_ms as i64,
        description: d.description,
        cover_thumb: d.cover_thumb,
        copies,
        media,
        is_listened: is_listened != 0,
        is_marked_for_deletion: is_marked_for_deletion != 0,
    }))
}

#[tauri::command]
pub async fn get_volumes(state: State<'_, AppState>) -> Result<Vec<VolumeDto>, String> {
    let summaries = state
        .service
        .volume_summaries()
        .map_err(|e| e.to_string())?;

    Ok(summaries
        .into_iter()
        .map(|v| VolumeDto {
            id: v.volume.id,
            label: v.volume.display_name,
            mount_path: v.volume.last_mount_path.unwrap_or_else(|| "Отключен".into()),
            online: v.volume.is_online,
            storage_class: v.volume.storage_class.as_str().into(),
            fs_type: v.volume.fs_type.unwrap_or_else(|| "NTFS".into()),
            capacity_bytes: v.volume.capacity_bytes.unwrap_or(0) as i64,
            free_bytes: v.volume.free_bytes.unwrap_or(0) as i64,
            book_count: v.book_count,
            roots_count: v.root_count,
        })
        .collect())
}

#[tauri::command]
pub async fn add_storage_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<bool, String> {
    state
        .service
        .add_storage_folder(Path::new(&path), true)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn rename_volume(
    state: State<'_, AppState>,
    volume_id: i64,
    new_name: String,
) -> Result<bool, String> {
    state
        .service
        .rename_volume(volume_id, &new_name)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reset_catalog(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .service
        .reset_catalog()
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pick_folder() -> Result<Option<String>, String> {
    let folder = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Выберите папку с аудиокнигами")
            .pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(folder.map(|f| f.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn scan_volume(
    state: State<'_, AppState>,
    volume_id: i64,
) -> Result<i64, String> {
    state
        .service
        .scan_volume(volume_id)
        .map(|id| id as i64)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_all(state: State<'_, AppState>) -> Result<Vec<i64>, String> {
    let vols = state
        .service
        .volume_summaries()
        .map_err(|e| e.to_string())?;
    let mut ids = Vec::new();
    for v in vols {
        if v.volume.is_online {
            if let Ok(job_id) = state.service.scan_volume(v.volume.id) {
                ids.push(job_id as i64);
            }
        }
    }
    Ok(ids)
}

#[tauri::command]
pub async fn search_books(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<BookDto>, String> {
    let hits = state
        .service
        .search(&query, 50)
        .map_err(|e| e.to_string())?;

    Ok(hits
        .into_iter()
        .map(|h| {
            let online = h.copies.iter().any(|c| c.online);
            let vol_name = h
                .copies
                .first()
                .map(|c| c.volume_name.clone())
                .unwrap_or_default();
            BookDto {
                id: h.book_id,
                title: h.title,
                authors: h.authors,
                narrators: h.narrators,
                series: h.series,
                series_index: None,
                duration_ms: h.duration_ms as i64,
                size: h.size as i64,
                format: h.format,
                copies_count: h.copies.len() as i64,
                online,
                volume_name: vol_name,
                cover_thumb: h.cover_thumb,
                is_listened: false,
                is_marked_for_deletion: false,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn get_duplicates(
    state: State<'_, AppState>,
) -> Result<Vec<DuplicatePairDto>, String> {
    let pairs = state
        .service
        .duplicate_pairs(true, 0, 100)
        .map_err(|e| e.to_string())?;

    Ok(pairs
        .into_iter()
        .map(|p| {
            let ev_str = p
                .evidence
                .iter()
                .map(|e| e.text.clone())
                .collect::<Vec<_>>()
                .join(", ");
            DuplicatePairDto {
                id: p.id,
                score: p.score,
                verdict: p.verdict.label().into(),
                evidence: ev_str,
                title: p.a.title,
                authors: p.a.authors,
                a: CopySideDto {
                    copy_id: p.a.copy_id,
                    book_id: p.a.book_id,
                    volume_id: p.a.volume_id,
                    volume_name: p.a.volume_name,
                    narrator: p.a.narrator,
                    format: p.a.codec.unwrap_or_else(|| "MP3".into()).to_uppercase(),
                    size: p.a.total_size,
                    duration_ms: p.a.duration_ms,
                    online: p.a.volume_online,
                    rel_path: p.a.rel_path,
                },
                b: CopySideDto {
                    copy_id: p.b.copy_id,
                    book_id: p.b.book_id,
                    volume_id: p.b.volume_id,
                    volume_name: p.b.volume_name,
                    narrator: p.b.narrator,
                    format: p.b.codec.unwrap_or_else(|| "MP3".into()).to_uppercase(),
                    size: p.b.total_size,
                    duration_ms: p.b.duration_ms,
                    online: p.b.volume_online,
                    rel_path: p.b.rel_path,
                },
            }
        })
        .collect())
}

#[tauri::command]
pub async fn find_all_duplicates(state: State<'_, AppState>) -> Result<i64, String> {
    state
        .service
        .find_all_duplicates()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_duplicate(
    state: State<'_, AppState>,
    dup_id: i64,
    status: String,
) -> Result<bool, String> {
    let st = match status.as_str() {
        "keep_a" => DuplicateStatus::KeptA,
        "keep_b" => DuplicateStatus::KeptB,
        "keep_both" => DuplicateStatus::KeptBoth,
        "mark_different" => DuplicateStatus::DifferentVariants,
        _ => DuplicateStatus::Ignored,
    };
    state
        .service
        .resolve_duplicate(dup_id, st)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_issues(
    state: State<'_, AppState>,
    kind: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<IssueDto>, String> {
    let list = state
        .service
        .issues(
            kind.and_then(|value| IssueKind::parse(&value)),
            offset.unwrap_or(0).max(0),
            limit.unwrap_or(100).clamp(1, 250),
        )
        .map_err(|e| e.to_string())?;

    Ok(list
        .into_iter()
        .map(|i| IssueDto {
            id: i.id,
            book_id: i.book_id,
            copy_id: i.copy_id,
            kind: i.kind.as_str().into(),
            kind_label: i.kind.label().into(),
            book_title: i.book_title,
            book_authors: i.book_authors,
            message: i.message,
            field: i.field,
            current_val: i.current_value,
            proposed_val: i.proposed_value,
        })
        .collect())
}

#[tauri::command]
pub async fn ignore_issue(
    state: State<'_, AppState>,
    issue_id: i64,
) -> Result<bool, String> {
    state
        .service
        .ignore_issue(issue_id)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_optimizations(
    state: State<'_, AppState>,
) -> Result<Vec<OptimizerDto>, String> {
    let items = state
        .service
        .optimizations(&["candidate", "suggested"], None, 0, 100)
        .map_err(|e| e.to_string())?;

    Ok(items
        .into_iter()
        .map(|r| {
            let savings = r.savings();
            let pct = if r.current_size > 0 {
                (savings as f64 / r.current_size as f64 * 100.0) as i32
            } else {
                0
            };
            OptimizerDto {
                copy_id: r.copy_id,
                book_title: r.title,
                authors: r.authors,
                current_codec: r.current_codec,
                current_bitrate_kbps: r.current_bitrate_kbps,
                current_size: r.current_size,
                target_format: r.target.label().into(),
                target_bitrate_kbps: r.target_bitrate_kbps,
                estimated_size: r.estimated_size,
                savings_bytes: savings,
                savings_pct: pct,
                notes: r.notes,
                reencode: r.reencode,
            }
        })
        .collect())
}

#[tauri::command]
pub async fn start_transcode(
    state: State<'_, AppState>,
    opt_id: i64,
) -> Result<i64, String> {
    state
        .service
        .start_transcode(opt_id, true, None)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn transcode_copy(
    state: State<'_, AppState>,
    copy_id: i64,
    format: Option<String>,
) -> Result<i64, String> {
    let target = match format.as_deref() {
        Some("mp3") => audiovault_core::optimize::TargetFormat::Mp3,
        Some("opus") => audiovault_core::optimize::TargetFormat::Opus,
        _ => audiovault_core::optimize::TargetFormat::M4bAac,
    };
    state
        .service
        .transcode_copy(copy_id, target, false)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_tasks(state: State<'_, AppState>) -> Result<Vec<TaskDto>, String> {
    let jobs = state
        .service
        .jobs(50)
        .map_err(|e| e.to_string())?;

    Ok(jobs
        .into_iter()
        .map(|j| TaskDto {
            id: j.id,
            kind: j.kind.as_str().into(),
            label: j.title,
            description: format!("Состояние: {}, попыток: {}", j.state.as_str(), j.attempts),
            progress: j.progress / 100.0,
            state: j.state.as_str().into(),
            started_at: None,
        })
        .collect())
}

#[tauri::command]
pub async fn cancel_task(
    state: State<'_, AppState>,
    task_id: i64,
) -> Result<bool, String> {
    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    audiovault_db::jobs::request_cancel(&conn, task_id).map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn search_torrents(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<TorrentResultDto>, String> {
    let results = state
        .service
        .torrents()
        .search_rutracker(&query)
        .await
        .map_err(|e| e.to_string())?;

    Ok(results
        .into_iter()
        .map(|r| TorrentResultDto {
            id: r.topic_id,
            title: r.title,
            author: String::new(),
            reader: String::new(),
            size_bytes: r.size_bytes as i64,
            seeds: r.seeders as i32,
            leeches: r.leechers as i32,
            bitrate: String::new(),
            category: r.category,
        })
        .collect())
}

#[tauri::command]
pub async fn download_torrent(
    state: State<'_, AppState>,
    topic_id: String,
) -> Result<bool, String> {
    let vols = state
        .service
        .volume_summaries()
        .map_err(|e| e.to_string())?;
    let Some(v) = vols.iter().find(|v| v.volume.is_online) else {
        return Err("Нет подключенных онлайн томов".into());
    };

    let bytes = state
        .service
        .torrents()
        .fetch_rutracker_torrent(&topic_id)
        .await
        .map_err(|e| e.to_string())?;

    let source = TorrentSource::TorrentFile(bytes);
    state
        .service
        .torrents()
        .add(&source, v.volume.id, "Downloads", None, false)
        .await
        .map_err(|e| e.to_string())?;

    Ok(true)
}

#[tauri::command]
pub async fn get_downloads(state: State<'_, AppState>) -> Result<Vec<DownloadDto>, String> {
    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    let transfers = audiovault_db::torrents::list(&conn).map_err(|e| e.to_string())?;

    Ok(transfers
        .into_iter()
        .map(|t| DownloadDto {
            id: t.id,
            name: t.name,
            state: t.state.as_str().into(),
            progress: t.progress,
            total_bytes: t.total_bytes,
            downloaded_bytes: t.downloaded_bytes,
            down_speed: "Активно".into(),
            up_speed: "0 KB/s".into(),
            eta_str: if t.state == audiovault_core::model::TorrentState::Completed {
                "Готово".into()
            } else {
                "Загрузка".into()
            },
            seeds: 0,
            peers: 0,
        })
        .collect())
}

#[tauri::command]
pub async fn pause_download(
    state: State<'_, AppState>,
    id: i64,
) -> Result<bool, String> {
    state
        .service
        .torrents()
        .pause(id)
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume_download(
    state: State<'_, AppState>,
    id: i64,
) -> Result<bool, String> {
    state
        .service
        .torrents()
        .resume(id)
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_download(
    state: State<'_, AppState>,
    id: i64,
) -> Result<bool, String> {
    state
        .service
        .torrents()
        .remove(id, false, true)
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<SettingsDto, String> {
    let s = state.service.settings();
    let (ok, ver) = if !s.ffmpeg_path.is_empty() {
        match audiovault_media::transcode::ffmpeg_version(Path::new(&s.ffmpeg_path)) {
            Ok(v) => (true, v),
            Err(_) => (false, "Ошибка запуска".into()),
        }
    } else {
        (false, "Не найден".into())
    };

    Ok(SettingsDto {
        rutracker_username: s.rutracker_username,
        rutracker_cookie: s.rutracker_cookie,
        rutracker_proxy: s.rutracker_proxy,
        rutracker_base_url: s.rutracker_base_url,
        ffmpeg_path: s.ffmpeg_path,
        ffmpeg_version: ver,
        ffmpeg_ok: ok,
        theme: s.theme,
        db_path: state.service.paths().catalog_path().display().to_string(),
        logs_path: state.service.paths().log_dir.display().to_string(),
    })
}

#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    settings: SettingsDto,
) -> Result<bool, String> {
    let mut s = state.service.settings();
    s.rutracker_username = settings.rutracker_username;
    s.rutracker_cookie = settings.rutracker_cookie;
    s.rutracker_proxy = settings.rutracker_proxy;
    if !settings.rutracker_base_url.trim().is_empty() {
        s.rutracker_base_url = settings.rutracker_base_url;
    }
    s.ffmpeg_path = settings.ffmpeg_path;
    s.theme = settings.theme;
    state
        .service
        .update_settings(s)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_rutracker_connection(
    state: State<'_, AppState>,
) -> Result<String, String> {
    state
        .service
        .torrents()
        .check_rutracker_session()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_credentials(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<bool, String> {
    state
        .service
        .torrents()
        .login_rutracker(&username, &password)
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_stats(state: State<'_, AppState>) -> Result<StatsDto, String> {
    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    let books: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM book WHERE (is_listened = 0 OR is_listened IS NULL) AND (is_marked_for_deletion = 0 OR is_marked_for_deletion IS NULL)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let copies: i64 = conn
        .query_row("SELECT COUNT(*) FROM book_copy", [], |r| r.get(0))
        .unwrap_or(0);
    let duration: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(duration_ms), 0) FROM book_copy WHERE state = 'present'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let size: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_size), 0) FROM book_copy WHERE state = 'present'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let online_vols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM storage_volume WHERE is_online = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let offline_vols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM storage_volume WHERE is_online = 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let (dups, _) = count_open(&conn).unwrap_or((0, 0));
    let issues: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM metadata_issue WHERE state = 'open'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let ebooks: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ebook_entry WHERE state = 'present' AND (is_read = 0 OR is_read IS NULL) AND (is_marked_for_deletion = 0 OR is_marked_for_deletion IS NULL)",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Ok(StatsDto {
        total_books: books,
        total_copies: copies,
        total_duration_ms: duration,
        total_size_bytes: size,
        online_volumes: online_vols,
        offline_volumes: offline_vols,
        duplicates_count: dups,
        issues_count: issues,
        total_ebooks: ebooks,
    })
}

#[tauri::command]
pub async fn reveal_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if p.exists() {
        shell::reveal(&p).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if let Ok(vols) = state.service.volume_summaries() {
        for v in vols {
            if v.volume.is_online {
                if let Some(mount) = &v.volume.last_mount_path {
                    let candidate = audiovault_core::paths::from_db_rel(Path::new(mount), &path);
                    if candidate.exists() {
                        shell::reveal(&candidate).map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
            }
        }
    }

    Err(format!("Папка не найдена или том отключен: {path}"))
}

#[tauri::command]
pub async fn open_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if p.exists() {
        shell::open_file(&p).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if let Ok(vols) = state.service.volume_summaries() {
        for v in vols {
            if v.volume.is_online {
                if let Some(mount) = &v.volume.last_mount_path {
                    let candidate = audiovault_core::paths::from_db_rel(Path::new(mount), &path);
                    if candidate.exists() {
                        shell::open_file(&candidate).map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
            }
        }
    }

    Err(format!("Файл не найден или том отключен: {path}"))
}

#[tauri::command]
pub async fn get_cover_data(
    state: State<'_, AppState>,
    path: String,
) -> Result<Option<String>, String> {
    let p = if Path::new(&path).is_absolute() {
        PathBuf::from(&path)
    } else {
        let direct = state.service.paths().cover_cache_dir.join(&path);
        if direct.exists() {
            direct
        } else if path.len() >= 2 {
            state.service.paths().cover_cache_dir.join(&path[..2]).join(format!("{path}.jpg"))
        } else {
            direct
        }
    };
    if !p.exists() {
        return Ok(None);
    }
    match std::fs::read(&p) {
        Ok(bytes) => {
            const B64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let mut result = String::with_capacity(bytes.len() * 4 / 3 + 4 + 30);
            result.push_str("data:image/jpeg;base64,");
            for chunk in bytes.chunks(3) {
                let b0 = chunk[0];
                let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
                let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
                result.push(B64_CHARS[(b0 >> 2) as usize] as char);
                result.push(B64_CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
                if chunk.len() > 1 {
                    result.push(B64_CHARS[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
                } else {
                    result.push('=');
                }
                if chunk.len() > 2 {
                    result.push(B64_CHARS[(b2 & 0x3F) as usize] as char);
                } else {
                    result.push('=');
                }
            }
            Ok(Some(result))
        }
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn search_ebooks(
    state: State<'_, AppState>,
    query: Option<String>,
    format_filter: Option<String>,
    sort_by: Option<String>,
    read_filter: Option<bool>,
    candidate_filter: Option<bool>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<audiovault_db::ebooks::EbookSearchResult, String> {
    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    audiovault_db::ebooks::search_ebooks(
        &conn,
        query.as_deref(),
        format_filter.as_deref(),
        sort_by.as_deref(),
        read_filter,
        candidate_filter,
        limit.unwrap_or(60),
        offset.unwrap_or(0),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_ebook_detail(
    state: State<'_, AppState>,
    ebook_id: i64,
) -> Result<Option<audiovault_db::ebooks::EbookEntry>, String> {
    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    audiovault_db::ebooks::get_ebook(&conn, ebook_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_ebook_stats(
    state: State<'_, AppState>,
) -> Result<audiovault_db::ebooks::EbookStats, String> {
    let conn = state.service.db().conn().map_err(|e| e.to_string())?;
    audiovault_db::ebooks::get_ebook_stats(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_book_listened(
    state: State<'_, AppState>,
    book_id: i64,
    listened: bool,
) -> Result<bool, String> {
    state.service.set_book_listened(book_id, listened).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_ebook_read(
    state: State<'_, AppState>,
    ebook_id: i64,
    read: bool,
) -> Result<bool, String> {
    state.service.set_ebook_read(ebook_id, read).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_book_marked_for_deletion(
    state: State<'_, AppState>,
    book_id: i64,
    marked: bool,
) -> Result<bool, String> {
    state.service.set_book_marked_for_deletion(book_id, marked).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_ebook_marked_for_deletion(
    state: State<'_, AppState>,
    ebook_id: i64,
    marked: bool,
) -> Result<bool, String> {
    state.service.set_ebook_marked_for_deletion(ebook_id, marked).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_multiple_books_marked_for_deletion(
    state: State<'_, AppState>,
    book_ids: Vec<i64>,
    marked: bool,
) -> Result<bool, String> {
    state.service.set_multiple_books_marked_for_deletion(&book_ids, marked).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_multiple_ebooks_marked_for_deletion(
    state: State<'_, AppState>,
    ebook_ids: Vec<i64>,
    marked: bool,
) -> Result<bool, String> {
    state.service.set_multiple_ebooks_marked_for_deletion(&ebook_ids, marked).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn execute_deletion(
    state: State<'_, AppState>,
    book_ids: Vec<i64>,
    ebook_ids: Vec<i64>,
) -> Result<audiovault_service::app::DeletionReport, String> {
    state.service.execute_deletion(&book_ids, &ebook_ids).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn execute_pending_deletions_for_volume(
    state: State<'_, AppState>,
    volume_id: i64,
) -> Result<usize, String> {
    state.service.execute_pending_deletions_for_volume(volume_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_deletion_history(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<audiovault_db::deletion::DeletionHistoryEntry>, String> {
    state.service.deletion_history(limit.unwrap_or(50), offset.unwrap_or(0)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_deletion_history(
    state: State<'_, AppState>,
) -> Result<bool, String> {
    state.service.clear_deletion_history().map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_pending_deletions(
    state: State<'_, AppState>,
) -> Result<Vec<audiovault_db::deletion::PendingDeletion>, String> {
    state.service.pending_deletions().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_deletion_counts(
    state: State<'_, AppState>,
) -> Result<audiovault_db::deletion::DeletionCounts, String> {
    state.service.deletion_counts().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ignore_issues(
    state: State<'_, AppState>,
    issue_ids: Vec<i64>,
) -> Result<bool, String> {
    state.service.ignore_issues(&issue_ids).map(|_| true).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ignore_issues_by_kind(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> Result<usize, String> {
    let k = kind.as_deref().and_then(IssueKind::parse);
    state.service.ignore_issues_by_kind(k).map_err(|e| e.to_string())
}
