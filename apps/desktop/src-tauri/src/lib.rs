mod commands;

use tauri::Emitter;
use audiovault_platform::paths::AppPaths;
use audiovault_service::{AppEvent, AppService};
use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_paths = AppPaths::resolve().expect("paths resolve");
    let _ = app_paths.ensure();

    let _single = match audiovault_platform::instance::SingleInstance::acquire(&app_paths.lock_path()) {
        Ok(audiovault_platform::instance::InstanceRole::Primary(s)) => Some(s),
        Ok(audiovault_platform::instance::InstanceRole::Secondary { notified }) => {
            if notified {
                eprintln!("AudioVault is already running; bringing window to front.");
            } else {
                eprintln!("AudioVault is already running.");
            }
            return;
        }
        Err(e) => {
            eprintln!("Warning: could not acquire single-instance lock: {e}");
            None
        }
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime build");

    let service = rt.block_on(async {
        AppService::open(app_paths.clone(), None, None).await.expect("open service")
    });

    let service_for_setup = service.clone();

    tauri::Builder::default()
        .manage(AppState { service })
        .setup(move |app| {
            let handle = app.handle().clone();
            service_for_setup.events().set_sink(move |event| {
                let ev_str = match event {
                    AppEvent::VolumesChanged => "volumes_changed",
                    AppEvent::LibraryChanged => "library_changed",
                    AppEvent::DuplicatesChanged => "duplicates_changed",
                    AppEvent::IssuesChanged => "issues_changed",
                    AppEvent::JobsChanged => "jobs_changed",
                    AppEvent::TorrentsChanged => "torrents_changed",
                    _ => "notice",
                };
                let _ = handle.emit("app_event", ev_str);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_library_books,
            get_book_details,
            get_volumes,
            add_storage_folder,
            rename_volume,
            reset_catalog,
            pick_folder,
            scan_volume,
            scan_all,
            search_books,
            get_duplicates,
            find_all_duplicates,
            resolve_duplicate,
            get_issues,
            ignore_issue,
            get_optimizations,
            start_transcode,
            transcode_copy,
            get_tasks,
            cancel_task,
            search_torrents,
            download_torrent,
            get_downloads,
            pause_download,
            resume_download,
            delete_download,
            get_settings,
            save_settings,
            save_credentials,
            test_rutracker_connection,
            get_stats,
            reveal_folder,
            open_file,
            get_cover_data,
            search_ebooks,
            get_ebook_detail,
            get_ebook_stats,
            set_book_listened,
            set_ebook_read,
            set_book_marked_for_deletion,
            set_ebook_marked_for_deletion,
            set_multiple_books_marked_for_deletion,
            set_multiple_ebooks_marked_for_deletion,
            execute_deletion,
            execute_pending_deletions_for_volume,
            get_deletion_history,
            clear_deletion_history,
            get_pending_deletions,
            get_deletion_counts,
            ignore_issues,
            ignore_issues_by_kind,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
