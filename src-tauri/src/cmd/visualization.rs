use std::sync::Mutex;

use tauri::State;

use crate::{
    backend::QuickAccessBackendState,
    config::{Config, VisualizationDataSource},
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
    visualization::{self, WordCloudRange, WordCloudResult, WordCloudSource},
};

#[tauri::command]
pub(crate) fn get_dashboard_word_cloud(
    backend: State<'_, QuickAccessBackendState>,
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    source: Option<WordCloudSource>,
    range: Option<WordCloudRange>,
) -> CommandResult<WordCloudResult> {
    let config = config
        .lock()
        .map_err(|error| super::state_error("read_word_cloud_config", error))?
        .clone();
    let source = source.unwrap_or(match config.visualization_data_source {
        VisualizationDataSource::RecentAccess => WordCloudSource::Access,
        VisualizationDataSource::RecentCleanup => WordCloudSource::Cleanup,
    });
    let range = range.unwrap_or_else(|| config.visualization_data_range.into());
    visualization::build_word_cloud(source, range, backend.inner(), database.inner(), &config)
        .map_err(|error| {
            CommandError::unexpected(
                "get_dashboard_word_cloud",
                ErrorCode::InternalUnexpected,
                "The word cloud could not be generated.",
                true,
                error,
            )
        })
}
