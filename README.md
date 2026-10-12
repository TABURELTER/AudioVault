# AudioVault

> **Local-First Native Audiobook Collection Manager & Catalog**

AudioVault is a high-performance cross-platform desktop application designed to catalog, organize, deduplicate, optimize, and search multi-terabyte audiobook libraries spanning multiple internal drives, external HDDs, and removable media.

Built entirely in Rust with a native [Slint](https://slint.dev/) graphical interface — zero Electron, zero Tauri, zero WebView overhead.

---

## Key Capabilities

### 1. Offline-Storage-Aware Catalog
- **Persistent Volume Identity:** Volumes are identified by `.audiovault-id` UUID markers, OS volume GUIDs, and hardware serial numbers. Drive letters and mount points can change without breaking the catalog.
- **Offline Availability:** Books residing on disconnected external HDDs or flash drives remain fully searchable and browsable. When a volume is reconnected, AudioVault instantly detects it and restores live playback and file management.
- **Concurrency Budgeting:** Hard disk drives (HDDs) are strictly protected from thrashing with a serialized 1-heavy / 1-light I/O budget. High-speed SSDs run multi-threaded parallel probes and hashes.

### 2. Deep Ingestion & Metadata Normalization
- **Multi-Format Probing:** Native parsing for MP4/M4B, MP3 (ID3v1/ID3v2.3/ID3v2.4), FLAC, Ogg Vorbis, and Opus via Lofty and Symphonia.
- **FB2 Companion Parsing:** Extracts annotations, series indices, publishers, and original cover art from embedded or sibling `.fb2` e-books.
- **Russian Transliteration Engine:** Bidirectional beam-search transliteration and phonetic normalization (e.g. searching *"dostoevsky"* or *"lukyanenko"* matches *"Достоевский"* and *"Лукьяненко"*).
- **SQLite WAL & FTS5:** Full-text search with typo tolerance, prefix matching, and sub-millisecond query latency across 100,000+ audio files.

### 3. Acoustic Fingerprinting & Deduplication
- **Multi-Level Content Analysis:** Progressive comparison starting from cheap file sizes and directory names, up to quick BLAKE3 head/tail hashing and full content digests.
- **Acoustic Fingerprinting:** Integrated Chromaprint acoustic fingerprinting detects identical audio recordings regardless of format (e.g. 320 kbps MP3 vs 64 kbps M4B vs FLAC).
- **Safe Variant Grouping:** Distinguishes between byte-identical duplicates, re-encodes of the same performance, and different narrators performing the same novel.

### 4. Storage Optimizer & Transcoding
- **Repack & Transcode Engine:** Merges folders with hundreds of loose MP3 files into a single, clean `.m4b` audiobook container with embedded AAC audio, chapter markers, and cover art.
- **Disk Savings Calculator:** Computes exact disk savings before running conversions.
- **FFmpeg Integration:** Auto-detects local FFmpeg installations or bundled sidecar binaries with real-time conversion progress.

### 5. Embedded BitTorrent & RuTracker Integration
- **Direct Torrent Engine:** Powered by embedded `librqbit 9.0`, providing high-throughput downloads without third-party torrent clients.
- **RuTracker Search Provider:** Native HTML parsing and CP1251 decoding with automatic rate limiting and CAPTCHA detection.
- **OS Keyring Security:** Credentials are encrypted and stored in the OS credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service). Plaintext passwords are never written to disk or logs.
- **Auto-Import:** Completed downloads are automatically probed, tagged, and indexed into the catalog.

### 6. Safe Journaled File Operations
- **Atomic Operations:** Renames, moves, and deletions are recorded in an SQLite journal before execution.
- **Recycle Bin by Default:** Deletions use the native system trash. Permanent file deletion requires explicit confirmation.
- **Startup Reconciliation:** If the app is closed or power is lost mid-operation, incomplete tasks are flagged and verified upon restart.

---

## Architecture Overview

AudioVault is organized as a Cargo workspace:

```
BookBase/
├── apps/
│   └── audiovault/          # Native Slint GUI application & desktop entry
│       ├── ui/              # Slint templates (theme, widgets, drawer, 10 pages)
│       └── src/main.rs      # Tokio runtime, Slint callbacks & AppService bridge
├── crates/
│   ├── core/                # Models, normalization, transliteration, quality scoring
│   ├── db/                  # SQLite WAL migrations, FTS5, catalog, duplicates, issues
│   ├── platform/            # Volume identity, AppPaths, OS keyring, single instance
│   ├── media/               # Audio probes, FB2 parsing, Chromaprint, FFmpeg transcode
│   ├── indexer/             # Incremental filesystem scanner & duplicate detector
│   ├── torrent/             # librqbit 9.0 session wrapper & RuTracker provider
│   └── service/             # AppService facade, scheduler, file operations, event bus
└── tools/
    └── seed/                # Synthetic catalog generator for testing & UI benchmarks
```

---

## Building and Running

### Prerequisites
- **Rust:** Stable toolchain (`1.85+` or `1.94+`).
- **C/C++ Compiler:** MSVC on Windows, GCC/Clang on Linux, Xcode Command Line Tools on macOS.
- **FFmpeg (Optional but Recommended):** Place `ffmpeg` on your `PATH` or specify its path in AudioVault Settings.

### Development Build
```powershell
# Clone the repository
git clone https://github.com/your-username/BookBase.git
cd BookBase

# Run all workspace unit and integration tests (136 tests)
cargo test --workspace

# Run the desktop application
cargo run -p audiovault
```

### Seeding Test Data
To populate a local catalog with realistic test audiobooks (authors, narrators, duplicate pairs, optimization candidates):
```powershell
cargo run -p audiovault-seed -- target/test_catalog.sqlite
```

---

## Keyboard Shortcuts & Navigation

| Tab | Hotkey | Function |
|---|---|---|
| **Фонотека** | `1` | Browse audiobooks, filter by title/author, inspect copies |
| **Поиск** | `2` | FTS5 transliteration search across the entire collection |
| **Хранилища** | `3` | Manage online/offline drives, root folders, trigger scans |
| **Дубликаты** | `4` | Review acoustic and metadata duplicate candidates |
| **Проблемы** | `5` | Metadata issue fixer (release garbage, missing tags) |
| **RuTracker** | `6` | Live search RuTracker audiobooks catalog |
| **Загрузки** | `7` | Active and completed BitTorrent transfers |
| **Оптимизация** | `8` | Storage savings and M4B transcode recommendations |
| **Задачи** | `9` | Background job dispatcher and queue status |
| **Настройки** | `0` | Theme, credentials, FFmpeg location, storage limits |

---

## License
AudioVault is open-source software licensed under the MIT License.
