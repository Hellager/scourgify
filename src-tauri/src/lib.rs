mod config;

use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(build_logger().build())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = config::load(app.handle())?;
            app.manage(std::sync::Mutex::new(config));
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
