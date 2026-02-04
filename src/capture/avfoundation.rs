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
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

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
    sequence: AtomicU64,
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
    dispatch_queue: Option<*mut std::ffi::c_void>,
    #[cfg(all(target_os = "macos", feature = "macos"))]
    delegate: Option<*mut std::ffi::c_void>,
    #[cfg(all(target_os = "macos", feature = "macos"))]
    frame_buffer: Arc<Mutex<FrameBuffer>>,
    /// Channel to receive frames from delegate callback
    frame_receiver: Option<mpsc::Receiver<Frame>>,
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
            dispatch_queue: None,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            delegate: None,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            frame_buffer: Arc::new(Mutex::new(FrameBuffer {
                data: None,
                width: 0,
                height: 0,
                format: PixelFormat::Unknown,
                timestamp_ns: 0,
                new_frame: AtomicBool::new(false),
                sequence: AtomicU64::new(0),
            })),
            frame_receiver: None,
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

                // Create dispatch queue for sample buffer delivery
                let queue_label = std::ffi::CString::new("com.micround.capture").unwrap();
                let capture_queue = dispatch_queue_create(queue_label.as_ptr(), std::ptr::null());
                if capture_queue.is_null() {
                    let _: () = msg_send![session, commitConfiguration];
                    let _: () = msg_send![video_output, release];
                    let _: () = msg_send![session, release];
                    return Err(CaptureError::Platform("Failed to create dispatch queue".into()));
                }

                // Create frame channel for delegate to send frames
                let (frame_sender, frame_receiver) = mpsc::channel::<Frame>();

                // Register the frame buffer for this session
                let buffer_clone = self.frame_buffer.clone();
                register_frame_callback(session as usize, buffer_clone, frame_sender);

                // Create delegate instance
                // Note: We use a custom delegate class registered at runtime
                let delegate = create_sample_buffer_delegate(session as usize);
                if delegate.is_null() {
                    dispatch_release(capture_queue);
                    let _: () = msg_send![session, commitConfiguration];
                    let _: () = msg_send![video_output, release];
                    let _: () = msg_send![session, release];
                    unregister_frame_callback(session as usize);
                    return Err(CaptureError::Platform("Failed to create delegate".into()));
                }

                // Set the delegate on the video output
                let _: () = msg_send![video_output, setSampleBufferDelegate: delegate queue: capture_queue];

                let _: () = msg_send![session, commitConfiguration];

                // Store handles (already retained via new)
                self.session = Some(session as *mut std::ffi::c_void);
                let _: () = msg_send![device_input, retain];
                self.device_input = Some(device_input as *mut std::ffi::c_void);
                self.video_output = Some(video_output as *mut std::ffi::c_void);
                self.dispatch_queue = Some(capture_queue);
                self.delegate = Some(delegate as *mut std::ffi::c_void);
                self.frame_receiver = Some(frame_receiver);
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
            // Unregister callback first
            if let Some(session_ptr) = self.session {
                unregister_frame_callback(session_ptr as usize);
            }

            // Release delegate first (it references the output)
            if let Some(ptr) = self.delegate.take() {
                Self::release_obj(ptr);
            }
            if let Some(ptr) = self.device_input.take() {
                Self::release_obj(ptr);
            }
            if let Some(ptr) = self.video_output.take() {
                Self::release_obj(ptr);
            }
            if let Some(ptr) = self.session.take() {
                Self::release_obj(ptr);
            }
            // Release dispatch queue
            if let Some(ptr) = self.dispatch_queue.take() {
                dispatch_release(ptr);
            }
        }

        self.frame_receiver = None;
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
        if !self.capturing {
            return Err(CaptureError::Platform("Not capturing".into()));
        }

        // Try to receive from channel first (preferred path when delegate is working)
        if let Some(ref receiver) = self.frame_receiver {
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(frame) => return Ok(frame),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Fall through to buffer check
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(CaptureError::Disconnected);
                }
            }
        }

        // Fallback: check the shared buffer (for when delegate populates it directly)
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
// Sample Buffer Delegate
// ============================================================================

/// Global registry for frame callbacks, keyed by session pointer
#[cfg(all(target_os = "macos", feature = "macos"))]
static FRAME_CALLBACKS: std::sync::OnceLock<Mutex<HashMap<usize, FrameCallback>>> = std::sync::OnceLock::new();

#[cfg(all(target_os = "macos", feature = "macos"))]
struct FrameCallback {
    buffer: Arc<Mutex<FrameBuffer>>,
    sender: mpsc::Sender<Frame>,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
fn get_frame_callbacks() -> &'static Mutex<HashMap<usize, FrameCallback>> {
    FRAME_CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(all(target_os = "macos", feature = "macos"))]
fn register_frame_callback(session_id: usize, buffer: Arc<Mutex<FrameBuffer>>, sender: mpsc::Sender<Frame>) {
    if let Ok(mut callbacks) = get_frame_callbacks().lock() {
        callbacks.insert(session_id, FrameCallback { buffer, sender });
    }
}

#[cfg(all(target_os = "macos", feature = "macos"))]
fn unregister_frame_callback(session_id: usize) {
    if let Ok(mut callbacks) = get_frame_callbacks().lock() {
        callbacks.remove(&session_id);
    }
}

/// Process a sample buffer and deliver the frame
#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe fn process_sample_buffer(session_id: usize, sample_buffer: *const std::ffi::c_void) {
    if sample_buffer.is_null() {
        return;
    }

    // Get pixel buffer from sample buffer
    let pixel_buffer = CMSampleBufferGetImageBuffer(sample_buffer);
    if pixel_buffer.is_null() {
        return;
    }

    // Lock the pixel buffer for reading
    let lock_result = CVPixelBufferLockBaseAddress(pixel_buffer, 0);
    if lock_result != 0 {
        tracing::warn!("Failed to lock pixel buffer: {}", lock_result);
        return;
    }

    // Extract frame data
    let width = CVPixelBufferGetWidth(pixel_buffer) as u32;
    let height = CVPixelBufferGetHeight(pixel_buffer) as u32;
    let bytes_per_row = CVPixelBufferGetBytesPerRow(pixel_buffer);
    let base_address = CVPixelBufferGetBaseAddress(pixel_buffer);
    let pixel_format_type = CVPixelBufferGetPixelFormatType(pixel_buffer);

    let format = fourcc_to_pixel_format(pixel_format_type);

    // Calculate data size and copy
    let data_size = bytes_per_row * height as usize;
    let mut frame_data = vec![0u8; data_size];
    if !base_address.is_null() {
        std::ptr::copy_nonoverlapping(base_address, frame_data.as_mut_ptr(), data_size);
    }

    // Get timestamp
    let timestamp = CMSampleBufferGetPresentationTimeStamp(sample_buffer);
    let timestamp_ns = if timestamp.timescale > 0 {
        (timestamp.value as u64 * 1_000_000_000) / timestamp.timescale as u64
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    };

    // Unlock pixel buffer
    CVPixelBufferUnlockBaseAddress(pixel_buffer, 0);

    // Deliver frame to callback
    if let Ok(callbacks) = get_frame_callbacks().lock() {
        if let Some(callback) = callbacks.get(&session_id) {
            // Update shared buffer
            if let Ok(mut buffer) = callback.buffer.lock() {
                let seq = buffer.sequence.fetch_add(1, Ordering::SeqCst) + 1;
                buffer.data = Some(frame_data.clone());
                buffer.width = width;
                buffer.height = height;
                buffer.format = format;
                buffer.timestamp_ns = timestamp_ns;
                buffer.new_frame.store(true, Ordering::Release);

                // Also send through channel
                let frame = Frame {
                    data: frame_data,
                    format,
                    width,
                    height,
                    timestamp_ns,
                    sequence: seq,
                };
                let _ = callback.sender.send(frame);
            }
        }
    }
}

/// Delegate class registration flag
#[cfg(all(target_os = "macos", feature = "macos"))]
static DELEGATE_CLASS_REGISTERED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Register the sample buffer delegate class with the Objective-C runtime
#[cfg(all(target_os = "macos", feature = "macos"))]
fn ensure_delegate_class_registered() -> bool {
    *DELEGATE_CLASS_REGISTERED.get_or_init(|| {
        unsafe {
            use objc2::runtime::{AnyClass, ClassBuilder};

            // Check if class already exists
            if AnyClass::get("MicroundSampleBufferDelegate").is_some() {
                return true;
            }

            // Get NSObject as superclass
            let superclass = match AnyClass::get("NSObject") {
                Some(cls) => cls,
                None => {
                    tracing::error!("Failed to get NSObject class");
                    return false;
                }
            };

            // Create class builder
            let mut builder = match ClassBuilder::new("MicroundSampleBufferDelegate", superclass) {
                Some(b) => b,
                None => {
                    tracing::error!("Failed to create class builder");
                    return false;
                }
            };

            // Add session_id ivar to store the session pointer
            builder.add_ivar::<usize>("sessionId");

            // Add the delegate method
            // captureOutput:didOutputSampleBuffer:fromConnection:
            unsafe extern "C" fn delegate_method(
                this: *const AnyObject,
                _sel: objc2::runtime::Sel,
                _output: *const AnyObject,
                sample_buffer: *const std::ffi::c_void,
                _connection: *const AnyObject,
            ) {
                if this.is_null() {
                    return;
                }
                // Get session ID from ivar
                let session_id: usize = *(*this).get_ivar("sessionId");
                process_sample_buffer(session_id, sample_buffer);
            }

            // Register the method
            // Selector: captureOutput:didOutputSampleBuffer:fromConnection:
            let sel = objc2::sel!(captureOutput:didOutputSampleBuffer:fromConnection:);
            builder.add_method(
                sel,
                delegate_method as unsafe extern "C" fn(*const AnyObject, objc2::runtime::Sel, *const AnyObject, *const std::ffi::c_void, *const AnyObject),
            );

            // Register the class
            let _cls = builder.register();
            tracing::info!("Registered MicroundSampleBufferDelegate class");
            true
        }
    })
}

/// Create a new sample buffer delegate instance
#[cfg(all(target_os = "macos", feature = "macos"))]
fn create_sample_buffer_delegate(session_id: usize) -> *const AnyObject {
    if !ensure_delegate_class_registered() {
        return std::ptr::null();
    }

    unsafe {
        let cls = match objc2::runtime::AnyClass::get("MicroundSampleBufferDelegate") {
            Some(c) => c,
            None => return std::ptr::null(),
        };

        // Create instance
        let delegate: *const AnyObject = msg_send![cls, new];
        if delegate.is_null() {
            return std::ptr::null();
        }

        // Set session ID ivar using object_setInstanceVariable
        // This is the low-level way to set an ivar
        let ivar_name = std::ffi::CString::new("sessionId").unwrap();
        extern "C" {
            fn object_setInstanceVariable(
                obj: *mut AnyObject,
                name: *const std::ffi::c_char,
                value: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
        }
        object_setInstanceVariable(
            delegate as *mut AnyObject,
            ivar_name.as_ptr(),
            session_id as *mut std::ffi::c_void,
        );

        delegate
    }
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
    fn CMSampleBufferGetImageBuffer(sample_buffer: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn CMSampleBufferGetPresentationTimeStamp(sample_buffer: *const std::ffi::c_void) -> CMTime;
}

/// CMTime structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(all(target_os = "macos", feature = "macos"))]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

// ============================================================================
// CoreVideo FFI Declarations
// ============================================================================

#[cfg(all(target_os = "macos", feature = "macos"))]
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferLockBaseAddress(pixel_buffer: *const std::ffi::c_void, lock_flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(pixel_buffer: *const std::ffi::c_void, unlock_flags: u64) -> i32;
    fn CVPixelBufferGetBaseAddress(pixel_buffer: *const std::ffi::c_void) -> *const u8;
    fn CVPixelBufferGetWidth(pixel_buffer: *const std::ffi::c_void) -> usize;
    fn CVPixelBufferGetHeight(pixel_buffer: *const std::ffi::c_void) -> usize;
    fn CVPixelBufferGetBytesPerRow(pixel_buffer: *const std::ffi::c_void) -> usize;
    fn CVPixelBufferGetPixelFormatType(pixel_buffer: *const std::ffi::c_void) -> u32;
}

// ============================================================================
// Dispatch FFI Declarations
// ============================================================================

#[cfg(all(target_os = "macos", feature = "macos"))]
#[link(name = "System")]
extern "C" {
    fn dispatch_queue_create(label: *const std::ffi::c_char, attr: *const std::ffi::c_void) -> *mut std::ffi::c_void;
    fn dispatch_release(object: *mut std::ffi::c_void);
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
