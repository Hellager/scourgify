use tauri::State;

use crate::{
    privacy::{PrivacyManager, PrivacyModeState},
    quick_access::{self, QaBatchResult, QaCounts, QaItem},
};

const PRIVACY_WRITE_ERROR: &str =
    "Privacy mode is active; Quick Access write operations are disabled.";

#[tauri::command]
pub(crate) fn list_qa_items(qa_type: String) -> Result<Vec<QaItem>, String> {
    quick_access::list_items(&qa_type).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn get_qa_counts() -> Result<QaCounts, String> {
    quick_access::get_counts().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn remove_qa_items(
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
    paths: Vec<String>,
) -> Result<QaBatchResult, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    quick_access::remove_items(&qa_type, paths).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn empty_qa_items(
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> Result<(), String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    quick_access::empty_items(&qa_type).map_err(|error| error.to_string())
}

fn ensure_quick_access_write_allowed(state: PrivacyModeState) -> Result<(), String> {
    if matches!(state, PrivacyModeState::Inactive) {
        Ok(())
    } else {
        log::warn!("Quick Access write operation blocked because privacy mode is active");
        Err(PRIVACY_WRITE_ERROR.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_writes_when_privacy_is_inactive() {
        assert!(ensure_quick_access_write_allowed(PrivacyModeState::Inactive).is_ok());
    }

    #[test]
    fn rejects_writes_when_privacy_is_active() {
        assert_eq!(
            ensure_quick_access_write_allowed(PrivacyModeState::ActiveFull).unwrap_err(),
            PRIVACY_WRITE_ERROR
        );
    }
}
