use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};
use tempfile::NamedTempFile;

use super::{default_language, normalize_language, Config};

const CONFIG_FILE: &str = "config.json";
const CONFIG_BACKUP_FILE: &str = "config.json.bak";

pub(crate) fn load<R: Runtime>(app: &AppHandle<R>) -> Result<Config> {
    let path = config_path(app)?;
    load_from_path(&path, || Config::new(detect_language()))
}

fn load_from_path(path: &Path, default: impl FnOnce() -> Config) -> Result<Config> {
    let mut config = match load_primary_or_backup(path) {
        Ok(Some(config)) => config,
        Ok(None) => default(),
        Err(error) => {
            log::error!("no usable config or backup found; restoring defaults: {error:#}");
            default()
        }
    };

    config.language = normalize_language(&config.language);
    if !config.privacy_mode_cleanup_links {
        log::info!("enabled fixed privacy-mode .lnk cleanup policy");
    }
    if config.history_retention != 0 {
        log::info!("migrated cleanup history retention to unlimited");
    }
    config.enforce_fixed_cleanup_policies();
    config.validate()?;
    save_to_path(path, &config)?;
    Ok(config)
}

fn load_primary_or_backup(path: &Path) -> Result<Option<Config>> {
    if !path.exists() {
        let backup = backup_path(path);
        if !backup.exists() {
            return Ok(None);
        }
        let config = read_usable_config(&backup)?;
        log::warn!("restoring missing config from backup {}", backup.display());
        return Ok(Some(config));
    }

    match read_usable_config(path) {
        Ok(config) => Ok(Some(config)),
        Err(primary_error) => {
            let backup = backup_path(path);
            let config = read_usable_config(&backup).map_err(|backup_error| {
                anyhow!(
                    "failed to load {}: {primary_error:#}; failed to recover from {}: {backup_error:#}",
                    path.display(),
                    backup.display()
                )
            })?;
            log::warn!(
                "restoring invalid config from backup {}: {primary_error:#}",
                backup.display()
            );
            Ok(Some(config))
        }
    }
}

fn read_config(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_config(&raw).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_usable_config(path: &Path) -> Result<Config> {
    let config = read_config(path)?;
    let mut validated = config.clone();
    validated.language = normalize_language(&validated.language);
    validated.enforce_fixed_cleanup_policies();
    validated
        .validate()
        .with_context(|| format!("invalid config in {}", path.display()))?;
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
    let path = config_path(app)?;
    save_to_path(&path, config)
}

fn save_to_path(path: &Path, config: &Config) -> Result<()> {
    config.validate()?;
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("failed to create {}", directory.display()))?;
    }

    let mut json = serde_json::to_vec_pretty(config)?;
    json.push(b'\n');
    preserve_recovery_config(path, &json)?;
    atomic_write(path, &json)
}

fn preserve_recovery_config(path: &Path, next: &[u8]) -> Result<()> {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return atomic_write(&backup_path(path), next)
                .context("failed to initialize config backup")
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()))
        }
    };
    let Ok(raw_text) = std::str::from_utf8(&raw) else {
        log::warn!("skipping backup of non-UTF-8 config {}", path.display());
        return ensure_valid_backup(path, next);
    };
    let Ok(mut current) = parse_config(raw_text) else {
        log::warn!("skipping backup of invalid config {}", path.display());
        return ensure_valid_backup(path, next);
    };
    current.enforce_fixed_cleanup_policies();
    if let Err(error) = current.validate() {
        log::warn!(
            "skipping backup of invalid config {}: {error:#}",
            path.display()
        );
        return ensure_valid_backup(path, next);
    }

    let backup = backup_path(path);
    atomic_write(&backup, &raw)
        .with_context(|| format!("failed to update config backup {}", backup.display()))
}

fn ensure_valid_backup(path: &Path, next: &[u8]) -> Result<()> {
    let backup = backup_path(path);
    if read_usable_config(&backup).is_ok() {
        return Ok(());
    }
    atomic_write(&backup, next)
        .with_context(|| format!("failed to initialize config backup {}", backup.display()))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    let directory = path
        .parent()
        .context("config target has no parent directory")?;
    let mut temporary = NamedTempFile::new_in(directory).with_context(|| {
        format!(
            "failed to create temporary config in {}",
            directory.display()
        )
    })?;
    temporary
        .write_all(contents)
        .with_context(|| format!("failed to write temporary config for {}", path.display()))?;
    temporary
        .as_file_mut()
        .sync_all()
        .with_context(|| format!("failed to sync temporary config for {}", path.display()))?;
    temporary.persist(path).map_err(|error| {
        anyhow::Error::new(error.error)
            .context(format!("failed to atomically replace {}", path.display()))
    })?;
    Ok(())
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_file_name(CONFIG_BACKUP_FILE)
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

    #[test]
    fn atomic_save_preserves_the_previous_valid_config() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILE);
        let mut original = Config::new("en-US".to_string());
        original.privacy_mode = false;
        save_to_path(&path, &original).unwrap();
        assert_eq!(read_config(&backup_path(&path)).unwrap(), original);

        let mut updated = original.clone();
        updated.privacy_mode = true;
        save_to_path(&path, &updated).unwrap();

        assert_eq!(read_config(&path).unwrap(), updated);
        assert_eq!(read_config(&backup_path(&path)).unwrap(), original);
    }

    #[test]
    fn load_recovers_and_repairs_a_corrupt_primary_config() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILE);
        let original = Config::new("en-US".to_string());
        save_to_path(&path, &original).unwrap();
        let mut updated = original.clone();
        updated.privacy_mode = true;
        save_to_path(&path, &updated).unwrap();
        std::fs::write(&path, b"{\"privacy_mode\":").unwrap();

        let recovered = load_from_path(&path, || Config::new("fr-FR".to_string())).unwrap();

        assert_eq!(recovered, original);
        assert_eq!(read_config(&path).unwrap(), original);
        assert_eq!(read_config(&backup_path(&path)).unwrap(), original);
    }

    #[test]
    fn load_rebuilds_defaults_when_no_valid_config_exists() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(CONFIG_FILE);
        std::fs::write(&path, b"{\"privacy_mode\":").unwrap();

        let recovered = load_from_path(&path, || Config::new("fr-FR".to_string())).unwrap();

        assert_eq!(recovered.language, "fr-FR");
        assert_eq!(read_config(&path).unwrap(), recovered);
        assert_eq!(read_config(&backup_path(&path)).unwrap(), recovered);
    }
}
