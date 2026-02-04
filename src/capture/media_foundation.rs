//! Media Foundation camera support for Windows
//!
//! Provides camera enumeration and capture using the Windows Media Foundation API.
//! This is the modern Windows API for video capture (DirectShow is legacy).
//!
//! # Requirements
//! - Windows 7 or later
//! - Camera drivers that support Media Foundation

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::capture::enumerator::{format_to_fourcc, fourcc_to_format, CameraEnumerator};
use crate::capture::{negotiate_format, CaptureBackend};
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

#[cfg(all(target_os = "windows", feature = "windows"))]
use windows::{
    core::{Interface, GUID, HSTRING, PWSTR},
    Win32::{
        Media::MediaFoundation::{
            IMFActivate, IMFAttributes, IMFMediaBuffer, IMFMediaSource, IMFMediaType, IMFSample,
            IMFSourceReader, MFCreateAttributes, MFCreateSourceReaderFromMediaSource,
            MFEnumDeviceSources, MFMediaType_Video, MFShutdown, MFStartup,
            MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
            MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_READWRITE_DISABLE_CONVERTERS,
            MF_SOURCE_READER_ASYNC_CALLBACK, MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_VERSION,
        },
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};

#[cfg(all(target_os = "windows", feature = "windows"))]
use std::ptr;

// Well-known Media Foundation format GUIDs
#[cfg(all(target_os = "windows", feature = "windows"))]
const MF_SUBTYPE_MJPG: GUID = GUID::from_u128(0x47504A4D_0000_0010_8000_00AA00389B71);
#[cfg(all(target_os = "windows", feature = "windows"))]
const MF_SUBTYPE_YUY2: GUID = GUID::from_u128(0x32595559_0000_0010_8000_00AA00389B71);
#[cfg(all(target_os = "windows", feature = "windows"))]
const MF_SUBTYPE_NV12: GUID = GUID::from_u128(0x3231564E_0000_0010_8000_00AA00389B71);
#[cfg(all(target_os = "windows", feature = "windows"))]
const MF_SUBTYPE_RGB24: GUID = GUID::from_u128(0xe436eb7d_524f_11ce_9f53_0020af0ba770);
#[cfg(all(target_os = "windows", feature = "windows"))]
const MF_SUBTYPE_RGB32: GUID = GUID::from_u128(0xe436eb7e_524f_11ce_9f53_0020af0ba770);

// ============================================================================
// COM Initialization Helper
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
            // Try to initialize COM - may already be initialized in this thread
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            // S_OK (0) means success, S_FALSE (1) means already initialized
            let initialized = hr.is_ok() || hr.0 == 1;
            if !initialized {
                return Err(CaptureError::Platform(format!(
                    "COM initialization failed: {:?}",
                    hr
                )));
            }
            Ok(Self { initialized })
        }
    }
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// RAII guard for Media Foundation initialization
#[cfg(all(target_os = "windows", feature = "windows"))]
struct MFGuard {
    initialized: bool,
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl MFGuard {
    fn new() -> Result<Self, CaptureError> {
        unsafe {
            MFStartup(MF_VERSION, 0)
                .map_err(|e| CaptureError::Platform(format!("MFStartup failed: {}", e)))?;
            Ok(Self { initialized: true })
        }
    }
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl Drop for MFGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                let _ = MFShutdown();
            }
        }
    }
}

// ============================================================================
// Media Foundation Enumerator
// ============================================================================

/// Media Foundation-based camera enumerator
pub struct MFEnumerator {
    /// Cached device list
    devices: HashMap<String, CameraDevice>,
}

impl MFEnumerator {
    pub fn new() -> Self {
        let mut enumerator = Self {
            devices: HashMap::new(),
        };
        // Initial device scan
        let _ = enumerator.refresh();
        enumerator
    }

    /// Convert Media Foundation GUID to PixelFormat
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn guid_to_format(guid: &GUID) -> Option<PixelFormat> {
        if *guid == MF_SUBTYPE_MJPG {
            Some(PixelFormat::Mjpeg)
        } else if *guid == MF_SUBTYPE_YUY2 {
            Some(PixelFormat::Yuyv)
        } else if *guid == MF_SUBTYPE_NV12 {
            Some(PixelFormat::Nv12)
        } else if *guid == MF_SUBTYPE_RGB24 {
            Some(PixelFormat::Rgb24)
        } else if *guid == MF_SUBTYPE_RGB32 {
            Some(PixelFormat::Rgba32)
        } else {
            None
        }
    }

    /// Convert PixelFormat to Media Foundation GUID
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn format_to_guid(format: PixelFormat) -> Option<GUID> {
        match format {
            PixelFormat::Mjpeg => Some(MF_SUBTYPE_MJPG),
            PixelFormat::Yuyv => Some(MF_SUBTYPE_YUY2),
            PixelFormat::Nv12 => Some(MF_SUBTYPE_NV12),
            PixelFormat::Rgb24 => Some(MF_SUBTYPE_RGB24),
            PixelFormat::Rgba32 => Some(MF_SUBTYPE_RGB32),
        }
    }

    /// Query device capabilities from a media source
    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn query_capabilities(source: &IMFMediaSource) -> Vec<CameraCapability> {
        let mut capabilities = Vec::new();

        unsafe {
            // Create source reader to enumerate formats
            let reader: IMFSourceReader = match MFCreateSourceReaderFromMediaSource(source, None) {
                Ok(r) => r,
                Err(_) => return capabilities,
            };

            // Enumerate all media types
            let mut type_index = 0u32;
            loop {
                let media_type: IMFMediaType = match reader
                    .GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, type_index)
                {
                    Ok(t) => t,
                    Err(_) => break, // No more types
                };

                // Get format GUID
                let mut subtype = GUID::zeroed();
                if media_type.GetGUID(&MF_MT_SUBTYPE, &mut subtype).is_ok() {
                    if let Some(format) = Self::guid_to_format(&subtype) {
                        // Get frame size
                        let mut frame_size: u64 = 0;
                        if media_type
                            .GetUINT64(&MF_MT_FRAME_SIZE, &mut frame_size)
                            .is_ok()
                        {
                            let width = (frame_size >> 32) as u32;
                            let height = frame_size as u32;

                            // Get frame rate
                            let mut frame_rate: u64 = 0;
                            let fps = if media_type
                                .GetUINT64(&MF_MT_FRAME_RATE, &mut frame_rate)
                                .is_ok()
                            {
                                let numerator = (frame_rate >> 32) as f32;
                                let denominator = frame_rate as u32 as f32;
                                if denominator > 0.0 {
                                    numerator / denominator
                                } else {
                                    30.0
                                }
                            } else {
                                30.0 // Default
                            };

                            capabilities.push(CameraCapability {
                                width,
                                height,
                                format,
                                framerate: fps,
                            });
                        }
                    }
                }

                type_index += 1;
            }
        }

        // Deduplicate capabilities with same resolution/format, keep highest framerate
        let mut merged: HashMap<(u32, u32, PixelFormat), CameraCapability> = HashMap::new();
        for cap in capabilities {
            let key = (cap.width, cap.height, cap.format);
            merged
                .entry(key)
                .and_modify(|existing| {
                    // Keep the higher framerate when we see duplicates
                    if cap.framerate > existing.framerate {
                        existing.framerate = cap.framerate;
                    }
                })
                .or_insert(cap);
        }

        merged.into_values().collect()
    }
}

impl Default for MFEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for MFEnumerator {
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
        self.devices
            .get(&id.0)
            .map(|d| d.is_available)
            .unwrap_or(false)
    }

    fn refresh(&mut self) -> Result<(), CaptureError> {
        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            // Initialize COM and MF
            let _com = ComGuard::new()?;
            let _mf = MFGuard::new()?;

            // Mark all devices as potentially unavailable
            for device in self.devices.values_mut() {
                device.is_available = false;
            }

            unsafe {
                // Create attributes for video capture devices
                let mut attributes: Option<IMFAttributes> = None;
                MFCreateAttributes(&mut attributes, 1).map_err(|e| {
                    CaptureError::Platform(format!("MFCreateAttributes failed: {}", e))
                })?;

                let attributes = attributes.ok_or_else(|| {
                    CaptureError::Platform("MFCreateAttributes returned None".into())
                })?;

                // Set the source type to video capture
                attributes
                    .SetGUID(
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                    )
                    .map_err(|e| CaptureError::Platform(format!("SetGUID failed: {}", e)))?;

                // Enumerate devices
                let mut devices: *mut Option<IMFActivate> = ptr::null_mut();
                let mut count: u32 = 0;

                MFEnumDeviceSources(&attributes, &mut devices, &mut count).map_err(|e| {
                    CaptureError::Platform(format!("MFEnumDeviceSources failed: {}", e))
                })?;

                if count == 0 || devices.is_null() {
                    return Ok(());
                }

                // Process each device
                let device_slice = std::slice::from_raw_parts(devices, count as usize);

                for (index, activate_opt) in device_slice.iter().enumerate() {
                    if let Some(activate) = activate_opt {
                        // Get friendly name
                        let mut name_len: u32 = 0;
                        let _ = activate
                            .GetStringLength(&MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, &mut name_len);

                        let name = if name_len > 0 {
                            let mut buffer = vec![0u16; (name_len + 1) as usize];
                            let mut actual_len: u32 = 0;
                            if activate
                                .GetString(
                                    &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
                                    &mut buffer,
                                    Some(&mut actual_len),
                                )
                                .is_ok()
                            {
                                String::from_utf16_lossy(&buffer[..actual_len as usize])
                            } else {
                                format!("Camera {}", index)
                            }
                        } else {
                            format!("Camera {}", index)
                        };

                        // Create device ID from index (stable within session)
                        let device_id = format!("mf:{}", index);

                        // Try to activate and get capabilities
                        let capabilities =
                            if let Ok(source) = activate.ActivateObject::<IMFMediaSource>() {
                                let caps = Self::query_capabilities(&source);
                                // Deactivate to release resources
                                let _ = activate.ShutdownObject();
                                caps
                            } else {
                                Vec::new()
                            };

                        let camera = CameraDevice {
                            id: DeviceId(device_id.clone()),
                            name,
                            manufacturer: None,
                            capabilities,
                            is_available: true,
                        };

                        self.devices.insert(device_id, camera);
                    }
                }

                // Clean up the device array (COM allocated)
                for activate_opt in device_slice {
                    if activate_opt.is_some() {
                        // IMFActivate will be dropped automatically
                    }
                }
                windows::Win32::System::Com::CoTaskMemFree(Some(devices as *const _));
            }

            // Remove devices that are no longer available
            self.devices.retain(|_, d| d.is_available);

            Ok(())
        }

        #[cfg(not(all(target_os = "windows", feature = "windows")))]
        Err(CaptureError::Platform(
            "Media Foundation not available on this platform".into(),
        ))
    }
}

// ============================================================================
// Media Foundation Capture Backend
// ============================================================================

/// Media Foundation-based capture backend
pub struct MFBackend {
    /// COM initialization guard (must be kept alive)
    #[cfg(all(target_os = "windows", feature = "windows"))]
    _com_guard: Option<ComGuard>,
    /// Media Foundation initialization guard
    #[cfg(all(target_os = "windows", feature = "windows"))]
    _mf_guard: Option<MFGuard>,
    /// The active media source
    #[cfg(all(target_os = "windows", feature = "windows"))]
    source: Option<IMFMediaSource>,
    /// Source reader for frame capture
    #[cfg(all(target_os = "windows", feature = "windows"))]
    reader: Option<IMFSourceReader>,
    /// Currently negotiated format
    negotiated_format: Option<NegotiatedFormat>,
    /// Cached enumerator
    enumerator: MFEnumerator,
    /// Whether currently capturing
    capturing: bool,
    /// Frame sequence counter
    sequence: AtomicU64,
}

impl MFBackend {
    pub fn new() -> Self {
        Self {
            #[cfg(all(target_os = "windows", feature = "windows"))]
            _com_guard: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            _mf_guard: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            source: None,
            #[cfg(all(target_os = "windows", feature = "windows"))]
            reader: None,
            negotiated_format: None,
            enumerator: MFEnumerator::new(),
            capturing: false,
            sequence: AtomicU64::new(0),
        }
    }
}

impl Default for MFBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for MFBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        let enumerator = MFEnumerator::new();
        enumerator.enumerate().unwrap_or_default()
    }

    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        // Close any existing device
        self.close();

        // Initialize COM and Media Foundation
        self._com_guard = Some(ComGuard::new()?);
        self._mf_guard = Some(MFGuard::new()?);

        // Parse device index from ID
        let index: usize = device_id
            .0
            .strip_prefix("mf:")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| CaptureError::DeviceNotFound(device_id.0.clone()))?;

        unsafe {
            // Create attributes for video capture devices
            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 1)
                .map_err(|e| CaptureError::Platform(format!("MFCreateAttributes failed: {}", e)))?;

            let attributes = attributes
                .ok_or_else(|| CaptureError::Platform("MFCreateAttributes returned None".into()))?;

            attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| CaptureError::Platform(format!("SetGUID failed: {}", e)))?;

            // Enumerate devices to find the one we want
            let mut devices: *mut Option<IMFActivate> = ptr::null_mut();
            let mut count: u32 = 0;

            MFEnumDeviceSources(&attributes, &mut devices, &mut count).map_err(|e| {
                CaptureError::Platform(format!("MFEnumDeviceSources failed: {}", e))
            })?;

            if index >= count as usize {
                return Err(CaptureError::DeviceNotFound(device_id.0.clone()));
            }

            let device_slice = std::slice::from_raw_parts(devices, count as usize);
            let activate = device_slice[index]
                .as_ref()
                .ok_or_else(|| CaptureError::DeviceNotFound(device_id.0.clone()))?;

            // Activate the media source
            let source: IMFMediaSource = activate.ActivateObject().map_err(|e| {
                if e.code().0 as u32 == 0x80070005 {
                    // E_ACCESSDENIED
                    CaptureError::PermissionDenied(
                        "Camera access denied. Check Windows camera privacy settings.".into(),
                    )
                } else if e.code().0 as u32 == 0x8007000E {
                    // E_OUTOFMEMORY (often means busy)
                    CaptureError::DeviceBusy
                } else {
                    CaptureError::Platform(format!("ActivateObject failed: {}", e))
                }
            })?;

            // Get capabilities for format negotiation
            let capabilities = self
                .enumerator
                .get_capabilities(device_id)
                .unwrap_or_default();

            // Negotiate format
            let negotiated = negotiate_format(&capabilities, &settings).ok_or_else(|| {
                CaptureError::FormatNegotiationFailed("No suitable format available".into())
            })?;

            // Create source reader
            let reader: IMFSourceReader = MFCreateSourceReaderFromMediaSource(&source, None)
                .map_err(|e| CaptureError::Platform(format!("CreateSourceReader failed: {}", e)))?;

            // Find and set the matching media type
            let mut type_index = 0u32;
            let mut found_type = false;
            loop {
                let media_type: IMFMediaType = match reader
                    .GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, type_index)
                {
                    Ok(t) => t,
                    Err(_) => break,
                };

                // Check if this type matches our negotiated format
                let mut frame_size: u64 = 0;
                let mut subtype = GUID::zeroed();

                if media_type
                    .GetUINT64(&MF_MT_FRAME_SIZE, &mut frame_size)
                    .is_ok()
                    && media_type.GetGUID(&MF_MT_SUBTYPE, &mut subtype).is_ok()
                {
                    let width = (frame_size >> 32) as u32;
                    let height = frame_size as u32;

                    if width == negotiated.width
                        && height == negotiated.height
                        && MFEnumerator::guid_to_format(&subtype) == Some(negotiated.format)
                    {
                        // Set this type on the reader
                        reader
                            .SetCurrentMediaType(
                                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                                None,
                                &media_type,
                            )
                            .map_err(|e| {
                                CaptureError::Platform(format!("SetCurrentMediaType failed: {}", e))
                            })?;
                        found_type = true;
                        break;
                    }
                }

                type_index += 1;
            }

            if !found_type {
                return Err(CaptureError::FormatNegotiationFailed(
                    "Could not set negotiated format on device".into(),
                ));
            }

            // Clean up device array
            windows::Win32::System::Com::CoTaskMemFree(Some(devices as *const _));

            self.source = Some(source);
            self.reader = Some(reader);
            self.negotiated_format = Some(negotiated.clone());
            self.sequence.store(0, Ordering::SeqCst);

            tracing::info!(
                "Opened camera: {}x{} @ {:?}",
                negotiated.width,
                negotiated.height,
                negotiated.format
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
            "Media Foundation not available on this platform".into(),
        ))
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        if self.reader.is_none() {
            return Err(CaptureError::Platform("No device open".into()));
        }
        self.capturing = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.capturing = false;
        Ok(())
    }

    fn close(&mut self) {
        self.capturing = false;
        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            // Drop reader first (it holds a reference to source)
            self.reader = None;
            // Then drop source
            if let Some(source) = self.source.take() {
                unsafe {
                    let _ = source.Shutdown();
                }
            }
            // MF and COM guards will be dropped in order
        }
        self.negotiated_format = None;
    }

    fn is_capturing(&self) -> bool {
        self.capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.negotiated_format.clone()
    }

    #[cfg(all(target_os = "windows", feature = "windows"))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        if !self.capturing {
            return Err(CaptureError::Platform("Not capturing".into()));
        }

        let reader = self
            .reader
            .as_ref()
            .ok_or_else(|| CaptureError::Platform("No device open".into()))?;

        let format = self
            .negotiated_format
            .as_ref()
            .ok_or_else(|| CaptureError::Platform("No format negotiated".into()))?;

        unsafe {
            let mut stream_index: u32 = 0;
            let mut flags: u32 = 0;
            let mut timestamp: i64 = 0;
            let mut sample: Option<IMFSample> = None;

            reader
                .ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    Some(&mut stream_index),
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
                .map_err(|e| CaptureError::Platform(format!("ReadSample failed: {}", e)))?;

            // Check for end of stream or error flags
            const MF_SOURCE_READERF_ENDOFSTREAM: u32 = 0x1;
            const MF_SOURCE_READERF_ERROR: u32 = 0x4;
            const MF_SOURCE_READERF_STREAMTICK: u32 = 0x100;

            if flags & MF_SOURCE_READERF_ENDOFSTREAM != 0 {
                return Err(CaptureError::Disconnected);
            }

            if flags & MF_SOURCE_READERF_ERROR != 0 {
                return Err(CaptureError::Platform("Stream error".into()));
            }

            if flags & MF_SOURCE_READERF_STREAMTICK != 0 || sample.is_none() {
                // No sample available, return a timeout-like error (0 = no actual timeout, just no frame ready)
                return Err(CaptureError::Timeout(0));
            }

            let sample = sample.unwrap();

            // Get buffer from sample
            let buffer: IMFMediaBuffer = sample.ConvertToContiguousBuffer().map_err(|e| {
                CaptureError::Platform(format!("ConvertToContiguousBuffer failed: {}", e))
            })?;

            // Lock and copy data
            let mut data_ptr: *mut u8 = ptr::null_mut();
            let mut max_length: u32 = 0;
            let mut current_length: u32 = 0;

            buffer
                .Lock(
                    &mut data_ptr,
                    Some(&mut max_length),
                    Some(&mut current_length),
                )
                .map_err(|e| CaptureError::Platform(format!("Buffer Lock failed: {}", e)))?;

            let data = std::slice::from_raw_parts(data_ptr, current_length as usize).to_vec();

            buffer
                .Unlock()
                .map_err(|e| CaptureError::Platform(format!("Buffer Unlock failed: {}", e)))?;

            let sequence = self.sequence.fetch_add(1, Ordering::SeqCst);

            Ok(Frame {
                data,
                format: format.format,
                width: format.width,
                height: format.height,
                timestamp_ns: timestamp as u64 * 100, // MF timestamps are in 100ns units
                sequence,
            })
        }
    }

    #[cfg(not(all(target_os = "windows", feature = "windows")))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Err(CaptureError::Platform(
            "Media Foundation not available on this platform".into(),
        ))
    }
}

#[cfg(all(target_os = "windows", feature = "windows"))]
impl Drop for MFBackend {
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
        // Should not panic even without Windows feature
        let _enumerator = MFEnumerator::new();
    }

    #[test]
    fn test_backend_creation() {
        let backend = MFBackend::new();
        assert!(!backend.is_capturing());
        assert!(backend.current_format().is_none());
    }

    #[test]
    fn test_backend_start_without_open() {
        let mut backend = MFBackend::new();
        let result = backend.start();
        assert!(result.is_err());
    }

    #[test]
    #[cfg(all(target_os = "windows", feature = "windows"))]
    #[ignore = "requires Windows with camera"]
    fn test_enumerate_devices() {
        let enumerator = MFEnumerator::new();
        let devices = enumerator.enumerate().unwrap();
        println!("Found {} camera(s)", devices.len());
        for device in &devices {
            println!("  - {} ({})", device.name, device.id.0);
            for cap in &device.capabilities {
                println!("    {}x{} {:?}", cap.width, cap.height, cap.format);
            }
        }
    }

    #[test]
    #[cfg(all(target_os = "windows", feature = "windows"))]
    #[ignore = "requires Windows with camera"]
    fn test_capture_frame() {
        let mut backend = MFBackend::new();

        // Get first camera
        let devices = backend.enumerate_devices();
        if devices.is_empty() {
            println!("No cameras found, skipping test");
            return;
        }

        let device = &devices[0];
        let settings = CaptureSettings::default();

        let format = backend.open(&device.id, settings).unwrap();
        println!(
            "Opened: {}x{} {:?}",
            format.width, format.height, format.format
        );

        backend.start().unwrap();

        // Capture a few frames
        for i in 0..5 {
            match backend.next_frame() {
                Ok(frame) => {
                    println!(
                        "Frame {}: {}x{}, {} bytes",
                        i,
                        frame.width,
                        frame.height,
                        frame.data.len()
                    );
                }
                Err(CaptureError::Timeout(_)) => {
                    println!("Frame {}: timeout (retry)", i);
                }
                Err(e) => {
                    println!("Frame {}: error {:?}", i, e);
                    break;
                }
            }
        }

        backend.stop().unwrap();
        backend.close();
    }
}
