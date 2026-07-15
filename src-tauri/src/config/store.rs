use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{AppHandle, Manager, Runtime};

use super::{default_language, normalize_language, Config};

const CONFIG_FILE: &str = "config.json";

pub(crate) fn load<R: Runtime>(app: &AppHandle<R>) -> Result<Config> {
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
    config.validate()?;
    save(app, &config)?;
    Ok(config)
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
