use tauri::State;

use super::ensure_quick_access_write_allowed;
use crate::{
    db::{rules, DbState},
    privacy::PrivacyManager,
    rules::{NewRule, Rule},
};

#[tauri::command]
pub(crate) fn get_rules(database: State<'_, DbState>) -> Result<Vec<Rule>, String> {
    database
        .with_connection(|connection| rules::list(connection))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn add_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    rule: NewRule,
) -> Result<Rule, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::add(connection, rule))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn update_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
    rule: NewRule,
) -> Result<Rule, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::update(connection, id, rule))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn remove_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
) -> Result<(), String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::remove(connection, id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn toggle_rule(
    database: State<'_, DbState>,
    privacy: State<'_, PrivacyManager>,
    id: i64,
    enabled: bool,
) -> Result<Rule, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    database
        .with_connection(|connection| rules::toggle(connection, id, enabled))
        .map_err(|error| error.to_string())
}
