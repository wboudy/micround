//! User-facing error messages and recovery guidance
//!
//! This module centralizes all user-visible error messages, following these principles:
#![allow(dead_code)] // Complete API for future UI integration
//!
//! 1. **Tell users what happened** (in plain language)
//! 2. **Tell users what to do** (specific action)
//! 3. **Don't blame the user**
//! 4. **Don't use technical jargon**
//!
//! # Design
//!
//! - Message templates stored in one place (localization-ready)
//! - Error codes for support purposes (small, optional "Details")
//! - Recovery actions with clear next steps
//! - Friendly but not cute tone

use std::fmt;

// ============================================================================
// Recovery Actions
// ============================================================================

/// An action the user can take to recover from an error
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryAction {
    /// Short label for the action (e.g., "Refresh", "Settings")
    pub label: String,
    /// Identifier for the action (for handling in UI)
    pub action_id: RecoveryActionId,
    /// Whether this is the primary/recommended action
    pub primary: bool,
}

/// Identifiers for recovery actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecoveryActionId {
    /// Refresh the camera list
    RefreshCameras,
    /// Open system settings (camera permissions, etc.)
    OpenSettings,
    /// Retry the failed operation
    Retry,
    /// Restart the feed
    RestartFeed,
    /// Select a different camera
    SelectCamera,
    /// Dismiss the error notification
    Dismiss,
    /// Open help/troubleshooting documentation
    OpenHelp,
    /// Check display settings
    CheckDisplaySettings,
    /// Update graphics drivers
    UpdateDrivers,
    /// Close other applications
    CloseOtherApps,
}

impl RecoveryAction {
    /// Create a primary recovery action
    pub fn primary(label: impl Into<String>, action_id: RecoveryActionId) -> Self {
        Self {
            label: label.into(),
            action_id,
            primary: true,
        }
    }

    /// Create a secondary recovery action
    pub fn secondary(label: impl Into<String>, action_id: RecoveryActionId) -> Self {
        Self {
            label: label.into(),
            action_id,
            primary: false,
        }
    }
}

// ============================================================================
// User Message
// ============================================================================

/// A complete user-facing message with recovery options
#[derive(Debug, Clone)]
pub struct UserMessage {
    /// The main message to display
    pub message: String,
    /// Optional detailed explanation (shown on expand)
    pub details: Option<String>,
    /// Error code for support purposes (e.g., "MIC-CAM-001")
    pub error_code: Option<String>,
    /// Available recovery actions
    pub actions: Vec<RecoveryAction>,
}

impl UserMessage {
    /// Create a new user message
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            details: None,
            error_code: None,
            actions: Vec::new(),
        }
    }

    /// Add details (technical info for support)
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Add an error code
    pub fn with_error_code(mut self, code: impl Into<String>) -> Self {
        self.error_code = Some(code.into());
        self
    }

    /// Add a recovery action
    pub fn with_action(mut self, action: RecoveryAction) -> Self {
        self.actions.push(action);
        self
    }

    /// Add multiple recovery actions
    pub fn with_actions(mut self, actions: impl IntoIterator<Item = RecoveryAction>) -> Self {
        self.actions.extend(actions);
        self
    }
}

impl fmt::Display for UserMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(ref code) = self.error_code {
            write!(f, " ({})", code)?;
        }
        Ok(())
    }
}

// ============================================================================
// Message Templates - Camera Errors
// ============================================================================

/// Messages for camera-related errors
pub mod camera {
    use super::*;

    /// No camera detected
    pub fn no_camera_found() -> UserMessage {
        UserMessage::new("No camera detected. Connect your USB microscope and click Refresh.")
            .with_error_code("MIC-CAM-001")
            .with_action(RecoveryAction::primary("Refresh", RecoveryActionId::RefreshCameras))
            .with_action(RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp))
    }

    /// Specific camera not found
    pub fn camera_not_found(name: &str) -> UserMessage {
        UserMessage::new(format!(
            "Camera '{}' is not available. It may have been disconnected.",
            name
        ))
        .with_error_code("MIC-CAM-002")
        .with_action(RecoveryAction::primary("Refresh", RecoveryActionId::RefreshCameras))
        .with_action(RecoveryAction::secondary("Select Camera", RecoveryActionId::SelectCamera))
    }

    /// Camera in use by another application
    pub fn camera_busy() -> UserMessage {
        UserMessage::new(
            "Camera is being used by another app. Close other apps using the camera and try again.",
        )
        .with_error_code("MIC-CAM-003")
        .with_action(RecoveryAction::primary("Retry", RecoveryActionId::Retry))
        .with_action(RecoveryAction::secondary("Close Other Apps", RecoveryActionId::CloseOtherApps))
    }

    /// Permission denied (generic)
    pub fn permission_denied() -> UserMessage {
        UserMessage::new(
            "Camera access needed. Click Settings to open Privacy settings and allow Micround to use your camera.",
        )
        .with_error_code("MIC-CAM-004")
        .with_action(RecoveryAction::primary("Settings", RecoveryActionId::OpenSettings))
        .with_action(RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp))
    }

    /// Permission denied with Linux-specific guidance
    pub fn permission_denied_linux(device_path: &str, in_video_group: bool) -> UserMessage {
        if in_video_group {
            UserMessage::new(format!(
                "Permission denied for '{}'. Check device permissions.",
                device_path
            ))
            .with_error_code("MIC-CAM-004")
            .with_details(format!("Try: sudo chmod 666 {}", device_path))
            .with_action(RecoveryAction::primary("Retry", RecoveryActionId::Retry))
            .with_action(RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp))
        } else {
            UserMessage::new(format!(
                "Permission denied for '{}'. Your user account needs camera access.",
                device_path
            ))
            .with_error_code("MIC-CAM-004")
            .with_details("Run: sudo usermod -a -G video $USER (then log out and back in)")
            .with_action(RecoveryAction::primary("Help", RecoveryActionId::OpenHelp))
        }
    }

    /// Camera disconnected during capture
    pub fn disconnected() -> UserMessage {
        UserMessage::new("Camera disconnected. Waiting to reconnect...")
            .with_error_code("MIC-CAM-005")
            .with_action(RecoveryAction::secondary("Select Camera", RecoveryActionId::SelectCamera))
    }

    /// Camera not responding (timeout)
    pub fn timeout() -> UserMessage {
        UserMessage::new("Camera is not responding. Attempting to reconnect...")
            .with_error_code("MIC-CAM-006")
            .with_action(RecoveryAction::secondary("Restart Feed", RecoveryActionId::RestartFeed))
    }

    /// Failed to negotiate format
    pub fn format_negotiation_failed(details: &str) -> UserMessage {
        UserMessage::new(
            "Unable to use this camera's video format. Try a different camera or resolution.",
        )
        .with_error_code("MIC-CAM-007")
        .with_details(details.to_string())
        .with_action(RecoveryAction::primary("Select Camera", RecoveryActionId::SelectCamera))
        .with_action(RecoveryAction::secondary("Settings", RecoveryActionId::OpenSettings))
    }

    /// Camera reconnected after disconnect (success message)
    pub fn reconnected() -> UserMessage {
        UserMessage::new("Camera reconnected. Feed resumed.")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
    }

    /// Reconnection failed after timeout
    pub fn reconnection_failed() -> UserMessage {
        UserMessage::new("Camera not found after waiting. Please check the connection.")
            .with_error_code("MIC-CAM-008")
            .with_action(RecoveryAction::primary("Try Again", RecoveryActionId::Retry))
            .with_action(RecoveryAction::secondary("Select Camera", RecoveryActionId::SelectCamera))
    }
}

// ============================================================================
// Message Templates - Display Errors
// ============================================================================

/// Messages for display-related errors
pub mod display {
    use super::*;

    /// Display not found
    pub fn not_found(name: &str) -> UserMessage {
        UserMessage::new(format!(
            "Display '{}' is not available. It may have been disconnected.",
            name
        ))
        .with_error_code("MIC-DSP-001")
        .with_action(RecoveryAction::primary("Settings", RecoveryActionId::OpenSettings))
    }

    /// Display disconnected during operation
    pub fn disconnected(fallback: Option<&str>) -> UserMessage {
        let msg = if let Some(target) = fallback {
            format!(
                "The display showing your feed was disconnected. Feed moved to {}.",
                target
            )
        } else {
            "The display showing your feed was disconnected.".to_string()
        };
        UserMessage::new(msg)
            .with_error_code("MIC-DSP-002")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
            .with_action(RecoveryAction::secondary("Settings", RecoveryActionId::OpenSettings))
    }

    /// Resolution changed (informational, usually auto-handled)
    pub fn resolution_changed() -> UserMessage {
        UserMessage::new("Display resolution changed. Adjusting feed...")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
    }

    /// Failed to set wallpaper
    pub fn wallpaper_failed(details: &str) -> UserMessage {
        UserMessage::new(
            "Unable to set wallpaper. Your desktop environment may not support this feature.",
        )
        .with_error_code("MIC-DSP-003")
        .with_details(details.to_string())
        .with_action(RecoveryAction::primary("Help", RecoveryActionId::OpenHelp))
        .with_action(RecoveryAction::secondary("Settings", RecoveryActionId::OpenSettings))
    }

    /// GPU/graphics error
    pub fn gpu_error(details: &str) -> UserMessage {
        UserMessage::new("A graphics error occurred. Please update your graphics drivers.")
            .with_error_code("MIC-DSP-004")
            .with_details(details.to_string())
            .with_action(RecoveryAction::primary("Update Drivers", RecoveryActionId::UpdateDrivers))
            .with_action(RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp))
    }

    /// Surface creation failed
    pub fn surface_creation_failed(details: &str) -> UserMessage {
        UserMessage::new("Unable to create display surface. Please check your graphics drivers.")
            .with_error_code("MIC-DSP-005")
            .with_details(details.to_string())
            .with_action(RecoveryAction::primary("Update Drivers", RecoveryActionId::UpdateDrivers))
    }
}

// ============================================================================
// Message Templates - Feed Errors
// ============================================================================

/// Messages for feed/processing errors
pub mod feed {
    use super::*;

    /// Feed appears frozen
    pub fn frozen() -> UserMessage {
        UserMessage::new("Feed appears frozen.")
            .with_error_code("MIC-FED-001")
            .with_action(RecoveryAction::primary("Restart Feed", RecoveryActionId::RestartFeed))
    }

    /// Frame processing error (usually recoverable)
    pub fn processing_error() -> UserMessage {
        UserMessage::new("Error processing video frame. Skipping frame...")
            .with_error_code("MIC-FED-002")
            .with_action(RecoveryAction::secondary("Dismiss", RecoveryActionId::Dismiss))
    }

    /// Feed paused
    pub fn paused() -> UserMessage {
        UserMessage::new("Feed paused.")
            .with_action(RecoveryAction::primary("Resume", RecoveryActionId::RestartFeed))
    }
}

// ============================================================================
// Message Templates - Configuration Errors
// ============================================================================

/// Messages for configuration errors
pub mod config {
    use super::*;

    /// Config file not found (using defaults)
    pub fn not_found() -> UserMessage {
        UserMessage::new("Settings file not found. Using default configuration.")
            .with_error_code("MIC-CFG-001")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
    }

    /// Config file corrupted
    pub fn corrupted() -> UserMessage {
        UserMessage::new("Settings file is corrupted. Using default configuration.")
            .with_error_code("MIC-CFG-002")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
            .with_action(RecoveryAction::secondary("Settings", RecoveryActionId::OpenSettings))
    }

    /// Failed to save config
    pub fn save_failed() -> UserMessage {
        UserMessage::new("Unable to save settings. Check that you have write permissions.")
            .with_error_code("MIC-CFG-003")
            .with_action(RecoveryAction::primary("Retry", RecoveryActionId::Retry))
            .with_action(RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp))
    }

    /// Failed to read config
    pub fn read_failed() -> UserMessage {
        UserMessage::new("Unable to read settings. Using default configuration.")
            .with_error_code("MIC-CFG-004")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
    }
}

// ============================================================================
// Message Templates - Recovery/Crash
// ============================================================================

/// Messages for crash recovery
pub mod recovery {
    use super::*;

    /// Recovered from crash
    pub fn recovered_from_crash() -> UserMessage {
        UserMessage::new(
            "Micround recovered from unexpected shutdown. Your original wallpaper has been restored.",
        )
        .with_error_code("MIC-REC-001")
        .with_action(RecoveryAction::primary("Restart Feed", RecoveryActionId::RestartFeed))
        .with_action(RecoveryAction::secondary("Dismiss", RecoveryActionId::Dismiss))
    }

    /// Original wallpaper restored
    pub fn wallpaper_restored() -> UserMessage {
        UserMessage::new("Your original wallpaper has been restored.")
            .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
    }
}

// ============================================================================
// Message Templates - Platform Errors
// ============================================================================

/// Messages for platform-specific errors
pub mod platform {
    use super::*;

    /// Feature not supported on this platform
    pub fn unsupported(feature: &str) -> UserMessage {
        UserMessage::new(format!(
            "This feature is not available on your system: {}",
            feature
        ))
        .with_error_code("MIC-PLT-001")
        .with_action(RecoveryAction::primary("Dismiss", RecoveryActionId::Dismiss))
        .with_action(RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp))
    }

    /// Generic platform error
    pub fn error(details: &str) -> UserMessage {
        UserMessage::new("A system error occurred. Please check your desktop environment configuration.")
            .with_error_code("MIC-PLT-002")
            .with_details(details.to_string())
            .with_action(RecoveryAction::primary("Help", RecoveryActionId::OpenHelp))
    }
}

// ============================================================================
// Message Templates - Internal/Generic
// ============================================================================

/// Messages for internal/unexpected errors
pub mod internal {
    use super::*;

    /// Unexpected internal error
    pub fn unexpected(details: &str) -> UserMessage {
        UserMessage::new("An unexpected error occurred. Please restart the application.")
            .with_error_code("MIC-INT-001")
            .with_details(details.to_string())
            .with_action(RecoveryAction::primary("Help", RecoveryActionId::OpenHelp))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_action_creation() {
        let action = RecoveryAction::primary("Refresh", RecoveryActionId::RefreshCameras);
        assert!(action.primary);
        assert_eq!(action.label, "Refresh");

        let action = RecoveryAction::secondary("Help", RecoveryActionId::OpenHelp);
        assert!(!action.primary);
    }

    #[test]
    fn test_user_message_builder() {
        let msg = UserMessage::new("Test message")
            .with_error_code("TEST-001")
            .with_details("Technical details")
            .with_action(RecoveryAction::primary("OK", RecoveryActionId::Dismiss));

        assert_eq!(msg.message, "Test message");
        assert_eq!(msg.error_code, Some("TEST-001".to_string()));
        assert_eq!(msg.details, Some("Technical details".to_string()));
        assert_eq!(msg.actions.len(), 1);
    }

    #[test]
    fn test_camera_messages() {
        let msg = camera::no_camera_found();
        assert!(msg.message.contains("No camera detected"));
        assert_eq!(msg.error_code, Some("MIC-CAM-001".to_string()));
        assert!(!msg.actions.is_empty());
        // Primary action should be first
        assert!(msg.actions[0].primary);
    }

    #[test]
    fn test_linux_permission_message() {
        let msg = camera::permission_denied_linux("/dev/video0", false);
        assert!(msg.message.contains("Permission denied"));
        assert!(msg.details.is_some());
        let details = msg.details.unwrap();
        assert!(details.contains("usermod") || details.contains("video"));
    }

    #[test]
    fn test_display_format() {
        let msg = UserMessage::new("Test").with_error_code("ABC-123");
        let formatted = format!("{}", msg);
        assert!(formatted.contains("Test"));
        assert!(formatted.contains("ABC-123"));
    }

    #[test]
    fn test_recovery_message() {
        let msg = recovery::recovered_from_crash();
        assert!(msg.message.contains("recovered"));
        assert!(msg.message.contains("wallpaper"));
        assert!(!msg.actions.is_empty());
    }
}
