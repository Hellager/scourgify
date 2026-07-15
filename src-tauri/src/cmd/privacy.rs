use std::sync::Mutex;

use tauri::State;

use crate::{
    app::settings,
    config::Config,
    privacy::{LockResult, PrivacyManager, PrivacyModeState},
};

#[tauri::command]
pub(crate) fn privacy_enter(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
) -> Result<LockResult, String> {
    let result = privacy.enter().map_err(|error| error.to_string())?;
    settings::persist_privacy_mode(&app, config.inner(), true)?;
    Ok(result)
}

#[tauri::command]
pub(crate) fn privacy_exit(
    app: tauri::AppHandle,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
) -> Result<(), String> {
    let reports = privacy.exit().map_err(|error| error.to_string())?;
    for report in reports {
        log::debug!(
            "privacy unlock report: new={}, deleted={}, failed={}",
            report.new_lnk_paths().len(),
            report.deleted_lnk_paths().len(),
            report.failed_lnk_deletions().len()
        );
    }
    settings::persist_privacy_mode(&app, config.inner(), false)
}

#[tauri::command]
pub(crate) fn privacy_state(privacy: State<'_, PrivacyManager>) -> PrivacyModeState {
    privacy.state()
}
