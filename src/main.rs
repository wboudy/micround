//! Micround - Live microscope camera feed as your desktop wallpaper
//!
//! This is the main entry point for the application.

use anyhow::Result;
use tracing::{info, Level};

mod core;
mod capture;
mod process;
mod render;
mod ui;
mod config;

fn main() -> Result<()> {
    // Initialize logging with file output and rotation
    core::logging::init(Some(Level::INFO))?;

    info!("Micround starting...");
    info!(
        version = env!("CARGO_PKG_VERSION"),
        log_dir = %core::logging::log_directory().display(),
        "Application initialized"
    );

    // TODO: Initialize application
    // 1. Load or create configuration
    // 2. Initialize capture backend
    // 3. Initialize render backend
    // 4. Start UI event loop

    info!("Micround ready");

    Ok(())
}
