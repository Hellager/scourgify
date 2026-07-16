use anyhow::{Context, Result};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    Connection,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum HistoryError {
    #[error("history page must be at least 1")]
    Page,
    #[error("history page_size must be between 1 and {MAX_PAGE_SIZE}")]
    PageSize,
    #[error("unsupported history sort field: {0}")]
    SortField(String),
    #[error("history sort_order must be asc or desc")]
    SortOrder,
    #[error("unsupported history item_type: {0}")]
    ItemType(String),
    #[error("unsupported history date_range: {0}")]
    DateRange(String),
    #[error("history export path is invalid: {0}")]
    ExportPath(String),
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CleanSource {
    Manual,
    Auto,
}

impl CleanSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Auto => "auto",
        }
    }
}

impl FromSql for CleanSource {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "manual" => Ok(Self::Manual),
            "auto" => Ok(Self::Auto),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCleanRecord {
    pub run_id: Option<i64>,
    pub item_path: String,
    pub item_type: String,
    pub rule_id: Option<i64>,
    pub rule_keyword: Option<String>,
    pub source: CleanSource,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HistoryQuery {
    pub page: u32,
    pub page_size: u32,
    pub sort_by: String,
    pub sort_order: String,
    #[serde(flatten)]
    pub filter: HistoryFilter,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct HistoryFilter {
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub matched_by_rule: Option<bool>,
    #[serde(default)]
    pub source: Option<CleanSource>,
    #[serde(default)]
    pub run_id: Option<i64>,
    #[serde(default)]
    pub date_range: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HistoryExportFormat {
    Csv,
    Json,
}

impl HistoryExportFormat {
    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HistoryExportResult {
    pub count: u64,
    pub path: String,
    pub format: HistoryExportFormat,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CleanRecord {
    pub id: i64,
    pub run_id: Option<i64>,
    pub item_path: String,
    pub item_type: String,
    pub rule_id: Option<i64>,
    pub rule_keyword: Option<String>,
    pub source: CleanSource,
    pub cleaned_at: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ExportCleanRecord {
    run_id: Option<i64>,
    item_path: String,
    item_type: String,
    rule_id: Option<i64>,
    rule_keyword: Option<String>,
    source: CleanSource,
    cleaned_at: String,
}

impl From<CleanRecord> for ExportCleanRecord {
    fn from(record: CleanRecord) -> Self {
        Self {
            run_id: record.run_id,
            item_path: record.item_path,
            item_type: record.item_type,
            rule_id: record.rule_id,
            rule_keyword: record.rule_keyword,
            source: record.source,
            cleaned_at: record.cleaned_at,
        }
    }
}

pub(super) struct ValidatedHistoryFilter {
    search: Option<String>,
    item_type: Option<&'static str>,
    matched_by_rule: Option<i64>,
    source: Option<&'static str>,
    run_id: Option<i64>,
    date_modifier: Option<&'static str>,
}

pub(super) const HISTORY_FILTERS: &str =
    "WHERE (?1 IS NULL OR item_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                    OR COALESCE(rule_keyword, '') LIKE ?1 ESCAPE '\\' COLLATE NOCASE)
           AND (?2 IS NULL OR item_type = ?2)
           AND (?3 IS NULL
                OR (?3 = 1 AND rule_keyword IS NOT NULL)
                OR (?3 = 0 AND rule_keyword IS NULL))
           AND (?4 IS NULL
                OR cleaned_at >= datetime('now', 'localtime', 'start of day', ?4, 'utc'))
           AND (?5 IS NULL OR source = ?5)
           AND (?6 IS NULL OR run_id = ?6)";

pub(super) const HISTORY_COLUMNS: &str =
    "id, run_id, item_path, item_type, rule_id, rule_keyword, source, cleaned_at";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CleanRecordPage {
    pub records: Vec<CleanRecord>,
    pub total: u64,
    pub overall_total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct StatsTrendPoint {
    pub period: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuleHitStat {
    pub keyword: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Stats {
    pub total: u64,
    pub recent_files: u64,
    pub frequent_folders: u64,
    pub retained_total: u64,
    pub daily_trend: Vec<StatsTrendPoint>,
    pub weekly_trend: Vec<StatsTrendPoint>,
    pub rule_hits: Vec<RuleHitStat>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct HistoryTotals {
    pub total: u64,
    pub recent_files: u64,
    pub frequent_folders: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum StatsRange {
    #[serde(rename = "7d")]
    Last7Days,
    #[serde(rename = "30d")]
    Last30Days,
    #[serde(rename = "all")]
    All,
}

impl StatsRange {
    pub(super) fn date_modifier(self) -> Option<&'static str> {
        match self {
            Self::Last7Days => Some("-6 days"),
            Self::Last30Days => Some("-29 days"),
            Self::All => None,
        }
    }
}

pub fn insert_batch(
    connection: &mut Connection,
    records: &[NewCleanRecord],
    retention: usize,
) -> Result<()> {
    if records.is_empty() {
        return Ok(());
    }

    let transaction = connection
        .transaction()
        .context("failed to start clean record transaction")?;
    {
        let mut statement = transaction
            .prepare(
                "INSERT INTO clean_records
                    (run_id, item_path, item_type, rule_id, rule_keyword, source)
                 VALUES (?1, ?2, ?3, (SELECT id FROM rules WHERE id = ?4), ?5, ?6)",
            )
            .context("failed to prepare clean record insert")?;
        for record in records {
            statement
                .execute(params![
                    record.run_id,
                    record.item_path,
                    record.item_type,
                    record.rule_id,
                    record.rule_keyword,
                    record.source.as_str()
                ])
                .with_context(|| format!("failed to record cleanup for {}", record.item_path))?;
        }
    }
    let cleaned_files = records
        .iter()
        .filter(|record| record.item_type == "recent_file")
        .count();
    let cleaned_folders = records.len() - cleaned_files;
    transaction
        .execute(
            "UPDATE cleanup_totals
             SET cleaned_total = cleaned_total + ?1,
                 cleaned_files = cleaned_files + ?2,
                 cleaned_folders = cleaned_folders + ?3,
                 updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now')
             WHERE id = 1",
            params![
                i64::try_from(records.len()).context("clean record count is too large")?,
                i64::try_from(cleaned_files).context("cleaned file count is too large")?,
                i64::try_from(cleaned_folders).context("cleaned folder count is too large")?,
            ],
        )
        .context("failed to update lifetime cleanup totals")?;
    trim(&transaction, retention)?;
    transaction
        .commit()
        .context("failed to commit clean records")
}

pub fn list(connection: &Connection, query: HistoryQuery) -> Result<CleanRecordPage> {
    if query.page == 0 {
        return Err(HistoryError::Page.into());
    }
    if !(1..=MAX_PAGE_SIZE).contains(&query.page_size) {
        return Err(HistoryError::PageSize.into());
    }

    let sort_column = match query.sort_by.as_str() {
        "cleaned_at" => "cleaned_at",
        "item_path" => "item_path",
        "item_type" => "item_type",
        "rule_keyword" => "rule_keyword",
        _ => return Err(HistoryError::SortField(query.sort_by).into()),
    };
    let sort_order = match query.sort_order.as_str() {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => return Err(HistoryError::SortOrder.into()),
    };
    let filter = validate_history_filter(query.filter)?;
    let count_sql = format!("SELECT COUNT(*) FROM clean_records {HISTORY_FILTERS}");
    let overall_total = connection
        .query_row("SELECT COUNT(*) FROM clean_records", [], |row| {
            row.get::<_, i64>(0)
        })
        .context("failed to count all clean records")?;
    let overall_total = u64::try_from(overall_total).context("clean record count is negative")?;
    let total = connection
        .query_row(&count_sql, history_filter_params(&filter), |row| {
            row.get::<_, i64>(0)
        })
        .context("failed to count clean records")?;
    let total = u64::try_from(total).context("clean record count is negative")?;
    let offset = i64::try_from(u64::from(query.page - 1) * u64::from(query.page_size))
        .context("history page offset is too large")?;
    let sql = format!(
        "SELECT {HISTORY_COLUMNS}
         FROM clean_records
         {HISTORY_FILTERS}
         ORDER BY {sort_column} {sort_order}, id {sort_order}
         LIMIT ?7 OFFSET ?8"
    );
    let mut statement = connection
        .prepare(&sql)
        .context("failed to prepare clean record query")?;
    let records = statement
        .query_map(
            params![
                filter.search.as_deref(),
                filter.item_type,
                filter.matched_by_rule,
                filter.date_modifier,
                filter.source,
                filter.run_id,
                i64::from(query.page_size),
                offset
            ],
            read_clean_record,
        )
        .context("failed to query clean records")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read clean records")?;

    Ok(CleanRecordPage {
        records,
        total,
        overall_total,
        page: query.page,
        page_size: query.page_size,
    })
}

pub fn export(
    connection: &Connection,
    path: &str,
    format: HistoryExportFormat,
    filter: HistoryFilter,
) -> Result<HistoryExportResult> {
    super::history_export::export(connection, path, format, filter)
}

pub fn export_runs(
    connection: &Connection,
    path: &str,
    format: HistoryExportFormat,
    filter: super::history_runs::CleanupRunFilter,
) -> Result<HistoryExportResult> {
    super::history_export::export_runs(connection, path, format, filter)
}

pub(super) fn validate_history_filter(filter: HistoryFilter) -> Result<ValidatedHistoryFilter> {
    let item_type = match filter.item_type.as_deref() {
        None => None,
        Some("recent_file") => Some("recent_file"),
        Some("frequent_folder") => Some("frequent_folder"),
        Some(value) => return Err(HistoryError::ItemType(value.to_string()).into()),
    };
    let date_modifier = match filter.date_range.as_deref() {
        None => None,
        Some("7d") => Some("-6 days"),
        Some("30d") => Some("-29 days"),
        Some(value) => return Err(HistoryError::DateRange(value.to_string()).into()),
    };
    Ok(ValidatedHistoryFilter {
        search: history_search_pattern(&filter.search),
        item_type,
        matched_by_rule: filter.matched_by_rule.map(i64::from),
        source: filter.source.map(CleanSource::as_str),
        run_id: filter.run_id,
        date_modifier,
    })
}

pub(super) fn history_filter_params(filter: &ValidatedHistoryFilter) -> [&dyn rusqlite::ToSql; 6] {
    [
        &filter.search,
        &filter.item_type,
        &filter.matched_by_rule,
        &filter.date_modifier,
        &filter.source,
        &filter.run_id,
    ]
}

pub(super) fn read_clean_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CleanRecord> {
    Ok(CleanRecord {
        id: row.get(0)?,
        run_id: row.get(1)?,
        item_path: row.get(2)?,
        item_type: row.get(3)?,
        rule_id: row.get(4)?,
        rule_keyword: row.get(5)?,
        source: row.get(6)?,
        cleaned_at: row.get(7)?,
    })
}

fn history_search_pattern(search: &str) -> Option<String> {
    let search = search.trim();
    if search.is_empty() {
        return None;
    }
    let escaped = search
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    Some(format!("%{escaped}%"))
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct HistoryClearResult {
    pub clean_records: u64,
    pub cleanup_runs: u64,
}

pub fn clear(connection: &mut Connection) -> Result<HistoryClearResult> {
    let transaction = connection
        .transaction()
        .context("failed to start history clear transaction")?;
    let clean_records = transaction
        .execute("DELETE FROM clean_records", [])
        .context("failed to clear clean records")?;
    let cleanup_runs = transaction
        .execute("DELETE FROM cleanup_runs", [])
        .context("failed to clear cleanup runs")?;
    transaction
        .commit()
        .context("failed to commit history clear")?;
    Ok(HistoryClearResult {
        clean_records: u64::try_from(clean_records).context("cleared record count is too large")?,
        cleanup_runs: u64::try_from(cleanup_runs).context("cleared run count is too large")?,
    })
}

pub fn stats(connection: &Connection, range: StatsRange) -> Result<Stats> {
    super::stats::stats(connection, range)
}

pub fn totals(connection: &Connection) -> Result<HistoryTotals> {
    let totals = super::history_runs::lifetime_totals(connection)?;
    Ok(HistoryTotals {
        total: totals.cleaned_total,
        recent_files: totals.cleaned_files,
        frequent_folders: totals.cleaned_folders,
    })
}

pub fn trim_to(connection: &Connection, retention: usize) -> Result<()> {
    trim(connection, retention)
}

fn trim(connection: &Connection, retention: usize) -> Result<()> {
    if retention == 0 {
        return Ok(());
    }
    let retention = i64::try_from(retention).context("history retention is too large")?;
    connection
        .execute(
            "DELETE FROM clean_records
             WHERE id NOT IN (
                 SELECT id FROM clean_records ORDER BY cleaned_at DESC, id DESC LIMIT ?1
             )",
            [retention],
        )
        .context("failed to trim clean records")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_targeted_and_neutral_records() {
        let mut connection = test_connection();
        let rule_id = connection
            .query_row("SELECT id FROM rules ORDER BY id LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        let records = [
            record(
                r"C:\Users\test\Desktop\report.txt",
                "recent_file",
                Some(rule_id),
                Some("Desktop"),
            ),
            record(r"C:\Users\test\Downloads", "frequent_folder", None, None),
        ];

        insert_batch(&mut connection, &records, 0).unwrap();

        assert_eq!(record_count(&connection), 2);
        let (stored_rule_id, keyword) = connection
            .query_row(
                "SELECT rule_id, rule_keyword FROM clean_records WHERE item_path = ?1",
                [records[0].item_path.as_str()],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(stored_rule_id, Some(rule_id));
        assert_eq!(keyword, "Desktop");
        assert_eq!(
            list(&connection, history_query()).unwrap().records[0].source,
            CleanSource::Manual
        );
    }

    #[test]
    fn stores_and_lists_auto_cleanup_source() {
        let mut connection = test_connection();
        let mut auto_record = record(r"C:\auto.txt", "recent_file", None, None);
        auto_record.source = CleanSource::Auto;

        insert_batch(&mut connection, &[auto_record], 0).unwrap();

        assert_eq!(
            list(&connection, history_query()).unwrap().records[0].source,
            CleanSource::Auto
        );
    }

    #[test]
    fn preserves_keyword_when_matching_rule_was_deleted() {
        let mut connection = test_connection();
        let rule_id = connection
            .query_row("SELECT id FROM rules ORDER BY id LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        connection
            .execute("DELETE FROM rules WHERE id = ?1", [rule_id])
            .unwrap();

        insert_batch(
            &mut connection,
            &[record(
                r"C:\Users\test\Desktop\report.txt",
                "recent_file",
                Some(rule_id),
                Some("Desktop"),
            )],
            0,
        )
        .unwrap();

        let (stored_rule_id, keyword) = connection
            .query_row(
                "SELECT rule_id, rule_keyword FROM clean_records",
                [],
                |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(stored_rule_id, None);
        assert_eq!(keyword, "Desktop");
    }

    #[test]
    fn rolls_back_entire_batch_when_one_record_is_invalid() {
        let mut connection = test_connection();
        let records = [
            record(r"C:\valid.txt", "recent_file", None, None),
            record(r"C:\invalid.txt", "invalid_type", None, None),
        ];

        assert!(insert_batch(&mut connection, &records, 0).is_err());
        assert_eq!(record_count(&connection), 0);
    }

    #[test]
    fn lists_records_with_server_side_sorting_and_pagination() {
        let mut connection = test_connection();
        insert_batch(
            &mut connection,
            &[
                record(r"C:\c.txt", "recent_file", None, None),
                record(r"C:\a.txt", "recent_file", None, None),
                record(r"C:\b.txt", "frequent_folder", None, None),
            ],
            0,
        )
        .unwrap();

        let first = list(
            &connection,
            HistoryQuery {
                page: 1,
                page_size: 2,
                sort_by: "item_path".to_string(),
                sort_order: "asc".to_string(),
                ..history_query()
            },
        )
        .unwrap();
        let second = list(
            &connection,
            HistoryQuery {
                page: 2,
                page_size: 2,
                sort_by: "item_path".to_string(),
                sort_order: "asc".to_string(),
                ..history_query()
            },
        )
        .unwrap();

        assert_eq!(first.total, 3);
        assert_eq!(
            first
                .records
                .iter()
                .map(|record| record.item_path.as_str())
                .collect::<Vec<_>>(),
            [r"C:\a.txt", r"C:\b.txt"]
        );
        assert_eq!(second.records[0].item_path, r"C:\c.txt");
        assert!(list(
            &connection,
            HistoryQuery {
                page: 1,
                page_size: 20,
                sort_by: "id; DROP TABLE clean_records".to_string(),
                sort_order: "asc".to_string(),
                ..history_query()
            }
        )
        .is_err());
    }

    #[test]
    fn filters_history_before_counting_sorting_and_pagination() {
        let connection = test_connection();
        for (path, item_type, keyword, age) in [
            (
                r"C:\Temp\100% cache.bin",
                "recent_file",
                Some("Cache"),
                "0 days",
            ),
            (r"C:\Temp\manual.txt", "recent_file", None, "0 days"),
            (r"C:\Archive", "frequent_folder", None, "-10 days"),
            (r"C:\Old\report.txt", "recent_file", Some("Old"), "-40 days"),
        ] {
            connection
                .execute(
                    "INSERT INTO clean_records
                     (item_path, item_type, rule_keyword, cleaned_at)
                     VALUES (?1, ?2, ?3, datetime('now', ?4))",
                    params![path, item_type, keyword, age],
                )
                .unwrap();
        }

        let literal_wildcard = list(
            &connection,
            HistoryQuery {
                filter: HistoryFilter {
                    search: "100%".to_string(),
                    ..HistoryFilter::default()
                },
                ..history_query()
            },
        )
        .unwrap();
        assert_eq!(literal_wildcard.total, 1);
        assert_eq!(literal_wildcard.overall_total, 4);
        assert_eq!(
            literal_wildcard.records[0].rule_keyword.as_deref(),
            Some("Cache")
        );

        let targeted_recent = list(
            &connection,
            HistoryQuery {
                filter: HistoryFilter {
                    item_type: Some("recent_file".to_string()),
                    matched_by_rule: Some(true),
                    date_range: Some("30d".to_string()),
                    ..HistoryFilter::default()
                },
                ..history_query()
            },
        )
        .unwrap();
        assert_eq!(targeted_recent.total, 1);
        assert_eq!(
            targeted_recent.records[0].item_path,
            r"C:\Temp\100% cache.bin"
        );

        let manual_first = list(
            &connection,
            HistoryQuery {
                page_size: 1,
                sort_by: "item_path".to_string(),
                sort_order: "asc".to_string(),
                filter: HistoryFilter {
                    matched_by_rule: Some(false),
                    date_range: Some("30d".to_string()),
                    ..HistoryFilter::default()
                },
                ..history_query()
            },
        )
        .unwrap();
        let manual_second = list(
            &connection,
            HistoryQuery {
                page: 2,
                page_size: 1,
                sort_by: "item_path".to_string(),
                sort_order: "asc".to_string(),
                filter: HistoryFilter {
                    matched_by_rule: Some(false),
                    date_range: Some("30d".to_string()),
                    ..HistoryFilter::default()
                },
            },
        )
        .unwrap();
        assert_eq!(manual_first.total, 2);
        assert_eq!(manual_first.records[0].item_path, r"C:\Archive");
        assert_eq!(manual_second.records[0].item_path, r"C:\Temp\manual.txt");

        assert!(list(
            &connection,
            HistoryQuery {
                filter: HistoryFilter {
                    item_type: Some("invalid".to_string()),
                    ..HistoryFilter::default()
                },
                ..history_query()
            }
        )
        .is_err());
        assert!(list(
            &connection,
            HistoryQuery {
                filter: HistoryFilter {
                    date_range: Some("forever".to_string()),
                    ..HistoryFilter::default()
                },
                ..history_query()
            }
        )
        .is_err());
    }

    #[test]
    fn exports_filtered_csv_with_special_characters() {
        let mut connection = test_connection();
        let special_path = "C:\\Temp\\\"report\",\n报告.txt";
        let special_keyword = "alpha,\"beta\"\nline";
        let mut targeted = record(special_path, "recent_file", None, Some(special_keyword));
        targeted.source = CleanSource::Auto;
        insert_batch(
            &mut connection,
            &[
                targeted,
                record(r"C:\Temp\manual.txt", "recent_file", None, None),
            ],
            0,
        )
        .unwrap();
        let path = export_path("csv");

        let result = export(
            &connection,
            path.to_str().unwrap(),
            HistoryExportFormat::Csv,
            HistoryFilter {
                search: "报告".to_string(),
                matched_by_rule: Some(true),
                ..HistoryFilter::default()
            },
        )
        .unwrap();

        assert_eq!(result.count, 1);
        let mut reader = csv::Reader::from_path(&path).unwrap();
        assert_eq!(
            reader.headers().unwrap().iter().collect::<Vec<_>>(),
            [
                "run_id",
                "item_path",
                "item_type",
                "rule_id",
                "rule_keyword",
                "source",
                "cleaned_at",
            ]
        );
        let rows = reader.records().collect::<csv::Result<Vec<_>>>().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(&rows[0][1], special_path);
        assert_eq!(&rows[0][4], special_keyword);
        assert_eq!(&rows[0][5], "auto");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn exports_all_json_records_without_pagination() {
        let mut connection = test_connection();
        insert_batch(
            &mut connection,
            &[
                record(r"C:\first.txt", "recent_file", None, None),
                record(r"C:\second.txt", "recent_file", None, Some("second")),
                record(r"C:\Third", "frequent_folder", None, None),
            ],
            0,
        )
        .unwrap();
        let path = export_path("json");

        let result = export(
            &connection,
            path.to_str().unwrap(),
            HistoryExportFormat::Json,
            HistoryFilter::default(),
        )
        .unwrap();

        assert_eq!(result.count, 3);
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let records = value.as_array().unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(
            records[0].as_object().unwrap().keys().collect::<Vec<_>>(),
            [
                "cleaned_at",
                "item_path",
                "item_type",
                "rule_id",
                "rule_keyword",
                "run_id",
                "source",
            ]
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn exports_empty_csv_with_headers() {
        let connection = test_connection();
        let path = export_path("csv");

        let result = export(
            &connection,
            path.to_str().unwrap(),
            HistoryExportFormat::Csv,
            HistoryFilter::default(),
        )
        .unwrap();

        assert_eq!(result.count, 0);
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_invalid_export_targets_and_filters() {
        let connection = test_connection();
        assert!(export(
            &connection,
            "relative.csv",
            HistoryExportFormat::Csv,
            HistoryFilter::default()
        )
        .is_err());

        let wrong_extension = export_path("json");
        assert!(export(
            &connection,
            wrong_extension.to_str().unwrap(),
            HistoryExportFormat::Csv,
            HistoryFilter::default()
        )
        .is_err());

        let invalid_filter = export_path("json");
        assert!(export(
            &connection,
            invalid_filter.to_str().unwrap(),
            HistoryExportFormat::Json,
            HistoryFilter {
                date_range: Some("forever".to_string()),
                ..HistoryFilter::default()
            }
        )
        .is_err());
        assert!(!invalid_filter.exists());

        let directory = export_path("csv");
        std::fs::create_dir(&directory).unwrap();
        assert!(export(
            &connection,
            directory.to_str().unwrap(),
            HistoryExportFormat::Csv,
            HistoryFilter::default()
        )
        .is_err());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn applies_retention_on_insert_and_when_setting_is_reduced() {
        let mut connection = test_connection();
        insert_batch(
            &mut connection,
            &[
                record(r"C:\first.txt", "recent_file", None, None),
                record(r"C:\second.txt", "recent_file", None, None),
                record(r"C:\third.txt", "recent_file", None, None),
            ],
            2,
        )
        .unwrap();

        assert_eq!(record_count(&connection), 2);
        insert_batch(
            &mut connection,
            &[record(r"C:\fourth.txt", "recent_file", None, None)],
            0,
        )
        .unwrap();
        trim_to(&connection, 1).unwrap();
        assert_eq!(record_count(&connection), 1);
        assert_eq!(
            connection
                .query_row("SELECT item_path FROM clean_records", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            r"C:\fourth.txt"
        );
    }

    #[test]
    fn clears_all_records() {
        let mut connection = test_connection();
        insert_batch(
            &mut connection,
            &[record(r"C:\report.txt", "recent_file", None, None)],
            0,
        )
        .unwrap();

        clear(&mut connection).unwrap();

        assert_eq!(record_count(&connection), 0);
    }

    #[test]
    fn aggregates_cleanup_history_statistics() {
        let connection = test_connection();
        for (path, item_type, keyword, cleaned_at) in [
            (
                r"C:\a.txt",
                "recent_file",
                Some("Cache"),
                "2026-01-06 12:00:00",
            ),
            (
                r"C:\b.txt",
                "recent_file",
                Some("cache"),
                "2026-01-06 12:00:00",
            ),
            (
                r"C:\Temp",
                "frequent_folder",
                Some("Temp"),
                "2026-01-07 12:00:00",
            ),
            (r"C:\Work", "frequent_folder", None, "2026-01-07 12:00:00"),
        ] {
            connection
                .execute(
                    "INSERT INTO clean_records
                     (item_path, item_type, rule_keyword, cleaned_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![path, item_type, keyword, cleaned_at],
                )
                .unwrap();
        }
        connection
            .execute(
                "UPDATE cleanup_totals
                 SET cleaned_total = 4, cleaned_files = 2, cleaned_folders = 2",
                [],
            )
            .unwrap();

        let stats = stats(&connection, StatsRange::All).unwrap();

        assert_eq!(stats.total, 4);
        assert_eq!(stats.recent_files, 2);
        assert_eq!(stats.frequent_folders, 2);
        assert_eq!(
            stats
                .daily_trend
                .iter()
                .map(|point| point.count)
                .sum::<u64>(),
            4
        );
        assert_eq!(
            stats
                .weekly_trend
                .iter()
                .map(|point| point.count)
                .sum::<u64>(),
            4
        );
        assert_eq!(
            stats.rule_hits,
            [
                RuleHitStat {
                    keyword: "Cache".to_string(),
                    count: 2,
                },
                RuleHitStat {
                    keyword: "Temp".to_string(),
                    count: 1,
                },
            ]
        );
    }

    #[test]
    fn lifetime_totals_survive_detail_retention() {
        let mut connection = test_connection();
        insert_batch(
            &mut connection,
            &[
                record(r"C:\first.txt", "recent_file", None, None),
                record(r"C:\second.txt", "recent_file", None, None),
                record(r"C:\folder", "frequent_folder", None, None),
            ],
            0,
        )
        .unwrap();
        trim_to(&connection, 1).unwrap();

        let lifetime = totals(&connection).unwrap();
        assert_eq!(lifetime.total, 3);
        assert_eq!(lifetime.recent_files, 2);
        assert_eq!(lifetime.frequent_folders, 1);
        assert_eq!(list(&connection, history_query()).unwrap().total, 1);
    }

    #[test]
    fn fills_local_date_ranges_and_matches_history_boundaries() {
        let connection = test_connection();
        for (path, modifier) in [
            (r"C:\today.txt", "+0 days"),
            (r"C:\two-days-ago.txt", "-2 days"),
            (r"C:\older.txt", "-40 days"),
        ] {
            connection
                .execute(
                    "INSERT INTO clean_records (item_path, item_type, cleaned_at)
                     VALUES (?1, 'recent_file',
                             datetime('now', 'localtime', 'start of day', ?2, 'utc'))",
                    params![path, modifier],
                )
                .unwrap();
        }
        connection
            .execute(
                "UPDATE cleanup_totals SET cleaned_total = 3, cleaned_files = 3",
                [],
            )
            .unwrap();

        let last_seven = stats(&connection, StatsRange::Last7Days).unwrap();
        let last_thirty = stats(&connection, StatsRange::Last30Days).unwrap();
        let all = stats(&connection, StatsRange::All).unwrap();
        let mut query = history_query();
        query.filter.date_range = Some("7d".to_string());
        let history = list(&connection, query).unwrap();
        let earliest_local_day = connection
            .query_row(
                "SELECT MIN(date(cleaned_at, 'localtime')) FROM clean_records",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();

        assert_eq!(last_seven.daily_trend.len(), 7);
        assert_eq!(last_thirty.daily_trend.len(), 30);
        assert_eq!(all.daily_trend.first().unwrap().period, earliest_local_day);
        assert!(last_seven.daily_trend.iter().any(|point| point.count == 0));
        assert_eq!(
            last_seven
                .daily_trend
                .iter()
                .map(|point| point.count)
                .sum::<u64>(),
            history.total
        );
        assert_eq!(
            last_seven
                .weekly_trend
                .iter()
                .map(|point| point.count)
                .sum::<u64>(),
            history.total
        );
        assert!(last_thirty
            .weekly_trend
            .iter()
            .any(|point| point.count == 0));
        assert_eq!(
            last_thirty
                .daily_trend
                .iter()
                .map(|point| point.count)
                .sum::<u64>(),
            2
        );
        assert_eq!(
            all.daily_trend.iter().map(|point| point.count).sum::<u64>(),
            3
        );
        assert_eq!(last_seven.total, 3);
        assert_eq!(last_thirty.total, 3);
        assert_eq!(all.total, 3);
    }

    fn record(
        item_path: &str,
        item_type: &str,
        rule_id: Option<i64>,
        rule_keyword: Option<&str>,
    ) -> NewCleanRecord {
        NewCleanRecord {
            run_id: None,
            item_path: item_path.to_string(),
            item_type: item_type.to_string(),
            rule_id,
            rule_keyword: rule_keyword.map(str::to_string),
            source: CleanSource::Manual,
        }
    }

    fn history_query() -> HistoryQuery {
        HistoryQuery {
            page: 1,
            page_size: 20,
            sort_by: "cleaned_at".to_string(),
            sort_order: "desc".to_string(),
            filter: HistoryFilter::default(),
        }
    }

    fn export_path(extension: &str) -> std::path::PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "scourgify-history-export-{}-{unique}.{extension}",
            std::process::id()
        ))
    }

    fn test_connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        super::super::migrate(&mut connection).unwrap();
        connection
    }

    fn record_count(connection: &Connection) -> i64 {
        connection
            .query_row("SELECT COUNT(*) FROM clean_records", [], |row| row.get(0))
            .unwrap()
    }
}
