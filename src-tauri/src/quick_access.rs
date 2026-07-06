use anyhow::{bail, Result};
use serde::Serialize;
use std::path::Path;
use wincent::prelude::{QuickAccess, QuickAccessManager};

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

pub fn list_items(qa_type: &str) -> Result<Vec<QaItem>> {
    let manager = QuickAccessManager::new();
    let items = manager.get_items(parse_qa_type(qa_type)?)?;

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

    Ok(QaCounts {
        recent: manager.get_items(QuickAccess::RecentFiles)?.len(),
        frequent: manager.get_items(QuickAccess::FrequentFolders)?.len(),
        all: manager.get_items(QuickAccess::All)?.len(),
    })
}

fn parse_qa_type(qa_type: &str) -> Result<QuickAccess> {
    match qa_type {
        "all" => Ok(QuickAccess::All),
        "recent" => Ok(QuickAccess::RecentFiles),
        "frequent" => Ok(QuickAccess::FrequentFolders),
        _ => bail!("unsupported Quick Access type: {qa_type}"),
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
