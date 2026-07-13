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
