use anyhow::{bail, Context, Result};
use rusqlite::Connection;

pub(super) const SCHEMA_VERSION: u32 = 2;
pub(super) const BUILTIN_WHITELIST_RULES: &[&str] = &["Desktop", "Documents"];

const SCHEMA_V1: &str = r#"
CREATE TABLE rules (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    keyword    TEXT NOT NULL CHECK(length(trim(keyword)) > 0),
    rule_type  TEXT NOT NULL CHECK(rule_type IN ('whitelist', 'blacklist')),
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE clean_records (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    item_path    TEXT NOT NULL,
    item_type    TEXT NOT NULL CHECK(item_type IN ('recent_file', 'frequent_folder')),
    rule_id      INTEGER,
    rule_keyword TEXT,
    cleaned_at   TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (rule_id) REFERENCES rules(id) ON DELETE SET NULL
);

CREATE INDEX idx_rules_type ON rules(rule_type);
CREATE INDEX idx_records_date ON clean_records(cleaned_at);
"#;

const SCHEMA_V2: &str = r#"
ALTER TABLE clean_records ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'
    CHECK(source IN ('manual', 'auto'));
"#;

pub(super) fn migrate(connection: &mut Connection) -> Result<u32> {
    let current_version = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
        .context("failed to read SQLite user_version")?;

    if current_version > SCHEMA_VERSION {
        bail!(
            "database schema version {current_version} is newer than supported version {SCHEMA_VERSION}"
        );
    }

    let mut version = current_version;
    if version == 0 {
        migrate_to_v1(connection)?;
        version = 1;
    }
    if version == 1 {
        migrate_to_v2(connection)?;
    }
    Ok(SCHEMA_VERSION)
}

pub(super) fn migrate_to_v1(connection: &mut Connection) -> Result<()> {
    let transaction = connection
        .transaction()
        .context("failed to start schema v1 migration")?;
    transaction
        .execute_batch(SCHEMA_V1)
        .context("failed to create schema v1")?;

    {
        let mut statement = transaction
            .prepare("INSERT INTO rules (keyword, rule_type) VALUES (?1, 'whitelist')")
            .context("failed to prepare built-in rule seed")?;
        for keyword in BUILTIN_WHITELIST_RULES {
            statement
                .execute([keyword])
                .with_context(|| format!("failed to seed built-in rule {keyword}"))?;
        }
    }

    transaction
        .pragma_update(None, "user_version", 1)
        .context("failed to set SQLite user_version")?;
    transaction
        .commit()
        .context("failed to commit schema v1 migration")
}

fn migrate_to_v2(connection: &mut Connection) -> Result<()> {
    let transaction = connection
        .transaction()
        .context("failed to start schema v2 migration")?;
    transaction
        .execute_batch(SCHEMA_V2)
        .context("failed to migrate schema to v2")?;
    transaction
        .pragma_update(None, "user_version", 2)
        .context("failed to set SQLite user_version")?;
    transaction
        .commit()
        .context("failed to commit schema v2 migration")
}
