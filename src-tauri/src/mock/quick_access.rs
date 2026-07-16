use anyhow::{anyhow, Result};
use fake::{
    faker::filesystem::en::{DirPath, FileName},
    Fake, Faker,
};
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

const RECENT_FILE_COUNT: usize = 20;
const FREQUENT_FOLDER_COUNT: usize = 8;

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
        let scenario = data.scenario;
        let revision = data.revision.wrapping_add(1);
        *data = seed(scenario);
        data.revision = revision;
        Ok(snapshot(&data))
    }

    pub(crate) fn trigger_change(&self, qa_type: &str) -> Result<MockSnapshot> {
        let mut data = self
            .data
            .write()
            .map_err(|error| anyhow!("mock Quick Access state is unavailable: {error}"))?;
        let revision = data.revision as usize;
        let (items, change_index) = match qa_type {
            "recent" => (&mut data.recent, RECENT_FILE_COUNT + revision),
            "frequent" => (&mut data.frequent, FREQUENT_FOLDER_COUNT + revision),
            _ => return Err(QuickAccessError::UnsupportedWriteType(qa_type.to_string()).into()),
        };
        let changed = if qa_type == "recent" {
            random_file(change_index)
        } else {
            random_folder(change_index)
        };
        if let Some(first) = items.first_mut() {
            *first = changed;
        } else {
            items.push(changed);
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
    let recent = if scenario != MockScenario::Empty {
        (0..RECENT_FILE_COUNT).map(random_file).collect()
    } else {
        Vec::new()
    };
    let frequent = if scenario != MockScenario::Empty {
        (0..FREQUENT_FOLDER_COUNT).map(random_folder).collect()
    } else {
        Vec::new()
    };
    let mut data = MockData {
        scenario,
        revision: 1,
        recent_metadata: Vec::new(),
        frequent_metadata: Vec::new(),
        recent,
        frequent,
        visibility: QaVisibility {
            recent: true,
            frequent: true,
            start_recommended: true,
        },
    };
    sync_metadata(&mut data);
    data
}

fn item(path: &str, pinned: bool) -> QaItem {
    QaItem {
        path: path.to_string(),
        name: path.rsplit('\\').next().unwrap_or(path).to_string(),
        last_interaction_at: Some(random_timestamp()),
        pinned: path.contains("Folders").then_some(pinned),
    }
}

fn metadata(item: &QaItem, index: usize) -> QaItemMetadata {
    QaItemMetadata {
        path: item.path.clone(),
        name: item.name.clone(),
        last_interaction_at: item.last_interaction_at,
        access_count: (1..=100).fake(),
        score: Some((0.0..=1.0).fake()),
        recent_rank: i32::try_from(index + 1).unwrap_or(i32::MAX),
        mru_position: u64::try_from(index + 1).unwrap_or(u64::MAX),
        pinned: item.pinned.unwrap_or(false),
        pin_order: item
            .pinned
            .filter(|pinned| *pinned)
            .map(|_| (1..=20).fake()),
        warning_count: (0..=2).fake(),
    }
}

fn random_timestamp() -> u64 {
    let now = u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or_default();
    let earliest = now.saturating_sub(90 * 24 * 60 * 60 * 1_000);
    (earliest..=now).fake()
}

fn random_file(index: usize) -> QaItem {
    let generated: String = FileName().fake();
    let name = unique_component(index, &generated, "item.txt");
    item(&format!(r"C:\Mock\Recent\{name}"), false)
}

fn random_folder(index: usize) -> QaItem {
    let generated: String = DirPath().fake();
    let generated = generated
        .split(['/', '\\'])
        .rfind(|part| !part.is_empty())
        .unwrap_or("Folder");
    let name = unique_component(index, generated, "Folder");
    let pinned: bool = Faker.fake();
    item(&format!(r"C:\Mock\Folders\{name}"), pinned)
}

fn unique_component(index: usize, generated: &str, fallback: &str) -> String {
    let sanitized = generated
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            _ => character,
        })
        .collect::<String>();
    let sanitized = sanitized.trim_matches([' ', '.']);
    let value = if sanitized.is_empty() {
        fallback
    } else {
        sanitized
    };
    format!("{:02}-{value}", index + 1)
}

fn sync_metadata(data: &mut MockData) {
    data.recent_metadata = data
        .recent
        .iter()
        .enumerate()
        .map(|(index, item)| metadata(item, index))
        .collect();
    data.frequent_metadata = data
        .frequent
        .iter()
        .enumerate()
        .map(|(index, item)| metadata(item, index))
        .collect();
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
    fn scenarios_keep_fixed_counts_and_apply_partial_failures() {
        let backend = MockQuickAccessBackend::new();
        let snapshot = backend.snapshot().unwrap();
        assert_eq!(snapshot.recent.len(), RECENT_FILE_COUNT);
        assert_eq!(snapshot.frequent.len(), FREQUENT_FOLDER_COUNT);
        assert_eq!(snapshot.recent_metadata.len(), RECENT_FILE_COUNT);
        assert_eq!(snapshot.frequent_metadata.len(), FREQUENT_FOLDER_COUNT);
        assert!(snapshot
            .recent_metadata
            .iter()
            .all(|item| item.access_count >= 1
                && item.access_count <= 100
                && item.score.is_some_and(|score| (0.0..=1.0).contains(&score))
                && item.warning_count <= 2));

        backend.set_scenario(MockScenario::PartialFailure).unwrap();
        let paths = backend
            .list_items("recent")
            .unwrap()
            .into_iter()
            .map(|item| item.path)
            .collect();
        let result = backend.remove_items("recent", paths).unwrap();

        assert_eq!(result.succeeded.len(), RECENT_FILE_COUNT / 2);
        assert_eq!(result.failed.len(), RECENT_FILE_COUNT / 2);
        assert_eq!(
            backend.list_items("recent").unwrap().len(),
            RECENT_FILE_COUNT / 2
        );
    }

    #[test]
    fn manual_change_toggles_an_item_and_revision() {
        let backend = MockQuickAccessBackend::new();
        let before = backend.snapshot().unwrap();

        let changed = backend.trigger_change("frequent").unwrap();

        assert_eq!(changed.frequent.len(), before.frequent.len());
        assert_ne!(changed.frequent[0].path, before.frequent[0].path);
        assert_eq!(changed.revision, before.revision + 1);
        assert_eq!(changed.frequent_metadata.len(), changed.frequent.len());
    }
}
