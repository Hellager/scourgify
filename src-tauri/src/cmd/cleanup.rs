use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use super::{begin_quick_access_write, run_blocking};
use crate::{
    app::scheduler,
    backend::QuickAccessBackendState,
    cleanup::{self, AutoCleanError, AutoCleanResult, ClassifiedItem, CleanupError},
    config::Config,
    db::{history::CleanSource, DatabaseStateError, DbState},
    error::{wincent_command_error, CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
    quick_access::{QaBatchResult, QuickAccessCache},
};

#[tauri::command]
pub(crate) async fn list_qa_items_classified(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    backend: State<'_, QuickAccessBackendState>,
    qa_type: String,
    fresh: Option<bool>,
) -> CommandResult<Vec<ClassifiedItem>> {
    let cache = cache.inner().clone();
    let backend = backend.inner().clone();
    run_blocking("list_qa_items_classified", move || {
        cleanup::validate_list_type(&qa_type)
            .map_err(|error| cleanup_error("list_qa_items_classified", error))?;
        let items = cache
            .items(&app, &backend, &qa_type, fresh.unwrap_or(false))
            .map_err(|error| cleanup_error("list_qa_items_classified", error))?;
        let database = app.state::<DbState>();
        cleanup::list_classified(database.inner(), &qa_type, items)
            .map_err(|error| cleanup_error("list_qa_items_classified", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn remove_qa_items(
    app: AppHandle,
    qa_type: String,
    paths: Vec<String>,
) -> CommandResult<QaBatchResult> {
    run_blocking("remove_qa_items", move || {
        let cache = app.state::<QuickAccessCache>();
        let backend = app.state::<QuickAccessBackendState>();
        let database = app.state::<DbState>();
        let privacy = app.state::<PrivacyManager>();
        let config = app.state::<Mutex<Config>>();
        let _write_permit = begin_quick_access_write(privacy.inner())?;
        let history_retention = config
            .lock()
            .map_err(|error| super::state_error("read_history_retention", error))?
            .history_retention;
        let result = cleanup::remove_selected(
            database.inner(),
            backend.inner(),
            &qa_type,
            paths,
            history_retention,
        )
        .map_err(|error| cleanup_error("remove_qa_items", error))?;
        cache.refresh_after_write(&app, backend.inner(), &qa_type);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub(crate) async fn empty_qa_items(
    app: AppHandle,
    qa_type: String,
) -> CommandResult<QaBatchResult> {
    run_blocking("empty_qa_items", move || {
        let cache = app.state::<QuickAccessCache>();
        let backend = app.state::<QuickAccessBackendState>();
        let database = app.state::<DbState>();
        let privacy = app.state::<PrivacyManager>();
        let config = app.state::<Mutex<Config>>();
        let _write_permit = begin_quick_access_write(privacy.inner())?;
        let history_retention = config
            .lock()
            .map_err(|error| super::state_error("read_history_retention", error))?
            .history_retention;
        let result = cleanup::empty_current(
            database.inner(),
            backend.inner(),
            &qa_type,
            history_retention,
        )
        .map_err(|error| cleanup_error("empty_qa_items", error))?;
        cache.refresh_after_write(&app, backend.inner(), &qa_type);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub(crate) async fn smart_clean(app: AppHandle, qa_type: String) -> CommandResult<QaBatchResult> {
    run_blocking("smart_clean", move || {
        let cache = app.state::<QuickAccessCache>();
        let backend = app.state::<QuickAccessBackendState>();
        let database = app.state::<DbState>();
        let privacy = app.state::<PrivacyManager>();
        let config = app.state::<Mutex<Config>>();
        let _write_permit = begin_quick_access_write(privacy.inner())?;
        let history_retention = config
            .lock()
            .map_err(|error| super::state_error("read_history_retention", error))?
            .history_retention;
        let result = cleanup::smart_clean(
            database.inner(),
            backend.inner(),
            &qa_type,
            history_retention,
            CleanSource::Manual,
        )
        .map_err(|error| cleanup_error("smart_clean", error))?;
        cache.refresh_after_write(&app, backend.inner(), &qa_type);
        Ok(result)
    })
    .await
}

#[tauri::command]
pub(crate) async fn run_auto_clean_now(app: AppHandle) -> CommandResult<AutoCleanResult> {
    run_blocking("run_auto_clean_now", move || {
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
    })
    .await
}

fn cleanup_error(operation: &str, error: anyhow::Error) -> CommandError {
    if error.downcast_ref::<DatabaseStateError>().is_some() {
        CommandError::unexpected(
            operation,
            ErrorCode::DatabaseUnavailable,
            "The cleanup database is unavailable.",
            true,
            error,
        )
    } else if error.downcast_ref::<CleanupError>().is_some() {
        CommandError::expected(
            operation,
            ErrorCode::ValidationInvalidArgument,
            "The requested Quick Access type is invalid.",
            false,
            error,
        )
    } else {
        wincent_command_error(operation, error)
    }
}
