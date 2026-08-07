use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::Serialize;
use tauri::{Emitter, Manager, Runtime};
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

use super::{
    i18n,
    scheduler::{AutoCleanMonitor, AutoCleanScheduler},
    window,
};
use crate::{
    config::{self, AppMode, Config},
    db::DbState,
    error::{command_warning, CommandWarning},
};

const LANGUAGE_CHANGED_EVENT: &str = "language-changed";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SettingsUpdate<T> {
    pub value: T,
    pub warnings: Vec<CommandWarning>,
}

pub(crate) fn update<R: Runtime>(
    app: &tauri::AppHandle<R>,
    _database: &DbState,
    state: &Mutex<Config>,
    mut next: Config,
) -> Result<SettingsUpdate<Config>> {
    next.language = config::normalize_language(&next.language);
    next.enforce_fixed_cleanup_policies();
    next.validate().context("invalid application settings")?;
    let (current, next) = persist_with(
        state,
        |candidate| {
            next.auto_clean_last_run = candidate.auto_clean_last_run;
            *candidate = next;
        },
        |candidate| config::save(app, candidate).context("failed to save application settings"),
    )?;
    let mut warnings = Vec::new();

    if current.auto_start != next.auto_start {
        if let Err(error) = set_auto_start_preference(app, next.auto_start) {
            warnings.push(command_warning(
                "update_config",
                "config_autostart_apply_failed",
                "apply_autostart",
                "The settings were saved, but the autostart preference could not be applied.",
                error,
            ));
        }
    }
    if current.auto_clean != next.auto_clean {
        if let Some(scheduler) = app.try_state::<AutoCleanScheduler>() {
            if let Err(error) = scheduler.reschedule() {
                warnings.push(command_warning(
                    "update_config",
                    "config_auto_clean_reschedule_failed",
                    "reschedule_auto_clean",
                    "The settings were saved, but automatic cleanup could not be rescheduled.",
                    error,
                ));
            }
        }
        if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
            if let Err(error) = monitor.trigger() {
                warnings.push(command_warning(
                    "update_config",
                    "config_auto_clean_trigger_failed",
                    "trigger_auto_clean",
                    "The settings were saved, but monitored cleanup could not be triggered.",
                    error,
                ));
            }
        }
    }
    if current.app_mode != next.app_mode {
        if let Err(error) = window::apply_strategy(app, next.app_mode) {
            warnings.push(command_warning(
                "update_config",
                "config_window_strategy_apply_failed",
                "apply_window_strategy",
                "The settings were saved, but the window mode could not be applied.",
                error,
            ));
        }
    }
    if current.language != next.language {
        if let Err(error) = emit_language_changed(app, &next.language) {
            warnings.push(command_warning(
                "update_config",
                "config_language_event_failed",
                "emit_language_change",
                "The settings were saved, but the language change could not be announced.",
                error,
            ));
        }
    }
    Ok(SettingsUpdate {
        value: next,
        warnings,
    })
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
) -> Result<SettingsUpdate<AppMode>> {
    persist(app, state, |config| config.app_mode = mode)?;
    let warnings = window::apply_strategy(app, mode)
        .err()
        .map(|error| {
            command_warning(
                "set_app_mode",
                "app_mode_window_strategy_apply_failed",
                "apply_window_strategy",
                "The application mode was saved, but the window could not be updated.",
                error,
            )
        })
        .into_iter()
        .collect();
    Ok(SettingsUpdate {
        value: mode,
        warnings,
    })
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
        if let Err(error) = emit_language_changed(app, &language) {
            log::warn!("failed to emit language change: {error}");
        }
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
    persist_with(state, update, |candidate| {
        config::save(app, candidate).context("failed to persist application settings")
    })
    .map(|_| ())
}

fn persist_with(
    state: &Mutex<Config>,
    update: impl FnOnce(&mut Config),
    save: impl FnOnce(&Config) -> Result<()>,
) -> Result<(Config, Config)> {
    let mut current = state
        .lock()
        .map_err(|error| anyhow::anyhow!("config state lock poisoned: {error}"))?;
    let previous = current.clone();
    let mut next = previous.clone();
    update(&mut next);
    save(&next)?;
    *current = next.clone();
    Ok((previous, next))
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

fn emit_language_changed<R: Runtime>(
    app: &tauri::AppHandle<R>,
    language: &str,
) -> tauri::Result<()> {
    app.emit(LANGUAGE_CHANGED_EVENT, i18n::language_event(language))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        serde_json::from_str("{}").expect("default config should deserialize")
    }

    #[test]
    fn failed_persist_keeps_in_memory_config_unchanged() {
        let state = Mutex::new(test_config());

        let result = persist_with(
            &state,
            |config| config.privacy_mode = true,
            |_| anyhow::bail!("save failed"),
        );

        assert!(result.is_err());
        assert!(!state.lock().unwrap().privacy_mode);
    }

    #[test]
    fn successful_persist_commits_the_saved_copy() {
        let state = Mutex::new(test_config());
        let mut saved = None;

        let (_, committed) = persist_with(
            &state,
            |config| config.privacy_mode = true,
            |candidate| {
                saved = Some(candidate.clone());
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(saved.as_ref(), Some(&committed));
        assert_eq!(*state.lock().unwrap(), committed);
    }
}
