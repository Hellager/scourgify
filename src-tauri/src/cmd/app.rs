use std::sync::Mutex;

use tauri::{Manager, State};

use crate::{
    app::{settings, window},
    config::{AppMode, Config},
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
};

use super::{state_error, ActionReceipt};

#[tauri::command]
pub(crate) fn get_config(config: State<'_, Mutex<Config>>) -> CommandResult<Config> {
    config
        .lock()
        .map(|config| config.clone())
        .map_err(|error| state_error("get_config", error))
}

#[tauri::command]
pub(crate) fn update_config(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    database: State<'_, DbState>,
    next_config: Config,
) -> CommandResult<Config> {
    settings::update(&app, database.inner(), config.inner(), next_config).map_err(|error| {
        CommandError::unexpected(
            "update_config",
            ErrorCode::ConfigPersistenceFailed,
            "The application settings could not be saved.",
            true,
            error,
        )
    })
}

#[tauri::command]
pub(crate) fn get_app_mode(config: State<'_, Mutex<Config>>) -> CommandResult<AppMode> {
    config
        .lock()
        .map(|config| config.app_mode)
        .map_err(|error| state_error("get_app_mode", error))
}

#[tauri::command]
pub(crate) fn set_app_mode(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    mode: AppMode,
) -> CommandResult<AppMode> {
    settings::set_app_mode(&app, config.inner(), mode).map_err(|error| {
        CommandError::unexpected(
            "set_app_mode",
            ErrorCode::ConfigPersistenceFailed,
            "The application mode could not be changed.",
            true,
            error,
        )
    })
}

#[tauri::command]
pub(crate) fn hide_about(app: tauri::AppHandle) -> CommandResult<ActionReceipt> {
    if app.get_webview_window("main").is_some() {
        let mode = app
            .try_state::<Mutex<Config>>()
            .and_then(|config| config.lock().ok().map(|config| config.app_mode))
            .unwrap_or(AppMode::Tray);
        let result = match mode {
            AppMode::Dashboard => window::show_dashboard(&app),
            AppMode::Grid => window::show_grid(&app),
            AppMode::Tray => window::hide_main_window(&app),
        };
        result.map_err(|error| {
            CommandError::unexpected(
                "hide_about",
                ErrorCode::SystemOperationFailed,
                "The About view could not be closed.",
                true,
                error,
            )
        })?;
    }
    Ok(ActionReceipt::new("hide_about", "main", 1))
}

#[tauri::command]
pub(crate) fn current_language(config: State<'_, Mutex<Config>>) -> CommandResult<String> {
    config
        .lock()
        .map(|config| config.language.clone())
        .map_err(|error| state_error("current_language", error))
}
