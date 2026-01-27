//! Linux wallpaper backend
//!
//! Uses X11 root window drawing for wallpaper integration.
//!
//! # Implementation Notes
//!
//! Primary strategy: Draw directly to X11 root window
//! Fallback: EWMH _NET_WM_WINDOW_TYPE_DESKTOP hint
//!
//! Tested compositors: GNOME (Mutter), KDE (KWin), XFCE (Xfwm)

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

/// Linux X11 wallpaper renderer
pub struct X11Renderer {
    // TODO: Add X11 connection and state (bd-fpo epic)
    _initialized: bool,
}

impl X11Renderer {
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self { _initialized: false })
    }
}

// Note: No Default impl since construction can fail

impl WallpaperRenderer for X11Renderer {
    fn init(&mut self, _display_id: &DisplayId) -> Result<(), RenderError> {
        // TODO: Implement X11 initialization (bd-fpo epic)
        Err(RenderError::Platform(
            "Linux X11 renderer not yet implemented".into(),
        ))
    }

    fn render(&mut self, _frame: &ProcessedFrame) -> Result<(), RenderError> {
        // TODO: Implement frame rendering (bd-fpo epic)
        Err(RenderError::Platform(
            "Linux X11 renderer not yet implemented".into(),
        ))
    }

    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        // TODO: Implement wallpaper restore (bd-fpo epic)
        Ok(())
    }

    fn shutdown(&mut self) {
        // TODO: Cleanup X11 resources (bd-fpo epic)
        self._initialized = false;
    }
}
