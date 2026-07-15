use tauri::State;

use super::ensure_quick_access_write_allowed;
use crate::{
    db::{
        history::{
            self, CleanRecordPage, HistoryExportFormat, HistoryExportResult, HistoryFilter,
            HistoryQuery, Stats, StatsRange,
        },
        DbState,
    },
    privacy::PrivacyManager,
};

#[tauri::command]
pub(crate) fn get_clean_records(
    database: State<'_, DbState>,
    query: HistoryQuery,
) -> Result<CleanRecordPage, String> {
    database
        .with_connection(|connection| history::list(connection, query))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn export_clean_records(
    database: State<'_, DbState>,
    path: String,
    format: HistoryExportFormat,
    filter: HistoryFilter,
) -> Result<HistoryExportResult, String> {
    database
        .with_connection(|connection| history::export(connection, &path, format, filter))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn clear_clean_records(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
) -> Result<(), String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| history::clear(connection))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn get_stats(database: State<'_, DbState>, range: StatsRange) -> Result<Stats, String> {
    database
        .with_connection(|connection| history::stats(connection, range))
        .map_err(|error| error.to_string())
}
