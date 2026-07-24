use chrono::Utc;
use fake::{faker::filesystem::en::FileName, Fake, Faker};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    app::scheduler::{AutoCleanFinished, AutoCleanMonitor, AUTO_CLEAN_FINISHED_EVENT},
    backend::{BackendMode, QuickAccessBackendState},
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
    quick_access::{QuickAccessCache, QuickAccessWatchers},
    rules::{NewRule, RuleScope, RuleType},
};

use super::{MockScenario, MockSnapshot};

const MAX_MOCK_RULES: usize = 1_000;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MockState {
    pub enabled: bool,
    pub snapshot: MockSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct MockRulesResult {
    pub generated: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MockEventKind {
    QuickAccessRecent,
    QuickAccessFrequent,
    AutoCleanFinished,
}

#[tauri::command]
pub(crate) fn get_mock_state(
    backend: State<'_, QuickAccessBackendState>,
) -> CommandResult<MockState> {
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn set_mock_mode(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
    watchers: State<'_, QuickAccessWatchers>,
    privacy: State<'_, PrivacyManager>,
    database: State<'_, DbState>,
    enabled: bool,
) -> CommandResult<MockState> {
    if enabled {
        privacy
            .set_mock_mode(true)
            .map_err(|error| mock_error("set_mock_mode", error))?;
        if let Err(error) = database.set_mock_mode(true) {
            let _ = privacy.set_mock_mode(false);
            return Err(mock_error("set_mock_mode", error));
        }
        if let Err(error) = watchers.pause().and_then(|()| backend.set_mock()) {
            let _ = database.set_mock_mode(false);
            let _ = privacy.set_mock_mode(false);
            let _ = watchers.resume(app.clone(), cache.inner().clone(), backend.inner().clone());
            return Err(mock_error("set_mock_mode", error));
        }
    } else {
        privacy
            .set_mock_mode(false)
            .map_err(|error| mock_error("set_mock_mode", error))?;
        backend
            .set_real()
            .map_err(|error| mock_error("set_mock_mode", error))?;
        database
            .set_mock_mode(false)
            .map_err(|error| mock_error("set_mock_mode", error))?;
        watchers
            .resume(app.clone(), cache.inner().clone(), backend.inner().clone())
            .map_err(|error| mock_error("set_mock_mode", error))?;
    }
    cache
        .clear()
        .map_err(|error| mock_error("set_mock_mode", error))?;
    for qa_type in ["recent", "frequent"] {
        cache
            .items(&app, backend.inner(), qa_type, true)
            .map_err(|error| mock_error("set_mock_mode", error))?;
    }
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn set_mock_scenario(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
    scenario: MockScenario,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    backend
        .mock()
        .set_scenario(scenario)
        .map_err(|error| mock_error("set_mock_scenario", error))?;
    refresh_cache(&app, backend.inner(), cache.inner(), "set_mock_scenario")?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn refresh_mock_data(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    backend
        .mock()
        .refresh()
        .map_err(|error| mock_error("refresh_mock_data", error))?;
    refresh_cache(&app, backend.inner(), cache.inner(), "refresh_mock_data")?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn reset_mock_data(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    backend
        .mock()
        .set_scenario(MockScenario::Normal)
        .map_err(|error| mock_error("reset_mock_data", error))?;
    refresh_cache(&app, backend.inner(), cache.inner(), "reset_mock_data")?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn generate_mock_rules(
    backend: State<'_, QuickAccessBackendState>,
    database: State<'_, DbState>,
    count: usize,
) -> CommandResult<MockRulesResult> {
    ensure_enabled(backend.inner())?;
    if !(1..=MAX_MOCK_RULES).contains(&count) {
        return Err(CommandError::expected(
            "generate_mock_rules",
            ErrorCode::ValidationInvalidArgument,
            "The mock rule count is invalid.",
            false,
            format!("rule count must be between 1 and {MAX_MOCK_RULES}"),
        ));
    }

    let mock_rules = random_rules(count);
    database
        .with_connection(|connection| replace_mock_rules(connection, mock_rules))
        .map_err(|error| mock_error("generate_mock_rules", error))?;

    Ok(MockRulesResult { generated: count })
}

#[tauri::command]
pub(crate) fn trigger_mock_event(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
    event: MockEventKind,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    match event {
        MockEventKind::QuickAccessRecent => {
            backend
                .mock()
                .trigger_change("recent")
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            cache
                .items(&app, backend.inner(), "recent", true)
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            trigger_monitor(&app)?;
        }
        MockEventKind::QuickAccessFrequent => {
            backend
                .mock()
                .trigger_change("frequent")
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            cache
                .items(&app, backend.inner(), "frequent", true)
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            trigger_monitor(&app)?;
        }
        MockEventKind::AutoCleanFinished => {
            app.emit(
                AUTO_CLEAN_FINISHED_EVENT,
                AutoCleanFinished {
                    completed_at: Utc::now(),
                    total: 3,
                    succeeded: 2,
                    failed: 1,
                    warnings: 0,
                    section_errors: 0,
                    history_errors: 0,
                },
            )
            .map_err(|error| mock_error("trigger_mock_event", error))?;
        }
    }
    state(backend.inner())
}

fn refresh_cache(
    app: &AppHandle,
    backend: &QuickAccessBackendState,
    cache: &QuickAccessCache,
    operation: &str,
) -> CommandResult<()> {
    for qa_type in ["recent", "frequent"] {
        cache
            .items(app, backend, qa_type, true)
            .map_err(|error| mock_error(operation, error))?;
    }
    Ok(())
}

fn trigger_monitor(app: &AppHandle) -> CommandResult<()> {
    if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
        monitor
            .trigger()
            .map_err(|error| mock_error("trigger_mock_event", error))?;
    }
    Ok(())
}

fn state(backend: &QuickAccessBackendState) -> CommandResult<MockState> {
    Ok(MockState {
        enabled: backend.mode() == BackendMode::Mock,
        snapshot: backend
            .mock()
            .snapshot()
            .map_err(|error| mock_error("get_mock_state", error))?,
    })
}

fn random_rules(count: usize) -> Vec<NewRule> {
    (0..count).map(random_rule).collect()
}

fn replace_mock_rules(
    connection: &mut rusqlite::Connection,
    mock_rules: Vec<NewRule>,
) -> anyhow::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM rules", [])?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO rules (keyword, rule_type, scope, enabled)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for rule in mock_rules {
            statement.execute(params![
                rule.keyword,
                rule.rule_type.as_str(),
                rule.scope.as_str(),
                rule.enabled
            ])?;
        }
    }
    transaction.commit()?;
    Ok(())
}

fn random_rule(index: usize) -> NewRule {
    let generated: String = FileName().fake();
    let keyword = generated.trim();
    let keyword = if keyword.is_empty() { "item" } else { keyword };
    let suffix: u32 = (0..1_000_000).fake();
    let scope_index: u8 = (0..3).fake();

    NewRule {
        keyword: format!("{keyword}-{suffix:06}-{index}"),
        rule_type: if Faker.fake() {
            RuleType::Whitelist
        } else {
            RuleType::Blacklist
        },
        scope: match scope_index {
            0 => RuleScope::All,
            1 => RuleScope::Files,
            _ => RuleScope::Folders,
        },
        enabled: Faker.fake(),
    }
}

fn ensure_enabled(backend: &QuickAccessBackendState) -> CommandResult<()> {
    if backend.mode() == BackendMode::Mock {
        Ok(())
    } else {
        Err(CommandError::expected(
            "mock_mode_guard",
            ErrorCode::ValidationInvalidArgument,
            "Mock mode is not enabled.",
            false,
            "mock mode is disabled",
        ))
    }
}

fn mock_error(operation: &str, error: impl std::fmt::Display) -> CommandError {
    CommandError::unexpected(
        operation,
        ErrorCode::InternalUnexpected,
        "The mock operation could not be completed.",
        true,
        error,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn generates_requested_number_of_unique_mock_rules() {
        let rules = random_rules(1_000);

        assert_eq!(rules.len(), 1_000);
        assert!(rules.iter().all(|rule| !rule.keyword.trim().is_empty()));
        assert_eq!(
            rules
                .iter()
                .map(|rule| rule.keyword.as_str())
                .collect::<HashSet<_>>()
                .len(),
            1_000
        );
    }

    #[test]
    fn replaces_existing_rules_with_large_batch() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE rules (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    keyword TEXT NOT NULL,
                    rule_type TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                INSERT INTO rules (keyword, rule_type, scope, enabled)
                VALUES ('existing', 'whitelist', 'all', 1);",
            )
            .unwrap();

        replace_mock_rules(&mut connection, random_rules(1_000)).unwrap();

        let count = connection
            .query_row("SELECT COUNT(*) FROM rules", [], |row| row.get::<_, i64>(0))
            .unwrap();
        assert_eq!(count, 1_000);
    }
}
