use std::sync::Mutex;

use anyhow::Result;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, Runtime,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

use crate::{
    alert,
    config::{self, Config},
    i18n,
    privacy::{LockResult, PrivacyManager, PrivacyModeState},
    theme,
};

const PRIVACY_MODE_ID: &str = "privacy-mode";
const AUTO_START_ID: &str = "auto-start";
const ABOUT_ID: &str = "about";
const QUIT_ID: &str = "quit";
const LANGUAGE_PREFIX: &str = "language:";
const LANGUAGE_CHANGED_EVENT: &str = "language-changed";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let menu = build_menu(app)?;

    TrayIconBuilder::with_id(theme::MAIN_TRAY_ID)
        .icon(theme::current_tray_icon()?)
        .tooltip("Scourgify")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            if let Some(language) = id.strip_prefix(LANGUAGE_PREFIX) {
                set_language(app, language);
                return;
            }

            match id {
                PRIVACY_MODE_ID => toggle_privacy_mode(app),
                AUTO_START_ID => toggle_auto_start(app),
                ABOUT_ID => show_about(app),
                QUIT_ID => quit_app(app),
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

fn build_menu<R: Runtime>(app: &AppHandle<R>) -> Result<Menu<R>> {
    let language = current_language(app);
    let text = i18n::tray_text(&language);
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
        privacy_label(&text, privacy_state),
        true,
        privacy_requested || is_privacy_enabled(privacy_state),
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, text.quit, true, None::<&str>)?;
    let about = MenuItem::with_id(app, ABOUT_ID, text.about, true, None::<&str>)?;
    let auto_start = CheckMenuItem::with_id(
        app,
        AUTO_START_ID,
        text.auto_start,
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
    let language_menu = build_language_menu(app, &language, text.language)?;
    let menu = Menu::with_items(
        app,
        &[
            &privacy_mode,
            &auto_start,
            &language_menu,
            &PredefinedMenuItem::separator(app)?,
            &about,
            &quit,
        ],
    )?;

    Ok(menu)
}

fn build_language_menu<R: Runtime>(
    app: &AppHandle<R>,
    current: &str,
    title: &str,
) -> Result<Submenu<R>> {
    let en = language_item(app, current, "en-US", "English")?;
    let zh_cn = language_item(app, current, "zh-CN", "简体中文")?;
    let zh_tw = language_item(app, current, "zh-TW", "繁體中文")?;
    let fr = language_item(app, current, "fr-FR", "Français")?;
    let ru = language_item(app, current, "ru-RU", "Русский")?;

    Ok(Submenu::with_items(
        app,
        title,
        true,
        &[&en, &zh_cn, &zh_tw, &fr, &ru],
    )?)
}

fn language_item<R: Runtime>(
    app: &AppHandle<R>,
    current: &str,
    code: &str,
    label: &str,
) -> Result<CheckMenuItem<R>> {
    Ok(CheckMenuItem::with_id(
        app,
        format!("{LANGUAGE_PREFIX}{code}"),
        label,
        true,
        current == code,
        None::<&str>,
    )?)
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

fn toggle_auto_start<R: Runtime>(app: &AppHandle<R>) {
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
            refresh_menu(app);
        }
        Err(error) => {
            log::error!("failed to toggle autostart: {error}");
            refresh_menu(app);
        }
    }
}

fn toggle_privacy_mode<R: Runtime>(app: &AppHandle<R>) {
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
            refresh_menu(app);
        }
        Err(error) => {
            log::error!("failed to toggle privacy mode: {error}");
            alert::warning(
                app,
                "Scourgify",
                &format!("Failed to toggle privacy mode.\n\n{error}"),
            );
            refresh_menu(app);
        }
    }
}

fn set_language<R: Runtime>(app: &AppHandle<R>, language: &str) {
    let language = config::normalize_language(language);
    if let Some(config) = app.try_state::<Mutex<Config>>() {
        match config.lock() {
            Ok(mut config) if config.language != language => {
                config.language = language.clone();
                if let Err(error) = config::save(app, &config) {
                    log::error!("failed to persist language: {error}");
                }
            }
            Ok(_) => {}
            Err(error) => log::error!("failed to update language config: {error}"),
        }
    }

    refresh_menu(app);
    if let Err(error) = app.emit(LANGUAGE_CHANGED_EVENT, i18n::language_event(&language)) {
        log::warn!("failed to emit language change: {error}");
    }
}

fn refresh_menu<R: Runtime>(app: &AppHandle<R>) {
    let Some(tray) = app.tray_by_id(theme::MAIN_TRAY_ID) else {
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            if let Err(error) = tray.set_menu(Some(menu)) {
                log::warn!("failed to rebuild tray menu: {error}");
            }
        }
        Err(error) => log::warn!("failed to build tray menu: {error}"),
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

fn privacy_label(text: &i18n::TrayText, state: PrivacyModeState) -> &'static str {
    match state {
        PrivacyModeState::ActivePartial { .. } => text.privacy_mode_partial,
        _ => text.privacy_mode,
    }
}

fn current_language<R: Runtime>(app: &AppHandle<R>) -> String {
    app.state::<Mutex<Config>>()
        .lock()
        .map(|config| config.language.clone())
        .unwrap_or_else(|error| {
            log::error!("failed to read language config: {error}");
            "en-US".to_string()
        })
}
