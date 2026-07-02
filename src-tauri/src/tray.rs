use std::sync::Mutex;

use anyhow::Result;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime,
};

use crate::{
    config::Config,
    privacy::{PrivacyManager, PrivacyModeState},
    theme,
};

const PRIVACY_MODE_ID: &str = "privacy-mode";
const QUIT_ID: &str = "quit";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let privacy = app.state::<PrivacyManager>();
    let privacy_mode = CheckMenuItem::with_id(
        app,
        PRIVACY_MODE_ID,
        "Privacy Mode",
        true,
        is_privacy_enabled(privacy.state()),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&privacy_mode, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    TrayIconBuilder::with_id(theme::MAIN_TRAY_ID)
        .icon(theme::current_tray_icon()?)
        .tooltip("Scourgify")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            PRIVACY_MODE_ID => toggle_privacy_mode(app, &privacy_mode),
            QUIT_ID => quit_app(app),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

fn toggle_privacy_mode<R: Runtime>(app: &AppHandle<R>, menu_item: &CheckMenuItem<R>) {
    let privacy = app.state::<PrivacyManager>();
    let config = app.state::<Mutex<Config>>();

    let result = if is_privacy_enabled(privacy.state()) {
        privacy.exit().map(|reports| {
            for report in reports {
                log::debug!(
                    "privacy unlock report: new={}, deleted={}, failed={}",
                    report.new_lnk_paths().len(),
                    report.deleted_lnk_paths().len(),
                    report.failed_lnk_deletions().len()
                );
            }
        })
    } else {
        privacy.enter().map(|result| {
            log::info!("privacy mode entered: {result:?}");
        })
    };

    match result {
        Ok(()) => {
            let enabled = is_privacy_enabled(privacy.state());
            if let Err(error) = crate::persist_privacy_mode(app, &config, enabled) {
                log::error!("failed to persist privacy mode: {error}");
            }
            if let Err(error) = menu_item.set_checked(enabled) {
                log::warn!("failed to update privacy menu item: {error}");
            }
        }
        Err(error) => {
            log::error!("failed to toggle privacy mode: {error}");
            if let Err(error) = menu_item.set_checked(is_privacy_enabled(privacy.state())) {
                log::warn!("failed to restore privacy menu item: {error}");
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
