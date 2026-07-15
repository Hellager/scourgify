use anyhow::Result;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use thiserror::Error;
use wincent::prelude::{
    AddOptions, BatchOptions, DestListEntry, FrequentFolderPinStatus, FrequentRawPathRemoveReport,
    FrequentRestoreReport, QuickAccess, QuickAccessItem, QuickAccessManager, RecentRestoreReport,
    RemoveOptions, RestoreDefaultsOptions, RestoreDefaultsReport, VisibilityOptions, WincentError,
};

use crate::error::{
    wincent_command_error_ref, wincent_post_mutation_warning, CommandError, CommandWarning,
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisibilityTarget {
    Recent,
    Frequent,
    StartRecommended,
}

pub fn list_items(qa_type: &str) -> Result<Vec<QaItem>> {
    let manager = QuickAccessManager::new();
    let qa_type = parse_qa_type(qa_type)?;
    let items = get_items_logged(&manager, qa_type, "list")?;

    Ok(items_from_paths_with_manager(&manager, qa_type, items))
}

pub fn list_item_metadata(qa_type: &str) -> Result<Vec<QaItemMetadata>> {
    let manager = QuickAccessManager::new();
    let qa_type = parse_write_qa_type(qa_type)?;
    let entries = match qa_type {
        QuickAccess::RecentFiles => manager.get_recent_files_metadata(),
        QuickAccess::FrequentFolders => manager.get_frequent_folders_metadata(),
        QuickAccess::All => unreachable!("metadata rejects QuickAccess::All"),
        _ => unreachable!("wincent QuickAccess gained an unsupported variant"),
    }?;

    log::debug!(
        "wincent item metadata succeeded qa_type={} count={}",
        qa_name(qa_type),
        entries.len()
    );
    Ok(entries.into_iter().map(metadata_from_entry).collect())
}

fn metadata_from_entry(entry: DestListEntry) -> QaItemMetadata {
    QaItemMetadata {
        path: entry.path().to_string(),
        name: item_name(entry.path()),
        last_interaction_at: entry
            .last_interaction_filetime()
            .and_then(filetime_to_unix_ms),
        access_count: entry.access_count(),
        score: entry.score().is_finite().then_some(entry.score()),
        recent_rank: entry.recent_rank(),
        mru_position: entry.mru_position() as u64,
        pinned: entry.is_pinned(),
        pin_order: entry.pin_order(),
        warning_count: entry.warnings().len(),
    }
}

pub(crate) fn items_from_paths(qa_type: QuickAccess, items: Vec<String>) -> Vec<QaItem> {
    items_from_paths_with_manager(&QuickAccessManager::new(), qa_type, items)
}

fn items_from_paths_with_manager(
    manager: &QuickAccessManager,
    qa_type: QuickAccess,
    items: Vec<String>,
) -> Vec<QaItem> {
    let interaction_times = interaction_times(manager, qa_type);

    items
        .into_iter()
        .map(|path| QaItem {
            name: item_name(&path),
            last_interaction_at: interaction_times.get(&path.to_lowercase()).copied(),
            pinned: frequent_folder_pinned(manager, qa_type, &path),
            path,
        })
        .collect()
}

fn frequent_folder_pinned(
    manager: &QuickAccessManager,
    qa_type: QuickAccess,
    path: &str,
) -> Option<bool> {
    if qa_type != QuickAccess::FrequentFolders {
        return None;
    }

    match manager.frequent_folder_pin_status(path) {
        Ok(status) => {
            let pinned = pin_status_value(status);
            if pinned.is_none() {
                log::warn!("wincent pin status missing for listed frequent folder path={path}");
            }
            pinned
        }
        Err(error) => {
            log::warn!("wincent pin status unavailable path={path} error={error}");
            None
        }
    }
}

fn pin_status_value(status: FrequentFolderPinStatus) -> Option<bool> {
    match status {
        FrequentFolderPinStatus::Pinned => Some(true),
        FrequentFolderPinStatus::Unpinned => Some(false),
        FrequentFolderPinStatus::NotFound => None,
    }
}

const FILETIME_UNIX_EPOCH_OFFSET: u64 = 116_444_736_000_000_000;

fn interaction_times(manager: &QuickAccessManager, qa_type: QuickAccess) -> HashMap<String, u64> {
    let entries = match qa_type {
        QuickAccess::RecentFiles => manager.get_recent_files_metadata(),
        QuickAccess::FrequentFolders => manager.get_frequent_folders_metadata(),
        _ => return HashMap::new(),
    };

    match entries {
        Ok(entries) => entries
            .into_iter()
            .filter_map(|entry| {
                entry
                    .last_interaction_filetime()
                    .and_then(filetime_to_unix_ms)
                    .map(|timestamp| (entry.path().to_lowercase(), timestamp))
            })
            .collect(),
        Err(error) => {
            log::warn!("wincent metadata unavailable; using path-only items: {error}");
            HashMap::new()
        }
    }
}

fn filetime_to_unix_ms(filetime: u64) -> Option<u64> {
    filetime
        .checked_sub(FILETIME_UNIX_EPOCH_OFFSET)
        .map(|ticks| ticks / 10_000)
}

pub fn add_item(qa_type: &str, path: &str) -> Result<()> {
    let qa_type = parse_write_qa_type(qa_type)?;
    let path = validate_add_item_path(qa_type, path)?;
    log::info!(
        "wincent add item started qa_type={} path={}",
        qa_name(qa_type),
        path.display()
    );

    match QuickAccessManager::new().add_item(&path, qa_type, AddOptions::new().refresh_explorer()) {
        Ok(()) => {
            log::info!(
                "wincent add item succeeded qa_type={} path={}",
                qa_name(qa_type),
                path.display()
            );
            Ok(())
        }
        Err(error) => {
            if matches!(&error, WincentError::PostMutationFailure { .. }) {
                log::warn!(
                    "wincent add item completed with post-mutation warning qa_type={} path={} error={error}",
                    qa_name(qa_type),
                    path.display()
                );
            } else {
                log::error!(
                    "wincent add item failed qa_type={} path={} error={error}",
                    qa_name(qa_type),
                    path.display()
                );
            }
            Err(error.into())
        }
    }
}

pub fn remove_items(qa_type: &str, paths: Vec<String>) -> Result<QaBatchResult> {
    let qa_type = parse_write_qa_type(qa_type)?;
    log::info!(
        "wincent remove batch started qa_type={} total={}",
        qa_name(qa_type),
        paths.len()
    );
    let items: Vec<QuickAccessItem> = paths
        .into_iter()
        .map(|path| QuickAccessItem::new(path, qa_type))
        .collect();
    let result = QuickAccessManager::new().remove_items_batch_with_batch_options(
        &items,
        BatchOptions::new().refresh_explorer(),
        default_remove_options(),
    );
    log_batch_result("remove", qa_type, &result);

    Ok(to_batch_result("quick_access_remove_batch", result))
}

fn default_remove_options() -> RemoveOptions {
    RemoveOptions::new().deep_clean_recent_links()
}

pub fn restore_defaults(qa_type: &str) -> Result<QaRestoreResult> {
    let qa_type = parse_qa_type(qa_type)?;
    log::info!(
        "wincent restore defaults started qa_type={}",
        qa_name(qa_type)
    );

    match QuickAccessManager::new().restore_defaults(qa_type, RestoreDefaultsOptions::new()) {
        Ok(report) => {
            let result = to_restore_result(&report);
            log::info!(
                "wincent restore defaults finished qa_type={} success={}",
                qa_name(qa_type),
                result.success
            );
            Ok(result)
        }
        Err(error) => {
            log::error!(
                "wincent restore defaults failed qa_type={} error={error}",
                qa_name(qa_type)
            );
            Err(error.into())
        }
    }
}

pub fn get_visibility() -> Result<QaVisibility> {
    let manager = QuickAccessManager::new();

    Ok(QaVisibility {
        recent: manager.is_visible(QuickAccess::RecentFiles)?,
        frequent: manager.is_visible(QuickAccess::FrequentFolders)?,
        start_recommended: manager.is_start_recommended_section_visible()?,
    })
}

pub fn set_visibility(qa_type: &str, visible: bool) -> Result<QaVisibility> {
    let target = parse_visibility_qa_type(qa_type)?;
    log::info!(
        "Quick Access visibility update started qa_type={} visible={visible}",
        visibility_name(target)
    );

    let manager = QuickAccessManager::new();
    let options = VisibilityOptions::new().refresh_explorer();
    let result = match target {
        VisibilityTarget::Recent => {
            manager.set_visible_with_options(QuickAccess::RecentFiles, visible, options)
        }
        VisibilityTarget::Frequent => {
            manager.set_visible_with_options(QuickAccess::FrequentFolders, visible, options)
        }
        VisibilityTarget::StartRecommended => {
            manager.set_start_recommended_section_visible_with_options(visible, options)
        }
    };
    match result {
        Ok(()) => {
            log::info!(
                "Quick Access visibility update succeeded qa_type={} visible={visible}",
                visibility_name(target)
            );
            get_visibility()
        }
        Err(error) => {
            log::error!(
                "Quick Access visibility update failed qa_type={} visible={visible} error={error}",
                visibility_name(target)
            );
            Err(error.into())
        }
    }
}

pub(crate) fn parse_qa_type(qa_type: &str) -> Result<QuickAccess> {
    match qa_type {
        "all" => Ok(QuickAccess::All),
        "recent" => Ok(QuickAccess::RecentFiles),
        "frequent" => Ok(QuickAccess::FrequentFolders),
        _ => Err(QuickAccessError::UnsupportedType(qa_type.to_string()).into()),
    }
}

fn parse_write_qa_type(qa_type: &str) -> Result<QuickAccess> {
    match qa_type {
        "recent" => Ok(QuickAccess::RecentFiles),
        "frequent" => Ok(QuickAccess::FrequentFolders),
        _ => Err(QuickAccessError::UnsupportedWriteType(qa_type.to_string()).into()),
    }
}

fn parse_visibility_qa_type(qa_type: &str) -> Result<VisibilityTarget> {
    match qa_type {
        "recent" => Ok(VisibilityTarget::Recent),
        "frequent" => Ok(VisibilityTarget::Frequent),
        "start_recommended" => Ok(VisibilityTarget::StartRecommended),
        _ => Err(QuickAccessError::UnsupportedVisibilityType(qa_type.to_string()).into()),
    }
}

fn visibility_name(target: VisibilityTarget) -> &'static str {
    match target {
        VisibilityTarget::Recent => "recent",
        VisibilityTarget::Frequent => "frequent",
        VisibilityTarget::StartRecommended => "start_recommended",
    }
}

fn validate_add_item_path(qa_type: QuickAccess, path: &str) -> Result<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return Err(QuickAccessError::EmptyItemPath.into());
    }

    let path = PathBuf::from(path);
    if !path.exists() {
        return Err(QuickAccessError::ItemNotFound(path.display().to_string()).into());
    }
    match qa_type {
        QuickAccess::RecentFiles if !path.is_file() => {
            return Err(QuickAccessError::NotAFile(path.display().to_string()).into());
        }
        QuickAccess::FrequentFolders if !path.is_dir() => {
            return Err(QuickAccessError::NotAFolder(path.display().to_string()).into());
        }
        _ => {}
    }

    Ok(path)
}

fn to_batch_result(operation: &str, result: wincent::prelude::BatchResult) -> QaBatchResult {
    let total = result.total();
    let (mut succeeded, failed) = result.into_parts();
    let mut command_failures = Vec::new();
    let mut warnings = Vec::new();

    for failure in failed {
        if let Some(warning) = wincent_post_mutation_warning(operation, failure.error()) {
            succeeded.push(failure.path().to_string());
            warnings.push(QaBatchWarning {
                path: failure.path().to_string(),
                warning,
            });
        } else {
            command_failures.push(QaBatchFailure {
                path: failure.path().to_string(),
                error: wincent_command_error_ref(operation, failure.error()),
            });
        }
    }

    QaBatchResult {
        total,
        succeeded,
        failed: command_failures,
        warnings,
        skipped_protected: Vec::new(),
        history_error: None,
    }
}

fn to_restore_result(report: &RestoreDefaultsReport) -> QaRestoreResult {
    QaRestoreResult {
        success: report.success(),
        recent: report.recent_report().map(recent_restore_section),
        frequent: report.frequent_report().map(frequent_restore_section),
    }
}

fn recent_restore_section(report: &RecentRestoreReport) -> QaRestoreSectionResult {
    QaRestoreSectionResult {
        success: report.success(),
        deleted_lnk_count: report.deleted_lnk_paths().len(),
        recent_files_cleared: Some(report.recent_files_cleared()),
        backing_file_deleted: None,
        rebuilt: None,
        non_default_raw_path_count: 0,
        raw_path_cleanup: None,
        error: report
            .error()
            .map(|error| wincent_command_error_ref("restore_qa_defaults.recent", error)),
    }
}

fn frequent_restore_section(report: &FrequentRestoreReport) -> QaRestoreSectionResult {
    QaRestoreSectionResult {
        success: report.success(),
        deleted_lnk_count: report.deleted_lnk_paths().len(),
        recent_files_cleared: None,
        backing_file_deleted: Some(report.backing_file_deleted()),
        rebuilt: Some(report.rebuilt()),
        non_default_raw_path_count: report.non_default_raw_paths().len(),
        raw_path_cleanup: report.raw_path_remove_report().map(raw_path_cleanup_result),
        error: report
            .error()
            .map(|error| wincent_command_error_ref("restore_qa_defaults.frequent", error)),
    }
}

fn raw_path_cleanup_result(report: &FrequentRawPathRemoveReport) -> QaRawPathCleanupResult {
    QaRawPathCleanupResult {
        success: report.success(),
        requested_count: report.requested_raw_paths().len(),
        backing_file_deleted: report.backing_file_deleted(),
        rebuilt: report.rebuilt(),
        remaining_count: report.remaining_non_default_raw_paths().len(),
        error: report
            .error()
            .map(|error| wincent_command_error_ref("restore_qa_defaults.raw_path_cleanup", error)),
    }
}

fn get_items_logged(
    manager: &QuickAccessManager,
    qa_type: QuickAccess,
    operation: &str,
) -> Result<Vec<String>> {
    log::debug!("wincent {operation} started qa_type={}", qa_name(qa_type));
    match manager.get_items(qa_type) {
        Ok(items) => {
            log::debug!(
                "wincent {operation} succeeded qa_type={} count={}",
                qa_name(qa_type),
                items.len()
            );
            Ok(items)
        }
        Err(error) => {
            log::error!(
                "wincent {operation} failed qa_type={} error={error}",
                qa_name(qa_type)
            );
            Err(error.into())
        }
    }
}

fn log_batch_result(operation: &str, qa_type: QuickAccess, result: &wincent::prelude::BatchResult) {
    let total = result.total();
    let post_mutation_warnings = result
        .failed()
        .iter()
        .filter(|failure| matches!(failure.error(), WincentError::PostMutationFailure { .. }))
        .count();
    let succeeded = result.succeeded().len() + post_mutation_warnings;
    let failed = result.failed().len() - post_mutation_warnings;

    if failed == 0 && post_mutation_warnings > 0 {
        log::warn!(
            "wincent {operation} batch completed with warnings qa_type={} total={total} succeeded={succeeded} warnings={post_mutation_warnings}",
            qa_name(qa_type)
        );
    } else if failed == 0 {
        log::info!(
            "wincent {operation} batch succeeded qa_type={} total={total} succeeded={succeeded}",
            qa_name(qa_type)
        );
    } else if succeeded == 0 {
        log::error!(
            "wincent {operation} batch failed qa_type={} total={total} failed={failed}",
            qa_name(qa_type)
        );
    } else {
        log::warn!(
            "wincent {operation} batch partially failed qa_type={} total={total} succeeded={succeeded} failed={failed}",
            qa_name(qa_type)
        );
    }

    for failure in result.failed() {
        if matches!(failure.error(), WincentError::PostMutationFailure { .. }) {
            log::warn!(
                "wincent {operation} item completed with warning qa_type={} path={} error={}",
                qa_name(qa_type),
                failure.path(),
                failure.error()
            );
        } else {
            log::warn!(
                "wincent {operation} item failed qa_type={} path={} error={}",
                qa_name(qa_type),
                failure.path(),
                failure.error()
            );
        }
    }
}

fn qa_name(qa_type: QuickAccess) -> &'static str {
    match qa_type {
        QuickAccess::RecentFiles => "recent",
        QuickAccess::FrequentFolders => "frequent",
        QuickAccess::All => "all",
        _ => "unknown",
    }
}

fn item_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_qa_types() {
        assert_eq!(parse_qa_type("all").unwrap(), QuickAccess::All);
        assert_eq!(parse_qa_type("recent").unwrap(), QuickAccess::RecentFiles);
        assert_eq!(
            parse_qa_type("frequent").unwrap(),
            QuickAccess::FrequentFolders
        );
    }

    #[test]
    fn parses_supported_write_qa_types() {
        assert_eq!(
            parse_write_qa_type("recent").unwrap(),
            QuickAccess::RecentFiles
        );
        assert_eq!(
            parse_write_qa_type("frequent").unwrap(),
            QuickAccess::FrequentFolders
        );
    }

    #[test]
    fn rejects_all_for_write_qa_type() {
        let error = parse_write_qa_type("all").unwrap_err().to_string();

        assert!(error.contains("unsupported Quick Access write type"));
    }

    #[test]
    fn converts_empty_batch_result() {
        let result = to_batch_result("test", wincent::prelude::BatchResult::default());

        assert_eq!(result.total, 0);
        assert!(result.succeeded.is_empty());
        assert!(result.failed.is_empty());
        assert!(result.warnings.is_empty());
        assert!(result.skipped_protected.is_empty());
        assert_eq!(result.history_error, None);
    }

    #[test]
    fn serializes_item_pin_status() {
        let item = QaItem {
            path: r"C:\Work".to_string(),
            name: "Work".to_string(),
            last_interaction_at: None,
            pinned: Some(true),
        };

        assert_eq!(
            serde_json::to_value(item).unwrap(),
            serde_json::json!({
                "path": r"C:\Work",
                "name": "Work",
                "last_interaction_at": null,
                "pinned": true
            })
        );
    }

    #[test]
    fn maps_frequent_folder_pin_status() {
        assert_eq!(
            pin_status_value(FrequentFolderPinStatus::Pinned),
            Some(true)
        );
        assert_eq!(
            pin_status_value(FrequentFolderPinStatus::Unpinned),
            Some(false)
        );
        assert_eq!(pin_status_value(FrequentFolderPinStatus::NotFound), None);
    }

    #[test]
    fn rejects_unknown_qa_type() {
        let error = parse_qa_type("unknown").unwrap_err().to_string();

        assert!(error.contains("unsupported Quick Access type"));
    }

    #[test]
    fn default_remove_options_enable_deep_link_cleanup() {
        let options = default_remove_options();
        assert!(options.deep_clean_recent_links_enabled());
        assert!(!options.refresh_explorer_enabled());
    }

    #[test]
    fn parses_visibility_qa_types() {
        assert_eq!(
            parse_visibility_qa_type("recent").unwrap(),
            VisibilityTarget::Recent
        );
        assert_eq!(
            parse_visibility_qa_type("frequent").unwrap(),
            VisibilityTarget::Frequent
        );
        assert_eq!(
            parse_visibility_qa_type("start_recommended").unwrap(),
            VisibilityTarget::StartRecommended
        );
    }

    #[test]
    fn serializes_all_visibility_fields() {
        let visibility = QaVisibility {
            recent: true,
            frequent: false,
            start_recommended: true,
        };

        assert_eq!(
            serde_json::to_value(visibility).unwrap(),
            serde_json::json!({
                "recent": true,
                "frequent": false,
                "start_recommended": true
            })
        );
    }

    #[test]
    fn rejects_all_for_visibility_qa_type() {
        let error = parse_visibility_qa_type("all").unwrap_err().to_string();

        assert!(error.contains("unsupported Quick Access visibility type"));
    }

    #[test]
    fn rejects_empty_add_item_path() {
        let error = validate_add_item_path(QuickAccess::RecentFiles, "   ").unwrap_err();

        assert_eq!(
            error.downcast_ref::<QuickAccessError>(),
            Some(&QuickAccessError::EmptyItemPath)
        );
    }

    #[test]
    fn rejects_missing_add_item_path() {
        let path =
            std::env::temp_dir().join(format!("scourgify-missing-add-item-{}", std::process::id()));
        let error =
            validate_add_item_path(QuickAccess::RecentFiles, path.to_string_lossy().as_ref())
                .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<QuickAccessError>(),
            Some(QuickAccessError::ItemNotFound(_))
        ));
    }

    #[test]
    fn rejects_file_for_frequent_folders() {
        let path =
            std::env::temp_dir().join(format!("scourgify-add-item-file-{}", std::process::id()));
        std::fs::write(&path, b"test").unwrap();
        let error = validate_add_item_path(
            QuickAccess::FrequentFolders,
            path.to_string_lossy().as_ref(),
        )
        .unwrap_err();
        std::fs::remove_file(&path).unwrap();

        assert!(matches!(
            error.downcast_ref::<QuickAccessError>(),
            Some(QuickAccessError::NotAFolder(_))
        ));
    }

    #[test]
    fn rejects_folder_for_recent_files() {
        let error = validate_add_item_path(
            QuickAccess::RecentFiles,
            std::env::temp_dir().to_string_lossy().as_ref(),
        )
        .unwrap_err();

        assert!(matches!(
            error.downcast_ref::<QuickAccessError>(),
            Some(QuickAccessError::NotAFile(_))
        ));
    }

    #[test]
    fn accepts_existing_folder_for_frequent_folders() {
        assert!(validate_add_item_path(
            QuickAccess::FrequentFolders,
            std::env::temp_dir().to_string_lossy().as_ref()
        )
        .is_ok());
    }

    #[test]
    fn accepts_existing_file_for_recent_files() {
        let path =
            std::env::temp_dir().join(format!("scourgify-add-recent-file-{}", std::process::id()));
        std::fs::write(&path, b"test").unwrap();
        let result =
            validate_add_item_path(QuickAccess::RecentFiles, path.to_string_lossy().as_ref());
        std::fs::remove_file(&path).unwrap();

        assert!(result.is_ok());
    }

    #[test]
    fn derives_item_name_from_path() {
        assert_eq!(item_name(r"C:\Users\hp\report.txt"), "report.txt");
        assert_eq!(item_name(r"C:\Users\hp\Projects"), "Projects");
    }

    #[test]
    fn falls_back_to_path_when_name_is_unavailable() {
        assert_eq!(item_name(r"C:\"), r"C:\");
    }

    #[test]
    fn converts_windows_filetime_to_unix_milliseconds() {
        assert_eq!(filetime_to_unix_ms(FILETIME_UNIX_EPOCH_OFFSET), Some(0));
        assert_eq!(
            filetime_to_unix_ms(FILETIME_UNIX_EPOCH_OFFSET + 10_000),
            Some(1)
        );
        assert_eq!(filetime_to_unix_ms(FILETIME_UNIX_EPOCH_OFFSET - 1), None);
    }
}
