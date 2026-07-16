use std::sync::Mutex;

use anyhow::{Context, Result};
use tauri::{Emitter, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

use super::{
    i18n,
    scheduler::{AutoCleanMonitor, AutoCleanScheduler},
    window,
};
use crate::{
    config::{self, AppMode, Config},
    db::{self, DbState},
};

const LANGUAGE_CHANGED_EVENT: &str = "language-changed";

pub(crate) fn update<R: Runtime>(
    app: &tauri::AppHandle<R>,
    database: &DbState,
    state: &Mutex<Config>,
    mut next: Config,
) -> Result<Config> {
    next.language = config::normalize_language(&next.language);
    next.validate().context("invalid application settings")?;
    let (auto_start, language, history_retention, auto_clean) = {
        let current = state
            .lock()
            .map_err(|error| anyhow::anyhow!("config state lock poisoned: {error}"))?;
        (
            current.auto_start,
            current.language.clone(),
            current.history_retention,
            current.auto_clean.clone(),
        )
    };

    if next.history_retention > 0
        && (history_retention == 0 || next.history_retention < history_retention)
    {
        database.with_connection(|connection| {
            db::history::trim_to(connection, next.history_retention)
        })?;
    }
    if auto_start != next.auto_start {
        set_auto_start_preference(app, next.auto_start)?;
    }
    {
        let mut current = state
            .lock()
            .map_err(|error| anyhow::anyhow!("config state lock poisoned: {error}"))?;
        next.auto_clean_last_run = current.auto_clean_last_run;
        config::save(app, &next).context("failed to save application settings")?;
        *current = next.clone();
    }
    if auto_clean != next.auto_clean {
        if let Some(scheduler) = app.try_state::<AutoCleanScheduler>() {
            if let Err(error) = scheduler.reschedule() {
                log::warn!("failed to reschedule auto-clean after config update: {error:#}");
            }
        }
        if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
            if let Err(error) = monitor.trigger() {
                log::warn!("failed to trigger monitored auto-clean after config update: {error:#}");
            }
        }
    }
    window::apply_strategy(app, next.app_mode).context("failed to apply window strategy")?;
    if language != next.language {
        emit_language_changed(app, &next.language);
    }
    Ok(next)
}

pub(crate) fn persist_privacy_mode<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Mutex<Config>,
    enabled: bool,
) -> Result<()> {
    persist(app, state, |config| config.privacy_mode = enabled)
}

pub(crate) fn persist_auto_start<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Mutex<Config>,
    enabled: bool,
) -> Result<()> {
    persist(app, state, |config| config.auto_start = enabled)
}

pub(crate) fn set_app_mode<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Mutex<Config>,
    mode: AppMode,
) -> Result<AppMode> {
    persist(app, state, |config| config.app_mode = mode)?;
    window::apply_strategy(app, mode).context("failed to apply window strategy")?;
    Ok(mode)
}

pub(crate) fn set_language<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Mutex<Config>,
    language: &str,
) -> Result<String> {
    let language = config::normalize_language(language);
    let changed = {
        let current = state
            .lock()
            .map_err(|error| anyhow::anyhow!("config state lock poisoned: {error}"))?;
        current.language != language
    };
    if changed {
        persist(app, state, |config| config.language = language.clone())?;
        emit_language_changed(app, &language);
    }
    Ok(language)
}

pub(crate) fn sync_auto_start<R: Runtime>(app: &tauri::AppHandle<R>, config: &mut Config) {
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

fn persist<R: Runtime>(
    app: &tauri::AppHandle<R>,
    state: &Mutex<Config>,
    update: impl FnOnce(&mut Config),
) -> Result<()> {
    let mut config = state
        .lock()
        .map_err(|error| anyhow::anyhow!("config state lock poisoned: {error}"))?;
    update(&mut config);
    config::save(app, &config).context("failed to persist application settings")
}

fn set_auto_start_preference<R: Runtime>(app: &tauri::AppHandle<R>, enabled: bool) -> Result<()> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .context("failed to update autostart preference")
}

fn emit_language_changed<R: Runtime>(app: &tauri::AppHandle<R>, language: &str) {
    if let Err(error) = app.emit(LANGUAGE_CHANGED_EVENT, i18n::language_event(language)) {
        log::warn!("failed to emit language change: {error}");
    }
}
