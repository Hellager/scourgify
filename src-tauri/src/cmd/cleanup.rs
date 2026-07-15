use std::sync::Mutex;

use tauri::{AppHandle, State};

use super::{ensure_quick_access_write_allowed, history_retention};
use crate::{
    app::scheduler,
    cleanup::{self, AutoCleanError, AutoCleanResult, ClassifiedItem, CleanupError},
    config::Config,
    db::{history::CleanSource, DatabaseStateError, DbState},
    error::{CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
    quick_access::QaBatchResult,
};

#[tauri::command]
pub(crate) fn list_qa_items_classified(
    database: State<'_, DbState>,
    qa_type: String,
) -> CommandResult<Vec<ClassifiedItem>> {
    cleanup::list_classified(database.inner(), &qa_type)
        .map_err(|error| cleanup_error("list_qa_items_classified", error))
}

#[tauri::command]
pub(crate) fn remove_qa_items(
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
    paths: Vec<String>,
) -> CommandResult<QaBatchResult> {
    ensure_quick_access_write_allowed(privacy.state())?;
    cleanup::remove_selected(
        database.inner(),
        &qa_type,
        paths,
        history_retention(&config)?,
    )
    .map_err(|error| cleanup_error("remove_qa_items", error))
}

#[tauri::command]
pub(crate) fn empty_qa_items(
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> CommandResult<QaBatchResult> {
    ensure_quick_access_write_allowed(privacy.state())?;
    cleanup::empty_current(database.inner(), &qa_type, history_retention(&config)?)
        .map_err(|error| cleanup_error("empty_qa_items", error))
}

#[tauri::command]
pub(crate) fn smart_clean(
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> CommandResult<QaBatchResult> {
    ensure_quick_access_write_allowed(privacy.state())?;
    cleanup::smart_clean(
        database.inner(),
        &qa_type,
        history_retention(&config)?,
        CleanSource::Manual,
    )
    .map_err(|error| cleanup_error("smart_clean", error))
}

#[tauri::command]
pub(crate) fn run_auto_clean_now(app: AppHandle) -> CommandResult<AutoCleanResult> {
    scheduler::run_now(&app).map_err(|error| {
        let (code, message, expected) = match error.downcast_ref::<AutoCleanError>() {
            Some(AutoCleanError::AlreadyRunning) => (
                ErrorCode::AutoCleanAlreadyRunning,
                "Automatic cleanup is already running.",
                true,
            ),
            Some(AutoCleanError::DatabaseUnavailable) => (
                ErrorCode::DatabaseUnavailable,
                "Automatic cleanup requires an available database.",
                true,
            ),
            Some(AutoCleanError::PrivacyModeActive) => (
                ErrorCode::PrivacyWriteBlocked,
                "Automatic cleanup is unavailable while privacy mode is active.",
                true,
            ),
            Some(AutoCleanError::StateUnavailable) | None => (
                ErrorCode::AutoCleanUnavailable,
                "Automatic cleanup could not be completed.",
                false,
            ),
        };
        if expected {
            CommandError::expected("run_auto_clean_now", code, message, true, error)
        } else {
            CommandError::unexpected("run_auto_clean_now", code, message, true, error)
        }
    })
}

fn cleanup_error(operation: &str, error: anyhow::Error) -> CommandError {
    let (code, message) = if error.downcast_ref::<DatabaseStateError>().is_some() {
        (
            ErrorCode::DatabaseUnavailable,
            "The cleanup database is unavailable.",
        )
    } else if error.downcast_ref::<CleanupError>().is_some() {
        (
            ErrorCode::ValidationInvalidArgument,
            "The requested Quick Access type is invalid.",
        )
    } else {
        (
            ErrorCode::QuickAccessOperationFailed,
            "The cleanup operation could not be completed.",
        )
    };
    CommandError::unexpected(operation, code, message, true, error)
}
