use tauri::State;

use super::ensure_quick_access_write_allowed;
use crate::{
    db::{
        history::{
            self, CleanRecordPage, HistoryError, HistoryExportFormat, HistoryExportResult,
            HistoryFilter, HistoryQuery, Stats, StatsRange,
        },
        DatabaseStateError, DbState,
    },
    error::{CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
};

use super::ActionReceipt;

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
        .with_connection(|connection| history::export(connection, &path, format, filter))
        .map_err(|error| history_error("export_clean_records", error))
}

#[tauri::command]
pub(crate) fn clear_clean_records(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
) -> CommandResult<ActionReceipt> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let affected = database
        .with_connection(|connection| history::clear(connection))
        .map_err(|error| history_error("clear_clean_records", error))?;
    Ok(ActionReceipt::new(
        "clear_clean_records",
        "clean_records",
        affected,
    ))
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
