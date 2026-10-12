# AudioVault — Developer Guide

This document contains setup instructions, architecture guidelines, test procedures, and deployment workflows for engineers working on the AudioVault codebase.

---

## 1. Prerequisites & Toolchain Setup

### 1.1 Rust Compiler
AudioVault requires **Rust 1.80 or later** (tested with stable Rust 1.94).
Verify your toolchain:
```powershell
rustc --version
cargo --version
```

If not installed or outdated:
```powershell
rustup update stable
rustup default stable
```

### 1.2 Platform Build Tools
- **Windows**: Microsoft Visual C++ (MSVC) Build Tools 2022 with the Windows 10/11 SDK.
- **macOS**: Xcode Command Line Tools (`xcode-select --install`).
- **Linux**: Build essentials and development headers for SQLite, fontconfig, and audio:
  ```bash
  sudo apt-get install -y build-essential libfontconfig1-dev libasound2-dev libssl-dev pkg-config
  ```

### 1.3 Media Toolchain (FFmpeg)
Audio transcode and repack planning relies on `ffmpeg` and `ffprobe`.
- **System PATH**: Ensure `ffmpeg` and `ffprobe` are available on your system path:
  ```powershell
  ffmpeg -version
  ffprobe -version
  ```
- **Sidecar Mode**: In portable distributions, place `ffmpeg.exe` and `ffprobe.exe` directly in the same directory as `audiovault.exe` or within a `bin/` subfolder. AudioVault automatically scans adjacent directories before falling back to system `PATH`.

---

## 2. Repository Layout

```
BookBase/
├── Cargo.toml                  # Cargo workspace definition with optimized profiles
├── Cargo.lock                  # Pinned dependency graph
├── ARCHITECTURE.md             # System architecture & technical specifications
├── DEVELOPMENT.md              # Engineering, testing, and deployment guide
├── README.md                   # Product overview and usage manual
│
├── apps/
│   └── audiovault/             # Native Slint GUI desktop application
│       ├── build.rs            # Slint compiler build script
│       ├── Cargo.toml
│       ├── src/
│       │   ├── main.rs         # Application entry point, event bridge, and UI bindings
│       │   └── ...
│       └── ui/                 # Slint declarative templates
│           ├── theme.slint     # Colors, typography, spacing tokens
│           ├── widgets.slint   # Reusable UI controls (Buttons, Inputs, Cards, etc.)
│           ├── sidebar.slint   # Main navigation drawer
│           ├── header.slint    # Top bar & progress status
│           ├── drawer.slint    # Book details inspector
│           ├── modals.slint    # Confirm delete & metadata editor modals
│           ├── pages/          # 10 dedicated view page components
│           └── main.slint      # Root window composition
│
├── crates/
│   ├── core/                   # Pure domain models, transliteration, scoring
│   ├── db/                     # SQLite WAL engine, migrations, FTS5 queries
│   ├── platform/               # Volume enumeration, OS keyring, single instance
│   ├── media/                  # Lofty tag extraction, Symphonia, Chromaprint, FFmpeg
│   ├── indexer/                # Ingestion crawler, acoustic matcher, duplicate grouper
│   ├── torrent/                # Embedded librqbit 9.0 engine & RuTracker scraper
│   └── service/                # AppService facade, I/O scheduler, safe file operations
│
└── tools/
    └── seed/                   # Synthetic multi-volume catalog generator
```

---

## 3. Building & Running

### 3.1 Quick Check
```powershell
cargo check --workspace
```

### 3.2 Running the Desktop App (Debug Mode)
```powershell
cargo run -p audiovault
```

### 3.3 Running with Tracing Logs
Set the `RUST_LOG` environment variable for detailed subsystem diagnostics:
```powershell
$env:RUST_LOG="audiovault=debug,audiovault_service=debug,audiovault_db=info"
cargo run -p audiovault
```

### 3.4 Optimized Release Build
```powershell
cargo build --workspace --release
```
The optimized binary will be located at `target/release/audiovault.exe`.

---

## 4. Test Suite & Validation

AudioVault contains a comprehensive automated test suite covering domain logic, database operations, transliteration beam search, FB2 parsing, Chromaprint fingerprinting, and I/O scheduling.

### 4.1 Running All Workspace Tests
```powershell
cargo test --workspace
```

### 4.2 Running Tests for Specific Subsystems
```powershell
# Core models, normalization, and Russian name parsing
cargo test -p audiovault-core

# SQLite schema migrations, WAL operations, and FTS5 search
cargo test -p audiovault-db

# Hardware volume detection and single-instance locks
cargo test -p audiovault-platform

# Audio tag reading, FB2 parsing, and acoustic fingerprinting
cargo test -p audiovault-media

# Indexer directory walk and duplicate grouping
cargo test -p audiovault-indexer

# librqbit engine integration and magnet link parsing
cargo test -p audiovault-torrent

# Concurrency scheduler and journaled file operations
cargo test -p audiovault-service
```

---

## 5. Working with Test Data (`tools/seed`)

To develop and test UI features against a realistic catalog without scanning tens of gigabytes of audiobooks, use the standalone `seed` tool:

```powershell
cargo run -p audiovault-seed
```

This will:
1. Initialize a clean SQLite database in the current application directory (`audiovault.db`).
2. Register 3 realistic virtual storage volumes:
   - `Internal NVMe (C:)` — High-speed online SSD.
   - `Audiobook Archive (D:)` — External USB HDD (online).
   - `Passport Portable (E:)` — External drive flagged as **offline** for disconnect testing.
3. Populate 120+ authentic audiobooks with authors, narrators, series, genres, cover art, and chapters.
4. Inject deliberate acoustic duplicates (identical audio performances re-encoded at different bitrates) to exercise the duplicate detector and storage optimizer.

---

## 6. Database Migrations Workflow

Migrations reside as raw SQL files in `crates/db/migrations/`:
```
crates/db/migrations/
├── 0001_init.sql          # Primary relational schema (volumes, books, copies, tracks)
├── 0002_fts.sql           # FTS5 full-text virtual tables and sync triggers
└── 0003_media_tags.sql    # Extended tag columns, series indices, acoustic fingerprints
```

### 6.1 Creating a New Migration
1. Add a sequentially numbered file: `0004_your_feature_name.sql`.
2. Include idempotent DDL statements using `CREATE TABLE IF NOT EXISTS` or safe `ALTER TABLE` statements.
3. Register the migration file in `crates/db/src/migrations.rs` within the migration runner loop.
4. Run `cargo test -p audiovault-db` to verify that automatic migration upgrades pass against an existing database.

---

## 7. Slint GUI Development & Hot Reload

AudioVault uses [Slint](https://slint.dev/) for native desktop rendering via DirectX/Direct3D (Windows), Metal (macOS), or Vulkan/Software (Linux).

### 7.1 IDE Setup
Install the official **Slint** extension in VS Code / Antigravity IDE for:
- Syntax highlighting and auto-completion in `.slint` files.
- Live graphical preview canvas for individual components.
- In-editor diagnostics for layout properties.

### 7.2 Slint Preview Tool
You can preview any individual UI page without compiling the full Rust application:
```powershell
cargo install slint-viewer
slint-viewer apps/audiovault/ui/main.slint
```

---

## 8. Continuous Integration & Release Packaging

### 8.1 Automated CI Pipeline
The repository includes a GitHub Actions configuration at `.github/workflows/ci.yml` that validates:
- Rust formatting (`cargo fmt --check`)
- Lints and safety audits (`cargo clippy --workspace --all-targets -- -D warnings`)
- Cross-platform test suite execution across Windows, macOS, and Ubuntu runners.

### 8.2 Release Artifacts
Production release builds strip debug symbols and enable Link-Time Optimization (LTO):
```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"
strip = true
```

The resulting binary has zero external runtime framework dependencies and starts in under 60 milliseconds.
