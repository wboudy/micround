//! Desktop-level window abstraction
//!
//! Provides platform-independent traits for creating and managing windows at the
//! desktop level (behind icons, at wallpaper layer).
#![allow(dead_code)] // Cross-platform window API
//!
//! # Platform Implementations
//! - Windows: WorkerW window behind SHELLDLL_DefView
//! - macOS: NSWindow at kCGDesktopWindowLevel
//! - Linux X11: Root window drawing or _NET_WM_WINDOW_TYPE_DESKTOP

use crate::platform::display::{DisplayHandle, Rect, Size};

/// Handle to a desktop window
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowHandle(pub(crate) u64);

impl WindowHandle {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Configuration for creating a desktop window
#[derive(Debug, Clone)]
pub struct WindowConfig {
    /// Target display for this window
    pub display: DisplayHandle,

    /// Initial bounds (if None, uses full display)
    pub bounds: Option<Rect>,

    /// Window title (may not be visible on desktop-level windows)
    pub title: String,

    /// Whether the window should be visible immediately
    pub visible: bool,
}

impl WindowConfig {
    pub fn new(display: DisplayHandle) -> Self {
        Self {
            display,
            bounds: None,
            title: "Micround".into(),
            visible: true,
        }
    }

    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
}

/// Pixel format for render surfaces
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceFormat {
    /// 8-bit BGRA (common on Windows)
    Bgra8,
    /// 8-bit RGBA
    Rgba8,
    /// 8-bit RGB (no alpha)
    Rgb8,
}

impl SurfaceFormat {
    /// Bytes per pixel for this format
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Bgra8 | Self::Rgba8 => 4,
            Self::Rgb8 => 3,
        }
    }
}

/// Information about the render surface
#[derive(Debug, Clone)]
pub struct SurfaceInfo {
    /// Surface size in pixels
    pub size: Size,

    /// Pixel format
    pub format: SurfaceFormat,

    /// Bytes per row (may include padding)
    pub stride: usize,
}

impl SurfaceInfo {
    /// Calculate expected buffer size
    pub fn buffer_size(&self) -> usize {
        self.stride * self.size.height as usize
    }
}

/// Error type for window operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum WindowError {
    #[error("Failed to create window: {0}")]
    CreationFailed(String),

    #[error("Window not found")]
    NotFound,

    #[error("Failed to get render surface: {0}")]
    SurfaceError(String),

    #[error("Invalid pixel data: expected {expected} bytes, got {actual}")]
    InvalidData { expected: usize, actual: usize },

    #[error("Window is not visible")]
    NotVisible,

    #[error("Platform error: {0}")]
    Platform(String),
}

/// Trait for creating and managing desktop-level windows
///
/// This window sits at the desktop level, behind icons but above the actual
/// wallpaper, allowing us to render video frames as a live wallpaper.
pub trait DesktopWindow: Send {
    /// Create a new desktop window
    fn create(&mut self, config: WindowConfig) -> Result<WindowHandle, WindowError>;

    /// Destroy a window
    fn destroy(&mut self, handle: &WindowHandle) -> Result<(), WindowError>;

    /// Show or hide the window
    fn set_visible(&mut self, handle: &WindowHandle, visible: bool) -> Result<(), WindowError>;

    /// Update window bounds
    fn set_bounds(&mut self, handle: &WindowHandle, bounds: Rect) -> Result<(), WindowError>;

    /// Get current window bounds
    fn get_bounds(&self, handle: &WindowHandle) -> Result<Rect, WindowError>;

    /// Get render surface information
    fn surface_info(&self, handle: &WindowHandle) -> Result<SurfaceInfo, WindowError>;

    /// Present pixel data to the window
    ///
    /// The data must match the surface format and size from `surface_info()`.
    /// This is the primary method for rendering frames.
    fn present(&mut self, handle: &WindowHandle, data: &[u8]) -> Result<(), WindowError>;

    /// Check if the window is currently visible
    fn is_visible(&self, handle: &WindowHandle) -> Result<bool, WindowError>;

    /// Force a redraw of the window
    fn invalidate(&mut self, handle: &WindowHandle) -> Result<(), WindowError>;
}

/// Trait for GPU-accelerated window rendering
///
/// Optional trait for platforms that support GPU texture uploads
/// instead of CPU pixel copies.
pub trait GpuWindow: DesktopWindow {
    /// Get a GPU texture handle for direct rendering
    ///
    /// Returns a platform-specific handle that can be used with
    /// GPU APIs (D3D11, Metal, Vulkan, OpenGL).
    fn gpu_texture(&self, handle: &WindowHandle) -> Result<GpuTexture, WindowError>;

    /// Signal that GPU rendering is complete
    fn present_gpu(&mut self, handle: &WindowHandle) -> Result<(), WindowError>;
}

/// Platform-specific GPU texture handle
#[derive(Debug)]
pub enum GpuTexture {
    /// Direct3D 11 texture (Windows)
    #[cfg(target_os = "windows")]
    D3D11 { texture: *mut std::ffi::c_void },

    /// Metal texture (macOS)
    #[cfg(target_os = "macos")]
    Metal { texture: *mut std::ffi::c_void },

    /// OpenGL texture ID (Linux)
    #[cfg(target_os = "linux")]
    OpenGL { texture_id: u32 },

    /// Placeholder for unsupported platforms
    #[allow(dead_code)]
    Unsupported,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_format_bytes() {
        assert_eq!(SurfaceFormat::Bgra8.bytes_per_pixel(), 4);
        assert_eq!(SurfaceFormat::Rgba8.bytes_per_pixel(), 4);
        assert_eq!(SurfaceFormat::Rgb8.bytes_per_pixel(), 3);
    }

    #[test]
    fn test_surface_info_buffer_size() {
        let info = SurfaceInfo {
            size: Size::new(1920, 1080),
            format: SurfaceFormat::Bgra8,
            stride: 1920 * 4, // No padding
        };

        assert_eq!(info.buffer_size(), 1920 * 1080 * 4);
    }

    #[test]
    fn test_window_config_builder() {
        let config = WindowConfig::new(DisplayHandle::new(1))
            .with_title("Test Window")
            .hidden();

        assert_eq!(config.title, "Test Window");
        assert!(!config.visible);
    }
}
