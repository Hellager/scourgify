# Scourgify

Version: 0.3.0

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
- Five-language tray, About, Dashboard, Settings, drawer, and command UI: English, Simplified Chinese, Traditional Chinese, French, and Russian.
- Runtime light/dark tray and window icons.

## Build

```powershell
pnpm install
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

The Windows MSI is emitted under `src-tauri/target/release/bundle/msi/`.
