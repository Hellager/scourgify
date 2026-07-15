use serde::Serialize;
use tauri::Manager;

use super::ActionReceipt;
use crate::error::{CommandError, CommandResult, ErrorCode};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct LogDirectoryStatus {
    pub path: String,
    pub exists: bool,
}

#[tauri::command]
pub(crate) fn get_log_directory_status(app: tauri::AppHandle) -> CommandResult<LogDirectoryStatus> {
    let directory = app.path().app_log_dir().map_err(|error| {
        CommandError::unexpected(
            "get_log_directory_status",
            ErrorCode::SystemOperationFailed,
            "The log directory is unavailable.",
            true,
            error,
        )
    })?;
    Ok(LogDirectoryStatus {
        exists: directory.is_dir(),
        path: directory.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub(crate) fn open_log_directory(app: tauri::AppHandle) -> CommandResult<ActionReceipt> {
    let directory = app.path().app_log_dir().map_err(|error| {
        CommandError::unexpected(
            "open_log_directory",
            ErrorCode::SystemOperationFailed,
            "The log directory is unavailable.",
            true,
            error,
        )
    })?;
    std::fs::create_dir_all(&directory).map_err(|error| {
        CommandError::unexpected(
            "open_log_directory",
            ErrorCode::SystemOperationFailed,
            "The log directory could not be created.",
            true,
            anyhow::Error::new(error).context(format!(
                "failed to create log directory {}",
                directory.display()
            )),
        )
    })?;
    tauri_plugin_opener::open_path(&directory, None::<&str>).map_err(|error| {
        CommandError::unexpected(
            "open_log_directory",
            ErrorCode::SystemOperationFailed,
            "The log directory could not be opened.",
            true,
            error,
        )
    })?;
    Ok(ActionReceipt::new(
        "open_log_directory",
        directory.to_string_lossy(),
        1,
    ))
}
