use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, Runtime};

pub(crate) mod records;
pub(crate) mod rules;

const DATABASE_FILE: &str = "scourgify.db";
const SCHEMA_VERSION: u32 = 1;
const BUILTIN_WHITELIST_RULES: &[&str] = &["Desktop", "Documents"];

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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DatabaseStatus {
    pub available: bool,
    pub path: Option<String>,
    pub schema_version: Option<u32>,
    pub error: Option<String>,
}

struct DbInner {
    connection: Option<Connection>,
    schema_version: Option<u32>,
    error: Option<String>,
}

pub struct DbState {
    path: Option<PathBuf>,
    inner: Mutex<DbInner>,
}

impl DbState {
    pub fn status(&self) -> DatabaseStatus {
        let path = self
            .path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned());

        match self.inner.lock() {
            Ok(inner) => DatabaseStatus {
                available: inner.connection.is_some(),
                path,
                schema_version: inner.schema_version,
                error: inner.error.clone(),
            },
            Err(error) => DatabaseStatus {
                available: false,
                path,
                schema_version: None,
                error: Some(format!("database state lock poisoned: {error}")),
            },
        }
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T>,
    ) -> Result<T> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|error| anyhow::anyhow!("database state lock poisoned: {error}"))?;
        if inner.connection.is_none() {
            let detail = inner
                .error
                .as_deref()
                .unwrap_or("unknown initialization error");
            bail!("database unavailable: {detail}");
        }
        operation(
            inner
                .connection
                .as_mut()
                .context("database connection is unavailable")?,
        )
    }

    fn available(path: PathBuf, connection: Connection, schema_version: u32) -> Self {
        Self {
            path: Some(path),
            inner: Mutex::new(DbInner {
                connection: Some(connection),
                schema_version: Some(schema_version),
                error: None,
            }),
        }
    }

    fn unavailable(path: Option<PathBuf>, error: impl Into<String>) -> Self {
        Self {
            path,
            inner: Mutex::new(DbInner {
                connection: None,
                schema_version: None,
                error: Some(error.into()),
            }),
        }
    }
}

pub fn initialize<R: Runtime>(app: &AppHandle<R>) -> DbState {
    let path = match app.path().app_config_dir() {
        Ok(directory) => directory.join(DATABASE_FILE),
        Err(error) => {
            let error = format!("failed to resolve app config directory: {error}");
            log::error!("database unavailable: {error}");
            return DbState::unavailable(None, error);
        }
    };

    initialize_path(path)
}

fn initialize_path(path: PathBuf) -> DbState {
    match open_database(&path) {
        Ok((connection, schema_version)) => {
            log::info!(
                "database initialized path={} schema_version={schema_version}",
                path.display()
            );
            DbState::available(path, connection, schema_version)
        }
        Err(error) => {
            let error = format!("{error:#}");
            log::error!("database unavailable path={} error={error}", path.display());
            DbState::unavailable(Some(path), error)
        }
    }
}

fn open_database(path: &Path) -> Result<(Connection, u32)> {
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
    }

    let mut connection =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .context("failed to enable SQLite foreign keys")?;
    let schema_version = migrate(&mut connection)?;
    Ok((connection, schema_version))
}

fn migrate(connection: &mut Connection) -> Result<u32> {
    let current_version = connection
        .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
        .context("failed to read SQLite user_version")?;

    if current_version > SCHEMA_VERSION {
        bail!(
            "database schema version {current_version} is newer than supported version {SCHEMA_VERSION}"
        );
    }

    if current_version == 0 {
        migrate_to_v1(connection)?;
    }

    Ok(SCHEMA_VERSION)
}

fn migrate_to_v1(connection: &mut Connection) -> Result<()> {
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
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .context("failed to set SQLite user_version")?;
    transaction
        .commit()
        .context("failed to commit schema v1 migration")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn migration_creates_schema_and_seeds_builtin_rules() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();

        assert_eq!(migrate(&mut connection).unwrap(), SCHEMA_VERSION);
        assert_eq!(user_version(&connection), SCHEMA_VERSION);
        assert_eq!(table_count(&connection, "rules"), 1);
        assert_eq!(table_count(&connection, "clean_records"), 1);

        let keywords = connection
            .prepare("SELECT keyword FROM rules ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(keywords, BUILTIN_WHITELIST_RULES);
    }

    #[test]
    fn migration_does_not_reseed_deleted_builtin_rules() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrate(&mut connection).unwrap();
        connection.execute("DELETE FROM rules", []).unwrap();

        migrate(&mut connection).unwrap();

        assert_eq!(rule_count(&connection), 0);
    }

    #[test]
    fn deleting_rule_preserves_history_keyword_snapshot() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        migrate(&mut connection).unwrap();
        let rule_id = connection
            .query_row("SELECT id FROM rules LIMIT 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap();
        connection
            .execute(
                "INSERT INTO clean_records (item_path, item_type, rule_id, rule_keyword) VALUES (?1, 'recent_file', ?2, ?3)",
                (r"C:\Users\test\report.txt", rule_id, "Desktop"),
            )
            .unwrap();

        connection
            .execute("DELETE FROM rules WHERE id = ?1", [rule_id])
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
    fn migration_rejects_newer_schema() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();

        let error = migrate(&mut connection).unwrap_err().to_string();

        assert!(error.contains("newer than supported"));
    }

    #[test]
    fn initialization_creates_database_file() {
        let directory = unique_temp_path("database");
        let path = directory.join(DATABASE_FILE);

        let state = initialize_path(path.clone());
        let status = state.status();

        assert!(status.available);
        assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
        assert!(path.exists());
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn initialization_degrades_when_parent_is_not_a_directory() {
        let blocker = unique_temp_path("blocker");
        std::fs::write(&blocker, b"not a directory").unwrap();

        let state = initialize_path(blocker.join(DATABASE_FILE));
        let status = state.status();

        assert!(!status.available);
        assert_eq!(status.schema_version, None);
        assert!(status.error.is_some());
        std::fs::remove_file(blocker).unwrap();
    }

    fn user_version(connection: &Connection) -> u32 {
        connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap()
    }

    fn table_count(connection: &Connection, name: &str) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn rule_count(connection: &Connection) -> i64 {
        connection
            .query_row("SELECT COUNT(*) FROM rules", [], |row| row.get(0))
            .unwrap()
    }

    fn unique_temp_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("scourgify-{label}-{}-{nonce}", std::process::id()))
    }
}
