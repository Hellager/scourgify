use std::path::PathBuf;
use tauri::State;

use crate::{
    db::{
        rules::{self, NewRule, Rule},
        DatabaseStatus, DbState,
    },
    privacy::{PrivacyManager, PrivacyModeState},
    quick_access::{self, QaBatchResult, QaCounts, QaItem, QaRestoreResult, QaVisibility},
};

const PRIVACY_WRITE_ERROR: &str =
    "Privacy mode is active; Quick Access write operations are disabled.";

#[tauri::command]
pub(crate) fn get_database_status(database: State<'_, DbState>) -> DatabaseStatus {
    database.status()
}

#[tauri::command]
pub(crate) fn get_rules(database: State<'_, DbState>) -> Result<Vec<Rule>, String> {
    database
        .with_connection(|connection| rules::list(connection))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn add_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    rule: NewRule,
) -> Result<Rule, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::add(connection, rule))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn update_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
    rule: NewRule,
) -> Result<Rule, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::update(connection, id, rule))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn remove_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
) -> Result<(), String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::remove(connection, id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn toggle_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
    enabled: bool,
) -> Result<Rule, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::toggle(connection, id, enabled))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn list_qa_items(qa_type: String) -> Result<Vec<QaItem>, String> {
    quick_access::list_items(&qa_type).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn get_qa_counts() -> Result<QaCounts, String> {
    quick_access::get_counts().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn pin_qa_folder(
    privacy: State<'_, PrivacyManager>,
    path: String,
) -> Result<(), String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    quick_access::pin_folder(&path).map_err(|error| error.to_string())
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

#[tauri::command]
pub(crate) fn restore_qa_defaults(
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> Result<QaRestoreResult, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    quick_access::restore_defaults(&qa_type).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn get_qa_visibility() -> Result<QaVisibility, String> {
    quick_access::get_visibility().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn set_qa_visibility(
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
    visible: bool,
) -> Result<(), String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    quick_access::set_visibility(&qa_type, visible).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn open_in_explorer(path: String) -> Result<(), String> {
    let path = match validate_open_path(&path) {
        Ok(path) => path,
        Err(error) => {
            log::warn!("open in explorer rejected error={error}");
            return Err(error);
        }
    };
    log::info!("open in explorer started path={}", path.display());

    match tauri_plugin_opener::reveal_item_in_dir(&path) {
        Ok(()) => {
            log::info!("open in explorer succeeded path={}", path.display());
            Ok(())
        }
        Err(error) => {
            log::error!(
                "open in explorer failed path={} error={error}",
                path.display()
            );
            Err(error.to_string())
        }
    }
}

fn ensure_quick_access_write_allowed(state: PrivacyModeState) -> Result<(), String> {
    if matches!(state, PrivacyModeState::Inactive) {
        Ok(())
    } else {
        log::warn!("Quick Access write operation blocked because privacy mode is active");
        Err(PRIVACY_WRITE_ERROR.to_string())
    }
}

fn validate_open_path(path: &str) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("Path is empty.".to_string());
    }

    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }

    Ok(path)
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

    #[test]
    fn rejects_empty_open_path() {
        assert_eq!(validate_open_path("   ").unwrap_err(), "Path is empty.");
    }

    #[test]
    fn rejects_missing_open_path() {
        let path = std::env::temp_dir().join(format!(
            "scourgify-missing-open-path-{}",
            std::process::id()
        ));
        let error = validate_open_path(path.to_string_lossy().as_ref()).unwrap_err();

        assert!(error.contains("Path does not exist:"));
    }
}
