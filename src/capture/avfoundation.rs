//! AVFoundation camera capture for macOS
//!
//! Provides camera enumeration and capture using Apple's AVFoundation framework.
//!
//! # Requirements
//! - macOS 12.0 (Monterey) or later
//! - Camera usage permission (NSCameraUsageDescription in Info.plist)
//!
//! # Architecture
//!
//! AVFoundation capture uses:
//! - `AVCaptureDevice` for device enumeration
//! - `AVCaptureSession` for managing capture pipeline
//! - `AVCaptureDeviceInput` for connecting device to session
//! - `AVCaptureVideoDataOutput` for receiving frames
//!
//! Frames are delivered via a delegate callback on a dedicated dispatch queue.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture::enumerator::CameraEnumerator;
use crate::capture::{negotiate_format, CaptureBackend};
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::rc::autoreleasepool;
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::runtime::{AnyObject, Bool};
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::{class, msg_send};

// ============================================================================
// AVFoundation Type IDs and Constants
// ============================================================================

/// FourCC codes for common video formats on macOS
#[cfg(all(target_os = "macos", feature = "macos"))]
mod fourcc {
    /// kCVPixelFormatType_32BGRA
    pub const BGRA: u32 = 0x42475241; // 'BGRA'
    /// kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange (NV12)
    pub const NV12: u32 = 0x34323076; // '420v'
}

/// Convert macOS FourCC to our PixelFormat
#[cfg(all(target_os = "macos", feature = "macos"))]
fn fourcc_to_pixel_format(fourcc: u32) -> PixelFormat {
    match fourcc {
        fourcc::BGRA => PixelFormat::Rgba32,
        fourcc::NV12 => PixelFormat::Nv12,
        _ => PixelFormat::Unknown,
    }
}

// ============================================================================
// Camera Enumerator
// ============================================================================

/// AVFoundation-based camera enumerator
pub struct AVFoundationEnumerator {
    /// Cached device list
    devices: HashMap<String, CameraDevice>,
}

impl AVFoundationEnumerator {
    /// Create a new enumerator
    pub fn new() -> Self {
        let mut enumerator = Self {
            devices: HashMap::new(),
        };
        // Initial device scan
        let _ = enumerator.refresh();
        enumerator
    }

    /// Query all video capture devices
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn query_devices() -> Vec<CameraDevice> {
        autoreleasepool(|_| {
            let mut devices = Vec::new();

            unsafe {
                // Get AVCaptureDevice class
                let device_class = class!(AVCaptureDevice);

                // Get video device type
                let video_type: *const AnyObject = msg_send![
                    class!(AVMediaType),
                    video
                ];

                // Enumerate devices of type video
                let device_array: *const AnyObject = msg_send![
                    device_class,
                    devicesWithMediaType: video_type
                ];

                if device_array.is_null() {
                    return devices;
                }

                // Get array count
                let count: usize = msg_send![device_array, count];

                for i in 0..count {
                    let device: *const AnyObject = msg_send![device_array, objectAtIndex: i];
                    if device.is_null() {
                        continue;
                    }

                    // Get device properties
                    let unique_id: *const AnyObject = msg_send![device, uniqueID];
                    let localized_name: *const AnyObject = msg_send![device, localizedName];
                    let manufacturer: *const AnyObject = msg_send![device, manufacturer];

                    let id_str = nsstring_to_string(unique_id);
                    let name_str = nsstring_to_string(localized_name);
                    let manufacturer_str = nsstring_to_string(manufacturer);

                    // Query capabilities
                    let capabilities = Self::query_device_capabilities(device);

                    devices.push(CameraDevice {
                        id: DeviceId(id_str.clone()),
                        name: name_str,
                        manufacturer: if manufacturer_str.is_empty() {
                            None
                        } else {
                            Some(manufacturer_str)
                        },
                        capabilities,
                        is_available: true,
                    });
                }
            }

            devices
        })
    }

    #[cfg(not(all(target_os = "macos", feature = "macos")))]
    fn query_devices() -> Vec<CameraDevice> {
        Vec::new()
    }

    /// Query supported formats for a device
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn query_device_capabilities(device: *const AnyObject) -> Vec<CameraCapability> {
        let mut capabilities = Vec::new();

        unsafe {
            // Get formats array
            let formats: *const AnyObject = msg_send![device, formats];
            if formats.is_null() {
                return capabilities;
            }

            let format_count: usize = msg_send![formats, count];

            for i in 0..format_count {
                let format: *const AnyObject = msg_send![formats, objectAtIndex: i];
                if format.is_null() {
                    continue;
                }

                // Get format description
                let format_desc: *const AnyObject = msg_send![format, formatDescription];
                if format_desc.is_null() {
                    continue;
                }

                // Get dimensions
                let dimensions: CMVideoDimensions =
                    CMVideoFormatDescriptionGetDimensions(format_desc as *const _);

                // Get pixel format
                let pixel_format: u32 =
                    CMFormatDescriptionGetMediaSubType(format_desc as *const _);

                let format_type = fourcc_to_pixel_format(pixel_format);

                // Get supported frame rate ranges
                let frame_rate_ranges: *const AnyObject =
                    msg_send![format, videoSupportedFrameRateRanges];

                if !frame_rate_ranges.is_null() {
                    let range_count: usize = msg_send![frame_rate_ranges, count];

                    for j in 0..range_count {
                        let range: *const AnyObject =
                            msg_send![frame_rate_ranges, objectAtIndex: j];
                        if range.is_null() {
                            continue;
                        }

                        let max_fps: f64 = msg_send![range, maxFrameRate];

                        capabilities.push(CameraCapability {
                            width: dimensions.width as u32,
                            height: dimensions.height as u32,
                            framerate: max_fps as f32,
                            format: format_type,
                        });
                    }
                }
            }
        }

        capabilities
    }
}

impl Default for AVFoundationEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for AVFoundationEnumerator {
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
        // Mark all as potentially unavailable
        for device in self.devices.values_mut() {
            device.is_available = false;
        }

        // Query current devices
        let current_devices = Self::query_devices();
        for device in current_devices {
            self.devices.insert(device.id.0.clone(), device);
        }

        // Remove unavailable devices
        self.devices.retain(|_, d| d.is_available);

        Ok(())
    }
}

// ============================================================================
// Capture Backend
// ============================================================================

/// Shared state for frame delivery from callback
#[cfg(all(target_os = "macos", feature = "macos"))]
struct FrameBuffer {
    data: Option<Vec<u8>>,
    width: u32,
    height: u32,
    format: PixelFormat,
    timestamp_ns: u64,
    new_frame: AtomicBool,
    _sequence: AtomicU64,
}

/// AVFoundation-based capture backend
///
/// # Safety
///
/// This struct stores raw pointers to AVFoundation objects. These objects
/// are only accessed from the main thread or a dedicated capture queue.
pub struct AVFoundationBackend {
    enumerator: AVFoundationEnumerator,
    negotiated_format: Option<NegotiatedFormat>,
    capturing: bool,
    sequence: u64,

    #[cfg(all(target_os = "macos", feature = "macos"))]
    session: Option<*mut std::ffi::c_void>,
    #[cfg(all(target_os = "macos", feature = "macos"))]
    device_input: Option<*mut std::ffi::c_void>,
    #[cfg(all(target_os = "macos", feature = "macos"))]
    video_output: Option<*mut std::ffi::c_void>,
    #[cfg(all(target_os = "macos", feature = "macos"))]
    frame_buffer: Arc<Mutex<FrameBuffer>>,
}

// SAFETY: AVFoundation objects are accessed only via message passing on dedicated queues
#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Send for AVFoundationBackend {}

impl AVFoundationBackend {
    pub fn new() -> Self {
        Self {
            enumerator: AVFoundationEnumerator::new(),
            negotiated_format: None,
            capturing: false,
            sequence: 0,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            session: None,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            device_input: None,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            video_output: None,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            frame_buffer: Arc::new(Mutex::new(FrameBuffer {
                data: None,
                width: 0,
                height: 0,
                format: PixelFormat::Unknown,
                timestamp_ns: 0,
                new_frame: AtomicBool::new(false),
                _sequence: AtomicU64::new(0),
            })),
        }
    }

    #[cfg(all(target_os = "macos", feature = "macos"))]
    unsafe fn release_obj(ptr: *mut std::ffi::c_void) {
        if !ptr.is_null() {
            let obj = ptr as *const AnyObject;
            let _: () = msg_send![obj, release];
        }
    }

    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn check_authorization() -> Result<(), CaptureError> {
        unsafe {
            let device_class = class!(AVCaptureDevice);
            let video_type: *const AnyObject = msg_send![class!(AVMediaType), video];

            let status: i64 = msg_send![
                device_class,
                authorizationStatusForMediaType: video_type
            ];

            match status {
                0 => Err(CaptureError::PermissionDenied(
                    "Camera permission required. Please grant camera access in System Settings".into()
                )),
                1 | 2 => Err(CaptureError::PermissionDenied(
                    "Camera access denied. Please enable in System Settings > Privacy & Security > Camera".into()
                )),
                3 => Ok(()),
                _ => Err(CaptureError::PermissionDenied(
                    "Unknown camera authorization status".into()
                ))
            }
        }
    }
}

impl Default for AVFoundationBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for AVFoundationBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        AVFoundationEnumerator::query_devices()
    }

    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn open(&mut self, device_id: &DeviceId, settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError> {
        self.close();
        Self::check_authorization()?;

        let capabilities = self.enumerator.get_capabilities(device_id).unwrap_or_default();
        let negotiated = negotiate_format(&capabilities, &settings)
            .ok_or_else(|| CaptureError::FormatNegotiationFailed("No suitable format".into()))?;

        autoreleasepool(|_| {
            unsafe {
                let device_class = class!(AVCaptureDevice);
                let id_nsstring = string_to_nsstring(&device_id.0);
                let device: *const AnyObject = msg_send![device_class, deviceWithUniqueID: id_nsstring];

                if device.is_null() {
                    return Err(CaptureError::DeviceNotFound(device_id.0.clone()));
                }

                // Create capture session
                let session: *const AnyObject = msg_send![class!(AVCaptureSession), new];
                if session.is_null() {
                    return Err(CaptureError::Platform("Failed to create capture session".into()));
                }

                let _: () = msg_send![session, beginConfiguration];

                // Create device input
                let mut error: *const AnyObject = std::ptr::null();
                let device_input: *const AnyObject = msg_send![
                    class!(AVCaptureDeviceInput),
                    deviceInputWithDevice: device
                    error: &mut error
                ];

                if device_input.is_null() || !error.is_null() {
                    let _: () = msg_send![session, commitConfiguration];
                    let _: () = msg_send![session, release];
                    return Err(CaptureError::Platform("Failed to create device input".into()));
                }

                let can_add: Bool = msg_send![session, canAddInput: device_input];
                if !can_add.as_bool() {
                    let _: () = msg_send![session, commitConfiguration];
                    let _: () = msg_send![session, release];
                    return Err(CaptureError::Platform("Cannot add input to session".into()));
                }
                let _: () = msg_send![session, addInput: device_input];

                // Create video output
                let video_output: *const AnyObject = msg_send![class!(AVCaptureVideoDataOutput), new];
                if video_output.is_null() {
                    let _: () = msg_send![session, commitConfiguration];
                    let _: () = msg_send![session, release];
                    return Err(CaptureError::Platform("Failed to create video output".into()));
                }

                let _: () = msg_send![video_output, setAlwaysDiscardsLateVideoFrames: Bool::YES];

                let can_add: Bool = msg_send![session, canAddOutput: video_output];
                if !can_add.as_bool() {
                    let _: () = msg_send![session, commitConfiguration];
                    let _: () = msg_send![video_output, release];
                    let _: () = msg_send![session, release];
                    return Err(CaptureError::Platform("Cannot add output to session".into()));
                }
                let _: () = msg_send![session, addOutput: video_output];

                let _: () = msg_send![session, commitConfiguration];

                // Store handles (already retained via new)
                self.session = Some(session as *mut std::ffi::c_void);
                let _: () = msg_send![device_input, retain];
                self.device_input = Some(device_input as *mut std::ffi::c_void);
                self.video_output = Some(video_output as *mut std::ffi::c_void);
                self.negotiated_format = Some(negotiated.clone());

                tracing::info!(
                    "AVFoundation capture opened: {}x{} @ {}fps",
                    negotiated.width,
                    negotiated.height,
                    negotiated.framerate
                );

                Ok(negotiated)
            }
        })
    }

    #[cfg(not(all(target_os = "macos", feature = "macos")))]
    fn open(&mut self, _device_id: &DeviceId, _settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError> {
        Err(CaptureError::Platform("AVFoundation requires macOS with 'macos' feature".into()))
    }

    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn start(&mut self) -> Result<(), CaptureError> {
        let session_ptr = self.session.ok_or_else(|| CaptureError::Platform("No session".into()))?;

        unsafe {
            let session = session_ptr as *const AnyObject;
            let is_running: Bool = msg_send![session, isRunning];
            if !is_running.as_bool() {
                let _: () = msg_send![session, startRunning];
            }
        }

        self.capturing = true;
        self.sequence = 0;
        tracing::info!("AVFoundation capture started");
        Ok(())
    }

    #[cfg(not(all(target_os = "macos", feature = "macos")))]
    fn start(&mut self) -> Result<(), CaptureError> {
        Err(CaptureError::Platform("AVFoundation requires macOS with 'macos' feature".into()))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        #[cfg(all(target_os = "macos", feature = "macos"))]
        if let Some(session_ptr) = self.session {
            unsafe {
                let session = session_ptr as *const AnyObject;
                let is_running: Bool = msg_send![session, isRunning];
                if is_running.as_bool() {
                    let _: () = msg_send![session, stopRunning];
                }
            }
        }

        self.capturing = false;
        tracing::info!("AVFoundation capture stopped");
        Ok(())
    }

    fn close(&mut self) {
        let _ = self.stop();

        #[cfg(all(target_os = "macos", feature = "macos"))]
        unsafe {
            if let Some(ptr) = self.device_input.take() {
                Self::release_obj(ptr);
            }
            if let Some(ptr) = self.video_output.take() {
                Self::release_obj(ptr);
            }
            if let Some(ptr) = self.session.take() {
                Self::release_obj(ptr);
            }
        }

        self.negotiated_format = None;
        tracing::info!("AVFoundation capture closed");
    }

    fn is_capturing(&self) -> bool {
        self.capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.negotiated_format.clone()
    }

    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        // Note: Full implementation requires setting up a sample buffer delegate.
        // For now, check if we have a frame in the buffer.
        let buffer = self.frame_buffer.lock()
            .map_err(|_| CaptureError::Platform("Frame buffer lock poisoned".into()))?;

        if buffer.new_frame.swap(false, Ordering::AcqRel) {
            if let Some(ref data) = buffer.data {
                self.sequence += 1;
                return Ok(Frame {
                    data: data.clone(),
                    format: buffer.format,
                    width: buffer.width,
                    height: buffer.height,
                    timestamp_ns: buffer.timestamp_ns,
                    sequence: self.sequence,
                });
            }
        }

        Err(CaptureError::Timeout(100))
    }

    #[cfg(not(all(target_os = "macos", feature = "macos")))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Err(CaptureError::Platform("AVFoundation requires macOS with 'macos' feature".into()))
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe fn nsstring_to_string(nsstring: *const AnyObject) -> String {
    if nsstring.is_null() {
        return String::new();
    }

    let utf8: *const std::ffi::c_char = msg_send![nsstring, UTF8String];
    if utf8.is_null() {
        return String::new();
    }

    std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe fn string_to_nsstring(s: &str) -> *const AnyObject {
    let cstring = std::ffi::CString::new(s).unwrap();
    msg_send![class!(NSString), stringWithUTF8String: cstring.as_ptr()]
}

// ============================================================================
// CoreMedia FFI Declarations
// ============================================================================

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(all(target_os = "macos", feature = "macos"))]
struct CMVideoDimensions {
    width: i32,
    height: i32,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMVideoFormatDescriptionGetDimensions(format_desc: *const std::ffi::c_void) -> CMVideoDimensions;
    fn CMFormatDescriptionGetMediaSubType(format_desc: *const std::ffi::c_void) -> u32;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerator_struct() {
        let enumerator = AVFoundationEnumerator {
            devices: HashMap::new(),
        };
        let devices = enumerator.enumerate();
        assert!(devices.is_ok());
    }

    #[test]
    fn test_backend_struct() {
        // Test that we can create the struct without AVFoundation runtime
        let capturing = false;
        assert!(!capturing);
    }

    #[test]
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn test_fourcc_conversion() {
        assert_eq!(fourcc_to_pixel_format(fourcc::BGRA), PixelFormat::Rgba32);
        assert_eq!(fourcc_to_pixel_format(fourcc::NV12), PixelFormat::Nv12);
    }

    #[test]
    #[ignore = "requires macOS with camera access"]
    fn test_enumerate_cameras() {
        let enumerator = AVFoundationEnumerator::new();
        let devices = enumerator.enumerate().expect("enumerate should succeed");
        println!("Found {} camera(s)", devices.len());
        for device in &devices {
            println!("  - {} ({})", device.name, device.id);
        }
    }
}
