# CODEBASE.md — navigation map (AudioVault / BookBase)

Desktop manager for a local audiobook + ebook library. Rust workspace + Tauri 2 shell + React 19 UI. Prose docs: `ARCHITECTURE.md` (design), `DEVELOPMENT.md` (setup), `README.md` (product). Line numbers drift; re-grep the symbol if a `path:line` misses.

## 1. Stack and commands

- Rust 2024, rust-version 1.89, workspace resolver 3 (`Cargo.toml`). Crate names are `audiovault-<dir>` (core, db, media, indexer, torrent, platform, service).
- DB: SQLite via `rusqlite` (bundled, FTS5). Torrents: `librqbit`. Fingerprint: Chromaprint. Transcode: external `ffmpeg`/`ffprobe`.
- Frontend: React 19, TypeScript ~6, Vite 8, `lucide-react`, plain CSS. No router, no state lib, no i18n, no Tailwind.
- Shell: Tauri 2 (`@tauri-apps/api` / `cli` pinned `~2.11.0` — keep in sync with the `tauri` crate).

Run from repo root unless noted:

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all -- --check
cd apps/desktop && npm ci
cd apps/desktop && npm run lint        # oxlint
cd apps/desktop && npm run build       # tsc -b && vite build
cd apps/desktop && npm run tauri dev   # app with hot reload (devUrl :5173)
cd apps/desktop && npm run tauri build # NSIS/msi installer
```

CI = `.github/workflows/ci.yml` (lint, build, fmt, check, test on win/ubuntu/macos). Release = `release.yml`. Default workspace member is `apps/desktop/src-tauri`.

## 2. Directory map

| Path | Owns | Must NOT contain |
|---|---|---|
| `crates/core` | Domain ids/enums, normalization, transliteration, quality + duplicate scoring, safe paths. Pure logic. | IO, SQL, OS calls |
| `crates/db` | SQLite schema, migrations, one module per aggregate. | Business rules, file IO |
| `crates/media` | Probe, tags, epub/fb2 parse, hash, fingerprint, transcode. | DB access |
| `crates/indexer` | Scan, analyze, hash, duplicate-detection pipeline. | Tauri / UI types |
| `crates/torrent` | RuTracker client, magnet parse, torrent engine. | DB access |
| `crates/platform` | OS glue: volumes, app paths, credentials, shell, power, single instance. | Domain logic |
| `crates/service` | `AppService` facade, job `Runner`/`Executor`, IO scheduler, events, fileops, settings. Only layer that wires crates together. | Tauri types |
| `apps/desktop/src-tauri` | Tauri commands, DTOs, event bridge, updater. Thin adapter over `AppService`. | Domain logic |
| `apps/desktop/src` | React UI. | Direct `invoke` outside `api/` |
| `tools/seed`, `tools/inspect` | Dev CLIs (synthetic catalog, read-only DB inspector). `anyhow`. | Shipped code |
| `tools/*.py` | UI automation / screenshot helpers (`ui_*.py`). | App logic |
| `artifacts/ui` | Generated UI captures. | Source |

Dependency direction: `core` ← `db`/`media`/`platform` ← `indexer`/`torrent` ← `service` ← `src-tauri` ← UI. Do not invert.

## 3. Entry points and core

### 3.1 App bootstrap and config
- `apps/desktop/src-tauri/src/main.rs:4` → `lib.rs:9 run`: single-instance lock (`lib.rs:13-28`), tokio runtime + `AppService::open` (`:30-39`), `.manage(AppState)` (`:43`), event sink → `"app_event"` (`:45-60`), `generate_handler!` with ~57 commands (`:61-119`). New command = add fn in `commands.rs` AND register here.
- `src-tauri/tauri.conf.json` (window 1280x720 dark, NSIS ru/en, `frontendDist ../dist`), `capabilities/default.json` (`core:default` only), `nsis/hooks.nsh`. No Tauri plugins registered.
- Frontend: `apps/desktop/src/main.tsx:6` → `App.tsx:51 App`.
- Settings: `crates/service/src/settings.rs:16 AppSettings` (`load` :92, `save` :99), stored in DB `app_setting` (`crates/db/src/settings.rs:10 get/set`, `get_json` :32).

### 3.2 Command handlers (IPC)
All in `src-tauri/src/commands.rs` (`AppState` :13; DTOs `BookDto` :18 … `StatsDto` :237).
- Library/volumes: `get_library_books` :250, `get_book_details` :333, `get_volumes` :464, `add_storage_folder` :491, `scan_volume` :535, `scan_all` :544, `search_books` :561
- Duplicates/issues/optimizer/tasks: `get_duplicates` :1036, `find_all_duplicates` :1041, `resolve_duplicate` :1051, `get_issues` :1085, `get_optimizations` :1128, `start_transcode` :1163, `get_tasks` :1188, `cancel_task` :1206
- Torrents/settings: `search_torrents` :1213, `download_torrent` :1241, `get_downloads` :1272, `get_settings` :1332, `save_settings` :1362, `save_credentials` :1398
- Ebooks/marks: `search_ebooks` :1590, `get_ebook_detail` :1615, `set_book_listened` :1632, `set_ebook_read` :1645, `set_*_marked_for_deletion` :1658-1697
- Deletion: `execute_deletion` :1710, `execute_copy_deletion` :1722, `cancel_pending_deletion` :1734, `execute_pending_deletions_for_volume` :1746, `get_deletion_history` :1757
- Updater (custom, GitHub releases, no updater plugin): `check_github_update` :1837, `download_and_install_update` :1990; emits `update://progress` (:2047, :2070).
- Helpers: `canonical_format_name` :602, `collect_all_duplicates` :658, `parse_semver` :1817.

### 3.3 Business logic / services (`crates/service/src`)
- `app.rs:67 AppService::open` — facade. Deletion: `plan_delete_copy` :611, `execute_deletion` :701, `execute_copy_deletion` :975, `execute_pending_deletions_for_volume` :1137.
- `runner.rs:47 Runner` — job queue, pause/cancel/retry (`dispatcher_loop` :232, `run_job` :341, `is_retryable` :334); `JobCtx` :51 implements `JobContext`.
- `exec.rs:36 Executor` — job-kind dispatch (`run` :84): `scan` :108, `analyze` :181, `duplicates` :257, `fingerprint` :315, `full_hash` :337, `torrent_import` :389.
- `scheduler.rs:25 Budget` / `budget_for` :32 / `IoScheduler::try_acquire` :115 — per-`StorageClass` IO limits (HDD vs SSD vs network).
- `events.rs:11 AppEvent`, `:37 EventBus` (`emit`, `info`/`warn`/`error`).
- `fileops.rs` — journaled ops: `plan_delete` :50, `plan_move` :134, `execute` :200, `copy_verified` :340 (never overwrites).
- `monitor.rs:58 refresh` / `:40 observe` — volume online/offline state.
- `torrents.rs:29 TorrentService` — `add` :269, `remove` :382, `poll_stats` :184.

### 3.4 Indexer pipeline (`crates/indexer/src`)
`walk.rs:279 walk` → `scan.rs:365 scan_root` / `:197 ingest_set` → `analyze.rs:888 analyze_copy` (observations `:166`, `:397`) → `hashing.rs:19 full_hash_copy`, `:52 fingerprint_copy` → `duplicates.rs:60 detect`, `:136 detect_all`, `:153 pairs_needing_fingerprints`. Trait `lib.rs:45 JobContext` (`is_cancelled`, `progress(fraction, text)`; fraction `-1.0` = indeterminate).

### 3.5 Data models and DB
- Ids: bare `i64` aliases in `crates/core/src/model.rs:8` (`BookId`, `VariantId`, `CopyId`, `VolumeId`, `JobId`, `MediaFileId`, `PersonId`). Enums via `str_enum!` macro `model.rs:21` (string round-trip with DB): `StorageClass` :41, `CopyState` :80, `FileKind` :88, `MetadataSource` :129, `DuplicateVerdict` :179, `JobKind` :201, `JobState` :248, `FileOpState` :290.
- `crates/db/src/lib.rs:52 Db` (`open` :101 migrates, `write` :158 tx), `DbError` :35, `Result` :48.
- Migrations: `crates/db/migrations/0001..0005_*.sql`, registered in `crates/db/src/migrations.rs:8 MIGRATIONS` (version = index+1, `PRAGMA user_version`, refuses newer DB). Add a migration = new `.sql` + append to `MIGRATIONS`; never edit old files.
  - 0001 core tables (book, audio_variant, book_copy, media_file, person, series, duplicate_candidate, background_job, file_operation, torrent_transfer, book_fts…); 0002 fingerprints/optimizer; 0003 media tags; 0004 ebooks (`ebook_entry`, `ebook_fts`); 0005 reading status + `pending_deletion`/`deletion_history`.
- DB modules: `catalog.rs` (`create_book` :127, `merge_books` :338, `upsert_media` :884), `library.rs` (`page` :135), `search.rs:175 search`, `ebooks.rs:272`, `duplicates.rs`, `jobs.rs:96 enqueue`, `fileops.rs:74 plan`, `deletion.rs:75`, `volumes.rs`, `torrents.rs`, `settings.rs`.

### 3.6 Shared utilities — reuse, do not rewrite
- Search/text: `core/normalize.rs:17 search_norm`, `:81 cyr_to_lat`, `:140 skeleton`; `core/translit.rs:535 auto_reverse_translit`, `:619 reverse_phrase`.
- Identity/dedupe: `core/identity.rs:40 book_identity_key`; `core/duplicate.rs:139 assess`; `core/quality.rs:44 score`, `:115 recommend_keep`; `core/optimize.rs:162 recommend`.
- Metadata merge: `core/metadata.rs:96 resolve`.
- Paths: `core/paths.rs:100 safe_join` (traversal guard — use for every user/archive-derived path); `platform/paths.rs:12 AppPaths`; `platform/shell.rs:80 move_to_trash`.
- Media: `media/probe.rs:65 probe_file`, `epub.rs:39`, `fb2.rs:89`, `hash.rs:16 quick_hash`/`:48 full_hash`, `transcode.rs:435 run_transcode`, `tags_write.rs:71 write_tags`; traits `Cancel` `lib.rs:44`, `AudioFingerprintService` `fingerprint.rs:41`.
- Platform: `CredentialStore` `credentials.rs:6` (`KeyringStore`, `MemoryStore` for tests), `VolumeEnumerator` `volumes/mod.rs:66`, `VolumeMarker` `marker.rs:16`, `SingleInstance` `instance.rs:17`, `SleepGuard` `power.rs:17`.
- Torrent: `SearchProvider` trait `provider.rs:35`, `RutrackerClient` `rutracker.rs:43`, `parse_magnet` `magnet.rs:15`, `TorrentEngine` `engine.rs:126`.

### 3.7 Frontend (`apps/desktop/src`)
- `api/client.ts:30 api` — the ONLY layer calling `invoke` (~50 wrappers); `api.onEvent` :482 listens `"app_event"`. Also `formatBytes` :493, `formatDuration` :500. Types mirror Rust DTOs in `api/types.ts`. Labels/verdict text in `api/presentation.ts` (`issueLabels` :2, `issueDescriptions` :13, `duplicateVerdict` :34, `duplicateEvidence` :37, `taskLabels` :65).
- New IPC = Rust command + register in `lib.rs` + DTO in `commands.rs` + type in `types.ts` + wrapper in `client.ts`.
- `App.tsx` — all top-level state (`useState` :52-115), page switch by string `currentPage` (:546, render blocks :684-807), theme in localStorage `audiovault-theme` (:97-132, `data-theme` attr, CSS `index.css:61`), event refresh `App.tsx:247`.
- Pages `pages/`: Library, Ebooks, Search, Storages, Duplicates, DeletionCandidates, Issues, Optimizer, RuTracker, Downloads, Tasks, Settings.
- Hook `hooks/useLibrary.ts:5` — paged fetch (PAGE_SIZE 250, 180 ms debounce).
- Shared UI: `components/Primitives.tsx` (`SearchField` :5, `EmptyState` :47, `StatusIndicator` :65, `BookCover` :147, `LoadingRows` :214, `VirtualList` :228, `ProgressBar` :291, `getFormatClass` :79, `getCoverPalette` :91), `Modal.tsx`, `ContextMenu.tsx`, `Sidebar.tsx`, `Header.tsx`, `BookList.tsx:258` (columns `ALL_COLUMNS` :45), `BookCard.tsx`, `BookInspectorDrawer.tsx`, `EbookInspectorDrawer.tsx`, `DeletionConfirmModal.tsx`, `DeletionHistoryModal.tsx`, `UpdateModal.tsx`, `ThemeOnboardingModal.tsx`.
- Styling: plain CSS, `index.css` (~3300 lines, global) + `App.css`.

## 4. Patterns and rules

- Errors: per-crate `thiserror` enum + `pub type Result<T>` (`DbError`, `MediaError`, `IndexerError` `indexer/lib.rs:20`, `ServiceError` `service/lib.rs:28`, `TorrentError` `torrent/lib.rs:21`, `PlatformError`). `From<rusqlite::Error>` impls exist. `anyhow` only in `tools/*`. Tauri commands return `Result<_, String>` to the UI.
- Layering: only `service` composes crates; `src-tauri` stays a thin adapter; UI talks only through `api/client.ts`.
- Long work = a job: enqueue via `db::jobs::NewJob`, run by `Runner`/`Executor`, report with `ctx.progress`, honor `ctx.is_cancelled()`.
- UI refresh is event-driven: service emits `AppEvent` → `"app_event"` (payloads `volumes|library|duplicates|issues|jobs|torrents_changed`, `notice`) → `api.onEvent` → refetch. Do not poll.
- File mutation goes through journaled `file_operation` (`db::fileops` + `service::fileops`) with state machine `FileOpState::can_transition_to`; deletions to offline volumes queue in `pending_deletion` and flush on volume return.
- Offline volumes stay in the catalog ("ghost"); volume identity = marker file `.audiovault-id`.
- DB: forward-only migrations; enums stored as strings via `str_enum!`; ids are `i64`; search keys come from `search_norm` (Cyrillic ↔ Latin aware).
- Secrets (RuTracker login) only via `CredentialStore`; never in `app_setting` or logs.
- Tests: inline `#[cfg(test)] mod tests`; integration `crates/indexer/tests/pipeline.rs` (WAVs generated at runtime with `tempfile`, `write_wav` :20); `crates/torrent/tests/fixtures/*.html` via `include_str!`. No frontend tests. `tools/` has none.
- Naming: Rust snake_case, types PascalCase, crate dirs short but package `audiovault-*`. TS: components PascalCase files, `api` methods camelCase mirroring snake_case Rust commands. UI strings are hardcoded Russian (no i18n; locale `ru-RU`).
- Gaps (not verified): `main.tsx` CSS import site; command bodies beyond signatures; `tools/*.py` internals.
