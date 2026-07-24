# Changelog

## 0.6.2 - 2026-07-24

- Added rule scopes for files, folders, or all targets, with localized rule editing and table filtering.
- Reworked the Rules center with TanStack Table, pagination, column filters, stable cross-page selection, and global deselection.
- Added versioned JSON rule import and export for all or selected rules, including paginated selective-import preview and validation.
- Added transactional batch clearing for all or selected rules with confirmation feedback.
- Added Mock Lab rule generation with randomized data and improved large-data pagination behavior.
- Fixed revealing exported files from the real filesystem while running in mock mode.

## 0.6.0 - 2026-07-16

- Reorganized the Rust backend by application, command, cleanup, configuration, database, and Quick Access responsibilities.
- Standardized all Tauri commands on structured success results and correlated `CommandError` responses, with stable error codes, incident IDs, complete local error chains, and detailed Wincent error mapping.
- Expanded Quick Access operations to add Recent Files or Frequent Folders by type, expose pin state and rich destination-list metadata, report post-mutation warnings correctly, improve restore reports, and enable deep `.lnk` cleanup by default.
- Added a three-second Quick Access watcher and shared cache that refreshes only after detected changes and notifies the frontend, while retaining explicit fresh reads.
- Redesigned configuration around Dashboard, Grid, and Tray modes; removed `sidebar_variant`; made unlimited history retention the default; and separated monitor-based from scheduled automatic cleanup.
- Added the read-only Grid summary command and nine-metric information view.
- Migrated SQLite to schema v3 with cleanup-run history, entry-to-run correlation, permanent lifetime totals, startup interruption recovery, filtered pagination, and streaming CSV/JSON exports through read-only connections and temporary files.
- Added Debug-only real/mock backend switching with isolated temporary SQLite storage, controllable Quick Access events, fixed-size randomized fixtures from `fake-rs`, and a frontend Mock Lab; release builds exclude Mock commands and UI.

### Breaking changes

- Tauri command error and success contracts are now structured and may require callers to update their response handling.
- Configuration no longer accepts `sidebar_variant`; legacy app modes and startup-only auto-clean values are migrated to supported values.
- `history_retention = 0` now explicitly means unlimited retention, while positive values retain only the latest records and runs.

## 0.5.0 - 2026-07-14

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
