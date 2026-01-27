//! Error types for Micround
//!
//! Uses thiserror for library-style errors that can be matched on.

use thiserror::Error;

/// Errors that can occur during camera capture
#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Camera device not found: {0}")]
    DeviceNotFound(String),

    #[error("Camera device is busy or in use by another application")]
    DeviceBusy,

    #[error("Failed to negotiate capture format: {0}")]
    FormatNegotiationFailed(String),

    #[error("Capture timeout: no frame received within {0}ms")]
    Timeout(u64),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Camera was disconnected")]
    Disconnected,

    #[error("Platform error: {0}")]
    Platform(String),
}

/// Errors that can occur during rendering
#[derive(Error, Debug)]
pub enum RenderError {
    #[error("Failed to create render surface: {0}")]
    SurfaceCreation(String),

    #[error("Display not found: {0}")]
    DisplayNotFound(String),

    #[error("GPU error: {0}")]
    Gpu(String),

    #[error("Wallpaper integration failed: {0}")]
    WallpaperIntegration(String),

    #[error("Platform error: {0}")]
    Platform(String),
}

/// Errors that can occur with configuration
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadFailed(String),

    #[error("Failed to write config file: {0}")]
    WriteFailed(String),

    #[error("Invalid configuration: {0}")]
    Invalid(String),

    #[error("Config file not found at: {0}")]
    NotFound(String),
}
