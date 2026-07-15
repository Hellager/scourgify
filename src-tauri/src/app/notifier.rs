use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::{cleanup::AutoCleanResult, config::Config};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoCleanNotification {
    Complete,
    PartialFailure,
}

pub fn notify_partial_failure<R: Runtime>(app: &AppHandle<R>, config: &Config, body: &str) {
    if !config.notifications_enabled || !config.notify_partial_failure {
        return;
    }

    show(app, body);
}

pub fn notify_auto_clean<R: Runtime>(
    app: &AppHandle<R>,
    config: &Config,
    result: &AutoCleanResult,
) {
    let Some(notification) = auto_clean_notification(config, result, main_window_is_inactive(app))
    else {
        return;
    };
    let body = match notification {
        AutoCleanNotification::Complete => format!(
            "Auto-clean completed: {} of {} matching items removed.",
            result.succeeded, result.total
        ),
        AutoCleanNotification::PartialFailure => format!(
            "Auto-clean completed with issues: {} removed, {} item failures, {} warnings, {} section failures, {} history failures.",
            result.succeeded,
            result.failed,
            result.warnings,
            result.section_errors,
            result.history_errors
        ),
    };
    show(app, &body);
}

fn auto_clean_notification(
    config: &Config,
    result: &AutoCleanResult,
    window_inactive: bool,
) -> Option<AutoCleanNotification> {
    if !config.notifications_enabled {
        return None;
    }
    if result.has_issues() {
        return config
            .notify_partial_failure
            .then_some(AutoCleanNotification::PartialFailure);
    }
    if !config.notify_operation_complete {
        return None;
    }
    let allowed_for_window = if window_inactive {
        config.notify_inactive_operation_complete
    } else {
        config.notify_active_operation_complete
    };
    allowed_for_window.then_some(AutoCleanNotification::Complete)
}

fn main_window_is_inactive<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        return true;
    };
    !window.is_visible().unwrap_or(false) || !window.is_focused().unwrap_or(false)
}

fn show<R: Runtime>(app: &AppHandle<R>, body: &str) {
    if let Err(error) = app
        .notification()
        .builder()
        .title("Scourgify")
        .body(body)
        .show()
    {
        log::warn!("failed to send system notification: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleanup::AutoCleanSectionResult;
    use crate::quick_access::QaBatchResult;

    #[test]
    fn auto_clean_notifications_follow_result_and_window_preferences() {
        let mut config: Config = serde_json::from_str("{}").unwrap();
        let complete = result(false);
        let partial = result(true);

        assert_eq!(
            auto_clean_notification(&config, &complete, true),
            Some(AutoCleanNotification::Complete)
        );
        assert_eq!(auto_clean_notification(&config, &complete, false), None);
        assert_eq!(
            auto_clean_notification(&config, &partial, false),
            Some(AutoCleanNotification::PartialFailure)
        );

        config.notifications_enabled = false;
        assert_eq!(auto_clean_notification(&config, &partial, true), None);
    }

    fn result(with_failure: bool) -> AutoCleanResult {
        AutoCleanResult {
            recent: AutoCleanSectionResult {
                result: Some(QaBatchResult::default()),
                error: None,
            },
            frequent: AutoCleanSectionResult {
                result: Some(QaBatchResult::default()),
                error: None,
            },
            total: 0,
            succeeded: 0,
            failed: usize::from(with_failure),
            warnings: 0,
            skipped_protected: 0,
            section_errors: 0,
            history_errors: 0,
        }
    }
}
