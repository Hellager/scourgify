use tauri::{Manager, Runtime};

use crate::config::AppMode;

pub(crate) fn show_dashboard<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.eval("window.location.hash = '#/'")?;
        window.center()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

pub(crate) fn show_about<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.eval("window.location.hash = '#/about'")?;
        window.center()?;
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}

pub(crate) fn hide_main_window<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide()?;
    }
    Ok(())
}

pub(crate) fn apply_strategy<R: Runtime>(
    app: &tauri::AppHandle<R>,
    mode: AppMode,
) -> Result<(), tauri::Error> {
    match mode {
        AppMode::Dashboard => show_dashboard(app),
        AppMode::Minimal => hide_main_window(app),
    }
}
