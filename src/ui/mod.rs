//! User interface components
//!
//! System tray integration and settings window using egui.
//!
//! # Features
//!
//! - `tray`: Enable system tray integration (requires GTK3 on Linux)

#[cfg(feature = "tray")]
pub mod tray;

#[cfg(feature = "tray")]
pub use tray::{process_events, IconState, TrayController, TrayError, TrayMenuId, TrayState};

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
