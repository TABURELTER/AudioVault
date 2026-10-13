# AudioVault — Developer Guide

This document contains setup instructions, architecture guidelines, test procedures, and CI/CD deployment workflows for engineers working on the AudioVault codebase.

---

## 1. Prerequisites & Toolchain Setup

### 1.1 Rust & Node.js
AudioVault requires:
- **Rust 1.85 or later** (stable toolchain)
- **Node.js 20 or later** (with `npm`)

Verify your toolchains:
```powershell
rustc --version
cargo --version
node --version
npm --version
```

### 1.2 Platform Build Tools
- **Windows**: Microsoft Visual C++ (MSVC) Build Tools 2022 with the Windows 10/11 SDK and WebView2.
- **macOS**: Xcode Command Line Tools (`xcode-select --install`).
- **Linux (Ubuntu/Debian)**: Build essentials and WebKitGTK / Tauri 2 headers:
  ```bash
  sudo apt-get update
  sudo apt-get install -y \
    build-essential \
    libwebkit2gtk-4.1-dev \
    libappindicator3-dev \
    librsvg2-dev \
    patchelf \
    libfontconfig1-dev \
    libasound2-dev \
    libssl-dev \
    pkg-config
  ```

### 1.3 Media Toolchain (FFmpeg)
Audio transcode and M4B repack operations use `ffmpeg` and `ffprobe`.
- **System PATH**: Ensure `ffmpeg` and `ffprobe` are available on your system path, or configure the custom path in AudioVault Settings.

---

## 2. Repository Layout

```text
BookBase/
├── Cargo.toml                  # Cargo workspace definition with optimized profiles
├── Cargo.lock                  # Pinned dependency graph
├── ARCHITECTURE.md             # System architecture & technical specifications
├── DEVELOPMENT.md              # Engineering, testing, and deployment guide
├── README.md                   # Product overview and usage manual
├── .github/workflows/
│   ├── ci.yml                  # Cross-platform Frontend + Rust CI pipeline
│   └── release.yml             # Automatic Tauri bundler & GitHub Release workflow
│
├── apps/
│   └── desktop/                # Tauri 2 + React 19 + TypeScript desktop application
│       ├── package.json        # Frontend dependencies & scripts
│       ├── src/                # React 19 UI components, pages, hooks, IPC client
│       └── src-tauri/          # Tauri 2 Rust host, IPC commands, GitHub auto-updater
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
    ├── inspect/                # CLI catalog inspection and diagnostics tool
    └── seed/                   # Synthetic multi-volume catalog generator
```

---

## 3. Building & Running

### 3.1 Frontend Lint & Build
Before running `cargo check` or `cargo build` on `audiovault-desktop`, build the frontend dist bundle:
```powershell
cd apps/desktop
npm install
npm run lint
npm run build
cd ../..
```

### 3.2 Workspace Check & Running the Desktop App
```powershell
cargo check --workspace
cargo run -p audiovault-desktop
```

### 3.3 Running with Tracing Logs
Set the `RUST_LOG` environment variable for detailed subsystem diagnostics:
```powershell
$env:RUST_LOG="audiovault_desktop=debug,audiovault_service=debug,audiovault_db=info"
cargo run -p audiovault-desktop
```

### 3.4 Optimized Release Build
```powershell
cd apps/desktop
npm run tauri build
```

---

## 4. Test Suite & Validation

AudioVault contains an automated test suite covering domain logic, database operations, transliteration beam search, FB2 parsing, Chromaprint fingerprinting, and I/O scheduling.

### 4.1 Running All Workspace Tests
```powershell
cargo test --workspace
```

### 4.2 Running Tests for Specific Subsystems
```powershell
cargo test -p audiovault-core
cargo test -p audiovault-db
cargo test -p audiovault-platform
cargo test -p audiovault-media
cargo test -p audiovault-indexer
cargo test -p audiovault-torrent
cargo test -p audiovault-service
```

---

## 5. Continuous Integration & Auto-Release

- **CI (`.github/workflows/ci.yml`)**: Runs on every push and pull request across Windows, Ubuntu, and macOS. Validates frontend TypeScript types/linting, builds the frontend bundle, checks `cargo fmt`, `cargo clippy`, and runs `cargo test --workspace`.
- **Auto Build & Release (`.github/workflows/release.yml`)**: Automatically builds native installers and publishes a new GitHub Release on commits to `main`/`master` or `v*` version tags.
