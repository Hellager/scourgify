use tauri::{AppHandle, Manager, State};

use super::ensure_quick_access_write_allowed;
use crate::{
    app::scheduler::AutoCleanMonitor,
    db::{
        rules,
        rules::RuleError,
        rules_transfer::{
            self, RuleExportResult, RuleImportPreview, RuleImportResult, RuleTransferError,
        },
        DatabaseStateError, DbState,
    },
    error::{CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
    rules::{NewRule, Rule},
};

use super::ActionReceipt;

#[tauri::command]
pub(crate) fn get_rules(database: State<'_, DbState>) -> CommandResult<Vec<Rule>> {
    database
        .with_connection(|connection| rules::list(connection))
        .map_err(|error| database_error("get_rules", error))
}

#[tauri::command]
pub(crate) fn add_rule(
    app: AppHandle,
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    rule: NewRule,
) -> CommandResult<Rule> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let result = database
        .with_connection(|connection| rules::add(connection, rule))
        .map_err(|error| database_error("add_rule", error))?;
    trigger_monitor(&app);
    Ok(result)
}

#[tauri::command]
pub(crate) fn update_rule(
    app: AppHandle,
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
    rule: NewRule,
) -> CommandResult<Rule> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let result = database
        .with_connection(|connection| rules::update(connection, id, rule))
        .map_err(|error| database_error("update_rule", error))?;
    trigger_monitor(&app);
    Ok(result)
}

#[tauri::command]
pub(crate) fn remove_rule(
    app: AppHandle,
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
) -> CommandResult<ActionReceipt> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::remove(connection, id))
        .map_err(|error| database_error("remove_rule", error))?;
    trigger_monitor(&app);
    Ok(ActionReceipt::new("remove_rule", id.to_string(), 1))
}

#[tauri::command]
pub(crate) fn clear_rules(
    app: AppHandle,
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    ids: Option<Vec<i64>>,
) -> CommandResult<ActionReceipt> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let affected = database
        .with_connection(|connection| rules::clear(connection, ids.as_deref()))
        .map_err(|error| database_error("clear_rules", error))?;
    trigger_monitor(&app);
    Ok(ActionReceipt::new(
        "clear_rules",
        if ids.is_some() { "selected" } else { "all" },
        affected as u64,
    ))
}

#[tauri::command]
pub(crate) fn toggle_rule(
    app: AppHandle,
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
    enabled: bool,
) -> CommandResult<Rule> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let result = database
        .with_connection(|connection| rules::toggle(connection, id, enabled))
        .map_err(|error| database_error("toggle_rule", error))?;
    trigger_monitor(&app);
    Ok(result)
}

#[tauri::command]
pub(crate) fn export_rules(
    database: State<'_, DbState>,
    path: String,
    ids: Option<Vec<i64>>,
) -> CommandResult<RuleExportResult> {
    database
        .read_connection()
        .and_then(|connection| rules_transfer::export(&connection, &path, ids.as_deref()))
        .map_err(|error| database_error("export_rules", error))
}

#[tauri::command]
pub(crate) fn preview_rules_import(path: String) -> CommandResult<RuleImportPreview> {
    rules_transfer::preview(&path).map_err(|error| database_error("preview_rules_import", error))
}

#[tauri::command]
pub(crate) fn import_rules(
    app: AppHandle,
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    path: String,
    indices: Option<Vec<usize>>,
) -> CommandResult<RuleImportResult> {
    ensure_quick_access_write_allowed(privacy.state())?;
    let result = database
        .with_connection(|connection| rules_transfer::import(connection, &path, indices.as_deref()))
        .map_err(|error| database_error("import_rules", error))?;
    trigger_monitor(&app);
    Ok(result)
}

fn trigger_monitor(app: &AppHandle) {
    if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
        if let Err(error) = monitor.trigger() {
            log::warn!("failed to trigger monitored auto-clean after rule update: {error:#}");
        }
    }
}

fn database_error(operation: &str, error: anyhow::Error) -> CommandError {
    let (code, message) = if error.downcast_ref::<RuleTransferError>().is_some() {
        (
            ErrorCode::ValidationInvalidArgument,
            "The rule import or export file is invalid.",
        )
    } else {
        match error.downcast_ref::<RuleError>() {
            Some(RuleError::NotFound(_)) => (
                ErrorCode::ResourceNotFound,
                "The requested rule was not found.",
            ),
            Some(RuleError::EmptyKeyword) => (
                ErrorCode::ValidationInvalidArgument,
                "The rule keyword is invalid.",
            ),
            None if error.downcast_ref::<DatabaseStateError>().is_some() => (
                ErrorCode::DatabaseUnavailable,
                "The rule database is unavailable.",
            ),
            None => (
                ErrorCode::InternalUnexpected,
                "The rule operation could not be completed.",
            ),
        }
    };
    if matches!(
        code,
        ErrorCode::ResourceNotFound | ErrorCode::ValidationInvalidArgument
    ) {
        CommandError::expected(operation, code, message, false, error)
    } else {
        CommandError::unexpected(operation, code, message, true, error)
    }
}
