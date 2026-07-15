use std::{
    fmt::Display,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use thiserror::Error;

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
    QuickAccessOperationFailed,
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
}
