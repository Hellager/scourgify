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

pub fn run() {
    app::run();
}
