use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

const CONFIG_FILE: &str = "config.json";
const FALLBACK_LANGUAGE: &str = "en-US";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub privacy_mode: bool,
    #[serde(default = "default_true")]
    pub privacy_mode_cleanup_links: bool,
}

impl Config {
    fn new(language: String) -> Self {
        Self {
            language,
            auto_start: false,
            privacy_mode: false,
            privacy_mode_cleanup_links: true,
        }
    }
}

pub fn load<R: Runtime>(app: &AppHandle<R>) -> Result<Config> {
    let path = config_path(app)?;
    let mut config = if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        serde_json::from_str::<Config>(&raw)
            .with_context(|| format!("failed to parse {}", path.display()))?
    } else {
        Config::new(detect_language())
    };

    config.language = normalize_language(&config.language);
    save(app, &config)?;
    Ok(config)
}

pub fn save<R: Runtime>(app: &AppHandle<R>, config: &Config) -> Result<()> {
    let path = config_path(app)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }

    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(CONFIG_FILE))
}

pub fn detect_language() -> String {
    tauri_plugin_os::locale()
        .map(|locale| normalize_language(&locale))
        .unwrap_or_else(default_language)
}

pub fn normalize_language(language: &str) -> String {
    let tag = language.trim().replace('_', "-").to_ascii_lowercase();
    let parts: Vec<&str> = tag.split('-').filter(|part| !part.is_empty()).collect();

    match parts.first().copied() {
        Some("en") => "en-US".to_string(),
        Some("fr") => "fr-FR".to_string(),
        Some("ru") => "ru-RU".to_string(),
        Some("zh") => {
            if parts.iter().any(|part| matches!(*part, "hant" | "tw" | "hk" | "mo")) {
                "zh-TW".to_string()
            } else {
                "zh-CN".to_string()
            }
        }
        _ => default_language(),
    }
}

fn default_language() -> String {
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

        assert_eq!(config.language, "zh-TW");
        assert!(!config.auto_start);
        assert!(!config.privacy_mode);
        assert!(config.privacy_mode_cleanup_links);
    }
}
