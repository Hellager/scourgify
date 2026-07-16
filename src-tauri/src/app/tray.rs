use std::sync::Mutex;

use anyhow::Result;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

use super::{alert, i18n, scheduler::AutoCleanMonitor, settings, theme, window};
use crate::{
    config::{AppMode, Config},
    privacy::{LockResult, PrivacyManager, PrivacyModeState},
};

const OPEN_DASHBOARD_ID: &str = "open-dashboard";
const PRIVACY_MODE_ID: &str = "privacy-mode";
const AUTO_START_ID: &str = "auto-start";
const QUIT_ID: &str = "quit";
const MODE_PREFIX: &str = "mode:";
const LANGUAGE_PREFIX: &str = "language:";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    let menu = build_menu(app)?;

    TrayIconBuilder::with_id(theme::MAIN_TRAY_ID)
        .icon(theme::current_tray_icon()?)
        .tooltip("Scourgify")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if is_left_click_up(&event) {
                handle_left_click(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            if let Some(mode) = id.strip_prefix(MODE_PREFIX) {
                set_mode(app, mode);
                return;
            }
            if let Some(language) = id.strip_prefix(LANGUAGE_PREFIX) {
                set_language(app, language);
                return;
            }

            match id {
                OPEN_DASHBOARD_ID => show_dashboard(app),
                PRIVACY_MODE_ID => toggle_privacy_mode(app),
                AUTO_START_ID => toggle_auto_start(app),
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
    let app_mode = current_app_mode(app);
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
    let open_dashboard = MenuItem::with_id(
        app,
        OPEN_DASHBOARD_ID,
        text.open_dashboard,
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, QUIT_ID, text.quit, true, None::<&str>)?;
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
    let mode_menu = build_mode_menu(app, app_mode, &text)?;
    let language_menu = build_language_menu(app, &language, text.language)?;
    let separator_top = PredefinedMenuItem::separator(app)?;
    let separator_bottom = PredefinedMenuItem::separator(app)?;
    let menu = if matches!(app_mode, AppMode::Dashboard) {
        Menu::with_items(
            app,
            &[
                &open_dashboard,
                &separator_top,
                &privacy_mode,
                &auto_start,
                &mode_menu,
                &language_menu,
                &separator_bottom,
                &quit,
            ],
        )?
    } else {
        Menu::with_items(
            app,
            &[
                &privacy_mode,
                &auto_start,
                &mode_menu,
                &language_menu,
                &separator_bottom,
                &quit,
            ],
        )?
    };

    Ok(menu)
}

fn build_mode_menu<R: Runtime>(
    app: &AppHandle<R>,
    current: AppMode,
    text: &i18n::TrayText,
) -> Result<Submenu<R>> {
    let dashboard = mode_item(
        app,
        current,
        AppMode::Dashboard,
        "dashboard",
        text.mode_dashboard,
    )?;
    let grid = mode_item(app, current, AppMode::Grid, "grid", text.mode_grid)?;
    let tray = mode_item(app, current, AppMode::Tray, "tray", text.mode_tray)?;

    Ok(Submenu::with_items(
        app,
        text.mode,
        true,
        &[&dashboard, &grid, &tray],
    )?)
}

fn mode_item<R: Runtime>(
    app: &AppHandle<R>,
    current: AppMode,
    mode: AppMode,
    id: &str,
    label: &str,
) -> Result<CheckMenuItem<R>> {
    Ok(CheckMenuItem::with_id(
        app,
        format!("{MODE_PREFIX}{id}"),
        label,
        true,
        current == mode,
        None::<&str>,
    )?)
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

fn show_dashboard<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = window::show_dashboard(app) {
        log::error!("failed to show dashboard: {error}");
    }
}

fn set_mode<R: Runtime>(app: &AppHandle<R>, mode: &str) {
    let mode = match mode {
        "dashboard" => AppMode::Dashboard,
        "grid" => AppMode::Grid,
        "tray" => AppMode::Tray,
        _ => return,
    };

    if let Some(config) = app.try_state::<Mutex<Config>>() {
        if let Err(error) = settings::set_app_mode(app, config.inner(), mode) {
            log::error!("failed to persist app mode: {error}");
            refresh_menu(app);
            return;
        }
    }

    refresh_menu(app);
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
                if let Err(error) = settings::persist_auto_start(app, config.inner(), enabled) {
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
        privacy.enter().inspect(|result| {
            log::info!("privacy mode entered: {result:?}");
            if matches!(result, LockResult::Partial) {
                alert::warning(
                    app,
                    "Scourgify",
                    "Privacy mode is active with partial protection.",
                );
            }
        })
    };

    match result {
        Ok(_) => {
            let enabled = !requested;
            if let Err(error) = settings::persist_privacy_mode(app, config.inner(), enabled) {
                log::error!("failed to persist privacy mode: {error}");
            }
            if requested {
                if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
                    if let Err(error) = monitor.trigger() {
                        log::warn!(
                            "failed to trigger monitored auto-clean after privacy exit: {error:#}"
                        );
                    }
                }
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
    if let Some(config) = app.try_state::<Mutex<Config>>() {
        if let Err(error) = settings::set_language(app, config.inner(), language) {
            log::error!("failed to update language: {error}");
        }
    }

    refresh_menu(app);
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
                    if let Err(error) = settings::persist_privacy_mode(app, config.inner(), false) {
                        log::error!("failed to persist privacy mode before quit: {error}");
                    }
                }
            }
            Err(error) => log::error!("failed to exit privacy mode before quit: {error}"),
        }
    }

    app.exit(0);
}

fn handle_left_click<R: Runtime>(app: &AppHandle<R>) {
    match current_app_mode(app) {
        AppMode::Dashboard => show_dashboard(app),
        AppMode::Grid => {
            if let Err(error) = window::show_grid(app) {
                log::error!("failed to show grid: {error}");
            }
        }
        AppMode::Tray => {}
    }
}

fn is_left_click_up(event: &TrayIconEvent) -> bool {
    matches!(
        event,
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
    )
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

fn current_app_mode<R: Runtime>(app: &AppHandle<R>) -> AppMode {
    app.state::<Mutex<Config>>()
        .lock()
        .map(|config| config.app_mode)
        .unwrap_or_else(|error| {
            log::error!("failed to read app mode config: {error}");
            AppMode::Dashboard
        })
}
