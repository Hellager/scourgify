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
pub(crate) struct QaItem {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) last_interaction_at: Option<u64>,
    pub(crate) pinned: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub(crate) struct QaCounts {
    pub(crate) recent: usize,
    pub(crate) frequent: usize,
    pub(crate) all: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct QaItemMetadata {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) last_interaction_at: Option<u64>,
    pub(crate) access_count: u32,
    pub(crate) score: Option<f32>,
    pub(crate) recent_rank: i32,
    pub(crate) mru_position: u64,
    pub(crate) pinned: bool,
    pub(crate) pin_order: Option<i32>,
    pub(crate) warning_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
/// Describes a best-effort Quick Access removal batch.
///
/// `succeeded` contains completed primary mutations, `failed` contains
/// per-item failures, and `warnings` contains post-mutation problems for items
/// whose primary mutation completed. Protected paths are never submitted to
/// Windows. A `history_error` means Windows changes already happened and were
/// not rolled back.
pub(crate) struct QaBatchResult {
    pub(crate) total: usize,
    pub(crate) succeeded: Vec<String>,
    pub(crate) failed: Vec<QaBatchFailure>,
    pub(crate) warnings: Vec<QaBatchWarning>,
    pub(crate) skipped_protected: Vec<String>,
    pub(crate) history_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaBatchFailure {
    pub(crate) path: String,
    pub(crate) error: CommandError,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaBatchWarning {
    pub(crate) path: String,
    pub(crate) warning: CommandWarning,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaMutationResult {
    pub(crate) action: &'static str,
    pub(crate) target: String,
    pub(crate) affected: u64,
    pub(crate) warnings: Vec<CommandWarning>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaRestoreResult {
    pub(crate) success: bool,
    pub(crate) recent: Option<QaRestoreSectionResult>,
    pub(crate) frequent: Option<QaRestoreSectionResult>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaRestoreSectionResult {
    pub(crate) success: bool,
    pub(crate) deleted_lnk_count: usize,
    pub(crate) recent_files_cleared: Option<bool>,
    pub(crate) backing_file_deleted: Option<bool>,
    pub(crate) rebuilt: Option<bool>,
    pub(crate) non_default_raw_path_count: usize,
    pub(crate) raw_path_cleanup: Option<QaRawPathCleanupResult>,
    pub(crate) error: Option<CommandError>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaRawPathCleanupResult {
    pub(crate) success: bool,
    pub(crate) requested_count: usize,
    pub(crate) backing_file_deleted: bool,
    pub(crate) rebuilt: bool,
    pub(crate) remaining_count: usize,
    pub(crate) error: Option<CommandError>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub(crate) struct QaVisibility {
    pub(crate) recent: bool,
    pub(crate) frequent: bool,
    pub(crate) start_recommended: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QaVisibilityMutationResult {
    pub(crate) target: String,
    pub(crate) requested_visible: bool,
    pub(crate) visibility: Option<QaVisibility>,
    pub(crate) warnings: Vec<CommandWarning>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_visibility_mutation_with_unavailable_readback() {
        let result = QaVisibilityMutationResult {
            target: "recent".to_string(),
            requested_visible: false,
            visibility: None,
            warnings: Vec::new(),
        };

        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::json!({
                "target": "recent",
                "requested_visible": false,
                "visibility": null,
                "warnings": []
            })
        );
    }
}
