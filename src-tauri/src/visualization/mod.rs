mod tokenizer;

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{
    backend::QuickAccessBackendState,
    cleanup::{classify, MatchResult},
    config::{Config, VisualizationDataRange},
    db::{rules, DbState},
    quick_access::QaItemMetadata,
    rules::{Rule, RuleScope},
};

use self::tokenizer::{tokenize_name, tokenize_path, tokenize_text, Token};

const MAX_TERMS: usize = 40;
const MAX_CLEANUP_RECORDS: usize = 20_000;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WordCloudSource {
    Access,
    Cleanup,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WordCloudRange {
    Last7Days,
    Last30Days,
    AllTime,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct WordCloudTerm {
    pub text: String,
    pub score: f64,
    pub occurrences: u64,
    pub recent_occurrences: u64,
    pub frequent_occurrences: u64,
    pub cleanup_occurrences: u64,
    pub total_access_count: u64,
    pub manual_cleanups: u64,
    pub automatic_cleanups: u64,
    pub pinned_items: u64,
    pub blacklist_hits: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct WordCloudWarning {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub(crate) struct WordCloudResult {
    pub source: WordCloudSource,
    pub range: Option<WordCloudRange>,
    pub corpus_items: u64,
    pub truncated: bool,
    pub blacklist_terms_included: bool,
    pub blacklist_terms_available: bool,
    pub terms: Vec<WordCloudTerm>,
    pub warnings: Vec<WordCloudWarning>,
}

#[derive(Debug, Default)]
struct TermAccumulator {
    terms: HashMap<String, AccumulatedTerm>,
}

#[derive(Debug, Default)]
struct AccumulatedTerm {
    display_counts: HashMap<String, u64>,
    score: f64,
    occurrences: u64,
    recent_occurrences: u64,
    frequent_occurrences: u64,
    cleanup_occurrences: u64,
    total_access_count: u64,
    manual_cleanups: u64,
    automatic_cleanups: u64,
    pinned_items: u64,
    blacklist_hits: u64,
}

#[derive(Debug)]
struct CleanupRow {
    item_path: String,
    item_type: String,
    rule_keyword: Option<String>,
    source: String,
    cleaned_at: String,
}

pub(crate) fn build_word_cloud(
    source: WordCloudSource,
    range: WordCloudRange,
    backend: &QuickAccessBackendState,
    database: &DbState,
    config: &Config,
) -> Result<WordCloudResult> {
    match source {
        WordCloudSource::Access => build_access_word_cloud(range, backend, database, config),
        WordCloudSource::Cleanup => build_cleanup_word_cloud(range, database, config),
    }
}

fn build_access_word_cloud(
    range: WordCloudRange,
    backend_state: &QuickAccessBackendState,
    database: &DbState,
    config: &Config,
) -> Result<WordCloudResult> {
    let backend = backend_state.backend()?;
    let mut warnings = Vec::new();
    let rules = load_rules(database, &mut warnings);
    let now = Utc::now();
    let mut accumulator = TermAccumulator::default();
    let mut corpus_items = 0_u64;
    let mut loaded_sections = 0_u8;

    for (qa_type, recent_file, source_factor, scope) in [
        ("recent", true, 1.0, RuleScope::Files),
        ("frequent", false, 1.15, RuleScope::Folders),
    ] {
        let items = if config.visualization_deep_parse {
            backend.list_item_metadata(qa_type)
        } else {
            backend.list_items(qa_type).map(|items| {
                items
                    .into_iter()
                    .map(|item| QaItemMetadata {
                        path: item.path,
                        name: item.name,
                        last_interaction_at: item.last_interaction_at,
                        access_count: 0,
                        score: None,
                        recent_rank: 0,
                        mru_position: 0,
                        pinned: item.pinned.unwrap_or(false),
                        pin_order: None,
                        warning_count: 0,
                    })
                    .collect()
            })
        };
        match items {
            Ok(items) => {
                let items = items
                    .into_iter()
                    .filter(|item| access_item_in_range(item, range, now))
                    .collect::<Vec<_>>();
                loaded_sections += 1;
                corpus_items += items.len() as u64;
                aggregate_access_items(
                    &items,
                    recent_file,
                    source_factor,
                    scope,
                    rules.as_deref(),
                    config.word_cloud_show_blacklist_terms,
                    now,
                    &mut accumulator,
                );
            }
            Err(_) => warnings.push(WordCloudWarning {
                code: "quick_access_section_unavailable",
                message: format!("The {qa_type} Quick Access section is unavailable."),
            }),
        }
    }

    if loaded_sections == 0 {
        anyhow::bail!("both Quick Access sections are unavailable")
    }

    Ok(WordCloudResult {
        source: WordCloudSource::Access,
        range: Some(range),
        corpus_items,
        truncated: false,
        blacklist_terms_included: config.word_cloud_show_blacklist_terms,
        blacklist_terms_available: rules.is_some(),
        terms: accumulator.finish(),
        warnings,
    })
}

fn build_cleanup_word_cloud(
    range: WordCloudRange,
    database: &DbState,
    config: &Config,
) -> Result<WordCloudResult> {
    let (modifier, now) = match range {
        WordCloudRange::Last7Days => (Some("-7 days"), Utc::now()),
        WordCloudRange::Last30Days => (Some("-30 days"), Utc::now()),
        WordCloudRange::AllTime => (None, Utc::now()),
    };
    let include_blacklist_terms = config.word_cloud_show_blacklist_terms;
    let (accumulator, corpus_items, truncated) = database.with_connection(|connection| {
        let mut statement = connection
            .prepare(
                "SELECT item_path, item_type, rule_keyword, source, cleaned_at
                 FROM clean_records
                 WHERE visible = 1
                   AND (?1 IS NULL OR cleaned_at >= datetime('now', 'localtime', 'start of day', ?1, 'utc'))
                 ORDER BY cleaned_at DESC, id DESC
                 LIMIT ?2",
            )
            .context("failed to prepare cleanup word cloud query")?;
        let rows = statement
            .query_map(params![modifier, (MAX_CLEANUP_RECORDS + 1) as i64], |row| {
                Ok(CleanupRow {
                    item_path: row.get(0)?,
                    item_type: row.get(1)?,
                    rule_keyword: row.get(2)?,
                    source: row.get(3)?,
                    cleaned_at: row.get(4)?,
                })
            })
            .context("failed to read cleanup word cloud rows")?;
        let mut accumulator = TermAccumulator::default();
        let mut corpus_items = 0_u64;
        let mut truncated = false;

        for row in rows {
            let row = row.context("failed to decode cleanup word cloud row")?;
            if corpus_items as usize >= MAX_CLEANUP_RECORDS {
                truncated = true;
                break;
            }
            corpus_items += 1;
            aggregate_cleanup_row(&row, include_blacklist_terms, now, &mut accumulator);
        }
        Ok::<_, anyhow::Error>((accumulator, corpus_items, truncated))
    })?;

    Ok(WordCloudResult {
        source: WordCloudSource::Cleanup,
        range: Some(range),
        corpus_items,
        truncated,
        blacklist_terms_included: include_blacklist_terms,
        blacklist_terms_available: true,
        terms: accumulator.finish(),
        warnings: Vec::new(),
    })
}

fn load_rules(database: &DbState, warnings: &mut Vec<WordCloudWarning>) -> Option<Vec<Rule>> {
    match database.with_connection(|connection| rules::list(connection)) {
        Ok(rules) => Some(rules),
        Err(_) => {
            warnings.push(WordCloudWarning {
                code: "blacklist_terms_unavailable",
                message: "Blacklist rule matching is temporarily unavailable.".to_string(),
            });
            None
        }
    }
}

fn aggregate_access_items(
    items: &[QaItemMetadata],
    recent_file: bool,
    source_factor: f64,
    scope: RuleScope,
    rules: Option<&[Rule]>,
    include_blacklist_terms: bool,
    now: DateTime<Utc>,
    accumulator: &mut TermAccumulator,
) {
    for item in items {
        let blacklist_tokens = rules
            .and_then(|rules| match classify(&item.path, scope, rules) {
                MatchResult::Targeted { keyword, .. } => Some(
                    tokenize_text(&keyword)
                        .into_iter()
                        .map(|token| token.key)
                        .collect::<HashSet<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let contribution = access_contribution(item, source_factor, now);
        let mut tokens = tokenize_path(&item.path, recent_file);
        if tokens.is_empty() {
            tokens = tokenize_name(&item.name);
        }
        let tokens = unique_tokens(tokens);
        let token_keys = tokens
            .iter()
            .map(|token| token.key.as_str())
            .collect::<HashSet<_>>();

        for token in &tokens {
            let blacklist_hit = blacklist_tokens.contains(&token.key);
            if blacklist_hit && !include_blacklist_terms {
                continue;
            }
            accumulator.add_access(
                token,
                contribution,
                recent_file,
                item.access_count as u64,
                item.pinned,
                blacklist_hit,
            );
        }

        if include_blacklist_terms {
            for key in blacklist_tokens {
                if token_keys.contains(key.as_str()) {
                    continue;
                }
                accumulator.add_access(
                    &Token {
                        key: key.clone(),
                        display: key,
                    },
                    contribution,
                    recent_file,
                    item.access_count as u64,
                    item.pinned,
                    true,
                );
            }
        }
    }
}

fn aggregate_cleanup_row(
    row: &CleanupRow,
    include_blacklist_terms: bool,
    now: DateTime<Utc>,
    accumulator: &mut TermAccumulator,
) {
    let recent_file = row.item_type == "recent_file";
    let blacklist_tokens = row
        .rule_keyword
        .as_deref()
        .map(|keyword| {
            tokenize_text(keyword)
                .into_iter()
                .map(|token| token.key)
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();
    let tokens = unique_tokens(tokenize_path(&row.item_path, recent_file));
    let token_keys = tokens
        .iter()
        .map(|token| token.key.as_str())
        .collect::<HashSet<_>>();
    let contribution = cleanup_contribution(&row.cleaned_at, now);

    for token in &tokens {
        let blacklist_hit = blacklist_tokens.contains(&token.key);
        if blacklist_hit && !include_blacklist_terms {
            continue;
        }
        accumulator.add_cleanup(token, contribution, recent_file, &row.source, blacklist_hit);
    }

    if include_blacklist_terms {
        for key in blacklist_tokens {
            if token_keys.contains(key.as_str()) {
                continue;
            }
            accumulator.add_cleanup(
                &Token {
                    key: key.clone(),
                    display: key,
                },
                contribution,
                recent_file,
                &row.source,
                true,
            );
        }
    }
}

fn access_contribution(item: &QaItemMetadata, source_factor: f64, now: DateTime<Utc>) -> f64 {
    let usage_factor = 1.0 + (1.0 + f64::from(item.access_count)).ln();
    usage_factor * recency_factor(item.last_interaction_at, now, 14.0) * source_factor
}

fn cleanup_contribution(cleaned_at: &str, now: DateTime<Utc>) -> f64 {
    let Some(cleaned_at) = parse_timestamp(cleaned_at) else {
        return 1.0;
    };
    let age_days = (now - cleaned_at).num_seconds().max(0) as f64 / 86_400.0;
    2_f64.powf(-age_days / 30.0)
}

fn recency_factor(timestamp: Option<u64>, now: DateTime<Utc>, half_life_days: f64) -> f64 {
    let Some(value) = timestamp else {
        return 0.5;
    };
    let Some(timestamp) = i64::try_from(value)
        .ok()
        .and_then(DateTime::from_timestamp_millis)
    else {
        log::warn!("word cloud access metadata contains an invalid interaction timestamp");
        return 0.5;
    };
    if timestamp > now {
        log::warn!("word cloud access metadata contains a future interaction timestamp");
        return 0.5;
    }
    let age_days = (now - timestamp).num_seconds() as f64 / 86_400.0;
    0.5 + 0.5 * 2_f64.powf(-age_days / half_life_days)
}

fn access_item_in_range(item: &QaItemMetadata, range: WordCloudRange, now: DateTime<Utc>) -> bool {
    let days = match range {
        WordCloudRange::Last7Days => 7,
        WordCloudRange::Last30Days => 30,
        WordCloudRange::AllTime => return true,
    };
    item.last_interaction_at
        .and_then(|value| i64::try_from(value).ok())
        .and_then(DateTime::from_timestamp_millis)
        .is_some_and(|timestamp| {
            timestamp <= now && timestamp >= now - chrono::Duration::days(days)
        })
}

impl From<VisualizationDataRange> for WordCloudRange {
    fn from(value: VisualizationDataRange) -> Self {
        match value {
            VisualizationDataRange::Last7Days => Self::Last7Days,
            VisualizationDataRange::Last30Days => Self::Last30Days,
            VisualizationDataRange::AllTime => Self::AllTime,
        }
    }
}

fn unique_tokens(tokens: Vec<Token>) -> Vec<Token> {
    let mut seen = HashSet::new();
    tokens
        .into_iter()
        .filter(|token| seen.insert(token.key.clone()))
        .collect()
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|value| DateTime::<Utc>::from_naive_utc_and_offset(value, Utc))
}

impl TermAccumulator {
    fn add_access(
        &mut self,
        token: &Token,
        score: f64,
        recent_file: bool,
        access_count: u64,
        pinned: bool,
        blacklist_hit: bool,
    ) {
        let term = self.terms.entry(token.key.clone()).or_default();
        term.score += score;
        term.occurrences += 1;
        term.total_access_count += access_count;
        term.pinned_items += u64::from(pinned);
        if recent_file {
            term.recent_occurrences += 1;
        } else {
            term.frequent_occurrences += 1;
        }
        term.blacklist_hits += u64::from(blacklist_hit);
        *term
            .display_counts
            .entry(token.display.clone())
            .or_default() += 1;
    }

    fn add_cleanup(
        &mut self,
        token: &Token,
        score: f64,
        recent_file: bool,
        source: &str,
        blacklist_hit: bool,
    ) {
        let term = self.terms.entry(token.key.clone()).or_default();
        term.score += score;
        term.occurrences += 1;
        term.cleanup_occurrences += 1;
        term.blacklist_hits += u64::from(blacklist_hit);
        if recent_file {
            term.recent_occurrences += 1;
        } else {
            term.frequent_occurrences += 1;
        }
        if source == "auto" {
            term.automatic_cleanups += 1;
        } else {
            term.manual_cleanups += 1;
        }
        *term
            .display_counts
            .entry(token.display.clone())
            .or_default() += 1;
    }

    fn finish(self) -> Vec<WordCloudTerm> {
        let mut terms = self
            .terms
            .into_iter()
            .map(|(key, term)| {
                let text = term
                    .display_counts
                    .iter()
                    .max_by(|(left_text, left_count), (right_text, right_count)| {
                        left_count
                            .cmp(right_count)
                            .then_with(|| right_text.cmp(left_text))
                    })
                    .map(|(text, _)| text.clone())
                    .unwrap_or_else(|| key.clone());
                (key, text, term)
            })
            .collect::<Vec<_>>();
        terms.sort_by(|(left_key, _, left), (right_key, _, right)| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left_key.cmp(right_key))
        });
        terms
            .into_iter()
            .take(MAX_TERMS)
            .map(|(_, text, term)| WordCloudTerm {
                text,
                score: term.score,
                occurrences: term.occurrences,
                recent_occurrences: term.recent_occurrences,
                frequent_occurrences: term.frequent_occurrences,
                cleanup_occurrences: term.cleanup_occurrences,
                total_access_count: term.total_access_count,
                manual_cleanups: term.manual_cleanups,
                automatic_cleanups: term.automatic_cleanups,
                pinned_items: term.pinned_items,
                blacklist_hits: term.blacklist_hits,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use serde_json::json;

    use super::*;

    fn metadata(path: &str, access_count: u32, timestamp: Option<u64>) -> QaItemMetadata {
        QaItemMetadata {
            path: path.to_string(),
            name: path.rsplit(['\\', '/']).next().unwrap_or(path).to_string(),
            last_interaction_at: timestamp,
            access_count,
            score: None,
            recent_rank: 1,
            mru_position: 1,
            pinned: false,
            pin_order: None,
            warning_count: 0,
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn access_aggregation_weights_recent_and_frequent_items() {
        let now = fixed_now();
        let timestamp = Some(now.timestamp_millis() as u64);
        let mut accumulator = TermAccumulator::default();
        aggregate_access_items(
            &[metadata(r"C:\Projects\Alpha.txt", 10, timestamp)],
            true,
            1.0,
            RuleScope::Files,
            Some(&[]),
            false,
            now,
            &mut accumulator,
        );
        aggregate_access_items(
            &[metadata(r"C:\Projects\Alpha", 10, timestamp)],
            false,
            1.15,
            RuleScope::Folders,
            Some(&[]),
            false,
            now,
            &mut accumulator,
        );

        let terms = accumulator.finish();
        let alpha = terms.iter().find(|term| term.text == "Alpha").unwrap();
        assert_eq!(alpha.occurrences, 2);
        assert_eq!(alpha.recent_occurrences, 1);
        assert_eq!(alpha.frequent_occurrences, 1);
        assert!(alpha.score > 0.0);
    }

    #[test]
    fn blacklist_terms_are_removed_or_marked_without_removing_other_words() {
        let now = fixed_now();
        let item = metadata(
            r"C:\Projects\Archive\report.txt",
            1,
            Some(now.timestamp_millis() as u64),
        );
        let rules = vec![Rule {
            id: 1,
            keyword: "Archive".to_string(),
            rule_type: crate::rules::RuleType::Blacklist,
            scope: RuleScope::Files,
            enabled: true,
            created_at: "2026-08-01 00:00:00".to_string(),
        }];

        let mut hidden = TermAccumulator::default();
        aggregate_access_items(
            &[item.clone()],
            true,
            1.0,
            RuleScope::Files,
            Some(&rules),
            false,
            now,
            &mut hidden,
        );
        let hidden = hidden.finish();
        assert!(hidden.iter().all(|term| term.text != "Archive"));
        assert!(hidden.iter().any(|term| term.text == "report"));

        let mut shown = TermAccumulator::default();
        aggregate_access_items(
            &[item],
            true,
            1.0,
            RuleScope::Files,
            Some(&rules),
            true,
            now,
            &mut shown,
        );
        let archive = shown
            .finish()
            .into_iter()
            .find(|term| term.text == "Archive");
        assert_eq!(archive.map(|term| term.blacklist_hits), Some(1));
    }

    #[test]
    fn cleanup_aggregation_uses_snapshot_keyword_and_source_counts() {
        let now = fixed_now();
        let row = CleanupRow {
            item_path: r"C:\Projects\Archive\report.txt".to_string(),
            item_type: "recent_file".to_string(),
            rule_keyword: Some("Archive".to_string()),
            source: "auto".to_string(),
            cleaned_at: "2026-08-02 00:00:00.000".to_string(),
        };
        let mut accumulator = TermAccumulator::default();
        aggregate_cleanup_row(&row, true, now, &mut accumulator);

        let archive = accumulator
            .finish()
            .into_iter()
            .find(|term| term.text == "Archive")
            .unwrap();
        assert_eq!(archive.cleanup_occurrences, 1);
        assert_eq!(archive.automatic_cleanups, 1);
        assert_eq!(archive.blacklist_hits, 1);
    }

    #[test]
    fn malformed_cleanup_timestamp_has_finite_fallback_weight() {
        let now = fixed_now();
        let row = CleanupRow {
            item_path: r"C:\Projects\report.txt".to_string(),
            item_type: "recent_file".to_string(),
            rule_keyword: None,
            source: "manual".to_string(),
            cleaned_at: "invalid".to_string(),
        };
        let mut accumulator = TermAccumulator::default();
        aggregate_cleanup_row(&row, false, now, &mut accumulator);

        let report = accumulator
            .finish()
            .into_iter()
            .find(|term| term.text == "report")
            .unwrap();
        assert!(report.score.is_finite());
    }

    #[test]
    fn cleanup_aggregation_tracks_manual_and_automatic_sources() {
        let now = fixed_now();
        let rows = [
            CleanupRow {
                item_path: r"C:\Projects\Archive\report.txt".to_string(),
                item_type: "recent_file".to_string(),
                rule_keyword: None,
                source: "auto".to_string(),
                cleaned_at: "2026-08-02 00:00:00.000".to_string(),
            },
            CleanupRow {
                item_path: r"C:\Projects\Archive\report.txt".to_string(),
                item_type: "recent_file".to_string(),
                rule_keyword: None,
                source: "manual".to_string(),
                cleaned_at: "2026-08-02 00:00:00.000".to_string(),
            },
        ];
        let mut accumulator = TermAccumulator::default();

        for row in &rows {
            aggregate_cleanup_row(row, false, now, &mut accumulator);
        }

        let archive = accumulator
            .finish()
            .into_iter()
            .find(|term| term.text == "Archive")
            .unwrap();
        assert_eq!(archive.cleanup_occurrences, 2);
        assert_eq!(archive.automatic_cleanups, 1);
        assert_eq!(archive.manual_cleanups, 1);
    }

    #[test]
    fn deduplicates_repeated_terms_within_one_access_item() {
        let now = fixed_now();
        let mut item = metadata(
            r"C:\Alpha\Alpha.txt",
            7,
            Some(now.timestamp_millis() as u64),
        );
        item.pinned = true;
        let mut accumulator = TermAccumulator::default();

        aggregate_access_items(
            &[item],
            true,
            1.0,
            RuleScope::Files,
            Some(&[]),
            false,
            now,
            &mut accumulator,
        );

        let alpha = accumulator
            .finish()
            .into_iter()
            .find(|term| term.text == "Alpha")
            .unwrap();
        assert_eq!(alpha.occurrences, 1);
        assert_eq!(alpha.recent_occurrences, 1);
        assert_eq!(alpha.total_access_count, 7);
        assert_eq!(alpha.pinned_items, 1);
    }

    #[test]
    fn access_aggregation_falls_back_to_metadata_name() {
        let now = fixed_now();
        let mut item = metadata("C:\\2026\\a.txt", 3, Some(now.timestamp_millis() as u64));
        item.name = "QuarterlyReport.docx".to_string();
        let mut accumulator = TermAccumulator::default();

        aggregate_access_items(
            &[item],
            true,
            1.0,
            RuleScope::Files,
            Some(&[]),
            false,
            now,
            &mut accumulator,
        );

        let terms = accumulator.finish();
        assert!(terms.iter().any(|term| term.text == "Quarterly"));
        assert!(terms.iter().any(|term| term.text == "Report"));
        assert!(terms.iter().all(|term| term.text != "docx"));
    }

    #[test]
    fn access_weight_is_monotonic_for_usage_and_recency() {
        let now = fixed_now();
        let current = Some(now.timestamp_millis() as u64);
        let older = Some((now - Duration::days(30)).timestamp_millis() as u64);

        let low_usage = access_contribution(&metadata("Alpha", 1, current), 1.0, now);
        let high_usage = access_contribution(&metadata("Alpha", 20, current), 1.0, now);
        let old_access = access_contribution(&metadata("Alpha", 20, older), 1.0, now);

        assert!(high_usage > low_usage);
        assert!(high_usage > old_access);
        assert!(low_usage.is_finite());
        assert!(old_access.is_finite());
    }

    #[test]
    fn future_invalid_and_missing_access_timestamps_use_the_same_fallback() {
        let now = fixed_now();
        let future = Some((now + Duration::days(1)).timestamp_millis() as u64);

        assert_eq!(recency_factor(None, now, 14.0), 0.5);
        assert_eq!(recency_factor(future, now, 14.0), 0.5);
        assert_eq!(recency_factor(Some(u64::MAX), now, 14.0), 0.5);
    }

    #[test]
    fn access_ranges_filter_by_last_interaction_time() {
        let now = fixed_now();
        let item_at = |days_ago| {
            metadata(
                "Alpha",
                1,
                Some((now - Duration::days(days_ago)).timestamp_millis() as u64),
            )
        };

        assert!(access_item_in_range(
            &item_at(6),
            WordCloudRange::Last7Days,
            now
        ));
        assert!(!access_item_in_range(
            &item_at(8),
            WordCloudRange::Last7Days,
            now
        ));
        assert!(access_item_in_range(
            &item_at(29),
            WordCloudRange::Last30Days,
            now
        ));
        assert!(access_item_in_range(
            &metadata("Alpha", 1, None),
            WordCloudRange::AllTime,
            now
        ));
        assert!(!access_item_in_range(
            &metadata("Alpha", 1, None),
            WordCloudRange::Last30Days,
            now
        ));
    }

    #[test]
    fn whitelist_priority_prevents_blacklist_marking() {
        let now = fixed_now();
        let item = metadata(
            r"C:\Archive\protected-report.txt",
            1,
            Some(now.timestamp_millis() as u64),
        );
        let rules = [
            Rule {
                id: 1,
                keyword: "Archive".to_string(),
                rule_type: crate::rules::RuleType::Blacklist,
                scope: RuleScope::Files,
                enabled: true,
                created_at: "2026-08-01 00:00:00".to_string(),
            },
            Rule {
                id: 2,
                keyword: "report".to_string(),
                rule_type: crate::rules::RuleType::Whitelist,
                scope: RuleScope::Files,
                enabled: true,
                created_at: "2026-08-01 00:00:00".to_string(),
            },
        ];
        let mut accumulator = TermAccumulator::default();

        aggregate_access_items(
            &[item],
            true,
            1.0,
            RuleScope::Files,
            Some(&rules),
            true,
            now,
            &mut accumulator,
        );

        let terms = accumulator.finish();
        assert!(terms.iter().any(|term| term.text == "Archive"));
        assert!(terms.iter().all(|term| term.blacklist_hits == 0));
    }

    #[test]
    fn disabled_blacklist_rule_does_not_mark_term() {
        let now = fixed_now();
        let item = metadata(
            r"C:\Archive\report.txt",
            1,
            Some(now.timestamp_millis() as u64),
        );
        let rules = [Rule {
            id: 1,
            keyword: "Archive".to_string(),
            rule_type: crate::rules::RuleType::Blacklist,
            scope: RuleScope::Files,
            enabled: false,
            created_at: "2026-08-01 00:00:00".to_string(),
        }];
        let mut accumulator = TermAccumulator::default();

        aggregate_access_items(
            &[item],
            true,
            1.0,
            RuleScope::Files,
            Some(&rules),
            true,
            now,
            &mut accumulator,
        );

        let archive = accumulator
            .finish()
            .into_iter()
            .find(|term| term.text == "Archive")
            .unwrap();
        assert_eq!(archive.blacklist_hits, 0);
    }

    #[test]
    fn finish_sorts_ties_stably_and_caps_terms_at_forty() {
        fn accumulated(reverse: bool) -> Vec<WordCloudTerm> {
            let mut accumulator = TermAccumulator::default();
            let indexes: Box<dyn Iterator<Item = usize>> = if reverse {
                Box::new((0..45).rev())
            } else {
                Box::new(0..45)
            };
            for index in indexes {
                let text = format!("term{index:02}");
                accumulator.add_access(
                    &Token {
                        key: text.clone(),
                        display: text,
                    },
                    1.0,
                    true,
                    0,
                    false,
                    false,
                );
            }
            accumulator.finish()
        }

        let ascending = accumulated(false);
        let reverse = accumulated(true);
        let expected = (0..40)
            .map(|index| format!("term{index:02}"))
            .collect::<Vec<_>>();

        assert_eq!(ascending.len(), MAX_TERMS);
        assert_eq!(
            ascending
                .iter()
                .map(|term| term.text.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert_eq!(ascending, reverse);
    }

    #[test]
    fn finish_uses_the_most_frequent_display_form() {
        let mut accumulator = TermAccumulator::default();
        for display in ["Report", "Report", "report"] {
            accumulator.add_access(
                &Token {
                    key: "report".to_string(),
                    display: display.to_string(),
                },
                1.0,
                true,
                0,
                false,
                false,
            );
        }

        let report = accumulator.finish().pop().unwrap();

        assert_eq!(report.text, "Report");
        assert_eq!(report.occurrences, 3);
    }

    #[test]
    fn serialized_result_contains_aggregates_but_no_paths() {
        let result = WordCloudResult {
            source: WordCloudSource::Cleanup,
            range: Some(WordCloudRange::Last30Days),
            corpus_items: 1,
            truncated: false,
            blacklist_terms_included: false,
            blacklist_terms_available: true,
            terms: vec![WordCloudTerm {
                text: "report".to_string(),
                score: 1.0,
                occurrences: 1,
                recent_occurrences: 1,
                frequent_occurrences: 0,
                cleanup_occurrences: 1,
                total_access_count: 0,
                manual_cleanups: 1,
                automatic_cleanups: 0,
                pinned_items: 0,
                blacklist_hits: 0,
            }],
            warnings: Vec::new(),
        };

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["source"], "cleanup");
        assert_eq!(value["range"], "last30_days");
        assert_eq!(value["terms"][0]["text"], "report");
        assert_eq!(value["terms"][0]["occurrences"], 1);
        assert!(!value.to_string().to_lowercase().contains("path"));
        assert_eq!(
            value.as_object().unwrap().keys().collect::<HashSet<_>>(),
            json!({
                "source": null,
                "range": null,
                "corpus_items": null,
                "truncated": null,
                "blacklist_terms_included": null,
                "blacklist_terms_available": null,
                "terms": null,
                "warnings": null
            })
            .as_object()
            .unwrap()
            .keys()
            .collect::<HashSet<_>>()
        );
    }

    #[test]
    fn cleanup_ranges_exclude_hidden_and_out_of_range_records() {
        let database = DbState::for_test();
        let now = Utc::now();
        let timestamp = |days_ago| {
            (now - Duration::days(days_ago))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        };
        database
            .with_connection(|connection| {
                for (path, visible, cleaned_at) in [
                    (r"C:\Reports\recent-report.txt", 1, timestamp(1)),
                    (r"C:\项目\项目报告.txt", 1, timestamp(10)),
                    (r"C:\Archive\ancient-record.txt", 1, timestamp(40)),
                    (r"C:\Hidden\secret-record.txt", 0, timestamp(0)),
                ] {
                    connection.execute(
                        "INSERT INTO clean_records
                         (item_path, item_type, source, cleaned_at, visible)
                         VALUES (?1, 'recent_file', 'manual', ?2, ?3)",
                        params![path, cleaned_at, visible],
                    )?;
                }
                Ok(())
            })
            .unwrap();
        let config = Config::new("en-US".to_string());

        let last_seven =
            build_cleanup_word_cloud(WordCloudRange::Last7Days, &database, &config).unwrap();
        let last_thirty =
            build_cleanup_word_cloud(WordCloudRange::Last30Days, &database, &config).unwrap();
        let all_time =
            build_cleanup_word_cloud(WordCloudRange::AllTime, &database, &config).unwrap();

        assert_eq!(last_seven.corpus_items, 1);
        assert_eq!(last_thirty.corpus_items, 2);
        assert_eq!(all_time.corpus_items, 3);
        assert!(last_seven.terms.iter().any(|term| term.text == "report"));
        assert!(last_thirty.terms.iter().any(|term| term.text == "项目"));
        assert!(last_thirty
            .terms
            .iter()
            .all(|term| !matches!(term.text.as_str(), "ancient" | "secret")));
        assert!(all_time.terms.iter().any(|term| term.text == "ancient"));
        assert!(all_time.terms.iter().all(|term| term.text != "secret"));
    }

    #[test]
    fn cleanup_cloud_caps_visible_records_and_reports_truncation() {
        let database = DbState::for_test();
        let cleaned_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        database
            .with_connection(|connection| {
                let transaction = connection.transaction()?;
                {
                    let mut statement = transaction.prepare(
                        "INSERT INTO clean_records
                         (item_path, item_type, source, cleaned_at, visible)
                         VALUES (?1, 'recent_file', 'auto', ?2, 1)",
                    )?;
                    for _ in 0..=MAX_CLEANUP_RECORDS {
                        statement.execute(params![r"C:\Reports\sample-report.txt", cleaned_at])?;
                    }
                }
                transaction.commit()?;
                Ok(())
            })
            .unwrap();
        let config = Config::new("en-US".to_string());

        let result =
            build_cleanup_word_cloud(WordCloudRange::Last7Days, &database, &config).unwrap();
        let report = result
            .terms
            .iter()
            .find(|term| term.text == "report")
            .unwrap();

        assert_eq!(result.corpus_items, MAX_CLEANUP_RECORDS as u64);
        assert!(result.truncated);
        assert_eq!(report.occurrences, MAX_CLEANUP_RECORDS as u64);
        assert_eq!(report.automatic_cleanups, MAX_CLEANUP_RECORDS as u64);
    }
}
