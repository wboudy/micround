//! Error types and handling strategy for Micround
//!
//! This module defines the error taxonomy, representation, and handling patterns.
//!
//! # Error Categories
//!
//! - **Recoverable**: Retry automatically, log, continue (e.g., frame timeout)
//! - **UserActionable**: Notify user, suggest action (e.g., camera permission denied)
//! - **Fatal**: Log, restore safe state, exit gracefully (e.g., GPU driver failure)
//!
//! # Design Principles
//!
//! - Rich context for debugging (component, operation, device)
//! - User-friendly messages separate from technical details
//! - Chain/cause support for wrapped errors
//! - Consistent logging integration

use std::fmt;
use thiserror::Error;

use super::messages::{self, UserMessage};

// ============================================================================
// Error Severity
// ============================================================================

/// Severity classification for errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// Retry automatically, log at debug/info level
    Recoverable,
    /// Notify user with actionable guidance
    UserActionable,
    /// Log, restore safe state, may need to exit
    Fatal,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recoverable => write!(f, "recoverable"),
            Self::UserActionable => write!(f, "user-actionable"),
            Self::Fatal => write!(f, "fatal"),
        }
    }
}

// ============================================================================
// Error Context
// ============================================================================

/// Rich context for debugging errors
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// Component where error originated
    pub component: Option<String>,
    /// Operation being performed
    pub operation: Option<String>,
    /// Device ID involved (if applicable)
    pub device_id: Option<String>,
    /// Display ID involved (if applicable)
    pub display_id: Option<String>,
    /// Additional key-value context
    pub extra: Vec<(String, String)>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    pub fn operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    pub fn device(mut self, device_id: impl Into<String>) -> Self {
        self.device_id = Some(device_id.into());
        self
    }

    pub fn display(mut self, display_id: impl Into<String>) -> Self {
        self.display_id = Some(display_id.into());
        self
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra.push((key.into(), value.into()));
        self
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();

        if let Some(ref c) = self.component {
            parts.push(format!("component={}", c));
        }
        if let Some(ref o) = self.operation {
            parts.push(format!("operation={}", o));
        }
        if let Some(ref d) = self.device_id {
            parts.push(format!("device={}", d));
        }
        if let Some(ref d) = self.display_id {
            parts.push(format!("display={}", d));
        }
        for (k, v) in &self.extra {
            parts.push(format!("{}={}", k, v));
        }

        write!(f, "{}", parts.join(", "))
    }
}

// ============================================================================
// Top-Level Error Type
// ============================================================================

/// Top-level error type for Micround application
#[derive(Error, Debug, Clone)]
pub enum MicroundError {
    #[error("Capture error: {source}")]
    Capture {
        #[source]
        source: CaptureError,
        context: ErrorContext,
    },

    #[error("Render error: {source}")]
    Render {
        #[source]
        source: RenderError,
        context: ErrorContext,
    },

    #[error("Configuration error: {source}")]
    Config {
        #[source]
        source: ConfigError,
        context: ErrorContext,
    },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
        context: ErrorContext,
    },
}

impl MicroundError {
    /// Get the severity of this error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Capture { source, .. } => source.severity(),
            Self::Render { source, .. } => source.severity(),
            Self::Config { source, .. } => source.severity(),
            Self::Internal { .. } => ErrorSeverity::Fatal,
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::Capture { source, .. } => source.user_message(),
            Self::Render { source, .. } => source.user_message(),
            Self::Config { source, .. } => source.user_message(),
            Self::Internal { message, .. } => {
                format!("An unexpected error occurred. Please restart the application. ({})", message)
            }
        }
    }

    /// Get a full user message with recovery actions and error code
    pub fn user_message_full(&self) -> UserMessage {
        match self {
            Self::Capture { source, .. } => source.user_message_full(),
            Self::Render { source, .. } => source.user_message_full(),
            Self::Config { source, .. } => source.user_message_full(),
            Self::Internal { message, .. } => messages::internal::unexpected(message),
        }
    }

    /// Get the error context
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::Capture { context, .. }
            | Self::Render { context, .. }
            | Self::Config { context, .. }
            | Self::Internal { context, .. } => context,
        }
    }

    /// Log this error at the appropriate level
    pub fn log(&self) {
        let ctx = self.context();
        match self.severity() {
            ErrorSeverity::Recoverable => {
                tracing::debug!(
                    error = %self,
                    context = %ctx,
                    severity = %self.severity(),
                    "Recoverable error"
                );
            }
            ErrorSeverity::UserActionable => {
                tracing::warn!(
                    error = %self,
                    context = %ctx,
                    severity = %self.severity(),
                    user_message = %self.user_message(),
                    "User-actionable error"
                );
            }
            ErrorSeverity::Fatal => {
                tracing::error!(
                    error = %self,
                    context = %ctx,
                    severity = %self.severity(),
                    "Fatal error"
                );
            }
        }
    }
}

// ============================================================================
// Capture Errors
// ============================================================================

/// Errors that can occur during camera capture
#[derive(Error, Debug, Clone)]
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

    #[error("No cameras available")]
    NoCameras,

    #[error("Platform error: {0}")]
    Platform(String),
}

impl CaptureError {
    /// Get the severity of this capture error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Timeout(_) | Self::Disconnected => ErrorSeverity::Recoverable,
            Self::DeviceNotFound(_) | Self::DeviceBusy | Self::NoCameras |
            Self::PermissionDenied(_) | Self::FormatNegotiationFailed(_) => {
                ErrorSeverity::UserActionable
            }
            Self::Platform(_) => ErrorSeverity::Fatal,
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::DeviceNotFound(name) => {
                format!("Camera '{}' is not available. Please check the connection.", name)
            }
            Self::DeviceBusy => {
                "The camera is being used by another application. Please close other apps using the camera.".into()
            }
            Self::FormatNegotiationFailed(_) => {
                "Unable to use this camera's video format. Try a different camera or resolution.".into()
            }
            Self::Timeout(_) => {
                "Camera is not responding. Attempting to reconnect...".into()
            }
            Self::PermissionDenied(_) => {
                "Camera access was denied. Please grant camera permission in system settings.".into()
            }
            Self::Disconnected => {
                "Camera was disconnected. Attempting to reconnect...".into()
            }
            Self::NoCameras => {
                "No cameras found. Please connect a camera and try again.".into()
            }
            Self::Platform(msg) => {
                format!("A system error occurred with the camera: {}", msg)
            }
        }
    }

    /// Get a full user message with recovery actions and error code
    pub fn user_message_full(&self) -> UserMessage {
        match self {
            Self::DeviceNotFound(name) => messages::camera::camera_not_found(name),
            Self::DeviceBusy => messages::camera::camera_busy(),
            Self::FormatNegotiationFailed(details) => {
                messages::camera::format_negotiation_failed(details)
            }
            Self::Timeout(_) => messages::camera::timeout(),
            Self::PermissionDenied(_) => messages::camera::permission_denied(),
            Self::Disconnected => messages::camera::disconnected(),
            Self::NoCameras => messages::camera::no_camera_found(),
            Self::Platform(msg) => messages::platform::error(msg),
        }
    }

    /// Create a MicroundError with context
    pub fn with_context(self, context: ErrorContext) -> MicroundError {
        MicroundError::Capture {
            source: self,
            context,
        }
    }
}

// ============================================================================
// Render Errors
// ============================================================================

/// Errors that can occur during rendering
#[derive(Error, Debug, Clone)]
pub enum RenderError {
    #[error("Failed to create render surface: {0}")]
    SurfaceCreation(String),

    #[error("Display not found: {0}")]
    DisplayNotFound(String),

    #[error("GPU error: {0}")]
    Gpu(String),

    #[error("Wallpaper integration failed: {0}")]
    WallpaperIntegration(String),

    #[error("Frame processing failed: {0}")]
    FrameProcessing(String),

    #[error("Platform error: {0}")]
    Platform(String),
}

impl RenderError {
    /// Get the severity of this render error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::FrameProcessing(_) => ErrorSeverity::Recoverable,
            Self::DisplayNotFound(_) | Self::WallpaperIntegration(_) => {
                ErrorSeverity::UserActionable
            }
            Self::SurfaceCreation(_) | Self::Gpu(_) | Self::Platform(_) => {
                ErrorSeverity::Fatal
            }
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::SurfaceCreation(_) => {
                "Unable to create display surface. Please check your graphics drivers.".into()
            }
            Self::DisplayNotFound(name) => {
                format!("Display '{}' is not available. Please check your display settings.", name)
            }
            Self::Gpu(_) => {
                "A graphics error occurred. Please update your graphics drivers.".into()
            }
            Self::WallpaperIntegration(_) => {
                "Unable to set wallpaper. Your desktop environment may not support this feature.".into()
            }
            Self::FrameProcessing(_) => {
                "Error processing video frame. Skipping frame...".into()
            }
            Self::Platform(msg) => {
                format!("A system error occurred: {}", msg)
            }
        }
    }

    /// Get a full user message with recovery actions and error code
    pub fn user_message_full(&self) -> UserMessage {
        match self {
            Self::SurfaceCreation(details) => messages::display::surface_creation_failed(details),
            Self::DisplayNotFound(name) => messages::display::not_found(name),
            Self::Gpu(details) => messages::display::gpu_error(details),
            Self::WallpaperIntegration(details) => messages::display::wallpaper_failed(details),
            Self::FrameProcessing(_) => messages::feed::processing_error(),
            Self::Platform(msg) => messages::platform::error(msg),
        }
    }

    /// Create a MicroundError with context
    pub fn with_context(self, context: ErrorContext) -> MicroundError {
        MicroundError::Render {
            source: self,
            context,
        }
    }
}

// ============================================================================
// Config Errors
// ============================================================================

/// Errors that can occur with configuration
#[derive(Error, Debug, Clone)]
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

impl ConfigError {
    /// Get the severity of this config error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::NotFound(_) => ErrorSeverity::Recoverable, // Use defaults
            Self::ReadFailed(_) | Self::WriteFailed(_) | Self::Invalid(_) => {
                ErrorSeverity::UserActionable
            }
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::ReadFailed(_) => {
                "Unable to read settings. Using default configuration.".into()
            }
            Self::WriteFailed(_) => {
                "Unable to save settings. Check that you have write permissions.".into()
            }
            Self::Invalid(_) => {
                "Settings file is corrupted. Using default configuration.".into()
            }
            Self::NotFound(_) => {
                "Settings file not found. Using default configuration.".into()
            }
        }
    }

    /// Get a full user message with recovery actions and error code
    pub fn user_message_full(&self) -> UserMessage {
        match self {
            Self::ReadFailed(_) => messages::config::read_failed(),
            Self::WriteFailed(_) => messages::config::save_failed(),
            Self::Invalid(_) => messages::config::corrupted(),
            Self::NotFound(_) => messages::config::not_found(),
        }
    }

    /// Create a MicroundError with context
    pub fn with_context(self, context: ErrorContext) -> MicroundError {
        MicroundError::Config {
            source: self,
            context,
        }
    }
}

// ============================================================================
// Platform Errors
// ============================================================================

/// Errors related to platform-specific operations
#[derive(Error, Debug, Clone)]
pub enum PlatformError {
    #[error("Operation not supported on this platform: {0}")]
    Unsupported(String),

    #[error("Platform command failed: {0}")]
    CommandFailed(String),

    #[error("Invalid platform state: {0}")]
    InvalidState(String),

    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

impl PlatformError {
    /// Get the severity of this platform error
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Unsupported(_) => ErrorSeverity::UserActionable,
            Self::CommandFailed(_) => ErrorSeverity::Recoverable,
            Self::InvalidState(_) => ErrorSeverity::UserActionable,
            Self::ResourceNotFound(_) => ErrorSeverity::UserActionable,
            Self::PermissionDenied(_) => ErrorSeverity::UserActionable,
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::Unsupported(msg) => {
                format!("This feature is not available on your system: {}", msg)
            }
            Self::CommandFailed(_) => {
                "A system command failed. Please check your desktop environment configuration.".into()
            }
            Self::InvalidState(msg) => {
                format!("Operation cannot be completed: {}", msg)
            }
            Self::ResourceNotFound(msg) => {
                format!("Required resource not found: {}", msg)
            }
            Self::PermissionDenied(_) => {
                "Permission denied. Please check your system settings.".into()
            }
        }
    }

    /// Get a full user message with recovery actions and error code
    pub fn user_message_full(&self) -> UserMessage {
        match self {
            Self::Unsupported(feature) => messages::platform::unsupported(feature),
            Self::CommandFailed(details) => messages::platform::error(details),
            Self::InvalidState(msg) => messages::platform::error(msg),
            Self::ResourceNotFound(resource) => messages::platform::error(resource),
            Self::PermissionDenied(msg) => messages::platform::error(msg),
        }
    }
}

// ============================================================================
// Result Type Aliases
// ============================================================================

/// Result type for Micround operations
pub type Result<T> = std::result::Result<T, MicroundError>;

/// Result type for platform operations
pub type PlatformResult<T> = std::result::Result<T, PlatformError>;

/// Result type for capture operations
pub type CaptureResult<T> = std::result::Result<T, CaptureError>;

/// Result type for render operations
pub type RenderResult<T> = std::result::Result<T, RenderError>;

/// Result type for config operations
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

// ============================================================================
// Error Extension Trait
// ============================================================================

/// Extension trait for adding context to errors
pub trait ErrorExt<T> {
    /// Add context to an error
    fn with_context(self, context: ErrorContext) -> Result<T>;
}

impl<T> ErrorExt<T> for CaptureResult<T> {
    fn with_context(self, context: ErrorContext) -> Result<T> {
        self.map_err(|e| e.with_context(context))
    }
}

impl<T> ErrorExt<T> for RenderResult<T> {
    fn with_context(self, context: ErrorContext) -> Result<T> {
        self.map_err(|e| e.with_context(context))
    }
}

impl<T> ErrorExt<T> for ConfigResult<T> {
    fn with_context(self, context: ErrorContext) -> Result<T> {
        self.map_err(|e| e.with_context(context))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_error_severity() {
        assert_eq!(CaptureError::Timeout(1000).severity(), ErrorSeverity::Recoverable);
        assert_eq!(CaptureError::PermissionDenied("camera".into()).severity(), ErrorSeverity::UserActionable);
        assert_eq!(CaptureError::Platform("unknown".into()).severity(), ErrorSeverity::Fatal);
    }

    #[test]
    fn test_error_context_display() {
        let ctx = ErrorContext::new()
            .component("capture")
            .operation("start")
            .device("camera-1");

        let display = format!("{}", ctx);
        assert!(display.contains("component=capture"));
        assert!(display.contains("operation=start"));
        assert!(display.contains("device=camera-1"));
    }

    #[test]
    fn test_user_message_is_helpful() {
        let err = CaptureError::PermissionDenied("video0".into());
        let msg = err.user_message();
        assert!(msg.contains("permission"));
        assert!(msg.contains("system settings"));
    }

    #[test]
    fn test_with_context() {
        let err = CaptureError::NoCameras;
        let ctx = ErrorContext::new().component("capture");
        let full_err = err.with_context(ctx);

        assert!(matches!(full_err, MicroundError::Capture { .. }));
        assert_eq!(full_err.severity(), ErrorSeverity::UserActionable);
    }
}
