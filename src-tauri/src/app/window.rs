use tauri::{LogicalSize, Manager, Runtime};

use crate::config::AppMode;

pub(crate) fn show_dashboard<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    show_window(app, "#/", LogicalSize::new(1040.0, 720.0), true)
}

pub(crate) fn show_grid<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    show_window(app, "#/grid", LogicalSize::new(560.0, 560.0), false)
}

fn show_window<R: Runtime>(
    app: &tauri::AppHandle<R>,
    route: &str,
    size: LogicalSize<f64>,
    resizable: bool,
) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.unmaximize()?;
        window.set_resizable(resizable)?;
        window.set_size(size)?;
        window.eval(format!("window.location.hash = '{route}'"))?;
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
        AppMode::Grid => show_grid(app),
        AppMode::Tray => hide_main_window(app),
    }
}
