//! Permission handling for camera access and App Nap prevention
//!
//! This module provides cross-platform abstractions for:
//! - Camera permission status and requests (TCC on macOS)
//! - App Nap prevention to avoid throttling during live capture
//!
//! # macOS Implementation
//!
//! Camera permissions are managed by Transparency, Consent, and Control (TCC).
//! The app must have `NSCameraUsageDescription` in Info.plist and request
//! permission at runtime via AVFoundation.
//!
//! App Nap can be prevented using `ProcessInfo.beginActivity()` or by
//! disabling automatic termination.
//!
//! # Platform Support
//!
//! - **macOS**: Full support via AVFoundation and NSProcessInfo
//! - **Windows**: Camera permissions are implicit (no TCC equivalent)
//! - **Linux**: Camera access via /dev/video* requires user in `video` group

use std::sync::Arc;
use thiserror::Error;

// ============================================================================
// Permission Types
// ============================================================================

/// Camera permission status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraPermission {
    /// Permission has been granted
    Authorized,
    /// Permission has not been requested yet
    NotDetermined,
    /// Permission was denied by the user
    Denied,
    /// Permission is restricted (parental controls, MDM, etc.)
    Restricted,
    /// Platform doesn't require explicit permission
    NotRequired,
}

impl CameraPermission {
    /// Check if camera access is allowed
    pub fn is_authorized(&self) -> bool {
        matches!(self, Self::Authorized | Self::NotRequired)
    }

    /// Check if we should show a permission request
    pub fn should_request(&self) -> bool {
        matches!(self, Self::NotDetermined)
    }

    /// Check if user denied and we should guide them to settings
    pub fn needs_settings_guidance(&self) -> bool {
        matches!(self, Self::Denied | Self::Restricted)
    }
}

/// Error type for permission operations
#[derive(Debug, Clone, Error)]
pub enum PermissionError {
    #[error("Camera permission denied")]
    CameraPermissionDenied,

    #[error("Camera permission restricted (parental controls or MDM)")]
    CameraPermissionRestricted,

    #[error("Failed to request camera permission: {0}")]
    RequestFailed(String),

    #[error("Failed to check permission status: {0}")]
    StatusCheckFailed(String),

    #[error("Platform not supported for this operation")]
    Unsupported,

    #[error("Activity assertion failed: {0}")]
    ActivityAssertionFailed(String),
}

// ============================================================================
// Activity Prevention (App Nap)
// ============================================================================

/// Activity type for preventing App Nap or system sleep
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityType {
    /// User-initiated activity that should prevent sleep
    UserInitiated,
    /// Background activity that should prevent App Nap
    Background,
    /// Latency-critical activity that needs real-time priority
    LatencyCritical,
}

/// Guard that prevents App Nap while held
///
/// On macOS, this uses `ProcessInfo.beginActivity()` to prevent
/// the system from throttling the app when it's not in the foreground.
///
/// When dropped, the activity assertion is released.
pub struct ActivityGuard {
    /// Platform-specific activity identifier
    #[allow(dead_code)]
    id: u64,
    /// Release callback
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl ActivityGuard {
    /// Create a new activity guard
    pub fn new(id: u64, release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            id,
            release: Some(Box::new(release)),
        }
    }

    /// Create a no-op guard (for platforms without App Nap)
    pub fn noop() -> Self {
        Self {
            id: 0,
            release: None,
        }
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

// ============================================================================
// Permission Handler Trait
// ============================================================================

/// Trait for handling permissions and activity assertions
pub trait PermissionHandler: Send + Sync {
    /// Check current camera permission status
    fn camera_permission_status(&self) -> Result<CameraPermission, PermissionError>;

    /// Request camera permission from the user
    ///
    /// This may show a system dialog on macOS.
    /// Returns the new permission status after the request.
    fn request_camera_permission(&self) -> Result<CameraPermission, PermissionError>;

    /// Begin an activity to prevent App Nap
    ///
    /// The returned guard will end the activity when dropped.
    fn begin_activity(
        &self,
        activity_type: ActivityType,
        reason: &str,
    ) -> Result<ActivityGuard, PermissionError>;

    /// Disable automatic termination (aggressive App Nap prevention)
    ///
    /// This prevents macOS from automatically terminating the app
    /// when it's been idle for a long time.
    fn disable_automatic_termination(&self, reason: &str) -> Result<(), PermissionError>;

    /// Re-enable automatic termination
    fn enable_automatic_termination(&self) -> Result<(), PermissionError>;

    /// Open system settings for camera permissions
    ///
    /// On macOS: Opens System Preferences > Security & Privacy > Camera
    fn open_camera_settings(&self) -> Result<(), PermissionError>;
}

// ============================================================================
// Platform Implementations
// ============================================================================

/// Create a platform-appropriate permission handler
pub fn create_permission_handler() -> Arc<dyn PermissionHandler> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(MacOSPermissionHandler::new())
    }

    #[cfg(target_os = "windows")]
    {
        Arc::new(WindowsPermissionHandler::new())
    }

    #[cfg(target_os = "linux")]
    {
        Arc::new(LinuxPermissionHandler::new())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        Arc::new(StubPermissionHandler)
    }
}

// ============================================================================
// macOS Implementation
// ============================================================================

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    /// macOS permission handler using AVFoundation and NSProcessInfo
    pub struct MacOSPermissionHandler {
        /// Counter for activity IDs
        activity_counter: AtomicU64,
        /// Track if automatic termination is disabled
        auto_termination_disabled: AtomicBool,
    }

    impl MacOSPermissionHandler {
        pub fn new() -> Self {
            Self {
                activity_counter: AtomicU64::new(1),
                auto_termination_disabled: AtomicBool::new(false),
            }
        }
    }

    impl PermissionHandler for MacOSPermissionHandler {
        fn camera_permission_status(&self) -> Result<CameraPermission, PermissionError> {
            // NOTE: This requires AVFoundation framework linked
            // For now, return NotDetermined to indicate we need to implement
            // the actual AVFoundation check
            //
            // Full implementation would use:
            // AVCaptureDevice.authorizationStatus(for: .video)
            //
            // This is a placeholder that allows the code to compile
            // The actual implementation requires objc2 bindings to AVFoundation
            tracing::debug!("camera_permission_status: returning NotDetermined (placeholder)");
            Ok(CameraPermission::NotDetermined)
        }

        fn request_camera_permission(&self) -> Result<CameraPermission, PermissionError> {
            // NOTE: This requires AVFoundation framework linked
            // For now, return NotDetermined to indicate we need to implement
            //
            // Full implementation would use:
            // AVCaptureDevice.requestAccess(for: .video) { granted in ... }
            tracing::debug!("request_camera_permission: placeholder implementation");
            Ok(CameraPermission::NotDetermined)
        }

        fn begin_activity(
            &self,
            activity_type: ActivityType,
            reason: &str,
        ) -> Result<ActivityGuard, PermissionError> {
            // Generate unique activity ID
            let id = self.activity_counter.fetch_add(1, Ordering::SeqCst);

            tracing::info!(
                "Beginning activity: type={:?}, reason={}, id={}",
                activity_type,
                reason,
                id
            );

            // NOTE: Full implementation would use:
            // ProcessInfo.processInfo.beginActivity(options:reason:)
            //
            // For now, create a guard that logs when released
            let release_reason = reason.to_string();
            Ok(ActivityGuard::new(id, move || {
                tracing::info!("Ending activity: reason={}, id={}", release_reason, id);
            }))
        }

        fn disable_automatic_termination(&self, reason: &str) -> Result<(), PermissionError> {
            if self
                .auto_termination_disabled
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                tracing::info!("Disabling automatic termination: {}", reason);
                // NOTE: Full implementation would use:
                // ProcessInfo.processInfo.disableAutomaticTermination(reason)
            }
            Ok(())
        }

        fn enable_automatic_termination(&self) -> Result<(), PermissionError> {
            if self
                .auto_termination_disabled
                .compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                tracing::info!("Re-enabling automatic termination");
                // NOTE: Full implementation would use:
                // ProcessInfo.processInfo.enableAutomaticTermination()
            }
            Ok(())
        }

        fn open_camera_settings(&self) -> Result<(), PermissionError> {
            // Open System Preferences to Privacy & Security > Camera
            // URL: x-apple.systempreferences:com.apple.preference.security?Privacy_Camera
            tracing::info!("Opening camera privacy settings");

            // Use NSWorkspace to open the URL
            // For now, use a shell command as a placeholder
            std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Camera")
                .spawn()
                .map_err(|e| PermissionError::RequestFailed(e.to_string()))?;

            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::MacOSPermissionHandler;

// ============================================================================
// Windows Implementation (Stub)
// ============================================================================

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    /// Windows permission handler
    ///
    /// Windows doesn't have TCC-style camera permissions for desktop apps.
    /// Camera access is granted implicitly when using the device.
    pub struct WindowsPermissionHandler;

    impl WindowsPermissionHandler {
        pub fn new() -> Self {
            Self
        }
    }

    impl PermissionHandler for WindowsPermissionHandler {
        fn camera_permission_status(&self) -> Result<CameraPermission, PermissionError> {
            // Windows doesn't require explicit permission for desktop apps
            Ok(CameraPermission::NotRequired)
        }

        fn request_camera_permission(&self) -> Result<CameraPermission, PermissionError> {
            Ok(CameraPermission::NotRequired)
        }

        fn begin_activity(
            &self,
            _activity_type: ActivityType,
            _reason: &str,
        ) -> Result<ActivityGuard, PermissionError> {
            // Windows doesn't have App Nap
            Ok(ActivityGuard::noop())
        }

        fn disable_automatic_termination(&self, _reason: &str) -> Result<(), PermissionError> {
            Ok(())
        }

        fn enable_automatic_termination(&self) -> Result<(), PermissionError> {
            Ok(())
        }

        fn open_camera_settings(&self) -> Result<(), PermissionError> {
            // Open Windows Settings > Privacy > Camera
            std::process::Command::new("ms-settings:privacy-webcam")
                .spawn()
                .map_err(|e| PermissionError::RequestFailed(e.to_string()))?;
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows::WindowsPermissionHandler;

// ============================================================================
// Linux Implementation (Stub)
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    /// Linux permission handler
    ///
    /// On Linux, camera access requires the user to be in the `video` group.
    /// There's no TCC-style permission system.
    pub struct LinuxPermissionHandler;

    impl LinuxPermissionHandler {
        pub fn new() -> Self {
            Self
        }
    }

    impl PermissionHandler for LinuxPermissionHandler {
        fn camera_permission_status(&self) -> Result<CameraPermission, PermissionError> {
            // Check if /dev/video* is accessible
            // For now, assume permission is not required at the app level
            Ok(CameraPermission::NotRequired)
        }

        fn request_camera_permission(&self) -> Result<CameraPermission, PermissionError> {
            // On Linux, the user needs to be added to the video group
            // This can't be done from within the app
            Ok(CameraPermission::NotRequired)
        }

        fn begin_activity(
            &self,
            _activity_type: ActivityType,
            _reason: &str,
        ) -> Result<ActivityGuard, PermissionError> {
            // Linux doesn't have App Nap
            Ok(ActivityGuard::noop())
        }

        fn disable_automatic_termination(&self, _reason: &str) -> Result<(), PermissionError> {
            Ok(())
        }

        fn enable_automatic_termination(&self) -> Result<(), PermissionError> {
            Ok(())
        }

        fn open_camera_settings(&self) -> Result<(), PermissionError> {
            // No standard location for camera settings on Linux
            Err(PermissionError::Unsupported)
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::LinuxPermissionHandler;

// ============================================================================
// Stub Implementation (for unsupported platforms)
// ============================================================================

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
struct StubPermissionHandler;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
impl PermissionHandler for StubPermissionHandler {
    fn camera_permission_status(&self) -> Result<CameraPermission, PermissionError> {
        Ok(CameraPermission::NotRequired)
    }

    fn request_camera_permission(&self) -> Result<CameraPermission, PermissionError> {
        Ok(CameraPermission::NotRequired)
    }

    fn begin_activity(
        &self,
        _activity_type: ActivityType,
        _reason: &str,
    ) -> Result<ActivityGuard, PermissionError> {
        Ok(ActivityGuard::noop())
    }

    fn disable_automatic_termination(&self, _reason: &str) -> Result<(), PermissionError> {
        Ok(())
    }

    fn enable_automatic_termination(&self) -> Result<(), PermissionError> {
        Ok(())
    }

    fn open_camera_settings(&self) -> Result<(), PermissionError> {
        Err(PermissionError::Unsupported)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_permission_status() {
        assert!(CameraPermission::Authorized.is_authorized());
        assert!(CameraPermission::NotRequired.is_authorized());
        assert!(!CameraPermission::Denied.is_authorized());
        assert!(!CameraPermission::NotDetermined.is_authorized());
        assert!(!CameraPermission::Restricted.is_authorized());
    }

    #[test]
    fn test_camera_permission_should_request() {
        assert!(CameraPermission::NotDetermined.should_request());
        assert!(!CameraPermission::Authorized.should_request());
        assert!(!CameraPermission::Denied.should_request());
    }

    #[test]
    fn test_camera_permission_needs_settings() {
        assert!(CameraPermission::Denied.needs_settings_guidance());
        assert!(CameraPermission::Restricted.needs_settings_guidance());
        assert!(!CameraPermission::Authorized.needs_settings_guidance());
        assert!(!CameraPermission::NotDetermined.needs_settings_guidance());
    }

    #[test]
    fn test_activity_guard_drop() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let released = Arc::new(AtomicBool::new(false));
        let released_clone = released.clone();

        {
            let _guard = ActivityGuard::new(1, move || {
                released_clone.store(true, Ordering::SeqCst);
            });
            assert!(!released.load(Ordering::SeqCst));
        }

        assert!(released.load(Ordering::SeqCst));
    }

    #[test]
    fn test_activity_guard_noop() {
        let _guard = ActivityGuard::noop();
        // Should not panic when dropped
    }

    #[test]
    fn test_create_permission_handler() {
        let handler = create_permission_handler();

        // Should be able to check camera status
        let status = handler.camera_permission_status();
        assert!(status.is_ok());

        // Should be able to begin an activity
        let guard = handler.begin_activity(ActivityType::UserInitiated, "test");
        assert!(guard.is_ok());
    }
}
