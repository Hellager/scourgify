use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

use super::{default_language, normalize_language, Config};

const CONFIG_FILE: &str = "config.json";

pub(crate) fn load<R: Runtime>(app: &AppHandle<R>) -> Result<Config> {
    let path = config_path(app)?;
    let mut config = if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        parse_config(&raw).with_context(|| format!("failed to parse {}", path.display()))?
    } else {
        Config::new(detect_language())
    };

    config.language = normalize_language(&config.language);
    config.validate()?;
    save(app, &config)?;
    Ok(config)
}

fn parse_config(raw: &str) -> Result<Config> {
    let mut value = serde_json::from_str::<Value>(raw)?;
    if value.pointer("/auto_clean/kind").and_then(Value::as_str) == Some("on_startup") {
        value["auto_clean"] = serde_json::json!({ "kind": "disabled" });
        log::warn!("migrated unsupported on_startup auto-clean policy to disabled");
    }
    Ok(serde_json::from_value(value)?)
}

pub(crate) fn save<R: Runtime>(app: &AppHandle<R>, config: &Config) -> Result<()> {
    config.validate()?;
    let path = config_path(app)?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
    }

    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))
}

fn config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(CONFIG_FILE))
}

fn detect_language() -> String {
    tauri_plugin_os::locale()
        .map(|locale| normalize_language(&locale))
        .unwrap_or_else(default_language)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppMode, AutoCleanPolicy};

    #[test]
    fn migrates_removed_config_values() {
        let config = parse_config(
            r#"{
                "app_mode": "minimal",
                "sidebar_variant": "floating",
                "auto_clean": { "kind": "on_startup" }
            }"#,
        )
        .unwrap();

        assert_eq!(config.app_mode, AppMode::Tray);
        assert_eq!(config.auto_clean, AutoCleanPolicy::Disabled);
        assert_eq!(config.history_retention, 0);
        assert!(serde_json::to_value(config)
            .unwrap()
            .get("sidebar_variant")
            .is_none());
    }
}
