//! Camera reconnection logic
//!
//! Automatically reconnects to the camera when it becomes available again after
//! disconnection. Essential for day-30 reliability where USB devices may be bumped.
//!
//! # Reconnection Flow
//!
//! ```text
//! Camera Disconnected
//!        │
//!        ▼
//! ┌──────────────────┐
//! │ Freeze last frame │
//! │ Show overlay      │
//! └────────┬─────────┘
//!          │
//!          ▼
//! ┌──────────────────┐     device appears
//! │   Poll for device │────────────────────┐
//! │   (every 2 sec)   │                    │
//! └────────┬─────────┘                     │
//!          │ timeout (30 sec)              │
//!          ▼                               ▼
//! ┌──────────────────┐          ┌──────────────────┐
//! │ Show fallback     │          │ Attempt reconnect │
//! │ wallpaper         │          └────────┬─────────┘
//! │ Notify user       │                   │
//! └──────────────────┘           success  │  failure
//!                                         ▼         │
//!                                ┌────────────┐     │
//!                                │ Resume feed │◄────┘ (retry)
//!                                └────────────┘
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use crate::capture::enumerator::{CameraEvent, CameraEventHandler};
use crate::core::events::{Event, EventBus};
use crate::core::{CaptureError, CaptureSettings, DeviceId, NegotiatedFormat};
use crate::capture::CaptureManager;

// ============================================================================
// Reconnection State
// ============================================================================

/// State of the reconnection process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectionState {
    /// Normal operation, no reconnection in progress
    Connected,
    /// Waiting for device to reappear (actively polling)
    WaitingForDevice {
        /// When the reconnection attempt started
        started_at_epoch_ms: u64,
    },
    /// Device appeared, attempting to open
    Reconnecting,
    /// Reconnection timed out, user intervention needed
    TimedOut,
    /// Reconnection failed fatally
    Failed,
}

impl ReconnectionState {
    /// Check if actively trying to reconnect
    pub fn is_reconnecting(&self) -> bool {
        matches!(
            self,
            Self::WaitingForDevice { .. } | Self::Reconnecting
        )
    }

    /// Check if user intervention is needed
    pub fn needs_user_action(&self) -> bool {
        matches!(self, Self::TimedOut | Self::Failed)
    }
}

// ============================================================================
// Reconnection Configuration
// ============================================================================

/// Configuration for reconnection behavior
#[derive(Debug, Clone)]
pub struct ReconnectionConfig {
    /// How often to check for the device during active polling
    pub poll_interval: Duration,
    /// How long to actively poll before switching to background mode
    pub active_poll_duration: Duration,
    /// How often to check in background mode
    pub background_poll_interval: Duration,
    /// Maximum number of reconnection attempts before giving up
    pub max_reconnect_attempts: u32,
    /// Delay between reconnection attempts
    pub reconnect_retry_delay: Duration,
    /// Whether to automatically try the same device
    pub auto_reconnect_same_device: bool,
    /// Whether to fallback to any available camera
    pub fallback_to_any_camera: bool,
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(2),
            active_poll_duration: Duration::from_secs(30),
            background_poll_interval: Duration::from_secs(10),
            max_reconnect_attempts: 3,
            reconnect_retry_delay: Duration::from_millis(500),
            auto_reconnect_same_device: true,
            fallback_to_any_camera: false,
        }
    }
}

// ============================================================================
// Device Matcher
// ============================================================================

/// Strategy for matching a returning device
#[derive(Debug, Clone)]
pub enum DeviceMatchStrategy {
    /// Match by exact device ID (persistent identifier)
    ExactId(DeviceId),
    /// Match by device name and capabilities
    NameAndCaps {
        name: String,
        width: u32,
        height: u32,
    },
    /// Match any available camera
    Any,
}

impl DeviceMatchStrategy {
    /// Create a matcher for a specific device
    pub fn for_device(device_id: DeviceId, _name: String, _width: u32, _height: u32) -> Self {
        // Try exact ID first, but store fallback info for future NameAndCaps matching
        Self::ExactId(device_id)
    }

    /// Check if a device matches this strategy
    pub fn matches(&self, device_id: &DeviceId, name: &str, caps: &[(u32, u32)]) -> bool {
        match self {
            Self::ExactId(id) => device_id == id,
            Self::NameAndCaps {
                name: match_name,
                width,
                height,
            } => {
                name == match_name && caps.iter().any(|(w, h)| w == width && h == height)
            }
            Self::Any => true,
        }
    }
}

// ============================================================================
// Reconnection Manager
// ============================================================================

/// Manages automatic camera reconnection
///
/// Monitors camera events and attempts to reconnect when a camera
/// disconnects and later reappears.
pub struct ReconnectionManager {
    /// Current reconnection state
    state: RwLock<ReconnectionState>,
    /// Configuration
    config: ReconnectionConfig,
    /// The device we're trying to reconnect to
    target_device: RwLock<Option<DeviceMatchStrategy>>,
    /// The device ID we're trying to reconnect to (for events)
    target_device_id: RwLock<Option<DeviceId>>,
    /// Original capture settings to use on reconnect
    capture_settings: RwLock<Option<CaptureSettings>>,
    /// Event bus for publishing state changes
    event_bus: EventBus,
    /// Number of reconnection attempts made
    reconnect_attempts: RwLock<u32>,
    /// Flag to stop reconnection attempts
    cancelled: AtomicBool,
    /// When the wait started (monotonic time for reliable timeout tracking)
    /// Uses Instant instead of SystemTime to avoid NTP clock adjustment issues.
    wait_started: RwLock<Option<Instant>>,
}

impl ReconnectionManager {
    /// Create a new reconnection manager
    pub fn new(event_bus: EventBus) -> Self {
        Self::with_config(event_bus, ReconnectionConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(event_bus: EventBus, config: ReconnectionConfig) -> Self {
        Self {
            state: RwLock::new(ReconnectionState::Connected),
            config,
            target_device: RwLock::new(None),
            target_device_id: RwLock::new(None),
            capture_settings: RwLock::new(None),
            event_bus,
            reconnect_attempts: RwLock::new(0),
            cancelled: AtomicBool::new(false),
            wait_started: RwLock::new(None),
        }
    }

    // ========================================================================
    // State Access
    // ========================================================================

    /// Get the current reconnection state
    pub fn state(&self) -> ReconnectionState {
        *self.state.read().unwrap()
    }

    /// Check if a reconnection is in progress
    pub fn is_reconnecting(&self) -> bool {
        self.state().is_reconnecting()
    }

    /// Check if user intervention is needed
    pub fn needs_user_action(&self) -> bool {
        self.state().needs_user_action()
    }

    // ========================================================================
    // Reconnection Control
    // ========================================================================

    /// Handle a camera disconnection event
    ///
    /// This triggers the reconnection flow:
    /// 1. Saves the target device info for matching
    /// 2. Transitions to WaitingForDevice state
    /// 3. Publishes CameraDisconnected event
    pub fn on_camera_disconnected(
        &self,
        device_id: DeviceId,
        device_name: String,
        settings: CaptureSettings,
    ) {
        tracing::info!(
            device_id = %device_id.0,
            device_name = %device_name,
            "Camera disconnected, starting reconnection flow"
        );

        // Save target device info
        *self.target_device.write().unwrap() = Some(DeviceMatchStrategy::for_device(
            device_id.clone(),
            device_name,
            settings.width,
            settings.height,
        ));
        *self.target_device_id.write().unwrap() = Some(device_id.clone());
        *self.capture_settings.write().unwrap() = Some(settings);

        // Reset attempts counter
        *self.reconnect_attempts.write().unwrap() = 0;
        self.cancelled.store(false, Ordering::Release);

        // Record monotonic start time for reliable timeout tracking
        // (Instant is monotonic; SystemTime can jump due to NTP)
        *self.wait_started.write().unwrap() = Some(Instant::now());

        // Transition to waiting state (epoch_ms kept for debug/logging purposes)
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        *self.state.write().unwrap() = ReconnectionState::WaitingForDevice {
            started_at_epoch_ms: now_ms,
        };

        // Publish event
        self.event_bus.publish(Event::CameraDisconnected { device_id });
    }

    /// Handle a camera connected event during reconnection
    ///
    /// Checks if the connected device matches our target and
    /// attempts to reconnect if so.
    pub fn on_camera_connected(&self, device_id: &DeviceId, device_name: &str) -> bool {
        // Only relevant if we're waiting for a device
        if !matches!(self.state(), ReconnectionState::WaitingForDevice { .. }) {
            return false;
        }

        // Check if this matches our target device
        let target = self.target_device.read().unwrap();
        if let Some(strategy) = target.as_ref() {
            // For now, just check exact ID (more sophisticated matching can be added)
            if let DeviceMatchStrategy::ExactId(target_id) = strategy {
                if device_id == target_id {
                    tracing::info!(
                        device_id = %device_id.0,
                        "Target device reconnected"
                    );
                    *self.state.write().unwrap() = ReconnectionState::Reconnecting;
                    return true;
                }
            }

            // Fallback to any camera if configured
            if self.config.fallback_to_any_camera {
                tracing::info!(
                    device_id = %device_id.0,
                    device_name = %device_name,
                    "Fallback device found"
                );
                *self.state.write().unwrap() = ReconnectionState::Reconnecting;
                return true;
            }
        }

        false
    }

    /// Attempt to reconnect to the camera
    ///
    /// Returns the negotiated format on success, or an error on failure.
    /// Should be called after on_camera_connected returns true.
    pub fn attempt_reconnect(
        &self,
        manager: &CaptureManager,
        device_id: &DeviceId,
    ) -> Result<NegotiatedFormat, CaptureError> {
        let settings = self.capture_settings.read().unwrap().clone()
            .unwrap_or_default();

        let mut attempts = self.reconnect_attempts.write().unwrap();
        *attempts += 1;

        tracing::info!(
            device_id = %device_id.0,
            attempt = *attempts,
            max = self.config.max_reconnect_attempts,
            "Attempting reconnection"
        );

        // Select the camera
        manager.select_camera(device_id)?;

        // Attempt to start capture
        match manager.start_capture(settings.clone()) {
            Ok(format) => {
                tracing::info!(
                    device_id = %device_id.0,
                    "Reconnection successful"
                );
                *self.state.write().unwrap() = ReconnectionState::Connected;
                *self.wait_started.write().unwrap() = None; // Clear wait timer
                self.event_bus.publish(Event::CameraReconnected { device_id: device_id.clone() });
                Ok(format)
            }
            Err(e) => {
                tracing::warn!(
                    device_id = %device_id.0,
                    error = %e,
                    "Reconnection attempt failed"
                );

                if *attempts >= self.config.max_reconnect_attempts {
                    *self.state.write().unwrap() = ReconnectionState::Failed;
                    *self.wait_started.write().unwrap() = None; // Clear wait timer
                    self.event_bus.publish(Event::CameraReconnectionFailed { device_id: device_id.clone() });
                } else {
                    // Go back to waiting state for another attempt
                    // Keep existing wait_started for cumulative timeout tracking
                    // (epoch_ms for debug/logging only)
                    let now_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    *self.state.write().unwrap() = ReconnectionState::WaitingForDevice {
                        started_at_epoch_ms: now_ms,
                    };
                }
                Err(e)
            }
        }
    }

    /// Check if the reconnection has timed out
    pub fn check_timeout(&self) -> bool {
        if matches!(self.state(), ReconnectionState::WaitingForDevice { .. }) {
            // Use monotonic Instant for reliable timeout (immune to NTP clock adjustments)
            if let Some(started) = *self.wait_started.read().unwrap() {
                let elapsed = started.elapsed();

                if elapsed > self.config.active_poll_duration {
                    tracing::info!("Reconnection timed out after {:?}", elapsed);
                    *self.state.write().unwrap() = ReconnectionState::TimedOut;
                    *self.wait_started.write().unwrap() = None;
                    if let Some(device_id) = self.target_device_id.read().unwrap().clone() {
                        self.event_bus.publish(Event::CameraReconnectionTimedOut { device_id });
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Cancel the current reconnection attempt
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        *self.state.write().unwrap() = ReconnectionState::Connected;
        *self.wait_started.write().unwrap() = None; // Clear wait timer
        tracing::info!("Reconnection cancelled");
    }

    /// Retry reconnection after a timeout or failure
    pub fn retry(&self) {
        if matches!(
            self.state(),
            ReconnectionState::TimedOut | ReconnectionState::Failed
        ) {
            // Reset monotonic timer for timeout tracking
            *self.wait_started.write().unwrap() = Some(Instant::now());

            // Update state (epoch_ms for debug/logging)
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            *self.state.write().unwrap() = ReconnectionState::WaitingForDevice {
                started_at_epoch_ms: now_ms,
            };
            *self.reconnect_attempts.write().unwrap() = 0;
            self.cancelled.store(false, Ordering::Release);

            tracing::info!("Reconnection retry initiated");
        }
    }

    /// Get the poll interval based on current state
    pub fn current_poll_interval(&self) -> Duration {
        if matches!(self.state(), ReconnectionState::WaitingForDevice { .. }) {
            // Use monotonic Instant for reliable timing (immune to NTP adjustments)
            if let Some(started) = *self.wait_started.read().unwrap() {
                if started.elapsed() < self.config.active_poll_duration {
                    self.config.poll_interval
                } else {
                    self.config.background_poll_interval
                }
            } else {
                self.config.poll_interval
            }
        } else {
            self.config.poll_interval
        }
    }
}

// ============================================================================
// Event Handler Integration
// ============================================================================

/// Reconnection-aware camera event handler
///
/// Wraps a ReconnectionManager and processes camera events for reconnection.
pub struct ReconnectionEventHandler {
    manager: Arc<ReconnectionManager>,
}

impl ReconnectionEventHandler {
    pub fn new(manager: Arc<ReconnectionManager>) -> Self {
        Self { manager }
    }
}

impl CameraEventHandler for ReconnectionEventHandler {
    fn on_camera_event(&mut self, event: CameraEvent) {
        match event {
            CameraEvent::Connected(device) => {
                self.manager.on_camera_connected(&device.id, &device.name);
            }
            CameraEvent::Disconnected(_device_id) => {
                // Disconnection handling is done externally with more context
                // (settings, device name, etc.)
            }
            CameraEvent::Changed(_device_id) => {
                // Properties changed, not relevant for reconnection
            }
        }
    }
}

// Note: ReconnectionManager is automatically Send + Sync because:
// - RwLock<T> is Send + Sync when T: Send + Sync
// - AtomicBool is Send + Sync
// - EventBus is Send + Sync
// - All inner types (DeviceMatchStrategy, CaptureSettings, etc.) are Send + Sync
// No unsafe impl needed.

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::EventBus;

    fn make_test_bus() -> EventBus {
        EventBus::new()
    }

    #[test]
    fn test_reconnection_config_default() {
        let config = ReconnectionConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(2));
        assert_eq!(config.active_poll_duration, Duration::from_secs(30));
        assert_eq!(config.max_reconnect_attempts, 3);
    }

    #[test]
    fn test_device_match_strategy_exact_id() {
        let device_id = DeviceId("test-device".into());
        let strategy = DeviceMatchStrategy::ExactId(device_id.clone());

        assert!(strategy.matches(&device_id, "Any Name", &[]));
        assert!(!strategy.matches(&DeviceId("other".into()), "Any Name", &[]));
    }

    #[test]
    fn test_reconnection_state_transitions() {
        let bus = make_test_bus();
        let manager = ReconnectionManager::new(bus);

        // Initially connected
        assert_eq!(manager.state(), ReconnectionState::Connected);
        assert!(!manager.is_reconnecting());

        // Trigger disconnection
        let settings = CaptureSettings::default();
        manager.on_camera_disconnected(
            DeviceId("test".into()),
            "Test Camera".into(),
            settings,
        );

        // Should now be waiting
        assert!(manager.is_reconnecting());
        assert!(matches!(
            manager.state(),
            ReconnectionState::WaitingForDevice { .. }
        ));

        // Cancel
        manager.cancel();
        assert_eq!(manager.state(), ReconnectionState::Connected);
    }

    #[test]
    fn test_on_camera_connected_during_wait() {
        let bus = make_test_bus();
        let manager = ReconnectionManager::new(bus);

        let device_id = DeviceId("test-device".into());
        let settings = CaptureSettings::default();

        // Start reconnection flow
        manager.on_camera_disconnected(
            device_id.clone(),
            "Test Camera".into(),
            settings,
        );

        // Device returns
        let matched = manager.on_camera_connected(&device_id, "Test Camera");
        assert!(matched);
        assert_eq!(manager.state(), ReconnectionState::Reconnecting);
    }

    #[test]
    fn test_on_camera_connected_wrong_device() {
        let bus = make_test_bus();
        let config = ReconnectionConfig {
            fallback_to_any_camera: false,
            ..Default::default()
        };
        let manager = ReconnectionManager::with_config(bus, config);

        let device_id = DeviceId("test-device".into());
        let settings = CaptureSettings::default();

        // Start reconnection flow
        manager.on_camera_disconnected(
            device_id,
            "Test Camera".into(),
            settings,
        );

        // Different device appears
        let matched = manager.on_camera_connected(&DeviceId("other".into()), "Other Camera");
        assert!(!matched);
        assert!(matches!(
            manager.state(),
            ReconnectionState::WaitingForDevice { .. }
        ));
    }

    #[test]
    fn test_retry_after_timeout() {
        let bus = make_test_bus();
        let manager = ReconnectionManager::new(bus);

        // Manually set to timed out
        *manager.state.write().unwrap() = ReconnectionState::TimedOut;

        // Retry
        manager.retry();

        assert!(matches!(
            manager.state(),
            ReconnectionState::WaitingForDevice { .. }
        ));
    }

    #[test]
    fn test_poll_interval_changes() {
        let bus = make_test_bus();
        let config = ReconnectionConfig {
            poll_interval: Duration::from_millis(100),
            background_poll_interval: Duration::from_millis(500),
            active_poll_duration: Duration::from_millis(50),
            ..Default::default()
        };
        let manager = ReconnectionManager::with_config(bus, config);

        // Not waiting, should return poll interval
        assert_eq!(manager.current_poll_interval(), Duration::from_millis(100));

        // Start waiting
        let settings = CaptureSettings::default();
        manager.on_camera_disconnected(
            DeviceId("test".into()),
            "Test".into(),
            settings,
        );

        // Initially active polling
        assert_eq!(manager.current_poll_interval(), Duration::from_millis(100));

        // After active duration, should switch to background
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(manager.current_poll_interval(), Duration::from_millis(500));
    }
}
