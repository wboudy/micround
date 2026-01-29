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

// Platform-specific implementations

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

// Simulator for testing (no feature gate - always available for testing)
pub mod simulator;

/// Create a platform-appropriate wallpaper renderer
#[cfg(target_os = "linux")]
pub fn create_renderer() -> Result<Box<dyn WallpaperRenderer>, RenderError> {
    Ok(Box::new(linux::X11Renderer::new()?))
}

/// Create a platform-appropriate wallpaper renderer (Windows)
#[cfg(target_os = "windows")]
pub fn create_renderer() -> Result<Box<dyn WallpaperRenderer>, RenderError> {
    Ok(Box::new(windows::WindowsRenderer::new()?))
}

// Placeholder for other platforms (macOS)
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn create_renderer() -> Result<Box<dyn WallpaperRenderer>, RenderError> {
    unimplemented!("Wallpaper renderer not implemented for this platform")
}
