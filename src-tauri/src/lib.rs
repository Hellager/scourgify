//! Windows desktop application for managing Quick Access privacy and cleanup.
//!
//! Scourgify provides the Rust entry point used by its Tauri executable. It
//! initializes the application configuration, SQLite database, Windows Quick
//! Access integration, background monitors, tray, windows, and IPC commands,
//! then runs the desktop event loop until the application exits.
//!
//! # Platform requirements
//!
//! This crate supports Windows 10 and Windows 11 only. It depends on an
//! interactive Windows Explorer session and uses Windows-specific Quick Access
//! APIs through `wincent`. Non-Windows builds fail at compile time.
//!
//! # Side effects and failures
//!
//! Starting the application may create or update `config.json`,
//! `scourgify.db`, and log files in the application configuration directories.
//! It also registers Tauri plugins, starts background workers, creates tray and
//! window resources, and may query or modify Windows Quick Access in response
//! to user actions. Initialization or event-loop failures are fatal and panic.
//!
//! Applications embedding this crate should call [`run`] once from their main
//! thread. The function blocks until the Tauri event loop terminates.
//!
//! # Usage
//!
//! ```no_run
//! scourgify_lib::run();
//! ```

#![deny(missing_docs)]

#[cfg(not(windows))]
compile_error!("Scourgify is Windows-only because wincent targets Windows Quick Access.");

mod app;
mod backend;
mod cleanup;
mod cmd;
mod config;
mod db;
mod error;
#[cfg(debug_assertions)]
mod mock;
mod privacy;
mod quick_access;
mod rules;
mod visualization;

/// Initializes Scourgify and runs its Tauri desktop event loop.
///
/// Call this function once, from the process main thread. It installs the
/// application panic hook, initializes persistent state and Windows
/// integrations, and blocks until the application exits. See the [crate-level
/// documentation](crate) for platform requirements and side effects.
///
/// # Panics
///
/// Panics when Tauri context generation, application initialization, or the
/// desktop event loop fails.
pub fn run() {
    app::run();
}
