//! Micround - Live microscope camera feed as your desktop wallpaper
//!
//! This is the main entry point for the application.

use anyhow::Result;
use tracing::{info, Level};

mod core;
mod platform;
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

    // 1. Load or create configuration
    let config = config::load_config()?;
    tracing::debug!(?config, "Configuration loaded");

    // 2. Initialize capture backend (TODO: wire into event loop)
    let _capture_backend = capture::create_backend();

    // 3. Initialize render backend (TODO: wire into event loop)
    let _render_backend = render::create_renderer()?;

    // TODO: Initialize application
    // 4. Start UI event loop

    info!("Micround ready");

    Ok(())
}
