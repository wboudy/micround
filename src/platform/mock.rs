//! Mock implementations for testing
//!
//! These implementations allow testing core application logic without
//! platform dependencies.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::platform::display::*;
use crate::platform::window::*;
use crate::platform::system::*;

// ============================================================================
// Mock Display Provider
// ============================================================================

/// Mock display provider for testing
#[derive(Debug, Default)]
pub struct MockDisplayProvider {
    displays: Vec<DisplayInfo>,
}

impl MockDisplayProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mock display
    pub fn add_display(&mut self, info: DisplayInfo) {
        self.displays.push(info);
    }

    /// Create with a default primary display
    pub fn with_primary(mut self, width: u32, height: u32) -> Self {
        self.displays.push(DisplayInfo {
            handle: DisplayHandle::new(1),
            name: "Primary Display".into(),
            bounds: Rect::new(0, 0, width, height),
            work_area: Rect::new(0, 0, width, height - 40), // Fake taskbar
            dpi: DpiScale::default(),
            is_primary: true,
            refresh_rate: Some(60.0),
        });
        self
    }

    /// Create with multiple displays
    pub fn with_displays(mut self, displays: Vec<DisplayInfo>) -> Self {
        self.displays = displays;
        self
    }
}

impl DisplayProvider for MockDisplayProvider {
    fn enumerate(&self) -> Result<Vec<DisplayInfo>, DisplayError> {
        Ok(self.displays.clone())
    }

    fn get(&self, handle: &DisplayHandle) -> Result<DisplayInfo, DisplayError> {
        self.displays
            .iter()
            .find(|d| d.handle == *handle)
            .cloned()
            .ok_or(DisplayError::NotFound(handle.clone()))
    }

    fn primary(&self) -> Result<DisplayInfo, DisplayError> {
        self.displays
            .iter()
            .find(|d| d.is_primary)
            .cloned()
            .ok_or(DisplayError::EnumerationFailed("No primary display".into()))
    }

    fn refresh(&mut self) -> Result<(), DisplayError> {
        Ok(())
    }
}

// ============================================================================
// Mock Desktop Window
// ============================================================================

static NEXT_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

/// Mock window state
#[derive(Debug, Clone)]
struct MockWindowState {
    bounds: Rect,
    visible: bool,
    surface_info: SurfaceInfo,
    last_frame: Option<Vec<u8>>,
}

/// Mock desktop window for testing
#[derive(Debug, Default)]
pub struct MockDesktopWindow {
    windows: HashMap<u64, MockWindowState>,
}

impl MockDesktopWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the last rendered frame for a window
    pub fn last_frame(&self, handle: &WindowHandle) -> Option<&[u8]> {
        self.windows
            .get(&handle.raw())
            .and_then(|w| w.last_frame.as_deref())
    }

    /// Get the number of windows created
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }
}

impl DesktopWindow for MockDesktopWindow {
    fn create(&mut self, config: WindowConfig) -> Result<WindowHandle, WindowError> {
        let id = NEXT_WINDOW_ID.fetch_add(1, Ordering::SeqCst);
        let bounds = config.bounds.unwrap_or(Rect::new(0, 0, 1920, 1080));

        let state = MockWindowState {
            bounds,
            visible: config.visible,
            surface_info: SurfaceInfo {
                size: bounds.size,
                format: SurfaceFormat::Rgba8,
                stride: bounds.width() as usize * 4,
            },
            last_frame: None,
        };

        self.windows.insert(id, state);
        Ok(WindowHandle::new(id))
    }

    fn destroy(&mut self, handle: &WindowHandle) -> Result<(), WindowError> {
        self.windows
            .remove(&handle.raw())
            .map(|_| ())
            .ok_or(WindowError::NotFound)
    }

    fn set_visible(&mut self, handle: &WindowHandle, visible: bool) -> Result<(), WindowError> {
        self.windows
            .get_mut(&handle.raw())
            .map(|w| w.visible = visible)
            .ok_or(WindowError::NotFound)
    }

    fn set_bounds(&mut self, handle: &WindowHandle, bounds: Rect) -> Result<(), WindowError> {
        if let Some(window) = self.windows.get_mut(&handle.raw()) {
            window.bounds = bounds;
            window.surface_info.size = bounds.size;
            window.surface_info.stride = bounds.width() as usize * 4;
            Ok(())
        } else {
            Err(WindowError::NotFound)
        }
    }

    fn get_bounds(&self, handle: &WindowHandle) -> Result<Rect, WindowError> {
        self.windows
            .get(&handle.raw())
            .map(|w| w.bounds)
            .ok_or(WindowError::NotFound)
    }

    fn surface_info(&self, handle: &WindowHandle) -> Result<SurfaceInfo, WindowError> {
        self.windows
            .get(&handle.raw())
            .map(|w| w.surface_info.clone())
            .ok_or(WindowError::NotFound)
    }

    fn present(&mut self, handle: &WindowHandle, data: &[u8]) -> Result<(), WindowError> {
        let window = self.windows.get_mut(&handle.raw()).ok_or(WindowError::NotFound)?;

        let expected = window.surface_info.buffer_size();
        if data.len() != expected {
            return Err(WindowError::InvalidData {
                expected,
                actual: data.len(),
            });
        }

        window.last_frame = Some(data.to_vec());
        Ok(())
    }

    fn is_visible(&self, handle: &WindowHandle) -> Result<bool, WindowError> {
        self.windows
            .get(&handle.raw())
            .map(|w| w.visible)
            .ok_or(WindowError::NotFound)
    }

    fn invalidate(&mut self, handle: &WindowHandle) -> Result<(), WindowError> {
        self.windows
            .get(&handle.raw())
            .map(|_| ())
            .ok_or(WindowError::NotFound)
    }
}

// ============================================================================
// Mock System Monitor
// ============================================================================

/// Mock system monitor for testing
#[derive(Debug)]
pub struct MockSystemMonitor {
    running: bool,
    power_state: PowerState,
    thermal_state: ThermalState,
}

impl Default for MockSystemMonitor {
    fn default() -> Self {
        Self {
            running: false,
            power_state: PowerState::AcPower,
            thermal_state: ThermalState::Normal,
        }
    }
}

impl MockSystemMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the mock power state
    pub fn set_power_state(&mut self, state: PowerState) {
        self.power_state = state;
    }

    /// Set the mock thermal state
    pub fn set_thermal_state(&mut self, state: ThermalState) {
        self.thermal_state = state;
    }

    /// Create configured for low battery testing
    pub fn with_low_battery(mut self) -> Self {
        self.power_state = PowerState::Battery {
            percent: 10,
            time_remaining: None,
        };
        self
    }

    /// Create configured for thermal throttling testing
    pub fn with_thermal_throttle(mut self) -> Self {
        self.thermal_state = ThermalState::Serious;
        self
    }
}

impl SystemMonitor for MockSystemMonitor {
    fn start(&mut self) -> Result<(), SystemError> {
        self.running = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), SystemError> {
        self.running = false;
        Ok(())
    }

    fn power_state(&self) -> Result<PowerState, SystemError> {
        Ok(self.power_state)
    }

    fn thermal_state(&self) -> Result<ThermalState, SystemError> {
        Ok(self.thermal_state)
    }

    fn prevent_sleep(&self, _reason: &str) -> Result<SleepPrevention, SystemError> {
        Ok(SleepPrevention::noop())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_display_provider() {
        let provider = MockDisplayProvider::new().with_primary(1920, 1080);

        let displays = provider.enumerate().unwrap();
        assert_eq!(displays.len(), 1);

        let primary = provider.primary().unwrap();
        assert_eq!(primary.bounds.width(), 1920);
        assert!(primary.is_primary);
    }

    #[test]
    fn test_mock_desktop_window() {
        let mut window = MockDesktopWindow::new();

        let handle = window
            .create(WindowConfig::new(DisplayHandle::new(1)))
            .unwrap();

        assert!(window.is_visible(&handle).unwrap());

        window.set_visible(&handle, false).unwrap();
        assert!(!window.is_visible(&handle).unwrap());

        // Present a frame
        let info = window.surface_info(&handle).unwrap();
        let frame = vec![0u8; info.buffer_size()];
        window.present(&handle, &frame).unwrap();

        assert!(window.last_frame(&handle).is_some());

        window.destroy(&handle).unwrap();
        assert!(window.is_visible(&handle).is_err());
    }

    #[test]
    fn test_mock_system_monitor() {
        let mut monitor = MockSystemMonitor::new();

        assert!(!monitor.should_reduce_activity());

        monitor.set_power_state(PowerState::Battery {
            percent: 15,
            time_remaining: None,
        });
        assert!(monitor.should_reduce_activity());

        monitor.set_power_state(PowerState::AcPower);
        monitor.set_thermal_state(ThermalState::Critical);
        assert!(monitor.should_reduce_activity());
    }
}
