use crate::rules::{NewRule, Rule, RuleType};
use anyhow::{bail, Context, Result};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    Connection, OptionalExtension, Row,
};

impl FromSql for RuleType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "whitelist" => Ok(Self::Whitelist),
            "blacklist" => Ok(Self::Blacklist),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

pub fn list(connection: &Connection) -> Result<Vec<Rule>> {
    let mut statement = connection
        .prepare("SELECT id, keyword, rule_type, enabled, created_at FROM rules ORDER BY id ASC")
        .context("failed to prepare rule list query")?;
    let rows = statement
        .query_map([], row_to_rule)
        .context("failed to query rules")?;
    let rules = rows
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read rules")?;
    Ok(rules)
}

pub fn add(connection: &Connection, rule: NewRule) -> Result<Rule> {
    let rule = normalize(rule)?;
    connection
        .execute(
            "INSERT INTO rules (keyword, rule_type, enabled) VALUES (?1, ?2, ?3)",
            params![rule.keyword, rule.rule_type.as_str(), rule.enabled],
        )
        .context("failed to add rule")?;
    find(connection, connection.last_insert_rowid())?.context("added rule was not found")
}

pub fn update(connection: &Connection, id: i64, rule: NewRule) -> Result<Rule> {
    let rule = normalize(rule)?;
    let changed = connection
        .execute(
            "UPDATE rules SET keyword = ?1, rule_type = ?2, enabled = ?3 WHERE id = ?4",
            params![rule.keyword, rule.rule_type.as_str(), rule.enabled, id],
        )
        .with_context(|| format!("failed to update rule {id}"))?;
    ensure_rule_changed(id, changed)?;
    find(connection, id)?.with_context(|| format!("updated rule {id} was not found"))
}

pub fn remove(connection: &Connection, id: i64) -> Result<()> {
    let changed = connection
        .execute("DELETE FROM rules WHERE id = ?1", [id])
        .with_context(|| format!("failed to remove rule {id}"))?;
    ensure_rule_changed(id, changed)
}

pub fn toggle(connection: &Connection, id: i64, enabled: bool) -> Result<Rule> {
    let changed = connection
        .execute(
            "UPDATE rules SET enabled = ?1 WHERE id = ?2",
            params![enabled, id],
        )
        .with_context(|| format!("failed to toggle rule {id}"))?;
    ensure_rule_changed(id, changed)?;
    find(connection, id)?.with_context(|| format!("toggled rule {id} was not found"))
}

fn find(connection: &Connection, id: i64) -> Result<Option<Rule>> {
    connection
        .query_row(
            "SELECT id, keyword, rule_type, enabled, created_at FROM rules WHERE id = ?1",
            [id],
            row_to_rule,
        )
        .optional()
        .with_context(|| format!("failed to query rule {id}"))
}

fn row_to_rule(row: &Row<'_>) -> rusqlite::Result<Rule> {
    Ok(Rule {
        id: row.get(0)?,
        keyword: row.get(1)?,
        rule_type: row.get(2)?,
        enabled: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn normalize(mut rule: NewRule) -> Result<NewRule> {
    rule.keyword = rule.keyword.trim().to_string();
    if rule.keyword.is_empty() {
        bail!("rule keyword cannot be empty");
    }
    Ok(rule)
}

fn ensure_rule_changed(id: i64, changed: usize) -> Result<()> {
    if changed == 0 {
        bail!("rule {id} not found");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_normalized_rule_and_lists_by_id() {
        let connection = test_connection();
        connection.execute("DELETE FROM rules", []).unwrap();

        let first = add(&connection, new_rule("  Temp  ", RuleType::Blacklist, true)).unwrap();
        let second = add(
            &connection,
            new_rule("Projects", RuleType::Whitelist, false),
        )
        .unwrap();

        assert_eq!(first.keyword, "Temp");
        assert_eq!(first.rule_type, RuleType::Blacklist);
        assert!(first.enabled);
        assert!(!first.created_at.is_empty());
        assert_eq!(
            list(&connection)
                .unwrap()
                .into_iter()
                .map(|rule| rule.id)
                .collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
    }

    #[test]
    fn updates_and_toggles_rule() {
        let connection = test_connection();
        let id = add(&connection, new_rule("Temp", RuleType::Blacklist, true))
            .unwrap()
            .id;

        let updated = update(
            &connection,
            id,
            new_rule("Documents", RuleType::Whitelist, false),
        )
        .unwrap();
        assert_eq!(updated.keyword, "Documents");
        assert_eq!(updated.rule_type, RuleType::Whitelist);
        assert!(!updated.enabled);

        let toggled = toggle(&connection, id, true).unwrap();
        assert!(toggled.enabled);
    }

    #[test]
    fn removes_rule() {
        let connection = test_connection();
        let id = add(&connection, new_rule("Temp", RuleType::Blacklist, true))
            .unwrap()
            .id;

        remove(&connection, id).unwrap();

        assert!(find(&connection, id).unwrap().is_none());
    }

    #[test]
    fn rejects_empty_keyword() {
        let connection = test_connection();

        let error = add(&connection, new_rule("   ", RuleType::Whitelist, true))
            .unwrap_err()
            .to_string();

        assert!(error.contains("keyword cannot be empty"));
    }

    #[test]
    fn reports_missing_rule_for_mutations() {
        let connection = test_connection();

        assert!(update(
            &connection,
            i64::MAX,
            new_rule("Temp", RuleType::Blacklist, true)
        )
        .unwrap_err()
        .to_string()
        .contains("not found"));
        assert!(toggle(&connection, i64::MAX, false)
            .unwrap_err()
            .to_string()
            .contains("not found"));
        assert!(remove(&connection, i64::MAX)
            .unwrap_err()
            .to_string()
            .contains("not found"));
    }

    fn new_rule(keyword: &str, rule_type: RuleType, enabled: bool) -> NewRule {
        NewRule {
            keyword: keyword.to_string(),
            rule_type,
            enabled,
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
}
