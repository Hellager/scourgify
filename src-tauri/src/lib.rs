mod config;
mod privacy;
mod theme;
mod tray;

use std::sync::Mutex;
use tauri::{Manager, Runtime, State};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use config::Config;
use privacy::{LockResult, PrivacyManager, PrivacyModeState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(build_logger().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            privacy_enter,
            privacy_exit,
            privacy_state
        ])
        .setup(|app| {
            let config = config::load(app.handle())?;
            let privacy_manager = privacy::PrivacyManager::new(config.privacy_mode_cleanup_links);
            if config.privacy_mode {
                match privacy_manager.enter() {
                    Ok(result) => log::info!("restored privacy mode: {result:?}"),
                    Err(error) => log::error!("failed to restore privacy mode: {error}"),
                }
            }
            app.manage(Mutex::new(config));
            app.manage(privacy_manager);
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
