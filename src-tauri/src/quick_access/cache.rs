use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use tauri::{AppHandle, Emitter, Runtime};
use wincent::prelude::{
    QuickAccess, QuickAccessManager, QuickAccessMonitor, QuickAccessMonitorOptions,
};

use crate::error::report_background_error;

use super::{operations, QaCounts, QaItem};

pub(crate) const QUICK_ACCESS_CHANGED_EVENT: &str = "quick-access-changed";
const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone, Default)]
pub(crate) struct QuickAccessCache {
    inner: Arc<RwLock<CacheSnapshot>>,
}

#[derive(Default)]
struct CacheSnapshot {
    recent: Option<Vec<QaItem>>,
    frequent: Option<Vec<QaItem>>,
    revision: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct QuickAccessChanged {
    pub qa_type: &'static str,
    pub revision: u64,
}

impl QuickAccessCache {
    pub(crate) fn items<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        qa_type: &str,
        fresh: bool,
    ) -> Result<Vec<QaItem>> {
        let qa_type = operations::parse_qa_type(qa_type)?;
        match qa_type {
            QuickAccess::RecentFiles | QuickAccess::FrequentFolders => {
                self.category_items(app, qa_type, fresh)
            }
            QuickAccess::All => {
                let mut items = self.category_items(app, QuickAccess::RecentFiles, fresh)?;
                items.extend(self.category_items(app, QuickAccess::FrequentFolders, fresh)?);
                Ok(items)
            }
            _ => unreachable!("wincent QuickAccess gained an unsupported variant"),
        }
    }

    pub(crate) fn counts<R: Runtime>(&self, app: &AppHandle<R>, fresh: bool) -> Result<QaCounts> {
        let recent = self
            .category_items(app, QuickAccess::RecentFiles, fresh)?
            .len();
        let frequent = self
            .category_items(app, QuickAccess::FrequentFolders, fresh)?
            .len();
        Ok(QaCounts {
            recent,
            frequent,
            all: recent + frequent,
        })
    }

    pub(crate) fn refresh_after_write<R: Runtime>(&self, app: &AppHandle<R>, qa_type: &str) {
        if let Err(error) = self.items(app, qa_type, true) {
            let incident_id = report_background_error("refresh_quick_access_cache", error);
            log::warn!(
                "Quick Access cache refresh after write failed qa_type={qa_type} incident_id={incident_id}"
            );
        }
    }

    fn category_items<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        qa_type: QuickAccess,
        fresh: bool,
    ) -> Result<Vec<QaItem>> {
        if !fresh {
            if let Some(items) = self.cached(qa_type)? {
                log::debug!("Quick Access cache hit qa_type={}", qa_name(qa_type));
                return Ok(items);
            }
        }

        log::debug!("Quick Access cache refresh qa_type={}", qa_name(qa_type));
        let items = operations::list_items(qa_name(qa_type))?;
        self.update(app, qa_type, items.clone())?;
        Ok(items)
    }

    fn cached(&self, qa_type: QuickAccess) -> Result<Option<Vec<QaItem>>> {
        let snapshot = self
            .inner
            .read()
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .context("failed to read Quick Access cache")?;
        Ok(match qa_type {
            QuickAccess::RecentFiles => snapshot.recent.clone(),
            QuickAccess::FrequentFolders => snapshot.frequent.clone(),
            _ => None,
        })
    }

    fn update<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        qa_type: QuickAccess,
        items: Vec<QaItem>,
    ) -> Result<()> {
        let event = self.store(qa_type, items)?;
        if let Some(event) = event {
            log::info!(
                "Quick Access cache updated qa_type={} revision={}",
                event.qa_type,
                event.revision
            );
            if let Err(error) = app.emit(QUICK_ACCESS_CHANGED_EVENT, event) {
                log::warn!("failed to emit Quick Access cache update: {error}");
            }
        }
        Ok(())
    }

    fn store(
        &self,
        qa_type: QuickAccess,
        items: Vec<QaItem>,
    ) -> Result<Option<QuickAccessChanged>> {
        let mut snapshot = self
            .inner
            .write()
            .map_err(|error| anyhow::anyhow!(error.to_string()))
            .context("failed to update Quick Access cache")?;
        let slot = match qa_type {
            QuickAccess::RecentFiles => &mut snapshot.recent,
            QuickAccess::FrequentFolders => &mut snapshot.frequent,
            _ => anyhow::bail!("Quick Access cache only stores individual categories"),
        };
        let initialized = slot.is_some();
        if slot.as_ref() == Some(&items) {
            return Ok(None);
        }
        *slot = Some(items);
        snapshot.revision = snapshot.revision.wrapping_add(1);

        Ok(initialized.then_some(QuickAccessChanged {
            qa_type: qa_name(qa_type),
            revision: snapshot.revision,
        }))
    }

    fn update_from_paths<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        qa_type: QuickAccess,
        paths: Vec<String>,
    ) -> Result<()> {
        self.update(app, qa_type, operations::items_from_paths(qa_type, paths))
    }
}

pub(crate) struct QuickAccessWatchers {
    _monitors: Vec<QuickAccessMonitor>,
}

impl QuickAccessWatchers {
    pub(crate) fn start(app: AppHandle, cache: QuickAccessCache) -> Self {
        let mut monitors = Vec::with_capacity(2);
        for qa_type in [QuickAccess::RecentFiles, QuickAccess::FrequentFolders] {
            match start_monitor(app.clone(), cache.clone(), qa_type) {
                Ok(monitor) => monitors.push(monitor),
                Err(error) => {
                    let incident_id = report_background_error("start_quick_access_monitor", error);
                    log::warn!(
                        "Quick Access monitor unavailable qa_type={} incident_id={incident_id}",
                        qa_name(qa_type)
                    );
                }
            }
        }
        log::info!(
            "Quick Access monitoring started interval_secs={} monitors={}",
            POLL_INTERVAL.as_secs(),
            monitors.len()
        );
        Self {
            _monitors: monitors,
        }
    }
}

fn start_monitor(
    app: AppHandle,
    cache: QuickAccessCache,
    qa_type: QuickAccess,
) -> Result<QuickAccessMonitor> {
    let options = QuickAccessMonitorOptions::new()
        .with_qa_type(qa_type)
        .try_poll_interval(POLL_INTERVAL)?;
    let error_reported = Arc::new(AtomicBool::new(false));
    Ok(
        QuickAccessManager::new().watch_quick_access(options, move |result| match result {
            Ok(event) => {
                error_reported.store(false, Ordering::Relaxed);
                if let Err(error) =
                    cache.update_from_paths(&app, qa_type, event.current_items().to_vec())
                {
                    report_background_error("update_quick_access_cache", error);
                }
            }
            Err(error) if !error_reported.swap(true, Ordering::Relaxed) => {
                report_background_error("watch_quick_access", error);
            }
            Err(_) => {}
        })?,
    )
}

fn qa_name(qa_type: QuickAccess) -> &'static str {
    match qa_type {
        QuickAccess::RecentFiles => "recent",
        QuickAccess::FrequentFolders => "frequent",
        QuickAccess::All => "all",
        _ => "unknown",
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
    fn cache_emits_only_after_an_initialized_category_changes() {
        let cache = QuickAccessCache::default();

        assert_eq!(
            cache
                .store(QuickAccess::RecentFiles, vec![item("first")])
                .unwrap(),
            None
        );
        assert_eq!(
            cache.cached(QuickAccess::RecentFiles).unwrap(),
            Some(vec![item("first")])
        );
        assert_eq!(
            cache
                .store(QuickAccess::RecentFiles, vec![item("first")])
                .unwrap(),
            None
        );
        assert_eq!(
            cache
                .store(QuickAccess::RecentFiles, vec![item("second")])
                .unwrap(),
            Some(QuickAccessChanged {
                qa_type: "recent",
                revision: 2,
            })
        );
    }

    #[test]
    fn monitor_uses_three_second_polling() {
        assert_eq!(POLL_INTERVAL, Duration::from_secs(3));
    }
}
