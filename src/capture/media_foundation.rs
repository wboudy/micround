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

use std::collections::HashMap;
use std::ptr;
use std::mem::MaybeUninit;

use tracing::{debug, error, info, trace, warn};

use crate::capture::enumerator::CameraEnumerator;
use crate::capture::{CaptureBackend, negotiate_format};
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

#[cfg(target_os = "windows")]
use windows::{
    core::{Interface, GUID, HSTRING, PWSTR},
    Win32::{
        Foundation::{BOOL, FALSE, TRUE},
        Media::MediaFoundation::*,
        System::Com::*,
    },
};

// ============================================================================
// COM Initialization
// ============================================================================

/// Guard for COM initialization - ensures proper cleanup
#[cfg(target_os = "windows")]
struct ComGuard {
    initialized: bool,
}

#[cfg(target_os = "windows")]
impl ComGuard {
    fn new() -> Result<Self, CaptureError> {
        // Initialize COM in multithreaded mode
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).map_err(|e| {
                CaptureError::Platform(format!("COM initialization failed: {:?}", e))
            })?;
        }
        Ok(Self { initialized: true })
    }
}

#[cfg(target_os = "windows")]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

/// Guard for Media Foundation initialization
#[cfg(target_os = "windows")]
struct MfGuard {
    initialized: bool,
}

#[cfg(target_os = "windows")]
impl MfGuard {
    fn new() -> Result<Self, CaptureError> {
        unsafe {
            MFStartup(MF_VERSION, MFSTARTUP_FULL).map_err(|e| {
                CaptureError::Platform(format!("Media Foundation startup failed: {:?}", e))
            })?;
        }
        Ok(Self { initialized: true })
    }
}

#[cfg(target_os = "windows")]
impl Drop for MfGuard {
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
pub struct MediaFoundationEnumerator {
    /// Cached device list
    devices: HashMap<String, CameraDevice>,
}

impl MediaFoundationEnumerator {
    pub fn new() -> Self {
        let mut enumerator = Self {
            devices: HashMap::new(),
        };
        // Initial device scan
        let _ = enumerator.refresh();
        enumerator
    }

    /// Enumerate video capture devices using Media Foundation
    #[cfg(target_os = "windows")]
    fn enumerate_devices_internal() -> Result<Vec<CameraDevice>, CaptureError> {
        let _com = ComGuard::new()?;
        let _mf = MfGuard::new()?;

        let mut devices = Vec::new();

        unsafe {
            // Create attributes for video capture device enumeration
            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 1).map_err(|e| {
                CaptureError::Platform(format!("Failed to create attributes: {:?}", e))
            })?;
            let attributes = attributes.unwrap();

            // Set the source type to video capture
            attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set source type: {:?}", e))
                })?;

            // Enumerate devices
            let mut device_sources: *mut Option<IMFActivate> = ptr::null_mut();
            let mut count: u32 = 0;

            MFEnumDeviceSources(&attributes, &mut device_sources, &mut count).map_err(|e| {
                CaptureError::Platform(format!("Failed to enumerate devices: {:?}", e))
            })?;

            if count > 0 && !device_sources.is_null() {
                let sources = std::slice::from_raw_parts(device_sources, count as usize);

                for (i, source_opt) in sources.iter().enumerate() {
                    if let Some(source) = source_opt {
                        if let Some(device) = Self::extract_device_info(source, i) {
                            devices.push(device);
                        }
                    }
                }

                // Free the activation objects
                for source_opt in sources {
                    drop(source_opt.clone());
                }
                CoTaskMemFree(Some(device_sources as *const _));
            }
        }

        debug!(count = devices.len(), "Enumerated Media Foundation devices");
        Ok(devices)
    }

    /// Extract device information from an IMFActivate object
    #[cfg(target_os = "windows")]
    fn extract_device_info(activate: &IMFActivate, index: usize) -> Option<CameraDevice> {
        unsafe {
            // Get friendly name
            let name = Self::get_string_attribute(activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
                .unwrap_or_else(|| format!("Camera {}", index + 1));

            // Get symbolic link (device path)
            let device_id = Self::get_string_attribute(
                activate,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )
            .unwrap_or_else(|| format!("camera_{}", index));

            // Try to get capabilities by activating the source
            let capabilities = Self::query_capabilities(activate).unwrap_or_default();

            Some(CameraDevice {
                id: DeviceId(device_id),
                name,
                manufacturer: None, // Media Foundation doesn't easily expose this
                capabilities,
                is_available: true,
            })
        }
    }

    /// Get a string attribute from an IMFActivate object
    #[cfg(target_os = "windows")]
    unsafe fn get_string_attribute(activate: &IMFActivate, key: &GUID) -> Option<String> {
        let mut length: u32 = 0;
        if activate.GetStringLength(key, &mut length).is_err() {
            return None;
        }

        let mut buffer = vec![0u16; (length + 1) as usize];
        let mut actual_length: u32 = 0;
        if activate
            .GetString(key, &mut buffer, Some(&mut actual_length))
            .is_err()
        {
            return None;
        }

        Some(String::from_utf16_lossy(&buffer[..actual_length as usize]))
    }

    /// Query device capabilities
    #[cfg(target_os = "windows")]
    fn query_capabilities(activate: &IMFActivate) -> Option<Vec<CameraCapability>> {
        unsafe {
            // Activate the source to query media types
            let source: IMFMediaSource = activate.ActivateObject().ok()?;

            // Create source reader
            let mut reader: Option<IMFSourceReader> = None;
            MFCreateSourceReaderFromMediaSource(&source, None, &mut reader).ok()?;
            let reader = reader?;

            let mut capabilities = Vec::new();

            // Iterate through available media types
            let mut type_index = 0u32;
            loop {
                let media_type: Option<IMFMediaType> = reader
                    .GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, type_index)
                    .ok();

                match media_type {
                    Some(mt) => {
                        if let Some(cap) = Self::media_type_to_capability(&mt) {
                            // Avoid duplicates
                            if !capabilities.iter().any(|c: &CameraCapability| {
                                c.width == cap.width
                                    && c.height == cap.height
                                    && c.format == cap.format
                            }) {
                                capabilities.push(cap);
                            }
                        }
                        type_index += 1;
                    }
                    None => break,
                }
            }

            // Release the source
            let _ = source.Shutdown();

            Some(capabilities)
        }
    }

    /// Convert IMFMediaType to CameraCapability
    #[cfg(target_os = "windows")]
    fn media_type_to_capability(media_type: &IMFMediaType) -> Option<CameraCapability> {
        unsafe {
            // Get frame size
            let mut frame_size: u64 = 0;
            media_type
                .GetUINT64(&MF_MT_FRAME_SIZE, &mut frame_size)
                .ok()?;
            let width = (frame_size >> 32) as u32;
            let height = (frame_size & 0xFFFFFFFF) as u32;

            // Get frame rate
            let mut frame_rate: u64 = 0;
            let framerate = if media_type
                .GetUINT64(&MF_MT_FRAME_RATE, &mut frame_rate)
                .is_ok()
            {
                let num = (frame_rate >> 32) as f32;
                let den = (frame_rate & 0xFFFFFFFF) as f32;
                if den > 0.0 {
                    num / den
                } else {
                    30.0
                }
            } else {
                30.0
            };

            // Get subtype (pixel format)
            let mut subtype: GUID = GUID::zeroed();
            media_type.GetGUID(&MF_MT_SUBTYPE, &mut subtype).ok()?;
            let format = guid_to_pixel_format(&subtype);

            Some(CameraCapability {
                width,
                height,
                framerate,
                format,
            })
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn enumerate_devices_internal() -> Result<Vec<CameraDevice>, CaptureError> {
        Ok(Vec::new())
    }
}

impl Default for MediaFoundationEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl CameraEnumerator for MediaFoundationEnumerator {
    fn list_devices(&self) -> Vec<CameraDevice> {
        self.devices.values().cloned().collect()
    }

    fn get_device(&self, device_id: &DeviceId) -> Option<CameraDevice> {
        self.devices.get(&device_id.0).cloned()
    }

    fn refresh(&mut self) -> Result<(), CaptureError> {
        #[cfg(target_os = "windows")]
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
// Media Foundation Backend
// ============================================================================

/// Media Foundation-based capture backend
pub struct MediaFoundationBackend {
    #[cfg(target_os = "windows")]
    com_guard: Option<ComGuard>,
    #[cfg(target_os = "windows")]
    mf_guard: Option<MfGuard>,
    #[cfg(target_os = "windows")]
    source_reader: Option<IMFSourceReader>,
    current_device: Option<DeviceId>,
    current_format: Option<NegotiatedFormat>,
    is_capturing: bool,
    frame_sequence: u64,
}

impl MediaFoundationBackend {
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            com_guard: None,
            #[cfg(target_os = "windows")]
            mf_guard: None,
            #[cfg(target_os = "windows")]
            source_reader: None,
            current_device: None,
            current_format: None,
            is_capturing: false,
            frame_sequence: 0,
        }
    }

    /// Initialize COM and Media Foundation if not already done
    #[cfg(target_os = "windows")]
    fn ensure_initialized(&mut self) -> Result<(), CaptureError> {
        if self.com_guard.is_none() {
            self.com_guard = Some(ComGuard::new()?);
        }
        if self.mf_guard.is_none() {
            self.mf_guard = Some(MfGuard::new()?);
        }
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn ensure_initialized(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }

    /// Find and activate a device by ID
    #[cfg(target_os = "windows")]
    fn activate_device(&mut self, device_id: &DeviceId) -> Result<IMFMediaSource, CaptureError> {
        self.ensure_initialized()?;

        unsafe {
            // Create attributes for enumeration
            let mut attributes: Option<IMFAttributes> = None;
            MFCreateAttributes(&mut attributes, 1).map_err(|e| {
                CaptureError::Platform(format!("Failed to create attributes: {:?}", e))
            })?;
            let attributes = attributes.unwrap();

            attributes
                .SetGUID(
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
                )
                .map_err(|e| {
                    CaptureError::Platform(format!("Failed to set source type: {:?}", e))
                })?;

            // Enumerate to find our device
            let mut device_sources: *mut Option<IMFActivate> = ptr::null_mut();
            let mut count: u32 = 0;

            MFEnumDeviceSources(&attributes, &mut device_sources, &mut count).map_err(|e| {
                CaptureError::Platform(format!("Failed to enumerate devices: {:?}", e))
            })?;

            if count == 0 || device_sources.is_null() {
                return Err(CaptureError::DeviceNotFound(device_id.0.clone()));
            }

            let sources = std::slice::from_raw_parts(device_sources, count as usize);

            // Find matching device
            let mut found_source: Option<IMFMediaSource> = None;
            for source_opt in sources {
                if let Some(source) = source_opt {
                    let sym_link = MediaFoundationEnumerator::get_string_attribute(
                        source,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    );

                    if sym_link.as_ref() == Some(&device_id.0) {
                        // Activate this device
                        match source.ActivateObject::<IMFMediaSource>() {
                            Ok(media_source) => {
                                found_source = Some(media_source);
                                break;
                            }
                            Err(e) => {
                                // Check for busy device
                                let hr = e.code();
                                if hr.0 == 0xC00D36B4u32 as i32 {
                                    // MF_E_HW_MFT_FAILED_START_STREAMING or device busy
                                    return Err(CaptureError::DeviceBusy(device_id.0.clone()));
                                }
                                return Err(CaptureError::Platform(format!(
                                    "Failed to activate device: {:?}",
                                    e
                                )));
                            }
                        }
                    }
                }
            }

            // Cleanup
            for source_opt in sources {
                drop(source_opt.clone());
            }
            CoTaskMemFree(Some(device_sources as *const _));

            found_source.ok_or_else(|| CaptureError::DeviceNotFound(device_id.0.clone()))
        }
    }

    /// Configure the source reader for the desired format
    #[cfg(target_os = "windows")]
    fn configure_format(
        reader: &IMFSourceReader,
        settings: &CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        unsafe {
            // Find the best matching media type
            let mut best_type: Option<IMFMediaType> = None;
            let mut best_match_score = 0u32;
            let mut type_index = 0u32;

            loop {
                let media_type = reader.GetNativeMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    type_index,
                );

                match media_type {
                    Ok(mt) => {
                        let mut frame_size: u64 = 0;
                        if mt.GetUINT64(&MF_MT_FRAME_SIZE, &mut frame_size).is_ok() {
                            let width = (frame_size >> 32) as u32;
                            let height = (frame_size & 0xFFFFFFFF) as u32;

                            // Calculate match score
                            let mut score = 0u32;
                            if width == settings.width && height == settings.height {
                                score += 100;
                            } else if width >= settings.width && height >= settings.height {
                                score += 50;
                            }

                            // Prefer formats we can decode
                            let mut subtype = GUID::zeroed();
                            if mt.GetGUID(&MF_MT_SUBTYPE, &mut subtype).is_ok() {
                                let fmt = guid_to_pixel_format(&subtype);
                                match fmt {
                                    PixelFormat::Nv12 => score += 20,
                                    PixelFormat::Yuyv => score += 15,
                                    PixelFormat::Mjpeg => score += 10,
                                    PixelFormat::Rgb24 | PixelFormat::Rgba32 => score += 25,
                                    _ => {}
                                }
                            }

                            if score > best_match_score {
                                best_match_score = score;
                                best_type = Some(mt);
                            }
                        }
                        type_index += 1;
                    }
                    Err(_) => break,
                }
            }

            let selected_type =
                best_type.ok_or(CaptureError::FormatNegotiationFailed {
                    requested: format!("{}x{}", settings.width, settings.height),
                    available: vec![],
                })?;

            // Set the media type
            reader
                .SetCurrentMediaType(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    None,
                    &selected_type,
                )
                .map_err(|e| CaptureError::Platform(format!("Failed to set media type: {:?}", e)))?;

            // Extract the negotiated format
            let mut frame_size: u64 = 0;
            selected_type.GetUINT64(&MF_MT_FRAME_SIZE, &mut frame_size).ok();
            let width = (frame_size >> 32) as u32;
            let height = (frame_size & 0xFFFFFFFF) as u32;

            let mut frame_rate: u64 = 0;
            let framerate = if selected_type.GetUINT64(&MF_MT_FRAME_RATE, &mut frame_rate).is_ok() {
                let num = (frame_rate >> 32) as f32;
                let den = (frame_rate & 0xFFFFFFFF) as f32;
                if den > 0.0 { num / den } else { 30.0 }
            } else {
                30.0
            };

            let mut subtype = GUID::zeroed();
            selected_type.GetGUID(&MF_MT_SUBTYPE, &mut subtype).ok();
            let format = guid_to_pixel_format(&subtype);

            Ok(NegotiatedFormat {
                width,
                height,
                framerate,
                format,
            })
        }
    }

    /// Read a single frame from the source reader
    #[cfg(target_os = "windows")]
    fn read_frame_internal(&mut self) -> Result<Frame, CaptureError> {
        let reader = self.source_reader.as_ref().ok_or_else(|| {
            CaptureError::Platform("No source reader available".to_string())
        })?;

        let format = self.current_format.as_ref().ok_or_else(|| {
            CaptureError::Platform("No format negotiated".to_string())
        })?;

        unsafe {
            let mut flags: u32 = 0;
            let mut timestamp: i64 = 0;
            let mut sample: Option<IMFSample> = None;

            reader
                .ReadSample(
                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )
                .map_err(|e| CaptureError::Platform(format!("Failed to read sample: {:?}", e)))?;

            // Check for stream errors
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                return Err(CaptureError::Platform("Stream error".to_string()));
            }

            // Check for end of stream
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Err(CaptureError::Platform("End of stream".to_string()));
            }

            let sample = sample.ok_or_else(|| {
                CaptureError::FrameTimeout(std::time::Duration::from_millis(100))
            })?;

            // Get the buffer from the sample
            let buffer: IMFMediaBuffer = sample.ConvertToContiguousBuffer().map_err(|e| {
                CaptureError::Platform(format!("Failed to get buffer: {:?}", e))
            })?;

            // Lock and copy data
            let mut data_ptr: *mut u8 = ptr::null_mut();
            let mut length: u32 = 0;
            let mut max_length: u32 = 0;

            buffer.Lock(&mut data_ptr, Some(&mut max_length), Some(&mut length)).map_err(|e| {
                CaptureError::Platform(format!("Failed to lock buffer: {:?}", e))
            })?;

            let data = std::slice::from_raw_parts(data_ptr, length as usize).to_vec();
            buffer.Unlock().ok();

            self.frame_sequence += 1;

            Ok(Frame {
                data,
                format: format.format,
                width: format.width,
                height: format.height,
                timestamp_ns: (timestamp as u64) * 100, // 100ns units to ns
                sequence: self.frame_sequence,
            })
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
        #[cfg(target_os = "windows")]
        {
            MediaFoundationEnumerator::enumerate_devices_internal().unwrap_or_default()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Vec::new()
        }
    }

    #[cfg(target_os = "windows")]
    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        // Close any existing session
        self.close();

        // Initialize and activate device
        let source = self.activate_device(device_id)?;

        // Create source reader
        let mut reader: Option<IMFSourceReader> = None;
        unsafe {
            MFCreateSourceReaderFromMediaSource(&source, None, &mut reader).map_err(|e| {
                CaptureError::Platform(format!("Failed to create source reader: {:?}", e))
            })?;
        }
        let reader = reader.ok_or_else(|| {
            CaptureError::Platform("Source reader creation returned null".to_string())
        })?;

        // Configure format
        let negotiated = Self::configure_format(&reader, &settings)?;

        self.source_reader = Some(reader);
        self.current_device = Some(device_id.clone());
        self.current_format = Some(negotiated.clone());

        info!(
            device = %device_id,
            width = negotiated.width,
            height = negotiated.height,
            fps = negotiated.framerate,
            format = ?negotiated.format,
            "Media Foundation device opened"
        );

        Ok(negotiated)
    }

    #[cfg(not(target_os = "windows"))]
    fn open(
        &mut self,
        _device_id: &DeviceId,
        _settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        Err(CaptureError::Platform(
            "Media Foundation not available on this platform".to_string(),
        ))
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        if self.current_device.is_none() {
            return Err(CaptureError::Platform("No device open".to_string()));
        }
        self.is_capturing = true;
        self.frame_sequence = 0;
        debug!("Media Foundation capture started");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.is_capturing = false;
        debug!("Media Foundation capture stopped");
        Ok(())
    }

    fn close(&mut self) {
        self.is_capturing = false;
        #[cfg(target_os = "windows")]
        {
            self.source_reader = None;
        }
        self.current_device = None;
        self.current_format = None;
        debug!("Media Foundation device closed");
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.current_format.clone()
    }

    #[cfg(target_os = "windows")]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        if !self.is_capturing {
            return Err(CaptureError::Platform("Not capturing".to_string()));
        }
        self.read_frame_internal()
    }

    #[cfg(not(target_os = "windows"))]
    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        Err(CaptureError::Platform(
            "Media Foundation not available on this platform".to_string(),
        ))
    }
}

// ============================================================================
// Format Conversion Helpers
// ============================================================================

/// Convert Media Foundation GUID to PixelFormat
#[cfg(target_os = "windows")]
fn guid_to_pixel_format(guid: &GUID) -> PixelFormat {
    // Note: These GUIDs are defined in mfapi.h
    // MFVideoFormat_NV12, MFVideoFormat_YUY2, MFVideoFormat_MJPG, etc.

    // NV12
    if *guid == GUID::from_u128(0x3231564E_0000_0010_8000_00AA00389B71) {
        return PixelFormat::Nv12;
    }
    // YUY2 (YUYV)
    if *guid == GUID::from_u128(0x32595559_0000_0010_8000_00AA00389B71) {
        return PixelFormat::Yuyv;
    }
    // MJPG
    if *guid == GUID::from_u128(0x47504A4D_0000_0010_8000_00AA00389B71) {
        return PixelFormat::Mjpeg;
    }
    // RGB24
    if *guid == GUID::from_u128(0x00000014_0000_0010_8000_00AA00389B71) {
        return PixelFormat::Rgb24;
    }
    // RGB32
    if *guid == GUID::from_u128(0x00000016_0000_0010_8000_00AA00389B71) {
        return PixelFormat::Rgba32;
    }

    PixelFormat::Unknown
}

#[cfg(not(target_os = "windows"))]
fn guid_to_pixel_format(_guid: &()) -> PixelFormat {
    PixelFormat::Unknown
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = MediaFoundationBackend::new();
        assert!(!backend.is_capturing());
        assert!(backend.current_format().is_none());
    }

    #[test]
    fn test_enumerator_creation() {
        let enumerator = MediaFoundationEnumerator::new();
        // Should not panic, devices list may be empty on non-Windows
        let _ = enumerator.list_devices();
    }

    #[test]
    fn test_start_without_open() {
        let mut backend = MediaFoundationBackend::new();
        let result = backend.start();
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_without_start() {
        let mut backend = MediaFoundationBackend::new();
        // Should not panic
        let _ = backend.stop();
    }

    #[test]
    fn test_close_without_open() {
        let mut backend = MediaFoundationBackend::new();
        // Should not panic
        backend.close();
    }

    #[test]
    fn test_next_frame_without_capturing() {
        let mut backend = MediaFoundationBackend::new();
        let result = backend.next_frame();
        assert!(result.is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_guid_to_pixel_format_nv12() {
        let nv12_guid = GUID::from_u128(0x3231564E_0000_0010_8000_00AA00389B71);
        assert_eq!(guid_to_pixel_format(&nv12_guid), PixelFormat::Nv12);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_guid_to_pixel_format_mjpg() {
        let mjpg_guid = GUID::from_u128(0x47504A4D_0000_0010_8000_00AA00389B71);
        assert_eq!(guid_to_pixel_format(&mjpg_guid), PixelFormat::Mjpeg);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_guid_to_pixel_format_unknown() {
        let unknown_guid = GUID::from_u128(0x12345678_1234_1234_1234_123456789ABC);
        assert_eq!(guid_to_pixel_format(&unknown_guid), PixelFormat::Unknown);
    }
}
