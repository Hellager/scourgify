use std::sync::Mutex;

use serde::Serialize;
use tauri::{Manager, State};

use crate::{
    app::{scheduler::AutoCleanMonitor, settings},
    config::Config,
    error::{CommandError, CommandResult, ErrorCode},
    privacy::{LockResult, PrivacyManager, PrivacyModeState},
};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PrivacyTransition {
    pub state: PrivacyModeState,
    pub lock_result: Option<LockResult>,
    pub reports: usize,
    pub new_links: usize,
    pub deleted_links: usize,
    pub failed_link_deletions: usize,
}

#[tauri::command]
pub(crate) fn privacy_enter(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
) -> CommandResult<PrivacyTransition> {
    let result = privacy.enter().map_err(|error| {
        CommandError::unexpected(
            "privacy_enter",
            ErrorCode::QuickAccessOperationFailed,
            "Privacy mode could not be enabled.",
            true,
            error,
        )
    })?;
    settings::persist_privacy_mode(&app, config.inner(), true).map_err(|error| {
        CommandError::unexpected(
            "privacy_enter",
            ErrorCode::ConfigPersistenceFailed,
            "Privacy mode was enabled, but the preference could not be saved.",
            true,
            error,
        )
    })?;
    Ok(PrivacyTransition {
        state: privacy.state(),
        lock_result: Some(result),
        reports: 0,
        new_links: 0,
        deleted_links: 0,
        failed_link_deletions: 0,
    })
}

#[tauri::command]
pub(crate) fn privacy_exit(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
) -> CommandResult<PrivacyTransition> {
    let reports = privacy.exit().map_err(|error| {
        CommandError::unexpected(
            "privacy_exit",
            ErrorCode::QuickAccessOperationFailed,
            "Privacy mode could not be disabled.",
            true,
            error,
        )
    })?;
    let mut transition = PrivacyTransition {
        state: privacy.state(),
        lock_result: None,
        reports: reports.len(),
        new_links: 0,
        deleted_links: 0,
        failed_link_deletions: 0,
    };
    for report in &reports {
        transition.new_links += report.new_lnk_paths().len();
        transition.deleted_links += report.deleted_lnk_paths().len();
        transition.failed_link_deletions += report.failed_lnk_deletions().len();
        log::debug!(
            "privacy unlock report: new={}, deleted={}, failed={}",
            report.new_lnk_paths().len(),
            report.deleted_lnk_paths().len(),
            report.failed_lnk_deletions().len()
        );
    }
    settings::persist_privacy_mode(&app, config.inner(), false).map_err(|error| {
        CommandError::unexpected(
            "privacy_exit",
            ErrorCode::ConfigPersistenceFailed,
            "Privacy mode was disabled, but the preference could not be saved.",
            true,
            error,
        )
    })?;
    if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
        if let Err(error) = monitor.trigger() {
            log::warn!("failed to trigger monitored auto-clean after privacy exit: {error:#}");
        }
    }
    Ok(transition)
}

#[tauri::command]
pub(crate) fn privacy_state(privacy: State<'_, PrivacyManager>) -> CommandResult<PrivacyModeState> {
    Ok(privacy.state())
}
