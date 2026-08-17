mod app;
mod cleanup;
mod database;
mod diagnostics;
mod grid;
mod history;
mod privacy;
mod quick_access;
mod rules;
mod status;
mod visualization;

use serde::Serialize;

use crate::{
    error::{CommandError, CommandGuardError, CommandResult, ErrorCode},
    privacy::{PrivacyAccessError, PrivacyManager, QuickAccessWritePermit},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ActionReceipt {
    pub action: &'static str,
    pub target: String,
    pub affected: u64,
}

impl ActionReceipt {
    fn new(action: &'static str, target: impl Into<String>, affected: u64) -> Self {
        Self {
            action,
            target: target.into(),
            affected,
        }
    }
}

#[cfg(not(debug_assertions))]
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
        status::get_runtime_status,
        database::get_database_status,
        database::retry_database,
        database::open_database_directory,
        diagnostics::get_log_directory_status,
        diagnostics::open_log_directory,
        diagnostics::clear_program_logs,
        grid::get_grid_summary,
        rules::get_rules,
        rules::add_rule,
        rules::update_rule,
        rules::remove_rule,
        rules::clear_rules,
        rules::toggle_rule,
        rules::export_rules,
        rules::preview_rules_import,
        rules::import_rules,
        quick_access::list_qa_items,
        cleanup::list_qa_items_classified,
        quick_access::get_qa_counts,
        quick_access::list_qa_item_metadata,
        quick_access::add_qa_item,
        cleanup::remove_qa_items,
        cleanup::empty_qa_items,
        cleanup::smart_clean,
        cleanup::run_auto_clean_now,
        history::get_clean_records,
        history::get_cleanup_runs,
        history::export_clean_records,
        history::export_cleanup_runs,
        history::clear_clean_records,
        history::remove_clean_record,
        history::set_clean_record_visibility,
        history::restore_hidden_clean_records,
        history::get_stats,
        quick_access::restore_qa_defaults,
        quick_access::get_qa_visibility,
        quick_access::set_qa_visibility,
        quick_access::open_in_explorer,
        visualization::get_dashboard_word_cloud,
    ]
}

#[cfg(debug_assertions)]
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
        status::get_runtime_status,
        database::get_database_status,
        database::retry_database,
        database::open_database_directory,
        diagnostics::get_log_directory_status,
        diagnostics::open_log_directory,
        diagnostics::clear_program_logs,
        grid::get_grid_summary,
        rules::get_rules,
        rules::add_rule,
        rules::update_rule,
        rules::remove_rule,
        rules::clear_rules,
        rules::toggle_rule,
        rules::export_rules,
        rules::preview_rules_import,
        rules::import_rules,
        quick_access::list_qa_items,
        cleanup::list_qa_items_classified,
        quick_access::get_qa_counts,
        quick_access::list_qa_item_metadata,
        quick_access::add_qa_item,
        cleanup::remove_qa_items,
        cleanup::empty_qa_items,
        cleanup::smart_clean,
        cleanup::run_auto_clean_now,
        history::get_clean_records,
        history::get_cleanup_runs,
        history::export_clean_records,
        history::export_cleanup_runs,
        history::clear_clean_records,
        history::remove_clean_record,
        history::set_clean_record_visibility,
        history::restore_hidden_clean_records,
        history::get_stats,
        quick_access::restore_qa_defaults,
        quick_access::get_qa_visibility,
        quick_access::set_qa_visibility,
        quick_access::open_in_explorer,
        visualization::get_dashboard_word_cloud,
        crate::mock::commands::get_mock_state,
        crate::mock::commands::set_mock_mode,
        crate::mock::commands::set_mock_scenario,
        crate::mock::commands::refresh_mock_data,
        crate::mock::commands::reset_mock_data,
        crate::mock::commands::generate_mock_rules,
        crate::mock::commands::generate_mock_history,
        crate::mock::commands::trigger_mock_event,
        crate::mock::commands::set_mock_overview_preset,
    ]
}

fn begin_quick_access_write(privacy: &PrivacyManager) -> CommandResult<QuickAccessWritePermit<'_>> {
    match privacy.begin_quick_access_write() {
        Ok(permit) => Ok(permit),
        Err(PrivacyAccessError::Active) => Err(CommandError::expected(
            "quick_access_write_guard",
            ErrorCode::PrivacyWriteBlocked,
            "The operation is unavailable while privacy mode is active.",
            true,
            CommandGuardError::PrivacyWriteBlocked,
        )),
        Err(PrivacyAccessError::StateUnavailable) => Err(CommandError::unexpected(
            "quick_access_write_guard",
            ErrorCode::InternalUnexpected,
            "Application state is temporarily unavailable.",
            true,
            PrivacyAccessError::StateUnavailable,
        )),
    }
}

fn state_error(operation: &str, error: impl std::fmt::Display) -> CommandError {
    CommandError::unexpected(
        operation,
        ErrorCode::InternalUnexpected,
        "Application state is temporarily unavailable.",
        true,
        CommandGuardError::StateUnavailable(error.to_string()),
    )
}

pub(crate) async fn run_blocking<T: Send + 'static>(
    operation: &'static str,
    work: impl FnOnce() -> CommandResult<T> + Send + 'static,
) -> CommandResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| {
            CommandError::unexpected(
                operation,
                ErrorCode::InternalUnexpected,
                "The operation could not be completed.",
                true,
                error,
            )
        })?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_writes_when_privacy_is_inactive() {
        assert!(begin_quick_access_write(&PrivacyManager::new(true)).is_ok());
    }

    #[test]
    fn rejects_writes_when_privacy_is_active() {
        let manager = PrivacyManager::new(true);
        manager.set_mock_mode(true).unwrap();
        manager.enter().unwrap();
        let error = begin_quick_access_write(&manager).unwrap_err();

        assert_eq!(error.code, ErrorCode::PrivacyWriteBlocked);
        assert!(error.retryable);
    }

    #[test]
    fn serializes_action_receipt_contract() {
        assert_eq!(
            serde_json::to_value(ActionReceipt::new("remove_rule", "42", 1)).unwrap(),
            serde_json::json!({
                "action": "remove_rule",
                "target": "42",
                "affected": 1
            })
        );
    }

    #[test]
    fn blocking_work_runs_off_the_calling_thread() {
        let calling_thread = std::thread::current().id();
        let worker_thread =
            tauri::async_runtime::block_on(run_blocking(
                "test",
                || Ok(std::thread::current().id()),
            ))
            .unwrap();

        assert_ne!(worker_thread, calling_thread);
    }
}
