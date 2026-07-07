use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;
use wincent::prelude::{
    BatchOptions, EmptyOptions, QuickAccess, QuickAccessItem, QuickAccessManager, RemoveOptions,
};

#[derive(Debug, Clone, Serialize)]
pub struct QaItem {
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaCounts {
    pub recent: usize,
    pub frequent: usize,
    pub all: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaBatchResult {
    pub total: usize,
    pub succeeded: Vec<String>,
    pub failed: Vec<QaBatchFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QaBatchFailure {
    pub path: String,
    pub error: String,
}

pub fn list_items(qa_type: &str) -> Result<Vec<QaItem>> {
    let manager = QuickAccessManager::new();
    let qa_type = parse_qa_type(qa_type)?;
    let items = get_items_logged(&manager, qa_type, "list")?;

    Ok(items
        .into_iter()
        .map(|path| QaItem {
            name: item_name(&path),
            path,
        })
        .collect())
}

pub fn get_counts() -> Result<QaCounts> {
    let manager = QuickAccessManager::new();
    log::debug!("wincent counts started");

    let recent = get_items_logged(&manager, QuickAccess::RecentFiles, "count")?.len();
    let frequent = get_items_logged(&manager, QuickAccess::FrequentFolders, "count")?.len();
    let all = get_items_logged(&manager, QuickAccess::All, "count")?.len();

    log::debug!("wincent counts succeeded recent={recent} frequent={frequent} all={all}");
    Ok(QaCounts { recent, frequent, all })
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

pub fn empty_items(qa_type: &str) -> Result<()> {
    let qa_type = parse_write_qa_type(qa_type)?;
    log::info!("wincent empty started qa_type={}", qa_name(qa_type));

    match QuickAccessManager::new().empty_items(qa_type, EmptyOptions::new().refresh_explorer()) {
        Ok(()) => {
            log::info!("wincent empty succeeded qa_type={}", qa_name(qa_type));
            Ok(())
        }
        Err(error) => {
            log::error!(
                "wincent empty failed qa_type={} error={error}",
                qa_name(qa_type)
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
    }
}

fn get_items_logged(
    manager: &QuickAccessManager,
    qa_type: QuickAccess,
    operation: &str,
) -> Result<Vec<String>> {
    log::debug!(
        "wincent {operation} started qa_type={}",
        qa_name(qa_type)
    );
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
        assert_eq!(parse_write_qa_type("recent").unwrap(), QuickAccess::RecentFiles);
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
    }

    #[test]
    fn rejects_unknown_qa_type() {
        let error = parse_qa_type("unknown").unwrap_err().to_string();

        assert!(error.contains("unsupported Quick Access type"));
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
}
