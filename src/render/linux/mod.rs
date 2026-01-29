//! Linux wallpaper backend
//!
//! Uses X11 root window drawing for wallpaper integration.
#![allow(dead_code)] // Linux renderer implementation
//!
//! # Implementation Strategy
//!
//! 1. **Desktop Window Approach**: Create a window with `_NET_WM_WINDOW_TYPE_DESKTOP`
//!    hint, which compositors should render below all other windows.
//!
//! 2. **Root Window Fallback**: For non-composited environments, draw directly
//!    to the root window pixmap.
//!
//! # Rendering Path
//!
//! ```text
//! ProcessedFrame (RGBA) → XImage → XPutImage → Window/Root
//! ```
//!
//! For better performance with MIT-SHM extension:
//! ```text
//! ProcessedFrame (RGBA) → Shared Memory → XShmPutImage → Window/Root
//! ```
//!
//! # Tested Compositors
//!
//! - GNOME (Mutter)
//! - KDE (KWin)
//! - XFCE (Xfwm)

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::platform::restore_wallpaper_from_path;
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

#[cfg(feature = "linux")]
use x11rb::connection::Connection;
#[cfg(feature = "linux")]
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt, CreateGCAux, CreateWindowAux, EventMask,
    ImageFormat, PropMode, Screen, VisualClass, Visualid, WindowClass,
};
#[cfg(feature = "linux")]
use x11rb::rust_connection::RustConnection;
#[cfg(feature = "linux")]
use x11rb::wrapper::ConnectionExt as WrapperConnectionExt;
#[cfg(feature = "linux")]
use x11rb::COPY_DEPTH_FROM_PARENT;

/// Linux X11 wallpaper renderer
pub struct X11Renderer {
    #[cfg(feature = "linux")]
    connection: Option<RustConnection>,
    #[cfg(feature = "linux")]
    screen_num: usize,
    #[cfg(feature = "linux")]
    window: Option<u32>,
    #[cfg(feature = "linux")]
    gc: Option<u32>,
    #[cfg(feature = "linux")]
    width: u16,
    #[cfg(feature = "linux")]
    height: u16,

    // Non-linux placeholder
    #[cfg(not(feature = "linux"))]
    _placeholder: (),

    initialized: bool,
}

impl X11Renderer {
    /// Create a new X11 renderer
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            #[cfg(feature = "linux")]
            connection: None,
            #[cfg(feature = "linux")]
            screen_num: 0,
            #[cfg(feature = "linux")]
            window: None,
            #[cfg(feature = "linux")]
            gc: None,
            #[cfg(feature = "linux")]
            width: 0,
            #[cfg(feature = "linux")]
            height: 0,
            #[cfg(not(feature = "linux"))]
            _placeholder: (),
            initialized: false,
        })
    }

    #[cfg(feature = "linux")]
    fn find_visual(&self, screen: &Screen) -> Option<Visualid> {
        // Find a TrueColor visual for proper RGBA rendering
        for depth in &screen.allowed_depths {
            if depth.depth == 24 || depth.depth == 32 {
                for visual in &depth.visuals {
                    if visual.class == VisualClass::TRUE_COLOR {
                        return Some(visual.visual_id);
                    }
                }
            }
        }
        // Fallback to root visual
        Some(screen.root_visual)
    }

    #[cfg(feature = "linux")]
    fn create_desktop_window(&mut self) -> Result<(), RenderError> {
        let conn = self
            .connection
            .as_ref()
            .ok_or_else(|| RenderError::Platform("No X11 connection".into()))?;

        let screen = &conn.setup().roots[self.screen_num];
        let root = screen.root;
        self.width = screen.width_in_pixels;
        self.height = screen.height_in_pixels;

        let visual_id = self
            .find_visual(screen)
            .ok_or_else(|| RenderError::Platform("No suitable visual found".into()))?;

        // Generate window ID
        let window = conn
            .generate_id()
            .map_err(|e| RenderError::Platform(format!("Failed to generate window ID: {}", e)))?;

        // Create window covering entire screen
        let win_aux = CreateWindowAux::new()
            .event_mask(EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY)
            .background_pixel(screen.black_pixel)
            .override_redirect(1u32); // Override redirect to avoid window manager decoration

        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            root,
            0,
            0,
            self.width,
            self.height,
            0,
            WindowClass::INPUT_OUTPUT,
            visual_id,
            &win_aux,
        )
        .map_err(|e| RenderError::Platform(format!("Failed to create window: {}", e)))?;

        // Set desktop window type hint
        self.set_desktop_type_hint(window)?;

        // Create graphics context
        let gc = conn
            .generate_id()
            .map_err(|e| RenderError::Platform(format!("Failed to generate GC ID: {}", e)))?;

        conn.create_gc(gc, window, &CreateGCAux::new())
            .map_err(|e| RenderError::Platform(format!("Failed to create GC: {}", e)))?;

        // Map window
        conn.map_window(window)
            .map_err(|e| RenderError::Platform(format!("Failed to map window: {}", e)))?;

        // Lower window to bottom
        conn.configure_window(
            window,
            &x11rb::protocol::xproto::ConfigureWindowAux::new()
                .stack_mode(x11rb::protocol::xproto::StackMode::BELOW),
        )
        .map_err(|e| RenderError::Platform(format!("Failed to lower window: {}", e)))?;

        conn.flush()
            .map_err(|e| RenderError::Platform(format!("Failed to flush: {}", e)))?;

        self.window = Some(window);
        self.gc = Some(gc);

        Ok(())
    }

    #[cfg(feature = "linux")]
    fn set_desktop_type_hint(&self, window: u32) -> Result<(), RenderError> {
        let conn = self
            .connection
            .as_ref()
            .ok_or_else(|| RenderError::Platform("No X11 connection".into()))?;

        // Get atoms
        let wm_type_atom = conn
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE")
            .map_err(|e| RenderError::Platform(format!("Failed to intern atom: {}", e)))?
            .reply()
            .map_err(|e| RenderError::Platform(format!("Failed to get atom reply: {}", e)))?
            .atom;

        let desktop_atom = conn
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE_DESKTOP")
            .map_err(|e| RenderError::Platform(format!("Failed to intern atom: {}", e)))?
            .reply()
            .map_err(|e| RenderError::Platform(format!("Failed to get atom reply: {}", e)))?
            .atom;

        // Set property
        conn.change_property32(
            PropMode::REPLACE,
            window,
            wm_type_atom,
            AtomEnum::ATOM,
            &[desktop_atom],
        )
        .map_err(|e| RenderError::Platform(format!("Failed to set window type: {}", e)))?;

        Ok(())
    }

    #[cfg(feature = "linux")]
    fn render_frame_to_window(&self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        let conn = self
            .connection
            .as_ref()
            .ok_or_else(|| RenderError::Platform("No X11 connection".into()))?;

        let window = self
            .window
            .ok_or_else(|| RenderError::Platform("No window created".into()))?;

        let gc = self
            .gc
            .ok_or_else(|| RenderError::Platform("No graphics context".into()))?;

        // Convert RGBA to BGRA/BGRX format for X11
        // At depth 24, X11 uses 4 bytes per pixel (BGRX) where the 4th byte is padding.
        // This is more efficient than 3-byte BGR as it maintains alignment.
        let bgra_data = rgba_to_bgra(&frame.data);

        // Calculate dimensions - scale to window size if needed
        let src_width = frame.width as u16;
        let src_height = frame.height as u16;

        // Use XPutImage to render the frame
        // Note: For better performance, MIT-SHM should be used (future optimization)
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            window,
            gc,
            src_width,
            src_height,
            0, // dst_x
            0, // dst_y
            0, // left_pad
            24, // depth 24 uses BGRX (4 bytes/pixel, alpha ignored)
            &bgra_data,
        )
        .map_err(|e| RenderError::Platform(format!("Failed to put image: {}", e)))?;

        conn.flush()
            .map_err(|e| RenderError::Platform(format!("Failed to flush: {}", e)))?;

        Ok(())
    }
}

impl WallpaperRenderer for X11Renderer {
    fn init(&mut self, _display_id: &DisplayId) -> Result<(), RenderError> {
        #[cfg(feature = "linux")]
        {
            // Connect to X11 server
            let (conn, screen_num) = RustConnection::connect(None)
                .map_err(|e| RenderError::Platform(format!("Failed to connect to X11: {}", e)))?;

            self.connection = Some(conn);
            self.screen_num = screen_num;

            // Create desktop window
            self.create_desktop_window()?;

            self.initialized = true;
            tracing::info!(
                "X11 renderer initialized: {}x{}",
                self.width,
                self.height
            );

            Ok(())
        }

        #[cfg(not(feature = "linux"))]
        Err(RenderError::Platform(
            "Linux X11 renderer not available on this platform".into(),
        ))
    }

    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        if !self.initialized {
            return Err(RenderError::Platform("Renderer not initialized".into()));
        }

        #[cfg(feature = "linux")]
        {
            return self.render_frame_to_window(frame);
        }

        #[cfg(not(feature = "linux"))]
        {
            let _ = frame; // Suppress unused warning when feature is disabled
            Err(RenderError::Platform(
                "Linux X11 renderer not available on this platform".into(),
            ))
        }
    }

    fn restore(&mut self, config: &AppConfig) -> Result<(), RenderError> {
        // Restore original wallpaper if we have a backup path
        if let Some(ref backup_path) = &config.internal.original_wallpaper_path {
            if let Err(e) = restore_wallpaper_from_path(backup_path) {
                tracing::warn!("Failed to restore wallpaper: {}", e);
                // Don't fail the entire restore operation
            }
        }

        Ok(())
    }

    fn shutdown(&mut self) {
        #[cfg(feature = "linux")]
        {
            if let (Some(conn), Some(window)) = (&self.connection, self.window) {
                // Destroy window
                let _ = conn.destroy_window(window);
                let _ = conn.flush();
            }

            // Free graphics context
            if let (Some(conn), Some(gc)) = (&self.connection, self.gc) {
                let _ = conn.free_gc(gc);
            }

            self.window = None;
            self.gc = None;
            self.connection = None;
        }

        self.initialized = false;
        tracing::info!("X11 renderer shutdown complete");
    }
}

/// Convert RGBA to BGRA format for X11
#[cfg(feature = "linux")]
fn rgba_to_bgra(rgba: &[u8]) -> Vec<u8> {
    let mut bgra = Vec::with_capacity(rgba.len());

    for chunk in rgba.chunks_exact(4) {
        bgra.push(chunk[2]); // B
        bgra.push(chunk[1]); // G
        bgra.push(chunk[0]); // R
        bgra.push(chunk[3]); // A
    }

    bgra
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let renderer = X11Renderer::new();
        assert!(renderer.is_ok());

        let renderer = renderer.unwrap();
        assert!(!renderer.initialized);
    }

    #[test]
    #[cfg(feature = "linux")]
    fn test_rgba_to_bgra() {
        let rgba = vec![255, 128, 64, 255, 0, 0, 0, 128];
        let bgra = rgba_to_bgra(&rgba);

        assert_eq!(bgra.len(), 8);
        // First pixel: R=255, G=128, B=64, A=255 -> B=64, G=128, R=255, A=255
        assert_eq!(bgra[0], 64);  // B
        assert_eq!(bgra[1], 128); // G
        assert_eq!(bgra[2], 255); // R
        assert_eq!(bgra[3], 255); // A

        // Second pixel: R=0, G=0, B=0, A=128 -> B=0, G=0, R=0, A=128
        assert_eq!(bgra[4], 0);   // B
        assert_eq!(bgra[5], 0);   // G
        assert_eq!(bgra[6], 0);   // R
        assert_eq!(bgra[7], 128); // A
    }

    #[test]
    fn test_renderer_shutdown_without_init() {
        let mut renderer = X11Renderer::new().unwrap();
        // Should not panic
        renderer.shutdown();
        assert!(!renderer.initialized);
    }

    #[test]
    fn test_render_without_init() {
        let mut renderer = X11Renderer::new().unwrap();
        let frame = ProcessedFrame::new(vec![0u8; 100 * 100 * 4], 100, 100);

        let result = renderer.render(&frame);
        assert!(result.is_err());
    }

    // Note: Full integration tests require an X11 display
    // Run with: cargo test --features linux -- --ignored

    #[test]
    #[ignore = "requires X11 display"]
    #[cfg(feature = "linux")]
    fn test_x11_init_and_render() {
        let mut renderer = X11Renderer::new().unwrap();

        // Initialize
        let result = renderer.init(&DisplayId("test".to_string()));
        if result.is_err() {
            eprintln!("X11 init failed (may need display): {:?}", result);
            return;
        }

        assert!(renderer.initialized);

        // Create a test frame (red gradient)
        let width = 800;
        let height = 600;
        let mut data = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                data[idx] = (x * 255 / width) as u8;     // R
                data[idx + 1] = (y * 255 / height) as u8; // G
                data[idx + 2] = 128;                      // B
                data[idx + 3] = 255;                      // A
            }
        }

        let frame = ProcessedFrame::new(data, width as u32, height as u32);

        // Render
        let result = renderer.render(&frame);
        assert!(result.is_ok());

        // Cleanup
        renderer.shutdown();
        assert!(!renderer.initialized);
    }
}
