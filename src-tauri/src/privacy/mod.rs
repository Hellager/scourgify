//! Coordinates Quick Access privacy locks with all crate-local write operations.
//!
//! Privacy transitions and guarded writes share an operation gate. A
//! [`QuickAccessWritePermit`] therefore prevents privacy mode from activating
//! for the complete mutation, while an active or exiting lock rejects new
//! writes. Lock acquisition may degrade to individual Recent and Frequent
//! targets, and unlock cleanup reports partial or best-effort outcomes.

use anyhow::{anyhow, Result};
use serde::Serialize;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    fmt,
    sync::{Mutex, MutexGuard},
};
use thiserror::Error;
use wincent::prelude::{
    QuickAccessLock, QuickAccessLockTarget, QuickAccessManager, QuickAccessUnlockOptions,
    QuickAccessUnlockReport,
};

pub(crate) const PRIVACY_STATE_CHANGED_EVENT: &str = "privacy-state-changed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum LockResult {
    Full,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) enum PrivacyModeState {
    Inactive,
    ActiveFull,
    ActivePartial { recent: bool, frequent: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrivacyLockAttemptTarget {
    All,
    RecentFiles,
    FrequentFolders,
}

impl PrivacyLockAttemptTarget {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::RecentFiles => "recent_files",
            Self::FrequentFolders => "frequent_folders",
        }
    }
}

#[derive(Debug)]
pub(crate) struct PrivacyEnterFailure {
    pub(crate) target: PrivacyLockAttemptTarget,
    pub(crate) error: anyhow::Error,
}

impl PrivacyEnterFailure {
    fn new(target: PrivacyLockAttemptTarget, error: anyhow::Error) -> Self {
        Self { target, error }
    }
}

#[derive(Debug)]
pub(crate) struct PrivacyEnterOutcome {
    pub(crate) lock_result: LockResult,
    pub(crate) failures: Vec<PrivacyEnterFailure>,
}

impl PrivacyEnterOutcome {
    pub(crate) fn has_target_failure(&self) -> bool {
        self.failures
            .iter()
            .any(|failure| failure.target != PrivacyLockAttemptTarget::All)
    }

    pub(crate) fn diagnostics(&self) -> PrivacyEnterDiagnostics<'_> {
        PrivacyEnterDiagnostics(&self.failures)
    }
}

#[derive(Debug)]
pub(crate) struct PrivacyEnterDiagnostics<'a>(&'a [PrivacyEnterFailure]);

impl fmt::Display for PrivacyEnterDiagnostics<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_enter_failures(formatter, self.0)
    }
}

#[derive(Debug)]
pub(crate) struct PrivacyLockError {
    failures: Vec<PrivacyEnterFailure>,
}

impl fmt::Display for PrivacyLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "failed to acquire Quick Access privacy locks: ")?;
        write_enter_failures(formatter, &self.failures)
    }
}

impl std::error::Error for PrivacyLockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failures
            .first()
            .map(|failure| failure.error.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[derive(Debug, Error)]
pub(crate) enum PrivacyEnterError {
    #[error("privacy operation state is unavailable")]
    StateUnavailable,
    #[error(transparent)]
    Lock(#[from] PrivacyLockError),
}

fn write_enter_failures(
    formatter: &mut fmt::Formatter<'_>,
    failures: &[PrivacyEnterFailure],
) -> fmt::Result {
    for (index, failure) in failures.iter().enumerate() {
        if index > 0 {
            formatter.write_str("; ")?;
        }
        write!(
            formatter,
            "target={} error={:#}",
            failure.target.as_str(),
            failure.error
        )?;
    }
    Ok(())
}

enum PrivacyLockState {
    Inactive,
    Exiting(PrivacyModeState),
    ActiveFull(QuickAccessLock),
    ActivePartial {
        recent: Option<QuickAccessLock>,
        frequent: Option<QuickAccessLock>,
    },
}

#[derive(Debug, Default)]
/// Collects every unlock report and target-level unlock failure.
///
/// Target failures do not keep privacy mode active. Successful reports may
/// still contain individual Recent shortcut deletions that failed best-effort.
pub(crate) struct PrivacyExitOutcome {
    pub(crate) reports: Vec<QuickAccessUnlockReport>,
    pub(crate) failures: Vec<PrivacyExitFailure>,
}

#[derive(Debug)]
pub(crate) struct PrivacyExitFailure {
    pub(crate) target: &'static str,
    pub(crate) error: anyhow::Error,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum PrivacyAccessError {
    #[error("privacy mode is active")]
    Active,
    #[error("privacy operation state is unavailable")]
    StateUnavailable,
}

#[must_use = "the permit must be held for the complete Quick Access write"]
#[derive(Debug)]
/// Prevents privacy transitions while a Quick Access mutation is in progress.
///
/// The permit owns the operation-gate guard. Callers must keep it alive until
/// every protected Windows side effect has completed; dropping it allows a
/// waiting privacy transition to proceed.
pub(crate) struct QuickAccessWritePermit<'a> {
    _guard: MutexGuard<'a, ()>,
}

/// Owns privacy lock state and the gate shared with Quick Access writers.
pub(crate) struct PrivacyManager {
    operation_gate: Mutex<()>,
    manager: QuickAccessManager,
    state: Mutex<PrivacyLockState>,
    cleanup_links: bool,
    #[cfg(debug_assertions)]
    mock_mode: AtomicBool,
    #[cfg(debug_assertions)]
    mock_state: Mutex<PrivacyModeState>,
}

impl PrivacyManager {
    pub(crate) fn new(cleanup_links: bool) -> Self {
        Self {
            operation_gate: Mutex::new(()),
            manager: QuickAccessManager::new(),
            state: Mutex::new(PrivacyLockState::Inactive),
            cleanup_links,
            #[cfg(debug_assertions)]
            mock_mode: AtomicBool::new(false),
            #[cfg(debug_assertions)]
            mock_state: Mutex::new(PrivacyModeState::Inactive),
        }
    }

    /// Acquires permission for one complete Quick Access write operation.
    ///
    /// This call blocks behind an in-progress privacy transition. It returns
    /// [`PrivacyAccessError::Active`] when privacy mode is active or exiting, and
    /// the returned permit blocks new transitions until it is dropped.
    pub(crate) fn begin_quick_access_write(
        &self,
    ) -> Result<QuickAccessWritePermit<'_>, PrivacyAccessError> {
        let guard = self
            .operation_gate
            .lock()
            .map_err(|_| PrivacyAccessError::StateUnavailable)?;

        #[cfg(debug_assertions)]
        if self.mock_mode.load(Ordering::Relaxed) {
            let state = self
                .mock_state
                .lock()
                .map_err(|_| PrivacyAccessError::StateUnavailable)?;
            if *state != PrivacyModeState::Inactive {
                return Err(PrivacyAccessError::Active);
            }
            return Ok(QuickAccessWritePermit { _guard: guard });
        }

        let state = self
            .state
            .lock()
            .map_err(|_| PrivacyAccessError::StateUnavailable)?;
        if !matches!(*state, PrivacyLockState::Inactive) {
            return Err(PrivacyAccessError::Active);
        }
        drop(state);

        Ok(QuickAccessWritePermit { _guard: guard })
    }

    /// Activates as much Quick Access privacy protection as can be acquired.
    ///
    /// The call waits for outstanding write permits. If the combined lock fails,
    /// Recent and Frequent locks are attempted independently. At least one
    /// successful target returns a partial outcome with diagnostics; failure of
    /// both targets returns an error and leaves privacy mode inactive.
    pub(crate) fn enter(&self) -> Result<PrivacyEnterOutcome, PrivacyEnterError> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| PrivacyEnterError::StateUnavailable)?;

        #[cfg(debug_assertions)]
        if self.mock_mode.load(Ordering::Relaxed) {
            *self
                .mock_state
                .lock()
                .expect("mock privacy state mutex poisoned") = PrivacyModeState::ActiveFull;
            return Ok(PrivacyEnterOutcome {
                lock_result: LockResult::Full,
                failures: Vec::new(),
            });
        }

        {
            let state = self.state.lock().expect("privacy state mutex poisoned");
            if !matches!(*state, PrivacyLockState::Inactive) {
                let lock_result = match &*state {
                    PrivacyLockState::Inactive => unreachable!(),
                    PrivacyLockState::Exiting(state) => lock_result(*state),
                    PrivacyLockState::ActiveFull(_) => LockResult::Full,
                    PrivacyLockState::ActivePartial { .. } => LockResult::Partial,
                };
                return Ok(PrivacyEnterOutcome {
                    lock_result,
                    failures: Vec::new(),
                });
            }
        }

        let full_failure = match self.manager.lock_quick_access() {
            Ok(lock) => {
                *self.state.lock().expect("privacy state mutex poisoned") =
                    PrivacyLockState::ActiveFull(lock);
                return Ok(PrivacyEnterOutcome {
                    lock_result: LockResult::Full,
                    failures: Vec::new(),
                });
            }
            Err(error) => {
                PrivacyEnterFailure::new(PrivacyLockAttemptTarget::All, anyhow::Error::from(error))
            }
        };
        let partial = resolve_partial_lock_attempts(
            full_failure,
            self.manager
                .lock_recent_files()
                .map_err(anyhow::Error::from),
            self.manager
                .lock_frequent_folders()
                .map_err(anyhow::Error::from),
        )?;

        *self.state.lock().expect("privacy state mutex poisoned") =
            PrivacyLockState::ActivePartial {
                recent: partial.recent,
                frequent: partial.frequent,
            };
        Ok(PrivacyEnterOutcome {
            lock_result: LockResult::Partial,
            failures: partial.failures,
        })
    }

    /// Releases all held privacy locks and returns every best-effort report.
    ///
    /// The call waits for outstanding write permits. Each held target is
    /// attempted even if another target fails, and the manager becomes inactive
    /// after those attempts. When Recent-link cleanup is enabled, individual
    /// deletion failures remain in the unlock reports and do not prevent exit.
    pub(crate) fn exit(&self) -> Result<PrivacyExitOutcome> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| anyhow!("privacy operation gate mutex poisoned"))?;

        #[cfg(debug_assertions)]
        if self.mock_mode.load(Ordering::Relaxed) {
            *self
                .mock_state
                .lock()
                .expect("mock privacy state mutex poisoned") = PrivacyModeState::Inactive;
            return Ok(PrivacyExitOutcome::default());
        }

        let state = {
            let mut state = self.state.lock().expect("privacy state mutex poisoned");
            if matches!(*state, PrivacyLockState::Inactive) {
                return Ok(PrivacyExitOutcome::default());
            }
            let visible_state = privacy_state(&state);
            std::mem::replace(&mut *state, PrivacyLockState::Exiting(visible_state))
        };
        let options = self.unlock_options();
        let mut outcome = PrivacyExitOutcome::default();

        match state {
            PrivacyLockState::Inactive => {}
            PrivacyLockState::Exiting(_) => unreachable!("exit owns the operation gate"),
            PrivacyLockState::ActiveFull(lock) => unlock_lock(&mut outcome, lock, options),
            PrivacyLockState::ActivePartial { recent, frequent } => {
                if let Some(lock) = recent {
                    unlock_lock(&mut outcome, lock, options);
                }
                if let Some(lock) = frequent {
                    unlock_lock(&mut outcome, lock, options);
                }
            }
        }

        *self.state.lock().expect("privacy state mutex poisoned") = PrivacyLockState::Inactive;
        Ok(outcome)
    }

    pub(crate) fn state(&self) -> PrivacyModeState {
        #[cfg(debug_assertions)]
        if self.mock_mode.load(Ordering::Relaxed) {
            return *self
                .mock_state
                .lock()
                .expect("mock privacy state mutex poisoned");
        }

        privacy_state(&self.state.lock().expect("privacy state mutex poisoned"))
    }

    fn unlock_options(&self) -> QuickAccessUnlockOptions {
        if self.cleanup_links {
            QuickAccessUnlockOptions::new().cleanup_new_recent_links()
        } else {
            QuickAccessUnlockOptions::new()
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn set_mock_mode(&self, enabled: bool) -> Result<()> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| anyhow!("privacy operation gate mutex poisoned"))?;
        if enabled && !matches!(self.state(), PrivacyModeState::Inactive) {
            return Err(anyhow!(
                "privacy mode must be inactive before enabling mock mode"
            ));
        }
        self.mock_mode.store(enabled, Ordering::Relaxed);
        *self
            .mock_state
            .lock()
            .expect("mock privacy state mutex poisoned") = PrivacyModeState::Inactive;
        Ok(())
    }
}

#[derive(Debug)]
struct PartialLockOutcome<T> {
    recent: Option<T>,
    frequent: Option<T>,
    failures: Vec<PrivacyEnterFailure>,
}

fn resolve_partial_lock_attempts<T>(
    full_failure: PrivacyEnterFailure,
    recent: Result<T>,
    frequent: Result<T>,
) -> Result<PartialLockOutcome<T>, PrivacyLockError> {
    let mut failures = vec![full_failure];
    let recent = record_lock_attempt(PrivacyLockAttemptTarget::RecentFiles, recent, &mut failures);
    let frequent = record_lock_attempt(
        PrivacyLockAttemptTarget::FrequentFolders,
        frequent,
        &mut failures,
    );

    if recent.is_none() && frequent.is_none() {
        return Err(PrivacyLockError { failures });
    }

    Ok(PartialLockOutcome {
        recent,
        frequent,
        failures,
    })
}

fn record_lock_attempt<T>(
    target: PrivacyLockAttemptTarget,
    result: Result<T>,
    failures: &mut Vec<PrivacyEnterFailure>,
) -> Option<T> {
    match result {
        Ok(lock) => Some(lock),
        Err(error) => {
            failures.push(PrivacyEnterFailure::new(target, error));
            None
        }
    }
}

fn privacy_state(state: &PrivacyLockState) -> PrivacyModeState {
    match state {
        PrivacyLockState::Inactive => PrivacyModeState::Inactive,
        PrivacyLockState::Exiting(state) => *state,
        PrivacyLockState::ActiveFull(_) => PrivacyModeState::ActiveFull,
        PrivacyLockState::ActivePartial { recent, frequent } => PrivacyModeState::ActivePartial {
            recent: recent.is_some(),
            frequent: frequent.is_some(),
        },
    }
}

fn lock_result(state: PrivacyModeState) -> LockResult {
    match state {
        PrivacyModeState::ActivePartial { .. } => LockResult::Partial,
        PrivacyModeState::Inactive | PrivacyModeState::ActiveFull => LockResult::Full,
    }
}

fn unlock_lock(
    outcome: &mut PrivacyExitOutcome,
    lock: QuickAccessLock,
    options: QuickAccessUnlockOptions,
) {
    let target = lock_target_name(lock.target());
    record_unlock_result(
        outcome,
        target,
        lock.unlock(options).map_err(anyhow::Error::from),
    );
}

fn record_unlock_result(
    outcome: &mut PrivacyExitOutcome,
    target: &'static str,
    result: Result<QuickAccessUnlockReport>,
) {
    match result {
        Ok(report) => outcome.reports.push(report),
        Err(error) => outcome.failures.push(PrivacyExitFailure { target, error }),
    }
}

fn lock_target_name(target: QuickAccessLockTarget) -> &'static str {
    match target {
        QuickAccessLockTarget::RecentFiles => "recent_files",
        QuickAccessLockTarget::FrequentFolders => "frequent_folders",
        QuickAccessLockTarget::All => "all",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    };

    #[test]
    fn starts_inactive() {
        let manager = PrivacyManager::new(true);

        assert_eq!(manager.state(), PrivacyModeState::Inactive);
    }

    #[test]
    fn cleanup_option_controls_unlock_cleanup() {
        let cleanup = PrivacyManager::new(true).unlock_options();
        let keep = PrivacyManager::new(false).unlock_options();

        assert!(cleanup.cleanup_new_recent_links_enabled());
        assert!(!keep.cleanup_new_recent_links_enabled());
    }

    #[test]
    fn debug_mock_mode_never_acquires_windows_locks() {
        let manager = PrivacyManager::new(true);
        manager.set_mock_mode(true).unwrap();

        let outcome = manager.enter().unwrap();
        assert_eq!(outcome.lock_result, LockResult::Full);
        assert!(outcome.failures.is_empty());
        assert_eq!(manager.state(), PrivacyModeState::ActiveFull);
        let outcome = manager.exit().unwrap();
        assert!(outcome.reports.is_empty());
        assert!(outcome.failures.is_empty());
        assert_eq!(manager.state(), PrivacyModeState::Inactive);
    }

    #[test]
    fn active_privacy_rejects_quick_access_writes() {
        let manager = PrivacyManager::new(true);
        manager.set_mock_mode(true).unwrap();
        manager.enter().unwrap();

        assert!(matches!(
            manager.begin_quick_access_write(),
            Err(PrivacyAccessError::Active)
        ));
    }

    #[test]
    fn rejected_write_releases_operation_gate() {
        let manager = PrivacyManager::new(true);
        manager.set_mock_mode(true).unwrap();
        manager.enter().unwrap();

        assert!(manager.begin_quick_access_write().is_err());
        assert!(manager.exit().is_ok());
    }

    #[test]
    fn write_permit_blocks_privacy_transition_until_dropped() {
        let manager = Arc::new(PrivacyManager::new(true));
        manager.set_mock_mode(true).unwrap();
        let permit = manager.begin_quick_access_write().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (finished_tx, finished_rx) = mpsc::channel();
        let worker = Arc::clone(&manager);

        let handle = thread::spawn(move || {
            started_tx.send(()).unwrap();
            let result = worker.enter();
            finished_tx.send(result).unwrap();
        });

        started_rx.recv().unwrap();
        assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(permit);
        let outcome = finished_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap()
            .unwrap();
        assert_eq!(outcome.lock_result, LockResult::Full);
        assert!(outcome.failures.is_empty());
        handle.join().unwrap();
        assert!(matches!(
            manager.begin_quick_access_write(),
            Err(PrivacyAccessError::Active)
        ));
    }

    #[test]
    fn exiting_state_remains_fail_closed_until_release_completes() {
        let manager = PrivacyManager::new(true);
        *manager.state.lock().unwrap() =
            PrivacyLockState::Exiting(PrivacyModeState::ActivePartial {
                recent: true,
                frequent: false,
            });

        assert_eq!(
            manager.state(),
            PrivacyModeState::ActivePartial {
                recent: true,
                frequent: false,
            }
        );
        assert!(matches!(
            manager.begin_quick_access_write(),
            Err(PrivacyAccessError::Active)
        ));
    }

    #[test]
    fn collects_unlock_report_failures_without_aborting_exit() {
        let mut outcome = PrivacyExitOutcome::default();

        record_unlock_result(
            &mut outcome,
            "recent_files",
            Err(anyhow!("Recent shortcut snapshot is unavailable")),
        );

        assert!(outcome.reports.is_empty());
        assert_eq!(outcome.failures.len(), 1);
        assert_eq!(outcome.failures[0].target, "recent_files");
        assert_eq!(
            outcome.failures[0].error.to_string(),
            "Recent shortcut snapshot is unavailable"
        );
    }

    #[test]
    fn partial_fallback_preserves_full_lock_failure_when_both_targets_succeed() {
        let outcome = resolve_partial_lock_attempts(
            enter_failure(PrivacyLockAttemptTarget::All, "full lock denied"),
            Ok("recent"),
            Ok("frequent"),
        )
        .unwrap();

        assert_eq!(outcome.recent, Some("recent"));
        assert_eq!(outcome.frequent, Some("frequent"));
        assert_eq!(failure_targets(&outcome.failures), ["all"]);
        assert!(outcome.failures[0].error.to_string().contains("denied"));
    }

    #[test]
    fn partial_fallback_preserves_frequent_target_failure() {
        let outcome = resolve_partial_lock_attempts(
            enter_failure(PrivacyLockAttemptTarget::All, "full lock denied"),
            Ok("recent"),
            Err(anyhow!("frequent lock denied")),
        )
        .unwrap();

        assert_eq!(outcome.recent, Some("recent"));
        assert_eq!(outcome.frequent, None);
        assert_eq!(
            failure_targets(&outcome.failures),
            ["all", "frequent_folders"]
        );
    }

    #[test]
    fn partial_fallback_preserves_recent_target_failure() {
        let outcome = resolve_partial_lock_attempts(
            enter_failure(PrivacyLockAttemptTarget::All, "full lock denied"),
            Err(anyhow!("recent lock denied")),
            Ok("frequent"),
        )
        .unwrap();

        assert_eq!(outcome.recent, None);
        assert_eq!(outcome.frequent, Some("frequent"));
        assert_eq!(failure_targets(&outcome.failures), ["all", "recent_files"]);
    }

    #[test]
    fn failed_fallback_returns_all_target_errors() {
        let error = resolve_partial_lock_attempts::<()>(
            enter_failure(PrivacyLockAttemptTarget::All, "full lock denied"),
            Err(anyhow!("recent lock denied")),
            Err(anyhow!("frequent lock denied")),
        )
        .unwrap_err();

        assert_eq!(
            failure_targets(&error.failures),
            ["all", "recent_files", "frequent_folders"]
        );
        let message = error.to_string();
        assert!(message.contains("full lock denied"));
        assert!(message.contains("recent lock denied"));
        assert!(message.contains("frequent lock denied"));
        assert!(std::error::Error::source(&error).is_some());
    }

    fn enter_failure(target: PrivacyLockAttemptTarget, message: &str) -> PrivacyEnterFailure {
        PrivacyEnterFailure::new(target, anyhow!(message.to_string()))
    }

    fn failure_targets(failures: &[PrivacyEnterFailure]) -> Vec<&'static str> {
        failures
            .iter()
            .map(|failure| failure.target.as_str())
            .collect()
    }
}
