mod app;
mod cleanup;
mod database;
mod history;
mod privacy;
mod quick_access;
mod rules;

use std::sync::Mutex;

use tauri::State;

use crate::{config::Config, privacy::PrivacyModeState};

const PRIVACY_WRITE_ERROR: &str =
    "Privacy mode is active; Quick Access write operations are disabled.";

pub(crate) fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::get_config,
        app::update_config,
        app::get_app_mode,
        app::set_app_mode,
        app::hide_about,
        app::current_language,
        privacy::privacy_enter,
        privacy::privacy_exit,
        privacy::privacy_state,
        database::get_database_status,
        database::retry_database,
        database::open_database_directory,
        rules::get_rules,
        rules::add_rule,
        rules::update_rule,
        rules::remove_rule,
        rules::toggle_rule,
        quick_access::list_qa_items,
        cleanup::list_qa_items_classified,
        quick_access::get_qa_counts,
        quick_access::pin_qa_folder,
        cleanup::remove_qa_items,
        cleanup::empty_qa_items,
        cleanup::smart_clean,
        cleanup::run_auto_clean_now,
        history::get_clean_records,
        history::export_clean_records,
        history::clear_clean_records,
        history::get_stats,
        quick_access::restore_qa_defaults,
        quick_access::get_qa_visibility,
        quick_access::set_qa_visibility,
        quick_access::open_in_explorer,
    ]
}

fn ensure_quick_access_write_allowed(state: PrivacyModeState) -> Result<(), String> {
    if matches!(state, PrivacyModeState::Inactive) {
        Ok(())
    } else {
        log::warn!("Quick Access write operation blocked because privacy mode is active");
        Err(PRIVACY_WRITE_ERROR.to_string())
    }
}

fn history_retention(config: &State<'_, Mutex<Config>>) -> Result<usize, String> {
    Ok(config
        .lock()
        .map_err(|error| error.to_string())?
        .history_retention)
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
