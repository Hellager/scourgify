use anyhow::{anyhow, Result};
use serde::Serialize;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use thiserror::Error;
use wincent::prelude::{
    QuickAccessLock, QuickAccessLockTarget, QuickAccessManager, QuickAccessUnlockOptions,
    QuickAccessUnlockReport,
};

pub(crate) const PRIVACY_STATE_CHANGED_EVENT: &str = "privacy-state-changed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LockResult {
    Full,
    Partial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PrivacyModeState {
    Inactive,
    ActiveFull,
    ActivePartial { recent: bool, frequent: bool },
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
pub(crate) struct QuickAccessWritePermit<'a> {
    _guard: MutexGuard<'a, ()>,
}

pub struct PrivacyManager {
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
    pub fn new(cleanup_links: bool) -> Self {
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

    pub fn enter(&self) -> Result<LockResult> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|_| anyhow!("privacy operation gate mutex poisoned"))?;

        #[cfg(debug_assertions)]
        if self.mock_mode.load(Ordering::Relaxed) {
            *self
                .mock_state
                .lock()
                .expect("mock privacy state mutex poisoned") = PrivacyModeState::ActiveFull;
            return Ok(LockResult::Full);
        }

        {
            let state = self.state.lock().expect("privacy state mutex poisoned");
            if !matches!(*state, PrivacyLockState::Inactive) {
                return Ok(match &*state {
                    PrivacyLockState::Inactive => unreachable!(),
                    PrivacyLockState::Exiting(state) => lock_result(*state),
                    PrivacyLockState::ActiveFull(_) => LockResult::Full,
                    PrivacyLockState::ActivePartial { .. } => LockResult::Partial,
                });
            }
        }

        match self.manager.lock_quick_access() {
            Ok(lock) => {
                *self.state.lock().expect("privacy state mutex poisoned") =
                    PrivacyLockState::ActiveFull(lock);
                return Ok(LockResult::Full);
            }
            Err(error) => {
                log::warn!("full Quick Access lock failed, trying partial locks: {error}")
            }
        }

        let recent = self.manager.lock_recent_files().ok();
        let frequent = self.manager.lock_frequent_folders().ok();

        if recent.is_none() && frequent.is_none() {
            return Err(anyhow!("failed to lock any Quick Access target"));
        }

        *self.state.lock().expect("privacy state mutex poisoned") =
            PrivacyLockState::ActivePartial { recent, frequent };
        Ok(LockResult::Partial)
    }

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

    pub fn state(&self) -> PrivacyModeState {
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

        assert_eq!(manager.enter().unwrap(), LockResult::Full);
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
        assert_eq!(
            finished_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            LockResult::Full
        );
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
}
