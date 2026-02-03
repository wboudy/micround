//! Display/monitor abstraction
//!
//! Provides platform-independent types and traits for working with displays/monitors.
#![allow(dead_code)] // Complete API for cross-platform display handling
//!
//! # Platform Implementations
//! - Windows: DXGI/Win32 EnumDisplayMonitors
//! - macOS: CGDisplay / NSScreen
//! - Linux: X11 RANDR / Wayland output protocol

use std::fmt;

/// Unique identifier for a display
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisplayHandle(pub(crate) u64);

impl DisplayHandle {
    /// Create a new display handle from a platform-specific ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw platform ID
    pub fn raw(&self) -> u64 {
        self.0
    }
}

impl fmt::Display for DisplayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Display({})", self.0)
    }
}

/// Physical position on the virtual desktop
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Physical size in pixels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// Calculate area in pixels
    pub fn area(&self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

/// Rectangle defined by position and size
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn x(&self) -> i32 {
        self.origin.x
    }

    pub fn y(&self) -> i32 {
        self.origin.y
    }

    pub fn width(&self) -> u32 {
        self.size.width
    }

    pub fn height(&self) -> u32 {
        self.size.height
    }

    /// Check if this rect contains a point
    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.x < self.origin.x + self.size.width as i32
            && point.y >= self.origin.y
            && point.y < self.origin.y + self.size.height as i32
    }
}

/// DPI scaling information
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiScale {
    /// Horizontal scale factor (1.0 = 100% = 96 DPI on Windows)
    pub x: f64,
    /// Vertical scale factor
    pub y: f64,
}

impl Default for DpiScale {
    fn default() -> Self {
        Self { x: 1.0, y: 1.0 }
    }
}

impl DpiScale {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Create uniform scale
    pub fn uniform(scale: f64) -> Self {
        Self { x: scale, y: scale }
    }

    /// Check if scaling is applied (not 1.0)
    pub fn is_scaled(&self) -> bool {
        (self.x - 1.0).abs() > f64::EPSILON || (self.y - 1.0).abs() > f64::EPSILON
    }
}

/// Information about a connected display
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Unique handle for this display
    pub handle: DisplayHandle,

    /// Human-readable name (e.g., "Dell U2720Q" or "Display 1")
    pub name: String,

    /// Bounds in virtual desktop coordinates (may be negative for multi-monitor)
    pub bounds: Rect,

    /// Work area (bounds minus taskbar/dock)
    pub work_area: Rect,

    /// DPI scale factor
    pub dpi: DpiScale,

    /// Whether this is the primary display
    pub is_primary: bool,

    /// Refresh rate in Hz (if known)
    pub refresh_rate: Option<f64>,
}

impl DisplayInfo {
    /// Get physical pixel resolution
    pub fn resolution(&self) -> Size {
        self.bounds.size
    }

    /// Get effective (scaled) resolution
    pub fn scaled_resolution(&self) -> Size {
        Size::new(
            (self.bounds.width() as f64 / self.dpi.x) as u32,
            (self.bounds.height() as f64 / self.dpi.y) as u32,
        )
    }
}

/// Event indicating a display configuration change
#[derive(Debug, Clone)]
pub enum DisplayEvent {
    /// A new display was connected
    Connected(DisplayHandle),
    /// A display was disconnected
    Disconnected(DisplayHandle),
    /// Display configuration changed (resolution, position, DPI)
    Changed(DisplayHandle),
    /// Primary display changed
    PrimaryChanged(DisplayHandle),
}

/// Error type for display operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum DisplayError {
    #[error("Display not found: {0}")]
    NotFound(DisplayHandle),

    #[error("Failed to enumerate displays: {0}")]
    EnumerationFailed(String),

    #[error("Platform error: {0}")]
    Platform(String),
}

/// Trait for display enumeration and monitoring
///
/// Platform implementations provide the actual display detection logic.
pub trait DisplayProvider: Send + Sync {
    /// Enumerate all currently connected displays
    fn enumerate(&self) -> Result<Vec<DisplayInfo>, DisplayError>;

    /// Get information about a specific display
    fn get(&self, handle: &DisplayHandle) -> Result<DisplayInfo, DisplayError>;

    /// Get the primary display
    fn primary(&self) -> Result<DisplayInfo, DisplayError>;

    /// Refresh the display list (call after receiving a change event)
    fn refresh(&mut self) -> Result<(), DisplayError>;
}

/// Trait for receiving display change notifications
///
/// Implement this to receive callbacks when displays are connected/disconnected
/// or when their configuration changes.
pub trait DisplayEventHandler: Send {
    /// Called when a display event occurs
    fn on_display_event(&mut self, event: DisplayEvent);
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(100, 100, 200, 150);

        assert!(rect.contains(Point::new(100, 100)));
        assert!(rect.contains(Point::new(200, 175)));
        assert!(!rect.contains(Point::new(99, 100)));
        assert!(!rect.contains(Point::new(300, 100))); // x >= 100 + 200
    }

    #[test]
    fn test_dpi_scale() {
        let default = DpiScale::default();
        assert!(!default.is_scaled());

        let scaled = DpiScale::uniform(1.5);
        assert!(scaled.is_scaled());
    }

    #[test]
    fn test_display_info_scaled_resolution() {
        let info = DisplayInfo {
            handle: DisplayHandle::new(1),
            name: "Test Display".into(),
            bounds: Rect::new(0, 0, 3840, 2160), // 4K
            work_area: Rect::new(0, 0, 3840, 2100),
            dpi: DpiScale::uniform(2.0), // 200% scaling
            is_primary: true,
            refresh_rate: Some(60.0),
        };

        let scaled = info.scaled_resolution();
        assert_eq!(scaled.width, 1920);
        assert_eq!(scaled.height, 1080);
    }
}
