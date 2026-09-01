use serde::Serialize;
use tauri::{Emitter, Manager, State};

use crate::{
    app::{scheduler::AutoCleanMonitor, settings},
    config::Config,
    error::{command_warning, CommandError, CommandResult, CommandWarning, ErrorCode},
    privacy::{
        LockResult, PrivacyEnterOutcome, PrivacyExitFailure, PrivacyManager, PrivacyModeState,
        PRIVACY_STATE_CHANGED_EVENT,
    },
    quick_access::{BackendMode, QuickAccessBackendState},
};

use super::run_blocking;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PrivacyTransition {
    pub(crate) state: PrivacyModeState,
    pub(crate) lock_result: Option<LockResult>,
    pub(crate) reports: usize,
    pub(crate) new_links: usize,
    pub(crate) deleted_links: usize,
    pub(crate) failed_link_deletions: usize,
    pub(crate) warnings: Vec<CommandWarning>,
}

#[tauri::command]
pub(crate) async fn privacy_enter(app: tauri::AppHandle) -> CommandResult<PrivacyTransition> {
    run_blocking("privacy_enter", move || {
        let config = app.state::<std::sync::Mutex<Config>>();
        let privacy = app.state::<PrivacyManager>();
        let backend = app.state::<QuickAccessBackendState>();
        let outcome = privacy.enter().map_err(|error| {
            CommandError::unexpected(
                "privacy_enter",
                ErrorCode::QuickAccessOperationFailed,
                "Privacy mode could not be enabled.",
                true,
                error,
            )
        })?;
        let state = privacy.state();
        emit_privacy_state(&app, state);
        let warnings = privacy_enter_warning(&outcome).into_iter().collect();
        let mut transition = PrivacyTransition {
            state,
            lock_result: Some(outcome.lock_result),
            reports: 0,
            new_links: 0,
            deleted_links: 0,
            failed_link_deletions: 0,
            warnings,
        };
        if backend.mode() == BackendMode::Real {
            if let Err(error) = settings::persist_privacy_mode(&app, config.inner(), true) {
                transition.warnings.push(command_warning(
                    "privacy_enter",
                    "privacy_enter_preference_not_saved",
                    "persist_preference",
                    "Privacy mode was enabled, but the preference could not be saved.",
                    error,
                ));
            }
        }
        Ok(transition)
    })
    .await
}

#[tauri::command]
pub(crate) async fn privacy_exit(app: tauri::AppHandle) -> CommandResult<PrivacyTransition> {
    run_blocking("privacy_exit", move || {
        let config = app.state::<std::sync::Mutex<Config>>();
        let privacy = app.state::<PrivacyManager>();
        let backend = app.state::<QuickAccessBackendState>();
        let outcome = privacy.exit().map_err(|error| {
            CommandError::unexpected(
                "privacy_exit",
                ErrorCode::QuickAccessOperationFailed,
                "Privacy mode could not be disabled.",
                true,
                error,
            )
        })?;
        let state = privacy.state();
        emit_privacy_state(&app, state);
        let warnings = outcome
            .failures
            .into_iter()
            .map(privacy_exit_failure_warning)
            .collect();
        let mut transition = PrivacyTransition {
            state,
            lock_result: None,
            reports: outcome.reports.len(),
            new_links: 0,
            deleted_links: 0,
            failed_link_deletions: 0,
            warnings,
        };
        for report in &outcome.reports {
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
        if backend.mode() == BackendMode::Real {
            if let Err(error) = settings::persist_privacy_mode(&app, config.inner(), false) {
                transition.warnings.push(command_warning(
                    "privacy_exit",
                    "privacy_exit_preference_not_saved",
                    "persist_preference",
                    "Privacy mode was disabled, but the preference could not be saved.",
                    error,
                ));
            }
        }
        if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
            if let Err(error) = monitor.trigger() {
                log::warn!("failed to trigger monitored auto-clean after privacy exit: {error:#}");
            }
        }
        Ok(transition)
    })
    .await
}

#[tauri::command]
pub(crate) fn privacy_state(privacy: State<'_, PrivacyManager>) -> CommandResult<PrivacyModeState> {
    Ok(privacy.state())
}

fn privacy_enter_warning(outcome: &PrivacyEnterOutcome) -> Option<CommandWarning> {
    if outcome.failures.is_empty() {
        return None;
    }
    let message = if outcome.has_target_failure() {
        "Privacy mode is active with partial protection."
    } else {
        "Privacy mode is active using fallback protection."
    };
    Some(command_warning(
        "privacy_enter",
        "privacy_enter_degraded",
        "acquire_quick_access_locks",
        message,
        outcome.diagnostics(),
    ))
}

fn emit_privacy_state(app: &tauri::AppHandle, state: PrivacyModeState) {
    if let Err(error) = app.emit(PRIVACY_STATE_CHANGED_EVENT, state) {
        log::warn!("failed to emit privacy state change: {error}");
    }
}

fn privacy_exit_failure_warning(failure: PrivacyExitFailure) -> CommandWarning {
    command_warning(
        "privacy_exit",
        "privacy_exit_report_failed",
        failure.target,
        "Privacy mode was disabled, but an unlock report could not be completed.",
        failure.error,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_unlock_report_failure_as_warning() {
        let warning = privacy_exit_failure_warning(PrivacyExitFailure {
            target: "recent_files",
            error: anyhow::anyhow!("Recent shortcut snapshot is unavailable"),
        });

        assert_eq!(warning.code, "privacy_exit_report_failed");
        assert_eq!(warning.step, "recent_files");
        assert!(warning.message.contains("disabled"));
    }

    #[test]
    fn exposes_degraded_enter_as_safe_aggregated_warning() {
        let outcome = PrivacyEnterOutcome {
            lock_result: LockResult::Partial,
            failures: vec![
                crate::privacy::PrivacyEnterFailure {
                    target: crate::privacy::PrivacyLockAttemptTarget::All,
                    error: anyhow::anyhow!("private full lock detail"),
                },
                crate::privacy::PrivacyEnterFailure {
                    target: crate::privacy::PrivacyLockAttemptTarget::FrequentFolders,
                    error: anyhow::anyhow!("private frequent lock detail"),
                },
            ],
        };

        let warning = privacy_enter_warning(&outcome).unwrap();

        assert_eq!(warning.code, "privacy_enter_degraded");
        assert_eq!(warning.step, "acquire_quick_access_locks");
        assert!(warning.message.contains("partial protection"));
        assert!(!warning.message.contains("private"));
        assert!(!warning.incident_id.is_empty());
    }

    #[test]
    fn full_enter_has_no_degradation_warning() {
        let outcome = PrivacyEnterOutcome {
            lock_result: LockResult::Full,
            failures: Vec::new(),
        };

        assert!(privacy_enter_warning(&outcome).is_none());
    }
}
