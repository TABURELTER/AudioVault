# AudioVault

> **Local-First Desktop Audiobook Collection Manager & Catalog**

AudioVault is a high-performance desktop application designed to catalog, organize, deduplicate, optimize, and search multi-terabyte audiobook libraries spanning multiple internal disks, external HDDs, and removable media (including offline/disconnected drives).

Built on **Tauri 2**, **React 19**, **TypeScript**, and a robust **Rust** core — delivering a modern, dense, and fluid desktop user experience with full offline database persistence and high I/O efficiency.

---

## Key Capabilities

### 1. Offline-Storage-Aware Catalog
- **Persistent Volume Identity:** Volumes are identified by OS volume GUIDs, hardware serial numbers, and optional `.audiovault-id` UUID markers. Drive letters (`F:\`, `G:\`) and mount points can change across reboots without breaking the catalog.
- **Offline Availability:** Books residing on disconnected external HDDs or flash drives remain fully searchable and browsable. When a volume is reconnected, AudioVault instantly detects it and restores live playback and file management.
- **HDD-Aware Concurrency Budgeting:** Hard disk drives (HDDs) are strictly protected from thrashing with a serialized I/O budget. High-speed SSDs run multi-threaded parallel probes and hashes.

### 2. Deep Ingestion & Metadata Normalization
- **Multi-Format Probing:** Native parsing for MP4/M4B, MP3 (ID3v1/ID3v2.3/ID3v2.4), FLAC, Ogg Vorbis, and Opus via Lofty and Symphonia.
- **FB2 Companion Parsing:** Extracts annotations, series indices, publishers, and original cover art from embedded or sibling `.fb2` e-books.
- **Russian Transliteration Engine:** Bidirectional beam-search transliteration and phonetic normalization (e.g. searching *"dostoevsky"* or *"lukyanenko"* matches *"Достоевский"* and *"Лукьяненко"*).
- **SQLite WAL & FTS5:** Full-text search with typo tolerance, prefix matching, and sub-millisecond query latency across 100,000+ audio files.

### 3. Acoustic Fingerprinting & Deduplication
- **Multi-Level Content Analysis:** Progressive comparison starting from cheap file sizes and directory names, up to quick BLAKE3 head/tail hashing and full content digests.
- **Acoustic Fingerprinting:** Integrated Chromaprint acoustic fingerprinting detects identical audio recordings regardless of format (e.g. 320 kbps MP3 vs 64 kbps M4B vs FLAC).
- **Safe Variant Grouping:** Distinguishes between byte-identical duplicates, re-encodes of the same performance, and intentionally different narrators performing the same novel.

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

AudioVault is organized as a Cargo workspace with a Tauri 2 desktop shell and React frontend:

```
BookBase/
├── apps/
│   └── desktop/             # Tauri 2 Desktop Application
│       ├── src/             # React 19 + TypeScript + Lucide UI
│       │   ├── api/         # Typed Tauri IPC client & event listener
│       │   ├── components/  # Sidebar, Header, BookCard, BookInspectorDrawer
│       │   └── pages/       # Library, Search, Storages, Duplicates, Issues,
│       │                    # RuTracker, Downloads, Optimizer, Tasks, Settings
│       └── src-tauri/       # Tauri 2 backend shell, commands & event forwarder
├── crates/
│   ├── core/                # Models, normalization, transliteration, quality scoring
│   ├── db/                  # SQLite WAL migrations, FTS5, catalog, duplicates, issues
│   ├── platform/            # Volume identity, AppPaths, OS keyring, single instance
│   ├── media/               # Audio probes, FB2 parsing, Chromaprint, FFmpeg transcode
│   ├── indexer/             # Incremental filesystem scanner & duplicate detector
│   ├── torrent/             # librqbit 9.0 session wrapper & RuTracker provider
│   └── service/             # AppService facade, scheduler, file operations, event bus
└── tools/
    ├── inspect/             # CLI catalog inspection and diagnostics tool
    └── seed/                # Synthetic catalog generator for testing & UI benchmarks
```

---

## Building and Running

### Prerequisites
- **Node.js:** `v20+` or `v22+` with `npm`
- **Rust:** Stable toolchain (`1.85+` or `1.89+` / `1.94+`)
- **C/C++ Compiler:** MSVC on Windows, GCC/Clang on Linux, Xcode Command Line Tools on macOS
- **FFmpeg (Optional):** Place `ffmpeg` on your `PATH` or specify its path in AudioVault Settings

### Development Build
```powershell
# 1. Install frontend dependencies
cd apps/desktop
npm install

# 2. Build the frontend
npm run build

# 3. Run all workspace unit and integration tests (94 tests)
cd ../..
cargo test --workspace

# 4. Build or launch the desktop application
cargo build -p audiovault-desktop
target\debug\audiovault-desktop.exe
```

### Inspecting Local Catalog
To inspect volumes, background jobs, library roots, and database statistics:
```powershell
cargo run -p audiovault-inspect
```

---

## License
AudioVault is open-source software licensed under the MIT License.
