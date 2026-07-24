use std::sync::Mutex;

use tauri::{LogicalSize, Manager, Runtime, WindowEvent};

use crate::config::{AppMode, CloseBehavior, Config};

pub(crate) fn install_close_handler<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let app_handle = app.clone();

    window.on_window_event(move |event| {
        if !matches!(event, WindowEvent::CloseRequested { .. }) {
            return;
        }

        let behavior = app_handle
            .try_state::<Mutex<Config>>()
            .and_then(|config| config.lock().ok().map(|config| config.close_behavior))
            .unwrap_or(CloseBehavior::Hide);
        if behavior != CloseBehavior::Hide {
            return;
        }

        let WindowEvent::CloseRequested { api, .. } = event else {
            return;
        };
        api.prevent_close();
        if let Some(window) = app_handle.get_webview_window("main") {
            if let Err(error) = window.hide() {
                log::warn!("failed to hide main window on close: {error}");
            }
        }
    });
}

pub(crate) fn show_dashboard<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    show_window(
        app,
        "#/",
        LogicalSize::new(1040.0, 720.0),
        LogicalSize::new(1040.0, 670.0),
        true,
    )
}

pub(crate) fn show_grid<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<(), tauri::Error> {
    show_window(
        app,
        "#/grid",
        LogicalSize::new(600.0, 400.0),
        LogicalSize::new(400.0, 320.0),
        true,
    )
}

fn show_window<R: Runtime>(
    app: &tauri::AppHandle<R>,
    route: &str,
    size: LogicalSize<f64>,
    min_size: LogicalSize<f64>,
    resizable: bool,
) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.unmaximize()?;
        window.set_decorations(false)?;
        window.set_resizable(resizable)?;
        window.set_min_size(Some(min_size))?;
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
