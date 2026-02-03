//! Windows Media Foundation camera support
//!
//! Provides camera enumeration and capture using the Media Foundation API.
//!
//! # Requirements
//! - Windows 7 or later
//! - Media Foundation components (included in Windows)
//!
//! # Architecture
//!
//! Media Foundation is the modern Windows multimedia framework:
//! - `MFEnumDeviceSources` for device enumeration
//! - `IMFSourceReader` for frame capture
//! - `IMFMediaType` for format negotiation
//!
//! # Status
//!
//! This module is a placeholder. The Windows API bindings have changed in recent
//! versions of the `windows` crate and require updates. Full implementation is
//! tracked as a separate work item.

use std::collections::HashMap;

use tracing::{debug, info};

use crate::capture::enumerator::CameraEnumerator;
use crate::capture::CaptureBackend;
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

// ============================================================================
// Media Foundation Enumerator (Placeholder)
// ============================================================================

/// Media Foundation-based camera enumerator
///
/// Note: This is currently a placeholder implementation. The full Media Foundation-based
/// enumeration requires updates to work with recent Windows API binding changes.
pub struct MediaFoundationEnumerator {
    /// Cached device list (empty in placeholder mode)
    devices: HashMap<String, CameraDevice>,
}

impl MediaFoundationEnumerator {
    pub fn new() -> Self {
        let enumerator = Self {
            devices: HashMap::new(),
        };
        info!("Media Foundation enumerator initialized (placeholder mode)");
        enumerator
    }
}

impl Default for MediaFoundationEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for MediaFoundationEnumerator {
    fn enumerate(&self) -> Result<Vec<CameraDevice>, CaptureError> {
        // Placeholder: Return empty list
        // Full implementation requires Windows API updates
        Ok(self.devices.values().cloned().collect())
    }

    fn get_device(&self, device_id: &DeviceId) -> Result<CameraDevice, CaptureError> {
        self.devices
            .get(&device_id.0)
            .cloned()
            .ok_or_else(|| CaptureError::DeviceNotFound(device_id.0.clone()))
    }

    fn get_capabilities(&self, id: &DeviceId) -> Result<Vec<CameraCapability>, CaptureError> {
        self.devices
            .get(&id.0)
            .map(|d| d.capabilities.clone())
            .ok_or_else(|| CaptureError::DeviceNotFound(id.0.clone()))
    }

    fn is_available(&self, id: &DeviceId) -> bool {
        self.devices.contains_key(&id.0)
    }

    fn refresh(&mut self) -> Result<(), CaptureError> {
        // Placeholder: Nothing to refresh
        Ok(())
    }
}

// ============================================================================
// Media Foundation Backend (Placeholder)
// ============================================================================

/// Media Foundation-based capture backend
///
/// Note: This is currently a placeholder implementation. The full Media Foundation-based
/// capture requires updates to work with recent Windows API binding changes.
pub struct MediaFoundationBackend {
    current_device: Option<DeviceId>,
    current_format: Option<NegotiatedFormat>,
    is_capturing: bool,
    frame_sequence: u64,
}

impl MediaFoundationBackend {
    pub fn new() -> Self {
        Self {
            current_device: None,
            current_format: None,
            is_capturing: false,
            frame_sequence: 0,
        }
    }
}

impl Default for MediaFoundationBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for MediaFoundationBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        // Placeholder: Return empty list
        Vec::new()
    }

    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        // Placeholder: Pretend to open the device and configure
        self.current_device = Some(device_id.clone());

        let negotiated = NegotiatedFormat {
            width: settings.width.min(1920),
            height: settings.height.min(1080),
            framerate: settings.framerate.min(30.0),
            format: PixelFormat::Nv12,
            exact_match: false,
        };
        self.current_format = Some(negotiated.clone());

        debug!(
            device = %device_id.0,
            width = negotiated.width,
            height = negotiated.height,
            fps = negotiated.framerate,
            "Media Foundation device opened (placeholder mode)"
        );
        Ok(negotiated)
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        if self.current_device.is_none() {
            return Err(CaptureError::Platform("No device opened".to_string()));
        }
        self.is_capturing = true;
        self.frame_sequence = 0;
        info!("Media Foundation capture started (placeholder mode - no frames will be delivered)");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.is_capturing = false;
        info!("Media Foundation capture stopped (placeholder mode)");
        Ok(())
    }

    fn close(&mut self) {
        self.is_capturing = false;
        self.current_device = None;
        self.current_format = None;
        debug!("Media Foundation device closed (placeholder mode)");
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.current_format.clone()
    }

    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        if !self.is_capturing {
            return Err(CaptureError::Platform("Not capturing".to_string()));
        }

        // Placeholder: Return a black frame
        let format = self
            .current_format
            .as_ref()
            .ok_or(CaptureError::Platform("Not configured".to_string()))?;

        let data_len = (format.width * format.height * 3 / 2) as usize; // NV12 format
        let frame = Frame {
            data: vec![0u8; data_len],
            width: format.width,
            height: format.height,
            format: format.format,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64,
            sequence: self.frame_sequence,
        };
        self.frame_sequence += 1;

        Ok(frame)
    }
}

// MediaFoundationBackend is Send because it doesn't contain any raw pointers in
// placeholder mode.
unsafe impl Send for MediaFoundationBackend {}

impl Drop for MediaFoundationBackend {
    fn drop(&mut self) {
        self.close();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerator_creation() {
        let enumerator = MediaFoundationEnumerator::new();
        // Placeholder returns empty list
        assert!(enumerator.enumerate().unwrap().is_empty());
    }

    #[test]
    fn test_backend_creation() {
        let backend = MediaFoundationBackend::new();
        assert!(!backend.is_capturing());
        assert!(backend.current_device.is_none());
    }

    #[test]
    fn test_backend_lifecycle() {
        let mut backend = MediaFoundationBackend::new();

        // Open with settings
        let device_id = DeviceId("test-device".to_string());
        let settings = CaptureSettings {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: None,
        };
        let format = backend.open(&device_id, settings).unwrap();
        assert_eq!(format.width, 1920);
        assert_eq!(format.height, 1080);

        // Start
        assert!(backend.start().is_ok());
        assert!(backend.is_capturing());

        // Read frame (placeholder returns black frame)
        let frame = backend.next_frame().unwrap();
        assert_eq!(frame.width, 1920);
        assert_eq!(frame.height, 1080);
        assert_eq!(frame.sequence, 0);

        // Stop
        assert!(backend.stop().is_ok());
        assert!(!backend.is_capturing());

        // Close
        backend.close();
        assert!(backend.current_format().is_none());
    }

    #[test]
    fn test_next_frame_without_start() {
        let mut backend = MediaFoundationBackend::new();
        let result = backend.next_frame();
        assert!(result.is_err());
    }
}
