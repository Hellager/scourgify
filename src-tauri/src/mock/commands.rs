use chrono::{Duration, Utc};
use fake::{
    faker::{
        chrono::en::DateTimeBetween,
        filesystem::{
            en::FileName,
            raw::{DirPath, FilePath},
        },
    },
    locales::Data,
    Fake, Faker,
};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::time::Duration as StdDuration;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    app::scheduler::{AutoCleanFinished, AutoCleanMonitor, AUTO_CLEAN_FINISHED_EVENT},
    db::DbState,
    error::{CommandError, CommandResult, ErrorCode},
    privacy::PrivacyManager,
    quick_access::{BackendMode, QuickAccessBackendState, QuickAccessCache, QuickAccessWatchers},
    rules::{NewRule, RuleScope, RuleType},
};

use super::{MockOverviewConfig, MockOverviewRequest, MockScenario, MockSnapshot};

const MAX_MOCK_RULES: usize = 1_000;
const MAX_MOCK_HISTORY_RECORDS: usize = 1_000;
const MOCK_PATH_EXTENSIONS: &[&str] = &[
    "docx", "xlsx", "pptx", "pdf", "txt", "csv", "jpg", "png", "mp3", "mp4", "zip", "7z",
];

macro_rules! define_mock_path_locale {
    ($name:ident, $roots:expr, $segments:expr) => {
        #[derive(Clone, Copy)]
        struct $name;

        impl Data for $name {
            const PATH_ROOT_DIRS: &'static [&'static str] = $roots;
            const PATH_SEGMENTS: &'static [&'static str] = $segments;
            const PATH_EXTENSIONS: &'static [&'static str] = MOCK_PATH_EXTENSIONS;
        }
    };
}

define_mock_path_locale!(
    EnglishPathLocale,
    &["/Users", "/ProgramData", "/Temp"],
    &[
        "Documents",
        "Downloads",
        "Pictures",
        "Projects",
        "Archives",
        "Reports",
        "Invoices",
        "Desktop",
        "Cache",
        "Backups",
        "Music",
        "Videos",
    ]
);
define_mock_path_locale!(
    SimplifiedChinesePathLocale,
    &["/用户", "/数据", "/临时文件"],
    &[
        "文档", "下载", "图片", "项目", "归档", "报告", "发票", "桌面", "缓存", "备份", "音乐",
        "视频",
    ]
);
define_mock_path_locale!(
    TraditionalChinesePathLocale,
    &["/使用者", "/資料", "/暫存"],
    &[
        "檔案", "下載", "圖片", "專案", "封存", "報告", "發票", "桌面", "快取", "備份", "音樂",
        "影片",
    ]
);
define_mock_path_locale!(
    FrenchPathLocale,
    &["/Utilisateurs", "/Données", "/Téléchargements"],
    &[
        "Documents",
        "Téléchargements",
        "Images",
        "Projets",
        "Archives",
        "Rapports",
        "Factures",
        "Bureau",
        "Cache",
        "Sauvegardes",
        "Musique",
        "Vidéos",
    ]
);
define_mock_path_locale!(
    RussianPathLocale,
    &["/Пользователи", "/Данные", "/Загрузки"],
    &[
        "Документы",
        "Загрузки",
        "Изображения",
        "Проекты",
        "Архив",
        "Отчёты",
        "Счета",
        "Рабочий стол",
        "Кэш",
        "Резервные копии",
        "Музыка",
        "Видео",
    ]
);

#[derive(Clone, Copy)]
enum MockPathLocale {
    English,
    SimplifiedChinese,
    TraditionalChinese,
    French,
    Russian,
}

impl MockPathLocale {
    fn for_index(index: usize) -> Self {
        match index % 5 {
            0 => Self::English,
            1 => Self::SimplifiedChinese,
            2 => Self::TraditionalChinese,
            3 => Self::French,
            _ => Self::Russian,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MockState {
    pub enabled: bool,
    pub snapshot: MockSnapshot,
    pub overview: MockOverviewConfig,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct MockRulesResult {
    pub generated: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct MockHistoryResult {
    pub generated: usize,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MockEventKind {
    QuickAccessRecent,
    QuickAccessFrequent,
    AutoCleanFinished,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MockOverviewPreset {
    Normal,
    Loading,
    CountsFailure,
    StatsFailure,
    BatchOverviewFailure,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum MockOverviewTarget {
    Counts,
    Stats,
}

pub(crate) fn apply_overview_request(
    backend: &QuickAccessBackendState,
    target: MockOverviewTarget,
    operation: &str,
) -> CommandResult<()> {
    if backend.mode() != BackendMode::Mock {
        return Ok(());
    }

    let config = backend
        .mock()
        .overview_config()
        .map_err(|error| mock_error(operation, error))?;
    if config.delay_ms > 0 {
        std::thread::sleep(StdDuration::from_millis(config.delay_ms));
    }
    let behavior = match target {
        MockOverviewTarget::Counts => config.counts,
        MockOverviewTarget::Stats => config.stats,
    };
    if behavior == MockOverviewRequest::Fail {
        return Err(CommandError::expected(
            operation,
            ErrorCode::SystemOperationFailed,
            "The mock overview request failed.",
            true,
            format!("mock overview {target:?} request was configured to fail"),
        ));
    }
    Ok(())
}

#[tauri::command]
pub(crate) fn get_mock_state(
    backend: State<'_, QuickAccessBackendState>,
) -> CommandResult<MockState> {
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn set_mock_overview_preset(
    backend: State<'_, QuickAccessBackendState>,
    preset: MockOverviewPreset,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    let config = match preset {
        MockOverviewPreset::Normal => MockOverviewConfig::default(),
        MockOverviewPreset::Loading => MockOverviewConfig {
            delay_ms: 2_500,
            ..MockOverviewConfig::default()
        },
        MockOverviewPreset::CountsFailure => MockOverviewConfig {
            counts: MockOverviewRequest::Fail,
            ..MockOverviewConfig::default()
        },
        MockOverviewPreset::StatsFailure => MockOverviewConfig {
            stats: MockOverviewRequest::Fail,
            ..MockOverviewConfig::default()
        },
        MockOverviewPreset::BatchOverviewFailure => MockOverviewConfig {
            counts: MockOverviewRequest::Fail,
            stats: MockOverviewRequest::Fail,
            ..MockOverviewConfig::default()
        },
    };
    backend
        .mock()
        .set_overview_config(config)
        .map_err(|error| mock_error("set_mock_overview_preset", error))?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn set_mock_mode(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
    watchers: State<'_, QuickAccessWatchers>,
    privacy: State<'_, PrivacyManager>,
    database: State<'_, DbState>,
    enabled: bool,
) -> CommandResult<MockState> {
    if enabled {
        privacy
            .set_mock_mode(true)
            .map_err(|error| mock_error("set_mock_mode", error))?;
        if let Err(error) = database.set_mock_mode(true) {
            let _ = privacy.set_mock_mode(false);
            return Err(mock_error("set_mock_mode", error));
        }
        if let Err(error) = watchers.pause().and_then(|()| backend.set_mock()) {
            let _ = database.set_mock_mode(false);
            let _ = privacy.set_mock_mode(false);
            let _ = watchers.resume(app.clone(), cache.inner().clone(), backend.inner().clone());
            return Err(mock_error("set_mock_mode", error));
        }
        backend
            .mock()
            .set_overview_config(MockOverviewConfig::default())
            .map_err(|error| mock_error("set_mock_mode", error))?;
    } else {
        privacy
            .set_mock_mode(false)
            .map_err(|error| mock_error("set_mock_mode", error))?;
        backend
            .set_real()
            .map_err(|error| mock_error("set_mock_mode", error))?;
        backend
            .mock()
            .set_overview_config(MockOverviewConfig::default())
            .map_err(|error| mock_error("set_mock_mode", error))?;
        database
            .set_mock_mode(false)
            .map_err(|error| mock_error("set_mock_mode", error))?;
        watchers
            .resume(app.clone(), cache.inner().clone(), backend.inner().clone())
            .map_err(|error| mock_error("set_mock_mode", error))?;
    }
    cache
        .clear()
        .map_err(|error| mock_error("set_mock_mode", error))?;
    for qa_type in ["recent", "frequent"] {
        cache
            .items(&app, backend.inner(), qa_type, true)
            .map_err(|error| mock_error("set_mock_mode", error))?;
    }
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn set_mock_scenario(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
    scenario: MockScenario,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    backend
        .mock()
        .set_scenario(scenario)
        .map_err(|error| mock_error("set_mock_scenario", error))?;
    refresh_cache(&app, backend.inner(), cache.inner(), "set_mock_scenario")?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn refresh_mock_data(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    backend
        .mock()
        .refresh()
        .map_err(|error| mock_error("refresh_mock_data", error))?;
    refresh_cache(&app, backend.inner(), cache.inner(), "refresh_mock_data")?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn reset_mock_data(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    backend
        .mock()
        .set_scenario(MockScenario::Normal)
        .map_err(|error| mock_error("reset_mock_data", error))?;
    refresh_cache(&app, backend.inner(), cache.inner(), "reset_mock_data")?;
    state(backend.inner())
}

#[tauri::command]
pub(crate) fn generate_mock_rules(
    backend: State<'_, QuickAccessBackendState>,
    database: State<'_, DbState>,
    count: usize,
) -> CommandResult<MockRulesResult> {
    ensure_enabled(backend.inner())?;
    if !(1..=MAX_MOCK_RULES).contains(&count) {
        return Err(CommandError::expected(
            "generate_mock_rules",
            ErrorCode::ValidationInvalidArgument,
            "The mock rule count is invalid.",
            false,
            format!("rule count must be between 1 and {MAX_MOCK_RULES}"),
        ));
    }

    let mock_rules = random_rules(count);
    database
        .with_connection(|connection| replace_mock_rules(connection, mock_rules))
        .map_err(|error| mock_error("generate_mock_rules", error))?;

    Ok(MockRulesResult { generated: count })
}

#[tauri::command]
pub(crate) fn generate_mock_history(
    backend: State<'_, QuickAccessBackendState>,
    database: State<'_, DbState>,
    count: usize,
) -> CommandResult<MockHistoryResult> {
    ensure_enabled(backend.inner())?;
    if !(1..=MAX_MOCK_HISTORY_RECORDS).contains(&count) {
        return Err(CommandError::expected(
            "generate_mock_history",
            ErrorCode::ValidationInvalidArgument,
            "The mock history count is invalid.",
            false,
            format!("history count must be between 1 and {MAX_MOCK_HISTORY_RECORDS}"),
        ));
    }

    database
        .with_connection(|connection| replace_mock_history(connection, count))
        .map_err(|error| mock_error("generate_mock_history", error))?;

    Ok(MockHistoryResult { generated: count })
}

#[tauri::command]
pub(crate) fn trigger_mock_event(
    app: AppHandle,
    backend: State<'_, QuickAccessBackendState>,
    cache: State<'_, QuickAccessCache>,
    event: MockEventKind,
) -> CommandResult<MockState> {
    ensure_enabled(backend.inner())?;
    match event {
        MockEventKind::QuickAccessRecent => {
            backend
                .mock()
                .trigger_change("recent")
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            cache
                .items(&app, backend.inner(), "recent", true)
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            trigger_monitor(&app)?;
        }
        MockEventKind::QuickAccessFrequent => {
            backend
                .mock()
                .trigger_change("frequent")
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            cache
                .items(&app, backend.inner(), "frequent", true)
                .map_err(|error| mock_error("trigger_mock_event", error))?;
            trigger_monitor(&app)?;
        }
        MockEventKind::AutoCleanFinished => {
            app.emit(
                AUTO_CLEAN_FINISHED_EVENT,
                AutoCleanFinished {
                    completed_at: Utc::now(),
                    total: 3,
                    succeeded: 2,
                    failed: 1,
                    warnings: 0,
                    section_errors: 0,
                    history_errors: 0,
                    post_process_warnings: Vec::new(),
                    last_run_persisted: true,
                },
            )
            .map_err(|error| mock_error("trigger_mock_event", error))?;
        }
    }
    state(backend.inner())
}

fn refresh_cache(
    app: &AppHandle,
    backend: &QuickAccessBackendState,
    cache: &QuickAccessCache,
    operation: &str,
) -> CommandResult<()> {
    for qa_type in ["recent", "frequent"] {
        cache
            .items(app, backend, qa_type, true)
            .map_err(|error| mock_error(operation, error))?;
    }
    Ok(())
}

fn trigger_monitor(app: &AppHandle) -> CommandResult<()> {
    if let Some(monitor) = app.try_state::<AutoCleanMonitor>() {
        monitor
            .trigger()
            .map_err(|error| mock_error("trigger_mock_event", error))?;
    }
    Ok(())
}

fn state(backend: &QuickAccessBackendState) -> CommandResult<MockState> {
    Ok(MockState {
        enabled: backend.mode() == BackendMode::Mock,
        snapshot: backend
            .mock()
            .snapshot()
            .map_err(|error| mock_error("get_mock_state", error))?,
        overview: backend
            .mock()
            .overview_config()
            .map_err(|error| mock_error("get_mock_state", error))?,
    })
}

fn random_rules(count: usize) -> Vec<NewRule> {
    (0..count).map(random_rule).collect()
}

fn replace_mock_rules(
    connection: &mut rusqlite::Connection,
    mock_rules: Vec<NewRule>,
) -> anyhow::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM rules", [])?;
    {
        let mut statement = transaction.prepare(
            "INSERT INTO rules (keyword, rule_type, scope, enabled)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for rule in mock_rules {
            statement.execute(params![
                rule.keyword,
                rule.rule_type.as_str(),
                rule.scope.as_str(),
                rule.enabled
            ])?;
        }
    }
    transaction.commit()?;
    Ok(())
}

fn replace_mock_history(connection: &mut rusqlite::Connection, count: usize) -> anyhow::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM clean_records", [])?;
    transaction.execute("DELETE FROM cleanup_runs", [])?;
    let history_end = Utc::now();
    let history_start = history_end - Duration::days(365);
    {
        let mut statement = transaction.prepare(
            "INSERT INTO clean_records
             (item_path, item_type, rule_keyword, source, cleaned_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for index in 0..count {
            let is_file = index % 2 == 0;
            let (item_path, path_keyword) =
                random_history_path(is_file, MockPathLocale::for_index(index));
            let rule_keyword = (index % 4 < 2).then_some(path_keyword);
            let cleaned_at: chrono::DateTime<Utc> =
                DateTimeBetween(history_start, history_end).fake();
            let cleaned_at = cleaned_at.format("%Y-%m-%d %H:%M:%S").to_string();
            statement.execute(params![
                item_path,
                if is_file {
                    "recent_file"
                } else {
                    "frequent_folder"
                },
                rule_keyword,
                if index % 3 == 0 { "auto" } else { "manual" },
                cleaned_at,
            ])?;
        }
    }
    let files = count.div_ceil(2);
    let folders = count / 2;
    let count = i64::try_from(count)?;
    let files = i64::try_from(files)?;
    let folders = i64::try_from(folders)?;
    transaction.execute(
        "UPDATE cleanup_totals
         SET cleaned_total = ?1,
             cleaned_files = ?2,
             cleaned_folders = ?3,
             cleanup_runs_total = 0,
             successful_runs = 0,
             partial_runs = 0,
             failed_runs = 0,
             noop_runs = 0,
             interrupted_runs = 0,
             updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now')
         WHERE id = 1",
        params![count, files, folders],
    )?;
    transaction.commit()?;
    Ok(())
}

fn random_history_path(is_file: bool, locale: MockPathLocale) -> (String, String) {
    loop {
        let generated = match locale {
            MockPathLocale::English => fake_history_path(is_file, EnglishPathLocale),
            MockPathLocale::SimplifiedChinese => {
                fake_history_path(is_file, SimplifiedChinesePathLocale)
            }
            MockPathLocale::TraditionalChinese => {
                fake_history_path(is_file, TraditionalChinesePathLocale)
            }
            MockPathLocale::French => fake_history_path(is_file, FrenchPathLocale),
            MockPathLocale::Russian => fake_history_path(is_file, RussianPathLocale),
        };
        let Some(path) = windows_history_path(&generated) else {
            continue;
        };
        if let Some(keyword) = history_path_keyword(&path, is_file) {
            return (path, keyword);
        }
    }
}

fn fake_history_path<L: Data>(is_file: bool, locale: L) -> String {
    if is_file {
        FilePath(locale).fake()
    } else {
        DirPath(locale).fake()
    }
}

fn windows_history_path(generated: &str) -> Option<String> {
    let relative = generated
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>()
        .join("\\");
    (!relative.is_empty()).then(|| format!(r"C:\{relative}"))
}

fn history_path_keyword(path: &str, is_file: bool) -> Option<String> {
    let name = path
        .split(['/', '\\'])
        .rfind(|component| !component.is_empty())?;
    let keyword = if is_file {
        name.rsplit_once('.')
            .map_or(name, |(file_stem, _)| file_stem)
    } else {
        name
    };
    let keyword = keyword.trim_matches([' ', '.']);
    (!keyword.is_empty()).then(|| keyword.to_string())
}

fn random_rule(index: usize) -> NewRule {
    let generated: String = FileName().fake();
    let keyword = generated.trim();
    let keyword = if keyword.is_empty() { "item" } else { keyword };
    let suffix: u32 = (0..1_000_000).fake();
    let scope_index: u8 = (0..3).fake();

    NewRule {
        keyword: format!("{keyword}-{suffix:06}-{index}"),
        rule_type: if Faker.fake() {
            RuleType::Whitelist
        } else {
            RuleType::Blacklist
        },
        scope: match scope_index {
            0 => RuleScope::All,
            1 => RuleScope::Files,
            _ => RuleScope::Folders,
        },
        enabled: Faker.fake(),
    }
}

fn ensure_enabled(backend: &QuickAccessBackendState) -> CommandResult<()> {
    if backend.mode() == BackendMode::Mock {
        Ok(())
    } else {
        Err(CommandError::expected(
            "mock_mode_guard",
            ErrorCode::ValidationInvalidArgument,
            "Mock mode is not enabled.",
            false,
            "mock mode is disabled",
        ))
    }
}

fn mock_error(operation: &str, error: impl std::fmt::Display) -> CommandError {
    CommandError::unexpected(
        operation,
        ErrorCode::InternalUnexpected,
        "The mock operation could not be completed.",
        true,
        error,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn generates_requested_number_of_unique_mock_rules() {
        let rules = random_rules(1_000);

        assert_eq!(rules.len(), 1_000);
        assert!(rules.iter().all(|rule| !rule.keyword.trim().is_empty()));
        assert_eq!(
            rules
                .iter()
                .map(|rule| rule.keyword.as_str())
                .collect::<HashSet<_>>()
                .len(),
            1_000
        );
    }

    #[test]
    fn replaces_existing_rules_with_large_batch() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE rules (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    keyword TEXT NOT NULL,
                    rule_type TEXT NOT NULL,
                    scope TEXT NOT NULL,
                    enabled INTEGER NOT NULL,
                    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
                );
                INSERT INTO rules (keyword, rule_type, scope, enabled)
                VALUES ('existing', 'whitelist', 'all', 1);",
            )
            .unwrap();

        replace_mock_rules(&mut connection, random_rules(1_000)).unwrap();

        let count = connection
            .query_row("SELECT COUNT(*) FROM rules", [], |row| row.get::<_, i64>(0))
            .unwrap();
        assert_eq!(count, 1_000);
    }

    #[test]
    fn replaces_mock_history_with_requested_mix() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        let earliest = Utc::now() - Duration::days(365) - Duration::seconds(1);
        connection
            .execute_batch(
                "CREATE TABLE clean_records (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    item_path TEXT NOT NULL,
                    item_type TEXT NOT NULL,
                    rule_keyword TEXT,
                    source TEXT NOT NULL,
                    cleaned_at TEXT NOT NULL
                );
                CREATE TABLE cleanup_runs (id INTEGER PRIMARY KEY AUTOINCREMENT);
                CREATE TABLE cleanup_totals (
                    id INTEGER PRIMARY KEY,
                    cleaned_total INTEGER NOT NULL,
                    cleaned_files INTEGER NOT NULL,
                    cleaned_folders INTEGER NOT NULL,
                    cleanup_runs_total INTEGER NOT NULL,
                    successful_runs INTEGER NOT NULL,
                    partial_runs INTEGER NOT NULL,
                    failed_runs INTEGER NOT NULL,
                    noop_runs INTEGER NOT NULL,
                    interrupted_runs INTEGER NOT NULL,
                    updated_at TEXT NOT NULL
                );
                INSERT INTO cleanup_totals VALUES (1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 'now');",
            )
            .unwrap();

        replace_mock_history(&mut connection, 21).unwrap();

        let totals = connection
            .query_row(
                "SELECT cleaned_total, cleaned_files, cleaned_folders
                 FROM cleanup_totals WHERE id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(totals, (21, 11, 10));
        let matched = connection
            .query_row(
                "SELECT COUNT(*) FROM clean_records WHERE rule_keyword IS NOT NULL",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(matched, 11);

        let records = connection
            .prepare(
                "SELECT item_path, item_type, rule_keyword, cleaned_at
                 FROM clean_records",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let latest = Utc::now() + Duration::seconds(1);
        assert!(records
            .iter()
            .all(|(path, item_type, keyword, cleaned_at)| {
                let is_file = item_type == "recent_file";
                let keyword_matches_path = keyword.as_ref().is_none_or(|keyword| {
                    history_path_keyword(path, is_file).as_deref() == Some(keyword.as_str())
                });
                let cleaned_at =
                    chrono::NaiveDateTime::parse_from_str(cleaned_at, "%Y-%m-%d %H:%M:%S")
                        .unwrap()
                        .and_utc();
                let windows_path = path.starts_with(r"C:\") && !path.contains('/');
                windows_path
                    && keyword_matches_path
                    && cleaned_at >= earliest
                    && cleaned_at <= latest
            }));
    }

    #[test]
    fn converts_fake_paths_to_windows_paths() {
        assert_eq!(
            windows_history_path("/home/ada/report.csv").as_deref(),
            Some(r"C:\home\ada\report.csv")
        );
        assert_eq!(
            windows_history_path(r"\var\cache\downloads").as_deref(),
            Some(r"C:\var\cache\downloads")
        );
        assert_eq!(windows_history_path("/"), None);
    }

    #[test]
    fn generates_paths_for_every_supported_ui_language() {
        let cases: &[(MockPathLocale, &[&str])] = &[
            (MockPathLocale::English, EnglishPathLocale::PATH_ROOT_DIRS),
            (
                MockPathLocale::SimplifiedChinese,
                SimplifiedChinesePathLocale::PATH_ROOT_DIRS,
            ),
            (
                MockPathLocale::TraditionalChinese,
                TraditionalChinesePathLocale::PATH_ROOT_DIRS,
            ),
            (MockPathLocale::French, FrenchPathLocale::PATH_ROOT_DIRS),
            (MockPathLocale::Russian, RussianPathLocale::PATH_ROOT_DIRS),
        ];

        for (locale, roots) in cases {
            let (path, _) = random_history_path(false, *locale);
            assert!(roots.iter().any(|root| {
                windows_history_path(root)
                    .is_some_and(|windows_root| path.starts_with(&windows_root))
            }));
        }
    }

    #[test]
    fn extracts_history_rule_keywords_from_paths() {
        assert_eq!(
            history_path_keyword(r"C:\Users\Ada\report.csv", true).as_deref(),
            Some("report")
        );
        assert_eq!(
            history_path_keyword(r"C:\Users\Ada\Downloads", false).as_deref(),
            Some("Downloads")
        );
        assert_eq!(history_path_keyword("/", false), None);
    }
}
