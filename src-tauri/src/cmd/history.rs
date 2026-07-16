use tauri::State;

use crate::{
    db::history_runs::{CleanupRunFilter, CleanupRunPage, CleanupRunQuery},
    db::{
        history::{
            self, CleanRecordPage, HistoryError, HistoryExportFormat, HistoryExportResult,
            HistoryFilter, HistoryQuery, Stats, StatsRange,
        },
        DatabaseStateError, DbState,
    },
    error::{CommandError, CommandResult, ErrorCode},
};

#[tauri::command]
pub(crate) fn get_clean_records(
    database: State<'_, DbState>,
    query: HistoryQuery,
) -> CommandResult<CleanRecordPage> {
    database
        .with_connection(|connection| history::list(connection, query))
        .map_err(|error| history_error("get_clean_records", error))
}

#[tauri::command]
pub(crate) fn export_clean_records(
    database: State<'_, DbState>,
    path: String,
    format: HistoryExportFormat,
    filter: HistoryFilter,
) -> CommandResult<HistoryExportResult> {
    database
        .read_connection()
        .and_then(|connection| history::export(&connection, &path, format, filter))
        .map_err(|error| history_error("export_clean_records", error))
}

#[tauri::command]
pub(crate) fn get_cleanup_runs(
    database: State<'_, DbState>,
    query: CleanupRunQuery,
) -> CommandResult<CleanupRunPage> {
    database
        .with_connection(|connection| crate::db::history_runs::list(connection, query))
        .map_err(|error| history_error("get_cleanup_runs", error))
}

#[tauri::command]
pub(crate) fn export_cleanup_runs(
    database: State<'_, DbState>,
    path: String,
    format: HistoryExportFormat,
    filter: CleanupRunFilter,
) -> CommandResult<HistoryExportResult> {
    database
        .read_connection()
        .and_then(|connection| history::export_runs(&connection, &path, format, filter))
        .map_err(|error| history_error("export_cleanup_runs", error))
}

#[tauri::command]
pub(crate) fn clear_clean_records(
    database: State<'_, DbState>,
) -> CommandResult<history::HistoryClearResult> {
    database
        .with_connection(history::clear)
        .map_err(|error| history_error("clear_clean_records", error))
}

#[tauri::command]
pub(crate) fn get_stats(database: State<'_, DbState>, range: StatsRange) -> CommandResult<Stats> {
    database
        .with_connection(|connection| history::stats(connection, range))
        .map_err(|error| history_error("get_stats", error))
}

fn history_error(operation: &str, error: anyhow::Error) -> CommandError {
    let (code, message) = if error.downcast_ref::<DatabaseStateError>().is_some() {
        (
            ErrorCode::DatabaseUnavailable,
            "The cleanup database is unavailable.",
        )
    } else if error.downcast_ref::<HistoryError>().is_some() {
        (
            ErrorCode::ValidationInvalidArgument,
            "The history request is invalid.",
        )
    } else {
        (
            ErrorCode::InternalUnexpected,
            "The history operation could not be completed.",
        )
    };
    if code == ErrorCode::ValidationInvalidArgument {
        CommandError::expected(operation, code, message, false, error)
    } else {
        CommandError::unexpected(operation, code, message, true, error)
    }
}
