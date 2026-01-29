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
mod engine;

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

    // 2. Check for crash recovery
    #[cfg(feature = "recovery")]
    {
        let mut recovery = core::recovery::RecoveryManager::new(config.clone());
        let startup_state = recovery.check_and_recover()?;
        if startup_state.was_crash() {
            tracing::warn!("Recovered from crash - wallpaper restored");
        }
    }

    // 3. Initialize capture backend
    let _capture_backend = capture::create_backend();

    // 4. Initialize render backend
    let _render_backend = render::create_renderer()?;

    // 5. Initialize system tray (if feature enabled)
    #[cfg(feature = "tray")]
    {
        use core::events::AppContext;

        let (ctx, _cmd_rx) = AppContext::new();
        let initial_state = ui::TrayState::default();

        match ui::TrayController::new(ctx.handle(), initial_state) {
            Ok(tray) => {
                info!("System tray initialized");
                // TODO: Wire into event loop
                // For now, just log success
                let _ = tray;
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to initialize system tray, continuing without it");
            }
        }
    }

    // TODO: Initialize application event loop
    // 6. Start UI event loop

    info!("Micround ready");

    Ok(())
}
