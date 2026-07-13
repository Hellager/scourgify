use anyhow::{bail, Result};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::{
    db::{
        records::{self, NewCleanRecord},
        rules::{self, Rule},
        DbState,
    },
    matcher::{classify, MatchResult},
    quick_access::{self, QaBatchResult},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClassifiedItem {
    pub path: String,
    pub name: String,
    pub item_type: String,
    #[serde(rename = "match")]
    pub match_result: MatchResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Selection {
    AllUnprotected,
    TargetedOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    path: String,
    match_result: MatchResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedCleanup {
    total: usize,
    candidates: Vec<Candidate>,
    skipped_protected: Vec<String>,
}

pub fn list_classified(database: &DbState, qa_type: &str) -> Result<Vec<ClassifiedItem>> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    Ok(quick_access::list_items(qa_type)?
        .into_iter()
        .map(|item| ClassifiedItem {
            match_result: classify(&item.path, &rules),
            path: item.path,
            name: item.name,
            item_type: item_type.to_string(),
        })
        .collect())
}

pub fn remove_selected(
    database: &DbState,
    qa_type: &str,
    paths: Vec<String>,
) -> Result<QaBatchResult> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    let prepared = prepare_cleanup(paths, &rules, Selection::AllUnprotected);
    execute(database, qa_type, item_type, prepared)
}

pub fn empty_current(database: &DbState, qa_type: &str) -> Result<QaBatchResult> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    let paths = quick_access::list_items(qa_type)?
        .into_iter()
        .map(|item| item.path)
        .collect();
    let prepared = prepare_cleanup(paths, &rules, Selection::AllUnprotected);
    execute(database, qa_type, item_type, prepared)
}

pub fn smart_clean(database: &DbState, qa_type: &str) -> Result<QaBatchResult> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    let paths = quick_access::list_items(qa_type)?
        .into_iter()
        .map(|item| item.path)
        .collect();
    let prepared = prepare_cleanup(paths, &rules, Selection::TargetedOnly);
    execute(database, qa_type, item_type, prepared)
}

fn execute(
    database: &DbState,
    qa_type: &str,
    item_type: &str,
    prepared: PreparedCleanup,
) -> Result<QaBatchResult> {
    let matches = prepared
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.path.to_lowercase(),
                candidate.match_result.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    let paths = prepared
        .candidates
        .into_iter()
        .map(|candidate| candidate.path)
        .collect::<Vec<_>>();
    let mut result = if paths.is_empty() {
        QaBatchResult::default()
    } else {
        quick_access::remove_items(qa_type, paths)?
    };
    result.total = prepared.total;
    result.skipped_protected = prepared.skipped_protected;

    if !result.succeeded.is_empty() {
        let records = result
            .succeeded
            .iter()
            .map(|path| clean_record(path, item_type, matches.get(&path.to_lowercase())))
            .collect::<Vec<_>>();
        if let Err(error) =
            database.with_connection(|connection| records::insert_batch(connection, &records))
        {
            let error = format!("{error:#}");
            log::error!("cleanup history write failed error={error}");
            result.history_error = Some(error);
        }
    }

    Ok(result)
}

fn load_rules(database: &DbState) -> Result<Vec<Rule>> {
    database.with_connection(|connection| rules::list(connection))
}

fn prepare_cleanup(paths: Vec<String>, rules: &[Rule], selection: Selection) -> PreparedCleanup {
    let mut seen = HashSet::new();
    let unique_paths = paths
        .into_iter()
        .filter(|path| seen.insert(path.to_lowercase()))
        .collect::<Vec<_>>();
    let mut candidates = Vec::new();
    let mut skipped_protected = Vec::new();

    for path in &unique_paths {
        let match_result = classify(path, rules);
        match (&selection, &match_result) {
            (Selection::AllUnprotected, MatchResult::Protected { .. }) => {
                skipped_protected.push(path.clone());
            }
            (Selection::AllUnprotected, _)
            | (Selection::TargetedOnly, MatchResult::Targeted { .. }) => {
                candidates.push(Candidate {
                    path: path.clone(),
                    match_result,
                });
            }
            (Selection::TargetedOnly, _) => {}
        }
    }

    PreparedCleanup {
        total: match selection {
            Selection::AllUnprotected => unique_paths.len(),
            Selection::TargetedOnly => candidates.len(),
        },
        candidates,
        skipped_protected,
    }
}

fn clean_record(path: &str, item_type: &str, match_result: Option<&MatchResult>) -> NewCleanRecord {
    let (rule_id, rule_keyword) = match match_result {
        Some(MatchResult::Targeted { rule_id, keyword }) => (Some(*rule_id), Some(keyword.clone())),
        _ => (None, None),
    };
    NewCleanRecord {
        item_path: path.to_string(),
        item_type: item_type.to_string(),
        rule_id,
        rule_keyword,
    }
}

fn item_type_for(qa_type: &str) -> Result<&'static str> {
    match qa_type {
        "recent" => Ok("recent_file"),
        "frequent" => Ok("frequent_folder"),
        _ => bail!("unsupported Quick Access cleanup type: {qa_type}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::rules::RuleType;

    #[test]
    fn manual_cleanup_skips_protected_and_keeps_other_matches() {
        let rules = [
            rule(1, "Projects", RuleType::Whitelist),
            rule(2, "Temp", RuleType::Blacklist),
        ];
        let prepared = prepare_cleanup(
            vec![
                r"C:\Projects\report.txt".to_string(),
                r"C:\Temp\cache.bin".to_string(),
                r"C:\Downloads\notes.txt".to_string(),
            ],
            &rules,
            Selection::AllUnprotected,
        );

        assert_eq!(prepared.total, 3);
        assert_eq!(prepared.skipped_protected, vec![r"C:\Projects\report.txt"]);
        assert_eq!(prepared.candidates.len(), 2);
        assert!(matches!(
            prepared.candidates[0].match_result,
            MatchResult::Targeted { rule_id: 2, .. }
        ));
        assert_eq!(prepared.candidates[1].match_result, MatchResult::Neutral);
    }

    #[test]
    fn smart_cleanup_selects_only_targeted_items() {
        let rules = [
            rule(1, "Projects", RuleType::Whitelist),
            rule(2, "Temp", RuleType::Blacklist),
        ];
        let prepared = prepare_cleanup(
            vec![
                r"C:\Projects\report.txt".to_string(),
                r"C:\Temp\cache.bin".to_string(),
                r"C:\Downloads\notes.txt".to_string(),
            ],
            &rules,
            Selection::TargetedOnly,
        );

        assert_eq!(prepared.total, 1);
        assert_eq!(prepared.candidates.len(), 1);
        assert!(prepared.skipped_protected.is_empty());
        assert_eq!(prepared.candidates[0].path, r"C:\Temp\cache.bin");
    }

    #[test]
    fn cleanup_deduplicates_paths_case_insensitively() {
        let prepared = prepare_cleanup(
            vec![r"C:\Temp\A.txt".to_string(), r"c:\temp\a.TXT".to_string()],
            &[],
            Selection::AllUnprotected,
        );

        assert_eq!(prepared.total, 1);
        assert_eq!(prepared.candidates.len(), 1);
        assert_eq!(prepared.candidates[0].path, r"C:\Temp\A.txt");
    }

    #[test]
    fn creates_rule_snapshot_only_for_targeted_match() {
        let targeted = MatchResult::Targeted {
            rule_id: 7,
            keyword: "Temp".to_string(),
        };

        let targeted_record = clean_record(r"C:\Temp\a.txt", "recent_file", Some(&targeted));
        let neutral_record = clean_record(r"C:\Docs\a.txt", "recent_file", None);

        assert_eq!(targeted_record.rule_id, Some(7));
        assert_eq!(targeted_record.rule_keyword.as_deref(), Some("Temp"));
        assert_eq!(neutral_record.rule_id, None);
        assert_eq!(neutral_record.rule_keyword, None);
    }

    #[test]
    fn validates_cleanup_item_type() {
        assert_eq!(item_type_for("recent").unwrap(), "recent_file");
        assert_eq!(item_type_for("frequent").unwrap(), "frequent_folder");
        assert!(item_type_for("all").is_err());
    }

    fn rule(id: i64, keyword: &str, rule_type: RuleType) -> Rule {
        Rule {
            id,
            keyword: keyword.to_string(),
            rule_type,
            enabled: true,
            created_at: "2026-07-13 00:00:00".to_string(),
        }
    }
}
