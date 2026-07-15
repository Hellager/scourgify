use serde::Serialize;
use thiserror::Error;

use crate::error::{CommandError, CommandWarning};

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum QuickAccessError {
    #[error("unsupported Quick Access type: {0}")]
    UnsupportedType(String),
    #[error("unsupported Quick Access write type: {0}")]
    UnsupportedWriteType(String),
    #[error("unsupported Quick Access visibility type: {0}")]
    UnsupportedVisibilityType(String),
    #[error("item path is empty")]
    EmptyItemPath,
    #[error("item path does not exist: {0}")]
    ItemNotFound(String),
    #[error("Recent Files requires a file path: {0}")]
    NotAFile(String),
    #[error("path is not a folder: {0}")]
    NotAFolder(String),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaItem {
    pub path: String,
    pub name: String,
    pub last_interaction_at: Option<u64>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct QaCounts {
    pub recent: usize,
    pub frequent: usize,
    pub all: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct QaItemMetadata {
    pub path: String,
    pub name: String,
    pub last_interaction_at: Option<u64>,
    pub access_count: u32,
    pub score: Option<f32>,
    pub recent_rank: i32,
    pub mru_position: u64,
    pub pinned: bool,
    pub pin_order: Option<i32>,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct QaBatchResult {
    pub total: usize,
    pub succeeded: Vec<String>,
    pub failed: Vec<QaBatchFailure>,
    pub warnings: Vec<QaBatchWarning>,
    pub skipped_protected: Vec<String>,
    pub history_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaBatchFailure {
    pub path: String,
    pub error: CommandError,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaBatchWarning {
    pub path: String,
    pub warning: CommandWarning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaMutationResult {
    pub action: &'static str,
    pub target: String,
    pub affected: u64,
    pub warnings: Vec<CommandWarning>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaRestoreResult {
    pub success: bool,
    pub recent: Option<QaRestoreSectionResult>,
    pub frequent: Option<QaRestoreSectionResult>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaRestoreSectionResult {
    pub success: bool,
    pub deleted_lnk_count: usize,
    pub recent_files_cleared: Option<bool>,
    pub backing_file_deleted: Option<bool>,
    pub rebuilt: Option<bool>,
    pub non_default_raw_path_count: usize,
    pub raw_path_cleanup: Option<QaRawPathCleanupResult>,
    pub error: Option<CommandError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaRawPathCleanupResult {
    pub success: bool,
    pub requested_count: usize,
    pub backing_file_deleted: bool,
    pub rebuilt: bool,
    pub remaining_count: usize,
    pub error: Option<CommandError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaVisibility {
    pub recent: bool,
    pub frequent: bool,
    pub start_recommended: bool,
}
