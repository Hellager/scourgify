# Scourgify

Version: 0.6.2

Scourgify is a Windows-only tray app and dashboard for managing Windows Quick Access privacy and cleanup behavior. Its Quick Access integration uses `wincent`, which targets Windows Explorer Recent Files and Frequent Folders.

## Features

- Dashboard, nine-metric Grid, and tray-only modes.
- Privacy mode for Windows Quick Access recent files and frequent folders.
- Quick Access browsing, search, multi-select, typed item addition, remove, clear, open-location, restore-defaults, pin-state metadata, and visibility controls.
- Settings center for run mode, language, auto-start, privacy, appearance, notifications, and destructive-action preferences.
- System notifications for operation completion and partial failures.
- Cmd/Ctrl+K command palette and appearance drawer.
- Dashboard table sorting, pagination, and column visibility controls.
- Dashboard overview chart and latest operation summary.
- SQLite-backed whitelist and blacklist rules with file/folder/all scopes, filtering, pagination, cross-page selection, and protected and targeted item labels.
- Rule JSON import/export for all rules or selected rules, with preview and batch clearing.
- Rule-aware smart cleanup with an optional confirmation preview.
- Paginated cleanup-item and cleanup-run history with sorting, filtering, clearing, retention, rule snapshots, and streaming CSV/JSON exports.
- Permanent cleanup totals, retained-history trends, rule-hit rankings, and interrupted-run recovery.
- A monitored Quick Access cache that refreshes on Windows changes and drives monitor-mode automatic cleanup.
- Debug-only Mock Lab with isolated SQLite data and randomized Quick Access fixtures; production builds use only the Windows backend.
- Native localized relative times for recent-file interactions, with path-only fallback when metadata is unavailable.
- Five-language UI: English, Simplified Chinese, Traditional Chinese, French, and Russian.
- Runtime light/dark tray and window icons.

Rules, cleanup entries, cleanup runs, and lifetime totals are stored in `scourgify.db` under the application config directory. Database initialization failures leave Quick Access browsing available while protected cleanup, rules, history, and statistics remain disabled.

## Build

```powershell
pnpm install
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

The Windows MSI is emitted under `src-tauri/target/release/bundle/msi/`.
