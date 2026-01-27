//! Wallpaper rendering backends
//!
//! Platform-specific implementations for rendering frames to the desktop wallpaper.

use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;

/// Trait for platform-specific wallpaper renderers
pub trait WallpaperRenderer: Send {
    /// Initialize the renderer for a specific display
    fn init(&mut self, display: &DisplayId) -> Result<(), RenderError>;

    /// Render a frame to the wallpaper
    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError>;

    /// Restore the original wallpaper
    fn restore(&mut self) -> Result<(), RenderError>;

    /// Clean up resources
    fn shutdown(&mut self);
}

// Platform-specific implementations will be added when those features are implemented
