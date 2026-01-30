//! System events abstraction
//!
//! Provides platform-independent handling of system-level events like
//! sleep/wake, power state changes, and session events.
#![allow(dead_code)] // System events infrastructure
//!
//! # Platform Implementations
//! - Windows: Win32 WM_POWERBROADCAST, WM_WTSSESSION_CHANGE
//! - macOS: NSWorkspace notifications, IOKit power assertions
//! - Linux: systemd-logind D-Bus, upower

use std::time::Duration;

/// System power state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// Running on AC power
    AcPower,
    /// Running on battery
    Battery {
        /// Percentage remaining (0-100)
        percent: u8,
        /// Estimated time remaining
        time_remaining: Option<Duration>,
    },
    /// Unknown power state
    Unknown,
}

impl PowerState {
    /// Check if running on battery
    pub fn is_on_battery(&self) -> bool {
        matches!(self, Self::Battery { .. })
    }

    /// Check if battery is low (below 20%)
    pub fn is_low_battery(&self) -> bool {
        matches!(self, Self::Battery { percent, .. } if *percent < 20)
    }
}

/// System sleep/wake events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepEvent {
    /// System is about to sleep
    WillSleep,
    /// System has resumed from sleep
    DidWake,
    /// Sleep was cancelled (user activity)
    SleepCancelled,
}

/// User session events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// Screen is being locked
    ScreenLocking,
    /// Screen was unlocked
    ScreenUnlocked,
    /// User is logging out
    LoggingOut,
    /// Fast user switching: another user is activating
    SwitchingUser,
    /// Session was reactivated (switched back to)
    SessionActivated,
}

/// Thermal state (for throttling)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalState {
    /// Normal operation
    Normal,
    /// System is warm, consider reducing work
    Fair,
    /// System is hot, should reduce work
    Serious,
    /// System is critically hot, must reduce work
    Critical,
}

impl ThermalState {
    /// Check if thermal throttling is recommended
    pub fn should_throttle(&self) -> bool {
        matches!(self, Self::Fair | Self::Serious | Self::Critical)
    }

    /// Get recommended frame rate multiplier (1.0 = full rate)
    pub fn frame_rate_multiplier(&self) -> f32 {
        match self {
            Self::Normal => 1.0,
            Self::Fair => 0.75,
            Self::Serious => 0.5,
            Self::Critical => 0.25,
        }
    }
}

/// Combined system event
#[derive(Debug, Clone)]
pub enum SystemEvent {
    /// Power state changed
    PowerStateChanged(PowerState),
    /// Sleep/wake event
    Sleep(SleepEvent),
    /// User session event
    Session(SessionEvent),
    /// Thermal state changed
    ThermalStateChanged(ThermalState),
    /// System time changed significantly (NTP sync, manual change)
    TimeChanged,
}

/// Error type for system operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum SystemError {
    #[error("Failed to register for notifications: {0}")]
    RegistrationFailed(String),

    #[error("Failed to query system state: {0}")]
    QueryFailed(String),

    #[error("Platform not supported")]
    Unsupported,

    #[error("Platform error: {0}")]
    Platform(String),
}

/// Trait for receiving system events
pub trait SystemEventHandler: Send {
    /// Called when a system event occurs
    fn on_system_event(&mut self, event: SystemEvent);
}

/// Trait for monitoring system state and events
///
/// Platform implementations register for OS-level notifications
/// and translate them to our event types.
pub trait SystemMonitor: Send {
    /// Start monitoring for system events
    fn start(&mut self) -> Result<(), SystemError>;

    /// Stop monitoring
    fn stop(&mut self) -> Result<(), SystemError>;

    /// Get current power state
    fn power_state(&self) -> Result<PowerState, SystemError>;

    /// Get current thermal state (if available)
    fn thermal_state(&self) -> Result<ThermalState, SystemError>;

    /// Prevent system sleep while active
    ///
    /// Returns a guard that releases the assertion when dropped.
    fn prevent_sleep(&self, reason: &str) -> Result<SleepPrevention, SystemError>;

    /// Check if the system is in a state where we should reduce activity
    ///
    /// Returns true if on low battery, thermally throttled, or similar.
    fn should_reduce_activity(&self) -> bool {
        if let Ok(power) = self.power_state() {
            if power.is_low_battery() {
                return true;
            }
        }
        if let Ok(thermal) = self.thermal_state() {
            if thermal.should_throttle() {
                return true;
            }
        }
        false
    }
}

// Platform-specific implementations
#[cfg(target_os = "windows")]
mod system_windows;
#[cfg(target_os = "windows")]
pub use system_windows::WindowsSystemMonitor;

/// Guard that prevents system sleep while held
///
/// When dropped, releases the sleep prevention assertion.
pub struct SleepPrevention {
    /// Platform-specific identifier for this assertion
    _id: u64,
    /// Callback to release the assertion
    release: Option<Box<dyn FnOnce() + Send>>,
}

impl SleepPrevention {
    /// Create a new sleep prevention guard
    pub fn new(id: u64, release: impl FnOnce() + Send + 'static) -> Self {
        Self {
            _id: id,
            release: Some(Box::new(release)),
        }
    }

    /// Create a no-op guard (for platforms that don't support this)
    pub fn noop() -> Self {
        Self {
            _id: 0,
            release: None,
        }
    }
}

impl Drop for SleepPrevention {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            release();
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_state_battery() {
        let ac = PowerState::AcPower;
        assert!(!ac.is_on_battery());
        assert!(!ac.is_low_battery());

        let battery_ok = PowerState::Battery {
            percent: 50,
            time_remaining: Some(Duration::from_secs(3600)),
        };
        assert!(battery_ok.is_on_battery());
        assert!(!battery_ok.is_low_battery());

        let battery_low = PowerState::Battery {
            percent: 15,
            time_remaining: Some(Duration::from_secs(900)),
        };
        assert!(battery_low.is_on_battery());
        assert!(battery_low.is_low_battery());
    }

    #[test]
    fn test_thermal_state_throttling() {
        assert!(!ThermalState::Normal.should_throttle());
        assert!(ThermalState::Fair.should_throttle());
        assert!(ThermalState::Serious.should_throttle());
        assert!(ThermalState::Critical.should_throttle());

        assert_eq!(ThermalState::Normal.frame_rate_multiplier(), 1.0);
        assert_eq!(ThermalState::Critical.frame_rate_multiplier(), 0.25);
    }

    #[test]
    fn test_sleep_prevention_drop() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let released = Arc::new(AtomicBool::new(false));
        let released_clone = released.clone();

        {
            let _guard = SleepPrevention::new(1, move || {
                released_clone.store(true, Ordering::SeqCst);
            });
            assert!(!released.load(Ordering::SeqCst));
        }

        assert!(released.load(Ordering::SeqCst));
    }
}
