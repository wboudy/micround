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

use tracing::{debug, error, info, warn};

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

// ============================================================================
// macOS Implementation
// ============================================================================

#[cfg(target_os = "macos")]
mod objc_impl {
    use objc2::ffi::NSInteger;
    use objc2::rc::autoreleasepool;
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    use std::ptr;

    /// NSWindowLevel for desktop level
    /// CGWindowLevelKey.desktopWindow = kCGDesktopWindowLevel = -2147483623
    pub const DESKTOP_WINDOW_LEVEL: i64 = -2147483623;

    /// NSWindowCollectionBehavior flags
    pub mod collection_behavior {
        /// Can appear in all Spaces
        pub const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
        /// Stationary during Space changes
        pub const STATIONARY: u64 = 1 << 4;
        /// Not in Cmd+Tab cycle
        pub const IGNORES_CYCLE: u64 = 1 << 6;
        /// Doesn't participate in Mission Control
        pub const TRANSIENT: u64 = 1 << 3;
    }

    /// NSWindowStyleMask flags
    pub mod style_mask {
        /// Borderless window (no title bar)
        pub const BORDERLESS: u64 = 0;
    }

    /// NSBackingStoreType
    pub const BACKING_STORE_BUFFERED: u64 = 2;

    /// Get main screen frame
    pub unsafe fn main_screen_frame() -> (f64, f64, f64, f64) {
        let screen_cls = class!(NSScreen);
        let main_screen: *const AnyObject = msg_send![screen_cls, mainScreen];
        if main_screen.is_null() {
            return (0.0, 0.0, 1920.0, 1080.0); // Fallback
        }

        // NSRect is a struct with origin (x, y) and size (width, height)
        // Each is two f64 values, totaling 32 bytes
        #[repr(C)]
        #[derive(Copy, Clone)]
        struct NSRect {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        }

        let frame: NSRect = msg_send![main_screen, frame];
        (frame.x, frame.y, frame.width, frame.height)
    }

    /// Create and configure a desktop-level window
    pub unsafe fn create_desktop_window() -> Result<*const AnyObject, String> {
        let window_cls = class!(NSWindow);

        // Get screen frame
        let (x, y, width, height) = main_screen_frame();

        #[repr(C)]
        #[derive(Copy, Clone)]
        struct NSRect {
            x: f64,
            y: f64,
            width: f64,
            height: f64,
        }

        let content_rect = NSRect {
            x,
            y,
            width,
            height,
        };

        // Create window with borderless style
        let window: *const AnyObject = msg_send![window_cls, alloc];
        if window.is_null() {
            return Err("Failed to allocate NSWindow".to_string());
        }

        let window: *const AnyObject = msg_send![
            window,
            initWithContentRect: content_rect
            styleMask: style_mask::BORDERLESS
            backing: BACKING_STORE_BUFFERED
            defer: Bool::NO
        ];
        if window.is_null() {
            return Err("Failed to initialize NSWindow".to_string());
        }

        // Set window level to desktop
        let _: () = msg_send![window, setLevel: DESKTOP_WINDOW_LEVEL];

        // Set collection behavior for Spaces handling
        let behavior = collection_behavior::CAN_JOIN_ALL_SPACES
            | collection_behavior::STATIONARY
            | collection_behavior::IGNORES_CYCLE;
        let _: () = msg_send![window, setCollectionBehavior: behavior];

        // Configure window properties
        let _: () = msg_send![window, setOpaque: Bool::YES];
        let _: () = msg_send![window, setHasShadow: Bool::NO];

        // Set black background
        let color_cls = class!(NSColor);
        let black_color: *const AnyObject = msg_send![color_cls, blackColor];
        let _: () = msg_send![window, setBackgroundColor: black_color];

        // Make click-through (ignores mouse events)
        let _: () = msg_send![window, setIgnoresMouseEvents: Bool::YES];

        // Ensure we control lifetime explicitly
        let _: () = msg_send![window, setReleasedWhenClosed: Bool::NO];

        Ok(window)
    }

    /// Show the window
    pub unsafe fn show_window(window: *const AnyObject) {
        let _: () = msg_send![window, orderFront: ptr::null::<AnyObject>()];
    }

    /// Hide the window
    pub unsafe fn hide_window(window: *const AnyObject) {
        let _: () = msg_send![window, orderOut: ptr::null::<AnyObject>()];
    }

    /// Close the window
    pub unsafe fn close_window(window: *const AnyObject) {
        let _: () = msg_send![window, close];
        let _: () = msg_send![window, release];
    }

    /// Render frame data to the window's content view
    pub unsafe fn render_to_window(
        window: *const AnyObject,
        data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        autoreleasepool(|| {
            // Get content view
            let content_view: *const AnyObject = msg_send![window, contentView];
            if content_view.is_null() {
                return Err("No content view".to_string());
            }

            // Create NSBitmapImageRep from raw RGBA data
            let image_rep_cls = class!(NSBitmapImageRep);

            // For RGBA data, bytes per row = width * 4
            let bytes_per_row = width as i64 * 4;
            let bits_per_sample: i64 = 8;
            let samples_per_pixel: i64 = 4;
            let has_alpha = Bool::YES;
            let is_planar = Bool::NO;

            // Create bitmap image rep
            // Note: This creates a new buffer, we'll need to copy data into it
            let image_rep: *const AnyObject = msg_send![image_rep_cls, alloc];

            let image_rep: *const AnyObject = msg_send![
                image_rep,
                initWithBitmapDataPlanes: ptr::null::<*const u8>()
                pixelsWide: width as i64
                pixelsHigh: height as i64
                bitsPerSample: bits_per_sample
                samplesPerPixel: samples_per_pixel
                hasAlpha: has_alpha
                isPlanar: is_planar
                colorSpaceName: "NSCalibratedRGBColorSpace"
                bytesPerRow: bytes_per_row
                bitsPerPixel: 32i64
            ];

            if image_rep.is_null() {
                return Err("Failed to create NSBitmapImageRep".to_string());
            }

            // Get the bitmap data pointer and copy our data
            let bitmap_data: *mut u8 = msg_send![image_rep, bitmapData];
            if bitmap_data.is_null() {
                let _: () = msg_send![image_rep, release];
                return Err("Failed to get bitmap data pointer".to_string());
            }

            // Copy frame data
            let copy_len = (width * height * 4) as usize;
            if data.len() >= copy_len {
                std::ptr::copy_nonoverlapping(data.as_ptr(), bitmap_data, copy_len);
            }

            // Create NSImage from bitmap rep
            let image_cls = class!(NSImage);
            let image: *const AnyObject = msg_send![image_cls, alloc];
            let image: *const AnyObject =
                msg_send![image, initWithSize: (width as f64, height as f64)];
            if image.is_null() {
                let _: () = msg_send![image_rep, release];
                return Err("Failed to create NSImage".to_string());
            }

            let _: () = msg_send![image, addRepresentation: image_rep];
            let _: () = msg_send![image_rep, release];

            // Create NSImageView and set as content view
            // Or update existing image view

            // For now, we'll update the layer's contents directly
            // This requires CALayer support

            // Alternative: Draw directly using lockFocus/unlockFocus
            let _: () = msg_send![content_view, lockFocus];

            // Draw the image
            #[repr(C)]
            #[derive(Copy, Clone)]
            struct NSRect {
                x: f64,
                y: f64,
                width: f64,
                height: f64,
            }

            let view_bounds: NSRect = msg_send![content_view, bounds];
            let draw_rect = NSRect {
                x: 0.0,
                y: 0.0,
                width: view_bounds.width,
                height: view_bounds.height,
            };

            // Draw image scaled to fill
            let _: () = msg_send![image, drawInRect: draw_rect];

            let _: () = msg_send![content_view, unlockFocus];
            let _: () = msg_send![image, release];

            // Refresh the view
            let _: () = msg_send![content_view, setNeedsDisplay: Bool::YES];

            Ok(())
        })
    }
}

// ============================================================================
// macOS Renderer
// ============================================================================

/// macOS desktop-level wallpaper renderer
pub struct MacOSRenderer {
    #[cfg(target_os = "macos")]
    window: Option<*const objc2::runtime::AnyObject>,
    display_id: Option<DisplayId>,
    initialized: bool,
}

impl MacOSRenderer {
    /// Create a new macOS renderer
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            #[cfg(target_os = "macos")]
            window: None,
            display_id: None,
            initialized: false,
        })
    }
}

impl Default for MacOSRenderer {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            #[cfg(target_os = "macos")]
            window: None,
            display_id: None,
            initialized: false,
        })
    }
}

impl WallpaperRenderer for MacOSRenderer {
    #[cfg(target_os = "macos")]
    fn init(&mut self, display: &DisplayId) -> Result<(), RenderError> {
        use objc_impl::*;

        // Close existing window if any
        if let Some(window) = self.window.take() {
            unsafe {
                close_window(window);
            }
        }

        // Create new desktop window
        let window = unsafe { create_desktop_window().map_err(RenderError::Platform)? };

        // Show the window
        unsafe {
            show_window(window);
        }

        self.window = Some(window);
        self.display_id = Some(display.clone());
        self.initialized = true;

        info!(target_display = %display, "macOS desktop window created");

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn init(&mut self, _display: &DisplayId) -> Result<(), RenderError> {
        Err(RenderError::Platform(
            "macOS renderer not available on this platform".to_string(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        use objc_impl::*;

        let window = self
            .window
            .ok_or_else(|| RenderError::Platform("Window not initialized".to_string()))?;

        unsafe {
            render_to_window(window, &frame.data, frame.width, frame.height)
                .map_err(RenderError::Platform)?;
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn render(&mut self, _frame: &ProcessedFrame) -> Result<(), RenderError> {
        Err(RenderError::Platform(
            "macOS renderer not available on this platform".to_string(),
        ))
    }

    #[cfg(target_os = "macos")]
    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        use objc_impl::*;

        // Hide our window to reveal original wallpaper
        if let Some(window) = self.window {
            unsafe {
                hide_window(window);
            }
            info!("macOS wallpaper restored (window hidden)");
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        Ok(())
    }

    fn shutdown(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use objc_impl::*;

            if let Some(window) = self.window.take() {
                unsafe {
                    hide_window(window);
                    close_window(window);
                }
            }
        }

        self.initialized = false;
        self.display_id = None;
        debug!("macOS renderer shutdown");
    }
}

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

    #[cfg(target_os = "macos")]
    #[test]
    fn test_desktop_window_level_constant() {
        use objc_impl::*;
        // kCGDesktopWindowLevel is a specific constant
        assert_eq!(DESKTOP_WINDOW_LEVEL, -2147483623);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_collection_behavior_flags() {
        use objc_impl::collection_behavior::*;
        // Ensure flags are distinct powers of 2
        assert_eq!(CAN_JOIN_ALL_SPACES, 1);
        assert_eq!(STATIONARY, 16); // 1 << 4
        assert_eq!(IGNORES_CYCLE, 64); // 1 << 6
    }
}
