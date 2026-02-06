//! Nokhwa-based camera capture backend
//!
//! This module provides cross-platform camera capture using the nokhwa crate.
//! It supports Windows (Media Foundation), macOS (AVFoundation), and Linux (V4L2).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

use crate::capture::enumerator::CameraEnumerator;
use crate::capture::CaptureBackend;
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

// ============================================================================
// Nokhwa Enumerator
// ============================================================================

/// Nokhwa-based camera enumerator
pub struct NokhwaEnumerator {
    /// Cached device list
    devices: Vec<CameraDevice>,
}

impl NokhwaEnumerator {
    /// Create a new nokhwa enumerator
    pub fn new() -> Self {
        // Initialize nokhwa on macOS
        #[cfg(target_os = "macos")]
        {
            nokhwa::nokhwa_initialize(|granted| {
                if granted {
                    tracing::info!("Camera permission granted");
                } else {
                    tracing::warn!("Camera permission denied");
                }
            });
        }

        let mut enumerator = Self {
            devices: Vec::new(),
        };
        let _ = enumerator.refresh();
        enumerator
    }

    /// Convert nokhwa camera info to our CameraDevice
    fn convert_device(index: u32, info: &nokhwa::utils::CameraInfo) -> CameraDevice {
        // Query capabilities if possible
        let capabilities = Self::query_capabilities(index);

        CameraDevice {
            id: DeviceId(format!("nokhwa:{}", index)),
            name: info.human_name().to_string(),
            description: Some(info.description().to_string()),
            is_available: true,
            capabilities,
        }
    }

    /// Query device capabilities
    fn query_capabilities(index: u32) -> Vec<CameraCapability> {
        // Try to open camera briefly to query formats
        let camera_index = CameraIndex::Index(index);
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);

        match Camera::new(camera_index, requested) {
            Ok(camera) => {
                let mut capabilities = Vec::new();

                // Get compatible formats
                if let Ok(formats) = camera.compatible_list_by_resolution(
                    nokhwa::utils::FrameFormat::MJPEG,
                ) {
                    for (resolution, fps_list) in formats {
                        for fps in fps_list {
                            capabilities.push(CameraCapability {
                                width: resolution.width(),
                                height: resolution.height(),
                                framerate: fps as f32,
                                format: PixelFormat::Mjpeg,
                            });
                        }
                    }
                }

                // Also try YUYV
                if let Ok(formats) = camera.compatible_list_by_resolution(
                    nokhwa::utils::FrameFormat::YUYV,
                ) {
                    for (resolution, fps_list) in formats {
                        for fps in fps_list {
                            capabilities.push(CameraCapability {
                                width: resolution.width(),
                                height: resolution.height(),
                                framerate: fps as f32,
                                format: PixelFormat::Yuyv,
                            });
                        }
                    }
                }

                capabilities
            }
            Err(e) => {
                tracing::debug!("Could not query capabilities for camera {}: {}", index, e);
                // Return common defaults
                vec![
                    CameraCapability {
                        width: 1920,
                        height: 1080,
                        framerate: 30.0,
                        format: PixelFormat::Mjpeg,
                    },
                    CameraCapability {
                        width: 1280,
                        height: 720,
                        framerate: 30.0,
                        format: PixelFormat::Mjpeg,
                    },
                    CameraCapability {
                        width: 640,
                        height: 480,
                        framerate: 30.0,
                        format: PixelFormat::Mjpeg,
                    },
                ]
            }
        }
    }
}

impl Default for NokhwaEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for NokhwaEnumerator {
    fn refresh(&mut self) -> Result<(), CaptureError> {
        self.devices.clear();

        // Query available cameras
        match nokhwa::query(ApiBackend::Auto) {
            Ok(cameras) => {
                for (index, info) in cameras.iter().enumerate() {
                    let device = Self::convert_device(index as u32, info);
                    tracing::info!(
                        id = %device.id.0,
                        name = %device.name,
                        "Found camera"
                    );
                    self.devices.push(device);
                }
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Failed to query cameras: {}", e);
                Err(CaptureError::EnumerationFailed(e.to_string()))
            }
        }
    }

    fn list_devices(&self) -> &[CameraDevice] {
        &self.devices
    }

    fn get_device(&self, id: &DeviceId) -> Option<&CameraDevice> {
        self.devices.iter().find(|d| &d.id == id)
    }
}

// ============================================================================
// Nokhwa Backend
// ============================================================================

/// Nokhwa-based capture backend
pub struct NokhwaBackend {
    /// Active camera
    camera: Option<Camera>,
    /// Current device ID
    device_id: Option<DeviceId>,
    /// Capture settings
    settings: CaptureSettings,
    /// Whether capture is active
    is_capturing: AtomicBool,
    /// Frame counter
    frame_count: AtomicU64,
    /// Last frame timestamp
    last_frame_time: Mutex<Option<Instant>>,
    /// Negotiated format
    negotiated_format: Mutex<Option<NegotiatedFormat>>,
}

impl NokhwaBackend {
    /// Create a new nokhwa backend
    pub fn new() -> Self {
        Self {
            camera: None,
            device_id: None,
            settings: CaptureSettings::default(),
            is_capturing: AtomicBool::new(false),
            frame_count: AtomicU64::new(0),
            last_frame_time: Mutex::new(None),
            negotiated_format: Mutex::new(None),
        }
    }

    /// Parse device index from our DeviceId format
    fn parse_device_index(id: &DeviceId) -> Option<u32> {
        // Format: "nokhwa:N" where N is the index
        id.0.strip_prefix("nokhwa:")
            .and_then(|s| s.parse().ok())
    }
}

impl Default for NokhwaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for NokhwaBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        // Use nokhwa to query devices
        match nokhwa::query(ApiBackend::Auto) {
            Ok(cameras) => {
                cameras.iter().enumerate().map(|(index, info)| {
                    let capabilities = NokhwaEnumerator::query_capabilities(index as u32);
                    CameraDevice {
                        id: DeviceId(format!("nokhwa:{}", index)),
                        name: info.human_name().to_string(),
                        description: Some(info.description().to_string()),
                        is_available: true,
                        capabilities,
                    }
                }).collect()
            }
            Err(e) => {
                tracing::warn!("Failed to enumerate devices: {}", e);
                Vec::new()
            }
        }
    }

    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        self.settings = settings;

        let index = Self::parse_device_index(device_id)
            .ok_or_else(|| CaptureError::DeviceNotFound(device_id.0.clone()))?;

        let camera_index = CameraIndex::Index(index);

        // Request format based on settings
        let requested = RequestedFormat::new::<RgbFormat>(
            RequestedFormatType::Closest(nokhwa::utils::CameraFormat::new(
                nokhwa::utils::Resolution::new(self.settings.width, self.settings.height),
                nokhwa::utils::FrameFormat::MJPEG,
                self.settings.framerate as u32,
            )),
        );

        let camera = Camera::new(camera_index, requested)
            .map_err(|e| CaptureError::DeviceOpenFailed(e.to_string()))?;

        // Get actual negotiated format
        let format = camera.camera_format();
        let negotiated = NegotiatedFormat {
            width: format.resolution().width(),
            height: format.resolution().height(),
            framerate: format.frame_rate() as f32,
            pixel_format: match format.format() {
                nokhwa::utils::FrameFormat::MJPEG => PixelFormat::Mjpeg,
                nokhwa::utils::FrameFormat::YUYV => PixelFormat::Yuyv,
                _ => PixelFormat::Rgb24,
            },
        };

        tracing::info!(
            device = %device_id.0,
            width = negotiated.width,
            height = negotiated.height,
            fps = negotiated.framerate,
            format = ?negotiated.pixel_format,
            "Camera opened with format"
        );

        *self.negotiated_format.lock().unwrap() = Some(negotiated.clone());
        self.camera = Some(camera);
        self.device_id = Some(device_id.clone());

        Ok(negotiated)
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        let camera = self.camera.as_mut()
            .ok_or(CaptureError::NotInitialized)?;

        camera.open_stream()
            .map_err(|e| CaptureError::StreamStartFailed(e.to_string()))?;

        self.is_capturing.store(true, Ordering::SeqCst);
        self.frame_count.store(0, Ordering::SeqCst);
        *self.last_frame_time.lock().unwrap() = Some(Instant::now());

        tracing::info!("Camera stream started");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        if !self.is_capturing.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(camera) = &mut self.camera {
            let _ = camera.stop_stream();
        }

        self.is_capturing.store(false, Ordering::SeqCst);
        tracing::info!("Camera stream stopped");
        Ok(())
    }

    fn close(&mut self) {
        let _ = self.stop();
        self.camera = None;
        self.device_id = None;
        *self.negotiated_format.lock().unwrap() = None;
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing.load(Ordering::SeqCst)
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.negotiated_format.lock().unwrap().clone()
    }

    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        let camera = self.camera.as_mut()
            .ok_or(CaptureError::NotInitialized)?;

        if !self.is_capturing.load(Ordering::SeqCst) {
            return Err(CaptureError::NotCapturing);
        }

        // Capture frame
        let buffer = camera.frame()
            .map_err(|e| CaptureError::FrameCaptureFailed(e.to_string()))?;

        let format = self.negotiated_format.lock().unwrap()
            .clone()
            .ok_or(CaptureError::NotInitialized)?;

        // Convert to RGBA
        let rgba_data = buffer.decode_image::<RgbFormat>()
            .map_err(|e| CaptureError::FrameCaptureFailed(format!("Decode failed: {}", e)))?;

        // Convert RGB to RGBA
        let rgb_bytes = rgba_data.as_raw();
        let mut rgba_bytes = Vec::with_capacity((format.width * format.height * 4) as usize);
        for chunk in rgb_bytes.chunks(3) {
            rgba_bytes.push(chunk[0]); // R
            rgba_bytes.push(chunk[1]); // G
            rgba_bytes.push(chunk[2]); // B
            rgba_bytes.push(255);      // A
        }

        let frame_num = self.frame_count.fetch_add(1, Ordering::SeqCst);
        let now = Instant::now();
        let mut last_time = self.last_frame_time.lock().unwrap();
        let timestamp = last_time.map(|t| now.duration_since(t)).unwrap_or_default();
        *last_time = Some(now);

        Ok(Frame {
            data: rgba_bytes,
            width: format.width,
            height: format.height,
            stride: format.width * 4,
            format: PixelFormat::Rgba,
            timestamp,
            sequence: frame_num,
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_device_index() {
        assert_eq!(NokhwaBackend::parse_device_index(&DeviceId("nokhwa:0".into())), Some(0));
        assert_eq!(NokhwaBackend::parse_device_index(&DeviceId("nokhwa:5".into())), Some(5));
        assert_eq!(NokhwaBackend::parse_device_index(&DeviceId("other:0".into())), None);
        assert_eq!(NokhwaBackend::parse_device_index(&DeviceId("invalid".into())), None);
    }

    #[test]
    fn test_backend_creation() {
        let backend = NokhwaBackend::new();
        assert!(!backend.is_capturing());
        assert!(backend.device_id().is_none());
    }

    #[test]
    fn test_enumerator_creation() {
        // This test may find cameras or not depending on the system
        let enumerator = NokhwaEnumerator::new();
        let devices = enumerator.list_devices();
        println!("Found {} cameras", devices.len());
        for device in devices {
            println!("  - {} ({})", device.name, device.id.0);
        }
    }
}
