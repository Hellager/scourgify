pub(crate) mod alert;
pub(crate) mod i18n;
pub(crate) mod notifier;
pub(crate) mod scheduler;
pub(crate) mod settings;
mod setup;
pub(crate) mod theme;
pub(crate) mod tray;
pub(crate) mod window;

use std::sync::Mutex;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use crate::config::{AppMode, Config};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(build_logger().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
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
                if let Err(error) = window::show_dashboard(app) {
                    log::warn!("failed to focus dashboard for secondary instance: {error}");
                }
            } else {
                alert::info(app, "Scourgify", "Scourgify is already running.");
            }
        }))
        .invoke_handler(crate::cmd::handler())
        .setup(setup::initialize)
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
