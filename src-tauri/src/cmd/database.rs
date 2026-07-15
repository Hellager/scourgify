use tauri::State;

use crate::db::{DatabaseStatus, DbState};

#[tauri::command]
pub(crate) fn get_database_status(database: State<'_, DbState>) -> DatabaseStatus {
    database.status()
}

#[tauri::command]
pub(crate) fn retry_database(database: State<'_, DbState>) -> DatabaseStatus {
    database.retry()
}

#[tauri::command]
pub(crate) fn open_database_directory(database: State<'_, DbState>) -> Result<(), String> {
    let directory = database
        .directory()
        .ok_or_else(|| "Database directory is unavailable.".to_string())?;
    if !directory.is_dir() {
        return Err(format!(
            "Database directory does not exist: {}",
            directory.display()
        ));
    }

    tauri_plugin_opener::open_path(&directory, None::<&str>).map_err(|error| error.to_string())
}
