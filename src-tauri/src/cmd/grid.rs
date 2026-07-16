use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    cleanup,
    db::{history, rules, DatabaseStateError, DbState},
    error::{wincent_command_error, CommandError, CommandResult, ErrorCode},
    quick_access::{QaItem, QuickAccessCache},
    rules::RuleType,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct GridSummary {
    pub recent_files: usize,
    pub quick_access: usize,
    pub frequent_folders: usize,
    pub blacklist_rules: Option<usize>,
    pub to_clean: Option<usize>,
    pub to_clean_recent: Option<usize>,
    pub to_clean_frequent: Option<usize>,
    pub whitelist_rules: Option<usize>,
    pub protected_items: Option<usize>,
    pub cleaned_files: Option<u64>,
    pub cleaned_total: Option<u64>,
    pub cleaned_folders: Option<u64>,
}

#[tauri::command]
pub(crate) fn get_grid_summary(
    app: AppHandle,
    cache: State<'_, QuickAccessCache>,
    database: State<'_, DbState>,
) -> CommandResult<GridSummary> {
    let recent = load_items(&app, &cache, "recent")?;
    let frequent = load_items(&app, &cache, "frequent")?;
    let mut summary = empty_database_summary(&recent, &frequent);

    let database_values = database
        .with_connection(|connection| Ok((rules::list(connection)?, history::totals(connection)?)));
    let (rules, stats) = match database_values {
        Ok(values) => values,
        Err(error) if error.downcast_ref::<DatabaseStateError>().is_some() => return Ok(summary),
        Err(error) => {
            return Err(CommandError::unexpected(
                "get_grid_summary",
                ErrorCode::InternalUnexpected,
                "The grid summary could not be loaded.",
                true,
                error,
            ));
        }
    };

    let recent_matches = cleanup::count_classifications(&recent, &rules);
    let frequent_matches = cleanup::count_classifications(&frequent, &rules);
    summary.blacklist_rules = Some(
        rules
            .iter()
            .filter(|rule| rule.enabled && rule.rule_type == RuleType::Blacklist)
            .count(),
    );
    summary.whitelist_rules = Some(
        rules
            .iter()
            .filter(|rule| rule.enabled && rule.rule_type == RuleType::Whitelist)
            .count(),
    );
    summary.to_clean_recent = Some(recent_matches.targeted);
    summary.to_clean_frequent = Some(frequent_matches.targeted);
    summary.to_clean = Some(recent_matches.targeted + frequent_matches.targeted);
    summary.protected_items = Some(recent_matches.protected + frequent_matches.protected);
    summary.cleaned_files = Some(stats.recent_files);
    summary.cleaned_total = Some(stats.total);
    summary.cleaned_folders = Some(stats.frequent_folders);
    Ok(summary)
}

fn load_items(
    app: &AppHandle,
    cache: &QuickAccessCache,
    qa_type: &str,
) -> CommandResult<Vec<QaItem>> {
    cache
        .items(app, qa_type, false)
        .map_err(|error| wincent_command_error("get_grid_summary", error))
}

fn empty_database_summary(recent: &[QaItem], frequent: &[QaItem]) -> GridSummary {
    GridSummary {
        recent_files: recent.len(),
        quick_access: recent.len() + frequent.len(),
        frequent_folders: frequent.len(),
        blacklist_rules: None,
        to_clean: None,
        to_clean_recent: None,
        to_clean_frequent: None,
        whitelist_rules: None,
        protected_items: None,
        cleaned_files: None,
        cleaned_total: None,
        cleaned_folders: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &str) -> QaItem {
        QaItem {
            path: path.to_string(),
            name: path.to_string(),
            last_interaction_at: None,
            pinned: None,
        }
    }

    #[test]
    fn builds_quick_access_summary_without_database_values() {
        let summary = empty_database_summary(&[item("a"), item("b")], &[item("c")]);

        assert_eq!(summary.recent_files, 2);
        assert_eq!(summary.quick_access, 3);
        assert_eq!(summary.frequent_folders, 1);
        assert_eq!(summary.to_clean, None);
        assert_eq!(summary.cleaned_total, None);
    }
}
