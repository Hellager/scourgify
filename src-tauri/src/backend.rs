use anyhow::{anyhow, Result};
use std::sync::{Arc, RwLock};

use crate::quick_access::{
    operations, QaBatchResult, QaItem, QaItemMetadata, QaRestoreResult, QaVisibility,
};

#[cfg(debug_assertions)]
use crate::mock::MockQuickAccessBackend;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackendMode {
    Real,
    #[cfg(debug_assertions)]
    Mock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathKind {
    Missing,
    File,
    Directory,
}

pub(crate) trait QuickAccessBackend: Send + Sync {
    fn list_items(&self, qa_type: &str) -> Result<Vec<QaItem>>;
    fn list_item_metadata(&self, qa_type: &str) -> Result<Vec<QaItemMetadata>>;
    fn add_item(&self, qa_type: &str, path: &str) -> Result<()>;
    fn remove_items(&self, qa_type: &str, paths: Vec<String>) -> Result<QaBatchResult>;
    fn restore_defaults(&self, qa_type: &str) -> Result<QaRestoreResult>;
    fn get_visibility(&self) -> Result<QaVisibility>;
    fn set_visibility(&self, qa_type: &str, visible: bool) -> Result<QaVisibility>;
    fn path_kind(&self, path: &str) -> PathKind;
}

struct RealQuickAccessBackend;

impl QuickAccessBackend for RealQuickAccessBackend {
    fn list_items(&self, qa_type: &str) -> Result<Vec<QaItem>> {
        operations::list_items(qa_type)
    }

    fn list_item_metadata(&self, qa_type: &str) -> Result<Vec<QaItemMetadata>> {
        operations::list_item_metadata(qa_type)
    }

    fn add_item(&self, qa_type: &str, path: &str) -> Result<()> {
        operations::add_item(qa_type, path)
    }

    fn remove_items(&self, qa_type: &str, paths: Vec<String>) -> Result<QaBatchResult> {
        operations::remove_items(qa_type, paths)
    }

    fn restore_defaults(&self, qa_type: &str) -> Result<QaRestoreResult> {
        operations::restore_defaults(qa_type)
    }

    fn get_visibility(&self) -> Result<QaVisibility> {
        operations::get_visibility()
    }

    fn set_visibility(&self, qa_type: &str, visible: bool) -> Result<QaVisibility> {
        operations::set_visibility(qa_type, visible)
    }

    fn path_kind(&self, path: &str) -> PathKind {
        let path = std::path::Path::new(path);
        if !path.exists() {
            PathKind::Missing
        } else if path.is_file() {
            PathKind::File
        } else if path.is_dir() {
            PathKind::Directory
        } else {
            PathKind::Missing
        }
    }
}

#[derive(Clone)]
pub(crate) struct QuickAccessBackendState {
    inner: Arc<RwLock<BackendSlot>>,
    #[cfg(debug_assertions)]
    mock: Arc<MockQuickAccessBackend>,
}

struct BackendSlot {
    mode: BackendMode,
    backend: Arc<dyn QuickAccessBackend>,
}

impl QuickAccessBackendState {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(BackendSlot {
                mode: BackendMode::Real,
                backend: Arc::new(RealQuickAccessBackend),
            })),
            #[cfg(debug_assertions)]
            mock: Arc::new(MockQuickAccessBackend::new()),
        }
    }

    pub(crate) fn mode(&self) -> BackendMode {
        self.inner
            .read()
            .map(|slot| slot.mode)
            .unwrap_or(BackendMode::Real)
    }

    pub(crate) fn backend(&self) -> Result<Arc<dyn QuickAccessBackend>> {
        self.inner
            .read()
            .map(|slot| slot.backend.clone())
            .map_err(|error| anyhow!("Quick Access backend state is unavailable: {error}"))
    }

    #[cfg(debug_assertions)]
    pub(crate) fn set_real(&self) -> Result<()> {
        let mut slot = self
            .inner
            .write()
            .map_err(|error| anyhow!("Quick Access backend state is unavailable: {error}"))?;
        slot.mode = BackendMode::Real;
        slot.backend = Arc::new(RealQuickAccessBackend);
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(crate) fn set_mock(&self) -> Result<()> {
        let mut slot = self
            .inner
            .write()
            .map_err(|error| anyhow!("Quick Access backend state is unavailable: {error}"))?;
        slot.mode = BackendMode::Mock;
        slot.backend = self.mock.clone();
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(crate) fn mock(&self) -> Arc<MockQuickAccessBackend> {
        self.mock.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_state_switches_to_the_in_memory_backend() {
        let state = QuickAccessBackendState::new();

        state.set_mock().unwrap();

        assert_eq!(state.mode(), BackendMode::Mock);
        assert_eq!(
            state.backend().unwrap().list_items("recent").unwrap().len(),
            2
        );
    }
}
