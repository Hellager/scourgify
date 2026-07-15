mod store;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const FALLBACK_LANGUAGE: &str = "en-US";

pub(crate) use store::{load, save};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppMode {
    Minimal,
    #[default]
    Dashboard,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CloseBehavior {
    #[default]
    Hide,
    Quit,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SidebarVariant {
    #[default]
    Sidebar,
    Inset,
    Floating,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AutoCleanSchedule {
    #[default]
    Disabled,
    OnStartup,
    EveryHours {
        hours: u32,
    },
    DailyAt {
        hour: u8,
        minute: u8,
    },
}

impl AutoCleanSchedule {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::EveryHours { hours } if !(1..=168).contains(hours) => {
                anyhow::bail!("auto-clean interval must be between 1 and 168 hours")
            }
            Self::DailyAt { hour, .. } if *hour > 23 => {
                anyhow::bail!("auto-clean hour must be between 0 and 23")
            }
            Self::DailyAt { minute, .. } if *minute > 59 => {
                anyhow::bail!("auto-clean minute must be between 0 and 59")
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default)]
    pub app_mode: AppMode,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub privacy_mode: bool,
    #[serde(default = "default_true")]
    pub privacy_mode_cleanup_links: bool,
    #[serde(default)]
    pub close_behavior: CloseBehavior,
    #[serde(default)]
    pub theme: ThemePreference,
    #[serde(default)]
    pub sidebar_variant: SidebarVariant,
    #[serde(default = "default_true")]
    pub show_recent_files: bool,
    #[serde(default = "default_true")]
    pub show_frequent_folders: bool,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default = "default_true")]
    pub notify_operation_complete: bool,
    #[serde(default = "default_true")]
    pub notify_inactive_operation_complete: bool,
    #[serde(default)]
    pub notify_active_operation_complete: bool,
    #[serde(default = "default_true")]
    pub notify_partial_failure: bool,
    #[serde(default = "default_true")]
    pub confirm_destructive_actions: bool,
    #[serde(default = "default_true")]
    pub smart_clean_confirm: bool,
    #[serde(default)]
    pub history_retention: usize,
    #[serde(default)]
    pub auto_clean: AutoCleanSchedule,
    #[serde(default)]
    pub auto_clean_last_run: Option<DateTime<Utc>>,
}

impl Config {
    pub(super) fn new(language: String) -> Self {
        Self {
            app_mode: AppMode::Dashboard,
            language,
            auto_start: false,
            privacy_mode: false,
            privacy_mode_cleanup_links: true,
            close_behavior: CloseBehavior::Hide,
            theme: ThemePreference::System,
            sidebar_variant: SidebarVariant::Sidebar,
            show_recent_files: true,
            show_frequent_folders: true,
            notifications_enabled: true,
            notify_operation_complete: true,
            notify_inactive_operation_complete: true,
            notify_active_operation_complete: false,
            notify_partial_failure: true,
            confirm_destructive_actions: true,
            smart_clean_confirm: true,
            history_retention: 0,
            auto_clean: AutoCleanSchedule::Disabled,
            auto_clean_last_run: None,
        }
    }

    pub fn validate(&self) -> Result<()> {
        self.auto_clean.validate()
    }
}

pub fn normalize_language(language: &str) -> String {
    let tag = language.trim().replace('_', "-").to_ascii_lowercase();
    let parts: Vec<&str> = tag.split('-').filter(|part| !part.is_empty()).collect();

    match parts.first().copied() {
        Some("en") => "en-US".to_string(),
        Some("fr") => "fr-FR".to_string(),
        Some("ru") => "ru-RU".to_string(),
        Some("zh") => {
            if parts
                .iter()
                .any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo"))
            {
                "zh-TW".to_string()
            } else {
                "zh-CN".to_string()
            }
        }
        _ => default_language(),
    }
}

pub(super) fn default_language() -> String {
    FALLBACK_LANGUAGE.to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_languages() {
        assert_eq!(normalize_language("en"), "en-US");
        assert_eq!(normalize_language("fr-FR"), "fr-FR");
        assert_eq!(normalize_language("ru_RU"), "ru-RU");
        assert_eq!(normalize_language("zh-HK"), "zh-TW");
        assert_eq!(normalize_language("zh-Hans-CN"), "zh-CN");
        assert_eq!(normalize_language("unknown"), "en-US");
    }

    #[test]
    fn fills_missing_config_fields() {
        let config: Config = serde_json::from_str(r#"{"language":"zh-TW"}"#).unwrap();

        assert_eq!(config.app_mode, AppMode::Dashboard);
        assert_eq!(config.language, "zh-TW");
        assert!(!config.auto_start);
        assert!(!config.privacy_mode);
        assert!(config.privacy_mode_cleanup_links);
        assert_eq!(config.close_behavior, CloseBehavior::Hide);
        assert_eq!(config.theme, ThemePreference::System);
        assert_eq!(config.sidebar_variant, SidebarVariant::Sidebar);
        assert!(config.show_recent_files);
        assert!(config.show_frequent_folders);
        assert!(config.notifications_enabled);
        assert!(config.notify_operation_complete);
        assert!(config.notify_inactive_operation_complete);
        assert!(!config.notify_active_operation_complete);
        assert!(config.notify_partial_failure);
        assert!(config.confirm_destructive_actions);
        assert!(config.smart_clean_confirm);
        assert_eq!(config.history_retention, 0);
        assert_eq!(config.auto_clean, AutoCleanSchedule::Disabled);
        assert_eq!(config.auto_clean_last_run, None);
    }

    #[test]
    fn validates_auto_clean_schedule_bounds() {
        assert!(AutoCleanSchedule::EveryHours { hours: 1 }
            .validate()
            .is_ok());
        assert!(AutoCleanSchedule::EveryHours { hours: 168 }
            .validate()
            .is_ok());
        assert!(AutoCleanSchedule::DailyAt {
            hour: 23,
            minute: 59,
        }
        .validate()
        .is_ok());
        assert!(AutoCleanSchedule::EveryHours { hours: 0 }
            .validate()
            .is_err());
        assert!(AutoCleanSchedule::EveryHours { hours: 169 }
            .validate()
            .is_err());
        assert!(AutoCleanSchedule::DailyAt {
            hour: 24,
            minute: 0,
        }
        .validate()
        .is_err());
        assert!(AutoCleanSchedule::DailyAt {
            hour: 0,
            minute: 60,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn serializes_auto_clean_schedule_and_last_run() {
        let mut config = Config::new("en-US".to_string());
        config.auto_clean = AutoCleanSchedule::EveryHours { hours: 6 };
        config.auto_clean_last_run = Some("2026-07-13T08:30:00Z".parse().unwrap());

        let value = serde_json::to_value(config).unwrap();

        assert_eq!(
            value["auto_clean"],
            serde_json::json!({ "kind": "every_hours", "hours": 6 })
        );
        assert_eq!(value["auto_clean_last_run"], "2026-07-13T08:30:00Z");
    }

    #[test]
    fn serializes_app_mode_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&AppMode::Dashboard).unwrap(),
            r#""dashboard""#
        );
        assert_eq!(
            serde_json::to_string(&AppMode::Minimal).unwrap(),
            r#""minimal""#
        );
    }

    #[test]
    fn serializes_settings_enums_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&CloseBehavior::Hide).unwrap(),
            r#""hide""#
        );
        assert_eq!(
            serde_json::to_string(&ThemePreference::System).unwrap(),
            r#""system""#
        );
        assert_eq!(
            serde_json::to_string(&SidebarVariant::Floating).unwrap(),
            r#""floating""#
        );
    }

    #[test]
    fn new_config_defaults_to_dashboard_mode() {
        assert_eq!(
            Config::new("en-US".to_string()).app_mode,
            AppMode::Dashboard
        );
    }
}
