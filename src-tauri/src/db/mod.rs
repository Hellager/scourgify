use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, Runtime};
use thiserror::Error;

pub(crate) mod history;
mod history_export;
pub(crate) mod history_runs;
mod migrations;
pub(crate) mod rules;
mod stats;

use migrations::migrate;
#[cfg(test)]
use migrations::{migrate_to_v1, BUILTIN_WHITELIST_RULES, SCHEMA_VERSION};

const DATABASE_FILE: &str = "scourgify.db";
const DATABASE_PATH_UNAVAILABLE: &str = "Database path is unavailable.";
const DATABASE_STATE_UNAVAILABLE: &str = "Database state could not be accessed.";

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum DatabaseStateError {
    #[error("database unavailable: {0}")]
    Unavailable(String),
    #[error("database state lock poisoned: {0}")]
    StateUnavailable(String),
}

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

#[cfg(debug_assertions)]
struct MockDb {
    inner: DbInner,
    path: tempfile::TempPath,
}

pub struct DbState {
    path: Option<PathBuf>,
    inner: Mutex<DbInner>,
    #[cfg(debug_assertions)]
    mock_enabled: AtomicBool,
    #[cfg(debug_assertions)]
    mock: Mutex<Option<MockDb>>,
}

impl DbState {
    pub fn status(&self) -> DatabaseStatus {
        #[cfg(debug_assertions)]
        if self.mock_enabled.load(Ordering::Relaxed) {
            return match self.mock.lock() {
                Ok(mock) => mock.as_ref().map_or_else(
                    || unavailable_status(None, DATABASE_STATE_UNAVAILABLE),
                    |mock| database_status(Some(mock.path.as_ref()), &mock.inner),
                ),
                Err(error) => {
                    log::error!("mock database state lock poisoned: {error}");
                    unavailable_status(None, DATABASE_STATE_UNAVAILABLE)
                }
            };
        }

        match self.inner.lock() {
            Ok(inner) => database_status(self.path.as_deref(), &inner),
            Err(error) => {
                log::error!("database state lock poisoned: {error}");
                unavailable_status(self.path.as_deref(), DATABASE_STATE_UNAVAILABLE)
            }
        }
    }

    pub fn retry(&self) -> DatabaseStatus {
        #[cfg(debug_assertions)]
        if self.mock_enabled.load(Ordering::Relaxed) {
            return self.status();
        }

        let mut inner = match self.inner.lock() {
            Ok(inner) => inner,
            Err(error) => {
                log::error!("database retry failed because state lock is poisoned: {error}");
                return unavailable_status(self.path.as_deref(), DATABASE_STATE_UNAVAILABLE);
            }
        };
        if inner.connection.is_some() {
            return database_status(self.path.as_deref(), &inner);
        }
        let Some(path) = self.path.as_deref() else {
            inner.error = Some(DATABASE_PATH_UNAVAILABLE.to_string());
            return database_status(None, &inner);
        };

        log::info!("database retry started path={}", path.display());
        match open_database(path) {
            Ok((connection, schema_version)) => {
                inner.connection = Some(connection);
                inner.schema_version = Some(schema_version);
                inner.error = None;
                log::info!(
                    "database retry succeeded path={} schema_version={schema_version}",
                    path.display()
                );
            }
            Err(error) => {
                log::error!(
                    "database retry failed path={} error={error:#}",
                    path.display()
                );
                inner.connection = None;
                inner.schema_version = None;
                inner.error = Some(error_summary(&error));
            }
        }

        database_status(Some(path), &inner)
    }

    pub fn directory(&self) -> Option<PathBuf> {
        #[cfg(debug_assertions)]
        if self.mock_enabled.load(Ordering::Relaxed) {
            return self
                .mock
                .lock()
                .ok()?
                .as_ref()?
                .path
                .parent()
                .map(Path::to_path_buf);
        }
        self.path.as_ref()?.parent().map(Path::to_path_buf)
    }

    pub(crate) fn with_connection<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T>,
    ) -> Result<T> {
        #[cfg(debug_assertions)]
        if self.mock_enabled.load(Ordering::Relaxed) {
            let mut mock = self
                .mock
                .lock()
                .map_err(|error| DatabaseStateError::StateUnavailable(error.to_string()))?;
            let mock = mock
                .as_mut()
                .context("mock database state is unavailable")?;
            return operation(
                mock.inner
                    .connection
                    .as_mut()
                    .context("mock database connection is unavailable")?,
            );
        }

        let mut inner = self
            .inner
            .lock()
            .map_err(|error| DatabaseStateError::StateUnavailable(error.to_string()))?;
        if inner.connection.is_none() {
            let detail = inner
                .error
                .as_deref()
                .unwrap_or("unknown initialization error");
            return Err(DatabaseStateError::Unavailable(detail.to_string()).into());
        }
        operation(
            inner
                .connection
                .as_mut()
                .context("database connection is unavailable")?,
        )
    }

    pub(crate) fn read_connection(&self) -> Result<Connection> {
        #[cfg(debug_assertions)]
        if self.mock_enabled.load(Ordering::Relaxed) {
            let path = {
                let mock = self
                    .mock
                    .lock()
                    .map_err(|error| DatabaseStateError::StateUnavailable(error.to_string()))?;
                mock.as_ref()
                    .context("mock database state is unavailable")?
                    .path
                    .to_path_buf()
            };
            return open_read_connection(&path);
        }

        {
            let inner = self
                .inner
                .lock()
                .map_err(|error| DatabaseStateError::StateUnavailable(error.to_string()))?;
            if inner.connection.is_none() {
                return Err(DatabaseStateError::Unavailable(
                    inner
                        .error
                        .clone()
                        .unwrap_or_else(|| "unknown initialization error".to_string()),
                )
                .into());
            }
        }
        let path = self
            .path
            .as_deref()
            .context("database path is unavailable")?;
        open_read_connection(path)
    }

    fn available(path: PathBuf, connection: Connection, schema_version: u32) -> Self {
        Self {
            path: Some(path),
            inner: Mutex::new(DbInner {
                connection: Some(connection),
                schema_version: Some(schema_version),
                error: None,
            }),
            #[cfg(debug_assertions)]
            mock_enabled: AtomicBool::new(false),
            #[cfg(debug_assertions)]
            mock: Mutex::new(None),
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
            #[cfg(debug_assertions)]
            mock_enabled: AtomicBool::new(false),
            #[cfg(debug_assertions)]
            mock: Mutex::new(None),
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn set_mock_mode(&self, enabled: bool) -> Result<()> {
        if enabled {
            let path = tempfile::NamedTempFile::new()
                .context("failed to create mock database file")?
                .into_temp_path();
            let (connection, schema_version) = open_database(path.as_ref())?;
            *self
                .mock
                .lock()
                .map_err(|error| DatabaseStateError::StateUnavailable(error.to_string()))? =
                Some(MockDb {
                    inner: DbInner {
                        connection: Some(connection),
                        schema_version: Some(schema_version),
                        error: None,
                    },
                    path,
                });
        } else {
            self.mock_enabled.store(false, Ordering::Relaxed);
            *self
                .mock
                .lock()
                .map_err(|error| DatabaseStateError::StateUnavailable(error.to_string()))? = None;
        }
        if enabled {
            self.mock_enabled.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}

fn open_read_connection(path: &Path) -> Result<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open read-only database at {}", path.display()))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .context("failed to configure read-only database timeout")?;
    Ok(connection)
}

pub fn initialize<R: Runtime>(app: &AppHandle<R>) -> DbState {
    let path = match app.path().app_config_dir() {
        Ok(directory) => directory.join(DATABASE_FILE),
        Err(error) => {
            log::error!("database unavailable: {error}");
            return DbState::unavailable(None, DATABASE_PATH_UNAVAILABLE);
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
            log::error!(
                "database unavailable path={} error={error:#}",
                path.display()
            );
            let summary = error_summary(&error);
            DbState::unavailable(Some(path), summary)
        }
    }
}

fn database_status(path: Option<&Path>, inner: &DbInner) -> DatabaseStatus {
    DatabaseStatus {
        available: inner.connection.is_some(),
        path: path.map(|path| path.to_string_lossy().into_owned()),
        schema_version: inner.schema_version,
        error: inner.error.clone(),
    }
}

fn unavailable_status(path: Option<&Path>, error: &str) -> DatabaseStatus {
    DatabaseStatus {
        available: false,
        path: path.map(|path| path.to_string_lossy().into_owned()),
        schema_version: None,
        error: Some(error.to_string()),
    }
}

fn error_summary(error: &anyhow::Error) -> String {
    error.root_cause().to_string()
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
    history_runs::mark_interrupted(&connection)?;
    Ok((connection, schema_version))
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
        assert_eq!(table_count(&connection, "cleanup_runs"), 1);
        assert_eq!(table_count(&connection, "cleanup_totals"), 1);

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
    fn migration_upgrades_v1_records_with_manual_source() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        migrate_to_v1(&mut connection).unwrap();
        connection
            .execute(
                "INSERT INTO clean_records (item_path, item_type) VALUES (?1, 'recent_file')",
                [r"C:\old.txt"],
            )
            .unwrap();

        assert_eq!(user_version(&connection), 1);
        assert_eq!(migrate(&mut connection).unwrap(), SCHEMA_VERSION);
        assert_eq!(user_version(&connection), SCHEMA_VERSION);
        assert_eq!(
            connection
                .query_row("SELECT scope FROM rules LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "all"
        );
        assert_eq!(
            connection
                .query_row("SELECT source FROM clean_records", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "manual"
        );
        assert!(connection
            .execute(
                "INSERT INTO clean_records (item_path, item_type, source)
                 VALUES (?1, 'recent_file', 'invalid')",
                [r"C:\invalid.txt"],
            )
            .is_err());
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
    fn debug_mock_database_is_isolated_from_the_real_connection() {
        let directory = unique_temp_path("database-mock");
        let path = directory.join(DATABASE_FILE);
        let state = initialize_path(path.clone());
        state
            .with_connection(|connection| {
                connection.execute("DELETE FROM rules", [])?;
                Ok(())
            })
            .unwrap();

        state.set_mock_mode(true).unwrap();
        assert_ne!(
            state.status().path.as_deref(),
            Some(path.to_string_lossy().as_ref())
        );
        assert!(!state
            .with_connection(|connection| rules::list(connection))
            .unwrap()
            .is_empty());

        state.set_mock_mode(false).unwrap();
        assert_eq!(
            state.status().path,
            Some(path.to_string_lossy().into_owned())
        );
        assert_eq!(
            state
                .with_connection(|connection| rules::list(connection))
                .unwrap()
                .len(),
            0
        );
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

    #[test]
    fn retry_recovers_an_unavailable_database_in_place() {
        let directory = unique_temp_path("database-retry");
        let path = directory.join(DATABASE_FILE);
        let state = DbState::unavailable(Some(path.clone()), "initial failure");

        let status = state.retry();

        assert!(status.available);
        assert_eq!(status.path, Some(path.to_string_lossy().into_owned()));
        assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
        assert_eq!(status.error, None);
        assert!(state.with_connection(|_| Ok(())).is_ok());
        drop(state);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn database_error_summary_uses_only_the_root_cause() {
        let error = anyhow::anyhow!("access denied").context("failed at a private path");

        assert_eq!(error_summary(&error), "access denied");
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
