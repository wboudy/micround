//! Wallpaper rendering backends
//!
//! Platform-specific implementations for rendering frames to the desktop wallpaper.

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;

/// Trait for platform-specific wallpaper renderers
pub trait WallpaperRenderer: Send {
    /// Initialize the renderer for a specific display
    fn init(&mut self, display: &DisplayId) -> Result<(), RenderError>;

    /// Render a frame to the wallpaper
    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError>;

    /// Restore the original wallpaper
    fn restore(&mut self, config: &AppConfig) -> Result<(), RenderError>;

    /// Clean up resources
    fn shutdown(&mut self);
}

// Platform-specific implementations will be added when those features are implemented

#[cfg(target_os = "linux")]
pub mod linux;

/// Create a platform-appropriate wallpaper renderer
#[cfg(target_os = "linux")]
pub fn create_renderer() -> Result<Box<dyn WallpaperRenderer>, RenderError> {
    Ok(Box::new(linux::X11Renderer::new()?))
}

// Placeholder for other platforms
#[cfg(not(target_os = "linux"))]
pub fn create_renderer() -> Result<Box<dyn WallpaperRenderer>, RenderError> {
    unimplemented!("Wallpaper renderer not implemented for this platform")
}
