# AudioVault desktop UI

The existing Tauri 2 / React 19 desktop client. Russian is the interface language.

## Run and build

From the repository root:

```powershell
npm --prefix apps/desktop run build
cargo build -p audiovault-desktop
target/debug/audiovault-desktop.exe
```

`npm --prefix apps/desktop run lint` checks the frontend. The build includes TypeScript checking. `cargo test --workspace` runs the existing Rust tests.

## Frontend structure

- `src/index.css`: graphite theme, semantic colors, typography, spacing, radii, responsive layouts, focus and reduced-motion rules.
- `src/components/Primitives.tsx`: search, covers, availability, loading, empty states, progress, and fixed-height virtualization.
- `src/components/BookList.tsx`: shared collection list, virtual cover shelves, keyboard navigation, and book context menu.
- `src/components/Modal.tsx`: keyboard focus trapping and focus restoration.
- `src/hooks/useLibrary.ts`: debounced catalog requests, paged loading, and stale-response protection.
- `src/pages`: collection, ebook catalog, search, storage, comparison, inbox, activity, download and settings views.
- `src/api`: typed IPC and Russian presentation labels. Filesystem operations continue to use the Rust service.

Collections fetch lightweight pages: 250 audiobooks or 100 ebooks. Covers are requested only for mounted items, with a bounded shared cache. Inspectors fetch details on selection. The metadata inbox uses the service's existing category filtering and paging.

## Keyboard

Ctrl K focuses global library search; Ctrl F focuses the current page's search when available. Arrow keys navigate audiobook rows; Enter opens details; Escape closes details, menus, and dialogs. Context menus and modal dialogs support keyboard focus.

## Runtime visual checks

These are optional local inspection tools, not dependencies of the shipped app. They require Python with `websocket-client` and Pillow. Opt into WebView debugging only while running QA:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9223'
target/debug/audiovault-desktop.exe
python tools/ui_validate.py
python tools/ui_native.py
```

`ui_validate.py` exercises the real catalog through the actual Tauri WebView and saves screenshots under `artifacts/ui`. `ui_native.py` checks native minimize/maximize/restore and captures the Windows window. `ui_states.py` uses a separate Edge preview on port 9224 and the Vite server on port 5173, with in-memory IPC fixtures for mutation and stress tests; it does not write to the real catalog.

The source headphone mark lives in `public/favicon.svg`. `tools/ui_icons.py` exports matching native packaging icons.
