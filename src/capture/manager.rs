//! Unified capture API
//!
//! Platform-agnostic interface for the capture subsystem.
#![allow(dead_code)] // Capture manager API
//! Hides complexity of V4L2/MediaFoundation/AVFoundation behind a simple API.
//!
//! # Thread Safety
//!
//! All methods are safe to call from any thread. Internal synchronization
//! uses mutexes and atomic operations.
//!
//! # Example
//!
//! ```ignore
//! let manager = CaptureManager::new()?;
//!
//! // List available cameras
//! let cameras = manager.enumerate_cameras();
//!
//! // Select a camera
//! manager.select_camera(&cameras[0].id)?;
//!
//! // Subscribe to frames
//! let mut rx = manager.subscribe_frames();
//!
//! // Start capture
//! manager.start_capture(CaptureSettings::default())?;
//!
//! // Receive frames
//! while let Some(frame) = rx.recv().await {
//!     // Process frame
//! }
//!
//! // Stop capture
//! manager.stop_capture()?;
//! ```

use std::sync::{Arc, Mutex, RwLock};
use std::collections::HashMap;

use tokio::sync::broadcast;

use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings,
    DeviceId, Frame, NegotiatedFormat,
};
use crate::capture::{
    CaptureBackend, CameraEnumerator,
    CameraState, SharedCameraState, shared_camera_state,
    start_capture_loop, CaptureLoopHandle, FrameReceiver, MetricsSnapshot,
};

// ============================================================================
// Device Events
// ============================================================================

/// Events related to camera device changes
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// A camera was connected
    Connected(CameraDevice),
    /// A camera was disconnected
    Disconnected(DeviceId),
    /// Camera availability changed
    AvailabilityChanged { device_id: DeviceId, available: bool },
}

/// Broadcast channel capacity for device events
const DEVICE_EVENT_CAPACITY: usize = 16;

/// Broadcast channel capacity for frames
const FRAME_BROADCAST_CAPACITY: usize = 4;

// ============================================================================
// Capture Handle
// ============================================================================

/// Handle returned when a camera is selected
///
/// Provides access to camera-specific operations without exposing
/// the internal manager state.
#[derive(Clone)]
pub struct CameraHandle {
    device_id: DeviceId,
    device_info: CameraDevice,
    state: SharedCameraState,
}

impl CameraHandle {
    /// Get the device ID
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Get the device information
    pub fn device_info(&self) -> &CameraDevice {
        &self.device_info
    }

    /// Get the current state
    pub fn state(&self) -> CameraState {
        self.state.state()
    }

    /// Get the negotiated format (if capturing)
    pub fn format(&self) -> Option<NegotiatedFormat> {
        self.state.format()
    }

    /// Check if currently capturing
    pub fn is_capturing(&self) -> bool {
        self.state.state().is_capturing()
    }
}

// ============================================================================
// Capture Manager
// ============================================================================

/// Internal state for a managed camera
struct ManagedCamera {
    device: CameraDevice,
    state: SharedCameraState,
}

/// Central manager for the capture subsystem
///
/// Provides a unified, thread-safe API for camera operations.
pub struct CaptureManager {
    /// Platform-specific backend (protected by mutex for Send)
    backend: Mutex<Option<Box<dyn CaptureBackend>>>,
    /// Platform-specific enumerator
    enumerator: Mutex<Option<Box<dyn CameraEnumerator>>>,
    /// Currently selected camera
    selected_camera: RwLock<Option<CameraHandle>>,
    /// Active capture loop handle
    capture_handle: Mutex<Option<CaptureLoopHandle>>,
    /// Frame receiver from capture loop
    frame_receiver: Mutex<Option<FrameReceiver>>,
    /// Frame broadcaster for multiple subscribers
    frame_broadcaster: broadcast::Sender<Arc<Frame>>,
    /// Device event broadcaster
    device_events: broadcast::Sender<DeviceEvent>,
    /// Known cameras and their states
    cameras: RwLock<HashMap<String, ManagedCamera>>,
}

impl CaptureManager {
    /// Create a new capture manager with the default platform backend
    pub fn new() -> Result<Self, CaptureError> {
        // Create platform-specific backend and enumerator
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        let (backend, enumerator) = (
            crate::capture::create_backend(),
            crate::capture::create_enumerator(),
        );

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        let (backend, enumerator): (Box<dyn CaptureBackend>, Box<dyn CameraEnumerator>) = {
            return Err(CaptureError::Platform(
                "Capture backend not implemented for this platform".into(),
            ));
        };

        let (frame_tx, _) = broadcast::channel(FRAME_BROADCAST_CAPACITY);
        let (device_tx, _) = broadcast::channel(DEVICE_EVENT_CAPACITY);

        Ok(Self {
            backend: Mutex::new(Some(backend)),
            enumerator: Mutex::new(Some(enumerator)),
            selected_camera: RwLock::new(None),
            capture_handle: Mutex::new(None),
            frame_receiver: Mutex::new(None),
            frame_broadcaster: frame_tx,
            device_events: device_tx,
            cameras: RwLock::new(HashMap::new()),
        })
    }

    fn cleanup_stopped_capture(&self) -> Result<(), CaptureError> {
        let mut handle_guard = self.capture_handle.lock().unwrap();
        if let Some(handle) = handle_guard.as_ref() {
            if handle.is_running() {
                return Err(CaptureError::Platform(
                    "Capture is still running".into(),
                ));
            }
        }

        if let Some(handle) = handle_guard.take() {
            if let Err(err) = handle.join() {
                tracing::warn!(error = %err, "Capture thread exited with error");
            }
        }

        Ok(())
    }

    fn ensure_backend_available(&self) -> Result<(), CaptureError> {
        self.cleanup_stopped_capture()?;

        let mut backend_guard = self.backend.lock().unwrap();
        if backend_guard.is_none() {
            #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
            {
                *backend_guard = Some(crate::capture::create_backend());
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
            {
                return Err(CaptureError::Platform(
                    "Capture backend not implemented for this platform".into(),
                ));
            }
        }

        Ok(())
    }

    /// Create a capture manager with a custom backend (for testing)
    pub fn with_backend(
        backend: Box<dyn CaptureBackend>,
        enumerator: Box<dyn CameraEnumerator>,
    ) -> Self {
        let (frame_tx, _) = broadcast::channel(FRAME_BROADCAST_CAPACITY);
        let (device_tx, _) = broadcast::channel(DEVICE_EVENT_CAPACITY);

        Self {
            backend: Mutex::new(Some(backend)),
            enumerator: Mutex::new(Some(enumerator)),
            selected_camera: RwLock::new(None),
            capture_handle: Mutex::new(None),
            frame_receiver: Mutex::new(None),
            frame_broadcaster: frame_tx,
            device_events: device_tx,
            cameras: RwLock::new(HashMap::new()),
        }
    }

    // ========================================================================
    // Device Discovery
    // ========================================================================

    /// Enumerate all available camera devices
    ///
    /// Returns a list of currently detected cameras. This list may change
    /// over time as devices are connected or disconnected.
    pub fn enumerate_cameras(&self) -> Vec<CameraDevice> {
        let mut enumerator_guard = self.enumerator.lock().unwrap();
        let enumerator = match enumerator_guard.as_mut() {
            Some(e) => e,
            None => return vec![],
        };

        // Refresh and get devices
        let _ = enumerator.refresh();
        let devices = enumerator.enumerate().unwrap_or_default();

        // Update internal camera tracking
        let mut cameras = self.cameras.write().unwrap();
        for device in &devices {
            let id = device.id.0.clone();
            if !cameras.contains_key(&id) {
                cameras.insert(id.clone(), ManagedCamera {
                    device: device.clone(),
                    state: shared_camera_state(device.id.clone()),
                });
                // Mark as available
                if let Some(cam) = cameras.get(&id) {
                    let _ = cam.state.device_arrived();
                }
            }
        }

        // Mark disconnected cameras
        let current_ids: std::collections::HashSet<_> =
            devices.iter().map(|d| d.id.0.clone()).collect();
        for (id, cam) in cameras.iter() {
            if !current_ids.contains(id) && cam.state.state().is_connected() {
                let _ = cam.state.device_removed();
                let _ = self.device_events.send(DeviceEvent::Disconnected(DeviceId(id.clone())));
            }
        }

        devices
    }

    /// Get capabilities for a specific device
    pub fn get_capabilities(&self, device_id: &DeviceId) -> Result<Vec<CameraCapability>, CaptureError> {
        let enumerator_guard = self.enumerator.lock().unwrap();
        let enumerator = enumerator_guard.as_ref()
            .ok_or_else(|| CaptureError::Platform("No enumerator available".into()))?;

        enumerator.get_capabilities(device_id)
    }

    /// Subscribe to device connection/disconnection events
    pub fn subscribe_device_events(&self) -> broadcast::Receiver<DeviceEvent> {
        self.device_events.subscribe()
    }

    // ========================================================================
    // Camera Selection
    // ========================================================================

    /// Select a camera for capture
    ///
    /// Returns a handle that can be used to query camera state.
    /// Only one camera can be selected at a time.
    pub fn select_camera(&self, device_id: &DeviceId) -> Result<CameraHandle, CaptureError> {
        // Check if camera exists
        let cameras = self.cameras.read().unwrap();
        let cam = cameras.get(&device_id.0)
            .ok_or_else(|| CaptureError::DeviceNotFound(device_id.0.clone()))?;

        // Create handle
        let handle = CameraHandle {
            device_id: device_id.clone(),
            device_info: cam.device.clone(),
            state: cam.state.clone(),
        };

        // Store as selected
        *self.selected_camera.write().unwrap() = Some(handle.clone());

        Ok(handle)
    }

    /// Get the currently selected camera
    pub fn selected_camera(&self) -> Option<CameraHandle> {
        self.selected_camera.read().unwrap().clone()
    }

    /// Deselect the current camera
    pub fn deselect_camera(&self) {
        // Stop capture if running
        let _ = self.stop_capture();
        *self.selected_camera.write().unwrap() = None;
    }

    // ========================================================================
    // Capture Control
    // ========================================================================

    /// Start capturing from the selected camera
    ///
    /// Opens the camera with the given settings and begins frame capture.
    /// Frames are delivered to subscribers via `subscribe_frames()`.
    pub fn start_capture(&self, settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError> {
        // Get selected camera
        let selected = self.selected_camera.read().unwrap();
        let handle = selected.as_ref()
            .ok_or_else(|| CaptureError::Platform("No camera selected".into()))?
            .clone();
        drop(selected);

        // Check state allows opening
        if !handle.state.state().can_open() {
            return Err(CaptureError::Platform(format!(
                "Cannot start capture in state: {}",
                handle.state.state()
            )));
        }

        self.ensure_backend_available()?;

        // Take the backend (we give it to the capture loop)
        let backend = self.backend.lock().unwrap().take()
            .ok_or_else(|| CaptureError::Platform("Backend already in use".into()))?;

        // Update state
        handle.state.begin_open()?;

        // Start capture loop
        let device_id = handle.device_id.clone();
        match start_capture_loop(backend, device_id.clone(), settings) {
            Ok((loop_handle, frame_rx)) => {
                let format = loop_handle.format().clone();

                // Update state with format
                handle.state.open_succeeded(format.clone())?;
                handle.state.start_capture()?;

                // Store handles
                *self.capture_handle.lock().unwrap() = Some(loop_handle);
                *self.frame_receiver.lock().unwrap() = Some(frame_rx);

                Ok(format)
            }
            Err(loop_error) => {
                // Recover the backend so we can retry later (if available)
                if let Some(recovered_backend) = loop_error.backend {
                    *self.backend.lock().unwrap() = Some(recovered_backend);
                }
                // If backend was None, it was irrecoverably lost (thread spawn failure)
                // The manager will need a new backend created to continue
                handle.state.open_failed(&loop_error.error)?;
                Err(loop_error.error)
            }
        }
    }

    /// Stop the current capture
    pub fn stop_capture(&self) -> Result<(), CaptureError> {
        // Stop capture loop
        if let Some(loop_handle) = self.capture_handle.lock().unwrap().as_ref() {
            loop_handle.stop();
        }

        // Clear frame receiver
        *self.frame_receiver.lock().unwrap() = None;

        // Update state
        if let Some(handle) = self.selected_camera.read().unwrap().as_ref() {
            if handle.state.state().is_capturing() {
                handle.state.stop_capture()?;
            }
        }

        Ok(())
    }

    /// Check if currently capturing
    pub fn is_capturing(&self) -> bool {
        self.capture_handle.lock().unwrap()
            .as_ref()
            .map(|h| h.is_running())
            .unwrap_or(false)
    }

    /// Get the current capture format (if capturing)
    pub fn current_format(&self) -> Option<NegotiatedFormat> {
        self.selected_camera.read().unwrap()
            .as_ref()
            .and_then(|h| h.format())
    }

    /// Get current capture metrics
    pub fn metrics(&self) -> Option<MetricsSnapshot> {
        self.capture_handle.lock().unwrap()
            .as_ref()
            .map(|h| h.metrics())
    }

    // ========================================================================
    // Frame Access
    // ========================================================================

    /// Subscribe to captured frames
    ///
    /// Returns a receiver that will receive frames as they are captured.
    /// Multiple subscribers can exist; each receives all frames.
    ///
    /// Note: If subscribers fall behind, older frames are dropped.
    pub fn subscribe_frames(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.frame_broadcaster.subscribe()
    }

    /// Get direct access to the frame receiver (single consumer)
    ///
    /// This provides lower-overhead access than broadcast subscription,
    /// but only one consumer can use this at a time.
    pub fn take_frame_receiver(&self) -> Option<FrameReceiver> {
        self.frame_receiver.lock().unwrap().take()
    }

    // ========================================================================
    // State Access
    // ========================================================================

    /// Get the state of a specific camera
    pub fn camera_state(&self, device_id: &DeviceId) -> Option<CameraState> {
        self.cameras.read().unwrap()
            .get(&device_id.0)
            .map(|c| c.state.state())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PixelFormat;

    /// Mock capture backend for testing
    struct MockBackend {
        devices: Vec<CameraDevice>,
        open_result: Result<NegotiatedFormat, CaptureError>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                devices: vec![CameraDevice {
                    id: DeviceId("mock-camera-1".into()),
                    name: "Mock Camera 1".into(),
                    manufacturer: Some("MockCorp".into()),
                    capabilities: vec![CameraCapability {
                        width: 1920,
                        height: 1080,
                        framerate: 30.0,
                        format: PixelFormat::Mjpeg,
                    }],
                    is_available: true,
                }],
                open_result: Ok(NegotiatedFormat {
                    width: 1920,
                    height: 1080,
                    framerate: 30.0,
                    format: PixelFormat::Mjpeg,
                    exact_match: true,
                }),
            }
        }
    }

    impl CaptureBackend for MockBackend {
        fn enumerate_devices(&self) -> Vec<CameraDevice> {
            self.devices.clone()
        }

        fn open(&mut self, _device_id: &DeviceId, _settings: CaptureSettings) -> Result<NegotiatedFormat, CaptureError> {
            self.open_result.clone()
        }

        fn start(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }

        fn stop(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }

        fn close(&mut self) {}

        fn is_capturing(&self) -> bool {
            false
        }

        fn current_format(&self) -> Option<NegotiatedFormat> {
            None
        }

        fn next_frame(&mut self) -> Result<Frame, CaptureError> {
            // Return a mock frame
            Ok(Frame {
                data: vec![0u8; 1920 * 1080 * 3],
                format: PixelFormat::Rgb24,
                width: 1920,
                height: 1080,
                timestamp_ns: 0,
                sequence: 0,
            })
        }
    }

    /// Mock enumerator for testing
    struct MockEnumerator {
        devices: Vec<CameraDevice>,
    }

    impl MockEnumerator {
        fn new() -> Self {
            Self {
                devices: vec![CameraDevice {
                    id: DeviceId("mock-camera-1".into()),
                    name: "Mock Camera 1".into(),
                    manufacturer: Some("MockCorp".into()),
                    capabilities: vec![CameraCapability {
                        width: 1920,
                        height: 1080,
                        framerate: 30.0,
                        format: PixelFormat::Mjpeg,
                    }],
                    is_available: true,
                }],
            }
        }
    }

    impl CameraEnumerator for MockEnumerator {
        fn enumerate(&self) -> Result<Vec<CameraDevice>, CaptureError> {
            Ok(self.devices.clone())
        }

        fn get_device(&self, id: &DeviceId) -> Result<CameraDevice, CaptureError> {
            self.devices.iter()
                .find(|d| &d.id == id)
                .cloned()
                .ok_or_else(|| CaptureError::DeviceNotFound(id.0.clone()))
        }

        fn get_capabilities(&self, id: &DeviceId) -> Result<Vec<CameraCapability>, CaptureError> {
            self.get_device(id).map(|d| d.capabilities)
        }

        fn is_available(&self, id: &DeviceId) -> bool {
            self.devices.iter().any(|d| &d.id == id && d.is_available)
        }

        fn refresh(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }
    }

    #[test]
    fn test_enumerate_cameras() {
        let manager = CaptureManager::with_backend(
            Box::new(MockBackend::new()),
            Box::new(MockEnumerator::new()),
        );

        let cameras = manager.enumerate_cameras();
        assert_eq!(cameras.len(), 1);
        assert_eq!(cameras[0].name, "Mock Camera 1");
    }

    #[test]
    fn test_select_camera() {
        let manager = CaptureManager::with_backend(
            Box::new(MockBackend::new()),
            Box::new(MockEnumerator::new()),
        );

        // Enumerate first to populate internal state
        let cameras = manager.enumerate_cameras();

        // Select camera
        let handle = manager.select_camera(&cameras[0].id).unwrap();
        assert_eq!(handle.device_id(), &cameras[0].id);
        assert_eq!(handle.state(), CameraState::Available);
    }

    #[test]
    fn test_no_camera_selected_error() {
        let manager = CaptureManager::with_backend(
            Box::new(MockBackend::new()),
            Box::new(MockEnumerator::new()),
        );

        // Try to start without selecting
        let result = manager.start_capture(CaptureSettings::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_device_not_found() {
        let manager = CaptureManager::with_backend(
            Box::new(MockBackend::new()),
            Box::new(MockEnumerator::new()),
        );

        let result = manager.select_camera(&DeviceId("nonexistent".into()));
        assert!(matches!(result, Err(CaptureError::DeviceNotFound(_))));
    }

    #[test]
    fn test_camera_handle_clone() {
        let manager = CaptureManager::with_backend(
            Box::new(MockBackend::new()),
            Box::new(MockEnumerator::new()),
        );

        let cameras = manager.enumerate_cameras();
        let handle = manager.select_camera(&cameras[0].id).unwrap();

        let handle2 = handle.clone();
        assert_eq!(handle.device_id(), handle2.device_id());
    }

    #[test]
    fn test_get_capabilities() {
        let manager = CaptureManager::with_backend(
            Box::new(MockBackend::new()),
            Box::new(MockEnumerator::new()),
        );

        let cameras = manager.enumerate_cameras();
        let caps = manager.get_capabilities(&cameras[0].id).unwrap();

        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].width, 1920);
        assert_eq!(caps[0].height, 1080);
    }
}
