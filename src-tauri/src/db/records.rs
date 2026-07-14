use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{bail, Context, Result};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    Connection,
};
use serde::{Deserialize, Serialize};

const MAX_PAGE_SIZE: u32 = 100;

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
    pub date_range: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HistoryExportFormat {
    Csv,
    Json,
}

impl HistoryExportFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HistoryExportResult {
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CleanRecord {
    pub id: i64,
    pub item_path: String,
    pub item_type: String,
    pub rule_id: Option<i64>,
    pub rule_keyword: Option<String>,
    pub source: CleanSource,
    pub cleaned_at: String,
}

#[derive(Debug, Serialize)]
struct ExportCleanRecord {
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
            item_path: record.item_path,
            item_type: record.item_type,
            rule_id: record.rule_id,
            rule_keyword: record.rule_keyword,
            source: record.source,
            cleaned_at: record.cleaned_at,
        }
    }
}

struct ValidatedHistoryFilter {
    search: Option<String>,
    item_type: Option<&'static str>,
    matched_by_rule: Option<i64>,
    date_modifier: Option<&'static str>,
}

const HISTORY_FILTERS: &str = "WHERE (?1 IS NULL OR item_path LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                    OR COALESCE(rule_keyword, '') LIKE ?1 ESCAPE '\\' COLLATE NOCASE)
           AND (?2 IS NULL OR item_type = ?2)
           AND (?3 IS NULL
                OR (?3 = 1 AND rule_keyword IS NOT NULL)
                OR (?3 = 0 AND rule_keyword IS NULL))
           AND (?4 IS NULL
                OR cleaned_at >= datetime('now', 'localtime', 'start of day', ?4, 'utc'))";

const HISTORY_COLUMNS: &str = "id, item_path, item_type, rule_id, rule_keyword, source, cleaned_at";

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
    pub daily_trend: Vec<StatsTrendPoint>,
    pub weekly_trend: Vec<StatsTrendPoint>,
    pub rule_hits: Vec<RuleHitStat>,
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
    fn date_modifier(self) -> Option<&'static str> {
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
                "INSERT INTO clean_records (item_path, item_type, rule_id, rule_keyword, source)
                 VALUES (?1, ?2, (SELECT id FROM rules WHERE id = ?3), ?4, ?5)",
            )
            .context("failed to prepare clean record insert")?;
        for record in records {
            statement
                .execute(params![
                    record.item_path,
                    record.item_type,
                    record.rule_id,
                    record.rule_keyword,
                    record.source.as_str()
                ])
                .with_context(|| format!("failed to record cleanup for {}", record.item_path))?;
        }
    }
    trim(&transaction, retention)?;
    transaction
        .commit()
        .context("failed to commit clean records")
}

pub fn list(connection: &Connection, query: HistoryQuery) -> Result<CleanRecordPage> {
    if query.page == 0 {
        bail!("history page must be at least 1");
    }
    if !(1..=MAX_PAGE_SIZE).contains(&query.page_size) {
        bail!("history page_size must be between 1 and {MAX_PAGE_SIZE}");
    }

    let sort_column = match query.sort_by.as_str() {
        "cleaned_at" => "cleaned_at",
        "item_path" => "item_path",
        "item_type" => "item_type",
        "rule_keyword" => "rule_keyword",
        _ => bail!("unsupported history sort field: {}", query.sort_by),
    };
    let sort_order = match query.sort_order.as_str() {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => bail!("history sort_order must be asc or desc"),
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
         LIMIT ?5 OFFSET ?6"
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
    let path = validate_export_path(path, format)?;
    let filter = validate_history_filter(filter)?;
    let file = File::create(path)
        .with_context(|| format!("failed to create history export at {}", path.display()))?;
    let count = match format {
        HistoryExportFormat::Csv => export_csv(connection, file, &filter)?,
        HistoryExportFormat::Json => export_json(connection, file, &filter)?,
    };
    Ok(HistoryExportResult { count })
}

fn export_csv(connection: &Connection, file: File, filter: &ValidatedHistoryFilter) -> Result<u64> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(BufWriter::new(file));
    writer
        .write_record([
            "item_path",
            "item_type",
            "rule_id",
            "rule_keyword",
            "source",
            "cleaned_at",
        ])
        .context("failed to write CSV history header")?;
    let count = visit_filtered_records(connection, filter, |record| {
        writer
            .serialize(ExportCleanRecord::from(record))
            .context("failed to serialize CSV history record")
    })?;
    writer
        .flush()
        .context("failed to finish CSV history export")?;
    Ok(count)
}

fn export_json(
    connection: &Connection,
    file: File,
    filter: &ValidatedHistoryFilter,
) -> Result<u64> {
    let mut records = Vec::new();
    let count = visit_filtered_records(connection, filter, |record| {
        records.push(ExportCleanRecord::from(record));
        Ok(())
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &records)
        .context("failed to serialize JSON history export")?;
    writer
        .flush()
        .context("failed to finish JSON history export")?;
    Ok(count)
}

fn visit_filtered_records(
    connection: &Connection,
    filter: &ValidatedHistoryFilter,
    mut visit: impl FnMut(CleanRecord) -> Result<()>,
) -> Result<u64> {
    let sql = format!(
        "SELECT {HISTORY_COLUMNS}
         FROM clean_records
         {HISTORY_FILTERS}
         ORDER BY cleaned_at DESC, id DESC"
    );
    let mut statement = connection
        .prepare(&sql)
        .context("failed to prepare history export query")?;
    let rows = statement
        .query_map(history_filter_params(filter), read_clean_record)
        .context("failed to query history export")?;
    let mut count = 0_u64;
    for row in rows {
        visit(row.context("failed to read history export record")?)?;
        count += 1;
    }
    Ok(count)
}

fn validate_history_filter(filter: HistoryFilter) -> Result<ValidatedHistoryFilter> {
    let item_type = match filter.item_type.as_deref() {
        None => None,
        Some("recent_file") => Some("recent_file"),
        Some("frequent_folder") => Some("frequent_folder"),
        Some(value) => bail!("unsupported history item_type: {value}"),
    };
    let date_modifier = match filter.date_range.as_deref() {
        None => None,
        Some("7d") => Some("-6 days"),
        Some("30d") => Some("-29 days"),
        Some(value) => bail!("unsupported history date_range: {value}"),
    };
    Ok(ValidatedHistoryFilter {
        search: history_search_pattern(&filter.search),
        item_type,
        matched_by_rule: filter.matched_by_rule.map(i64::from),
        date_modifier,
    })
}

fn history_filter_params(filter: &ValidatedHistoryFilter) -> [&dyn rusqlite::ToSql; 4] {
    [
        &filter.search,
        &filter.item_type,
        &filter.matched_by_rule,
        &filter.date_modifier,
    ]
}

fn read_clean_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CleanRecord> {
    Ok(CleanRecord {
        id: row.get(0)?,
        item_path: row.get(1)?,
        item_type: row.get(2)?,
        rule_id: row.get(3)?,
        rule_keyword: row.get(4)?,
        source: row.get(5)?,
        cleaned_at: row.get(6)?,
    })
}

fn validate_export_path(path: &str, format: HistoryExportFormat) -> Result<&Path> {
    if path.trim().is_empty() {
        bail!("history export path is empty");
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        bail!("history export path must be absolute");
    }
    let extension_matches = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(format.extension()));
    if !extension_matches {
        bail!(
            "history export path must use the .{} extension",
            format.extension()
        );
    }
    path.parent()
        .filter(|parent| parent.is_dir())
        .context("history export directory does not exist")?;
    Ok(path)
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

pub fn clear(connection: &Connection) -> Result<()> {
    connection
        .execute("DELETE FROM clean_records", [])
        .context("failed to clear clean records")?;
    Ok(())
}

pub fn stats(connection: &Connection, range: StatsRange) -> Result<Stats> {
    let (total, recent_files, frequent_folders) = connection
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(item_type = 'recent_file'), 0),
                    COALESCE(SUM(item_type = 'frequent_folder'), 0)
             FROM clean_records",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .context("failed to count cleanup statistics")?;

    let total = u64::try_from(total).context("cleanup total is negative")?;
    let (daily_trend, weekly_trend) = if total == 0 {
        (Vec::new(), Vec::new())
    } else {
        (
            trend(connection, range, false, "daily cleanup trend")?,
            trend(connection, range, true, "weekly cleanup trend")?,
        )
    };

    Ok(Stats {
        total,
        recent_files: u64::try_from(recent_files).context("recent file total is negative")?,
        frequent_folders: u64::try_from(frequent_folders)
            .context("frequent folder total is negative")?,
        daily_trend,
        weekly_trend,
        rule_hits: rule_hits(connection)?,
    })
}

fn trend(
    connection: &Connection,
    range: StatsRange,
    weekly: bool,
    label: &str,
) -> Result<Vec<StatsTrendPoint>> {
    let mut statement = connection
        .prepare(
            "WITH RECURSIVE
             bounds(start_day, end_day) AS (
                 SELECT
                     COALESCE(
                         CASE
                             WHEN ?1 IS NULL THEN (
                                 SELECT MIN(date(cleaned_at, 'localtime')) FROM clean_records
                             )
                             ELSE date('now', 'localtime', ?1)
                         END,
                         date('now', 'localtime')
                     ),
                     date('now', 'localtime')
             ),
             days(day) AS (
                 SELECT start_day FROM bounds
                 UNION ALL
                 SELECT date(day, '+1 day')
                 FROM days, bounds
                 WHERE day < end_day
             ),
             record_days(day, count) AS (
                 SELECT date(cleaned_at, 'localtime'), COUNT(*)
                 FROM clean_records
                 GROUP BY date(cleaned_at, 'localtime')
             )
             SELECT
                 CASE
                     WHEN ?2 = 1 THEN date(days.day, 'weekday 0', '-6 days')
                     ELSE days.day
                 END AS period,
                 COALESCE(SUM(record_days.count), 0)
             FROM days
             LEFT JOIN record_days ON record_days.day = days.day
             GROUP BY period
             ORDER BY period",
        )
        .with_context(|| format!("failed to prepare {label}"))?;
    let rows = statement
        .query_map(params![range.date_modifier(), weekly], |row| {
            let count = row.get::<_, i64>(1)?;
            Ok((row.get::<_, String>(0)?, count))
        })
        .with_context(|| format!("failed to query {label}"))?;
    rows.map(|row| {
        let (period, count) = row.with_context(|| format!("failed to read {label}"))?;
        Ok(StatsTrendPoint {
            period,
            count: u64::try_from(count).context("cleanup trend count is negative")?,
        })
    })
    .collect()
}

fn rule_hits(connection: &Connection) -> Result<Vec<RuleHitStat>> {
    let mut statement = connection
        .prepare(
            "SELECT rule_keyword, COUNT(*) AS hit_count
             FROM clean_records
             WHERE rule_keyword IS NOT NULL
             GROUP BY rule_keyword COLLATE NOCASE
             ORDER BY hit_count DESC, rule_keyword COLLATE NOCASE
             LIMIT 10",
        )
        .context("failed to prepare rule hit statistics")?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .context("failed to query rule hit statistics")?;
    rows.map(|row| {
        let (keyword, count) = row.context("failed to read rule hit statistics")?;
        Ok(RuleHitStat {
            keyword,
            count: u64::try_from(count).context("rule hit count is negative")?,
        })
    })
    .collect()
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
                ..history_query()
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
                "item_path",
                "item_type",
                "rule_id",
                "rule_keyword",
                "source",
                "cleaned_at"
            ]
        );
        let rows = reader.records().collect::<csv::Result<Vec<_>>>().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(&rows[0][0], special_path);
        assert_eq!(&rows[0][3], special_keyword);
        assert_eq!(&rows[0][4], "auto");
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
                "source"
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

        clear(&connection).unwrap();

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
