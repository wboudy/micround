//! Video capture subsystem
//!
//! Platform-specific camera capture implementations that conform to
//! the `CaptureBackend` trait.

use crate::core::{CameraDevice, CaptureSettings, DeviceId, Frame, CaptureError};

/// Trait for platform-specific capture implementations
pub trait CaptureBackend: Send {
    /// Enumerate all available camera devices
    fn enumerate_devices(&self) -> Vec<CameraDevice>;

    /// Open a camera device with the given settings
    fn open(&mut self, device_id: &DeviceId, settings: CaptureSettings) -> Result<(), CaptureError>;

    /// Start capturing frames
    fn start(&mut self) -> Result<(), CaptureError>;

    /// Stop capturing frames
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Close the camera device
    fn close(&mut self);

    /// Check if currently capturing
    fn is_capturing(&self) -> bool;

    /// Get the next frame (blocking)
    fn next_frame(&mut self) -> Result<Frame, CaptureError>;
}

// Platform-specific implementations will be added when those features are implemented
