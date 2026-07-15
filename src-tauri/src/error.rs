use std::{
    fmt::Display,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use thiserror::Error;
use wincent::prelude::{QuickAccessPostMutationStep, WincentError};

static INCIDENT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ErrorCode {
    AutoCleanAlreadyRunning,
    AutoCleanUnavailable,
    ConfigPersistenceFailed,
    DatabaseUnavailable,
    InternalUnexpected,
    PrivacyWriteBlocked,
    QuickAccessAlreadyExists,
    QuickAccessMetadataUnavailable,
    QuickAccessOperationFailed,
    QuickAccessPartialFailure,
    QuickAccessPermissionDenied,
    QuickAccessTimeout,
    ResourceNotFound,
    SystemOperationFailed,
    ValidationInvalidArgument,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub incident_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CommandWarning {
    pub code: &'static str,
    pub step: &'static str,
    pub message: String,
    pub incident_id: String,
}

impl CommandError {
    pub(crate) fn expected(
        operation: &str,
        code: ErrorCode,
        message: impl Into<String>,
        retryable: bool,
        error: impl Display,
    ) -> Self {
        Self::new(operation, code, message, retryable, error, false)
    }

    pub(crate) fn unexpected(
        operation: &str,
        code: ErrorCode,
        message: impl Into<String>,
        retryable: bool,
        error: impl Display,
    ) -> Self {
        Self::new(operation, code, message, retryable, error, true)
    }

    fn new(
        operation: &str,
        code: ErrorCode,
        message: impl Into<String>,
        retryable: bool,
        error: impl Display,
        severe: bool,
    ) -> Self {
        let incident_id = next_incident_id();
        let record = format_log_record(operation, &format!("{code:?}"), &incident_id, error);
        if severe {
            log::error!("{record}");
        } else {
            log::warn!("{record}");
        }
        Self {
            code,
            message: message.into(),
            retryable,
            incident_id,
        }
    }
}

pub(crate) fn wincent_command_error(operation: &str, error: anyhow::Error) -> CommandError {
    let classification = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<WincentError>())
        .map(classify_wincent_error);

    match classification {
        Some((code, message, retryable, expected)) => {
            command_error(operation, code, message, retryable, error, expected)
        }
        None => CommandError::unexpected(
            operation,
            ErrorCode::QuickAccessOperationFailed,
            "The Quick Access operation could not be completed.",
            false,
            error,
        ),
    }
}

pub(crate) fn wincent_command_error_ref(operation: &str, error: &WincentError) -> CommandError {
    let (code, message, retryable, expected) = classify_wincent_error(error);
    command_error(operation, code, message, retryable, error, expected)
}

pub(crate) fn wincent_post_mutation_warning(
    operation: &str,
    error: &WincentError,
) -> Option<CommandWarning> {
    let WincentError::PostMutationFailure { step, .. } = error else {
        return None;
    };

    let step_name = match step {
        QuickAccessPostMutationStep::DeleteRecentFilesBackingData => {
            "delete_recent_files_backing_data"
        }
        QuickAccessPostMutationStep::RefreshExplorer => "refresh_explorer",
        _ => "unknown",
    };
    let incident_id = next_incident_id();
    let record = format_log_record(
        operation,
        "QuickAccessPostMutationWarning",
        &incident_id,
        error,
    );
    log::warn!("{record}");

    Some(CommandWarning {
        code: "quick_access_post_mutation_failed",
        step: step_name,
        message: "The Quick Access change completed, but a follow-up step failed.".to_string(),
        incident_id,
    })
}

fn command_error(
    operation: &str,
    code: ErrorCode,
    message: &'static str,
    retryable: bool,
    error: impl Display,
    expected: bool,
) -> CommandError {
    if expected {
        CommandError::expected(operation, code, message, retryable, error)
    } else {
        CommandError::unexpected(operation, code, message, retryable, error)
    }
}

fn classify_wincent_error(error: &WincentError) -> (ErrorCode, &'static str, bool, bool) {
    match error {
        WincentError::InvalidPath(_) | WincentError::InvalidArgument(_) => (
            ErrorCode::ValidationInvalidArgument,
            "The Quick Access request is invalid.",
            false,
            true,
        ),
        WincentError::UnsupportedOperation(_) | WincentError::UnknownQuickAccessType(_) => (
            ErrorCode::ValidationInvalidArgument,
            "The requested Quick Access operation is not supported.",
            false,
            true,
        ),
        WincentError::AlreadyExists { .. } => (
            ErrorCode::QuickAccessAlreadyExists,
            "The item is already present in Quick Access.",
            false,
            true,
        ),
        WincentError::NotInQuickAccess { .. } => (
            ErrorCode::ResourceNotFound,
            "The item is not present in Quick Access.",
            false,
            true,
        ),
        WincentError::Timeout(_) => (
            ErrorCode::QuickAccessTimeout,
            "The Quick Access operation timed out.",
            true,
            true,
        ),
        WincentError::PowerShellExecution(error) => {
            if error.is_access_denied() || error.is_execution_policy_error() {
                (
                    ErrorCode::QuickAccessPermissionDenied,
                    "Windows or PowerShell permissions blocked the Quick Access operation.",
                    false,
                    true,
                )
            } else if error.is_timeout() || error.is_transient() {
                (
                    ErrorCode::QuickAccessTimeout,
                    "The Quick Access operation timed out or was temporarily unavailable.",
                    true,
                    true,
                )
            } else {
                (
                    ErrorCode::SystemOperationFailed,
                    "The PowerShell operation could not be completed.",
                    false,
                    false,
                )
            }
        }
        WincentError::Io(error) => match error.kind() {
            std::io::ErrorKind::PermissionDenied => (
                ErrorCode::QuickAccessPermissionDenied,
                "Windows permissions blocked the Quick Access operation.",
                false,
                true,
            ),
            std::io::ErrorKind::TimedOut => (
                ErrorCode::QuickAccessTimeout,
                "The Quick Access operation timed out.",
                true,
                true,
            ),
            std::io::ErrorKind::Interrupted | std::io::ErrorKind::WouldBlock => (
                ErrorCode::SystemOperationFailed,
                "The Windows operation was temporarily unavailable.",
                true,
                true,
            ),
            _ => (
                ErrorCode::SystemOperationFailed,
                "The Windows operation could not be completed.",
                false,
                false,
            ),
        },
        WincentError::PartialEmpty { .. } => (
            ErrorCode::QuickAccessPartialFailure,
            "The Quick Access cleanup partially completed.",
            false,
            true,
        ),
        WincentError::DestListParse(_) | WincentError::DestListUnsupportedVersion(_) => (
            ErrorCode::QuickAccessMetadataUnavailable,
            "Quick Access metadata is unavailable or unsupported.",
            false,
            true,
        ),
        WincentError::ComApartmentMismatch(_) => (
            ErrorCode::SystemOperationFailed,
            "The Windows COM operation could not be started.",
            false,
            false,
        ),
        WincentError::WindowsApi(_) | WincentError::Utf8(_) | WincentError::ArrayConversion(_) => (
            ErrorCode::SystemOperationFailed,
            "The Windows operation could not be completed.",
            false,
            false,
        ),
        WincentError::ScriptFailed(_) => (
            ErrorCode::SystemOperationFailed,
            "The Windows script operation could not be completed.",
            false,
            false,
        ),
        WincentError::UnknownScriptMethod(_)
        | WincentError::MissingParameter
        | WincentError::ScriptStrategyNotFound(_) => (
            ErrorCode::InternalUnexpected,
            "An unexpected application error occurred.",
            false,
            false,
        ),
        WincentError::PostMutationFailure { .. } => (
            ErrorCode::QuickAccessOperationFailed,
            "The Quick Access operation completed with a follow-up failure.",
            false,
            true,
        ),
        _ => (
            ErrorCode::QuickAccessOperationFailed,
            "The Quick Access operation could not be completed.",
            false,
            false,
        ),
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum CommandGuardError {
    #[error("privacy mode is active")]
    PrivacyWriteBlocked,
    #[error("application state is unavailable: {0}")]
    StateUnavailable(String),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum ValidationError {
    #[error("{0}")]
    InvalidArgument(String),
    #[error("resource does not exist: {0}")]
    NotFound(String),
}

pub(crate) fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let incident_id = next_incident_id();
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        let location = info
            .location()
            .map(|location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            })
            .unwrap_or_else(|| "unknown".to_string());
        log::error!(
            "operation=panic code=InternalUnexpected incident_id={incident_id} thread={thread_name} location={location} error={payload}"
        );
        default_hook(info);
    }));
}

pub(crate) fn report_background_error(operation: &str, error: impl Display) -> String {
    let incident_id = next_incident_id();
    let record = format_log_record(operation, "InternalUnexpected", &incident_id, error);
    log::error!("{record}");
    incident_id
}

fn format_log_record(
    operation: &str,
    code: &str,
    incident_id: &str,
    error: impl Display,
) -> String {
    format!("operation={operation} code={code} incident_id={incident_id} error={error:#}")
}

pub(crate) fn next_incident_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let sequence = INCIDENT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{millis:x}-{:x}-{sequence:x}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_command_error_contract() {
        let error = CommandError {
            code: ErrorCode::PrivacyWriteBlocked,
            message: "The operation is unavailable while privacy mode is active.".to_string(),
            retryable: true,
            incident_id: "incident-1".to_string(),
        };

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "code": "privacy_write_blocked",
                "message": "The operation is unavailable while privacy mode is active.",
                "retryable": true,
                "incident_id": "incident-1"
            })
        );
    }

    #[test]
    fn incident_ids_are_distinct() {
        assert_ne!(next_incident_id(), next_incident_id());
    }

    #[test]
    fn formats_complete_error_chain() {
        let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let wrapped = anyhow::Error::new(error).context("failed to write config");

        assert_eq!(
            format!("{wrapped:#}"),
            "failed to write config: access denied"
        );
    }

    #[test]
    fn log_record_contains_correlation_and_complete_error_chain() {
        let source = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let error = anyhow::Error::new(source).context("failed to write config");

        assert_eq!(
            format_log_record(
                "update_config",
                "ConfigPersistenceFailed",
                "incident-42",
                error,
            ),
            "operation=update_config code=ConfigPersistenceFailed incident_id=incident-42 error=failed to write config: access denied"
        );
    }

    #[test]
    fn maps_wincent_timeout_to_retryable_error() {
        let error = WincentError::Timeout("refresh timed out".to_string());

        let (code, _, retryable, expected) = classify_wincent_error(&error);

        assert_eq!(code, ErrorCode::QuickAccessTimeout);
        assert!(retryable);
        assert!(expected);
    }

    #[test]
    fn warning_serializes_with_incident_and_step() {
        let warning = CommandWarning {
            code: "quick_access_post_mutation_failed",
            step: "refresh_explorer",
            message: "The Quick Access change completed, but a follow-up step failed.".to_string(),
            incident_id: "incident-1".to_string(),
        };

        assert_eq!(warning.code, "quick_access_post_mutation_failed");
        assert_eq!(warning.step, "refresh_explorer");
        assert!(serde_json::to_value(warning).unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("follow-up"));
    }
}
