use crate::rules::{NewRule, Rule, RuleScope, RuleType};
use anyhow::{Context, Result};
use rusqlite::{
    params,
    types::{FromSql, FromSqlError, FromSqlResult, ValueRef},
    Connection, OptionalExtension, Row,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum RuleError {
    #[error("rule keyword cannot be empty")]
    EmptyKeyword,
    #[error("rule {0} not found")]
    NotFound(i64),
}

impl FromSql for RuleType {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "whitelist" => Ok(Self::Whitelist),
            "blacklist" => Ok(Self::Blacklist),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

impl FromSql for RuleScope {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "all" => Ok(Self::All),
            "files" => Ok(Self::Files),
            "folders" => Ok(Self::Folders),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

pub fn list(connection: &Connection) -> Result<Vec<Rule>> {
    let mut statement = connection
        .prepare(
            "SELECT id, keyword, rule_type, scope, enabled, created_at FROM rules ORDER BY id ASC",
        )
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
            "INSERT INTO rules (keyword, rule_type, scope, enabled) VALUES (?1, ?2, ?3, ?4)",
            params![
                rule.keyword,
                rule.rule_type.as_str(),
                rule.scope.as_str(),
                rule.enabled
            ],
        )
        .context("failed to add rule")?;
    find(connection, connection.last_insert_rowid())?.context("added rule was not found")
}

pub fn update(connection: &Connection, id: i64, rule: NewRule) -> Result<Rule> {
    let rule = normalize(rule)?;
    let changed = connection
        .execute(
            "UPDATE rules SET keyword = ?1, rule_type = ?2, scope = ?3, enabled = ?4 WHERE id = ?5",
            params![
                rule.keyword,
                rule.rule_type.as_str(),
                rule.scope.as_str(),
                rule.enabled,
                id
            ],
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

pub fn clear(connection: &mut Connection, ids: Option<&[i64]>) -> Result<usize> {
    let Some(ids) = ids else {
        return connection
            .execute("DELETE FROM rules", [])
            .context("failed to clear rules");
    };
    let transaction = connection.transaction()?;
    let mut affected = 0;
    {
        let mut statement = transaction.prepare("DELETE FROM rules WHERE id = ?1")?;
        for id in ids {
            affected += statement
                .execute([id])
                .with_context(|| format!("failed to clear rule {id}"))?;
        }
    }
    transaction.commit()?;
    Ok(affected)
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
            "SELECT id, keyword, rule_type, scope, enabled, created_at FROM rules WHERE id = ?1",
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
        scope: row.get(3)?,
        enabled: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn normalize(mut rule: NewRule) -> Result<NewRule> {
    rule.keyword = rule.keyword.trim().to_string();
    if rule.keyword.is_empty() {
        return Err(RuleError::EmptyKeyword.into());
    }
    Ok(rule)
}

fn ensure_rule_changed(id: i64, changed: usize) -> Result<()> {
    if changed == 0 {
        return Err(RuleError::NotFound(id).into());
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

        let first = add(
            &connection,
            new_rule("  Temp  ", RuleType::Blacklist, RuleScope::Files, true),
        )
        .unwrap();
        let second = add(
            &connection,
            new_rule("Projects", RuleType::Whitelist, RuleScope::Folders, false),
        )
        .unwrap();

        assert_eq!(first.keyword, "Temp");
        assert_eq!(first.rule_type, RuleType::Blacklist);
        assert_eq!(first.scope, RuleScope::Files);
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
        let id = add(
            &connection,
            new_rule("Temp", RuleType::Blacklist, RuleScope::All, true),
        )
        .unwrap()
        .id;

        let updated = update(
            &connection,
            id,
            new_rule("Documents", RuleType::Whitelist, RuleScope::Folders, false),
        )
        .unwrap();
        assert_eq!(updated.keyword, "Documents");
        assert_eq!(updated.rule_type, RuleType::Whitelist);
        assert_eq!(updated.scope, RuleScope::Folders);
        assert!(!updated.enabled);

        let toggled = toggle(&connection, id, true).unwrap();
        assert!(toggled.enabled);
    }

    #[test]
    fn removes_rule() {
        let connection = test_connection();
        let id = add(
            &connection,
            new_rule("Temp", RuleType::Blacklist, RuleScope::All, true),
        )
        .unwrap()
        .id;

        remove(&connection, id).unwrap();

        assert!(find(&connection, id).unwrap().is_none());
    }

    #[test]
    fn clears_all_rules() {
        let mut connection = test_connection();
        connection.execute("DELETE FROM rules", []).unwrap();
        add(
            &connection,
            new_rule("Temp", RuleType::Blacklist, RuleScope::All, true),
        )
        .unwrap();
        add(
            &connection,
            new_rule("Projects", RuleType::Whitelist, RuleScope::Folders, false),
        )
        .unwrap();

        assert_eq!(clear(&mut connection, None).unwrap(), 2);
        assert!(list(&connection).unwrap().is_empty());
        assert_eq!(clear(&mut connection, None).unwrap(), 0);
    }

    #[test]
    fn clears_only_selected_rules() {
        let mut connection = test_connection();
        connection.execute("DELETE FROM rules", []).unwrap();
        let first = add(
            &connection,
            new_rule("Temp", RuleType::Blacklist, RuleScope::All, true),
        )
        .unwrap();
        let second = add(
            &connection,
            new_rule("Projects", RuleType::Whitelist, RuleScope::Folders, false),
        )
        .unwrap();

        assert_eq!(clear(&mut connection, Some(&[first.id])).unwrap(), 1);
        let remaining = list(&connection).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, second.id);
    }

    #[test]
    fn rejects_empty_keyword() {
        let connection = test_connection();

        let error = add(
            &connection,
            new_rule("   ", RuleType::Whitelist, RuleScope::All, true),
        )
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
            new_rule("Temp", RuleType::Blacklist, RuleScope::All, true)
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

    fn new_rule(keyword: &str, rule_type: RuleType, scope: RuleScope, enabled: bool) -> NewRule {
        NewRule {
            keyword: keyword.to_string(),
            rule_type,
            scope,
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
