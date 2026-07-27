use tauri::State;

use super::ActionReceipt;

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
    let result = database
        .with_connection(history::clear)
        .map_err(|error| history_error("clear_clean_records", error))?;
    log::info!(
        "cleanup history cleared records={} runs={}",
        result.clean_records,
        result.cleanup_runs
    );
    Ok(result)
}

#[tauri::command]
pub(crate) fn remove_clean_record(
    database: State<'_, DbState>,
    id: i64,
) -> CommandResult<ActionReceipt> {
    database
        .with_connection(|connection| history::remove(connection, id))
        .map_err(|error| history_error("remove_clean_record", error))?;
    Ok(ActionReceipt::new("remove_clean_record", id.to_string(), 1))
}

#[tauri::command]
pub(crate) fn set_clean_record_visibility(
    database: State<'_, DbState>,
    id: i64,
    visible: bool,
) -> CommandResult<ActionReceipt> {
    database
        .with_connection(|connection| history::set_visible(connection, id, visible))
        .map_err(|error| history_error("set_clean_record_visibility", error))?;
    Ok(ActionReceipt::new(
        "set_clean_record_visibility",
        id.to_string(),
        1,
    ))
}

#[tauri::command]
pub(crate) fn restore_hidden_clean_records(
    database: State<'_, DbState>,
) -> CommandResult<ActionReceipt> {
    let affected = database
        .with_connection(|connection| history::restore_hidden(connection))
        .map_err(|error| history_error("restore_hidden_clean_records", error))?;
    Ok(ActionReceipt::new(
        "restore_hidden_clean_records",
        "hidden",
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
    } else if matches!(
        error.downcast_ref::<HistoryError>(),
        Some(HistoryError::NotFound(_))
    ) {
        (
            ErrorCode::ResourceNotFound,
            "The requested history record was not found.",
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
    if matches!(
        code,
        ErrorCode::ResourceNotFound | ErrorCode::ValidationInvalidArgument
    ) {
        CommandError::expected(operation, code, message, false, error)
    } else {
        CommandError::unexpected(operation, code, message, true, error)
    }
}
