use std::sync::Mutex;

use tauri::State;

use crate::{
    backend::QuickAccessBackendState,
    config::Config,
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
    visualization::{self, WordCloudRange, WordCloudResult, WordCloudSource},
};

#[tauri::command]
pub(crate) fn get_dashboard_word_cloud(
    backend: State<'_, QuickAccessBackendState>,
    database: State<'_, DbState>,
    config: State<'_, Mutex<Config>>,
    source: WordCloudSource,
    range: Option<WordCloudRange>,
) -> CommandResult<WordCloudResult> {
    if matches!(source, WordCloudSource::Access) && range.is_some()
        || matches!(source, WordCloudSource::Cleanup) && range.is_none()
    {
        return Err(CommandError::expected(
            "get_dashboard_word_cloud",
            ErrorCode::ValidationInvalidArgument,
            "The word cloud request is invalid.",
            false,
            "the requested source and range do not match",
        ));
    }

    let config = config
        .lock()
        .map_err(|error| super::state_error("read_word_cloud_config", error))?
        .clone();
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
