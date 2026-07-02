use std::sync::Mutex;

use anyhow::Result;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

use crate::{
    alert,
    config::Config,
    privacy::{LockResult, PrivacyManager, PrivacyModeState},
    theme,
};

const PRIVACY_MODE_ID: &str = "privacy-mode";
const AUTO_START_ID: &str = "auto-start";
const ABOUT_ID: &str = "about";
const QUIT_ID: &str = "quit";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let privacy = app.state::<PrivacyManager>();
    let privacy_requested = app
        .state::<Mutex<Config>>()
        .lock()
        .map(|config| config.privacy_mode)
        .unwrap_or(false);
    let privacy_state = privacy.state();
    let privacy_mode = CheckMenuItem::with_id(
        app,
        PRIVACY_MODE_ID,
        privacy_label(privacy_state),
        true,
        privacy_requested || is_privacy_enabled(privacy_state),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let about = MenuItem::with_id(app, ABOUT_ID, "About", true, None::<&str>)?;
    let auto_start = CheckMenuItem::with_id(
        app,
        AUTO_START_ID,
        "Auto Start",
        true,
        app.autolaunch().is_enabled().unwrap_or_else(|error| {
            log::warn!("failed to read autostart state: {error}");
            app.state::<Mutex<Config>>()
                .lock()
                .map(|config| config.auto_start)
                .unwrap_or(false)
        }),
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &privacy_mode,
            &auto_start,
            &PredefinedMenuItem::separator(app)?,
            &about,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(theme::MAIN_TRAY_ID)
        .icon(theme::current_tray_icon()?)
        .tooltip("Scourgify")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            PRIVACY_MODE_ID => toggle_privacy_mode(app, &privacy_mode),
            AUTO_START_ID => toggle_auto_start(app, &auto_start),
            ABOUT_ID => show_about(app),
            QUIT_ID => quit_app(app),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn show_about<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.center() {
            log::warn!("failed to center about window: {error}");
        }
        if let Err(error) = window.show() {
            log::error!("failed to show about window: {error}");
        }
        if let Err(error) = window.set_focus() {
            log::warn!("failed to focus about window: {error}");
        }
    }
}

fn toggle_auto_start<R: Runtime>(app: &AppHandle<R>, menu_item: &CheckMenuItem<R>) {
    let manager = app.autolaunch();
    let enabled = manager.is_enabled().unwrap_or(false);
    let result = if enabled {
        manager.disable()
    } else {
        manager.enable()
    };

    match result {
        Ok(()) => {
            let enabled = manager.is_enabled().unwrap_or(!enabled);
            if let Some(config) = app.try_state::<Mutex<Config>>() {
                if let Err(error) = crate::persist_auto_start(app, &config, enabled) {
                    log::error!("failed to persist autostart state: {error}");
                }
            }
            if let Err(error) = menu_item.set_checked(enabled) {
                log::warn!("failed to update autostart menu item: {error}");
            }
        }
        Err(error) => {
            log::error!("failed to toggle autostart: {error}");
            if let Err(error) = menu_item.set_checked(enabled) {
                log::warn!("failed to restore autostart menu item: {error}");
            }
        }
    }
}

fn toggle_privacy_mode<R: Runtime>(app: &AppHandle<R>, menu_item: &CheckMenuItem<R>) {
    let privacy = app.state::<PrivacyManager>();
    let config = app.state::<Mutex<Config>>();
    let requested = config
        .lock()
        .map(|config| config.privacy_mode)
        .unwrap_or_else(|error| {
            log::error!("failed to read privacy mode config: {error}");
            is_privacy_enabled(privacy.state())
        });

    let result = if requested {
        privacy.exit().map(|reports| {
            for report in reports {
                log::debug!(
                    "privacy unlock report: new={}, deleted={}, failed={}",
                    report.new_lnk_paths().len(),
                    report.deleted_lnk_paths().len(),
                    report.failed_lnk_deletions().len()
                );
            }
            LockResult::Full
        })
    } else {
        privacy.enter().map(|result| {
            log::info!("privacy mode entered: {result:?}");
            if matches!(result, LockResult::Partial) {
                alert::warning(
                    app,
                    "Scourgify",
                    "Privacy mode is active with partial protection.",
                );
            }
            result
        })
    };

    match result {
        Ok(_) => {
            let enabled = !requested;
            if let Err(error) = crate::persist_privacy_mode(app, &config, enabled) {
                log::error!("failed to persist privacy mode: {error}");
            }
            if let Err(error) = menu_item.set_checked(enabled) {
                log::warn!("failed to update privacy menu item: {error}");
            }
            if let Err(error) = menu_item.set_text(privacy_label(privacy.state())) {
                log::warn!("failed to update privacy menu text: {error}");
            }
        }
        Err(error) => {
            log::error!("failed to toggle privacy mode: {error}");
            alert::warning(
                app,
                "Scourgify",
                &format!("Failed to toggle privacy mode.\n\n{error}"),
            );
            if let Err(error) =
                menu_item.set_checked(requested || is_privacy_enabled(privacy.state()))
            {
                log::warn!("failed to restore privacy menu item: {error}");
            }
            if let Err(error) = menu_item.set_text(privacy_label(privacy.state())) {
                log::warn!("failed to restore privacy menu text: {error}");
            }
        }
    }
}

fn quit_app<R: Runtime>(app: &AppHandle<R>) {
    if let Some(privacy) = app.try_state::<PrivacyManager>() {
        match privacy.exit() {
            Ok(_) => {
                if let Some(config) = app.try_state::<Mutex<Config>>() {
                    if let Err(error) = crate::persist_privacy_mode(app, &config, false) {
                        log::error!("failed to persist privacy mode before quit: {error}");
                    }
                }
            }
            Err(error) => log::error!("failed to exit privacy mode before quit: {error}"),
        }
    }

    app.exit(0);
}

fn is_privacy_enabled(state: PrivacyModeState) -> bool {
    !matches!(state, PrivacyModeState::Inactive)
}

fn privacy_label(state: PrivacyModeState) -> &'static str {
    match state {
        PrivacyModeState::ActivePartial { .. } => "Privacy Mode (Partial)",
        _ => "Privacy Mode",
    }
}
