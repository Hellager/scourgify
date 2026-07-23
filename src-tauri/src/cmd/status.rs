use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::{
    app::scheduler::{AutoCleanMonitor, AutoCleanScheduler},
    backend::{BackendMode, QuickAccessBackendState},
    cleanup::AutoCleanState,
    config::{AutoCleanPolicy, Config},
    db::{
        history_runs::{
            self, CleanupAction, CleanupRunFilter, CleanupRunQuery, CleanupRunStatus,
            CleanupTrigger,
        },
        rules as database_rules, DbState,
    },
    error::CommandResult,
    privacy::{PrivacyManager, PrivacyModeState},
    quick_access::{QuickAccessWatcherStatus, QuickAccessWatchers},
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeState {
    Healthy,
    Degraded,
    Error,
    Inactive,
    Running,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RuntimeStatusItem {
    pub state: RuntimeState,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QuickAccessRuntimeStatus {
    pub state: RuntimeState,
    pub active_watchers: usize,
    pub expected_watchers: usize,
    pub failing_watchers: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AutoCleanRuntimeStatus {
    pub state: RuntimeState,
    pub policy: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RulesRuntimeStatus {
    pub state: RuntimeState,
    pub total: usize,
    pub enabled: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RuntimeStatusSnapshot {
    pub overall: RuntimeState,
    pub database: RuntimeStatusItem,
    pub qa_monitoring: QuickAccessRuntimeStatus,
    pub auto_clean: AutoCleanRuntimeStatus,
    pub rules: RulesRuntimeStatus,
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub(crate) fn get_runtime_status(
    config: State<'_, Mutex<Config>>,
    database: State<'_, DbState>,
    backend: State<'_, QuickAccessBackendState>,
    watchers: State<'_, QuickAccessWatchers>,
    scheduler: State<'_, AutoCleanScheduler>,
    monitor: State<'_, AutoCleanMonitor>,
    auto_clean: State<'_, AutoCleanState>,
    privacy: State<'_, PrivacyManager>,
) -> CommandResult<RuntimeStatusSnapshot> {
    let database_state = if database.status().available {
        RuntimeState::Healthy
    } else {
        RuntimeState::Error
    };
    let qa_monitoring = quick_access_status(backend.mode(), watchers.status().ok());
    let rules = database
        .with_connection(|connection| database_rules::list(connection))
        .map(|rules| {
            let enabled = rules.iter().filter(|rule| rule.enabled).count();
            RulesRuntimeStatus {
                state: if enabled == 0 {
                    RuntimeState::Inactive
                } else {
                    RuntimeState::Healthy
                },
                total: rules.len(),
                enabled,
            }
        })
        .unwrap_or(RulesRuntimeStatus {
            state: RuntimeState::Error,
            total: 0,
            enabled: 0,
        });
    let policy = config
        .lock()
        .map(|config| config.auto_clean.clone())
        .unwrap_or(AutoCleanPolicy::Disabled);
    let running = auto_clean.is_running().ok();
    let last_run = latest_auto_clean_run(database.inner(), &policy)
        .ok()
        .flatten();
    let auto_clean_state = auto_clean_state(
        &policy,
        running,
        last_run,
        privacy.state(),
        database_state,
        rules.state,
        qa_monitoring.state,
        scheduler.is_alive(),
        monitor.is_alive(),
    );
    let states = [
        database_state,
        qa_monitoring.state,
        auto_clean_state,
        rules.state,
    ];

    Ok(RuntimeStatusSnapshot {
        overall: overall_state(&states),
        database: RuntimeStatusItem {
            state: database_state,
        },
        qa_monitoring,
        auto_clean: AutoCleanRuntimeStatus {
            state: auto_clean_state,
            policy: policy_name(&policy),
        },
        rules,
    })
}

fn quick_access_status(
    backend_mode: BackendMode,
    status: Option<QuickAccessWatcherStatus>,
) -> QuickAccessRuntimeStatus {
    if backend_mode != BackendMode::Real {
        return QuickAccessRuntimeStatus {
            state: RuntimeState::Inactive,
            active_watchers: 0,
            expected_watchers: 0,
            failing_watchers: 0,
        };
    }
    let Some(status) = status else {
        return QuickAccessRuntimeStatus {
            state: RuntimeState::Error,
            active_watchers: 0,
            expected_watchers: 2,
            failing_watchers: 0,
        };
    };
    let state = if status.active == 0 {
        RuntimeState::Error
    } else if status.active < status.expected || status.failing > 0 {
        RuntimeState::Degraded
    } else {
        RuntimeState::Healthy
    };
    QuickAccessRuntimeStatus {
        state,
        active_watchers: status.active,
        expected_watchers: status.expected,
        failing_watchers: status.failing,
    }
}

#[allow(clippy::too_many_arguments)]
fn auto_clean_state(
    policy: &AutoCleanPolicy,
    running: Option<bool>,
    last_run: Option<CleanupRunStatus>,
    privacy: PrivacyModeState,
    database: RuntimeState,
    rules: RuntimeState,
    qa_monitoring: RuntimeState,
    scheduler_alive: bool,
    monitor_alive: bool,
) -> RuntimeState {
    match running {
        Some(true) => return RuntimeState::Running,
        None => return RuntimeState::Error,
        Some(false) => {}
    }
    if matches!(policy, AutoCleanPolicy::Disabled) {
        return RuntimeState::Inactive;
    }
    if privacy != PrivacyModeState::Inactive {
        return RuntimeState::Paused;
    }
    if database == RuntimeState::Error || rules == RuntimeState::Error {
        return RuntimeState::Error;
    }
    let worker_state = match policy {
        AutoCleanPolicy::Disabled => RuntimeState::Inactive,
        AutoCleanPolicy::Monitor if !monitor_alive => RuntimeState::Error,
        AutoCleanPolicy::Monitor => match qa_monitoring {
            RuntimeState::Error => RuntimeState::Error,
            RuntimeState::Degraded => RuntimeState::Degraded,
            _ => RuntimeState::Healthy,
        },
        AutoCleanPolicy::EveryHours { .. } | AutoCleanPolicy::DailyAt { .. } if scheduler_alive => {
            RuntimeState::Healthy
        }
        AutoCleanPolicy::EveryHours { .. } | AutoCleanPolicy::DailyAt { .. } => RuntimeState::Error,
    };
    merge_last_run(worker_state, last_run)
}

fn latest_auto_clean_run(
    database: &DbState,
    policy: &AutoCleanPolicy,
) -> anyhow::Result<Option<CleanupRunStatus>> {
    let trigger = match policy {
        AutoCleanPolicy::Disabled => return Ok(None),
        AutoCleanPolicy::Monitor => CleanupTrigger::Monitor,
        AutoCleanPolicy::EveryHours { .. } | AutoCleanPolicy::DailyAt { .. } => {
            CleanupTrigger::Scheduled
        }
    };
    database.with_connection(|connection| {
        history_runs::list(
            connection,
            CleanupRunQuery {
                page: 1,
                page_size: 1,
                filter: CleanupRunFilter {
                    action: Some(CleanupAction::AutoClean),
                    trigger: Some(trigger),
                    status: None,
                    date_range: None,
                },
            },
        )
        .map(|page| page.runs.first().map(|run| run.status))
    })
}

fn merge_last_run(worker_state: RuntimeState, last_run: Option<CleanupRunStatus>) -> RuntimeState {
    if !matches!(worker_state, RuntimeState::Healthy | RuntimeState::Degraded) {
        return worker_state;
    }
    match last_run {
        Some(CleanupRunStatus::Running) => RuntimeState::Running,
        Some(CleanupRunStatus::Partial) => RuntimeState::Degraded,
        Some(CleanupRunStatus::Failed | CleanupRunStatus::Interrupted) => RuntimeState::Error,
        Some(CleanupRunStatus::Success | CleanupRunStatus::Noop) | None => worker_state,
    }
}

fn overall_state(states: &[RuntimeState]) -> RuntimeState {
    if states.contains(&RuntimeState::Error) {
        RuntimeState::Error
    } else if states.contains(&RuntimeState::Degraded) {
        RuntimeState::Degraded
    } else if states.contains(&RuntimeState::Unknown) {
        RuntimeState::Unknown
    } else if states.contains(&RuntimeState::Running) {
        RuntimeState::Running
    } else {
        RuntimeState::Healthy
    }
}

fn policy_name(policy: &AutoCleanPolicy) -> &'static str {
    match policy {
        AutoCleanPolicy::Disabled => "disabled",
        AutoCleanPolicy::Monitor => "monitor",
        AutoCleanPolicy::EveryHours { .. } => "every_hours",
        AutoCleanPolicy::DailyAt { .. } => "daily_at",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overall_ignores_inactive_optional_features() {
        assert_eq!(
            overall_state(&[
                RuntimeState::Healthy,
                RuntimeState::Healthy,
                RuntimeState::Inactive,
                RuntimeState::Inactive,
            ]),
            RuntimeState::Healthy
        );
    }

    #[test]
    fn monitor_policy_reflects_watcher_health() {
        let state = auto_clean_state(
            &AutoCleanPolicy::Monitor,
            Some(false),
            None,
            PrivacyModeState::Inactive,
            RuntimeState::Healthy,
            RuntimeState::Healthy,
            RuntimeState::Degraded,
            true,
            true,
        );

        assert_eq!(state, RuntimeState::Degraded);
    }

    #[test]
    fn privacy_mode_pauses_enabled_auto_clean() {
        let state = auto_clean_state(
            &AutoCleanPolicy::EveryHours { hours: 6 },
            Some(false),
            None,
            PrivacyModeState::ActiveFull,
            RuntimeState::Healthy,
            RuntimeState::Healthy,
            RuntimeState::Healthy,
            true,
            true,
        );

        assert_eq!(state, RuntimeState::Paused);
    }

    #[test]
    fn failed_last_run_marks_an_available_worker_as_error() {
        assert_eq!(
            merge_last_run(RuntimeState::Healthy, Some(CleanupRunStatus::Failed)),
            RuntimeState::Error
        );
    }
}
