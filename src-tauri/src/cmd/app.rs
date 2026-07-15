use std::sync::Mutex;

use tauri::{Manager, State};

use crate::{
    app::{settings, window},
    config::{AppMode, Config},
    db::DbState,
};

#[tauri::command]
pub(crate) fn get_config(config: State<'_, Mutex<Config>>) -> Result<Config, String> {
    Ok(config.lock().map_err(|error| error.to_string())?.clone())
}

#[tauri::command]
pub(crate) fn update_config(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    database: State<'_, DbState>,
    next_config: Config,
) -> Result<Config, String> {
    settings::update(&app, database.inner(), config.inner(), next_config)
}

#[tauri::command]
pub(crate) fn get_app_mode(config: State<'_, Mutex<Config>>) -> Result<AppMode, String> {
    Ok(config.lock().map_err(|error| error.to_string())?.app_mode)
}

#[tauri::command]
pub(crate) fn set_app_mode(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    mode: AppMode,
) -> Result<AppMode, String> {
    settings::set_app_mode(&app, config.inner(), mode)
}

#[tauri::command]
pub(crate) fn hide_about(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window_handle) = app.get_webview_window("main") {
        let mode = app
            .try_state::<Mutex<Config>>()
            .and_then(|config| config.lock().ok().map(|config| config.app_mode))
            .unwrap_or(AppMode::Minimal);
        if matches!(mode, AppMode::Dashboard) {
            window_handle
                .eval("window.location.hash = '#/'")
                .map_err(|error| error.to_string())?;
        } else {
            window::hide_main_window(&app).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn current_language(config: State<'_, Mutex<Config>>) -> Result<String, String> {
    Ok(config
        .lock()
        .map_err(|error| error.to_string())?
        .language
        .clone())
}
