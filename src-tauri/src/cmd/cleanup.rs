use std::sync::Mutex;

use tauri::{AppHandle, State};

use super::{ensure_quick_access_write_allowed, history_retention};
use crate::{
    app::scheduler,
    cleanup::{self, AutoCleanResult, ClassifiedItem},
    config::Config,
    db::{history::CleanSource, DbState},
    privacy::PrivacyManager,
    quick_access::QaBatchResult,
};

#[tauri::command]
pub(crate) fn list_qa_items_classified(
    database: State<'_, DbState>,
    qa_type: String,
) -> Result<Vec<ClassifiedItem>, String> {
    cleanup::list_classified(database.inner(), &qa_type).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn remove_qa_items(
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
    paths: Vec<String>,
) -> Result<QaBatchResult, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    cleanup::remove_selected(
        database.inner(),
        &qa_type,
        paths,
        history_retention(&config)?,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn empty_qa_items(
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> Result<QaBatchResult, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    cleanup::empty_current(database.inner(), &qa_type, history_retention(&config)?)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn smart_clean(
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    privacy: State<'_, PrivacyManager>,
    qa_type: String,
) -> Result<QaBatchResult, String> {
    ensure_quick_access_write_allowed(privacy.state())?;
    cleanup::smart_clean(
        database.inner(),
        &qa_type,
        history_retention(&config)?,
        CleanSource::Manual,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn run_auto_clean_now(app: AppHandle) -> Result<AutoCleanResult, String> {
    scheduler::run_now(&app).map_err(|error| error.to_string())
}
