//! Camera state management and lifecycle
//!
//! Manages the state machine for camera operations, ensuring clean transitions
//! between states and proper resource management.
//!
//! # Camera States
//!
//! ```text
//! ┌──────────────┐
//! │ Disconnected │ (no device)
//! └──────┬───────┘
//!        │ device_arrived
//!        ▼
//! ┌──────────────┐
//! │   Available  │ (enumerated, not open)
//! └──────┬───────┘
//!        │ open()
//!        ▼
//! ┌──────────────┐
//! │   Opening    │ (format negotiation in progress)
//! └──────┬───────┘
//!        │ success
//!        ▼
//! ┌──────────────┐
//! │    Ready     │ (open, not capturing)
//! └──────┬───────┘
//!        │ start_capture()
//!        ▼
//! ┌──────────────┐     capture_error
//! │  Capturing   │────────────────────►┌───────┐
//! └──────────────┘                     │ Error │
//!        │ stop_capture()              └───┬───┘
//!        ▼                                 │ recover()
//! ┌──────────────┐◄────────────────────────┘
//! │    Ready     │
//! └──────┬───────┘
//!        │ close()
//!        ▼
//! ┌──────────────┐
//! │   Available  │
//! └──────────────┘
//! ```

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::core::{CaptureError, DeviceId, NegotiatedFormat};

// ============================================================================
// Camera State Enum
// ============================================================================

/// State of an individual camera device
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CameraState {
    /// Device is not connected or not detected
    #[default]
    Disconnected,
    /// Device is enumerated and available but not open
    Available,
    /// Device is being opened (format negotiation in progress)
    Opening,
    /// Device is open and ready to capture
    Ready,
    /// Device is actively capturing frames
    Capturing,
    /// Device is in an error state
    Error(CameraErrorInfo),
}

impl CameraState {
    /// Returns true if the camera can accept open() command
    pub fn can_open(&self) -> bool {
        matches!(self, Self::Available | Self::Error(_))
    }

    /// Returns true if the camera can start capturing
    pub fn can_start_capture(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Returns true if the camera is actively capturing
    pub fn is_capturing(&self) -> bool {
        matches!(self, Self::Capturing)
    }

    /// Returns true if the device is in an error state
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Returns true if the device is connected (available or open)
    pub fn is_connected(&self) -> bool {
        !matches!(self, Self::Disconnected)
    }

    /// Returns true if the device has resources allocated (handle, buffers)
    pub fn has_resources(&self) -> bool {
        matches!(self, Self::Opening | Self::Ready | Self::Capturing)
    }

    /// Check if a transition to the target state is valid
    pub fn can_transition_to(&self, target: &CameraState) -> bool {
        matches!(
            (self, target),
            // From Disconnected
            (Self::Disconnected, Self::Available)
                // From Available
                | (Self::Available, Self::Opening)
                | (Self::Available, Self::Disconnected)
                // From Opening
                | (Self::Opening, Self::Ready)
                | (Self::Opening, Self::Error(_))
                | (Self::Opening, Self::Available) // Cancelled/failed
                // From Ready
                | (Self::Ready, Self::Capturing)
                | (Self::Ready, Self::Available) // close()
                | (Self::Ready, Self::Error(_))
                | (Self::Ready, Self::Disconnected) // Sudden disconnect
                // From Capturing
                | (Self::Capturing, Self::Ready) // stop_capture()
                | (Self::Capturing, Self::Error(_))
                | (Self::Capturing, Self::Disconnected) // Sudden disconnect
                // From Error
                | (Self::Error(_), Self::Available) // recover/reset
                | (Self::Error(_), Self::Opening) // retry open
                | (Self::Error(_), Self::Disconnected) // Device removed
        )
    }

    /// Get a human-readable description of the state
    pub fn description(&self) -> &'static str {
        match self {
            Self::Disconnected => "Camera not connected",
            Self::Available => "Camera available",
            Self::Opening => "Opening camera...",
            Self::Ready => "Camera ready",
            Self::Capturing => "Capturing",
            Self::Error(_) => "Error",
        }
    }
}

impl fmt::Display for CameraState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Available => write!(f, "Available"),
            Self::Opening => write!(f, "Opening"),
            Self::Ready => write!(f, "Ready"),
            Self::Capturing => write!(f, "Capturing"),
            Self::Error(info) => write!(f, "Error: {}", info.message),
        }
    }
}

// ============================================================================
// Error Information
// ============================================================================

/// Information about a camera error
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraErrorInfo {
    /// Error message
    pub message: String,
    /// Whether recovery should be attempted automatically
    pub recoverable: bool,
    /// Number of recovery attempts made
    pub recovery_attempts: u32,
    /// Maximum recovery attempts before giving up
    pub max_recovery_attempts: u32,
}

impl CameraErrorInfo {
    pub fn new(message: impl Into<String>, recoverable: bool) -> Self {
        Self {
            message: message.into(),
            recoverable,
            recovery_attempts: 0,
            max_recovery_attempts: 5,
        }
    }

    pub fn from_capture_error(err: &CaptureError) -> Self {
        let recoverable = matches!(err, CaptureError::Timeout(_) | CaptureError::Disconnected);
        Self::new(err.to_string(), recoverable)
    }

    /// Increment recovery attempt count, returns true if more attempts allowed
    pub fn attempt_recovery(&mut self) -> bool {
        if !self.recoverable {
            return false;
        }
        self.recovery_attempts += 1;
        self.recovery_attempts <= self.max_recovery_attempts
    }
}

// ============================================================================
// State Transition
// ============================================================================

/// A recorded state transition
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// Previous state
    pub from: CameraState,
    /// New state
    pub to: CameraState,
    /// Monotonic timestamp
    pub timestamp: Instant,
    /// Reason for transition
    pub reason: TransitionReason,
}

/// Reason for a state transition
#[derive(Debug, Clone)]
pub enum TransitionReason {
    /// Device was detected
    DeviceArrived,
    /// Device was removed
    DeviceRemoved,
    /// User requested open
    OpenRequested,
    /// Format negotiation succeeded
    OpenSucceeded,
    /// Format negotiation failed
    OpenFailed(String),
    /// User requested capture start
    CaptureStarted,
    /// User requested capture stop
    CaptureStopped,
    /// User requested close
    CloseRequested,
    /// An error occurred
    Error(String),
    /// Recovery was attempted
    RecoveryAttempted,
    /// State was reset
    Reset,
}

impl fmt::Display for TransitionReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceArrived => write!(f, "device arrived"),
            Self::DeviceRemoved => write!(f, "device removed"),
            Self::OpenRequested => write!(f, "open requested"),
            Self::OpenSucceeded => write!(f, "open succeeded"),
            Self::OpenFailed(msg) => write!(f, "open failed: {}", msg),
            Self::CaptureStarted => write!(f, "capture started"),
            Self::CaptureStopped => write!(f, "capture stopped"),
            Self::CloseRequested => write!(f, "close requested"),
            Self::Error(msg) => write!(f, "error: {}", msg),
            Self::RecoveryAttempted => write!(f, "recovery attempted"),
            Self::Reset => write!(f, "reset"),
        }
    }
}

// ============================================================================
// Camera State Manager
// ============================================================================

/// Manages the lifecycle and state of a camera device
///
/// This struct is thread-safe and can be shared across threads.
pub struct CameraStateManager {
    /// Device ID this manager is tracking
    device_id: DeviceId,
    /// Current state (protected by RwLock for thread safety)
    state: RwLock<CameraState>,
    /// Negotiated format when open
    format: RwLock<Option<NegotiatedFormat>>,
    /// State transition history (limited size)
    history: RwLock<Vec<StateTransition>>,
    /// Maximum history entries to keep
    max_history: usize,
    /// Transition counter for debugging
    transition_count: AtomicU64,
    /// Callback for state changes
    /// Uses Arc to allow cloning the callback out of the lock before invoking,
    /// which prevents deadlock if the callback tries to modify state.
    #[allow(clippy::type_complexity)]
    on_state_change: RwLock<Option<Arc<dyn Fn(&StateTransition) + Send + Sync>>>,
}

impl CameraStateManager {
    /// Create a new state manager for a device
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            state: RwLock::new(CameraState::Disconnected),
            format: RwLock::new(None),
            history: RwLock::new(Vec::with_capacity(100)),
            max_history: 100,
            transition_count: AtomicU64::new(0),
            on_state_change: RwLock::new(None),
        }
    }

    /// Create a new state manager starting in Available state
    pub fn new_available(device_id: DeviceId) -> Self {
        Self {
            device_id,
            state: RwLock::new(CameraState::Available),
            format: RwLock::new(None),
            history: RwLock::new(Vec::with_capacity(100)),
            max_history: 100,
            transition_count: AtomicU64::new(0),
            on_state_change: RwLock::new(None),
        }
    }

    /// Get the device ID
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Get the current state
    pub fn state(&self) -> CameraState {
        self.state.read().unwrap().clone()
    }

    /// Get the negotiated format (if open)
    pub fn format(&self) -> Option<NegotiatedFormat> {
        self.format.read().unwrap().clone()
    }

    /// Get the number of state transitions
    pub fn transition_count(&self) -> u64 {
        self.transition_count.load(Ordering::Relaxed)
    }

    /// Set a callback to be called on state changes
    ///
    /// Note: The callback is invoked outside of any locks, so it is safe for
    /// the callback to call methods on this CameraStateManager (including
    /// set_on_state_change itself).
    pub fn set_on_state_change<F>(&self, callback: F)
    where
        F: Fn(&StateTransition) + Send + Sync + 'static,
    {
        *self.on_state_change.write().unwrap() = Some(Arc::new(callback));
    }

    /// Get recent state transition history
    pub fn history(&self) -> Vec<StateTransition> {
        self.history.read().unwrap().clone()
    }

    /// Internal transition method
    fn transition(
        &self,
        new_state: CameraState,
        reason: TransitionReason,
    ) -> Result<(), CaptureError> {
        let mut state = self.state.write().unwrap();
        let old_state = state.clone();

        // Validate transition
        if !old_state.can_transition_to(&new_state) {
            return Err(CaptureError::Platform(format!(
                "Invalid state transition: {} -> {} (reason: {})",
                old_state, new_state, reason
            )));
        }

        // Perform the transition
        *state = new_state.clone();
        drop(state);

        // Record in history
        let transition = StateTransition {
            from: old_state.clone(),
            to: new_state.clone(),
            timestamp: Instant::now(),
            reason,
        };

        let mut history = self.history.write().unwrap();
        if history.len() >= self.max_history {
            history.remove(0);
        }
        history.push(transition.clone());
        drop(history);

        // Update counter
        self.transition_count.fetch_add(1, Ordering::Relaxed);

        // Fire callback - clone the Arc out of the lock to avoid potential deadlock
        // if the callback tries to modify state. This is safe because we clone the
        // Arc reference, then drop the lock before invoking the callback.
        let callback_clone = self.on_state_change.read().unwrap().clone();
        if let Some(ref callback) = callback_clone {
            callback(&transition);
        }

        // Log transition
        tracing::info!(
            device = %self.device_id.0,
            from = %old_state,
            to = %new_state,
            "Camera state transition"
        );

        Ok(())
    }

    // ========================================================================
    // State Transition Methods
    // ========================================================================

    /// Mark device as arrived/available
    pub fn device_arrived(&self) -> Result<(), CaptureError> {
        self.transition(CameraState::Available, TransitionReason::DeviceArrived)
    }

    /// Mark device as disconnected
    pub fn device_removed(&self) -> Result<(), CaptureError> {
        // Clear format on disconnect
        *self.format.write().unwrap() = None;
        self.transition(CameraState::Disconnected, TransitionReason::DeviceRemoved)
    }

    /// Begin opening the device
    pub fn begin_open(&self) -> Result<(), CaptureError> {
        let state = self.state();
        if !state.can_open() {
            return Err(CaptureError::Platform(format!(
                "Cannot open camera in state: {}",
                state
            )));
        }
        self.transition(CameraState::Opening, TransitionReason::OpenRequested)
    }

    /// Complete opening with negotiated format
    pub fn open_succeeded(&self, format: NegotiatedFormat) -> Result<(), CaptureError> {
        *self.format.write().unwrap() = Some(format);
        self.transition(CameraState::Ready, TransitionReason::OpenSucceeded)
    }

    /// Mark open as failed
    pub fn open_failed(&self, error: &CaptureError) -> Result<(), CaptureError> {
        let error_info = CameraErrorInfo::from_capture_error(error);
        self.transition(
            CameraState::Error(error_info),
            TransitionReason::OpenFailed(error.to_string()),
        )
    }

    /// Start capturing
    pub fn start_capture(&self) -> Result<(), CaptureError> {
        let state = self.state();
        if !state.can_start_capture() {
            return Err(CaptureError::Platform(format!(
                "Cannot start capture in state: {}",
                state
            )));
        }
        self.transition(CameraState::Capturing, TransitionReason::CaptureStarted)
    }

    /// Stop capturing
    pub fn stop_capture(&self) -> Result<(), CaptureError> {
        if !self.state().is_capturing() {
            return Err(CaptureError::Platform(
                "Cannot stop capture: not capturing".into(),
            ));
        }
        self.transition(CameraState::Ready, TransitionReason::CaptureStopped)
    }

    /// Close the device
    pub fn close(&self) -> Result<(), CaptureError> {
        let state = self.state();
        if !state.has_resources() {
            // Already closed, that's fine
            return Ok(());
        }

        // If capturing, stop first
        if state.is_capturing() {
            self.transition(CameraState::Ready, TransitionReason::CaptureStopped)?;
        }

        // Clear format
        *self.format.write().unwrap() = None;
        self.transition(CameraState::Available, TransitionReason::CloseRequested)
    }

    /// Report an error
    pub fn report_error(&self, error: &CaptureError) -> Result<(), CaptureError> {
        let error_info = CameraErrorInfo::from_capture_error(error);
        self.transition(
            CameraState::Error(error_info),
            TransitionReason::Error(error.to_string()),
        )
    }

    /// Attempt recovery from error state
    pub fn attempt_recovery(&self) -> Result<bool, CaptureError> {
        let mut state = self.state.write().unwrap();

        if let CameraState::Error(ref mut info) = *state {
            if info.attempt_recovery() {
                drop(state);
                // Transition back to Available to allow retry
                self.transition(CameraState::Available, TransitionReason::RecoveryAttempted)?;
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Reset to initial state (Disconnected or Available based on device presence)
    pub fn reset(&self, device_present: bool) {
        *self.format.write().unwrap() = None;
        let new_state = if device_present {
            CameraState::Available
        } else {
            CameraState::Disconnected
        };
        let _ = self.transition(new_state, TransitionReason::Reset);
    }
}

impl fmt::Debug for CameraStateManager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CameraStateManager")
            .field("device_id", &self.device_id)
            .field("state", &self.state())
            .field("format", &self.format())
            .field("transition_count", &self.transition_count())
            .finish()
    }
}

// ============================================================================
// Thread-Safe Wrapper
// ============================================================================

/// A thread-safe, clonable handle to a CameraStateManager
pub type SharedCameraState = Arc<CameraStateManager>;

/// Create a new shared camera state manager
pub fn shared_camera_state(device_id: DeviceId) -> SharedCameraState {
    Arc::new(CameraStateManager::new(device_id))
}

/// Create a new shared camera state manager starting in Available state
pub fn shared_camera_state_available(device_id: DeviceId) -> SharedCameraState {
    Arc::new(CameraStateManager::new_available(device_id))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn test_initial_state_disconnected() {
        let manager = CameraStateManager::new(DeviceId("test".into()));
        assert_eq!(manager.state(), CameraState::Disconnected);
    }

    #[test]
    fn test_initial_state_available() {
        let manager = CameraStateManager::new_available(DeviceId("test".into()));
        assert_eq!(manager.state(), CameraState::Available);
    }

    #[test]
    fn test_device_arrival() {
        let manager = CameraStateManager::new(DeviceId("test".into()));
        assert!(manager.device_arrived().is_ok());
        assert_eq!(manager.state(), CameraState::Available);
    }

    #[test]
    fn test_full_lifecycle() {
        let manager = CameraStateManager::new_available(DeviceId("test".into()));

        // Open
        assert!(manager.begin_open().is_ok());
        assert_eq!(manager.state(), CameraState::Opening);

        // Open succeeded
        let format = NegotiatedFormat {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: crate::core::PixelFormat::Mjpeg,
            exact_match: true,
        };
        assert!(manager.open_succeeded(format.clone()).is_ok());
        assert_eq!(manager.state(), CameraState::Ready);
        assert!(manager.format().is_some());

        // Start capture
        assert!(manager.start_capture().is_ok());
        assert_eq!(manager.state(), CameraState::Capturing);

        // Stop capture
        assert!(manager.stop_capture().is_ok());
        assert_eq!(manager.state(), CameraState::Ready);

        // Close
        assert!(manager.close().is_ok());
        assert_eq!(manager.state(), CameraState::Available);
        assert!(manager.format().is_none());
    }

    #[test]
    fn test_invalid_transition() {
        let manager = CameraStateManager::new(DeviceId("test".into()));
        // Try to start capture from Disconnected - should fail
        assert!(manager.start_capture().is_err());
    }

    #[test]
    fn test_error_and_recovery() {
        let manager = CameraStateManager::new_available(DeviceId("test".into()));
        manager.begin_open().unwrap();

        // Simulate open failure
        let err = CaptureError::Timeout(1000);
        manager.open_failed(&err).unwrap();
        assert!(manager.state().is_error());

        // Attempt recovery
        let can_recover = manager.attempt_recovery().unwrap();
        assert!(can_recover);
        assert_eq!(manager.state(), CameraState::Available);
    }

    #[test]
    fn test_state_callback() {
        let manager = CameraStateManager::new(DeviceId("test".into()));
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        manager.set_on_state_change(move |transition| {
            called_clone.store(true, Ordering::SeqCst);
            assert_eq!(transition.from, CameraState::Disconnected);
            assert_eq!(transition.to, CameraState::Available);
        });

        manager.device_arrived().unwrap();
        assert!(called.load(Ordering::SeqCst));
    }

    #[test]
    fn test_transition_history() {
        let manager = CameraStateManager::new(DeviceId("test".into()));
        manager.device_arrived().unwrap();
        manager.begin_open().unwrap();

        let history = manager.history();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].from, CameraState::Disconnected);
        assert_eq!(history[0].to, CameraState::Available);
        assert_eq!(history[1].from, CameraState::Available);
        assert_eq!(history[1].to, CameraState::Opening);
    }

    #[test]
    fn test_sudden_disconnect() {
        let manager = CameraStateManager::new_available(DeviceId("test".into()));
        manager.begin_open().unwrap();
        manager
            .open_succeeded(NegotiatedFormat {
                width: 640,
                height: 480,
                framerate: 30.0,
                format: crate::core::PixelFormat::Mjpeg,
                exact_match: true,
            })
            .unwrap();
        manager.start_capture().unwrap();

        // Sudden disconnect while capturing
        assert!(manager.device_removed().is_ok());
        assert_eq!(manager.state(), CameraState::Disconnected);
        assert!(manager.format().is_none());
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let manager = shared_camera_state_available(DeviceId("test".into()));
        let manager_clone = manager.clone();

        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let _ = manager_clone.state();
            }
        });

        for _ in 0..100 {
            let _ = manager.state();
        }

        handle.join().unwrap();
    }

    #[test]
    fn test_can_open_states() {
        assert!(CameraState::Available.can_open());
        assert!(CameraState::Error(CameraErrorInfo::new("test", true)).can_open());
        assert!(!CameraState::Disconnected.can_open());
        assert!(!CameraState::Capturing.can_open());
    }

    #[test]
    fn test_state_display() {
        assert_eq!(format!("{}", CameraState::Disconnected), "Disconnected");
        assert_eq!(format!("{}", CameraState::Available), "Available");
        assert_eq!(format!("{}", CameraState::Capturing), "Capturing");
    }

    #[test]
    fn test_error_info_recovery_limit() {
        let mut info = CameraErrorInfo::new("test", true);
        info.max_recovery_attempts = 2;

        assert!(info.attempt_recovery()); // 1
        assert!(info.attempt_recovery()); // 2
        assert!(!info.attempt_recovery()); // 3 - exceeded
    }
}
