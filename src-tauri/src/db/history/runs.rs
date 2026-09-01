use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use super::HistoryError;

const MAX_PAGE_SIZE: u32 = 100;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CleanupAction {
    RemoveSelected,
    Empty,
    SmartClean,
    AutoClean,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CleanupTrigger {
    Manual,
    Monitor,
    Scheduled,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CleanupRunStatus {
    Running,
    Success,
    Partial,
    Failed,
    Noop,
    Interrupted,
}

impl CleanupAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::RemoveSelected => "remove_selected",
            Self::Empty => "empty",
            Self::SmartClean => "smart_clean",
            Self::AutoClean => "auto_clean",
        }
    }
}

impl CleanupTrigger {
    fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Monitor => "monitor",
            Self::Scheduled => "scheduled",
        }
    }
}

impl CleanupRunStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failed => "failed",
            Self::Noop => "noop",
            Self::Interrupted => "interrupted",
        }
    }

    fn from_str(value: &str) -> rusqlite::Result<Self> {
        match value {
            "running" => Ok(Self::Running),
            "success" => Ok(Self::Success),
            "partial" => Ok(Self::Partial),
            "failed" => Ok(Self::Failed),
            "noop" => Ok(Self::Noop),
            "interrupted" => Ok(Self::Interrupted),
            _ => Err(rusqlite::Error::InvalidQuery),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NewCleanupRun<'a> {
    pub(crate) action: CleanupAction,
    pub(crate) trigger: CleanupTrigger,
    pub(crate) qa_type: &'a str,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RunCompletion {
    pub(crate) requested: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) protected: usize,
    pub(crate) warnings: usize,
    pub(crate) history_errors: usize,
    pub(crate) section_errors: usize,
    pub(crate) incident_id: Option<String>,
}

impl RunCompletion {
    pub(crate) fn status(&self) -> CleanupRunStatus {
        if self.succeeded == 0
            && self.failed == 0
            && self.section_errors == 0
            && self.warnings == 0
            && self.history_errors == 0
        {
            CleanupRunStatus::Noop
        } else if self.succeeded == 0
            && (self.failed > 0 || self.section_errors > 0 || self.history_errors > 0)
        {
            CleanupRunStatus::Failed
        } else if self.failed > 0
            || self.warnings > 0
            || self.history_errors > 0
            || self.section_errors > 0
        {
            CleanupRunStatus::Partial
        } else {
            CleanupRunStatus::Success
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CleanupRun {
    pub(crate) id: i64,
    pub(crate) action: String,
    pub(crate) trigger: String,
    pub(crate) qa_type: String,
    pub(crate) status: CleanupRunStatus,
    pub(crate) requested_count: i64,
    pub(crate) succeeded_count: i64,
    pub(crate) failed_count: i64,
    pub(crate) protected_count: i64,
    pub(crate) warning_count: i64,
    pub(crate) history_error_count: i64,
    pub(crate) section_error_count: i64,
    pub(crate) incident_id: Option<String>,
    pub(crate) started_at: String,
    pub(crate) completed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub(crate) struct CleanupRunFilter {
    #[serde(default)]
    pub(crate) action: Option<CleanupAction>,
    #[serde(default)]
    pub(crate) trigger: Option<CleanupTrigger>,
    #[serde(default)]
    pub(crate) status: Option<CleanupRunStatus>,
    #[serde(default)]
    pub(crate) date_range: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct CleanupRunQuery {
    pub(crate) page: u32,
    pub(crate) page_size: u32,
    #[serde(flatten)]
    pub(crate) filter: CleanupRunFilter,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct CleanupRunPage {
    pub(crate) runs: Vec<CleanupRun>,
    pub(crate) total: u64,
    pub(crate) page: u32,
    pub(crate) page_size: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct LifetimeTotals {
    pub(crate) cleaned_total: u64,
    pub(crate) cleaned_files: u64,
    pub(crate) cleaned_folders: u64,
    pub(crate) cleanup_runs_total: u64,
    pub(crate) successful_runs: u64,
    pub(crate) partial_runs: u64,
    pub(crate) failed_runs: u64,
    pub(crate) noop_runs: u64,
    pub(crate) interrupted_runs: u64,
    pub(crate) tracking_started_at: String,
}

pub(crate) fn begin(connection: &Connection, run: NewCleanupRun<'_>) -> Result<i64> {
    connection
        .execute(
            "INSERT INTO cleanup_runs (action, trigger, qa_type) VALUES (?1, ?2, ?3)",
            params![run.action.as_str(), run.trigger.as_str(), run.qa_type],
        )
        .context("failed to start cleanup run")?;
    Ok(connection.last_insert_rowid())
}

pub(crate) fn finish(
    connection: &mut Connection,
    run_id: i64,
    completion: &RunCompletion,
    retention: usize,
) -> Result<()> {
    let status = completion.status();
    let transaction = connection
        .transaction()
        .context("failed to start cleanup run completion transaction")?;
    transaction
        .execute(
            "UPDATE cleanup_runs
             SET status = ?1, requested_count = ?2, succeeded_count = ?3,
                 failed_count = ?4, protected_count = ?5, warning_count = ?6,
                 history_error_count = ?7, section_error_count = ?8,
                 incident_id = ?9, completed_at = strftime('%Y-%m-%d %H:%M:%f', 'now')
             WHERE id = ?10",
            params![
                status.as_str(),
                as_i64(completion.requested)?,
                as_i64(completion.succeeded)?,
                as_i64(completion.failed)?,
                as_i64(completion.protected)?,
                as_i64(completion.warnings)?,
                as_i64(completion.history_errors)?,
                as_i64(completion.section_errors)?,
                completion.incident_id,
                run_id,
            ],
        )
        .with_context(|| format!("failed to finish cleanup run {run_id}"))?;
    increment_run_totals(&transaction, status)?;
    trim(&transaction, retention)?;
    transaction
        .commit()
        .context("failed to commit cleanup run completion")
}

pub(crate) fn latest_terminal_auto_clean_completion(
    connection: &Connection,
    trigger: CleanupTrigger,
) -> Result<Option<DateTime<Utc>>> {
    let completed_at = connection
        .query_row(
            "SELECT completed_at
             FROM cleanup_runs
             WHERE action = 'auto_clean'
               AND trigger = ?1
               AND status IN ('success', 'partial', 'failed', 'noop')
               AND completed_at IS NOT NULL
             ORDER BY completed_at DESC, id DESC
             LIMIT 1",
            [trigger.as_str()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to read latest completed auto-clean run")?;
    completed_at
        .map(|value| {
            NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S%.f")
                .map(|value| value.and_utc())
                .with_context(|| format!("invalid auto-clean completion timestamp: {value}"))
        })
        .transpose()
}

pub(crate) fn list(connection: &Connection, query: CleanupRunQuery) -> Result<CleanupRunPage> {
    if query.page == 0 {
        return Err(HistoryError::Page.into());
    }
    if !(1..=MAX_PAGE_SIZE).contains(&query.page_size) {
        return Err(HistoryError::PageSize.into());
    }
    let date_modifier = validate_date_range(query.filter.date_range.as_deref())?;
    let action = query.filter.action.map(CleanupAction::as_str);
    let trigger = query.filter.trigger.map(CleanupTrigger::as_str);
    let status = query.filter.status.map(CleanupRunStatus::as_str);
    let filter_sql = "WHERE (?1 IS NULL OR action = ?1)
                        AND (?2 IS NULL OR trigger = ?2)
                        AND (?3 IS NULL OR status = ?3)
                        AND (?4 IS NULL OR started_at >= datetime('now', 'localtime', 'start of day', ?4, 'utc'))";
    let total = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM cleanup_runs {filter_sql}"),
            params![action, trigger, status, date_modifier],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to count cleanup runs")?;
    let offset = i64::try_from(u64::from(query.page - 1) * u64::from(query.page_size))
        .context("cleanup run page offset is too large")?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT id, action, trigger, qa_type, status,
                    requested_count, succeeded_count, failed_count, protected_count,
                    warning_count, history_error_count, section_error_count,
                    incident_id, started_at, completed_at
             FROM cleanup_runs {filter_sql}
             ORDER BY started_at DESC, id DESC LIMIT ?5 OFFSET ?6"
        ))
        .context("failed to prepare cleanup run query")?;
    let runs = statement
        .query_map(
            params![
                action,
                trigger,
                status,
                date_modifier,
                i64::from(query.page_size),
                offset
            ],
            read_run,
        )
        .context("failed to query cleanup runs")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to read cleanup runs")?;
    Ok(CleanupRunPage {
        runs,
        total: u64::try_from(total).context("cleanup run count is negative")?,
        page: query.page,
        page_size: query.page_size,
    })
}

pub(crate) fn visit_filtered(
    connection: &Connection,
    filter: CleanupRunFilter,
    mut visit: impl FnMut(CleanupRun) -> Result<()>,
) -> Result<u64> {
    let date_modifier = validate_date_range(filter.date_range.as_deref())?;
    let action = filter.action.map(CleanupAction::as_str);
    let trigger = filter.trigger.map(CleanupTrigger::as_str);
    let status = filter.status.map(CleanupRunStatus::as_str);
    let mut statement = connection
        .prepare(
            "SELECT id, action, trigger, qa_type, status,
                    requested_count, succeeded_count, failed_count, protected_count,
                    warning_count, history_error_count, section_error_count,
                    incident_id, started_at, completed_at
             FROM cleanup_runs
             WHERE (?1 IS NULL OR action = ?1)
               AND (?2 IS NULL OR trigger = ?2)
               AND (?3 IS NULL OR status = ?3)
               AND (?4 IS NULL OR started_at >= datetime('now', 'localtime', 'start of day', ?4, 'utc'))
             ORDER BY started_at DESC, id DESC",
        )
        .context("failed to prepare cleanup run export query")?;
    let rows = statement
        .query_map(params![action, trigger, status, date_modifier], read_run)
        .context("failed to query cleanup runs for export")?;
    let mut count = 0_u64;
    for row in rows {
        visit(row.context("failed to read cleanup run for export")?)?;
        count += 1;
    }
    Ok(count)
}

pub(crate) fn lifetime_totals(connection: &Connection) -> Result<LifetimeTotals> {
    let values = connection
        .query_row(
            "SELECT cleaned_total, cleaned_files, cleaned_folders, cleanup_runs_total,
                    successful_runs, partial_runs, failed_runs, noop_runs, interrupted_runs,
                    tracking_started_at
             FROM cleanup_totals WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )
        .context("failed to read lifetime cleanup totals")?;
    Ok(LifetimeTotals {
        cleaned_total: nonnegative(values.0, "cleaned total")?,
        cleaned_files: nonnegative(values.1, "cleaned file total")?,
        cleaned_folders: nonnegative(values.2, "cleaned folder total")?,
        cleanup_runs_total: nonnegative(values.3, "cleanup run total")?,
        successful_runs: nonnegative(values.4, "successful run total")?,
        partial_runs: nonnegative(values.5, "partial run total")?,
        failed_runs: nonnegative(values.6, "failed run total")?,
        noop_runs: nonnegative(values.7, "noop run total")?,
        interrupted_runs: nonnegative(values.8, "interrupted run total")?,
        tracking_started_at: values.9,
    })
}

pub(crate) fn mark_interrupted(connection: &Connection) -> Result<u64> {
    let interrupted = connection
        .query_row(
            "SELECT COUNT(*) FROM cleanup_runs WHERE status = 'running'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .context("failed to count interrupted cleanup runs")?;
    if interrupted == 0 {
        return Ok(0);
    }
    connection
        .execute(
            "UPDATE cleanup_runs
             SET status = 'interrupted', completed_at = strftime('%Y-%m-%d %H:%M:%f', 'now')
             WHERE status = 'running'",
            [],
        )
        .context("failed to mark interrupted cleanup runs")?;
    connection
        .execute(
            "UPDATE cleanup_totals
             SET cleanup_runs_total = cleanup_runs_total + ?1,
                 interrupted_runs = interrupted_runs + ?1,
                 updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now')
             WHERE id = 1",
            [interrupted],
        )
        .context("failed to count interrupted cleanup runs")?;
    u64::try_from(interrupted).context("interrupted cleanup run count is negative")
}

fn increment_run_totals(connection: &Connection, status: CleanupRunStatus) -> Result<()> {
    let column = match status {
        CleanupRunStatus::Success => "successful_runs",
        CleanupRunStatus::Partial => "partial_runs",
        CleanupRunStatus::Failed => "failed_runs",
        CleanupRunStatus::Noop => "noop_runs",
        CleanupRunStatus::Interrupted => "interrupted_runs",
        CleanupRunStatus::Running => return Ok(()),
    };
    connection
        .execute(
            &format!(
                "UPDATE cleanup_totals
                 SET cleanup_runs_total = cleanup_runs_total + 1,
                     {column} = {column} + 1,
                     updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now')
                 WHERE id = 1"
            ),
            [],
        )
        .context("failed to update cleanup run totals")?;
    Ok(())
}

fn trim(connection: &Connection, retention: usize) -> Result<()> {
    if retention == 0 {
        return Ok(());
    }
    connection
        .execute(
            "DELETE FROM cleanup_runs
             WHERE id NOT IN (
                 SELECT id FROM cleanup_runs ORDER BY started_at DESC, id DESC LIMIT ?1
             )",
            [as_i64(retention)?],
        )
        .context("failed to trim cleanup runs")?;
    Ok(())
}

fn read_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<CleanupRun> {
    let status = row.get::<_, String>(4)?;
    Ok(CleanupRun {
        id: row.get(0)?,
        action: row.get(1)?,
        trigger: row.get(2)?,
        qa_type: row.get(3)?,
        status: CleanupRunStatus::from_str(&status)?,
        requested_count: row.get(5)?,
        succeeded_count: row.get(6)?,
        failed_count: row.get(7)?,
        protected_count: row.get(8)?,
        warning_count: row.get(9)?,
        history_error_count: row.get(10)?,
        section_error_count: row.get(11)?,
        incident_id: row.get(12)?,
        started_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

fn validate_date_range(value: Option<&str>) -> Result<Option<&'static str>> {
    match value {
        None => Ok(None),
        Some("7d") => Ok(Some("-6 days")),
        Some("30d") => Ok(Some("-29 days")),
        Some(value) => Err(HistoryError::DateRange(value.to_string()).into()),
    }
}

fn as_i64(value: usize) -> Result<i64> {
    i64::try_from(value).context("cleanup run count is too large")
}

fn nonnegative(value: i64, label: &str) -> Result<u64> {
    u64::try_from(value).with_context(|| format!("{label} is negative"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{history, migrations::migrate};

    fn connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .unwrap();
        migrate(&mut connection).unwrap();
        connection
    }

    #[test]
    fn status_distinguishes_success_partial_failed_and_noop() {
        assert_eq!(
            RunCompletion {
                requested: 1,
                succeeded: 1,
                ..RunCompletion::default()
            }
            .status(),
            CleanupRunStatus::Success
        );
        assert_eq!(
            RunCompletion {
                requested: 2,
                succeeded: 1,
                failed: 1,
                ..RunCompletion::default()
            }
            .status(),
            CleanupRunStatus::Partial
        );
        assert_eq!(
            RunCompletion {
                requested: 1,
                failed: 1,
                ..RunCompletion::default()
            }
            .status(),
            CleanupRunStatus::Failed
        );
        assert_eq!(
            RunCompletion {
                requested: 1,
                protected: 1,
                ..RunCompletion::default()
            }
            .status(),
            CleanupRunStatus::Noop
        );
    }

    #[test]
    fn finishes_run_and_updates_lifetime_run_totals() {
        let mut connection = connection();
        let run_id = begin(
            &connection,
            NewCleanupRun {
                action: CleanupAction::SmartClean,
                trigger: CleanupTrigger::Manual,
                qa_type: "recent",
            },
        )
        .unwrap();
        history::insert_batch(
            &mut connection,
            &[history::NewCleanRecord {
                run_id: Some(run_id),
                item_path: r"C:\Temp\a.txt".to_string(),
                item_type: "recent_file".to_string(),
                rule_id: None,
                rule_keyword: None,
                source: history::CleanSource::Manual,
            }],
            0,
        )
        .unwrap();
        finish(
            &mut connection,
            run_id,
            &RunCompletion {
                requested: 1,
                succeeded: 1,
                ..RunCompletion::default()
            },
            0,
        )
        .unwrap();

        let totals = lifetime_totals(&connection).unwrap();
        assert_eq!(totals.cleaned_total, 1);
        assert_eq!(totals.cleaned_files, 1);
        assert_eq!(totals.cleanup_runs_total, 1);
        assert_eq!(totals.successful_runs, 1);
        assert_eq!(
            list(
                &connection,
                CleanupRunQuery {
                    page: 1,
                    page_size: 20,
                    filter: CleanupRunFilter::default(),
                }
            )
            .unwrap()
            .runs[0]
                .status,
            CleanupRunStatus::Success
        );
    }

    #[test]
    fn marks_running_runs_as_interrupted() {
        let connection = connection();
        begin(
            &connection,
            NewCleanupRun {
                action: CleanupAction::AutoClean,
                trigger: CleanupTrigger::Monitor,
                qa_type: "all",
            },
        )
        .unwrap();
        assert_eq!(mark_interrupted(&connection).unwrap(), 1);
        let totals = lifetime_totals(&connection).unwrap();
        assert_eq!(totals.interrupted_runs, 1);
        assert_eq!(totals.cleanup_runs_total, 1);
    }

    #[test]
    fn latest_terminal_auto_clean_completion_uses_only_matching_terminal_runs() {
        let mut connection = connection();
        let scheduled = begin(
            &connection,
            NewCleanupRun {
                action: CleanupAction::AutoClean,
                trigger: CleanupTrigger::Scheduled,
                qa_type: "all",
            },
        )
        .unwrap();
        finish(
            &mut connection,
            scheduled,
            &RunCompletion {
                succeeded: 1,
                ..RunCompletion::default()
            },
            0,
        )
        .unwrap();
        let interrupted = begin(
            &connection,
            NewCleanupRun {
                action: CleanupAction::AutoClean,
                trigger: CleanupTrigger::Scheduled,
                qa_type: "all",
            },
        )
        .unwrap();
        assert_eq!(mark_interrupted(&connection).unwrap(), 1);
        connection
            .execute(
                "UPDATE cleanup_runs SET completed_at = '2099-01-01 00:00:00.000' WHERE id = ?1",
                [interrupted],
            )
            .unwrap();
        let monitor = begin(
            &connection,
            NewCleanupRun {
                action: CleanupAction::AutoClean,
                trigger: CleanupTrigger::Monitor,
                qa_type: "all",
            },
        )
        .unwrap();
        finish(
            &mut connection,
            monitor,
            &RunCompletion {
                succeeded: 1,
                ..RunCompletion::default()
            },
            0,
        )
        .unwrap();

        let completion =
            latest_terminal_auto_clean_completion(&connection, CleanupTrigger::Scheduled)
                .unwrap()
                .unwrap();
        assert!(completion < DateTime::parse_from_rfc3339("2099-01-01T00:00:00Z").unwrap());
        assert!(
            latest_terminal_auto_clean_completion(&connection, CleanupTrigger::Manual)
                .unwrap()
                .is_none()
        );
    }
}
