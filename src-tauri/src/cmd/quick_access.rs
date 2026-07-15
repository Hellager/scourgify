use std::path::PathBuf;

use tauri::{AppHandle, State};

use super::ensure_quick_access_write_allowed;
use crate::{
    error::{
        wincent_command_error, wincent_post_mutation_warning, CommandError, CommandResult,
        ErrorCode, ValidationError,
    },
    privacy::PrivacyManager,
    quick_access::{
        self, QaCounts, QaItem, QaItemMetadata, QaMutationResult, QaRestoreResult, QaVisibility,
        QuickAccessError,
    },
    quick_access_cache::QuickAccessCache,
};

use super::ActionReceipt;

#[tauri::command]
pub(crate) fn list_qa_items(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    qa_type: String,
    fresh: Option<bool>,
) -> CommandResult<Vec<QaItem>> {
    cache
        .items(&app, &qa_type, fresh.unwrap_or(true))
        .map_err(|error| quick_access_error("list_qa_items", error))
}

#[tauri::command]
pub(crate) fn get_qa_counts(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    fresh: Option<bool>,
) -> CommandResult<QaCounts> {
    cache
        .counts(&app, fresh.unwrap_or(false))
        .map_err(|error| quick_access_error("get_qa_counts", error))
}

#[tauri::command]
pub(crate) fn list_qa_item_metadata(qa_type: String) -> CommandResult<Vec<QaItemMetadata>> {
    quick_access::list_item_metadata(&qa_type)
        .map_err(|error| quick_access_error("list_qa_item_metadata", error))
}

#[tauri::command]
pub(crate) fn add_qa_item(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
    path: String,
) -> CommandResult<QaMutationResult> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let mut warnings = Vec::new();
    if let Err(error) = quick_access::add_item(&qa_type, &path) {
        let warning = error
            .downcast_ref::<wincent::prelude::WincentError>()
            .and_then(|error| wincent_post_mutation_warning("add_qa_item", error));
        if let Some(warning) = warning {
            warnings.push(warning);
        } else {
            return Err(quick_access_error("add_qa_item", error));
        }
    }
    cache.refresh_after_write(&app, &qa_type);
    Ok(QaMutationResult {
        action: "add_qa_item",
        target: path,
        affected: 1,
        warnings,
    })
}

#[tauri::command]
pub(crate) fn restore_qa_defaults(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> CommandResult<QaRestoreResult> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let result = quick_access::restore_defaults(&qa_type)
        .map_err(|error| quick_access_error("restore_qa_defaults", error))?;
    cache.refresh_after_write(&app, &qa_type);
    Ok(result)
}

#[tauri::command]
pub(crate) fn get_qa_visibility() -> CommandResult<QaVisibility> {
    quick_access::get_visibility().map_err(|error| quick_access_error("get_qa_visibility", error))
}

#[tauri::command]
pub(crate) fn set_qa_visibility(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
    visible: bool,
) -> CommandResult<QaVisibility> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let result = quick_access::set_visibility(&qa_type, visible)
        .map_err(|error| quick_access_error("set_qa_visibility", error))?;
    if matches!(qa_type.as_str(), "recent" | "frequent") {
        cache.refresh_after_write(&app, &qa_type);
    }
    Ok(result)
}

#[tauri::command]
pub(crate) fn open_in_explorer(path: String) -> CommandResult<ActionReceipt> {
    let path =
        validate_open_path(&path).map_err(|error| validation_error("open_in_explorer", error))?;
    log::info!("open in explorer started path={}", path.display());

    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|error| {
        CommandError::unexpected(
            "open_in_explorer",
            ErrorCode::SystemOperationFailed,
            "The item could not be revealed in Explorer.",
            true,
            anyhow::Error::new(error).context(format!("failed to reveal {}", path.display())),
        )
    })?;
    Ok(ActionReceipt::new(
        "open_in_explorer",
        path.to_string_lossy(),
        1,
    ))
}

fn validate_open_path(path: &str) -> Result<PathBuf, ValidationError> {
    if path.trim().is_empty() {
        return Err(ValidationError::InvalidArgument(
            "path is empty".to_string(),
        ));
    }

    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(ValidationError::NotFound(path.display().to_string()));
    }
    Ok(path)
}

fn validation_error(operation: &str, error: ValidationError) -> CommandError {
    let (code, message) = match error {
        ValidationError::InvalidArgument(_) => (
            ErrorCode::ValidationInvalidArgument,
            "The requested path is invalid.",
        ),
        ValidationError::NotFound(_) => (
            ErrorCode::ResourceNotFound,
            "The requested path does not exist.",
        ),
    };
    CommandError::expected(operation, code, message, false, error)
}

fn quick_access_error(operation: &str, error: anyhow::Error) -> CommandError {
    if let Some(local_error) = error.downcast_ref::<QuickAccessError>() {
        let (code, message) = match local_error {
            QuickAccessError::ItemNotFound(_) => (
                ErrorCode::ResourceNotFound,
                "The requested path does not exist.",
            ),
            _ => (
                ErrorCode::ValidationInvalidArgument,
                "The Quick Access request is invalid.",
            ),
        };
        CommandError::expected(operation, code, message, false, error)
    } else {
        wincent_command_error(operation, error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_open_path() {
        assert!(matches!(
            validate_open_path("   "),
            Err(ValidationError::InvalidArgument(_))
        ));
    }

    #[test]
    fn rejects_missing_open_path() {
        let path = std::env::temp_dir().join(format!(
            "scourgify-missing-open-path-{}",
            std::process::id()
        ));
        assert!(matches!(
            validate_open_path(path.to_string_lossy().as_ref()),
            Err(ValidationError::NotFound(_))
        ));
    }

    #[test]
    fn path_validation_error_does_not_expose_local_path() {
        let error = validation_error(
            "open_in_explorer",
            ValidationError::NotFound(r"C:\Users\private\secret.txt".to_string()),
        );

        assert_eq!(error.code, ErrorCode::ResourceNotFound);
        assert!(!error.message.contains("private"));
        assert!(!error.retryable);
        assert!(!error.incident_id.is_empty());
    }
}
