use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use super::history::{HistoryTotals, RuleHitStat, Stats, StatsRange, StatsTrendPoint};

pub(super) fn stats(connection: &Connection, range: StatsRange) -> Result<Stats> {
    let totals = totals(connection)?;
    let (daily_trend, weekly_trend) = if totals.total == 0 {
        (Vec::new(), Vec::new())
    } else {
        (
            trend(connection, range, false, "daily cleanup trend")?,
            trend(connection, range, true, "weekly cleanup trend")?,
        )
    };

    Ok(Stats {
        total: totals.total,
        recent_files: totals.recent_files,
        frequent_folders: totals.frequent_folders,
        daily_trend,
        weekly_trend,
        rule_hits: rule_hits(connection)?,
    })
}

pub(super) fn totals(connection: &Connection) -> Result<HistoryTotals> {
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

    Ok(HistoryTotals {
        total: u64::try_from(total).context("cleanup total is negative")?,
        recent_files: u64::try_from(recent_files).context("recent file total is negative")?,
        frequent_folders: u64::try_from(frequent_folders)
            .context("frequent folder total is negative")?,
    })
}

fn trend(
    connection: &Connection,
    range: StatsRange,
    weekly: bool,
    label: &str,
) -> Result<Vec<StatsTrendPoint>> {
    let mut statement = connection
        .prepare(
            "WITH RECURSIVE
             bounds(start_day, end_day) AS (
                 SELECT
                     COALESCE(
                         CASE
                             WHEN ?1 IS NULL THEN (
                                 SELECT MIN(date(cleaned_at, 'localtime')) FROM clean_records
                             )
                             ELSE date('now', 'localtime', ?1)
                         END,
                         date('now', 'localtime')
                     ),
                     date('now', 'localtime')
             ),
             days(day) AS (
                 SELECT start_day FROM bounds
                 UNION ALL
                 SELECT date(day, '+1 day')
                 FROM days, bounds
                 WHERE day < end_day
             ),
             record_days(day, count) AS (
                 SELECT date(cleaned_at, 'localtime'), COUNT(*)
                 FROM clean_records
                 GROUP BY date(cleaned_at, 'localtime')
             )
             SELECT
                 CASE
                     WHEN ?2 = 1 THEN date(days.day, 'weekday 0', '-6 days')
                     ELSE days.day
                 END AS period,
                 COALESCE(SUM(record_days.count), 0)
             FROM days
             LEFT JOIN record_days ON record_days.day = days.day
             GROUP BY period
             ORDER BY period",
        )
        .with_context(|| format!("failed to prepare {label}"))?;
    let rows = statement
        .query_map(params![range.date_modifier(), weekly], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
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
