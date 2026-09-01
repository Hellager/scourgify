//! Defines the Windows Quick Access boundary and its test backend.
//!
//! [`QuickAccessBackend`] separates cleanup and IPC code from `wincent` and the
//! debug mock. Operations normalize Windows results into serializable DTOs,
//! preserving per-item failures and post-mutation warnings instead of treating
//! a batch as all-or-nothing. The cache accelerates reads but is not the source
//! of truth for mutations.

mod backend;
mod cache;
pub(crate) mod operations;
mod types;

pub(crate) use backend::{BackendMode, PathKind, QuickAccessBackend, QuickAccessBackendState};
pub(crate) use types::QaVisibilityMutationResult;

pub(crate) use cache::{QuickAccessCache, QuickAccessWatcherStatus, QuickAccessWatchers};
pub(crate) use types::*;
