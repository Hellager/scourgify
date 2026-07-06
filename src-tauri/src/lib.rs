#[cfg(not(windows))]
compile_error!("Scourgify is Windows-only because wincent targets Windows Quick Access.");

mod alert;
mod config;
mod i18n;
mod privacy;
mod theme;
mod tray;

use std::sync::Mutex;
use tauri::{Manager, Runtime, State};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as AutostartManagerExt};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use config::{AppMode, Config};
use privacy::{LockResult, PrivacyManager, PrivacyModeState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(build_logger().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            log::info!("secondary instance requested");
            let mode = app
                .try_state::<Mutex<Config>>()
                .and_then(|config| config.lock().ok().map(|config| config.app_mode))
                .unwrap_or(AppMode::Minimal);
            if matches!(mode, AppMode::Dashboard) {
                if let Err(error) = show_dashboard(app) {
                    log::warn!("failed to focus dashboard for secondary instance: {error}");
                }
            } else {
                alert::info(app, "Scourgify", "Scourgify is already running.");
            }
        }))
        .invoke_handler(tauri::generate_handler![
            get_config,
            update_config,
            get_app_mode,
            set_app_mode,
            hide_about,
            current_language,
            privacy_enter,
            privacy_exit,
            privacy_state
        ])
        .setup(|app| {
            let mut config = config::load(app.handle())?;
            sync_auto_start_config(app.handle(), &mut config);
            let privacy_manager = privacy::PrivacyManager::new(config.privacy_mode_cleanup_links);
            if config.privacy_mode {
                match privacy_manager.enter() {
                    Ok(LockResult::Full) => log::info!("restored privacy mode: full"),
                    Ok(LockResult::Partial) => {
                        log::warn!("restored privacy mode with partial protection");
                        alert::warning(
                            app.handle(),
                            "Scourgify",
                            "Privacy mode was restored with partial protection.",
                        );
                    }
                    Err(error) => {
                        log::error!("failed to restore privacy mode: {error}");
                        alert::warning(
                            app.handle(),
                            "Scourgify",
                            &format!("Failed to restore privacy mode.\n\n{error}"),
                        );
                    }
                }
            }
            app.manage(Mutex::new(config));
            app.manage(privacy_manager);
            let mode = app.state::<Mutex<Config>>()
                .lock()
                .map(|config| config.app_mode)
                .unwrap_or(AppMode::Dashboard);
            apply_window_strategy(app.handle(), mode)?;
            tray::build(app.handle())?;
            theme::spawn_theme_watcher(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn build_logger() -> tauri_plugin_log::Builder {
    let level = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    let mut targets = vec![Target::new(TargetKind::LogDir {
        file_name: Some("scourgify".to_string()),
    })];

    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }

    tauri_plugin_log::Builder::new()
        .level(level)
        .max_file_size(1_000_000)
        .rotation_strategy(RotationStrategy::KeepAll)
        .targets(targets)
}

#[tauri::command]
fn get_config(config: State<'_, Mutex<Config>>) -> Result<Config, String> {
    Ok(config.lock().map_err(|error| error.to_string())?.clone())
}

#[tauri::command]
fn update_config(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    mut next_config: Config,
) -> Result<Config, String> {
    next_config.language = config::normalize_language(&next_config.language);
    config::save(&app, &next_config).map_err(|error| error.to_string())?;
    {
        let mut config = config.lock().map_err(|error| error.to_string())?;
        *config = next_config.clone();
    }
    apply_window_strategy(&app, next_config.app_mode).map_err(|error| error.to_string())?;
    Ok(next_config)
}

#[tauri::command]
fn get_app_mode(config: State<'_, Mutex<Config>>) -> Result<AppMode, String> {
    Ok(config
        .lock()
        .map_err(|error| error.to_string())?
        .app_mode)
}

#[tauri::command]
fn set_app_mode(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    mode: AppMode,
) -> Result<AppMode, String> {
    persist_app_mode(&app, &config, mode)?;
    apply_window_strategy(&app, mode).map_err(|error| error.to_string())?;
    Ok(mode)
}

#[tauri::command]
fn hide_about(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let mode = app
            .try_state::<Mutex<Config>>()
            .and_then(|config| config.lock().ok().map(|config| config.app_mode))
            .unwrap_or(AppMode::Minimal);
        if matches!(mode, AppMode::Dashboard) {
            window
                .eval("window.location.hash = '#/'")
                .map_err(|error| error.to_string())?;
        } else {
            window.hide().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn current_language(config: State<'_, Mutex<Config>>) -> Result<String, String> {
    Ok(config
        .lock()
        .map_err(|error| error.to_string())?
        .language
        .clone())
}

#[tauri::command]
fn privacy_enter(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
) -> Result<LockResult, String> {
    let result = privacy.enter().map_err(|error| error.to_string())?;
    persist_privacy_mode(&app, &config, true)?;
    Ok(result)
}

#[tauri::command]
fn privacy_exit(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
) -> Result<(), String> {
    let reports = privacy.exit().map_err(|error| error.to_string())?;
    for report in reports {
        log::debug!(
            "privacy unlock report: new={}, deleted={}, failed={}",
            report.new_lnk_paths().len(),
            report.deleted_lnk_paths().len(),
            report.failed_lnk_deletions().len()
        );
    }
    persist_privacy_mode(&app, &config, false)
}

#[tauri::command]
fn privacy_state(privacy: State<'_, PrivacyManager>) -> PrivacyModeState {
    privacy.state()
}

pub(crate) fn persist_privacy_mode<R: Runtime>(
    app: &tauri::AppHandle<R>,
    config: &State<'_, Mutex<Config>>,
    enabled: bool,
) -> Result<(), String> {
    let mut config = config.lock().map_err(|error| error.to_string())?;
    config.privacy_mode = enabled;
    config::save(app, &config).map_err(|error| error.to_string())
}

pub(crate) fn persist_auto_start<R: Runtime>(
    app: &tauri::AppHandle<R>,
    config: &State<'_, Mutex<Config>>,
    enabled: bool,
) -> Result<(), String> {
    let mut config = config.lock().map_err(|error| error.to_string())?;
    config.auto_start = enabled;
    config::save(app, &config).map_err(|error| error.to_string())
}

pub(crate) fn persist_app_mode<R: Runtime>(
    app: &tauri::AppHandle<R>,
    config: &State<'_, Mutex<Config>>,
    mode: AppMode,
) -> Result<(), String> {
    let mut config = config.lock().map_err(|error| error.to_string())?;
    config.app_mode = mode;
    config::save(app, &config).map_err(|error| error.to_string())
}

fn sync_auto_start_config<R: Runtime>(app: &tauri::AppHandle<R>, config: &mut Config) {
    match app.autolaunch().is_enabled() {
        Ok(enabled) if config.auto_start != enabled => {
            config.auto_start = enabled;
            if let Err(error) = config::save(app, config) {
                log::error!("failed to persist autostart state: {error}");
            }
        }
        Ok(_) => {}
        Err(error) => log::warn!("failed to read autostart state: {error}"),
    }
}

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

pub(crate) fn hide_main_window<R: Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), tauri::Error> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide()?;
    }
    Ok(())
}

pub(crate) fn apply_window_strategy<R: Runtime>(
    app: &tauri::AppHandle<R>,
    mode: AppMode,
) -> Result<(), tauri::Error> {
    match mode {
        AppMode::Dashboard => show_dashboard(app)?,
        AppMode::Minimal => hide_main_window(app)?,
    }

    Ok(())
}
