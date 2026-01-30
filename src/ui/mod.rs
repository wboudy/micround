//! User interface components
//!
//! System tray integration and settings window using egui.
#![allow(dead_code)] // UI components API
//!
//! # Features
//!
//! - `tray`: Enable system tray integration (requires GTK3 on Linux)

pub mod first_run;

#[cfg(feature = "tray")]
pub mod tray;

// Re-exports for first-run wizard (will be used by settings window integration)
#[allow(unused_imports)]
pub use first_run::{FirstRunState, FirstRunAction, FirstRunEvent, FirstRunController, StateContent};

#[cfg(feature = "tray")]
pub use tray::{TrayController, TrayError, TrayState, TrayMenuId, IconState, process_events};

/// System tray state (stub for when tray feature is disabled)
#[cfg(not(feature = "tray"))]
pub struct TrayState {
    pub is_running: bool,
    pub is_paused: bool,
    pub fps: f32,
    pub resolution: String,
}

/// Initialize the system tray icon and menu
#[cfg(feature = "tray")]
pub fn init_tray() {
    // TODO: Implement tray initialization with event loop integration
}

/// Initialize the system tray (no-op when tray feature is disabled)
#[cfg(not(feature = "tray"))]
pub fn init_tray() {
    tracing::info!("System tray feature not enabled");
}

/// Show the settings window
pub fn show_settings() {
    // TODO: Implement egui settings window
}
