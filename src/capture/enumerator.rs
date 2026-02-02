//! Camera device enumeration and format negotiation
//!
//! Provides platform-independent traits and types for discovering
//! available video capture devices and negotiating capture formats.
#![allow(dead_code)] // Camera enumeration API

use crate::core::{CameraCapability, CameraDevice, CaptureError, DeviceId, PixelFormat};

/// Event indicating a change in available camera devices
#[derive(Debug, Clone)]
pub enum CameraEvent {
    /// A new camera was connected
    Connected(CameraDevice),
    /// A camera was disconnected
    Disconnected(DeviceId),
    /// A camera's properties changed (e.g., resolution list updated)
    Changed(DeviceId),
}

/// Trait for discovering and monitoring camera devices
///
/// Platform implementations handle the actual device enumeration
/// using platform-specific APIs (V4L2, Media Foundation, AVFoundation).
pub trait CameraEnumerator: Send {
    /// Enumerate all currently connected video capture devices
    ///
    /// Returns a list of all cameras that are available for capture.
    /// The list may be empty if no cameras are connected.
    fn enumerate(&self) -> Result<Vec<CameraDevice>, CaptureError>;

    /// Get detailed information about a specific device
    ///
    /// Returns device info if found, or an error if the device
    /// is not available.
    fn get_device(&self, id: &DeviceId) -> Result<CameraDevice, CaptureError>;

    /// Get the capabilities of a specific device
    ///
    /// Returns the list of supported resolutions, framerates,
    /// and pixel formats.
    fn get_capabilities(&self, id: &DeviceId) -> Result<Vec<CameraCapability>, CaptureError>;

    /// Check if a device is currently available
    fn is_available(&self, id: &DeviceId) -> bool;

    /// Refresh the device list
    ///
    /// Call this to update the internal cache of devices after
    /// receiving a hot-plug event.
    fn refresh(&mut self) -> Result<(), CaptureError>;

    /// Get the "best" capability for a device
    ///
    /// Selects the capability that best matches the requested
    /// resolution and framerate. Falls back to the highest
    /// resolution if no exact match is found.
    fn best_capability(
        &self,
        id: &DeviceId,
        preferred_width: u32,
        preferred_height: u32,
        preferred_fps: f32,
    ) -> Result<CameraCapability, CaptureError> {
        let caps = self.get_capabilities(id)?;

        if caps.is_empty() {
            return Err(CaptureError::FormatNegotiationFailed(
                "No capabilities available".into(),
            ));
        }

        // First, try to find an exact match
        if let Some(cap) = caps.iter().find(|c| {
            c.width == preferred_width
                && c.height == preferred_height
                && (c.framerate - preferred_fps).abs() < 1.0
        }) {
            return Ok(cap.clone());
        }

        // Then, find the closest resolution match
        let mut best = caps[0].clone();
        let mut best_score = resolution_score(&best, preferred_width, preferred_height);

        for cap in &caps[1..] {
            let score = resolution_score(cap, preferred_width, preferred_height);
            if score < best_score {
                best = cap.clone();
                best_score = score;
            }
        }

        Ok(best)
    }
}

/// Calculate a score for how close a capability matches the preferred resolution
/// Lower score is better
fn resolution_score(cap: &CameraCapability, pref_width: u32, pref_height: u32) -> u64 {
    let width_diff = (cap.width as i64 - pref_width as i64).unsigned_abs();
    let height_diff = (cap.height as i64 - pref_height as i64).unsigned_abs();
    width_diff * width_diff + height_diff * height_diff
}

/// Trait for receiving camera hot-plug events
pub trait CameraEventHandler: Send {
    /// Called when a camera event occurs
    fn on_camera_event(&mut self, event: CameraEvent);
}

/// Convert a V4L2 fourcc code to our PixelFormat enum
pub fn fourcc_to_format(fourcc: u32) -> PixelFormat {
    // Common fourcc codes (little-endian on most systems)
    match fourcc {
        // MJPEG variants
        0x47504A4D | 0x4745504A => PixelFormat::Mjpeg, // "MJPG" | "JPEG"
        // YUYV/YUY2
        0x56595559 => PixelFormat::Yuyv, // "YUYV"
        // NV12
        0x3231564E => PixelFormat::Nv12, // "NV12"
        // RGB24
        0x33424752 => PixelFormat::Rgb24, // "RGB3"
        // RGBA32
        0x34424752 => PixelFormat::Rgba32, // "RGB4"
        // Unknown
        _ => PixelFormat::Unknown,
    }
}

/// Convert a PixelFormat to a V4L2 fourcc code
pub fn format_to_fourcc(format: PixelFormat) -> Option<u32> {
    match format {
        PixelFormat::Mjpeg => Some(0x47504A4D),  // "MJPG"
        PixelFormat::Yuyv => Some(0x56595559),   // "YUYV"
        PixelFormat::Nv12 => Some(0x3231564E),   // "NV12"
        PixelFormat::Rgb24 => Some(0x33424752),  // "RGB3"
        PixelFormat::Rgba32 => Some(0x34424752), // "RGB4"
        PixelFormat::Unknown => None,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fourcc_to_format() {
        assert_eq!(fourcc_to_format(0x47504A4D), PixelFormat::Mjpeg);
        assert_eq!(fourcc_to_format(0x56595559), PixelFormat::Yuyv);
        assert_eq!(fourcc_to_format(0x12345678), PixelFormat::Unknown);
    }

    #[test]
    fn test_format_to_fourcc() {
        assert_eq!(format_to_fourcc(PixelFormat::Mjpeg), Some(0x47504A4D));
        assert_eq!(format_to_fourcc(PixelFormat::Unknown), None);
    }

    #[test]
    fn test_resolution_score() {
        let cap_1080p = CameraCapability {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: PixelFormat::Mjpeg,
        };

        let cap_720p = CameraCapability {
            width: 1280,
            height: 720,
            framerate: 30.0,
            format: PixelFormat::Mjpeg,
        };

        // Requesting 1080p, 1080p should score better (lower)
        let score_1080 = resolution_score(&cap_1080p, 1920, 1080);
        let score_720 = resolution_score(&cap_720p, 1920, 1080);

        assert!(score_1080 < score_720);
        assert_eq!(score_1080, 0); // Exact match
    }
}
