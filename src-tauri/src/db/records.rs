use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCleanRecord {
    pub item_path: String,
    pub item_type: String,
    pub rule_id: Option<i64>,
    pub rule_keyword: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct HistoryQuery {
    pub page: u32,
    pub page_size: u32,
    pub sort_by: String,
    pub sort_order: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CleanRecord {
    pub id: i64,
    pub item_path: String,
    pub item_type: String,
    pub rule_id: Option<i64>,
    pub rule_keyword: Option<String>,
    pub cleaned_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CleanRecordPage {
    pub records: Vec<CleanRecord>,
    pub total: u64,
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
                "INSERT INTO clean_records (item_path, item_type, rule_id, rule_keyword)
                 VALUES (?1, ?2, (SELECT id FROM rules WHERE id = ?3), ?4)",
            )
            .context("failed to prepare clean record insert")?;
        for record in records {
            statement
                .execute(params![
                    record.item_path,
                    record.item_type,
                    record.rule_id,
                    record.rule_keyword
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
    let total = connection
        .query_row("SELECT COUNT(*) FROM clean_records", [], |row| {
            row.get::<_, i64>(0)
        })
        .context("failed to count clean records")?;
    let total = u64::try_from(total).context("clean record count is negative")?;
    let offset = i64::try_from(u64::from(query.page - 1) * u64::from(query.page_size))
        .context("history page offset is too large")?;
    let sql = format!(
        "SELECT id, item_path, item_type, rule_id, rule_keyword, cleaned_at
         FROM clean_records
         ORDER BY {sort_column} {sort_order}, id {sort_order}
         LIMIT ?1 OFFSET ?2"
    );
    let mut statement = connection
        .prepare(&sql)
        .context("failed to prepare clean record query")?;
    let records = statement
        .query_map(params![i64::from(query.page_size), offset], |row| {
            Ok(CleanRecord {
                id: row.get(0)?,
                item_path: row.get(1)?,
                item_type: row.get(2)?,
                rule_id: row.get(3)?,
                rule_keyword: row.get(4)?,
                cleaned_at: row.get(5)?,
            })
        })
        .context("failed to query clean records")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read clean records")?;

    Ok(CleanRecordPage {
        records,
        total,
        page: query.page,
        page_size: query.page_size,
    })
}

pub fn clear(connection: &Connection) -> Result<()> {
    connection
        .execute("DELETE FROM clean_records", [])
        .context("failed to clear clean records")?;
    Ok(())
}

pub fn stats(connection: &Connection) -> Result<Stats> {
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

    Ok(Stats {
        total: u64::try_from(total).context("cleanup total is negative")?,
        recent_files: u64::try_from(recent_files).context("recent file total is negative")?,
        frequent_folders: u64::try_from(frequent_folders)
            .context("frequent folder total is negative")?,
        daily_trend: trend(
            connection,
            "date(cleaned_at, 'localtime')",
            "daily cleanup trend",
        )?,
        weekly_trend: trend(
            connection,
            "date(cleaned_at, 'localtime', 'weekday 0', '-6 days')",
            "weekly cleanup trend",
        )?,
        rule_hits: rule_hits(connection)?,
    })
}

fn trend(connection: &Connection, period_sql: &str, label: &str) -> Result<Vec<StatsTrendPoint>> {
    let sql = format!(
        "SELECT {period_sql} AS period, COUNT(*)
         FROM clean_records
         GROUP BY period
         ORDER BY period"
    );
    let mut statement = connection
        .prepare(&sql)
        .with_context(|| format!("failed to prepare {label}"))?;
    let rows = statement
        .query_map([], |row| {
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
            }
        )
        .is_err());
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

        let stats = stats(&connection).unwrap();

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
        }
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
