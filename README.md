# Scourgify

Version: 0.4.5

Scourgify is a Windows-only tray app and dashboard for managing Windows Quick Access privacy and cleanup behavior. Its Quick Access integration uses `wincent`, which targets Windows Explorer Recent Files and Frequent Folders.

## Features

- Minimal tray mode and Dashboard mode.
- Privacy mode for Windows Quick Access recent files and frequent folders.
- Quick Access browsing, search, multi-select, remove, clear, open-location, pin-folder, restore-defaults, and visibility controls.
- Settings center for run mode, language, auto-start, privacy, appearance, notifications, and destructive-action preferences.
- System notifications for operation completion and partial failures.
- Cmd/Ctrl+K command palette, appearance drawer, and configurable sidebar style.
- Dashboard table sorting, pagination, and column visibility controls.
- Dashboard overview chart and latest operation summary.
- SQLite-backed whitelist and blacklist rules with protected and targeted item labels.
- Rule-aware smart cleanup with an optional confirmation preview.
- Paginated cleanup history with sorting, clearing, retention, and rule snapshots.
- Cleanup totals, daily or weekly trends, and rule-hit rankings alongside the existing current-state charts.
- Native localized relative times for recent-file interactions, with path-only fallback when metadata is unavailable.
- Five-language UI: English, Simplified Chinese, Traditional Chinese, French, and Russian.
- Runtime light/dark tray and window icons.

Rules and cleanup history are stored in `scourgify.db` under the application config directory. Database initialization failures leave Quick Access browsing available while protected cleanup, rules, history, and statistics remain disabled.

## Build

```powershell
pnpm install
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

The Windows MSI is emitted under `src-tauri/target/release/bundle/msi/`.
