use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde::{ser::SerializeSeq, Serializer};
use tempfile::NamedTempFile;

use super::{
    history::{
        history_filter_params, read_clean_record, validate_history_filter, CleanRecord,
        ExportCleanRecord, HistoryError, HistoryExportFormat, HistoryExportResult, HistoryFilter,
        ValidatedHistoryFilter, HISTORY_COLUMNS, HISTORY_FILTERS,
    },
    history_runs::{self, CleanupRunFilter},
};

pub(super) fn export(
    connection: &Connection,
    path: &str,
    format: HistoryExportFormat,
    filter: HistoryFilter,
) -> Result<HistoryExportResult> {
    let filter = validate_history_filter(filter)?;
    export_atomic(path, format, |file| match format {
        HistoryExportFormat::Csv => export_items_csv(connection, file, &filter),
        HistoryExportFormat::Json => export_items_json(connection, file, &filter),
    })
}

pub(super) fn export_runs(
    connection: &Connection,
    path: &str,
    format: HistoryExportFormat,
    filter: CleanupRunFilter,
) -> Result<HistoryExportResult> {
    export_atomic(path, format, |file| match format {
        HistoryExportFormat::Csv => export_runs_csv(connection, file, filter.clone()),
        HistoryExportFormat::Json => export_runs_json(connection, file, filter.clone()),
    })
}

fn export_atomic(
    path: &str,
    format: HistoryExportFormat,
    write: impl FnOnce(&mut File) -> Result<u64>,
) -> Result<HistoryExportResult> {
    let path = validate_export_path(path, format)?;
    let parent = path
        .parent()
        .context("history export target has no parent directory")?;
    let mut temporary = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temporary export in {}", parent.display()))?;
    let count = write(temporary.as_file_mut())?;
    temporary
        .as_file_mut()
        .sync_all()
        .context("failed to flush temporary history export")?;
    temporary.persist(path).map_err(|error| {
        anyhow::Error::new(error.error).context(format!(
            "failed to persist history export at {}",
            path.display()
        ))
    })?;
    Ok(HistoryExportResult {
        count,
        path: path.to_string_lossy().into_owned(),
        format,
    })
}

fn export_items_csv(
    connection: &Connection,
    file: &mut File,
    filter: &ValidatedHistoryFilter,
) -> Result<u64> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(BufWriter::new(file));
    writer
        .write_record([
            "run_id",
            "item_path",
            "item_type",
            "rule_id",
            "rule_keyword",
            "source",
            "cleaned_at",
        ])
        .context("failed to write CSV history header")?;
    let count = visit_filtered_records(connection, filter, |record| {
        writer
            .serialize(ExportCleanRecord::from(record))
            .context("failed to serialize CSV history record")
    })?;
    writer
        .flush()
        .context("failed to finish CSV history export")?;
    Ok(count)
}

fn export_items_json(
    connection: &Connection,
    file: &mut File,
    filter: &ValidatedHistoryFilter,
) -> Result<u64> {
    let mut writer = BufWriter::new(file);
    let count;
    {
        let mut serializer = serde_json::Serializer::pretty(&mut writer);
        let mut sequence = serializer
            .serialize_seq(None)
            .context("failed to start JSON history export")?;
        count = visit_filtered_records(connection, filter, |record| {
            sequence
                .serialize_element(&ExportCleanRecord::from(record))
                .context("failed to serialize JSON history record")
        })?;
        sequence
            .end()
            .context("failed to finish JSON history export")?;
    }
    writer
        .flush()
        .context("failed to flush JSON history export")?;
    Ok(count)
}

fn export_runs_csv(
    connection: &Connection,
    file: &mut File,
    filter: CleanupRunFilter,
) -> Result<u64> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(BufWriter::new(file));
    writer
        .write_record([
            "id",
            "action",
            "trigger",
            "qa_type",
            "status",
            "requested_count",
            "succeeded_count",
            "failed_count",
            "protected_count",
            "warning_count",
            "history_error_count",
            "section_error_count",
            "incident_id",
            "started_at",
            "completed_at",
        ])
        .context("failed to write cleanup run CSV header")?;
    let count = history_runs::visit_filtered(connection, filter, |run| {
        writer
            .serialize(run)
            .context("failed to serialize cleanup run CSV record")
    })?;
    writer
        .flush()
        .context("failed to finish cleanup run CSV export")?;
    Ok(count)
}

fn export_runs_json(
    connection: &Connection,
    file: &mut File,
    filter: CleanupRunFilter,
) -> Result<u64> {
    let mut writer = BufWriter::new(file);
    let count;
    {
        let mut serializer = serde_json::Serializer::pretty(&mut writer);
        let mut sequence = serializer
            .serialize_seq(None)
            .context("failed to start cleanup run JSON export")?;
        count = history_runs::visit_filtered(connection, filter, |run| {
            sequence
                .serialize_element(&run)
                .context("failed to serialize cleanup run JSON record")
        })?;
        sequence
            .end()
            .context("failed to finish cleanup run JSON export")?;
    }
    writer
        .flush()
        .context("failed to flush cleanup run JSON export")?;
    Ok(count)
}

fn visit_filtered_records(
    connection: &Connection,
    filter: &ValidatedHistoryFilter,
    mut visit: impl FnMut(CleanRecord) -> Result<()>,
) -> Result<u64> {
    let sql = format!(
        "SELECT {HISTORY_COLUMNS}
         FROM clean_records
         {HISTORY_FILTERS}
         ORDER BY cleaned_at DESC, id DESC"
    );
    let mut statement = connection
        .prepare(&sql)
        .context("failed to prepare history export query")?;
    let rows = statement
        .query_map(history_filter_params(filter), read_clean_record)
        .context("failed to query history export")?;
    let mut count = 0_u64;
    for row in rows {
        visit(row.context("failed to read history export record")?)?;
        count += 1;
    }
    Ok(count)
}

fn validate_export_path(path: &str, format: HistoryExportFormat) -> Result<&Path> {
    if path.trim().is_empty() {
        return Err(HistoryError::ExportPath("path is empty".to_string()).into());
    }
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(HistoryError::ExportPath("path must be absolute".to_string()).into());
    }
    let extension_matches = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(format.extension()));
    if !extension_matches {
        return Err(HistoryError::ExportPath(format!(
            "path must use the .{} extension",
            format.extension()
        ))
        .into());
    }
    if path.parent().is_none_or(|parent| !parent.is_dir()) {
        return Err(HistoryError::ExportPath("directory does not exist".to_string()).into());
    }
    Ok(path)
}
