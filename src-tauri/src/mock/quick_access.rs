use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::sync::RwLock;

use crate::{
    backend::{PathKind, QuickAccessBackend},
    error::{CommandError, CommandWarning, ErrorCode},
    quick_access::{
        QaBatchFailure, QaBatchResult, QaBatchWarning, QaItem, QaItemMetadata,
        QaRawPathCleanupResult, QaRestoreResult, QaRestoreSectionResult, QaVisibility,
        QuickAccessError,
    },
};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MockScenario {
    #[default]
    Normal,
    Empty,
    PartialFailure,
    PostMutationWarning,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MockSnapshot {
    pub scenario: MockScenario,
    pub revision: u64,
    pub recent: Vec<QaItem>,
    pub frequent: Vec<QaItem>,
    pub recent_metadata: Vec<QaItemMetadata>,
    pub frequent_metadata: Vec<QaItemMetadata>,
    pub visibility: QaVisibility,
}

#[derive(Debug, Clone)]
struct MockData {
    scenario: MockScenario,
    revision: u64,
    recent: Vec<QaItem>,
    frequent: Vec<QaItem>,
    recent_metadata: Vec<QaItemMetadata>,
    frequent_metadata: Vec<QaItemMetadata>,
    visibility: QaVisibility,
}

pub(crate) struct MockQuickAccessBackend {
    data: RwLock<MockData>,
}

impl MockQuickAccessBackend {
    pub(crate) fn new() -> Self {
        Self {
            data: RwLock::new(seed(MockScenario::Normal)),
        }
    }

    pub(crate) fn set_scenario(&self, scenario: MockScenario) -> Result<MockSnapshot> {
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        *data = seed(scenario);
        Ok(snapshot(&data))
    }

    pub(crate) fn snapshot(&self) -> Result<MockSnapshot> {
        let data = self
            .data
            .read()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        Ok(snapshot(&data))
    }

    pub(crate) fn refresh(&self) -> Result<MockSnapshot> {
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        data.revision = data.revision.wrapping_add(1);
        Ok(snapshot(&data))
    }

    pub(crate) fn trigger_change(&self, qa_type: &str) -> Result<MockSnapshot> {
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        let (items, path, pinned) = match qa_type {
            "recent" => (&mut data.recent, r"C:\Mock\Recent\event.txt", false),
            "frequent" => (&mut data.frequent, r"C:\Mock\Folders\Event", true),
            _ => return Err(QuickAccessError::UnsupportedWriteType(qa_type.to_string()).into()),
        };
        if items.iter().any(|item| item.path == path) {
            items.retain(|item| item.path != path);
        } else {
            items.push(item(path, pinned));
        }
        sync_metadata(&mut data);
        data.revision = data.revision.wrapping_add(1);
        Ok(snapshot(&data))
    }

    pub(crate) fn path_kind(&self, path: &str) -> PathKind {
        if path.trim().is_empty() {
            return PathKind::Missing;
        }
        if path.ends_with("\\") || path.contains("\\Folders\\") {
            PathKind::Directory
        } else if path.contains("\\Recent\\") || path.ends_with(".txt") || path.ends_with(".docx") {
            PathKind::File
        } else {
            PathKind::Missing
        }
    }
}

impl QuickAccessBackend for MockQuickAccessBackend {
    fn list_items(&self, qa_type: &str) -> Result<Vec<QaItem>> {
        let data = self
            .data
            .read()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        match qa_type {
            "recent" => Ok(data.recent.clone()),
            "frequent" => Ok(data.frequent.clone()),
            "all" => Ok(data
                .recent
                .iter()
                .chain(data.frequent.iter())
                .cloned()
                .collect()),
            _ => Err(QuickAccessError::UnsupportedType(qa_type.to_string()).into()),
        }
    }

    fn list_item_metadata(&self, qa_type: &str) -> Result<Vec<QaItemMetadata>> {
        let data = self
            .data
            .read()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        match qa_type {
            "recent" => Ok(data.recent_metadata.clone()),
            "frequent" => Ok(data.frequent_metadata.clone()),
            _ => Err(QuickAccessError::UnsupportedWriteType(qa_type.to_string()).into()),
        }
    }

    fn add_item(&self, qa_type: &str, path: &str) -> Result<()> {
        let kind = self.path_kind(path);
        if kind == PathKind::Missing {
            return Err(QuickAccessError::ItemNotFound(path.to_string()).into());
        }
        if qa_type == "recent" && kind != PathKind::File {
            return Err(QuickAccessError::NotAFile(path.to_string()).into());
        }
        if qa_type == "frequent" && kind != PathKind::Directory {
            return Err(QuickAccessError::NotAFolder(path.to_string()).into());
        }

        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        let items = match qa_type {
            "recent" => &mut data.recent,
            "frequent" => &mut data.frequent,
            _ => return Err(QuickAccessError::UnsupportedWriteType(qa_type.to_string()).into()),
        };
        if items
            .iter()
            .any(|item| item.path.eq_ignore_ascii_case(path))
        {
            return Err(anyhow!("mock item already exists: {path}"));
        }
        items.push(item(path, qa_type == "frequent"));
        sync_metadata(&mut data);
        data.revision = data.revision.wrapping_add(1);
        Ok(())
    }

    fn remove_items(&self, qa_type: &str, paths: Vec<String>) -> Result<QaBatchResult> {
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        let scenario = data.scenario;
        let items = match qa_type {
            "recent" => &mut data.recent,
            "frequent" => &mut data.frequent,
            _ => return Err(QuickAccessError::UnsupportedWriteType(qa_type.to_string()).into()),
        };
        let mut result = QaBatchResult {
            total: paths.len(),
            ..QaBatchResult::default()
        };

        for (index, path) in paths.into_iter().enumerate() {
            if scenario == MockScenario::PartialFailure && index % 2 == 1 {
                result
                    .failed
                    .push(failure(&path, ErrorCode::QuickAccessPermissionDenied));
                continue;
            }
            if scenario == MockScenario::PostMutationWarning && index == 0 {
                result.succeeded.push(path.clone());
                result.warnings.push(warning(&path));
            } else {
                result.succeeded.push(path.clone());
            }
            items.retain(|item| !item.path.eq_ignore_ascii_case(&path));
        }
        if !result.succeeded.is_empty() {
            sync_metadata(&mut data);
            data.revision = data.revision.wrapping_add(1);
        }
        Ok(result)
    }

    fn restore_defaults(&self, qa_type: &str) -> Result<QaRestoreResult> {
        if !matches!(qa_type, "recent" | "frequent" | "all") {
            return Err(QuickAccessError::UnsupportedType(qa_type.to_string()).into());
        }
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        let recent = matches!(qa_type, "recent" | "all").then(|| {
            data.recent.clear();
            QaRestoreSectionResult {
                success: true,
                deleted_lnk_count: 2,
                recent_files_cleared: Some(true),
                backing_file_deleted: None,
                rebuilt: None,
                non_default_raw_path_count: 0,
                raw_path_cleanup: None,
                error: None,
            }
        });
        let frequent = matches!(qa_type, "frequent" | "all").then(|| {
            data.frequent.clear();
            QaRestoreSectionResult {
                success: true,
                deleted_lnk_count: 1,
                recent_files_cleared: None,
                backing_file_deleted: Some(true),
                rebuilt: Some(true),
                non_default_raw_path_count: 0,
                raw_path_cleanup: Some(QaRawPathCleanupResult {
                    success: true,
                    requested_count: 1,
                    backing_file_deleted: true,
                    rebuilt: true,
                    remaining_count: 0,
                    error: None,
                }),
                error: None,
            }
        });
        sync_metadata(&mut data);
        data.revision = data.revision.wrapping_add(1);
        Ok(QaRestoreResult {
            success: true,
            recent,
            frequent,
        })
    }

    fn get_visibility(&self) -> Result<QaVisibility> {
        let data = self
            .data
            .read()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        Ok(data.visibility)
    }

    fn set_visibility(&self, qa_type: &str, visible: bool) -> Result<QaVisibility> {
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        match qa_type {
            "recent" => data.visibility.recent = visible,
            "frequent" => data.visibility.frequent = visible,
            "start_recommended" => data.visibility.start_recommended = visible,
            _ => {
                return Err(QuickAccessError::UnsupportedVisibilityType(qa_type.to_string()).into())
            }
        }
        data.revision = data.revision.wrapping_add(1);
        Ok(data.visibility)
    }

    fn path_kind(&self, path: &str) -> PathKind {
        self.path_kind(path)
    }
}

fn snapshot(data: &MockData) -> MockSnapshot {
    MockSnapshot {
        scenario: data.scenario,
        revision: data.revision,
        recent: data.recent.clone(),
        frequent: data.frequent.clone(),
        recent_metadata: data.recent_metadata.clone(),
        frequent_metadata: data.frequent_metadata.clone(),
        visibility: data.visibility,
    }
}

fn seed(scenario: MockScenario) -> MockData {
    let recent = if scenario == MockScenario::Empty {
        Vec::new()
    } else {
        vec![
            item(r"C:\Mock\Recent\report.docx", false),
            item(r"C:\Mock\Recent\notes.txt", false),
        ]
    };
    let frequent = if scenario == MockScenario::Empty {
        Vec::new()
    } else {
        vec![
            item(r"C:\Mock\Folders\Projects", true),
            item(r"C:\Mock\Folders\Archive", false),
        ]
    };
    MockData {
        scenario,
        revision: 1,
        recent_metadata: recent.iter().map(metadata).collect(),
        frequent_metadata: frequent.iter().map(metadata).collect(),
        recent,
        frequent,
        visibility: QaVisibility {
            recent: true,
            frequent: true,
            start_recommended: true,
        },
    }
}

fn item(path: &str, pinned: bool) -> QaItem {
    QaItem {
        path: path.to_string(),
        name: path.rsplit('\\').next().unwrap_or(path).to_string(),
        last_interaction_at: Some(1_735_689_600_000),
        pinned: path.contains("Folders").then_some(pinned),
    }
}

fn metadata(item: &QaItem) -> QaItemMetadata {
    QaItemMetadata {
        path: item.path.clone(),
        name: item.name.clone(),
        last_interaction_at: item.last_interaction_at,
        access_count: 4,
        score: Some(0.75),
        recent_rank: 1,
        mru_position: 1,
        pinned: item.pinned.unwrap_or(false),
        pin_order: item.pinned.filter(|pinned| *pinned).map(|_| 1),
        warning_count: 0,
    }
}

fn sync_metadata(data: &mut MockData) {
    data.recent_metadata = data.recent.iter().map(metadata).collect();
    data.frequent_metadata = data.frequent.iter().map(metadata).collect();
}

fn failure(path: &str, code: ErrorCode) -> QaBatchFailure {
    QaBatchFailure {
        path: path.to_string(),
        error: CommandError::expected(
            "mock_remove_qa_items",
            code,
            "The mock Quick Access operation failed.",
            false,
            format!("mock failure for {path}"),
        ),
    }
}

fn warning(path: &str) -> QaBatchWarning {
    QaBatchWarning {
        path: path.to_string(),
        warning: CommandWarning {
            code: "quick_access_post_mutation_failed",
            step: "refresh_explorer",
            message: "The mock Quick Access change completed with a warning.".to_string(),
            incident_id: "mock-warning".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_data_and_mutations_are_deterministic() {
        let backend = MockQuickAccessBackend::new();
        assert_eq!(backend.list_items("recent").unwrap().len(), 2);

        backend.set_scenario(MockScenario::PartialFailure).unwrap();
        let paths = backend
            .list_items("recent")
            .unwrap()
            .into_iter()
            .map(|item| item.path)
            .collect();
        let result = backend.remove_items("recent", paths).unwrap();

        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(backend.list_items("recent").unwrap().len(), 1);
    }

    #[test]
    fn manual_change_toggles_an_item_and_revision() {
        let backend = MockQuickAccessBackend::new();
        let before = backend.snapshot().unwrap();

        let changed = backend.trigger_change("frequent").unwrap();

        assert_eq!(changed.frequent.len(), before.frequent.len() + 1);
        assert_eq!(changed.revision, before.revision + 1);
        assert_eq!(changed.frequent_metadata.len(), changed.frequent.len());
    }
}
