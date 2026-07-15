use std::sync::{Mutex, MutexGuard, TryLockError};

use anyhow::{bail, Result};
use serde::Serialize;

use super::smart_clean;
use crate::{
    db::{history::CleanSource, DbState},
    privacy::{PrivacyManager, PrivacyModeState},
    quick_access::QaBatchResult,
};

const AUTO_CLEAN_RUNNING_ERROR: &str = "Auto-clean is already running.";
const AUTO_CLEAN_DATABASE_ERROR: &str = "Database is unavailable; auto-clean cannot run.";
const AUTO_CLEAN_PRIVACY_ERROR: &str = "Privacy mode is active; auto-clean cannot run.";

#[derive(Default)]
pub(crate) struct AutoCleanState {
    running: Mutex<()>,
}

impl AutoCleanState {
    fn begin(&self) -> Result<MutexGuard<'_, ()>> {
        match self.running.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => bail!(AUTO_CLEAN_RUNNING_ERROR),
            Err(TryLockError::Poisoned(_)) => bail!("Auto-clean state is unavailable."),
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
    pub skipped_protected: usize,
    pub section_errors: usize,
    pub history_errors: usize,
}

impl AutoCleanResult {
    pub(crate) fn has_failures(&self) -> bool {
        self.failed > 0 || self.section_errors > 0 || self.history_errors > 0
    }

    fn from_sections(recent: AutoCleanSectionResult, frequent: AutoCleanSectionResult) -> Self {
        let mut aggregate = Self {
            recent,
            frequent,
            total: 0,
            succeeded: 0,
            failed: 0,
            skipped_protected: 0,
            section_errors: 0,
            history_errors: 0,
        };

        for section in [&aggregate.recent, &aggregate.frequent] {
            if let Some(result) = &section.result {
                aggregate.total += result.total;
                aggregate.succeeded += result.succeeded.len();
                aggregate.failed += result.failed.len();
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
    history_retention: usize,
    privacy: &PrivacyManager,
    state: &AutoCleanState,
) -> Result<AutoCleanResult> {
    ensure_allowed(database.status().available, privacy.state())?;
    let _running = state.begin()?;
    log::info!("auto-clean started");
    let result =
        run_sections(|qa_type, source| smart_clean(database, qa_type, history_retention, source));

    for (qa_type, section) in [("recent", &result.recent), ("frequent", &result.frequent)] {
        if let Some(error) = &section.error {
            log::warn!("auto-clean section failed qa_type={qa_type} error={error}");
        }
    }
    if result.has_failures() {
        log::warn!(
            "auto-clean completed with issues total={} succeeded={} failed={} section_errors={} history_errors={}",
            result.total,
            result.succeeded,
            result.failed,
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

fn ensure_allowed(database_available: bool, privacy_state: PrivacyModeState) -> Result<()> {
    if !database_available {
        bail!(AUTO_CLEAN_DATABASE_ERROR);
    }
    if privacy_state != PrivacyModeState::Inactive {
        bail!(AUTO_CLEAN_PRIVACY_ERROR);
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
                        error: "in use".to_string(),
                    }],
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
        assert!(ensure_allowed(false, PrivacyModeState::Inactive)
            .unwrap_err()
            .to_string()
            .contains("Database is unavailable"));
        assert!(ensure_allowed(true, PrivacyModeState::ActiveFull)
            .unwrap_err()
            .to_string()
            .contains("Privacy mode is active"));
    }

    #[test]
    fn allows_only_one_run_at_a_time() {
        let state = AutoCleanState::default();
        let first = state.begin().unwrap();
        assert_eq!(
            state.begin().unwrap_err().to_string(),
            AUTO_CLEAN_RUNNING_ERROR
        );
        drop(first);
        assert!(state.begin().is_ok());
    }
}
