# Changelog

## 0.3.0 - 2026-07-10

- Added a full Settings page with persistent app, privacy, appearance, notification, and Quick Access visibility preferences.
- Added Quick Access folder pinning, default restore actions, and Explorer Recent/Frequent visibility controls.
- Added system notification support with active/inactive operation-complete preferences and partial-failure alerts.
- Added Cmd/Ctrl+K command palette, appearance drawer, and configurable sidebar variants.
- Upgraded the Dashboard table with sorting, pagination, column visibility, and stable selection across pages.
- Added Dashboard overview chart and last-operation summary for Quick Access state.
- Completed five-language frontend coverage for Dashboard, Settings, ConfigDrawer, command palette, and About.

## 0.2.0 - 2026-07-07

- Added Dashboard mode with startup/window strategy, tray mode switching, and left-click open behavior.
- Added Quick Access browsing for recent files and frequent folders with counts, tabs, local search, and multi-select.
- Added Quick Access remove, clear, and open-location actions with confirmations, Toast feedback, refresh, and privacy-mode write guards.
- Added backend Quick Access IPC wrappers for list, counts, remove, clear, and Explorer reveal operations.
- Added runtime light/dark tray and window icon switching using theme-specific icons.
- Kept Minimal mode as the v0.1-style tray-only experience.

## 0.1.0 - 2026-07-02

- Added Windows tray shell with privacy mode, auto-start, language, about, and quit actions.
- Added Quick Access privacy locking with partial fallback and startup recovery alerts.
- Added light/dark tray icons with system theme polling.
- Added JSON configuration, logging, single-instance handling, and Windows-only build guard.
- Added About dialog with version, author, MIT license, and GitHub link.
- Added MSI production bundle configuration.
