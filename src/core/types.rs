//! Core type definitions for Micround
//!
//! These types are used across all modules and platforms.

use serde::{Deserialize, Serialize};

/// Unique identifier for a camera device
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(pub String);

/// Information about a camera device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraDevice {
    /// Unique identifier that persists across reconnects
    pub id: DeviceId,
    /// Human-readable device name
    pub name: String,
    /// Manufacturer name if available
    pub manufacturer: Option<String>,
    /// Supported capture capabilities
    pub capabilities: Vec<CameraCapability>,
    /// Whether the device is currently available
    pub is_available: bool,
}

/// A specific capture capability (resolution + framerate + format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCapability {
    pub width: u32,
    pub height: u32,
    pub framerate: f32,
    pub format: PixelFormat,
}

/// Supported pixel formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    /// Motion JPEG (common for webcams)
    Mjpeg,
    /// YUV 4:2:2 packed (YUYV)
    Yuyv,
    /// NV12 (Y plane + interleaved UV)
    Nv12,
    /// RGB 24-bit
    Rgb24,
    /// RGBA 32-bit
    Rgba32,
    /// Unknown or unsupported format
    Unknown,
}

/// Settings for capture
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureSettings {
    pub width: u32,
    pub height: u32,
    pub framerate: f32,
    pub format: Option<PixelFormat>,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: None,
        }
    }
}

/// A single captured frame
pub struct Frame {
    /// Raw pixel data
    pub data: Vec<u8>,
    /// Pixel format of the data
    pub format: PixelFormat,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Monotonic timestamp in nanoseconds
    pub timestamp_ns: u64,
    /// Sequence number for drop detection
    pub sequence: u64,
}

#[cfg(feature = "secure-zero")]
impl Drop for Frame {
    fn drop(&mut self) {
        // Privacy: Zero out frame data before deallocation (opt-in)
        self.data.iter_mut().for_each(|b| *b = 0);
    }
}

/// Display identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DisplayId(pub String);

/// Scaling mode for fitting camera feed to display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ScalingMode {
    /// Scale to fit within display, preserving aspect ratio (letterbox/pillarbox)
    Fit,
    /// Scale to fill display, preserving aspect ratio (may crop)
    #[default]
    Fill,
    /// Stretch to exactly match display dimensions
    Stretch,
    /// Center at native resolution (no scaling)
    Center,
}

/// Rotation angle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

/// Flip direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Flip {
    #[default]
    None,
    Horizontal,
    Vertical,
    Both,
}

/// Result of format negotiation when opening a camera stream
///
/// This struct reports what was actually negotiated with the camera,
/// which may differ from what was requested if the exact settings
/// weren't available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiatedFormat {
    /// Actual width in pixels
    pub width: u32,
    /// Actual height in pixels
    pub height: u32,
    /// Actual framerate achieved
    pub framerate: f32,
    /// Actual pixel format
    pub format: PixelFormat,
    /// Whether this exactly matched the request
    pub exact_match: bool,
}

impl NegotiatedFormat {
    /// Create a NegotiatedFormat from a CameraCapability
    pub fn from_capability(cap: &CameraCapability, exact_match: bool) -> Self {
        Self {
            width: cap.width,
            height: cap.height,
            framerate: cap.framerate,
            format: cap.format,
            exact_match,
        }
    }

    /// Check if resolution matches the requested values
    pub fn resolution_matches(&self, width: u32, height: u32) -> bool {
        self.width == width && self.height == height
    }
}
