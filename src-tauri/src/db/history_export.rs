use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::history::{
    history_filter_params, read_clean_record, validate_history_filter, CleanRecord,
    ExportCleanRecord, HistoryError, HistoryExportFormat, HistoryExportResult, HistoryFilter,
    ValidatedHistoryFilter, HISTORY_COLUMNS, HISTORY_FILTERS,
};

pub(super) fn export(
    connection: &Connection,
    path: &str,
    format: HistoryExportFormat,
    filter: HistoryFilter,
) -> Result<HistoryExportResult> {
    let path = validate_export_path(path, format)?;
    let filter = validate_history_filter(filter)?;
    let file = File::create(path)
        .with_context(|| format!("failed to create history export at {}", path.display()))?;
    let count = match format {
        HistoryExportFormat::Csv => export_csv(connection, file, &filter)?,
        HistoryExportFormat::Json => export_json(connection, file, &filter)?,
    };
    Ok(HistoryExportResult {
        count,
        path: path.to_string_lossy().into_owned(),
        format,
    })
}

fn export_csv(connection: &Connection, file: File, filter: &ValidatedHistoryFilter) -> Result<u64> {
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(BufWriter::new(file));
    writer
        .write_record([
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

fn export_json(
    connection: &Connection,
    file: File,
    filter: &ValidatedHistoryFilter,
) -> Result<u64> {
    let mut records = Vec::new();
    let count = visit_filtered_records(connection, filter, |record| {
        records.push(ExportCleanRecord::from(record));
        Ok(())
    })?;
    let mut writer = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, &records)
        .context("failed to serialize JSON history export")?;
    writer
        .flush()
        .context("failed to finish JSON history export")?;
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
