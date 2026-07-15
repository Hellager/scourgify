use std::sync::Mutex;

use tauri::Manager;

use super::{alert, notifier, scheduler::AutoCleanScheduler, settings, theme, tray, window};
use crate::{
    cleanup::AutoCleanState,
    config::{AppMode, Config},
    db,
    privacy::{LockResult, PrivacyManager},
};

pub(crate) fn initialize(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = crate::config::load(app.handle())?;
    settings::sync_auto_start(app.handle(), &mut config);
    let database = db::initialize(app.handle());
    let privacy = PrivacyManager::new(config.privacy_mode_cleanup_links);
    restore_privacy_mode(app.handle(), &config, &privacy);

    app.manage(Mutex::new(config));
    app.manage(database);
    app.manage(privacy);
    app.manage(AutoCleanState::default());
    app.manage(AutoCleanScheduler::start(app.handle().clone())?);

    let mode = app
        .state::<Mutex<Config>>()
        .lock()
        .map(|config| config.app_mode)
        .unwrap_or(AppMode::Dashboard);
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
            log::error!("failed to restore privacy mode: {error}");
            let message = format!("Failed to restore privacy mode: {error}");
            alert::warning(
                app,
                "Scourgify",
                &format!("Failed to restore privacy mode.\n\n{error}"),
            );
            notifier::notify_partial_failure(app, config, &message);
        }
    }
}
