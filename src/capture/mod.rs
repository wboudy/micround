//! Video capture subsystem
//!
//! Platform-specific camera capture implementations that conform to
//! the `CaptureBackend` trait.
//!
//! # Architecture
//!
//! The capture system is divided into:
//! - **Enumeration**: Discovering available camera devices
//! - **Backend**: Opening, configuring, and capturing frames from cameras
//! - **Negotiation**: Selecting optimal capture format
//!
//! Platform-specific implementations are enabled via Cargo features:
//! - `linux`: V4L2-based capture
//! - `windows`: Media Foundation capture
//! - `macos`: AVFoundation capture

pub mod capture_loop;
pub mod enumerator;
pub mod hotplug;
pub mod manager;
pub mod negotiation;
pub mod state;

#[cfg(target_os = "linux")]
pub mod v4l2;

// Simulator module is always available - it's production code, not mocks
pub mod simulator;

use crate::core::{CameraDevice, CaptureSettings, DeviceId, Frame, CaptureError, NegotiatedFormat};

pub use enumerator::*;
pub use negotiation::{negotiate_format, filter_acceptable_capabilities};
pub use capture_loop::{
    start_capture_loop, CaptureLoopHandle, CaptureLoopError, CaptureMetrics,
    CaptureState, FrameReceiver, MetricsSnapshot,
};
pub use state::{
    CameraState, CameraStateManager, CameraErrorInfo, StateTransition,
    TransitionReason, SharedCameraState, shared_camera_state, shared_camera_state_available,
};
pub use hotplug::{
    HotplugConfig, HotplugMonitorHandle, start_hotplug_monitor,
    ChannelHandler, TokioChannelHandler,
};
pub use manager::{
    CaptureManager, CameraHandle, DeviceEvent,
};

/// Trait for platform-specific capture implementations
pub trait CaptureBackend: Send {
    /// Enumerate all available camera devices
    fn enumerate_devices(&self) -> Vec<CameraDevice>;

    /// Open a camera device with the given settings
    ///
    /// Performs format negotiation if the exact requested settings aren't available.
    /// Returns the actual negotiated format so callers know what to expect.
    ///
    /// # Errors
    /// - `DeviceNotFound` - Device ID doesn't exist
    /// - `DeviceBusy` - Device is in use by another application
    /// - `FormatNegotiationFailed` - No suitable format available
    /// - `PermissionDenied` - Insufficient permissions
    fn open(&mut self, device_id: &DeviceId, settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError>;

    /// Start capturing frames
    fn start(&mut self) -> Result<(), CaptureError>;

    /// Stop capturing frames
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Close the camera device
    fn close(&mut self);

    /// Check if currently capturing
    fn is_capturing(&self) -> bool;

    /// Get the currently negotiated format (if device is open)
    fn current_format(&self) -> Option<NegotiatedFormat>;

    /// Get the next frame (blocking)
    fn next_frame(&mut self) -> Result<Frame, CaptureError>;
}

/// Create a platform-appropriate capture backend
#[cfg(target_os = "linux")]
pub fn create_backend() -> Box<dyn CaptureBackend> {
    Box::new(v4l2::V4l2Backend::new())
}

/// Create a platform-appropriate camera enumerator
#[cfg(target_os = "linux")]
pub fn create_enumerator() -> Box<dyn CameraEnumerator> {
    Box::new(v4l2::V4l2Enumerator::new())
}

// Placeholder for other platforms
#[cfg(not(target_os = "linux"))]
pub fn create_backend() -> Box<dyn CaptureBackend> {
    unimplemented!("Capture backend not implemented for this platform")
}

#[cfg(not(target_os = "linux"))]
pub fn create_enumerator() -> Box<dyn CameraEnumerator> {
    unimplemented!("Camera enumerator not implemented for this platform")
}

/// Create a simulator backend for testing (requires test-simulator feature)
#[cfg(feature = "test-simulator")]
pub fn create_simulator_backend() -> Box<dyn CaptureBackend> {
    Box::new(simulator::SimulatorBackend::new_default())
}

/// Create a simulator backend with custom configuration
#[cfg(feature = "test-simulator")]
pub fn create_simulator_backend_with_config(config: simulator::SimulatorConfig) -> Box<dyn CaptureBackend> {
    Box::new(simulator::SimulatorBackend::new(config))
}
