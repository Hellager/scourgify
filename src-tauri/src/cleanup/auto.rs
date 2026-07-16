use std::sync::{Mutex, MutexGuard, TryLockError};

use anyhow::Result;
use serde::Serialize;
use thiserror::Error;

use super::smart_clean_in_run;
use crate::{
    backend::QuickAccessBackendState,
    db::{
        history::CleanSource,
        history_runs::{self, CleanupAction, CleanupTrigger, NewCleanupRun, RunCompletion},
        DbState,
    },
    privacy::{PrivacyManager, PrivacyModeState},
    quick_access::QaBatchResult,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum AutoCleanError {
    #[error("auto-clean is already running")]
    AlreadyRunning,
    #[error("database is unavailable; auto-clean cannot run")]
    DatabaseUnavailable,
    #[error("privacy mode is active; auto-clean cannot run")]
    PrivacyModeActive,
    #[error("auto-clean state is unavailable")]
    StateUnavailable,
}

#[derive(Default)]
pub(crate) struct AutoCleanState {
    running: Mutex<()>,
}

impl AutoCleanState {
    fn begin(&self) -> Result<MutexGuard<'_, ()>, AutoCleanError> {
        match self.running.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => Err(AutoCleanError::AlreadyRunning),
            Err(TryLockError::Poisoned(_)) => Err(AutoCleanError::StateUnavailable),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AutoCleanSectionResult {
    pub result: Option<QaBatchResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct AutoCleanResult {
    pub recent: AutoCleanSectionResult,
    pub frequent: AutoCleanSectionResult,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub warnings: usize,
    pub skipped_protected: usize,
    pub section_errors: usize,
    pub history_errors: usize,
}

impl AutoCleanResult {
    pub(crate) fn has_failures(&self) -> bool {
        self.failed > 0 || self.section_errors > 0 || self.history_errors > 0
    }

    pub(crate) fn has_issues(&self) -> bool {
        self.has_failures() || self.warnings > 0
    }

    fn from_sections(recent: AutoCleanSectionResult, frequent: AutoCleanSectionResult) -> Self {
        let mut aggregate = Self {
            recent,
            frequent,
            total: 0,
            succeeded: 0,
            failed: 0,
            warnings: 0,
            skipped_protected: 0,
            section_errors: 0,
            history_errors: 0,
        };

        for section in [&aggregate.recent, &aggregate.frequent] {
            if let Some(result) = &section.result {
                aggregate.total += result.total;
                aggregate.succeeded += result.succeeded.len();
                aggregate.failed += result.failed.len();
                aggregate.warnings += result.warnings.len();
                aggregate.skipped_protected += result.skipped_protected.len();
                aggregate.history_errors += usize::from(result.history_error.is_some());
            }
            aggregate.section_errors += usize::from(section.error.is_some());
        }
        aggregate
    }
}

pub(crate) fn run(
    database: &DbState,
    backend: &QuickAccessBackendState,
    history_retention: usize,
    privacy: &PrivacyManager,
    state: &AutoCleanState,
    trigger: CleanupTrigger,
) -> Result<AutoCleanResult> {
    ensure_allowed(database.status().available, privacy.state())?;
    let _running = state.begin()?;
    let run_id = database.with_connection(|connection| {
        history_runs::begin(
            connection,
            NewCleanupRun {
                action: CleanupAction::AutoClean,
                trigger,
                qa_type: "all",
            },
        )
    })?;
    log::info!("auto-clean started");
    let mut result = run_sections(|qa_type, source| {
        smart_clean_in_run(
            database,
            backend,
            qa_type,
            history_retention,
            source,
            run_id,
        )
    });

    let completion = completion_from_result(&result);
    if let Err(error) = database.with_connection(|connection| {
        history_runs::finish(connection, run_id, &completion, history_retention)
    }) {
        result.history_errors += 1;
        log::error!("auto-clean run completion failed run_id={run_id} error={error:#}");
    }

    for (qa_type, section) in [("recent", &result.recent), ("frequent", &result.frequent)] {
        if let Some(error) = &section.error {
            log::warn!("auto-clean section failed qa_type={qa_type} error={error}");
        }
    }
    if result.has_issues() {
        log::warn!(
            "auto-clean completed with issues total={} succeeded={} failed={} warnings={} section_errors={} history_errors={}",
            result.total,
            result.succeeded,
            result.failed,
            result.warnings,
            result.section_errors,
            result.history_errors
        );
    } else {
        log::info!(
            "auto-clean completed total={} succeeded={}",
            result.total,
            result.succeeded
        );
    }
    Ok(result)
}

fn completion_from_result(result: &AutoCleanResult) -> RunCompletion {
    let incident_id = [&result.recent, &result.frequent]
        .into_iter()
        .filter_map(|section| section.result.as_ref())
        .flat_map(|result| result.failed.iter())
        .next()
        .map(|failure| failure.error.incident_id.clone());
    RunCompletion {
        requested: result.total,
        succeeded: result.succeeded,
        failed: result.failed,
        protected: result.skipped_protected,
        warnings: result.warnings,
        history_errors: result.history_errors,
        section_errors: result.section_errors,
        incident_id,
    }
}

fn ensure_allowed(
    database_available: bool,
    privacy_state: PrivacyModeState,
) -> Result<(), AutoCleanError> {
    if !database_available {
        return Err(AutoCleanError::DatabaseUnavailable);
    }
    if privacy_state != PrivacyModeState::Inactive {
        return Err(AutoCleanError::PrivacyModeActive);
    }
    Ok(())
}

fn run_sections(
    mut clean: impl FnMut(&str, CleanSource) -> Result<QaBatchResult>,
) -> AutoCleanResult {
    let mut run_section = |qa_type| match clean(qa_type, CleanSource::Auto) {
        Ok(result) => AutoCleanSectionResult {
            result: Some(result),
            error: None,
        },
        Err(error) => AutoCleanSectionResult {
            result: None,
            error: Some(format!("{error:#}")),
        },
    };
    AutoCleanResult::from_sections(run_section("recent"), run_section("frequent"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{CommandError, ErrorCode};
    use crate::quick_access::QaBatchFailure;

    #[test]
    fn runs_both_sections_in_order_and_aggregates_results() {
        let mut calls = Vec::new();
        let result = run_sections(|qa_type, source| {
            calls.push((qa_type.to_string(), source));
            if qa_type == "recent" {
                Ok(QaBatchResult {
                    total: 3,
                    succeeded: vec![r"C:\Temp\a.txt".to_string()],
                    failed: vec![QaBatchFailure {
                        path: r"C:\Temp\b.txt".to_string(),
                        error: CommandError::expected(
                            "test",
                            ErrorCode::QuickAccessOperationFailed,
                            "The Quick Access operation could not be completed.",
                            false,
                            "in use",
                        ),
                    }],
                    warnings: Vec::new(),
                    skipped_protected: vec![r"C:\Work".to_string()],
                    history_error: Some("database busy".to_string()),
                })
            } else {
                Err(anyhow::anyhow!("frequent scan failed"))
            }
        });

        assert_eq!(
            calls,
            [
                ("recent".to_string(), CleanSource::Auto),
                ("frequent".to_string(), CleanSource::Auto),
            ]
        );
        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.warnings, 0);
        assert_eq!(result.skipped_protected, 1);
        assert_eq!(result.section_errors, 1);
        assert_eq!(result.history_errors, 1);
        assert!(result.has_failures());
        assert_eq!(
            result.frequent.error.as_deref(),
            Some("frequent scan failed")
        );
    }

    #[test]
    fn rejects_unavailable_database_and_active_privacy() {
        assert_eq!(
            ensure_allowed(false, PrivacyModeState::Inactive),
            Err(AutoCleanError::DatabaseUnavailable)
        );
        assert_eq!(
            ensure_allowed(true, PrivacyModeState::ActiveFull),
            Err(AutoCleanError::PrivacyModeActive)
        );
    }

    #[test]
    fn allows_only_one_run_at_a_time() {
        let state = AutoCleanState::default();
        let first = state.begin().unwrap();
        assert!(matches!(state.begin(), Err(AutoCleanError::AlreadyRunning)));
        drop(first);
        assert!(state.begin().is_ok());
    }
}
