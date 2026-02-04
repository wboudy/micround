//! Media Foundation camera capture for Windows
//!
//! Provides camera enumeration and capture using Windows Media Foundation API.
//!
//! # Requirements
//! - Windows 10 or later
//! - Camera access permission (Windows handles this automatically)
//!
//! # Architecture
//!
//! Media Foundation capture uses:
//! - `MFEnumDeviceSources` for device enumeration
//! - `IMFSourceReader` for reading video frames
//! - `IMFMediaType` for format negotiation
//!
//! Frames are read synchronously via ReadSample().

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::capture::enumerator::CameraEnumerator;
use crate::capture::{negotiate_format, CaptureBackend};
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

// ============================================================================
// Media Foundation GUIDs and Constants
// ============================================================================

#[cfg(all(target_os = "windows", feature = "windows"))]
use windows::{
    core::{GUID, HRESULT, PCWSTR, PWSTR},
    Win32::{
        Media::MediaFoundation::{
            IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaType, IMFSourceReader,
            MFCreateAttributes, MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources,
            MFMediaType_Video, MFShutdown, MFStartup, MF_API_VERSION,
            MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
            MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
            MF_SOURCE_READER_ASYNC_CALLBACK, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        },
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};

/// Media Foundation video format GUIDs
#[cfg(all(target_os = "windows", feature = "windows"))]
mod mf_guids {
    use windows::core::GUID;

    /// MFVideoFormat_NV12
    pub const NV12: GUID = GUID::from_values(
        0x3231564E,
        0x0000,
        0x0010,
        [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
    );

    /// MFVideoFormat_YUY2 (YUYV)
    pub const YUY2: GUID = GUID::from_values(
        0x32595559,
        0x0000,
        0x0010,
        [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
    );

    /// MFVideoFormat_MJPG
    pub const MJPG: GUID = GUID::from_values(
        0x47504A4D,
        0x0000,
        0x0010,
        [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
    );

    /// MFVideoFormat_RGB24
    pub const RGB24: GUID = GUID::from_values(
        0x00000014,
        0x0000,
        0x0010,
        [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
    );

    /// MFVideoFormat_RGB32
    pub const RGB32: GUID = GUID::from_values(
        0x00000016,
        0x0000,
        0x0010,
        [0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71],
    );
}

/// Convert Media Foundation GUID to our PixelFormat
#[cfg(all(target_os = "windows", feature = "windows"))]
fn guid_to_pixel_format(guid: &GUID) -> PixelFormat {
    if *guid == mf_guids::NV12 {
        PixelFormat::Nv12
    } else if *guid == mf_guids::YUY2 {
        PixelFormat::Yuyv
    } else if *guid == mf_guids::MJPG {
        PixelFormat::Mjpeg
    } else if *guid == mf_guids::RGB24 {
        PixelFormat::Rgb24
    } else if *guid == mf_guids::RGB32 {
        PixelFormat::Rgba32
    } else {
        PixelFormat::Unknown
    }
}

/// Convert our PixelFormat to Media Foundation GUID
#[cfg(all(target_os = "windows", feature = "windows"))]
fn pixel_format_to_guid(format: PixelFormat) -> GUID {
    match format {
        PixelFormat::Nv12 => mf_guids::NV12,
        PixelFormat::Yuyv => mf_guids::YUY2,
        PixelFormat::Mjpeg => mf_guids::MJPG,
        PixelFormat::Rgb24 => mf_guids::RGB24,
        PixelFormat::Rgba32 => mf_guids::RGB32,
        PixelFormat::Unknown => mf_guids::NV12, // Default to NV12
    }
}

// ============================================================================
// COM/MF Initialization Helper
// ============================================================================

/// RAII guard for COM initialization
#[cfg(all(target_os = "windows", feature = "windows"))]
struct ComGuard {
    initialized: bool,
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl ComGuard {
    fn new() -> Result<Self, CaptureError> {
        unsafe {
            // Initialize COM
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_err() && hr != windows::Win32::Foundation::S_FALSE {
                return Err(CaptureError::Platform(format!(
                    "COM initialization failed: {:?}",
                    hr
                )));
            }

            // Initialize Media Foundation
            MFStartup(MF_API_VERSION, 0).map_err(|e| {
                CaptureError::Platform(format!("MFStartup failed: {}", e))
            })?;

            Ok(Self { initialized: true })
        }
    }
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
        }
    }
}

// ============================================================================
// Camera Enumerator
// ============================================================================

/// Media Foundation-based camera enumerator
pub struct MediaFoundationEnumerator {
    /// Cached device list
    devices: HashMap<String, CameraDevice>,
    /// COM initialization guard
    #[cfg(all(target_os = "windows", feature = "windows"))]
    _com_guard: Option<ComGuard>,
}

impl MediaFoundationEnumerator {
    /// Create a new enumerator
    pub fn new() -> Self {
        let mut enumerator = Self {
            devices: HashMap::new(),
            #[cfg(all(target_os = "windows", feature = "windows"))]
            _com_guard: ComGuard::new().ok(),
        };
        // Initial device scan
        let _ = enumerator.refresh();
        enumerator
    }

    /// Query all video capture devices
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn query_devices() -> Vec<CameraDevice> {
        let mut devices = Vec::new();

        unsafe {
            // Create attributes for video capture device enumeration
            let mut attributes: Option<IMFAttributes> = None;
            if MFCreateAttributes(&mut attributes, 1).is_err() {
                tracing::error!("Failed to create MF attributes");
                return devices;
            }

            let attributes = match attributes {
                Some(a) => a,
                None => return devices,
            };

            // Set the device type to video capture
            if attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .is_err()
            {
                tracing::error!("Failed to set device source type");
                return devices;
            }

            // Enumerate devices
            let mut device_sources: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count: u32 = 0;

            if MFEnumDeviceSources(&attributes, &mut device_sources, &mut count).is_err() {
                tracing::error!("MFEnumDeviceSources failed");
                return devices;
            }

            if count == 0 || device_sources.is_null() {
                return devices;
            }

            // Iterate through found devices
            for i in 0..count {
                let activate_ptr = device_sources.add(i as usize);
                if let Some(ref activate) = *activate_ptr {
                    if let Some(device) = Self::device_from_activate(activate) {
                        devices.push(device);
                    }
                }
            }

            // Free the array (individual IMFActivate are released when dropped)
            windows::Win32::System::Com::CoTaskMemFree(Some(
                device_sources as *const std::ffi::c_void,
            ));
        }

        devices
    }

    #[cfg(not(all(target_os = "windows", feature = "windows")))]
    fn query_devices() -> Vec<CameraDevice> {
        Vec::new()
    }

    /// Extract device info from IMFActivate
    #[cfg(all(target_os = "windows", feature = "windows"))]
    unsafe fn device_from_activate(activate: &IMFActivate) -> Option<CameraDevice> {
        // Get friendly name
        let name = Self::get_string_attribute(activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
            .unwrap_or_else(|| "Unknown Camera".to_string());

        // Get symbolic link (unique device ID)
        let symbolic_link = Self::get_string_attribute(
            activate,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
        )?;

        // Activate the device to query capabilities
        let media_source: IMFMediaSource = activate.ActivateObject().ok()?;

        // Query capabilities from the source
        let capabilities = Self::query_capabilities_from_source(&media_source);

        // Shutdown the source (we'll reactivate when actually capturing)
        let _ = media_source.Shutdown();

        Some(CameraDevice {
            id: DeviceId(symbolic_link),
            name,
            manufacturer: None, // MF doesn't easily expose manufacturer
            capabilities,
            is_available: true,
        })
    }

    /// Get a string attribute from IMFActivate
    #[cfg(all(target_os = "windows", feature = "windows"))]
    unsafe fn get_string_attribute(activate: &IMFActivate, key: &GUID) -> Option<String> {
        let mut length: u32 = 0;

        // Get the length first
        if activate.GetStringLength(key, &mut length).is_err() {
            return None;
        }

        if length == 0 {
            return None;
        }

        // Allocate buffer and get the string
        let mut buffer: Vec<u16> = vec![0; (length + 1) as usize];
        let mut actual_length: u32 = 0;

        if activate
            .GetString(key, &mut buffer, Some(&mut actual_length))
            .is_err()
        {
            return None;
        }

        // Convert to Rust string
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16(&buffer[..end]).ok()
    }

    /// Query capabilities from an activated media source
    #[cfg(all(target_os = "windows", feature = "windows"))]
    unsafe fn query_capabilities_from_source(source: &IMFMediaSource) -> Vec<CameraCapability> {
        let mut capabilities = Vec::new();

        // Create source reader to enumerate formats
        let mut reader_attributes: Option<IMFAttributes> = None;
        if MFCreateAttributes(&mut reader_attributes, 1).is_err() {
            return capabilities;
        }

        let reader: IMFSourceReader = match MFCreateSourceReaderFromMediaSource(source, None) {
            Ok(r) => r,
            Err(_) => return capabilities,
        };

        // Enumerate available media types
        let mut type_index: u32 = 0;
        loop {
            let media_type: IMFMediaType = match reader.GetNativeMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                type_index,
            ) {
                Ok(t) => t,
                Err(_) => break, // No more types
            };

            if let Some(cap) = Self::capability_from_media_type(&media_type) {
                // Avoid duplicates
                if !capabilities.iter().any(|c| {
                    c.width == cap.width
                        && c.height == cap.height
                        && c.format == cap.format
                        && (c.framerate - cap.framerate).abs() < 0.1
                }) {
                    capabilities.push(cap);
                }
            }

            type_index += 1;
        }

        capabilities
    }

    /// Extract capability info from a media type
    #[cfg(all(target_os = "windows", feature = "windows"))]
    unsafe fn capability_from_media_type(media_type: &IMFMediaType) -> Option<CameraCapability> {
        // Get frame size
        let mut frame_size: u64 = 0;
        if media_type.GetUINT64(&MF_MT_FRAME_SIZE, &mut frame_size).is_err() {
            return None;
        }

        let width = (frame_size >> 32) as u32;
        let height = (frame_size & 0xFFFFFFFF) as u32;

        // Get frame rate
        let mut frame_rate: u64 = 0;
        let framerate = if media_type.GetUINT64(&MF_MT_FRAME_RATE, &mut frame_rate).is_ok() {
            let numerator = (frame_rate >> 32) as f32;
            let denominator = (frame_rate & 0xFFFFFFFF) as f32;
            if denominator > 0.0 {
                numerator / denominator
            } else {
                30.0 // Default
            }
        } else {
            30.0 // Default
        };

        // Get subtype (pixel format)
        let mut subtype = GUID::zeroed();
        if media_type.GetGUID(&MF_MT_SUBTYPE, &mut subtype).is_err() {
            return None;
        }

        let format = guid_to_pixel_format(&subtype);

        Some(CameraCapability {
            width,
            height,
            framerate,
            format,
        })
    }
}

impl Default for MediaFoundationEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for MediaFoundationEnumerator {
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

/// Media Foundation-based capture backend
pub struct MediaFoundationBackend {
    /// Enumerator for device queries
    enumerator: MediaFoundationEnumerator,
    /// Currently negotiated format
    negotiated_format: Option<NegotiatedFormat>,
    /// Whether capture is active
    capturing: bool,
    /// Frame sequence counter
    sequence: u64,
    /// Current device symbolic link (for reopening)
    current_device_id: Option<String>,

    // Windows-specific handles
    #[cfg(all(target_os = "windows", feature = "windows"))]
    source_reader: Option<IMFSourceReader>,
    #[cfg(all(target_os = "windows", feature = "windows"))]
    media_source: Option<IMFMediaSource>,
    #[cfg(all(target_os = "windows", feature = "windows"))]
    _com_guard: Option<ComGuard>,
}

// SAFETY: Media Foundation objects are COM objects that support apartment threading.
// We ensure all MF operations happen on a single thread or use proper synchronization.
#[cfg(all(target_os = "windows", feature = "windows"))]
unsafe impl Send for MediaFoundationBackend {}

impl MediaFoundationBackend {
    /// Create a new backend
    pub fn new() -> Self {
        Self {
            enumerator: MediaFoundationEnumerator::new(),
            negotiated_format: None,
            capturing: false,
            sequence: 0,
            current_device_id: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            source_reader: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            media_source: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            _com_guard: ComGuard::new().ok(),
        }
    }

    /// Find and activate a device by its symbolic link
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn activate_device(&self, device_id: &str) -> Result<IMFMediaSource, CaptureError> {
        unsafe {
            // Create attributes for enumeration
            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 1).map_err(|e| {
                CaptureError::Platform(format!("Failed to create attributes: {}", e))
            })?;

            let attributes = attributes
                .ok_or_else(|| CaptureError::Platform("Null attributes".into()))?;

            // Set device type to video capture
            attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set source type: {}", e))
                })?;

            // Enumerate and find our device
            let mut device_sources: *mut Option<IMFActivate> = std::ptr::null_mut();
            let mut count: u32 = 0;

            MFEnumDeviceSources(&attributes, &mut device_sources, &mut count).map_err(|e| {
                CaptureError::Platform(format!("MFEnumDeviceSources failed: {}", e))
            })?;

            if count == 0 || device_sources.is_null() {
                return Err(CaptureError::DeviceNotFound(device_id.to_string()));
            }

            // Find the device with matching symbolic link
            let mut found_source: Option<IMFMediaSource> = None;

            for i in 0..count {
                let activate_ptr = device_sources.add(i as usize);
                if let Some(ref activate) = *activate_ptr {
                    if let Some(link) = MediaFoundationEnumerator::get_string_attribute(
                        activate,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    ) {
                        if link == device_id {
                            found_source = activate.ActivateObject().ok();
                            break;
                        }
                    }
                }
            }

            // Free the array
            windows::Win32::System::Com::CoTaskMemFree(Some(
                device_sources as *const std::ffi::c_void,
            ));

            found_source.ok_or_else(|| CaptureError::DeviceNotFound(device_id.to_string()))
        }
    }

    /// Configure the source reader with the negotiated format
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn configure_reader(
        &self,
        reader: &IMFSourceReader,
        format: &NegotiatedFormat,
    ) -> Result<(), CaptureError> {
        unsafe {
            // Create a media type for our desired format
            let mut media_type: Option<IMFMediaType> = None;
            windows::Win32::Media::MediaFoundation::MFCreateMediaType(&mut media_type)
                .map_err(|e| CaptureError::Platform(format!("MFCreateMediaType failed: {}", e)))?;

            let media_type = media_type
                .ok_or_else(|| CaptureError::Platform("Null media type".into()))?;

            // Set major type to video
            media_type
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set major type: {}", e))
                })?;

            // Set subtype (pixel format)
            let subtype = pixel_format_to_guid(format.format);
            media_type.SetGUID(&MF_MT_SUBTYPE, &subtype).map_err(|e| {
                CaptureError::Platform(format!("Failed to set subtype: {}", e))
            })?;

            // Set frame size
            let frame_size: u64 = ((format.width as u64) << 32) | (format.height as u64);
            media_type
                .SetUINT64(&MF_MT_FRAME_SIZE, frame_size)
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set frame size: {}", e))
                })?;

            // Set frame rate
            let frame_rate: u64 = ((format.framerate as u64) << 32) | 1u64;
            media_type
                .SetUINT64(&MF_MT_FRAME_RATE, frame_rate)
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set frame rate: {}", e))
                })?;

            // Set the media type on the reader
            reader
                .SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    None,
                    &media_type,
                )
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set media type: {}", e))
                })?;

            tracing::info!(
                "Configured Media Foundation reader: {}x{} @ {}fps {:?}",
                format.width,
                format.height,
                format.framerate,
                format.format
            );

            Ok(())
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
        MediaFoundationEnumerator::query_devices()
    }

    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        // Close any existing session first
        self.close();

        // Get device capabilities for negotiation
        let capabilities = self
            .enumerator
            .get_capabilities(device_id)
            .unwrap_or_default();

        // Negotiate the best format
        let negotiated = negotiate_format(&capabilities, &settings).ok_or_else(|| {
            CaptureError::FormatNegotiationFailed("No suitable format available".into())
        })?;

        // Activate the device
        let media_source = self.activate_device(&device_id.0)?;

        unsafe {
            // Create source reader attributes
            let mut reader_attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut reader_attributes, 2).map_err(|e| {
                CaptureError::Platform(format!("Failed to create reader attributes: {}", e))
            })?;

            let reader_attributes = reader_attributes
                .ok_or_else(|| CaptureError::Platform("Null reader attributes".into()))?;

            // Enable hardware transforms for better performance
            reader_attributes
                .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
                .ok(); // Non-fatal if this fails

            // Create the source reader
            let reader = MFCreateSourceReaderFromMediaSource(&media_source, &reader_attributes)
                .map_err(|e| {
                    CaptureError::Platform(format!(
                        "MFCreateSourceReaderFromMediaSource failed: {}",
                        e
                    ))
                })?;

            // Configure the reader with our desired format
            self.configure_reader(&reader, &negotiated)?;

            // Store handles
            self.media_source = Some(media_source);
            self.source_reader = Some(reader);
            self.negotiated_format = Some(negotiated.clone());
            self.current_device_id = Some(device_id.0.clone());

            tracing::info!(
                "Media Foundation capture opened: {}x{} @ {}fps",
                negotiated.width,
                negotiated.height,
                negotiated.framerate
            );

            Ok(negotiated)
        }
    }

    #[cfg(not(all(target_os = "windows", feature = "windows")))]
    fn open(
        &mut self,
        _device_id: &DeviceId,
        _settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        Err(CaptureError::Platform(
            "Media Foundation support requires Windows with 'windows' feature".into(),
        ))
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            if self.source_reader.is_none() {
                return Err(CaptureError::Platform("No source reader opened".into()));
            }
            // Nothing special to do - ReadSample will start capture
        }

        self.capturing = true;
        self.sequence = 0;
        tracing::info!("Media Foundation capture started");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.capturing = false;
        tracing::info!("Media Foundation capture stopped");
        Ok(())
    }

    fn close(&mut self) {
        let _ = self.stop();

        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            // Release source reader first
            self.source_reader = None;

            // Then shutdown and release media source
            if let Some(ref source) = self.media_source {
                let _ = unsafe { source.Shutdown() };
            }
            self.media_source = None;
        }

        self.negotiated_format = None;
        self.current_device_id = None;
        tracing::info!("Media Foundation capture closed");
    }

    fn is_capturing(&self) -> bool {
        self.capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.negotiated_format.clone()
    }

    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        let reader = self
            .source_reader
            .as_ref()
            .ok_or_else(|| CaptureError::Platform("No source reader".into()))?;

        let format = self
            .negotiated_format
            .as_ref()
            .ok_or_else(|| CaptureError::Platform("No negotiated format".into()))?;

        unsafe {
            use windows::Win32::Media::MediaFoundation::{
                IMFSample, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
                MF_SOURCE_READERF_STREAMTICK,
            };

            let mut stream_index: u32 = 0;
            let mut flags: u32 = 0;
            let mut timestamp: i64 = 0;
            let mut sample: Option<IMFSample> = None;

            // Read the next sample
            reader
                .ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0, // No flags
                    Some(&mut stream_index),
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
                .map_err(|e| CaptureError::Platform(format!("ReadSample failed: {}", e)))?;

            // Check for errors
            if (flags & MF_SOURCE_READERF_ERROR.0) != 0 {
                return Err(CaptureError::Platform("Source reader error".into()));
            }

            if (flags & MF_SOURCE_READERF_ENDOFSTREAM.0) != 0 {
                return Err(CaptureError::Platform("End of stream".into()));
            }

            // Stream tick means no data yet
            if (flags & MF_SOURCE_READERF_STREAMTICK.0) != 0 || sample.is_none() {
                return Err(CaptureError::Timeout("No frame available".into()));
            }

            let sample = sample.unwrap();

            // Get the buffer from the sample
            use windows::Win32::Media::MediaFoundation::IMFMediaBuffer;

            let buffer: IMFMediaBuffer = sample.ConvertToContiguousBuffer().map_err(|e| {
                CaptureError::Platform(format!("ConvertToContiguousBuffer failed: {}", e))
            })?;

            // Lock the buffer to access data
            let mut data_ptr: *mut u8 = std::ptr::null_mut();
            let mut max_length: u32 = 0;
            let mut current_length: u32 = 0;

            buffer
                .Lock(&mut data_ptr, Some(&mut max_length), Some(&mut current_length))
                .map_err(|e| CaptureError::Platform(format!("Buffer lock failed: {}", e)))?;

            // Copy the data
            let data = std::slice::from_raw_parts(data_ptr, current_length as usize).to_vec();

            // Unlock the buffer
            buffer.Unlock().map_err(|e| {
                CaptureError::Platform(format!("Buffer unlock failed: {}", e))
            })?;

            self.sequence += 1;

            Ok(Frame {
                data,
                format: format.format,
                width: format.width,
                height: format.height,
                timestamp_ns: (timestamp as u64) * 100, // Convert 100ns units to ns
                sequence: self.sequence,
            })
        }
    }

    #[cfg(not(all(target_os = "windows", feature = "windows")))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Err(CaptureError::Platform(
            "Media Foundation support requires Windows with 'windows' feature".into(),
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
    fn test_enumerator_creation() {
        let enumerator = MediaFoundationEnumerator::new();
        // Should not panic
        let devices = enumerator.enumerate();
        assert!(devices.is_ok());
    }

    #[test]
    fn test_backend_creation() {
        let backend = MediaFoundationBackend::new();
        assert!(!backend.is_capturing());
    }

    #[test]
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn test_guid_conversion() {
        assert_eq!(guid_to_pixel_format(&mf_guids::NV12), PixelFormat::Nv12);
        assert_eq!(guid_to_pixel_format(&mf_guids::YUY2), PixelFormat::Yuyv);
        assert_eq!(guid_to_pixel_format(&mf_guids::MJPG), PixelFormat::Mjpeg);
    }
}
