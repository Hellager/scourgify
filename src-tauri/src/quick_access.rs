use anyhow::{bail, Result};
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use wincent::prelude::{
    AddOptions, BatchOptions, FrequentRestoreReport, QuickAccess, QuickAccessItem,
    QuickAccessManager, RecentRestoreReport, RemoveOptions, RestoreDefaultsOptions,
    RestoreDefaultsReport, VisibilityOptions,
};

#[derive(Debug, Clone, Serialize)]
pub struct QaItem {
    pub path: String,
    pub name: String,
    pub last_interaction_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaCounts {
    pub recent: usize,
    pub frequent: usize,
    pub all: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct QaBatchResult {
    pub total: usize,
    pub succeeded: Vec<String>,
    pub failed: Vec<QaBatchFailure>,
    pub skipped_protected: Vec<String>,
    pub history_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct QaBatchFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaRestoreResult {
    pub success: bool,
    pub recent: Option<QaRestoreSectionResult>,
    pub frequent: Option<QaRestoreSectionResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaRestoreSectionResult {
    pub success: bool,
    pub deleted_lnk_count: usize,
    pub error: Option<String>,
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
    let interaction_times = interaction_times(&manager, qa_type);

    Ok(items
        .into_iter()
        .map(|path| QaItem {
            name: item_name(&path),
            last_interaction_at: interaction_times.get(&path.to_lowercase()).copied(),
            path,
        })
        .collect())
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

pub fn get_counts() -> Result<QaCounts> {
    let manager = QuickAccessManager::new();
    log::debug!("wincent counts started");

    let recent = get_items_logged(&manager, QuickAccess::RecentFiles, "count")?.len();
    let frequent = get_items_logged(&manager, QuickAccess::FrequentFolders, "count")?.len();
    let all = get_items_logged(&manager, QuickAccess::All, "count")?.len();

    log::debug!("wincent counts succeeded recent={recent} frequent={frequent} all={all}");
    Ok(QaCounts {
        recent,
        frequent,
        all,
    })
}

pub fn pin_folder(path: &str) -> Result<()> {
    let path = validate_pin_folder_path(path)?;
    log::info!("wincent pin folder started path={}", path.display());

    match QuickAccessManager::new().add_item(
        &path,
        QuickAccess::FrequentFolders,
        AddOptions::new().refresh_explorer(),
    ) {
        Ok(()) => {
            log::info!("wincent pin folder succeeded path={}", path.display());
            Ok(())
        }
        Err(error) => {
            log::error!(
                "wincent pin folder failed path={} error={error}",
                path.display()
            );
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
        RemoveOptions::new().refresh_explorer(),
    );
    log_batch_result("remove", qa_type, &result);

    Ok(to_batch_result(result))
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

fn parse_qa_type(qa_type: &str) -> Result<QuickAccess> {
    match qa_type {
        "all" => Ok(QuickAccess::All),
        "recent" => Ok(QuickAccess::RecentFiles),
        "frequent" => Ok(QuickAccess::FrequentFolders),
        _ => bail!("unsupported Quick Access type: {qa_type}"),
    }
}

fn parse_write_qa_type(qa_type: &str) -> Result<QuickAccess> {
    match qa_type {
        "recent" => Ok(QuickAccess::RecentFiles),
        "frequent" => Ok(QuickAccess::FrequentFolders),
        _ => bail!("unsupported Quick Access write type: {qa_type}"),
    }
}

fn parse_visibility_qa_type(qa_type: &str) -> Result<VisibilityTarget> {
    match qa_type {
        "recent" => Ok(VisibilityTarget::Recent),
        "frequent" => Ok(VisibilityTarget::Frequent),
        "start_recommended" => Ok(VisibilityTarget::StartRecommended),
        _ => bail!("unsupported Quick Access visibility type: {qa_type}"),
    }
}

fn visibility_name(target: VisibilityTarget) -> &'static str {
    match target {
        VisibilityTarget::Recent => "recent",
        VisibilityTarget::Frequent => "frequent",
        VisibilityTarget::StartRecommended => "start_recommended",
    }
}

fn validate_pin_folder_path(path: &str) -> Result<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        bail!("Folder path is empty");
    }

    let path = PathBuf::from(path);
    if !path.exists() {
        bail!("Folder path does not exist: {}", path.display());
    }
    if !path.is_dir() {
        bail!("Path is not a folder: {}", path.display());
    }

    Ok(path)
}

fn to_batch_result(result: wincent::prelude::BatchResult) -> QaBatchResult {
    let total = result.total();
    let (succeeded, failed) = result.into_parts();

    QaBatchResult {
        total,
        succeeded,
        failed: failed
            .into_iter()
            .map(|failure| QaBatchFailure {
                path: failure.path().to_string(),
                error: failure.error().to_string(),
            })
            .collect(),
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
        error: report.error().map(|error| error.to_string()),
    }
}

fn frequent_restore_section(report: &FrequentRestoreReport) -> QaRestoreSectionResult {
    QaRestoreSectionResult {
        success: report.success(),
        deleted_lnk_count: report.deleted_lnk_paths().len(),
        error: report.error().map(|error| error.to_string()),
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
    let succeeded = result.succeeded().len();
    let failed = result.failed().len();

    if failed == 0 {
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
        log::warn!(
            "wincent {operation} item failed qa_type={} path={} error={}",
            qa_name(qa_type),
            failure.path(),
            failure.error()
        );
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
        let result = to_batch_result(wincent::prelude::BatchResult::default());

        assert_eq!(result.total, 0);
        assert!(result.succeeded.is_empty());
        assert!(result.failed.is_empty());
        assert!(result.skipped_protected.is_empty());
        assert_eq!(result.history_error, None);
    }

    #[test]
    fn rejects_unknown_qa_type() {
        let error = parse_qa_type("unknown").unwrap_err().to_string();

        assert!(error.contains("unsupported Quick Access type"));
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
    fn rejects_empty_pin_folder_path() {
        let error = validate_pin_folder_path("   ").unwrap_err().to_string();

        assert!(error.contains("Folder path is empty"));
    }

    #[test]
    fn rejects_missing_pin_folder_path() {
        let path = std::env::temp_dir().join(format!(
            "scourgify-missing-pin-folder-{}",
            std::process::id()
        ));
        let error = validate_pin_folder_path(path.to_string_lossy().as_ref())
            .unwrap_err()
            .to_string();

        assert!(error.contains("Folder path does not exist"));
    }

    #[test]
    fn rejects_file_pin_folder_path() {
        let path =
            std::env::temp_dir().join(format!("scourgify-pin-folder-file-{}", std::process::id()));
        std::fs::write(&path, b"test").unwrap();
        let error = validate_pin_folder_path(path.to_string_lossy().as_ref())
            .unwrap_err()
            .to_string();
        std::fs::remove_file(&path).unwrap();

        assert!(error.contains("Path is not a folder"));
    }

    #[test]
    fn accepts_existing_pin_folder_path() {
        assert!(validate_pin_folder_path(std::env::temp_dir().to_string_lossy().as_ref()).is_ok());
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
