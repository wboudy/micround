//! macOS AVFoundation camera support
//!
//! Provides camera enumeration and capture using Apple's AVFoundation framework.
//!
//! # Requirements
//! - macOS 10.15 (Catalina) or later
//! - Camera permission granted via TCC
//!
//! # Architecture
//!
//! AVFoundation is Apple's modern media framework:
//! - `AVCaptureDevice` for device enumeration
//! - `AVCaptureSession` for capture pipeline
//! - `AVCaptureVideoDataOutput` for frame delivery

use std::collections::HashMap;

use tracing::{debug, error, info, trace, warn};

use crate::capture::enumerator::CameraEnumerator;
use crate::capture::CaptureBackend;
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

// ============================================================================
// Objective-C Bindings (macOS only)
// ============================================================================

#[cfg(target_os = "macos")]
mod objc_bindings {
    use objc2::rc::{Retained, Allocated};
    use objc2::runtime::{AnyClass, AnyObject, Sel, Bool};
    use objc2::{class, msg_send, msg_send_id, sel, ClassType};
    use objc2::ffi::NSInteger;
    use std::ptr::NonNull;

    /// AVAuthorizationStatus enumeration
    #[repr(isize)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AVAuthorizationStatus {
        NotDetermined = 0,
        Restricted = 1,
        Denied = 2,
        Authorized = 3,
    }

    /// AVMediaType for video
    pub fn av_media_type_video() -> &'static str {
        "vide" // AVMediaTypeVideo FourCC
    }

    /// Check camera authorization status
    pub fn authorization_status() -> AVAuthorizationStatus {
        unsafe {
            let cls = class!(AVCaptureDevice);
            let status: NSInteger = msg_send![cls, authorizationStatusForMediaType: av_media_type_video()];
            match status {
                0 => AVAuthorizationStatus::NotDetermined,
                1 => AVAuthorizationStatus::Restricted,
                2 => AVAuthorizationStatus::Denied,
                3 => AVAuthorizationStatus::Authorized,
                _ => AVAuthorizationStatus::Denied,
            }
        }
    }

    /// Request camera access (blocking - calls completion handler)
    pub fn request_access() -> bool {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let granted = Arc::new(AtomicBool::new(false));
        let granted_clone = granted.clone();

        // Note: In a real implementation, we'd use a proper async/completion handler
        // This is a simplified synchronous version
        unsafe {
            let cls = class!(AVCaptureDevice);
            // For now, just check status - proper async request needs block support
            let status = authorization_status();
            match status {
                AVAuthorizationStatus::Authorized => return true,
                AVAuthorizationStatus::NotDetermined => {
                    // Request would be async - for now, return false
                    return false;
                }
                _ => return false,
            }
        }
    }
}

// ============================================================================
// AVFoundation Enumerator
// ============================================================================

/// AVFoundation-based camera enumerator
pub struct AVFoundationEnumerator {
    /// Cached device list
    devices: HashMap<String, CameraDevice>,
}

impl AVFoundationEnumerator {
    pub fn new() -> Self {
        let mut enumerator = Self {
            devices: HashMap::new(),
        };
        // Initial device scan
        let _ = enumerator.refresh();
        enumerator
    }

    /// Enumerate video capture devices using AVFoundation
    #[cfg(target_os = "macos")]
    fn enumerate_devices_internal() -> Result<Vec<CameraDevice>, CaptureError> {
        use objc2::{class, msg_send, msg_send_id, sel};
        use objc2::rc::Retained;

        let mut devices = Vec::new();

        // Check authorization first
        let status = objc_bindings::authorization_status();
        if status != objc_bindings::AVAuthorizationStatus::Authorized {
            warn!(?status, "Camera access not authorized");
            if status == objc_bindings::AVAuthorizationStatus::NotDetermined {
                // Could request access here
                info!("Camera access not yet determined - user will be prompted on first capture");
            }
        }

        unsafe {
            // Get AVCaptureDevice class
            let device_cls = class!(AVCaptureDevice);

            // Get all video devices
            // Note: devicesWithMediaType: is deprecated, but works for now
            // Modern approach uses AVCaptureDeviceDiscoverySession
            let video_type = objc_bindings::av_media_type_video();
            let device_array: *const objc2::runtime::AnyObject =
                msg_send![device_cls, devicesWithMediaType: video_type];

            if device_array.is_null() {
                return Ok(devices);
            }

            // Get array count
            let count: usize = msg_send![device_array, count];

            for i in 0..count {
                let device: *const objc2::runtime::AnyObject = msg_send![device_array, objectAtIndex: i];
                if device.is_null() {
                    continue;
                }

                // Get unique ID
                let unique_id: *const objc2::runtime::AnyObject = msg_send![device, uniqueID];
                let unique_id_str = nsstring_to_string(unique_id);

                // Get localized name
                let name: *const objc2::runtime::AnyObject = msg_send![device, localizedName];
                let name_str = nsstring_to_string(name);

                // Get manufacturer (if available)
                let manufacturer: *const objc2::runtime::AnyObject = msg_send![device, manufacturer];
                let manufacturer_str = if !manufacturer.is_null() {
                    Some(nsstring_to_string(manufacturer))
                } else {
                    None
                };

                // Query capabilities
                let capabilities = query_device_capabilities(device);

                devices.push(CameraDevice {
                    id: DeviceId(unique_id_str),
                    name: name_str,
                    manufacturer: manufacturer_str,
                    capabilities,
                    is_available: true,
                });
            }
        }

        debug!(count = devices.len(), "Enumerated AVFoundation devices");
        Ok(devices)
    }

    #[cfg(not(target_os = "macos"))]
    fn enumerate_devices_internal() -> Result<Vec<CameraDevice>, CaptureError> {
        Ok(Vec::new())
    }
}

impl Default for AVFoundationEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for AVFoundationEnumerator {
    fn list_devices(&self) -> Vec<CameraDevice> {
        self.devices.values().cloned().collect()
    }

    fn get_device(&self, device_id: &DeviceId) -> Option<CameraDevice> {
        self.devices.get(&device_id.0).cloned()
    }

    fn refresh(&mut self) -> Result<(), CaptureError> {
        #[cfg(target_os = "macos")]
        {
            let devices = Self::enumerate_devices_internal()?;
            self.devices.clear();
            for device in devices {
                self.devices.insert(device.id.0.clone(), device);
            }
        }
        Ok(())
    }
}

// ============================================================================
// Helper Functions (macOS only)
// ============================================================================

#[cfg(target_os = "macos")]
unsafe fn nsstring_to_string(nsstring: *const objc2::runtime::AnyObject) -> String {
    if nsstring.is_null() {
        return String::new();
    }

    use objc2::msg_send;

    let utf8: *const std::os::raw::c_char = msg_send![nsstring, UTF8String];
    if utf8.is_null() {
        return String::new();
    }

    std::ffi::CStr::from_ptr(utf8)
        .to_string_lossy()
        .into_owned()
}

#[cfg(target_os = "macos")]
unsafe fn query_device_capabilities(device: *const objc2::runtime::AnyObject) -> Vec<CameraCapability> {
    use objc2::msg_send;

    let mut capabilities = Vec::new();

    // Get formats array
    let formats: *const objc2::runtime::AnyObject = msg_send![device, formats];
    if formats.is_null() {
        return capabilities;
    }

    let count: usize = msg_send![formats, count];

    for i in 0..count {
        let format: *const objc2::runtime::AnyObject = msg_send![formats, objectAtIndex: i];
        if format.is_null() {
            continue;
        }

        // Get format description
        let format_desc: *const std::ffi::c_void = msg_send![format, formatDescription];
        if format_desc.is_null() {
            continue;
        }

        // Get dimensions via CMVideoFormatDescriptionGetDimensions
        // Note: This requires linking against CoreMedia
        // For now, we'll use a simplified approach

        // Get video supported frame rate ranges
        let frame_rate_ranges: *const objc2::runtime::AnyObject =
            msg_send![format, videoSupportedFrameRateRanges];
        if frame_rate_ranges.is_null() {
            continue;
        }

        let range_count: usize = msg_send![frame_rate_ranges, count];
        for j in 0..range_count {
            let range: *const objc2::runtime::AnyObject =
                msg_send![frame_rate_ranges, objectAtIndex: j];
            if range.is_null() {
                continue;
            }

            let max_fps: f64 = msg_send![range, maxFrameRate];

            // For now, use common resolutions - proper implementation needs CMFormatDescription
            // This is a placeholder - real implementation would parse formatDescription
            capabilities.push(CameraCapability {
                width: 1920,
                height: 1080,
                framerate: max_fps as f32,
                format: PixelFormat::Nv12, // AVFoundation typically uses NV12/420v
            });
        }
    }

    // Deduplicate
    capabilities.dedup_by(|a, b| {
        a.width == b.width && a.height == b.height && a.format == b.format
    });

    capabilities
}

// ============================================================================
// AVFoundation Backend
// ============================================================================

/// AVFoundation-based capture backend
pub struct AVFoundationBackend {
    #[cfg(target_os = "macos")]
    capture_session: Option<*const objc2::runtime::AnyObject>,
    #[cfg(target_os = "macos")]
    video_output: Option<*const objc2::runtime::AnyObject>,
    current_device: Option<DeviceId>,
    current_format: Option<NegotiatedFormat>,
    is_capturing: bool,
    frame_sequence: u64,
    // Frame buffer for receiving frames from delegate
    #[cfg(target_os = "macos")]
    frame_buffer: std::sync::Arc<std::sync::Mutex<Option<Frame>>>,
}

impl AVFoundationBackend {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            capture_session: None,
            #[cfg(target_os = "macos")]
            video_output: None,
            current_device: None,
            current_format: None,
            is_capturing: false,
            frame_sequence: 0,
            #[cfg(target_os = "macos")]
            frame_buffer: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Check and request camera authorization
    #[cfg(target_os = "macos")]
    fn check_authorization(&self) -> Result<(), CaptureError> {
        let status = objc_bindings::authorization_status();
        match status {
            objc_bindings::AVAuthorizationStatus::Authorized => Ok(()),
            objc_bindings::AVAuthorizationStatus::NotDetermined => {
                // Request access
                if objc_bindings::request_access() {
                    Ok(())
                } else {
                    Err(CaptureError::PermissionDenied(
                        "Camera access not yet granted - please allow in System Preferences".to_string(),
                    ))
                }
            }
            objc_bindings::AVAuthorizationStatus::Denied => {
                Err(CaptureError::PermissionDenied(
                    "Camera access denied. Open System Preferences > Privacy & Security > Camera to allow access.".to_string(),
                ))
            }
            objc_bindings::AVAuthorizationStatus::Restricted => {
                Err(CaptureError::PermissionDenied(
                    "Camera access is restricted by system policy.".to_string(),
                ))
            }
        }
    }

    /// Find a device by ID
    #[cfg(target_os = "macos")]
    fn find_device(&self, device_id: &DeviceId) -> Result<*const objc2::runtime::AnyObject, CaptureError> {
        use objc2::{class, msg_send};

        unsafe {
            let device_cls = class!(AVCaptureDevice);
            let nsstring = string_to_nsstring(&device_id.0);
            let device: *const objc2::runtime::AnyObject =
                msg_send![device_cls, deviceWithUniqueID: nsstring];

            if device.is_null() {
                Err(CaptureError::DeviceNotFound(device_id.0.clone()))
            } else {
                Ok(device)
            }
        }
    }

    /// Configure capture session with the device
    #[cfg(target_os = "macos")]
    fn configure_session(
        &mut self,
        device: *const objc2::runtime::AnyObject,
        settings: &CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        use objc2::{class, msg_send, msg_send_id};

        unsafe {
            // Create capture session
            let session_cls = class!(AVCaptureSession);
            let session: *const objc2::runtime::AnyObject = msg_send![session_cls, new];
            if session.is_null() {
                return Err(CaptureError::Platform("Failed to create capture session".to_string()));
            }

            // Begin configuration
            let _: () = msg_send![session, beginConfiguration];

            // Set session preset based on requested resolution
            let preset = match (settings.width, settings.height) {
                (w, h) if w >= 3840 && h >= 2160 => "AVCaptureSessionPreset3840x2160",
                (w, h) if w >= 1920 && h >= 1080 => "AVCaptureSessionPreset1920x1080",
                (w, h) if w >= 1280 && h >= 720 => "AVCaptureSessionPreset1280x720",
                (w, h) if w >= 640 && h >= 480 => "AVCaptureSessionPreset640x480",
                _ => "AVCaptureSessionPresetHigh",
            };

            // Create device input
            let input_cls = class!(AVCaptureDeviceInput);
            let error_ptr: *mut *const objc2::runtime::AnyObject = std::ptr::null_mut();
            let input: *const objc2::runtime::AnyObject =
                msg_send![input_cls, deviceInputWithDevice: device error: error_ptr];

            if input.is_null() {
                let _: () = msg_send![session, commitConfiguration];
                return Err(CaptureError::Platform("Failed to create device input".to_string()));
            }

            // Add input to session
            let can_add: bool = msg_send![session, canAddInput: input];
            if !can_add {
                let _: () = msg_send![session, commitConfiguration];
                return Err(CaptureError::DeviceBusy(
                    "Cannot add device input to session".to_string(),
                ));
            }
            let _: () = msg_send![session, addInput: input];

            // Create video output
            let output_cls = class!(AVCaptureVideoDataOutput);
            let output: *const objc2::runtime::AnyObject = msg_send![output_cls, new];
            if output.is_null() {
                let _: () = msg_send![session, commitConfiguration];
                return Err(CaptureError::Platform("Failed to create video output".to_string()));
            }

            // Configure output settings - prefer NV12 (kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
            // This is "420v" FourCC = 0x34323076

            // Add output to session
            let can_add_output: bool = msg_send![session, canAddOutput: output];
            if !can_add_output {
                let _: () = msg_send![session, commitConfiguration];
                return Err(CaptureError::Platform("Cannot add video output to session".to_string()));
            }
            let _: () = msg_send![session, addOutput: output];

            // Commit configuration
            let _: () = msg_send![session, commitConfiguration];

            self.capture_session = Some(session);
            self.video_output = Some(output);

            // Return negotiated format
            // Note: Actual format depends on session preset and device capabilities
            let negotiated = NegotiatedFormat {
                width: settings.width.min(1920),
                height: settings.height.min(1080),
                framerate: settings.framerate.min(30.0),
                format: PixelFormat::Nv12,
            };

            Ok(negotiated)
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn string_to_nsstring(s: &str) -> *const objc2::runtime::AnyObject {
    use objc2::{class, msg_send};
    use std::ffi::CString;

    let cls = class!(NSString);
    let c_str = CString::new(s).unwrap_or_default();
    let nsstring: *const objc2::runtime::AnyObject =
        msg_send![cls, stringWithUTF8String: c_str.as_ptr()];
    nsstring
}

impl Default for AVFoundationBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for AVFoundationBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        #[cfg(target_os = "macos")]
        {
            AVFoundationEnumerator::enumerate_devices_internal().unwrap_or_default()
        }
        #[cfg(not(target_os = "macos"))]
        {
            Vec::new()
        }
    }

    #[cfg(target_os = "macos")]
    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        // Close any existing session
        self.close();

        // Check authorization
        self.check_authorization()?;

        // Find the device
        let device = self.find_device(device_id)?;

        // Configure capture session
        let negotiated = self.configure_session(device, &settings)?;

        self.current_device = Some(device_id.clone());
        self.current_format = Some(negotiated.clone());

        info!(
            device = %device_id,
            width = negotiated.width,
            height = negotiated.height,
            fps = negotiated.framerate,
            format = ?negotiated.format,
            "AVFoundation device opened"
        );

        Ok(negotiated)
    }

    #[cfg(not(target_os = "macos"))]
    fn open(
        &mut self,
        _device_id: &DeviceId,
        _settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        Err(CaptureError::Platform(
            "AVFoundation not available on this platform".to_string(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn start(&mut self) -> Result<(), CaptureError> {
        use objc2::msg_send;

        if let Some(session) = self.capture_session {
            unsafe {
                let _: () = msg_send![session, startRunning];
            }
            self.is_capturing = true;
            self.frame_sequence = 0;
            debug!("AVFoundation capture started");
            Ok(())
        } else {
            Err(CaptureError::Platform("No capture session".to_string()))
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn start(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::Platform(
            "AVFoundation not available on this platform".to_string(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn stop(&mut self) -> Result<(), CaptureError> {
        use objc2::msg_send;

        if let Some(session) = self.capture_session {
            unsafe {
                let _: () = msg_send![session, stopRunning];
            }
        }
        self.is_capturing = false;
        debug!("AVFoundation capture stopped");
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn stop(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }

    fn close(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.is_capturing {
                let _ = self.stop();
            }
            self.capture_session = None;
            self.video_output = None;
        }
        self.current_device = None;
        self.current_format = None;
        debug!("AVFoundation device closed");
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.current_format.clone()
    }

    #[cfg(target_os = "macos")]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        if !self.is_capturing {
            return Err(CaptureError::Platform("Not capturing".to_string()));
        }

        // In a real implementation, this would receive frames from the delegate
        // For now, return a timeout error as placeholder
        // The actual frame delivery happens via AVCaptureVideoDataOutputSampleBufferDelegate
        // which requires Objective-C delegate implementation

        // Check frame buffer
        if let Ok(mut guard) = self.frame_buffer.lock() {
            if let Some(frame) = guard.take() {
                return Ok(frame);
            }
        }

        // No frame available - in production, would wait with timeout
        Err(CaptureError::FrameTimeout(std::time::Duration::from_millis(100)))
    }

    #[cfg(not(target_os = "macos"))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Err(CaptureError::Platform(
            "AVFoundation not available on this platform".to_string(),
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
    fn test_backend_creation() {
        let backend = AVFoundationBackend::new();
        assert!(!backend.is_capturing());
        assert!(backend.current_format().is_none());
    }

    #[test]
    fn test_enumerator_creation() {
        let enumerator = AVFoundationEnumerator::new();
        // Should not panic, devices list may be empty on non-macOS
        let _ = enumerator.list_devices();
    }

    #[test]
    fn test_start_without_open() {
        let mut backend = AVFoundationBackend::new();
        let result = backend.start();
        // Should fail since no device is open
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_without_start() {
        let mut backend = AVFoundationBackend::new();
        // Should not panic
        let _ = backend.stop();
    }

    #[test]
    fn test_close_without_open() {
        let mut backend = AVFoundationBackend::new();
        // Should not panic
        backend.close();
    }

    #[test]
    fn test_next_frame_without_capturing() {
        let mut backend = AVFoundationBackend::new();
        let result = backend.next_frame();
        assert!(result.is_err());
    }

    #[test]
    fn test_enumerate_devices_empty_on_non_macos() {
        // On non-macOS platforms, should return empty list
        #[cfg(not(target_os = "macos"))]
        {
            let devices = AVFoundationEnumerator::enumerate_devices_internal();
            assert!(devices.is_ok());
            assert!(devices.unwrap().is_empty());
        }
    }
}
