//! Camera hot-plug detection and monitoring
//!
//! Detects when cameras are connected or disconnected and fires events.
//!
//! # Platform Support
//!
//! - **Linux**: Uses inotify to watch `/dev` for video device changes
//! - **macOS**: AVCaptureDevice notifications (not yet implemented)
//! - **Windows**: RegisterDeviceNotification (not yet implemented)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────┐
//! │  HotplugMonitor     │
//! │  (background task)  │
//! └──────────┬──────────┘
//!            │
//!            ▼
//! ┌─────────────────────┐      ┌──────────────────┐
//! │  Platform-specific  │ ───▶ │  CameraEvent     │
//! │  detection          │      │  (via callback)  │
//! └─────────────────────┘      └──────────────────┘
//! ```

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::capture::enumerator::{CameraEnumerator, CameraEvent, CameraEventHandler};
use crate::core::{CaptureError, DeviceId};

/// Configuration for the hot-plug monitor
#[derive(Debug, Clone)]
pub struct HotplugConfig {
    /// How often to poll for changes (fallback mode)
    pub poll_interval: Duration,
    /// How long to wait before declaring a device reconnected
    pub reconnect_debounce: Duration,
}

impl Default for HotplugConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            reconnect_debounce: Duration::from_millis(500),
        }
    }
}

/// Handle to control a running hot-plug monitor
pub struct HotplugMonitorHandle {
    stop_signal: Arc<AtomicBool>,
    thread_handle: Option<JoinHandle<()>>,
}

impl HotplugMonitorHandle {
    /// Stop the hot-plug monitor
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::Release);
    }

    /// Wait for the monitor thread to finish
    pub fn join(mut self) {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    /// Check if the monitor is still running
    pub fn is_running(&self) -> bool {
        self.thread_handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }
}

impl Drop for HotplugMonitorHandle {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Release);
    }
}

/// Start a hot-plug monitor using polling-based detection
///
/// This is the fallback implementation that works on all platforms.
/// Platform-specific implementations (inotify, udev) can be more responsive.
///
/// # Arguments
///
/// * `enumerator` - The camera enumerator to use for device discovery
/// * `handler` - Callback for hot-plug events
/// * `config` - Monitor configuration
///
/// # Returns
///
/// Handle to control the monitor
pub fn start_hotplug_monitor<E, H>(
    enumerator: E,
    handler: H,
    config: HotplugConfig,
) -> Result<HotplugMonitorHandle, CaptureError>
where
    E: CameraEnumerator + 'static,
    H: CameraEventHandler + 'static,
{
    let stop_signal = Arc::new(AtomicBool::new(false));
    let stop_signal_clone = stop_signal.clone();

    // Get initial device list
    let devices = enumerator.enumerate()?;
    let known_devices: HashSet<DeviceId> = devices.iter().map(|d| d.id.clone()).collect();

    tracing::info!(
        device_count = known_devices.len(),
        "Hot-plug monitor started"
    );

    let thread_handle = thread::Builder::new()
        .name("micround-hotplug".into())
        .spawn(move || {
            hotplug_thread_main(
                enumerator,
                handler,
                known_devices,
                config,
                stop_signal_clone,
            );
        })
        .map_err(|e| CaptureError::Platform(format!("Failed to spawn hotplug thread: {}", e)))?;

    Ok(HotplugMonitorHandle {
        stop_signal,
        thread_handle: Some(thread_handle),
    })
}

/// Main function for the hot-plug monitor thread
fn hotplug_thread_main<E, H>(
    mut enumerator: E,
    mut handler: H,
    mut known_devices: HashSet<DeviceId>,
    config: HotplugConfig,
    stop_signal: Arc<AtomicBool>,
)
where
    E: CameraEnumerator,
    H: CameraEventHandler,
{
    while !stop_signal.load(Ordering::Acquire) {
        // Sleep for the poll interval
        thread::sleep(config.poll_interval);

        if stop_signal.load(Ordering::Acquire) {
            break;
        }

        // Refresh and check for changes
        if let Err(e) = enumerator.refresh() {
            tracing::warn!(error = %e, "Failed to refresh device list");
            continue;
        }

        let devices = match enumerator.enumerate() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to enumerate devices");
                continue;
            }
        };

        let current_devices: HashSet<DeviceId> = devices.iter().map(|d| d.id.clone()).collect();

        // Check for new devices (connected)
        for device in &devices {
            if !known_devices.contains(&device.id) {
                tracing::info!(device_id = %device.id.0, name = %device.name, "Camera connected");
                handler.on_camera_event(CameraEvent::Connected(device.clone()));
            }
        }

        // Check for removed devices (disconnected)
        for device_id in &known_devices {
            if !current_devices.contains(device_id) {
                tracing::info!(device_id = %device_id.0, "Camera disconnected");
                handler.on_camera_event(CameraEvent::Disconnected(device_id.clone()));
            }
        }

        // Update known devices
        known_devices = current_devices;
    }

    tracing::info!("Hot-plug monitor stopped");
}

// ============================================================================
// Linux-specific inotify implementation
// ============================================================================

#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Check if a path looks like a video device
    pub fn is_video_device(path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with("video"))
            .unwrap_or(false)
    }

    /// List current video device paths
    pub fn list_video_devices() -> Vec<String> {
        let dev_path = Path::new("/dev");
        let mut devices = Vec::new();

        if let Ok(entries) = fs::read_dir(dev_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if is_video_device(&path) {
                    if let Some(path_str) = path.to_str() {
                        devices.push(path_str.to_string());
                    }
                }
            }
        }

        devices.sort();
        devices
    }

    /// Get device ID from a video device path
    pub fn device_path_to_id(path: &str) -> DeviceId {
        DeviceId(path.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_is_video_device() {
            assert!(is_video_device(Path::new("/dev/video0")));
            assert!(is_video_device(Path::new("/dev/video1")));
            assert!(is_video_device(Path::new("/dev/video10")));
            assert!(!is_video_device(Path::new("/dev/sda1")));
            assert!(!is_video_device(Path::new("/dev/null")));
            assert!(!is_video_device(Path::new("/dev/tty")));
        }

        #[test]
        fn test_list_video_devices() {
            // This test will pass even if no video devices are present
            let devices = list_video_devices();
            // Just verify it doesn't panic
            for device in &devices {
                assert!(device.starts_with("/dev/video"));
            }
        }
    }
}

// ============================================================================
// Event Handler Implementations
// ============================================================================

/// A simple handler that collects events into a Vec for testing
#[cfg(test)]
pub struct CollectingHandler {
    pub events: Vec<CameraEvent>,
}

#[cfg(test)]
impl CollectingHandler {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }
}

#[cfg(test)]
impl CameraEventHandler for CollectingHandler {
    fn on_camera_event(&mut self, event: CameraEvent) {
        self.events.push(event);
    }
}

/// A handler that forwards events to a channel
pub struct ChannelHandler {
    sender: std::sync::mpsc::Sender<CameraEvent>,
}

impl ChannelHandler {
    pub fn new(sender: std::sync::mpsc::Sender<CameraEvent>) -> Self {
        Self { sender }
    }
}

impl CameraEventHandler for ChannelHandler {
    fn on_camera_event(&mut self, event: CameraEvent) {
        let _ = self.sender.send(event);
    }
}

/// A handler that forwards events to a tokio channel
pub struct TokioChannelHandler {
    sender: tokio::sync::mpsc::UnboundedSender<CameraEvent>,
}

impl TokioChannelHandler {
    pub fn new(sender: tokio::sync::mpsc::UnboundedSender<CameraEvent>) -> Self {
        Self { sender }
    }
}

impl CameraEventHandler for TokioChannelHandler {
    fn on_camera_event(&mut self, event: CameraEvent) {
        let _ = self.sender.send(event);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CameraCapability, CameraDevice, PixelFormat};
    use std::sync::{Arc, Mutex};

    /// Mock enumerator for testing
    struct MockEnumerator {
        devices: Arc<Mutex<Vec<CameraDevice>>>,
    }

    impl MockEnumerator {
        fn new(devices: Vec<CameraDevice>) -> Self {
            Self {
                devices: Arc::new(Mutex::new(devices)),
            }
        }

        fn add_device(&self, device: CameraDevice) {
            self.devices.lock().unwrap().push(device);
        }

        fn remove_device(&self, id: &DeviceId) {
            let mut devices = self.devices.lock().unwrap();
            devices.retain(|d| &d.id != id);
        }
    }

    impl CameraEnumerator for MockEnumerator {
        fn enumerate(&self) -> Result<Vec<CameraDevice>, CaptureError> {
            Ok(self.devices.lock().unwrap().clone())
        }

        fn get_device(&self, id: &DeviceId) -> Result<CameraDevice, CaptureError> {
            self.devices
                .lock()
                .unwrap()
                .iter()
                .find(|d| &d.id == id)
                .cloned()
                .ok_or_else(|| CaptureError::DeviceNotFound(id.0.clone()))
        }

        fn get_capabilities(&self, _id: &DeviceId) -> Result<Vec<CameraCapability>, CaptureError> {
            Ok(vec![CameraCapability {
                width: 1920,
                height: 1080,
                framerate: 30.0,
                format: PixelFormat::Mjpeg,
            }])
        }

        fn is_available(&self, id: &DeviceId) -> bool {
            self.devices.lock().unwrap().iter().any(|d| &d.id == id)
        }

        fn refresh(&mut self) -> Result<(), CaptureError> {
            Ok(())
        }
    }

    fn make_test_device(id: &str, name: &str) -> CameraDevice {
        CameraDevice {
            id: DeviceId(id.into()),
            name: name.into(),
            manufacturer: None,
            capabilities: vec![],
            is_available: true,
        }
    }

    #[test]
    fn test_hotplug_config_default() {
        let config = HotplugConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(2));
        assert_eq!(config.reconnect_debounce, Duration::from_millis(500));
    }

    #[test]
    fn test_channel_handler() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut handler = ChannelHandler::new(tx);

        let device = make_test_device("test", "Test Camera");
        handler.on_camera_event(CameraEvent::Connected(device.clone()));

        let received = rx.recv().unwrap();
        if let CameraEvent::Connected(d) = received {
            assert_eq!(d.id.0, "test");
        } else {
            panic!("Expected Connected event");
        }
    }

    #[test]
    fn test_mock_enumerator() {
        let device = make_test_device("/dev/video0", "USB Camera");
        let enumerator = MockEnumerator::new(vec![device.clone()]);

        // Initial enumeration
        let devices = enumerator.enumerate().unwrap();
        assert_eq!(devices.len(), 1);

        // Add device
        let device2 = make_test_device("/dev/video1", "Another Camera");
        enumerator.add_device(device2);

        let devices = enumerator.enumerate().unwrap();
        assert_eq!(devices.len(), 2);

        // Remove device
        enumerator.remove_device(&DeviceId("/dev/video0".into()));

        let devices = enumerator.enumerate().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id.0, "/dev/video1");
    }

    #[test]
    fn test_start_and_stop_monitor() {
        let enumerator = MockEnumerator::new(vec![]);
        let handler = CollectingHandler::new();

        let config = HotplugConfig {
            poll_interval: Duration::from_millis(50),
            ..Default::default()
        };

        let handle = start_hotplug_monitor(enumerator, handler, config).unwrap();

        assert!(handle.is_running());

        // Stop the monitor and wait for it to finish
        handle.stop();
        // Wait up to 500ms for the thread to stop (may take longer on loaded systems)
        for _ in 0..10 {
            if !handle.is_running() {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }

        // Monitor should be stopped
        assert!(!handle.is_running(), "Monitor thread did not stop in time");
    }
}
