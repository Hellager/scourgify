# Scourgify

Version: 0.2.0

Scourgify is a Windows-only tray app and lightweight dashboard for controlling and managing Quick Access privacy behavior. Its core integration uses `wincent`, which targets Windows Quick Access.

## Features

- Minimal tray mode and Dashboard mode.
- Light/dark runtime tray and window icons.
- Privacy mode for Windows Quick Access recent files and frequent folders.
- Partial-protection fallback with warnings.
- Quick Access browsing, search, selection, remove, clear, and open-location actions.
- Auto-start toggle.
- Five-language tray menu and About dialog: English, Simplified Chinese, Traditional Chinese, French, and Russian.
- About dialog with version, author, MIT license, and GitHub link.

## Build

```powershell
pnpm install
pnpm tauri build
```

The Windows MSI is emitted under `src-tauri/target/release/bundle/msi/`.
