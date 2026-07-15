use std::path::PathBuf;

use tauri::State;

use super::ensure_quick_access_write_allowed;
use crate::{
    privacy::PrivacyManager,
    quick_access::{self, QaCounts, QaItem, QaRestoreResult, QaVisibility},
};

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
) -> Result<QaVisibility, String> {
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

    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|error| {
        log::error!(
            "open in explorer failed path={} error={error}",
            path.display()
        );
        error.to_string()
    })
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
