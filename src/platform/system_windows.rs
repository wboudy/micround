//! Windows system event monitor
//!
//! Implements sleep/wake and session notifications using a hidden message window.
//!
//! # Status
//!
//! This module is a placeholder. The Windows API bindings have changed in recent
//! versions of the `windows` crate and require updates. Full implementation is
//! tracked as a separate work item.
#![cfg(target_os = "windows")]

use super::{PowerState, SleepPrevention, SystemError, SystemMonitor, ThermalState};

/// Windows system event monitor (placeholder)
///
/// Note: This is currently a placeholder implementation. The full Windows-based
/// monitoring requires updates to work with recent Windows API binding changes.
pub struct WindowsSystemMonitor {
    _placeholder: (),
}

impl WindowsSystemMonitor {
    pub fn new(_handler: Box<dyn super::SystemEventHandler>) -> Self {
        tracing::info!("Windows system monitor initialized (placeholder mode)");
        Self { _placeholder: () }
    }
}

impl SystemMonitor for WindowsSystemMonitor {
    fn start(&mut self) -> Result<(), SystemError> {
        // Placeholder: Nothing to start
        Ok(())
    }

    fn stop(&mut self) -> Result<(), SystemError> {
        // Placeholder: Nothing to stop
        Ok(())
    }

    fn power_state(&self) -> Result<PowerState, SystemError> {
        Err(SystemError::Unsupported)
    }

    fn thermal_state(&self) -> Result<ThermalState, SystemError> {
        Err(SystemError::Unsupported)
    }

    fn prevent_sleep(&self, _reason: &str) -> Result<SleepPrevention, SystemError> {
        Ok(SleepPrevention::noop())
    }
}
