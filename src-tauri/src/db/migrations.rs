use anyhow::{bail, Context, Result};
use rusqlite::Connection;

pub(super) const SCHEMA_VERSION: u32 = 3;
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

const SCHEMA_V3: &str = r#"
CREATE TABLE cleanup_runs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    action              TEXT NOT NULL CHECK(action IN ('remove_selected', 'empty', 'smart_clean', 'auto_clean')),
    trigger             TEXT NOT NULL CHECK(trigger IN ('manual', 'monitor', 'scheduled')),
    qa_type             TEXT NOT NULL CHECK(qa_type IN ('recent', 'frequent', 'all')),
    status              TEXT NOT NULL DEFAULT 'running'
                        CHECK(status IN ('running', 'success', 'partial', 'failed', 'noop', 'interrupted')),
    requested_count     INTEGER NOT NULL DEFAULT 0 CHECK(requested_count >= 0),
    succeeded_count     INTEGER NOT NULL DEFAULT 0 CHECK(succeeded_count >= 0),
    failed_count        INTEGER NOT NULL DEFAULT 0 CHECK(failed_count >= 0),
    protected_count     INTEGER NOT NULL DEFAULT 0 CHECK(protected_count >= 0),
    warning_count       INTEGER NOT NULL DEFAULT 0 CHECK(warning_count >= 0),
    history_error_count INTEGER NOT NULL DEFAULT 0 CHECK(history_error_count >= 0),
    section_error_count INTEGER NOT NULL DEFAULT 0 CHECK(section_error_count >= 0),
    incident_id         TEXT,
    started_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%f', 'now')),
    completed_at        TEXT
);

ALTER TABLE clean_records ADD COLUMN run_id INTEGER
    REFERENCES cleanup_runs(id) ON DELETE SET NULL;

CREATE TABLE cleanup_totals (
    id                INTEGER PRIMARY KEY CHECK(id = 1),
    cleaned_total     INTEGER NOT NULL DEFAULT 0 CHECK(cleaned_total >= 0),
    cleaned_files     INTEGER NOT NULL DEFAULT 0 CHECK(cleaned_files >= 0),
    cleaned_folders   INTEGER NOT NULL DEFAULT 0 CHECK(cleaned_folders >= 0),
    cleanup_runs_total INTEGER NOT NULL DEFAULT 0 CHECK(cleanup_runs_total >= 0),
    successful_runs   INTEGER NOT NULL DEFAULT 0 CHECK(successful_runs >= 0),
    partial_runs      INTEGER NOT NULL DEFAULT 0 CHECK(partial_runs >= 0),
    failed_runs       INTEGER NOT NULL DEFAULT 0 CHECK(failed_runs >= 0),
    noop_runs         INTEGER NOT NULL DEFAULT 0 CHECK(noop_runs >= 0),
    interrupted_runs  INTEGER NOT NULL DEFAULT 0 CHECK(interrupted_runs >= 0),
    tracking_started_at TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);

INSERT INTO cleanup_totals (
    id, cleaned_total, cleaned_files, cleaned_folders,
    tracking_started_at, updated_at
)
SELECT
    1,
    COUNT(*),
    COALESCE(SUM(item_type = 'recent_file'), 0),
    COALESCE(SUM(item_type = 'frequent_folder'), 0),
    strftime('%Y-%m-%d %H:%M:%f', 'now'),
    strftime('%Y-%m-%d %H:%M:%f', 'now')
FROM clean_records;

DROP INDEX idx_records_date;
CREATE INDEX idx_records_date_id ON clean_records(cleaned_at DESC, id DESC);
CREATE INDEX idx_records_run_id ON clean_records(run_id);
CREATE INDEX idx_cleanup_runs_date_id ON cleanup_runs(started_at DESC, id DESC);
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
        version = 2;
    }
    if version == 2 {
        migrate_to_v3(connection)?;
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

fn migrate_to_v3(connection: &mut Connection) -> Result<()> {
    let transaction = connection
        .transaction()
        .context("failed to start schema v3 migration")?;
    transaction
        .execute_batch(SCHEMA_V3)
        .context("failed to migrate schema to v3")?;
    transaction
        .pragma_update(None, "user_version", 3)
        .context("failed to set SQLite user_version to 3")?;
    transaction
        .commit()
        .context("failed to commit schema v3 migration")
}
