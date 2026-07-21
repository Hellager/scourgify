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

#[tauri::command]
pub(crate) fn clear_program_logs(app: tauri::AppHandle) -> CommandResult<ActionReceipt> {
    let directory = app.path().app_log_dir().map_err(|error| {
        CommandError::unexpected(
            "clear_program_logs",
            ErrorCode::SystemOperationFailed,
            "The program logs are unavailable.",
            true,
            error,
        )
    })?;
    let affected = clear_log_files(&directory).map_err(|error| {
        CommandError::unexpected(
            "clear_program_logs",
            ErrorCode::SystemOperationFailed,
            "The program logs could not be cleared.",
            true,
            anyhow::Error::new(error).context(format!(
                "failed to clear program logs in {}",
                directory.display()
            )),
        )
    })?;
    log::info!("program logs cleared files={affected}");
    Ok(ActionReceipt::new(
        "clear_program_logs",
        "program_logs",
        affected,
    ))
}

fn clear_log_files(directory: &std::path::Path) -> std::io::Result<u64> {
    if !directory.exists() {
        return Ok(0);
    }

    let mut affected = 0;
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if name == "scourgify.log" {
            std::fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&path)?;
            affected += 1;
        } else if name.starts_with("scourgify_")
            && (name.ends_with(".log") || name.ends_with(".log.bak"))
        {
            std::fs::remove_file(path)?;
            affected += 1;
        }
    }
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn clears_only_scourgify_log_files() {
        let directory = tempfile::tempdir().unwrap();
        let active_path = directory.path().join("scourgify.log");
        let mut active = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active_path)
            .unwrap();
        writeln!(active, "before clear").unwrap();
        std::fs::write(directory.path().join("scourgify_2026-07-21.log"), "old").unwrap();
        std::fs::write(directory.path().join("scourgify_2026-07-20.log.bak"), "old").unwrap();
        std::fs::write(directory.path().join("other.log"), "keep").unwrap();

        assert_eq!(clear_log_files(directory.path()).unwrap(), 3);
        assert_eq!(std::fs::metadata(&active_path).unwrap().len(), 0);
        assert!(!directory.path().join("scourgify_2026-07-21.log").exists());
        assert!(!directory
            .path()
            .join("scourgify_2026-07-20.log.bak")
            .exists());
        assert_eq!(
            std::fs::read_to_string(directory.path().join("other.log")).unwrap(),
            "keep"
        );

        writeln!(active, "after clear").unwrap();
        assert!(std::fs::read_to_string(active_path)
            .unwrap()
            .contains("after clear"));
    }

    #[test]
    fn clearing_missing_log_directory_is_a_noop() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing");

        assert_eq!(clear_log_files(&missing).unwrap(), 0);
    }
}
