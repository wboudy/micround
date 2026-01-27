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
//!
//! Platform-specific implementations are enabled via Cargo features:
//! - `linux`: V4L2-based capture
//! - `windows`: Media Foundation capture
//! - `macos`: AVFoundation capture

pub mod enumerator;

#[cfg(target_os = "linux")]
pub mod v4l2;

use crate::core::{CameraDevice, CaptureSettings, DeviceId, Frame, CaptureError};

pub use enumerator::*;

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
