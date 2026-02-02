//! macOS wallpaper backend
//!
//! Uses NSWindow at desktop window level to display content below all normal windows
//! but above the system wallpaper.
//!
//! # Architecture
//!
//! The macOS desktop uses a layered window system:
//! ```text
//! ┌─────────────────────────────────────┐
//! │     Normal Windows (top)            │
//! ├─────────────────────────────────────┤
//! │     Desktop Icons (Finder)          │
//! ├─────────────────────────────────────┤
//! │     Our Desktop Window              │  ← kCGDesktopWindowLevel
//! ├─────────────────────────────────────┤
//! │     System Wallpaper (bottom)       │
//! └─────────────────────────────────────┘
//! ```
//!
//! # Key Concepts
//!
//! - **Window Level**: kCGDesktopWindowLevel positions window above wallpaper
//! - **Collection Behavior**: Controls Spaces/Mission Control behavior
//! - **Borderless**: No title bar, fills screen
//! - **Click-through**: Ignores mouse events
//!
//! # Status
//!
//! This module is a placeholder. The objc2 0.5 API requires updates to the
//! encoding traits and autoreleasepool API. Full implementation is tracked
//! as a separate work item.

use tracing::debug;

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

// ============================================================================
// macOS Renderer (Placeholder)
// ============================================================================

/// macOS desktop-level wallpaper renderer
///
/// Note: This is currently a placeholder implementation. The full NSWindow-based
/// rendering requires updates to work with objc2 0.5 API changes.
pub struct MacOSRenderer {
    display_id: Option<DisplayId>,
    initialized: bool,
}

impl MacOSRenderer {
    /// Create a new macOS renderer
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            display_id: None,
            initialized: false,
        })
    }
}

impl Default for MacOSRenderer {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            display_id: None,
            initialized: false,
        })
    }
}

impl WallpaperRenderer for MacOSRenderer {
    fn init(&mut self, display_id: &DisplayId) -> Result<(), RenderError> {
        // Placeholder: Full implementation requires objc2 0.5 API updates
        self.display_id = Some(display_id.clone());
        self.initialized = true;
        debug!(
            target_display = %display_id,
            "macOS renderer initialized (placeholder mode)"
        );
        Ok(())
    }

    fn render(&mut self, _frame: &ProcessedFrame) -> Result<(), RenderError> {
        if !self.initialized {
            return Err(RenderError::Platform(
                "Renderer not initialized".to_string(),
            ));
        }
        // Placeholder: Full implementation requires objc2 0.5 API updates
        // Silently succeed - frames are not rendered in placeholder mode
        Ok(())
    }

    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        // Placeholder: Nothing to restore in placeholder mode
        Ok(())
    }

    fn shutdown(&mut self) {
        self.initialized = false;
        self.display_id = None;
        debug!("macOS renderer shutdown (placeholder mode)");
    }
}

// MacOSRenderer is Send because it doesn't contain any raw pointers in
// placeholder mode.
unsafe impl Send for MacOSRenderer {}

impl Drop for MacOSRenderer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let renderer = MacOSRenderer::new();
        assert!(renderer.is_ok());
    }

    #[test]
    fn test_renderer_default() {
        let renderer = MacOSRenderer::default();
        assert!(!renderer.initialized);
    }

    #[test]
    fn test_renderer_shutdown_without_init() {
        let mut renderer = MacOSRenderer::default();
        // Should not panic
        renderer.shutdown();
    }

    #[test]
    fn test_render_without_init() {
        let mut renderer = MacOSRenderer::default();
        let frame = ProcessedFrame {
            data: vec![0u8; 1920 * 1080 * 4],
            width: 1920,
            height: 1080,
            timestamp_ns: 0,
        };
        let result = renderer.render(&frame);
        // Should fail since not initialized
        assert!(result.is_err());
    }

    #[test]
    fn test_render_after_init() {
        let mut renderer = MacOSRenderer::default();
        let display = DisplayId::primary();

        // Initialize
        assert!(renderer.init(&display).is_ok());
        assert!(renderer.initialized);

        // Render should succeed in placeholder mode
        let frame = ProcessedFrame {
            data: vec![0u8; 1920 * 1080 * 4],
            width: 1920,
            height: 1080,
            timestamp_ns: 0,
        };
        assert!(renderer.render(&frame).is_ok());
    }
}
