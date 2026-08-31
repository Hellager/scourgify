use crate::{
    app::scheduler::AutoCleanMonitor,
    db::{rule_repository, rule_repository::RuleError, DatabaseStateError, DbState},
    error::{CommandError, CommandResult, ErrorCode},
    rules::{
        transfer::{
            self, RuleExportResult, RuleImportPreview, RuleImportResult, RuleTransferError,
        },
        NewRule, Rule,
    },
};
use tauri::{AppHandle, Manager};

use super::{run_blocking, ActionReceipt};

#[tauri::command]
pub(crate) async fn get_rules(app: AppHandle) -> CommandResult<Vec<Rule>> {
    run_blocking("get_rules", move || {
        app.state::<DbState>()
            .with_connection(|connection| rule_repository::list(connection))
            .map_err(|error| database_error("get_rules", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn add_rule(app: AppHandle, rule: NewRule) -> CommandResult<Rule> {
    let app_for_worker = app.clone();
    let result = run_blocking("add_rule", move || {
        app_for_worker
            .state::<DbState>()
            .with_connection(|connection| rule_repository::add(connection, rule))
            .map_err(|error| database_error("add_rule", error))
    })
    .await?;
    trigger_monitor(&app);
    Ok(result)
}

#[tauri::command]
pub(crate) async fn update_rule(app: AppHandle, id: i64, rule: NewRule) -> CommandResult<Rule> {
    let app_for_worker = app.clone();
    let result = run_blocking("update_rule", move || {
        app_for_worker
            .state::<DbState>()
            .with_connection(|connection| rule_repository::update(connection, id, rule))
            .map_err(|error| database_error("update_rule", error))
    })
    .await?;
    trigger_monitor(&app);
    Ok(result)
}

#[tauri::command]
pub(crate) async fn remove_rule(app: AppHandle, id: i64) -> CommandResult<ActionReceipt> {
    let app_for_worker = app.clone();
    run_blocking("remove_rule", move || {
        app_for_worker
            .state::<DbState>()
            .with_connection(|connection| rule_repository::remove(connection, id))
            .map_err(|error| database_error("remove_rule", error))
    })
    .await?;
    trigger_monitor(&app);
    Ok(ActionReceipt::new("remove_rule", id.to_string(), 1))
}

#[tauri::command]
pub(crate) async fn clear_rules(
    app: AppHandle,
    ids: Option<Vec<i64>>,
) -> CommandResult<ActionReceipt> {
    let target = if ids.is_some() { "selected" } else { "all" };
    let app_for_worker = app.clone();
    let affected = run_blocking("clear_rules", move || {
        app_for_worker
            .state::<DbState>()
            .with_connection(|connection| rule_repository::clear(connection, ids.as_deref()))
            .map_err(|error| database_error("clear_rules", error))
    })
    .await?;
    trigger_monitor(&app);
    Ok(ActionReceipt::new("clear_rules", target, affected as u64))
}

#[tauri::command]
pub(crate) async fn toggle_rule(app: AppHandle, id: i64, enabled: bool) -> CommandResult<Rule> {
    let app_for_worker = app.clone();
    let result = run_blocking("toggle_rule", move || {
        app_for_worker
            .state::<DbState>()
            .with_connection(|connection| rule_repository::toggle(connection, id, enabled))
            .map_err(|error| database_error("toggle_rule", error))
    })
    .await?;
    trigger_monitor(&app);
    Ok(result)
}

#[tauri::command]
pub(crate) async fn export_rules(
    app: AppHandle,
    path: String,
    ids: Option<Vec<i64>>,
) -> CommandResult<RuleExportResult> {
    run_blocking("export_rules", move || {
        app.state::<DbState>()
            .read_connection()
            .and_then(|connection| transfer::export(&connection, &path, ids.as_deref()))
            .map_err(|error| database_error("export_rules", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn preview_rules_import(path: String) -> CommandResult<RuleImportPreview> {
    run_blocking("preview_rules_import", move || {
        transfer::preview(&path).map_err(|error| database_error("preview_rules_import", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_rules(
    app: AppHandle,
    path: String,
    indices: Option<Vec<usize>>,
) -> CommandResult<RuleImportResult> {
    let app_for_worker = app.clone();
    let result = run_blocking("import_rules", move || {
        app_for_worker
            .state::<DbState>()
            .with_connection(|connection| transfer::import(connection, &path, indices.as_deref()))
            .map_err(|error| database_error("import_rules", error))
    })
    .await?;
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
