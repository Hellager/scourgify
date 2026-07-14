# Changelog

## 0.5.0 - Unreleased

- Added automatic cleanup schedules for startup, recurring hourly intervals, and daily fixed times, with runtime rescheduling and last-run tracking.
- Added a shared automatic cleanup service with privacy-mode, database-availability, and concurrency guards, plus aggregated history and notification results.
- Added automatic cleanup settings, immediate execution, localized completion feedback, and live Dashboard and History refresh events.
- Added CSV and JSON cleanup-history exports for all records or the active filters, with native save dialogs, result Toasts, and an open-folder action.
- Added Start Recommended recent-item visibility control with Explorer refresh, privacy protection, and Windows-state synchronization.
- Migrated cleanup history to schema v2 with manual/automatic source tracking and completed five-language coverage for v0.5.0 features.

## 0.4.5 - 2026-07-13

- Unified Dashboard, Rules, History, Settings, and About under a shared application shell and navigation model.
- Added server-side Rule and History search, filtering, date ranges, sorting, pagination, and filter-aware totals.
- Added ranged daily and weekly cleanup trends, rule-hit statistics, and Quick Access interaction metadata.
- Added runtime database diagnostics and recovery actions without requiring an application restart.
- Improved empty, loading, failure, dialog-focus, and narrow-layout states across database-backed pages.
- Hardened v0.4.0 behavior with focused regression tests and bumped application metadata to v0.4.5.

## 0.4.0 - 2026-07-13

- Added a bundled SQLite database with schema migration, built-in rule seeding, and persistent cleanup history.
- Added blacklist and whitelist rule CRUD, deterministic matching, conflict handling, and a dedicated Rules page.
- Unified protected Quick Access cleanup so whitelist matches are preserved and successful removals are recorded with rule snapshots.
- Added rule-aware Smart Clean previews and execution for Recent Files and Frequent Folders.
- Added paginated cleanup history with paths, item types, matched rules, timestamps, retention controls, and destructive-clear confirmation.
- Added cleanup totals, type distribution, trend charts, and rule-hit insights to the Dashboard.

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
