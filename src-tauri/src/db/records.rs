use anyhow::{Context, Result};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCleanRecord {
    pub item_path: String,
    pub item_type: String,
    pub rule_id: Option<i64>,
    pub rule_keyword: Option<String>,
}

pub fn insert_batch(connection: &mut Connection, records: &[NewCleanRecord]) -> Result<()> {
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
    transaction
        .commit()
        .context("failed to commit clean records")
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

        insert_batch(&mut connection, &records).unwrap();

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

        assert!(insert_batch(&mut connection, &records).is_err());
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
