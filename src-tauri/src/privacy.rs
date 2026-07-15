use anyhow::{anyhow, Result};
use serde::Serialize;
use std::sync::Mutex;
use wincent::prelude::{
    QuickAccessLock, QuickAccessManager, QuickAccessUnlockOptions, QuickAccessUnlockReport,
};

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
    ActiveFull(QuickAccessLock),
    ActivePartial {
        recent: Option<QuickAccessLock>,
        frequent: Option<QuickAccessLock>,
    },
}

// ponytail: QuickAccessLock only owns Windows HANDLEs; access is serialized and
// Windows handles may be closed from any thread. Remove this if wincent exposes
// a Send lock wrapper later.
unsafe impl Send for PrivacyLockState {}

pub struct PrivacyManager {
    manager: QuickAccessManager,
    state: Mutex<PrivacyLockState>,
    cleanup_links: bool,
}

impl PrivacyManager {
    pub fn new(cleanup_links: bool) -> Self {
        Self {
            manager: QuickAccessManager::new(),
            state: Mutex::new(PrivacyLockState::Inactive),
            cleanup_links,
        }
    }

    pub fn enter(&self) -> Result<LockResult> {
        let mut state = self.state.lock().expect("privacy state mutex poisoned");
        if !matches!(*state, PrivacyLockState::Inactive) {
            return Ok(match &*state {
                PrivacyLockState::Inactive => unreachable!(),
                PrivacyLockState::ActiveFull(_) => LockResult::Full,
                PrivacyLockState::ActivePartial { .. } => LockResult::Partial,
            });
        }

        match self.manager.lock_quick_access() {
            Ok(lock) => {
                *state = PrivacyLockState::ActiveFull(lock);
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

        *state = PrivacyLockState::ActivePartial { recent, frequent };
        Ok(LockResult::Partial)
    }

    pub fn exit(&self) -> Result<Vec<QuickAccessUnlockReport>> {
        let state = std::mem::replace(
            &mut *self.state.lock().expect("privacy state mutex poisoned"),
            PrivacyLockState::Inactive,
        );
        let options = self.unlock_options();
        let mut reports = Vec::new();

        match state {
            PrivacyLockState::Inactive => {}
            PrivacyLockState::ActiveFull(lock) => reports.push(lock.unlock(options)?),
            PrivacyLockState::ActivePartial { recent, frequent } => {
                if let Some(lock) = recent {
                    reports.push(lock.unlock(options)?);
                }
                if let Some(lock) = frequent {
                    reports.push(lock.unlock(options)?);
                }
            }
        }

        Ok(reports)
    }

    pub fn state(&self) -> PrivacyModeState {
        match &*self.state.lock().expect("privacy state mutex poisoned") {
            PrivacyLockState::Inactive => PrivacyModeState::Inactive,
            PrivacyLockState::ActiveFull(_) => PrivacyModeState::ActiveFull,
            PrivacyLockState::ActivePartial { recent, frequent } => {
                PrivacyModeState::ActivePartial {
                    recent: recent.is_some(),
                    frequent: frequent.is_some(),
                }
            }
        }
    }

    fn unlock_options(&self) -> QuickAccessUnlockOptions {
        if self.cleanup_links {
            QuickAccessUnlockOptions::new().cleanup_new_recent_links()
        } else {
            QuickAccessUnlockOptions::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
