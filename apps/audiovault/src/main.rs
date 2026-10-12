//! AudioVault native desktop application entry point.
//!
//! Integrates Slint UI, Tokio async runtime, and the AppService facade
//! managing offline storage, SQLite WAL catalog, Chromaprint acoustic fingerprinting,
//! embedded BitTorrent engine, and background thread pools.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use audiovault_core::model::{DuplicateStatus, JobState};
use audiovault_db::library::{LibraryFilter, LibrarySort};
use audiovault_platform::instance::SingleInstance;
use audiovault_platform::paths::AppPaths;
use audiovault_platform::shell;
use audiovault_service::app::AppService;
use audiovault_service::events::AppEvent;
use audiovault_torrent::engine::TorrentSource;
use slint::{ComponentHandle, ModelRc, VecModel};
use tracing_subscriber::prelude::*;

slint::include_modules!();

fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours} ч {mins:02} мин")
    } else {
        format!("{mins}:{secs:02}")
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{bytes} B")
    }
}

fn load_stats(service: &Arc<AppService>, window: &MainWindow) {
    if let Ok(st) = service.catalog_stats() {
        let tasks = service.jobs(50).unwrap_or_default();
        let active_tasks = tasks
            .iter()
            .filter(|j| matches!(j.state, JobState::Running | JobState::Queued))
            .count() as i32;

        window.set_stats(GlobalStats {
            total_books: st.books as i32,
            total_copies: st.copies as i32,
            total_volumes: st.volumes as i32,
            online_volumes: st.online_volumes as i32,
            total_duration_hours: 0,
            total_size_gb: (st.total_bytes / (1024 * 1024 * 1024)) as i32,
            duplicates_count: st.open_duplicates as i32,
            issues_count: st.open_issues as i32,
            active_downloads: 0,
            active_tasks,
        });
    }
}

fn load_library(service: &Arc<AppService>, window: &MainWindow, filter: LibraryFilter, sort: LibrarySort, query: &str) {
    let rows = service.library_page(filter, sort, 0, 500).unwrap_or_default();
    let q_lower = query.trim().to_lowercase();

    let items: Vec<BookItem> = rows
        .into_iter()
        .filter(|r| {
            if q_lower.is_empty() {
                true
            } else {
                r.title.to_lowercase().contains(&q_lower)
                    || r.authors.to_lowercase().contains(&q_lower)
                    || r.narrators.to_lowercase().contains(&q_lower)
            }
        })
        .map(|r| BookItem {
            id: r.book_id as i32,
            title: r.title.into(),
            authors: r.authors.into(),
            narrator: r.narrators.into(),
            series: r.series.unwrap_or_default().into(),
            series_index: r.series_index.map(|idx| format!("{idx}")).unwrap_or_default().into(),
            duration_str: format_duration(r.duration_ms).into(),
            format_badge: r.format.to_uppercase().into(),
            quality_score: 85,
            volume_name: r.volume_name.unwrap_or_else(|| "Архив".into()).into(),
            volume_online: r.any_online,
            copy_count: r.copy_count as i32,
            total_size_str: format_bytes(r.size).into(),
        })
        .collect();

    window.set_library_books(ModelRc::new(VecModel::from(items)));
}

fn load_volumes(service: &Arc<AppService>, window: &MainWindow) {
    let summaries = service.volume_summaries().unwrap_or_default();
    let items: Vec<VolumeItem> = summaries
        .into_iter()
        .map(|v| {
            let cap = v.volume.capacity_bytes.unwrap_or(0);
            let free = v.volume.free_bytes.unwrap_or(0);
            let used = cap.saturating_sub(free);
            let pct = if cap > 0 { (used as f32) / (cap as f32) } else { 0.0 };

            VolumeItem {
                id: v.volume.id as i32,
                label: v.volume.display_name.into(),
                mount_path: v.volume.last_mount_path.unwrap_or_else(|| "Отключен".into()).into(),
                online: v.volume.is_online,
                storage_class: v.volume.storage_class.as_str().into(),
                fs_type: v.volume.fs_type.unwrap_or_else(|| "NTFS".into()).into(),
                capacity_str: format_bytes(cap).into(),
                free_str: format_bytes(free).into(),
                used_pct: pct,
                book_count: v.book_count as i32,
                roots_count: v.root_count as i32,
            }
        })
        .collect();

    window.set_volumes(ModelRc::new(VecModel::from(items)));
}

fn load_duplicates(service: &Arc<AppService>, window: &MainWindow) {
    let pairs = service.duplicate_pairs(true, 0, 100).unwrap_or_default();
    let items: Vec<DuplicateItem> = pairs
        .into_iter()
        .map(|p| {
            let ev_str = p.evidence.iter().map(|e| e.text.clone()).collect::<Vec<_>>().join(", ");
            DuplicateItem {
                id: p.id as i32,
                copy_a_id: p.a.copy_id as i32,
                copy_b_id: p.b.copy_id as i32,
                title: p.a.title.into(),
                authors: p.a.authors.into(),
                verdict: p.verdict.label().into(),
                score: (p.score * 100.0) as i32,
                evidence: ev_str.into(),
                a_volume: p.a.volume_name.into(),
                a_narrator: p.a.narrator.into(),
                a_format: p.a.codec.unwrap_or_else(|| "MP3".into()).to_uppercase().into(),
                a_size_str: format_bytes(p.a.total_size as u64).into(),
                a_duration_str: format_duration(p.a.duration_ms as u64).into(),
                a_online: p.a.volume_online,
                b_volume: p.b.volume_name.into(),
                b_narrator: p.b.narrator.into(),
                b_format: p.b.codec.unwrap_or_else(|| "MP3".into()).to_uppercase().into(),
                b_size_str: format_bytes(p.b.total_size as u64).into(),
                b_duration_str: format_duration(p.b.duration_ms as u64).into(),
                b_online: p.b.volume_online,
            }
        })
        .collect();

    window.set_duplicates(ModelRc::new(VecModel::from(items)));
}

fn load_issues(service: &Arc<AppService>, window: &MainWindow) {
    let rows = service.issues(None, 0, 100).unwrap_or_default();
    let items: Vec<IssueItem> = rows
        .into_iter()
        .map(|i| IssueItem {
            id: i.id as i32,
            book_id: i.book_id as i32,
            copy_id: i.copy_id.unwrap_or(0) as i32,
            kind: i.kind.as_str().into(),
            kind_label: i.kind.label().into(),
            book_title: i.book_title.into(),
            book_authors: i.book_authors.into(),
            message: i.message.into(),
            field: i.field.unwrap_or_default().into(),
            current_val: i.current_value.unwrap_or_default().into(),
            proposed_val: i.proposed_value.unwrap_or_default().into(),
        })
        .collect();

    window.set_issues(ModelRc::new(VecModel::from(items)));
}

fn load_optimizer(service: &Arc<AppService>, window: &MainWindow) {
    let recs = service.optimizations(&["candidate", "suggested"], None, 0, 100).unwrap_or_default();
    let items: Vec<OptimizerItem> = recs
        .into_iter()
        .map(|r| {
            let savings = r.savings();
            let pct = if r.current_size > 0 {
                (savings as f64 / r.current_size as f64 * 100.0) as i32
            } else {
                0
            };

            OptimizerItem {
                copy_id: r.copy_id as i32,
                book_title: r.title.into(),
                authors: r.authors.into(),
                current_codec: r.current_codec.into(),
                current_bitrate_str: r.current_bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_default().into(),
                current_size_str: format_bytes(r.current_size.max(0) as u64).into(),
                target_format: r.target.label().into(),
                target_bitrate_str: r.target_bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_default().into(),
                estimated_size_str: format_bytes(r.estimated_size.max(0) as u64).into(),
                savings_str: format_bytes(savings.max(0) as u64).into(),
                savings_pct: pct,
                notes: r.notes.into(),
                reencode: r.reencode,
            }
        })
        .collect();

    window.set_optimizer_items(ModelRc::new(VecModel::from(items)));
}

fn load_tasks(service: &Arc<AppService>, window: &MainWindow) {
    let jobs = service.jobs(50).unwrap_or_default();
    let items: Vec<TaskItem> = jobs
        .into_iter()
        .map(|j| {
            let pct = (j.progress as f32) / 100.0;
            TaskItem {
                id: j.id as i32,
                kind: j.kind.as_str().into(),
                label: j.title.into(),
                description: format!("Состояние: {}, попыток: {}", j.state.as_str(), j.attempts).into(),
                progress_pct: pct,
                state: j.state.as_str().into(),
                started_at: "".into(),
            }
        })
        .collect();

    window.set_tasks(ModelRc::new(VecModel::from(items)));
}

fn load_downloads(service: &Arc<AppService>, window: &MainWindow) {
    let transfers = {
        let conn = service.db().conn().unwrap();
        audiovault_db::torrents::list(&conn).unwrap_or_default()
    };

    let items: Vec<DownloadItem> = transfers
        .into_iter()
        .map(|t| {
            DownloadItem {
                id: t.id as i32,
                name: t.name.into(),
                state: t.state.as_str().into(),
                progress_pct: t.progress as f32,
                size_str: format_bytes(t.total_bytes.max(0) as u64).into(),
                downloaded_str: format_bytes(t.downloaded_bytes.max(0) as u64).into(),
                down_speed_str: "Активно".into(),
                up_speed_str: "—".into(),
                eta_str: if t.state == audiovault_core::model::TorrentState::Completed { "Готово".into() } else { "Загрузка".into() },
                seeds: 0,
                peers: 0,
            }
        })
        .collect();

    window.set_downloads(ModelRc::new(VecModel::from(items)));
    window.set_total_down_speed("В сети".into());
    window.set_total_up_speed("0 KB/s".into());
}

fn load_settings(service: &Arc<AppService>, window: &MainWindow) {
    let s = service.settings();
    window.set_rutracker_user(s.rutracker_username.into());
    window.set_ffmpeg_path(s.ffmpeg_path.clone().into());
    window.set_db_path(service.paths().catalog_path().display().to_string().into());
    window.set_logs_path(service.paths().log_dir.display().to_string().into());

    let (ok, ver) = if !s.ffmpeg_path.is_empty() {
        match audiovault_media::transcode::ffmpeg_version(Path::new(&s.ffmpeg_path)) {
            Ok(v) => (true, v),
            Err(_) => (false, "Ошибка запуска".into()),
        }
    } else {
        (false, "Не найден".into())
    };
    window.set_ffmpeg_ok(ok);
    window.set_ffmpeg_version(ver.into());
}

fn open_book_details(service: &Arc<AppService>, window: &MainWindow, book_id: i64) {
    if let Ok(Some(d)) = service.book_details(book_id) {
        let book_item = BookItem {
            id: d.id as i32,
            title: d.title.clone().into(),
            authors: d.authors.clone().into(),
            narrator: d.narrators.clone().into(),
            series: d.series.clone().unwrap_or_default().into(),
            series_index: d.series_index.map(|idx| format!("{idx}")).unwrap_or_default().into(),
            duration_str: format_duration(d.duration_ms).into(),
            format_badge: d.format.to_uppercase().into(),
            quality_score: 88,
            volume_name: "".into(),
            volume_online: true,
            copy_count: d.copies.len() as i32,
            total_size_str: format_bytes(d.size).into(),
        };

        let copy_items: Vec<CopyItem> = d
            .copies
            .iter()
            .map(|c| CopyItem {
                id: c.brief.copy_id as i32,
                volume_name: c.brief.volume_name.clone().into(),
                volume_online: c.brief.online,
                mount_path: c.brief.mount_path.clone().unwrap_or_default().into(),
                rel_path: c.brief.rel_path.clone().into(),
                state: c.brief.state.clone().into(),
                codec: c.brief.codec.clone().unwrap_or_else(|| "MP3".into()).to_uppercase().into(),
                bitrate_str: c.brief.bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_default().into(),
                sample_rate_str: c.sample_rate.map(|s| format!("{s} Hz")).unwrap_or_default().into(),
                channels: c.channels.unwrap_or(2) as i32,
                duration_str: format_duration(c.brief.duration_ms).into(),
                size_str: format_bytes(c.brief.total_size).into(),
                files_count: c.audio_file_count as i32,
                has_chapters: c.has_chapters,
                has_cover: c.has_cover,
                quality_score: c.quality_score.unwrap_or(80.0) as i32,
            })
            .collect();

        // Get media files for first copy if present
        let first_copy_id = d.copies.first().map(|c| c.brief.copy_id).unwrap_or(0);
        let media_items = if first_copy_id > 0 {
            let conn = service.db().conn().unwrap();
            audiovault_db::catalog::media_for_copy(&conn, first_copy_id)
                .unwrap_or_default()
                .into_iter()
                .map(|m| MediaItem {
                    id: m.id as i32,
                    file_name: m.file_name.into(),
                    rel_path: m.rel_path.into(),
                    duration_str: format_duration(m.duration_ms.unwrap_or(0)).into(),
                    size_str: format_bytes(m.size).into(),
                    bitrate_str: m.bitrate_kbps.map(|b| format!("{b} kbps")).unwrap_or_default().into(),
                    track_no: m.track_no.unwrap_or(0) as i32,
                })
                .collect()
        } else {
            Vec::new()
        };

        window.set_drawer_book(book_item);
        window.set_drawer_copies(ModelRc::new(VecModel::from(copy_items)));
        window.set_drawer_media(ModelRc::new(VecModel::from(media_items)));
        window.set_drawer_selected_copy(first_copy_id as i32);
        window.set_drawer_open(true);
    }
}

fn main() -> anyhow::Result<()> {
    // 1. Initialize Paths & Portable check
    let app_paths = AppPaths::resolve()?;
    app_paths.ensure()?;

    // 2. Logging Setup
    let file_appender = tracing_appender::rolling::daily(&app_paths.log_dir, "audiovault.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    let subscriber = tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(non_blocking));
    tracing::subscriber::set_global_default(subscriber)?;

    tracing::info!("AudioVault starting from {:?}", std::env::current_exe()?);

    // 3. Single Instance Enforcement
    let _instance_lock = match SingleInstance::acquire(&app_paths.lock_path()) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("AudioVault is already running: {e}");
            tracing::warn!("AudioVault instance already running, exiting");
            return Ok(());
        }
    };

    // 4. Start Tokio Runtime
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;

    // 5. Open Service
    let service = rt.block_on(async { AppService::open(app_paths.clone(), None, None).await })?;

    // 6. Instantiate Slint Window
    let window = MainWindow::new()?;

    // Initial Data Population
    load_stats(&service, &window);
    load_library(&service, &window, LibraryFilter::All, LibrarySort::Author, "");
    load_volumes(&service, &window);
    load_duplicates(&service, &window);
    load_issues(&service, &window);
    load_optimizer(&service, &window);
    load_tasks(&service, &window);
    load_downloads(&service, &window);
    load_settings(&service, &window);

    // 7. Hook up Slint Callbacks
    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_navigate_to(move |page_idx| {
        let Some(win) = win_handle.upgrade() else { return };
        match page_idx {
            0 => {
                win.set_header_title("Фонотека".into());
                win.set_header_subtitle("Все аудиокниги в каталоге".into());
                load_library(&svc, &win, LibraryFilter::All, LibrarySort::Author, "");
            }
            1 => {
                win.set_header_title("Поиск".into());
                win.set_header_subtitle("Быстрый полнотекстовый поиск FTS5".into());
            }
            2 => {
                win.set_header_title("Хранилища".into());
                win.set_header_subtitle("Локальные диски, внешние HDD и флешки".into());
                load_volumes(&svc, &win);
            }
            3 => {
                win.set_header_title("Дубликаты".into());
                win.set_header_subtitle("Поиск одинаковых аудиозаписей и вариантов".into());
                load_duplicates(&svc, &win);
            }
            4 => {
                win.set_header_title("Проблемы метаданных".into());
                win.set_header_subtitle("Ошибки в тегах, мусор в названиях и транслитерация".into());
                load_issues(&svc, &win);
            }
            5 => {
                win.set_header_title("RuTracker".into());
                win.set_header_subtitle("Поиск аудиокниг по каталогу раздач".into());
            }
            6 => {
                win.set_header_title("Загрузки".into());
                win.set_header_subtitle("Встроенный BitTorrent клиент".into());
                load_downloads(&svc, &win);
            }
            7 => {
                win.set_header_title("Оптимизация".into());
                win.set_header_subtitle("Сборка M4B, AAC перекодирование и экономия места".into());
                load_optimizer(&svc, &win);
            }
            8 => {
                win.set_header_title("Задачи".into());
                win.set_header_subtitle("Очереди дисковых операций и сканеры".into());
                load_tasks(&svc, &win);
            }
            9 => {
                win.set_header_title("Настройки".into());
                win.set_header_subtitle("Конфигурация AudioVault".into());
                load_settings(&svc, &win);
            }
            _ => {}
        }
        load_stats(&svc, &win);
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_refresh_library(move || {
        let Some(win) = win_handle.upgrade() else { return };
        load_library(&svc, &win, LibraryFilter::All, LibrarySort::Author, "");
        load_volumes(&svc, &win);
        load_stats(&svc, &win);
    });

    let svc = service.clone();
    window.on_toggle_theme(move || {
        let mut s = svc.settings();
        s.theme = if s.theme == "dark" { "light".into() } else { "dark".into() };
        let _ = svc.update_settings(s);
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_select_book(move |book_id| {
        let Some(win) = win_handle.upgrade() else { return };
        open_book_details(&svc, &win, book_id as i64);
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_filter_library(move |txt| {
        let Some(win) = win_handle.upgrade() else { return };
        load_library(&svc, &win, LibraryFilter::All, LibrarySort::Author, txt.as_str());
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_sort_library(move |sort_idx| {
        let Some(win) = win_handle.upgrade() else { return };
        let sort = match sort_idx {
            0 => LibrarySort::Author,
            1 => LibrarySort::Title,
            2 => LibrarySort::Duration,
            3 => LibrarySort::Recent,
            _ => LibrarySort::Author,
        };
        load_library(&svc, &win, LibraryFilter::All, sort, "");
    });

    let svc = service.clone();
    window.on_scan_all(move || {
        if let Ok(vols) = svc.volume_summaries() {
            for v in vols {
                if v.volume.is_online {
                    let _ = svc.scan_volume(v.volume.id);
                }
            }
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_search_query(move |query| {
        let Some(win) = win_handle.upgrade() else { return };
        let q_str = query.as_str().trim();
        if q_str.is_empty() {
            win.set_search_results(ModelRc::new(VecModel::from(Vec::new())));
            return;
        }

        let hits = svc.search(q_str, 50).unwrap_or_default();
        let items: Vec<BookItem> = hits
            .into_iter()
            .map(|h| {
                let online = h.copies.iter().any(|c| c.online);
                let vol_name = h.copies.first().map(|c| c.volume_name.clone()).unwrap_or_default();
                BookItem {
                    id: h.book_id as i32,
                    title: h.title.into(),
                    authors: h.authors.into(),
                    narrator: h.narrators.into(),
                    series: h.series.unwrap_or_default().into(),
                    series_index: "".into(),
                    duration_str: format_duration(h.duration_ms).into(),
                    format_badge: h.format.to_uppercase().into(),
                    quality_score: 85,
                    volume_name: vol_name.into(),
                    volume_online: online,
                    copy_count: 1,
                    total_size_str: format_bytes(h.size).into(),
                }
            })
            .collect();
        win.set_search_results(ModelRc::new(VecModel::from(items)));
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_add_folder(move || {
        if let Some(folder) = rfd::FileDialog::new().set_title("Выберите папку с аудиокнигами").pick_folder() {
            if let Ok(_) = svc.add_storage_folder(&folder, true) {
                if let Some(win) = win_handle.upgrade() {
                    load_volumes(&svc, &win);
                    load_stats(&svc, &win);
                }
            }
        }
    });

    let svc = service.clone();
    window.on_scan_volume(move |vol_id| {
        let _ = svc.scan_volume(vol_id as i64);
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_refresh_volumes(move || {
        let Some(win) = win_handle.upgrade() else { return };
        load_volumes(&svc, &win);
        load_stats(&svc, &win);
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_dup_keep_a(move |dup_id| {
        let _ = svc.resolve_duplicate(dup_id as i64, DuplicateStatus::KeptA);
        if let Some(win) = win_handle.upgrade() {
            load_duplicates(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_dup_keep_b(move |dup_id| {
        let _ = svc.resolve_duplicate(dup_id as i64, DuplicateStatus::KeptB);
        if let Some(win) = win_handle.upgrade() {
            load_duplicates(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_dup_keep_both(move |dup_id| {
        let _ = svc.resolve_duplicate(dup_id as i64, DuplicateStatus::KeptBoth);
        if let Some(win) = win_handle.upgrade() {
            load_duplicates(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_dup_mark_different(move |dup_id| {
        let _ = svc.resolve_duplicate(dup_id as i64, DuplicateStatus::DifferentVariants);
        if let Some(win) = win_handle.upgrade() {
            load_duplicates(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_dup_ignore(move |dup_id| {
        let _ = svc.resolve_duplicate(dup_id as i64, DuplicateStatus::Ignored);
        if let Some(win) = win_handle.upgrade() {
            load_duplicates(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_issue_apply_fix(move |issue_id| {
        let _ = svc.ignore_issue(issue_id as i64);
        if let Some(win) = win_handle.upgrade() {
            load_issues(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_issue_dismiss(move |issue_id| {
        let _ = svc.ignore_issue(issue_id as i64);
        if let Some(win) = win_handle.upgrade() {
            load_issues(&svc, &win);
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_issue_edit(move |book_id| {
        if let Some(win) = win_handle.upgrade() {
            if let Ok(Some(d)) = svc.book_details(book_id as i64) {
                win.set_edit_book_id(book_id);
                win.set_edit_title(d.title.into());
                win.set_edit_authors(d.authors.into());
                win.set_edit_narrator(d.narrators.into());
                win.set_edit_series(d.series.unwrap_or_default().into());
                win.set_edit_series_index(d.series_index.map(|i| format!("{i}")).unwrap_or_default().into());
                win.set_edit_modal_open(true);
            }
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_torrent_search(move |q| {
        let svc_inner = svc.clone();
        let win_inner = win_handle.clone();
        let query = q.as_str().to_string();

        if let Some(win) = win_inner.upgrade() {
            win.set_is_torrent_searching(true);
        }

        tokio::spawn(async move {
            let res = svc_inner.torrents().search_rutracker(&query).await.unwrap_or_default();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(win) = win_inner.upgrade() {
                    let items: Vec<TorrentSearchResultItem> = res
                        .into_iter()
                        .enumerate()
                        .map(|(idx, r)| TorrentSearchResultItem {
                            id: idx as i32,
                            topic_id: r.topic_id.into(),
                            title: r.title.into(),
                            author: "".into(),
                            reader: "".into(),
                            size_str: format_bytes(r.size_bytes).into(),
                            seeds: r.seeders as i32,
                            leeches: r.leechers as i32,
                            bitrate: "".into(),
                            category: r.category.into(),
                        })
                        .collect();
                    win.set_torrent_results(ModelRc::new(VecModel::from(items)));
                    win.set_is_torrent_searching(false);
                }
            });
        });
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_torrent_download(move |topic_id| {
        let svc_inner = svc.clone();
        let win_inner = win_handle.clone();
        let topic = topic_id.as_str().to_string();

        tokio::spawn(async move {
            if let Ok(vols) = svc_inner.volume_summaries() {
                if let Some(v) = vols.iter().find(|v| v.volume.is_online) {
                    if let Ok(bytes) = svc_inner.torrents().fetch_rutracker_torrent(&topic).await {
                        let source = TorrentSource::TorrentFile(bytes);
                        let _ = svc_inner.torrents().add(&source, v.volume.id, "Downloads", None, false).await;
                    }
                }
            }
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(win) = win_inner.upgrade() {
                    win.set_current_page(6); // Switch to Downloads tab
                    load_downloads(&svc_inner, &win);
                }
            });
        });
    });

    let svc = service.clone();
    window.on_dl_pause(move |id| {
        let svc_inner = svc.clone();
        tokio::spawn(async move {
            let _ = svc_inner.torrents().pause(id as i64).await;
        });
    });

    let svc = service.clone();
    window.on_dl_resume(move |id| {
        let svc_inner = svc.clone();
        tokio::spawn(async move {
            let _ = svc_inner.torrents().resume(id as i64).await;
        });
    });

    let svc = service.clone();
    window.on_dl_delete(move |id| {
        let svc_inner = svc.clone();
        tokio::spawn(async move {
            let _ = svc_inner.torrents().remove(id as i64, false, true).await;
        });
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_opt_transcode(move |copy_id| {
        let _ = svc.start_transcode(copy_id as i64, true, None);
        if let Some(win) = win_handle.upgrade() {
            load_optimizer(&svc, &win);
            load_tasks(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_opt_dismiss(move |copy_id| {
        let conn = svc.db().conn().unwrap();
        let _ = audiovault_db::optimize::set_state(&conn, copy_id as i64, "dismissed");
        if let Some(win) = win_handle.upgrade() {
            load_optimizer(&svc, &win);
        }
    });

    let svc = service.clone();
    window.on_task_cancel(move |task_id| {
        let conn = svc.db().conn().unwrap();
        let _ = audiovault_db::jobs::request_cancel(&conn, task_id as i64);
    });

    let svc = service.clone();
    window.on_save_credentials(move |u, p| {
        let svc_inner = svc.clone();
        let username = u.as_str().to_string();
        let password = p.as_str().to_string();
        tokio::spawn(async move {
            let _ = svc_inner.torrents().login_rutracker(&username, &password).await;
        });
    });

    let svc = service.clone();
    window.on_test_rutracker_login(move || {
        let svc_inner = svc.clone();
        tokio::spawn(async move {
            let s = svc_inner.settings();
            let user = s.rutracker_username;
            if let Ok(Some(pass)) = svc_inner.credentials().get(&format!("rutracker:{user}")) {
                let _ = svc_inner.torrents().login_rutracker(&user, &pass).await;
            }
        });
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_browse_ffmpeg(move || {
        if let Some(file) = rfd::FileDialog::new()
            .set_title("Укажите исполняемый файл FFmpeg")
            .add_filter("Исполняемые файлы", &["exe", "*"])
            .pick_file()
        {
            let mut s = svc.settings();
            s.ffmpeg_path = file.display().to_string();
            let _ = svc.update_settings(s);
            if let Some(win) = win_handle.upgrade() {
                load_settings(&svc, &win);
            }
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_detect_ffmpeg(move || {
        let bundled = audiovault_platform::paths::bundled_resource_dirs();
        if let Some(detected) = audiovault_media::transcode::locate_ffmpeg(None, &bundled) {
            let mut s = svc.settings();
            s.ffmpeg_path = detected.display().to_string();
            let _ = svc.update_settings(s);
            if let Some(win) = win_handle.upgrade() {
                load_settings(&svc, &win);
            }
        }
    });

    let svc = service.clone();
    window.on_open_data_dir(move || {
        let _ = shell::open_folder(&svc.paths().data_dir);
    });

    let svc = service.clone();
    window.on_open_logs_dir(move || {
        let _ = shell::open_folder(&svc.paths().log_dir);
    });

    let svc = service.clone();
    window.on_drawer_open_folder(move |copy_id| {
        let conn = svc.db().conn().unwrap();
        if let Ok(Some(copy)) = audiovault_db::catalog::get_copy(&conn, copy_id as i64) {
            if let Ok(Some(vol)) = audiovault_db::volumes::get(&conn, copy.volume_id) {
                if let Some(mount) = vol.last_mount_path {
                    let full = PathBuf::from(mount).join(&copy.rel_path);
                    let _ = shell::reveal(&full);
                }
            }
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_drawer_edit_book(move |book_id| {
        if let Some(win) = win_handle.upgrade() {
            if let Ok(Some(d)) = svc.book_details(book_id as i64) {
                win.set_edit_book_id(book_id);
                win.set_edit_title(d.title.into());
                win.set_edit_authors(d.authors.into());
                win.set_edit_narrator(d.narrators.into());
                win.set_edit_series(d.series.unwrap_or_default().into());
                win.set_edit_series_index(d.series_index.map(|i| format!("{i}")).unwrap_or_default().into());
                win.set_edit_modal_open(true);
            }
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_drawer_delete_copy(move |copy_id| {
        if let Some(win) = win_handle.upgrade() {
            let conn = svc.db().conn().unwrap();
            if let Ok(Some(c)) = audiovault_db::catalog::get_copy(&conn, copy_id as i64) {
                win.set_delete_copy_id(copy_id);
                win.set_delete_title("Копия аудиокниги".into());
                win.set_delete_path(c.rel_path.into());
                win.set_delete_modal_open(true);
            }
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_drawer_optimize_copy(move |copy_id| {
        let _ = svc.start_transcode(copy_id as i64, true, None);
        if let Some(win) = win_handle.upgrade() {
            win.set_drawer_open(false);
            win.set_current_page(8); // Switch to Tasks tab
            load_tasks(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_confirm_delete_copy(move |copy_id, perm| {
        let _ = svc.plan_delete_copy(copy_id as i64, perm, true);
        if let Some(win) = win_handle.upgrade() {
            win.set_drawer_open(false);
            load_library(&svc, &win, LibraryFilter::All, LibrarySort::Author, "");
            load_stats(&svc, &win);
        }
    });

    let svc = service.clone();
    let win_handle = window.as_weak();
    window.on_save_edited_book(move |book_id, title, authors, narrator, series, series_idx| {
        let conn = svc.db().conn().unwrap();
        let authors_vec: Vec<String> = authors.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let _ = conn.execute(
            "UPDATE book SET title = ?2, sort_title = ?3, title_key = ?4, series_name = ?5, series_index = ?6, updated_at = ?7 WHERE id = ?1",
            rusqlite::params![
                book_id as i64,
                title.as_str(),
                audiovault_core::normalize::sort_key(title.as_str()),
                audiovault_core::normalize::title_key(title.as_str()),
                if series.is_empty() { None } else { Some(series.as_str()) },
                series_idx.parse::<f64>().ok(),
                audiovault_db::now_ts()
            ],
        );
        let _ = audiovault_db::catalog::set_book_authors(&conn, book_id as i64, &authors_vec, "manual");
        let _ = audiovault_db::catalog::ensure_variant(&conn, book_id as i64, &[narrator.as_str().to_string()]);
        let _ = audiovault_db::catalog::refresh_book(&conn, book_id as i64);

        if let Some(win) = win_handle.upgrade() {
            load_library(&svc, &win, LibraryFilter::All, LibrarySort::Author, "");
            open_book_details(&svc, &win, book_id as i64);
        }
    });

    // 8. Event Sink for live UI refresh
    let svc_ev = service.clone();
    let win_ev = window.as_weak();
    service.events().set_sink(move |event| {
        let svc_clone = svc_ev.clone();
        let win_clone = win_ev.clone();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(win) = win_clone.upgrade() else { return };
            match event {
                AppEvent::VolumesChanged => {
                    load_volumes(&svc_clone, &win);
                    load_stats(&svc_clone, &win);
                }
                AppEvent::LibraryChanged => {
                    load_library(&svc_clone, &win, LibraryFilter::All, LibrarySort::Author, "");
                    load_stats(&svc_clone, &win);
                }
                AppEvent::DuplicatesChanged => {
                    load_duplicates(&svc_clone, &win);
                    load_stats(&svc_clone, &win);
                }
                AppEvent::IssuesChanged => {
                    load_issues(&svc_clone, &win);
                    load_stats(&svc_clone, &win);
                }
                AppEvent::JobsChanged => {
                    load_tasks(&svc_clone, &win);
                    load_stats(&svc_clone, &win);
                }
                AppEvent::TorrentsChanged => {
                    load_downloads(&svc_clone, &win);
                    load_stats(&svc_clone, &win);
                }
                AppEvent::Notice { text, .. } => {
                    win.set_header_subtitle(text.into());
                }
                _ => {}
            }
        });
    });

    // 9. Run Slint Application Window
    window.run()?;

    // 10. Clean Graceful Shutdown
    tracing::info!("Shutting down AudioVault service...");
    rt.block_on(service.shutdown());

    Ok(())
}
