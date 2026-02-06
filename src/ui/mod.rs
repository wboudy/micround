//! User interface components
//!
//! System tray integration, settings window, and first-run experience using egui.
//!
//! # Features
//!
//! - `tray`: Enable system tray integration (requires GTK3 on Linux)
//!
//! # Settings Window
//!
//! The settings window provides a cross-platform UI for configuring:
//! - Camera selection and capture settings
//! - Display target and transformations
//! - Startup behavior
//!
//! # First-Run Experience
//!
//! The first-run wizard guides new users through:
//! - Camera detection and selection
//! - Permission handling (macOS)
//! - Quick setup to wallpaper

pub mod first_run;
pub mod settings;

#[cfg(feature = "tray")]
pub mod tray;

#[cfg(feature = "tray")]
pub use tray::{process_events, IconState, TrayController, TrayError, TrayMenuId, TrayState};

pub use first_run::{FirstRunCameraInfo, FirstRunStep, FirstRunWizard};
pub use settings::{CameraInfo, DisplayInfo, PreviewConfig, PreviewFrame, PreviewState, SettingsWindow};

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
