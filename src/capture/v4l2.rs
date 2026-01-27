//! V4L2 (Video4Linux2) camera support for Linux
//!
//! Provides camera enumeration and capture using the V4L2 API.
//!
//! # Requirements
//! - Linux kernel with V4L2 support
//! - User must be in the `video` group for device access

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::capture::enumerator::{fourcc_to_format, format_to_fourcc, CameraEnumerator, CameraEvent, CameraEventHandler};
use crate::capture::negotiation::negotiate_format;
use crate::capture::CaptureBackend;
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame, NegotiatedFormat, PixelFormat,
};

#[cfg(feature = "linux")]
use v4l::prelude::*;
#[cfg(feature = "linux")]
use v4l::video::Capture;
#[cfg(feature = "linux")]
use v4l::{Format, FourCC};

/// V4L2-based camera enumerator
pub struct V4l2Enumerator {
    /// Cached device list
    devices: HashMap<String, CameraDevice>,
}

impl V4l2Enumerator {
    pub fn new() -> Self {
        let mut enumerator = Self {
            devices: HashMap::new(),
        };
        // Initial device scan
        let _ = enumerator.refresh();
        enumerator
    }

    /// Find all video devices in /dev
    fn find_video_devices() -> Vec<PathBuf> {
        let dev_dir = Path::new("/dev");

        fs::read_dir(dev_dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|s| s.starts_with("video"))
                            .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Query device information
    #[cfg(feature = "linux")]
    fn query_device(path: &Path) -> Option<CameraDevice> {
        let device = Device::with_path(path).ok()?;
        let caps = device.query_caps().ok()?;

        // Only include video capture devices
        if !caps.capabilities.contains(v4l::capability::Flags::VIDEO_CAPTURE) {
            return None;
        }

        let device_id = path.to_string_lossy().to_string();
        let name = if caps.card.is_empty() {
            format!("Camera ({})", path.display())
        } else {
            caps.card.clone()
        };

        // Query supported formats
        let capabilities = Self::query_capabilities(&device).unwrap_or_default();

        Some(CameraDevice {
            id: DeviceId(device_id),
            name,
            manufacturer: if caps.driver.is_empty() {
                None
            } else {
                Some(caps.driver.clone())
            },
            capabilities,
            is_available: true,
        })
    }

    #[cfg(not(feature = "linux"))]
    fn query_device(path: &Path) -> Option<CameraDevice> {
        // Fallback for when v4l feature is not enabled
        let device_id = path.to_string_lossy().to_string();

        Some(CameraDevice {
            id: DeviceId(device_id),
            name: format!("Camera ({})", path.display()),
            manufacturer: None,
            capabilities: vec![],
            is_available: true,
        })
    }

    /// Query device capabilities (formats, resolutions, framerates)
    #[cfg(feature = "linux")]
    fn query_capabilities(device: &Device) -> Option<Vec<CameraCapability>> {
        let mut capabilities = Vec::new();

        // Enumerate supported formats
        if let Ok(formats) = device.enum_formats() {
            for fmt_desc in formats {
                let pixel_format = fourcc_to_format(fmt_desc.fourcc.repr);

                // Enumerate frame sizes for this format
                if let Ok(sizes) = device.enum_framesizes(fmt_desc.fourcc) {
                    for size in sizes {
                        match size.size {
                            v4l::framesize::FrameSizeEnum::Discrete(d) => {
                                // Query framerates for this size
                                let framerates =
                                    Self::query_framerates(device, fmt_desc.fourcc, d.width, d.height);

                                for fps in framerates {
                                    capabilities.push(CameraCapability {
                                        width: d.width,
                                        height: d.height,
                                        framerate: fps,
                                        format: pixel_format,
                                    });
                                }

                                // If no framerates found, add a default
                                if capabilities.is_empty()
                                    || capabilities.last().map(|c| c.width != d.width).unwrap_or(true)
                                {
                                    capabilities.push(CameraCapability {
                                        width: d.width,
                                        height: d.height,
                                        framerate: 30.0, // Assume 30fps
                                        format: pixel_format,
                                    });
                                }
                            }
                            v4l::framesize::FrameSizeEnum::Stepwise(s) => {
                                // Add common resolutions within the range
                                for &(w, h) in &[(640, 480), (1280, 720), (1920, 1080)] {
                                    if w >= s.min_width
                                        && w <= s.max_width
                                        && h >= s.min_height
                                        && h <= s.max_height
                                    {
                                        capabilities.push(CameraCapability {
                                            width: w,
                                            height: h,
                                            framerate: 30.0,
                                            format: pixel_format,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if capabilities.is_empty() {
            None
        } else {
            Some(capabilities)
        }
    }

    /// Query supported framerates for a specific format and size
    #[cfg(feature = "linux")]
    fn query_framerates(device: &Device, fourcc: FourCC, width: u32, height: u32) -> Vec<f32> {
        let mut framerates = Vec::new();

        if let Ok(intervals) = device.enum_frameintervals(fourcc, width, height) {
            for interval in intervals {
                match interval.interval {
                    v4l::frameinterval::FrameIntervalEnum::Discrete(d) => {
                        if d.numerator > 0 {
                            let fps = d.denominator as f32 / d.numerator as f32;
                            framerates.push(fps);
                        }
                    }
                    v4l::frameinterval::FrameIntervalEnum::Stepwise(s) => {
                        // Add common framerates within the range
                        let min_fps = if s.max.numerator > 0 {
                            s.max.denominator as f32 / s.max.numerator as f32
                        } else {
                            1.0
                        };
                        let max_fps = if s.min.numerator > 0 {
                            s.min.denominator as f32 / s.min.numerator as f32
                        } else {
                            60.0
                        };

                        for &fps in &[15.0, 24.0, 30.0, 60.0] {
                            if fps >= min_fps && fps <= max_fps {
                                framerates.push(fps);
                            }
                        }
                    }
                }
            }
        }

        framerates
    }
}

impl Default for V4l2Enumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for V4l2Enumerator {
    fn enumerate(&self) -> Result<Vec<CameraDevice>, CaptureError> {
        Ok(self.devices.values().cloned().collect())
    }

    fn get_device(&self, id: &DeviceId) -> Result<CameraDevice, CaptureError> {
        self.devices
            .get(&id.0)
            .cloned()
            .ok_or_else(|| CaptureError::DeviceNotFound(id.0.clone()))
    }

    fn get_capabilities(&self, id: &DeviceId) -> Result<Vec<CameraCapability>, CaptureError> {
        self.devices
            .get(&id.0)
            .map(|d| d.capabilities.clone())
            .ok_or_else(|| CaptureError::DeviceNotFound(id.0.clone()))
    }

    fn is_available(&self, id: &DeviceId) -> bool {
        self.devices.get(&id.0).map(|d| d.is_available).unwrap_or(false)
    }

    fn refresh(&mut self) -> Result<(), CaptureError> {
        let paths = Self::find_video_devices();

        // Mark all devices as potentially unavailable
        for device in self.devices.values_mut() {
            device.is_available = false;
        }

        // Scan for devices
        for path in paths {
            if let Some(device) = Self::query_device(&path) {
                let id = device.id.0.clone();
                self.devices.insert(id, device);
            }
        }

        // Remove devices that are no longer available
        self.devices.retain(|_, d| d.is_available);

        Ok(())
    }
}

/// V4L2-based capture backend
pub struct V4l2Backend {
    #[cfg(feature = "linux")]
    device: Option<Device>,
    #[cfg(feature = "linux")]
    stream: Option<MmapStream<'static>>,
    /// Currently negotiated format
    negotiated_format: Option<NegotiatedFormat>,
    /// Cached enumerator for device queries
    enumerator: V4l2Enumerator,
    capturing: bool,
    sequence: u64,
}

impl V4l2Backend {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "linux")]
            device: None,
            #[cfg(feature = "linux")]
            stream: None,
            negotiated_format: None,
            enumerator: V4l2Enumerator::new(),
            capturing: false,
            sequence: 0,
        }
    }
}

impl Default for V4l2Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for V4l2Backend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        let enumerator = V4l2Enumerator::new();
        enumerator.enumerate().unwrap_or_default()
    }

    #[cfg(feature = "linux")]
    fn open(&mut self, device_id: &DeviceId, settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError> {
        // Close any existing device first
        self.close();

        let path = Path::new(&device_id.0);

        // Check if device exists
        if !path.exists() {
            return Err(CaptureError::DeviceNotFound(device_id.0.clone()));
        }

        // Try to open the device
        let device = Device::with_path(path).map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("busy") || err_str.contains("EBUSY") {
                CaptureError::DeviceBusy
            } else if err_str.contains("permission") || err_str.contains("EACCES") {
                CaptureError::PermissionDenied(device_id.0.clone())
            } else {
                CaptureError::Platform(format!("Failed to open device: {}", e))
            }
        })?;

        // Get device capabilities for negotiation
        let capabilities = self.enumerator
            .get_capabilities(device_id)
            .unwrap_or_default();

        // Negotiate the best format
        let negotiated = negotiate_format(&capabilities, &settings)
            .ok_or_else(|| CaptureError::FormatNegotiationFailed(
                "No suitable format available".into()
            ))?;

        // Get the fourcc for the negotiated format
        let fourcc = format_to_fourcc(negotiated.format)
            .unwrap_or(0x47504A4D); // Default to MJPEG

        // Set the format on the device
        let mut fmt = device
            .format()
            .map_err(|e| CaptureError::Platform(format!("Failed to get format: {}", e)))?;

        fmt.width = negotiated.width;
        fmt.height = negotiated.height;
        fmt.fourcc = FourCC::new(&fourcc.to_le_bytes());

        device
            .set_format(&fmt)
            .map_err(|e| CaptureError::FormatNegotiationFailed(format!("{}", e)))?;

        // Read back what the driver actually set (may differ from request)
        let actual_fmt = device
            .format()
            .map_err(|e| CaptureError::Platform(format!("Failed to read format: {}", e)))?;

        // Create the final negotiated format based on what driver accepted
        let final_negotiated = NegotiatedFormat {
            width: actual_fmt.width,
            height: actual_fmt.height,
            framerate: negotiated.framerate, // Driver doesn't always report this
            format: fourcc_to_format(actual_fmt.fourcc.repr),
            exact_match: actual_fmt.width == settings.width
                && actual_fmt.height == settings.height
                && settings.format.map_or(true, |f| f == fourcc_to_format(actual_fmt.fourcc.repr)),
        };

        self.device = Some(device);
        self.negotiated_format = Some(final_negotiated.clone());

        Ok(final_negotiated)
    }

    #[cfg(not(feature = "linux"))]
    fn open(&mut self, _device_id: &DeviceId, _settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError> {
        Err(CaptureError::Platform(
            "V4L2 support requires the 'linux' feature".into(),
        ))
    }

    #[cfg(feature = "linux")]
    fn start(&mut self) -> Result<(), CaptureError> {
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| CaptureError::Platform("No device opened".into()))?;

        // Create memory-mapped stream
        // Note: We need to handle lifetime properly here
        let stream = MmapStream::with_buffers(device, v4l::buffer::Type::VideoCapture, 4)
            .map_err(|e| CaptureError::Platform(format!("Failed to create stream: {}", e)))?;

        // SAFETY: We're storing the stream with the device, so the lifetime is valid
        // as long as both are held together
        self.stream = Some(unsafe { std::mem::transmute(stream) });
        self.capturing = true;
        self.sequence = 0;
        Ok(())
    }

    #[cfg(not(feature = "linux"))]
    fn start(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::Platform(
            "V4L2 support requires the 'linux' feature".into(),
        ))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        #[cfg(feature = "linux")]
        {
            self.stream = None;
        }
        self.capturing = false;
        Ok(())
    }

    fn close(&mut self) {
        let _ = self.stop();
        #[cfg(feature = "linux")]
        {
            self.device = None;
        }
        self.negotiated_format = None;
    }

    fn is_capturing(&self) -> bool {
        self.capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.negotiated_format.clone()
    }

    #[cfg(feature = "linux")]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| CaptureError::Platform("Stream not started".into()))?;

        let (buf, meta) = stream
            .next()
            .map_err(|e| CaptureError::Platform(format!("Failed to capture frame: {}", e)))?;

        let device = self.device.as_ref().unwrap();
        let fmt = device.format().map_err(|e| CaptureError::Platform(e.to_string()))?;

        self.sequence += 1;

        Ok(Frame {
            data: buf.to_vec(),
            format: fourcc_to_format(fmt.fourcc.repr),
            width: fmt.width,
            height: fmt.height,
            timestamp_ns: meta.timestamp.sec as u64 * 1_000_000_000
                + meta.timestamp.usec as u64 * 1_000,
            sequence: self.sequence,
        })
    }

    #[cfg(not(feature = "linux"))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Err(CaptureError::Platform(
            "V4L2 support requires the 'linux' feature".into(),
        ))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_video_devices() {
        // This test just verifies the function doesn't panic
        // Actual devices may or may not be present
        let devices = V4l2Enumerator::find_video_devices();
        // Should return a vec (possibly empty)
        assert!(devices.len() >= 0);
    }

    #[test]
    fn test_enumerator_creation() {
        let enumerator = V4l2Enumerator::new();
        // Should not panic
        let devices = enumerator.enumerate();
        assert!(devices.is_ok());
    }

    #[test]
    fn test_backend_creation() {
        let backend = V4l2Backend::new();
        assert!(!backend.is_capturing());
    }
}
