use tauri::{AppHandle, Manager, State};

use crate::{
    app::scheduler::AutoCleanMonitor,
    db::{DatabaseStatus, DbState},
    error::{CommandError, CommandResult, ErrorCode, ValidationError},
};

use super::ActionReceipt;

#[tauri::command]
pub(crate) fn get_database_status(database: State<'_, DbState>) -> CommandResult<DatabaseStatus> {
    Ok(database.status())
}

#[tauri::command]
pub(crate) fn retry_database(
    app: AppHandle,
    database: State<'_, DbState>,
) -> CommandResult<DatabaseStatus> {
    let status = database.retry();
    if status.available {
        if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
            if let Err(error) = monitor.trigger() {
                log::warn!(
                    "failed to trigger monitored auto-clean after database retry: {error:#}"
                );
            }
        }
    }
    Ok(status)
}

#[tauri::command]
pub(crate) fn open_database_directory(
    database: State<'_, DbState>,
) -> CommandResult<ActionReceipt> {
    let directory = database.directory().ok_or_else(|| {
        CommandError::expected(
            "open_database_directory",
            ErrorCode::DatabaseUnavailable,
            "The database directory is unavailable.",
            true,
            ValidationError::NotFound("database directory".to_string()),
        )
    })?;
    if !directory.is_dir() {
        return Err(CommandError::expected(
            "open_database_directory",
            ErrorCode::ResourceNotFound,
            "The database directory does not exist.",
            true,
            ValidationError::NotFound(directory.display().to_string()),
        ));
    }

    tauri_plugin_opener::open_path(&directory, None::<&str>).map_err(|error| {
        CommandError::unexpected(
            "open_database_directory",
            ErrorCode::SystemOperationFailed,
            "The database directory could not be opened.",
            true,
            error,
        )
    })?;
    Ok(ActionReceipt::new(
        "open_database_directory",
        directory.to_string_lossy(),
        1,
    ))
}
