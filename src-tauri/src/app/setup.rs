use std::sync::Mutex;

use tauri::Manager;

use super::{
    alert, notifier,
    scheduler::{AutoCleanMonitor, AutoCleanScheduler},
    settings, theme, tray, window,
};
use crate::{
    backend::QuickAccessBackendState,
    cleanup::AutoCleanState,
    config::{AppMode, Config},
    db,
    error::report_background_error,
    privacy::{LockResult, PrivacyManager},
    quick_access::{QuickAccessCache, QuickAccessWatchers},
};

pub(crate) fn initialize(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = crate::config::load(app.handle())?;
    settings::sync_auto_start(app.handle(), &mut config);
    let database = db::initialize(app.handle());
    let privacy = PrivacyManager::new(config.privacy_mode_cleanup_links);
    let backend = QuickAccessBackendState::new();
    restore_privacy_mode(app.handle(), &config, &privacy);

    app.manage(Mutex::new(config));
    app.manage(database);
    app.manage(privacy);
    app.manage(backend.clone());
    app.manage(AutoCleanState::default());
    app.manage(AutoCleanScheduler::start(app.handle().clone())?);
    let auto_clean_monitor = AutoCleanMonitor::start(app.handle().clone())?;
    auto_clean_monitor.trigger()?;
    app.manage(auto_clean_monitor);
    let quick_access_cache = QuickAccessCache::default();
    app.manage(quick_access_cache.clone());
    app.manage(QuickAccessWatchers::start(
        app.handle().clone(),
        quick_access_cache,
        backend,
    ));

    let mode = app
        .state::<Mutex<Config>>()
        .lock()
        .map(|config| config.app_mode)
        .unwrap_or(AppMode::Dashboard);
    window::install_close_handler(app.handle());
    window::apply_strategy(app.handle(), mode)?;
    tray::build(app.handle())?;
    theme::update_current_window_icon(app.handle());
    theme::spawn_theme_watcher(app.handle().clone());
    Ok(())
}

fn restore_privacy_mode(app: &tauri::AppHandle, config: &Config, privacy: &PrivacyManager) {
    if !config.privacy_mode {
        return;
    }

    match privacy.enter() {
        Ok(LockResult::Full) => log::info!("restored privacy mode: full"),
        Ok(LockResult::Partial) => {
            log::warn!("restored privacy mode with partial protection");
            alert::warning(
                app,
                "Scourgify",
                "Privacy mode was restored with partial protection.",
            );
            notifier::notify_partial_failure(
                app,
                config,
                "Privacy mode was restored with partial protection.",
            );
        }
        Err(error) => {
            let incident_id = report_background_error("restore_privacy_mode", &error);
            let message = format!("Failed to restore privacy mode: {error}");
            alert::warning(
                app,
                "Scourgify",
                &format!("Failed to restore privacy mode.\n\n{error}"),
            );
            notifier::notify_partial_failure(app, config, &message);
            log::warn!("privacy mode restore failed incident_id={incident_id}");
        }
    }
}
