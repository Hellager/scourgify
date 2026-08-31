use tauri::{AppHandle, Manager};

use super::ActionReceipt;

#[cfg(debug_assertions)]
use crate::mock::commands::{apply_overview_request, MockOverviewTarget};
use crate::{
    db::history::runs::{CleanupRunFilter, CleanupRunPage, CleanupRunQuery},
    db::{
        history::{
            self, CleanRecordPage, HistoryError, HistoryExportFormat, HistoryExportResult,
            HistoryFilter, HistoryQuery, Stats, StatsRange,
        },
        DatabaseStateError, DbState,
    },
    error::{CommandError, CommandResult, ErrorCode},
};

use super::run_blocking;

#[tauri::command]
pub(crate) async fn get_clean_records(
    app: AppHandle,
    query: HistoryQuery,
) -> CommandResult<CleanRecordPage> {
    run_blocking("get_clean_records", move || {
        app.state::<DbState>()
            .with_connection(|connection| history::list(connection, query))
            .map_err(|error| history_error("get_clean_records", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_clean_records(
    app: AppHandle,
    path: String,
    format: HistoryExportFormat,
    filter: HistoryFilter,
) -> CommandResult<HistoryExportResult> {
    run_blocking("export_clean_records", move || {
        app.state::<DbState>()
            .read_connection()
            .and_then(|connection| history::export(&connection, &path, format, filter))
            .map_err(|error| history_error("export_clean_records", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn get_cleanup_runs(
    app: AppHandle,
    query: CleanupRunQuery,
) -> CommandResult<CleanupRunPage> {
    run_blocking("get_cleanup_runs", move || {
        app.state::<DbState>()
            .with_connection(|connection| crate::db::history::runs::list(connection, query))
            .map_err(|error| history_error("get_cleanup_runs", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn export_cleanup_runs(
    app: AppHandle,
    path: String,
    format: HistoryExportFormat,
    filter: CleanupRunFilter,
) -> CommandResult<HistoryExportResult> {
    run_blocking("export_cleanup_runs", move || {
        app.state::<DbState>()
            .read_connection()
            .and_then(|connection| history::export_runs(&connection, &path, format, filter))
            .map_err(|error| history_error("export_cleanup_runs", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn clear_clean_records(
    app: AppHandle,
) -> CommandResult<history::HistoryClearResult> {
    let result = run_blocking("clear_clean_records", move || {
        app.state::<DbState>()
            .with_connection(history::clear)
            .map_err(|error| history_error("clear_clean_records", error))
    })
    .await?;
    log::info!(
        "cleanup history cleared records={} runs={}",
        result.clean_records,
        result.cleanup_runs
    );
    Ok(result)
}

#[tauri::command]
pub(crate) async fn remove_clean_record(app: AppHandle, id: i64) -> CommandResult<ActionReceipt> {
    run_blocking("remove_clean_record", move || {
        app.state::<DbState>()
            .with_connection(|connection| history::remove(connection, id))
            .map_err(|error| history_error("remove_clean_record", error))
    })
    .await?;
    Ok(ActionReceipt::new("remove_clean_record", id.to_string(), 1))
}

#[tauri::command]
pub(crate) async fn set_clean_record_visibility(
    app: AppHandle,
    id: i64,
    visible: bool,
) -> CommandResult<ActionReceipt> {
    run_blocking("set_clean_record_visibility", move || {
        app.state::<DbState>()
            .with_connection(|connection| history::set_visible(connection, id, visible))
            .map_err(|error| history_error("set_clean_record_visibility", error))
    })
    .await?;
    Ok(ActionReceipt::new(
        "set_clean_record_visibility",
        id.to_string(),
        1,
    ))
}

#[tauri::command]
pub(crate) async fn restore_hidden_clean_records(app: AppHandle) -> CommandResult<ActionReceipt> {
    let affected = run_blocking("restore_hidden_clean_records", move || {
        app.state::<DbState>()
            .with_connection(|connection| history::restore_hidden(connection))
            .map_err(|error| history_error("restore_hidden_clean_records", error))
    })
    .await?;
    Ok(ActionReceipt::new(
        "restore_hidden_clean_records",
        "hidden",
        affected,
    ))
}

#[tauri::command]
pub(crate) async fn get_stats(app: AppHandle, range: StatsRange) -> CommandResult<Stats> {
    run_blocking("get_stats", move || {
        #[cfg(debug_assertions)]
        apply_overview_request(
            app.state::<crate::quick_access::QuickAccessBackendState>()
                .inner(),
            MockOverviewTarget::Stats,
            "get_stats",
        )?;
        app.state::<DbState>()
            .with_connection(|connection| history::stats(connection, range))
            .map_err(|error| history_error("get_stats", error))
    })
    .await
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
