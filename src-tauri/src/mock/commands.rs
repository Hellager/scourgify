use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    app::scheduler::{AutoCleanFinished, AutoCleanMonitor, AUTO_CLEAN_FINISHED_EVENT},
    backend::{BackendMode, QuickAccessBackendState},
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
    quick_access::{QuickAccessCache, QuickAccessWatchers},
};

use super::{MockScenario, MockSnapshot};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MockState {
    pub enabled: bool,
    pub snapshot: MockSnapshot,
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
