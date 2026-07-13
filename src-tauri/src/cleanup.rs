use anyhow::{bail, Result};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, MutexGuard, TryLockError},
};
use tauri::{AppHandle, Runtime};

use crate::{
    config::Config,
    db::{
        records::{self, CleanSource, NewCleanRecord},
        rules::{self, Rule},
        DbState,
    },
    matcher::{classify, MatchResult},
    notifier,
    privacy::{PrivacyManager, PrivacyModeState},
    quick_access::{self, QaBatchResult},
};

const AUTO_CLEAN_RUNNING_ERROR: &str = "Auto-clean is already running.";
const AUTO_CLEAN_DATABASE_ERROR: &str = "Database is unavailable; auto-clean cannot run.";
const AUTO_CLEAN_PRIVACY_ERROR: &str = "Privacy mode is active; auto-clean cannot run.";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ClassifiedItem {
    pub path: String,
    pub name: String,
    pub item_type: String,
    pub last_interaction_at: Option<u64>,
    #[serde(rename = "match")]
    pub match_result: MatchResult,
}

#[derive(Default)]
pub struct AutoCleanState {
    running: Mutex<()>,
}

impl AutoCleanState {
    fn begin(&self) -> Result<MutexGuard<'_, ()>> {
        match self.running.try_lock() {
            Ok(guard) => Ok(guard),
            Err(TryLockError::WouldBlock) => bail!(AUTO_CLEAN_RUNNING_ERROR),
            Err(TryLockError::Poisoned(_)) => bail!("Auto-clean state is unavailable."),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AutoCleanSectionResult {
    pub result: Option<QaBatchResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AutoCleanResult {
    pub recent: AutoCleanSectionResult,
    pub frequent: AutoCleanSectionResult,
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub skipped_protected: usize,
    pub section_errors: usize,
    pub history_errors: usize,
}

impl AutoCleanResult {
    pub fn has_failures(&self) -> bool {
        self.failed > 0 || self.section_errors > 0 || self.history_errors > 0
    }

    fn from_sections(recent: AutoCleanSectionResult, frequent: AutoCleanSectionResult) -> Self {
        let mut aggregate = Self {
            recent,
            frequent,
            total: 0,
            succeeded: 0,
            failed: 0,
            skipped_protected: 0,
            section_errors: 0,
            history_errors: 0,
        };

        for section in [&aggregate.recent, &aggregate.frequent] {
            if let Some(result) = &section.result {
                aggregate.total += result.total;
                aggregate.succeeded += result.succeeded.len();
                aggregate.failed += result.failed.len();
                aggregate.skipped_protected += result.skipped_protected.len();
                aggregate.history_errors += usize::from(result.history_error.is_some());
            }
            aggregate.section_errors += usize::from(section.error.is_some());
        }

        aggregate
    }
}

pub fn run_auto_clean<R: Runtime>(
    app: &AppHandle<R>,
    database: &DbState,
    config: &Config,
    privacy: &PrivacyManager,
    state: &AutoCleanState,
) -> Result<AutoCleanResult> {
    ensure_auto_clean_allowed(database.status().available, privacy.state())?;
    let _running = state.begin()?;
    log::info!("auto-clean started");
    let result = run_auto_clean_sections(|qa_type, source| {
        smart_clean(database, qa_type, config.history_retention, source)
    });
    for (qa_type, section) in [("recent", &result.recent), ("frequent", &result.frequent)] {
        if let Some(error) = &section.error {
            log::warn!("auto-clean section failed qa_type={qa_type} error={error}");
        }
    }
    if result.has_failures() {
        log::warn!(
            "auto-clean completed with issues total={} succeeded={} failed={} section_errors={} history_errors={}",
            result.total,
            result.succeeded,
            result.failed,
            result.section_errors,
            result.history_errors
        );
    } else {
        log::info!(
            "auto-clean completed total={} succeeded={}",
            result.total,
            result.succeeded
        );
    }
    notifier::notify_auto_clean(app, config, &result);
    Ok(result)
}

fn ensure_auto_clean_allowed(
    database_available: bool,
    privacy_state: PrivacyModeState,
) -> Result<()> {
    if !database_available {
        bail!(AUTO_CLEAN_DATABASE_ERROR);
    }
    if privacy_state != PrivacyModeState::Inactive {
        bail!(AUTO_CLEAN_PRIVACY_ERROR);
    }
    Ok(())
}

fn run_auto_clean_sections(
    mut clean: impl FnMut(&str, CleanSource) -> Result<QaBatchResult>,
) -> AutoCleanResult {
    let mut run_section = |qa_type| match clean(qa_type, CleanSource::Auto) {
        Ok(result) => AutoCleanSectionResult {
            result: Some(result),
            error: None,
        },
        Err(error) => AutoCleanSectionResult {
            result: None,
            error: Some(format!("{error:#}")),
        },
    };
    let recent = run_section("recent");
    let frequent = run_section("frequent");
    AutoCleanResult::from_sections(recent, frequent)
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
            last_interaction_at: item.last_interaction_at,
        })
        .collect())
}

pub fn remove_selected(
    database: &DbState,
    qa_type: &str,
    paths: Vec<String>,
    history_retention: usize,
) -> Result<QaBatchResult> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    let prepared = prepare_cleanup(paths, &rules, Selection::AllUnprotected);
    execute(
        database,
        qa_type,
        item_type,
        prepared,
        history_retention,
        CleanSource::Manual,
    )
}

pub fn empty_current(
    database: &DbState,
    qa_type: &str,
    history_retention: usize,
) -> Result<QaBatchResult> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    let paths = quick_access::list_items(qa_type)?
        .into_iter()
        .map(|item| item.path)
        .collect();
    let prepared = prepare_cleanup(paths, &rules, Selection::AllUnprotected);
    execute(
        database,
        qa_type,
        item_type,
        prepared,
        history_retention,
        CleanSource::Manual,
    )
}

pub fn smart_clean(
    database: &DbState,
    qa_type: &str,
    history_retention: usize,
    source: CleanSource,
) -> Result<QaBatchResult> {
    let item_type = item_type_for(qa_type)?;
    let rules = load_rules(database)?;
    let paths = quick_access::list_items(qa_type)?
        .into_iter()
        .map(|item| item.path)
        .collect();
    let prepared = prepare_cleanup(paths, &rules, Selection::TargetedOnly);
    execute(
        database,
        qa_type,
        item_type,
        prepared,
        history_retention,
        source,
    )
}

fn execute(
    database: &DbState,
    qa_type: &str,
    item_type: &str,
    prepared: PreparedCleanup,
    history_retention: usize,
    source: CleanSource,
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
            .map(|path| clean_record(path, item_type, matches.get(&path.to_lowercase()), source))
            .collect::<Vec<_>>();
        if let Err(error) = database.with_connection(|connection| {
            records::insert_batch(connection, &records, history_retention)
        }) {
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

fn clean_record(
    path: &str,
    item_type: &str,
    match_result: Option<&MatchResult>,
    source: CleanSource,
) -> NewCleanRecord {
    let (rule_id, rule_keyword) = match match_result {
        Some(MatchResult::Targeted { rule_id, keyword }) => (Some(*rule_id), Some(keyword.clone())),
        _ => (None, None),
    };
    NewCleanRecord {
        item_path: path.to_string(),
        item_type: item_type.to_string(),
        rule_id,
        rule_keyword,
        source,
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
    use crate::quick_access::QaBatchFailure;

    #[test]
    fn auto_clean_runs_both_sections_in_order_and_aggregates_results() {
        let mut calls = Vec::new();

        let result = run_auto_clean_sections(|qa_type, source| {
            calls.push((qa_type.to_string(), source));
            if qa_type == "recent" {
                Ok(QaBatchResult {
                    total: 3,
                    succeeded: vec![r"C:\Temp\a.txt".to_string()],
                    failed: vec![QaBatchFailure {
                        path: r"C:\Temp\b.txt".to_string(),
                        error: "in use".to_string(),
                    }],
                    skipped_protected: vec![r"C:\Work".to_string()],
                    history_error: Some("database busy".to_string()),
                })
            } else {
                Err(anyhow::anyhow!("frequent scan failed"))
            }
        });

        assert_eq!(
            calls,
            [
                ("recent".to_string(), CleanSource::Auto),
                ("frequent".to_string(), CleanSource::Auto),
            ]
        );
        assert_eq!(result.total, 3);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.failed, 1);
        assert_eq!(result.skipped_protected, 1);
        assert_eq!(result.section_errors, 1);
        assert_eq!(result.history_errors, 1);
        assert!(result.has_failures());
        assert_eq!(
            result.frequent.error.as_deref(),
            Some("frequent scan failed")
        );
    }

    #[test]
    fn auto_clean_rejects_unavailable_database_and_active_privacy() {
        assert!(ensure_auto_clean_allowed(false, PrivacyModeState::Inactive)
            .unwrap_err()
            .to_string()
            .contains("Database is unavailable"));
        assert!(
            ensure_auto_clean_allowed(true, PrivacyModeState::ActiveFull)
                .unwrap_err()
                .to_string()
                .contains("Privacy mode is active")
        );
        assert!(ensure_auto_clean_allowed(
            true,
            PrivacyModeState::ActivePartial {
                recent: true,
                frequent: false,
            },
        )
        .is_err());
    }

    #[test]
    fn auto_clean_allows_only_one_run_at_a_time() {
        let state = AutoCleanState::default();
        let first = state.begin().unwrap();

        assert_eq!(
            state.begin().unwrap_err().to_string(),
            AUTO_CLEAN_RUNNING_ERROR
        );

        drop(first);
        assert!(state.begin().is_ok());
    }

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

        let targeted_record = clean_record(
            r"C:\Temp\a.txt",
            "recent_file",
            Some(&targeted),
            CleanSource::Manual,
        );
        let neutral_record = clean_record(r"C:\Docs\a.txt", "recent_file", None, CleanSource::Auto);

        assert_eq!(targeted_record.rule_id, Some(7));
        assert_eq!(targeted_record.rule_keyword.as_deref(), Some("Temp"));
        assert_eq!(neutral_record.rule_id, None);
        assert_eq!(neutral_record.rule_keyword, None);
        assert_eq!(targeted_record.source, CleanSource::Manual);
        assert_eq!(neutral_record.source, CleanSource::Auto);
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
