use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::config::Config;

pub fn notify_partial_failure<R: Runtime>(app: &AppHandle<R>, config: &Config, body: &str) {
    if !config.notifications_enabled || !config.notify_partial_failure {
        return;
    }

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
