use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use thiserror::Error;

use crate::rules::NewRule;

use super::rules;

const RULE_FILE_VERSION: u32 = 1;
const MAX_IMPORT_RULES: usize = 10_000;
const MAX_IMPORT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum RuleTransferError {
    #[error("invalid rule file path: {0}")]
    InvalidPath(&'static str),
    #[error("invalid rule file: {0}")]
    InvalidFormat(String),
    #[error("unsupported rule file version {0}")]
    UnsupportedVersion(u32),
    #[error("rule file contains no rules")]
    EmptyFile,
    #[error("rule file contains too many rules: {0}")]
    TooManyRules(usize),
    #[error("no rules were selected")]
    EmptySelection,
    #[error("selected rule index {0} is out of range")]
    InvalidSelection(usize),
    #[error("rule {0} has an empty keyword")]
    EmptyKeyword(usize),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RuleExportResult {
    pub count: usize,
    pub path: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RuleImportResult {
    pub count: usize,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct RuleImportPreview {
    pub count: usize,
    pub rules: Vec<NewRule>,
    pub version: u32,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
struct RuleFile {
    version: u32,
    rules: Vec<NewRule>,
}

pub(crate) fn export(
    connection: &Connection,
    path: &str,
    selected_ids: Option<&[i64]>,
) -> Result<RuleExportResult> {
    let path = validate_export_path(path)?;
    let parent = path
        .parent()
        .context("rule export target has no parent directory")?;
    let all_rules = rules::list(connection)?;
    let selected_ids = selected_ids.map(|ids| ids.iter().copied().collect::<HashSet<_>>());
    let rules = all_rules
        .into_iter()
        .filter(|rule| {
            selected_ids
                .as_ref()
                .is_none_or(|selected| selected.contains(&rule.id))
        })
        .map(|rule| NewRule {
            keyword: rule.keyword,
            rule_type: rule.rule_type,
            scope: rule.scope,
            enabled: rule.enabled,
        })
        .collect::<Vec<_>>();
    let count = rules.len();
    let payload = RuleFile {
        version: RULE_FILE_VERSION,
        rules,
    };
    let mut temporary = NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "failed to create temporary rule export in {}",
            parent.display()
        )
    })?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        serde_json::to_writer_pretty(&mut writer, &payload)
            .context("failed to serialize rule export")?;
        writer
            .write_all(b"\n")
            .context("failed to finish rule export")?;
        writer.flush().context("failed to flush rule export")?;
    }
    temporary
        .as_file_mut()
        .sync_all()
        .context("failed to sync rule export")?;
    temporary.persist(&path).map_err(|error| {
        anyhow::Error::new(error.error).context(format!(
            "failed to persist rule export at {}",
            path.display()
        ))
    })?;

    Ok(RuleExportResult {
        count,
        path: path.to_string_lossy().into_owned(),
        version: RULE_FILE_VERSION,
    })
}

pub(crate) fn preview(path: &str) -> Result<RuleImportPreview> {
    let payload = read_payload(path)?;
    Ok(RuleImportPreview {
        count: payload.rules.len(),
        rules: payload.rules,
        version: RULE_FILE_VERSION,
    })
}

pub(crate) fn import(
    connection: &mut Connection,
    path: &str,
    selected_indices: Option<&[usize]>,
) -> Result<RuleImportResult> {
    let payload = read_payload(path)?;
    let rules = select_rules(payload.rules, selected_indices)?;
    let count = rules.len();
    let transaction = connection.transaction()?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO rules (keyword, rule_type, scope, enabled)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for rule in rules {
            statement.execute(params![
                rule.keyword,
                rule.rule_type.as_str(),
                rule.scope.as_str(),
                rule.enabled
            ])?;
        }
    }
    transaction.commit()?;

    Ok(RuleImportResult {
        count,
        version: RULE_FILE_VERSION,
    })
}

fn read_payload(path: &str) -> Result<RuleFile> {
    let path = validate_import_path(path)?;
    let metadata = fs::metadata(&path).context("failed to read rule import metadata")?;
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(
            RuleTransferError::InvalidFormat("file is larger than 10 MB".to_string()).into(),
        );
    }
    let file = File::open(&path).context("failed to open rule import")?;
    let mut payload: RuleFile = serde_json::from_reader(BufReader::new(file))
        .map_err(|error| RuleTransferError::InvalidFormat(error.to_string()))?;
    if payload.version != RULE_FILE_VERSION {
        return Err(RuleTransferError::UnsupportedVersion(payload.version).into());
    }
    if payload.rules.is_empty() {
        return Err(RuleTransferError::EmptyFile.into());
    }
    if payload.rules.len() > MAX_IMPORT_RULES {
        return Err(RuleTransferError::TooManyRules(payload.rules.len()).into());
    }
    for (index, rule) in payload.rules.iter_mut().enumerate() {
        rule.keyword = rule.keyword.trim().to_string();
        if rule.keyword.is_empty() {
            return Err(RuleTransferError::EmptyKeyword(index + 1).into());
        }
    }

    Ok(payload)
}

fn select_rules(rules: Vec<NewRule>, selected_indices: Option<&[usize]>) -> Result<Vec<NewRule>> {
    let Some(selected_indices) = selected_indices else {
        return Ok(rules);
    };
    if selected_indices.is_empty() {
        return Err(RuleTransferError::EmptySelection.into());
    }
    let selected = selected_indices.iter().copied().collect::<HashSet<_>>();
    for index in &selected {
        if *index >= rules.len() {
            return Err(RuleTransferError::InvalidSelection(*index).into());
        }
    }
    Ok(rules
        .into_iter()
        .enumerate()
        .filter_map(|(index, rule)| selected.contains(&index).then_some(rule))
        .collect())
}

fn validate_export_path(path: &str) -> Result<PathBuf> {
    let path = validate_json_path(path)?;
    if path.parent().is_none_or(|parent| !parent.is_dir()) {
        return Err(RuleTransferError::InvalidPath("directory does not exist").into());
    }
    Ok(path)
}

fn validate_import_path(path: &str) -> Result<PathBuf> {
    let path = validate_json_path(path)?;
    if !path.is_file() {
        return Err(RuleTransferError::InvalidPath("file does not exist").into());
    }
    Ok(path)
}

fn validate_json_path(path: &str) -> Result<PathBuf> {
    if path.trim().is_empty() {
        return Err(RuleTransferError::InvalidPath("path is empty").into());
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(RuleTransferError::InvalidPath("path must be absolute").into());
    }
    let json_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
    if !json_extension {
        return Err(RuleTransferError::InvalidPath("path must use the .json extension").into());
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{RuleScope, RuleType};

    #[test]
    fn exports_and_imports_versioned_rule_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rules.json");
        let mut connection = test_connection();
        rules::add(
            &connection,
            new_rule("  Report  ", RuleType::Whitelist, RuleScope::Files, true),
        )
        .unwrap();
        rules::add(
            &connection,
            new_rule("Cache", RuleType::Blacklist, RuleScope::Folders, false),
        )
        .unwrap();

        let exported = export(&connection, path.to_str().unwrap(), None).unwrap();
        connection.execute("DELETE FROM rules", []).unwrap();
        let imported = import(&mut connection, path.to_str().unwrap(), None).unwrap();
        let restored = rules::list(&connection).unwrap();

        assert_eq!(exported.count, 2);
        assert_eq!(exported.version, RULE_FILE_VERSION);
        assert_eq!(imported.count, 2);
        assert_eq!(restored.len(), 2);
        assert_eq!(restored[0].keyword, "Report");
        assert_eq!(restored[0].scope, RuleScope::Files);
        assert_eq!(restored[1].rule_type, RuleType::Blacklist);
        assert!(!restored[1].enabled);
    }

    #[test]
    fn rejects_invalid_rule_before_writing_any_rows() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rules.json");
        fs::write(
            &path,
            r#"{"version":1,"rules":[{"keyword":"valid","rule_type":"whitelist","scope":"all","enabled":true},{"keyword":"   ","rule_type":"blacklist","scope":"files","enabled":false}]}"#,
        )
        .unwrap();
        let mut connection = test_connection();

        let error = import(&mut connection, path.to_str().unwrap(), None).unwrap_err();

        assert!(error.to_string().contains("empty keyword"));
        assert!(rules::list(&connection).unwrap().is_empty());
    }

    #[test]
    fn exports_and_imports_only_selected_rules() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rules.json");
        let mut connection = test_connection();
        let first = rules::add(
            &connection,
            new_rule("First", RuleType::Whitelist, RuleScope::All, true),
        )
        .unwrap();
        let second = rules::add(
            &connection,
            new_rule("Second", RuleType::Blacklist, RuleScope::Files, false),
        )
        .unwrap();

        let exported = export(&connection, path.to_str().unwrap(), Some(&[second.id])).unwrap();
        let preview = preview(path.to_str().unwrap()).unwrap();
        connection.execute("DELETE FROM rules", []).unwrap();
        let imported = import(&mut connection, path.to_str().unwrap(), Some(&[0])).unwrap();
        let restored = rules::list(&connection).unwrap();

        assert_eq!(exported.count, 1);
        assert_eq!(preview.count, 1);
        assert_eq!(preview.rules[0].keyword, "Second");
        assert_eq!(imported.count, 1);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].keyword, "Second");
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn rejects_empty_or_out_of_range_import_selection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rules.json");
        fs::write(
            &path,
            r#"{"version":1,"rules":[{"keyword":"valid","rule_type":"whitelist","scope":"all","enabled":true}]}"#,
        )
        .unwrap();
        let mut connection = test_connection();

        let empty = import(&mut connection, path.to_str().unwrap(), Some(&[])).unwrap_err();
        let out_of_range = import(&mut connection, path.to_str().unwrap(), Some(&[1])).unwrap_err();

        assert!(empty.to_string().contains("no rules were selected"));
        assert!(out_of_range.to_string().contains("out of range"));
        assert!(rules::list(&connection).unwrap().is_empty());
    }

    fn new_rule(keyword: &str, rule_type: RuleType, scope: RuleScope, enabled: bool) -> NewRule {
        NewRule {
            keyword: keyword.to_string(),
            rule_type,
            scope,
            enabled,
        }
    }

    fn test_connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE rules (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    keyword TEXT NOT NULL,
                    rule_type TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );",
            )
            .unwrap();
        connection
    }
}
