use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use crate::{
    backend::QuickAccessBackendState,
    config::{Config, VisualizationDataSource},
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
    visualization::{self, WordCloudRange, WordCloudResult, WordCloudSource},
};

use super::run_blocking;

#[tauri::command]
pub(crate) async fn get_dashboard_word_cloud(
    app: AppHandle,
    config: State<'_, Mutex<Config>>,
    source: Option<WordCloudSource>,
    range: Option<WordCloudRange>,
    deep_parse_enabled: Option<bool>,
    blacklist_terms_enabled: Option<bool>,
) -> CommandResult<WordCloudResult> {
    let mut config = config
        .lock()
        .map_err(|error| super::state_error("read_word_cloud_config", error))?
        .clone();
    let source = source.unwrap_or(match config.visualization_data_source {
        VisualizationDataSource::RecentAccess => WordCloudSource::Access,
        VisualizationDataSource::RecentCleanup => WordCloudSource::Cleanup,
    });
    let range = range.unwrap_or_else(|| config.visualization_data_range.into());
    if let Some(enabled) = deep_parse_enabled {
        config.visualization_deep_parse = enabled;
    }
    if let Some(enabled) = blacklist_terms_enabled {
        config.word_cloud_show_blacklist_terms = enabled;
    }
    let backend = app.state::<QuickAccessBackendState>().inner().clone();
    let app_for_worker = app.clone();
    run_blocking("get_dashboard_word_cloud", move || {
        let database = app_for_worker.state::<DbState>();
        visualization::build_word_cloud(source, range, &backend, database.inner(), &config).map_err(
            |error| {
                CommandError::unexpected(
                    "get_dashboard_word_cloud",
                    ErrorCode::InternalUnexpected,
                    "The word cloud could not be generated.",
                    true,
                    error,
                )
            },
        )
    })
    .await
}
