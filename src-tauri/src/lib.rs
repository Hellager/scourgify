#[cfg(not(windows))]
compile_error!("Scourgify is Windows-only because wincent targets Windows Quick Access.");

mod app;
mod cleanup;
mod cmd;
mod config;
mod db;
mod error;
mod privacy;
mod quick_access;
mod rules;

pub fn run() {
    app::run();
}
