//! macOS wallpaper backend
//!
//! Uses NSWindow at desktop window level to render below all normal windows
//! but above the system wallpaper.
//!
//! # Rendering Backends
//!
//! Two rendering backends are available:
//! - **Software (NSImageView)**: Default, works everywhere but slower
//! - **Metal (CAMetalLayer)**: GPU-accelerated, much faster, preferred when available
//!
//! Use `MetalRenderer` for new code when possible.
//!
//! # Implementation Strategy
//!
//! The macOS desktop uses window levels to determine Z-order:
//! ```text
//! ┌─────────────────────────────────┐
//! │       Normal Windows            │  ← NSWindow.Level.normal (top)
//! ├─────────────────────────────────┤
//! │       Desktop Icons             │  ← Finder icons
//! ├─────────────────────────────────┤
//! │       Our Window                │  ← kCGDesktopWindowLevel
//! ├─────────────────────────────────┤
//! │       System Wallpaper          │  ← Actual wallpaper (bottom)
//! └─────────────────────────────────┘
//! ```
//!
//! # Window Configuration
//!
//! - Level: kCGDesktopWindowLevel (below icons, above wallpaper)
//! - Style: Borderless (no title bar)
//! - Behavior: canJoinAllSpaces, stationary, ignoresCycle
//! - Does not become key window
//! - Ignores mouse events (click-through)
//!
//! # Thread Safety
//!
//! NSWindow and NSView operations must be performed on the main thread.
//! The caller is responsible for ensuring this. In a typical app architecture,
//! the render loop should dispatch to the main thread for window updates.

pub mod metal;

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::encode::{Encode, Encoding};
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::rc::autoreleasepool;
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::runtime::{AnyObject, Bool};
#[cfg(all(target_os = "macos", feature = "macos"))]
use objc2::{class, msg_send};
#[cfg(all(target_os = "macos", feature = "macos"))]
use std::ffi::c_void;

// ============================================================================
// Constants
// ============================================================================

/// Window level for desktop windows (below icons, above wallpaper)
#[cfg(all(target_os = "macos", feature = "macos"))]
const K_CG_DESKTOP_WINDOW_LEVEL: i64 = -2147483623; // kCGDesktopWindowLevel

/// NSWindow style mask constants
#[cfg(all(target_os = "macos", feature = "macos"))]
mod style_mask {
    pub const BORDERLESS: u64 = 0;
}

/// NSWindowCollectionBehavior constants
#[cfg(all(target_os = "macos", feature = "macos"))]
mod collection_behavior {
    pub const CAN_JOIN_ALL_SPACES: u64 = 1 << 0;
    pub const STATIONARY: u64 = 1 << 4;
    pub const IGNORES_CYCLE: u64 = 1 << 6;
    pub const FULL_SCREEN_AUXILIARY: u64 = 1 << 8;
}

// ============================================================================
// macOS Renderer
// ============================================================================

/// macOS wallpaper renderer using NSWindow at desktop level
pub struct MacOSRenderer {
    /// Handle to the NSWindow
    #[cfg(all(target_os = "macos", feature = "macos"))]
    window: Option<*mut c_void>,

    /// Handle to the NSImageView for displaying frames
    #[cfg(all(target_os = "macos", feature = "macos"))]
    image_view: Option<*mut c_void>,

    /// Current display width
    #[allow(dead_code)] // Used only with macos feature
    width: u32,
    /// Current display height
    #[allow(dead_code)] // Used only with macos feature
    height: u32,

    /// Whether the renderer is initialized
    initialized: bool,

    /// Non-macos placeholder
    #[cfg(not(all(target_os = "macos", feature = "macos")))]
    _placeholder: (),
}

// SAFETY: The raw pointers to NSWindow and NSImageView are only accessed
// via Objective-C message passing. While macOS GUI objects must be accessed
// from the main thread, the WallpaperRenderer trait requires Send to allow
// the renderer to be moved between threads (e.g., for initialization on one
// thread and later use on the main thread). The caller is responsible for
// ensuring GUI operations happen on the main thread.
#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Send for MacOSRenderer {}

impl MacOSRenderer {
    /// Create a new macOS renderer
    pub fn new() -> Result<Self, RenderError> {
        Ok(Self {
            #[cfg(all(target_os = "macos", feature = "macos"))]
            window: None,
            #[cfg(all(target_os = "macos", feature = "macos"))]
            image_view: None,
            width: 0,
            height: 0,
            initialized: false,
            #[cfg(not(all(target_os = "macos", feature = "macos")))]
            _placeholder: (),
        })
    }

    /// Get the main screen bounds
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn get_main_screen_frame(&self) -> Result<NSRect, RenderError> {
        unsafe {
            let screen_class = class!(NSScreen);
            let main_screen: *const AnyObject = msg_send![screen_class, mainScreen];

            if main_screen.is_null() {
                return Err(RenderError::DisplayNotFound("No main screen found".into()));
            }

            let frame: NSRect = msg_send![main_screen, frame];
            Ok(frame)
        }
    }

    /// Create the desktop-level window
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn create_window(&mut self, frame: NSRect) -> Result<*mut c_void, RenderError> {
        unsafe {
            autoreleasepool(|_| {
                // Create NSWindow
                let window_class = class!(NSWindow);
                let window: *const AnyObject = msg_send![window_class, alloc];

                if window.is_null() {
                    return Err(RenderError::SurfaceCreation(
                        "Failed to allocate NSWindow".into(),
                    ));
                }

                // Initialize with borderless style
                let window: *const AnyObject = msg_send![
                    window,
                    initWithContentRect: frame
                    styleMask: style_mask::BORDERLESS
                    backing: 2u64 // NSBackingStoreBuffered
                    defer: Bool::NO
                ];

                if window.is_null() {
                    return Err(RenderError::SurfaceCreation(
                        "Failed to initialize NSWindow".into(),
                    ));
                }

                // Set window level to desktop
                let _: () = msg_send![window, setLevel: K_CG_DESKTOP_WINDOW_LEVEL];

                // Configure collection behavior
                let behavior = collection_behavior::CAN_JOIN_ALL_SPACES
                    | collection_behavior::STATIONARY
                    | collection_behavior::IGNORES_CYCLE
                    | collection_behavior::FULL_SCREEN_AUXILIARY;
                let _: () = msg_send![window, setCollectionBehavior: behavior];

                // Configure window properties
                let _: () = msg_send![window, setOpaque: Bool::YES];
                let _: () = msg_send![window, setHasShadow: Bool::NO];
                // Note: canBecomeKeyWindow/canBecomeMainWindow are read-only properties.
                // For a borderless window at desktop level with ignoresMouseEvents,
                // the window won't receive focus anyway.
                let _: () = msg_send![window, setIgnoresMouseEvents: Bool::YES];
                let _: () = msg_send![window, setAcceptsMouseMovedEvents: Bool::NO];

                // Set background color to black
                let black_color: *const AnyObject = msg_send![class!(NSColor), blackColor];
                let _: () = msg_send![window, setBackgroundColor: black_color];

                // Retain the window
                let _: () = msg_send![window, retain];

                Ok(window as *mut c_void)
            })
        }
    }

    /// Create the image view for displaying frames
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn create_image_view(&mut self, frame: NSRect) -> Result<*mut c_void, RenderError> {
        unsafe {
            autoreleasepool(|_| {
                // Create NSImageView
                let image_view_class = class!(NSImageView);
                let image_view: *const AnyObject = msg_send![image_view_class, alloc];

                if image_view.is_null() {
                    return Err(RenderError::SurfaceCreation(
                        "Failed to allocate NSImageView".into(),
                    ));
                }

                let image_view: *const AnyObject = msg_send![image_view, initWithFrame: frame];

                if image_view.is_null() {
                    return Err(RenderError::SurfaceCreation(
                        "Failed to initialize NSImageView".into(),
                    ));
                }

                // Configure image view
                let _: () = msg_send![image_view, setImageScaling: 2i64]; // NSImageScaleProportionallyUpOrDown
                let _: () = msg_send![image_view, setImageAlignment: 5i64]; // NSImageAlignCenter

                // Retain the view
                let _: () = msg_send![image_view, retain];

                Ok(image_view as *mut c_void)
            })
        }
    }

    /// Create an NSImage from RGBA frame data
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn create_image_from_frame(
        &self,
        frame: &ProcessedFrame,
    ) -> Result<*const AnyObject, RenderError> {
        unsafe {
            autoreleasepool(|_| {
                // Create NSBitmapImageRep from the RGBA data
                let bitmap_class = class!(NSBitmapImageRep);

                // Allocate bitmap
                let bitmap: *const AnyObject = msg_send![bitmap_class, alloc];
                if bitmap.is_null() {
                    return Err(RenderError::FrameProcessing(
                        "Failed to allocate NSBitmapImageRep".into(),
                    ));
                }

                // Initialize bitmap with our frame data
                // initWithBitmapDataPlanes:pixelsWide:pixelsHigh:bitsPerSample:samplesPerPixel:
                // hasAlpha:isPlanar:colorSpaceName:bytesPerRow:bitsPerPixel:
                let bitmap: *const AnyObject = msg_send![
                    bitmap,
                    initWithBitmapDataPlanes: std::ptr::null::<*const u8>()
                    pixelsWide: frame.width as i64
                    pixelsHigh: frame.height as i64
                    bitsPerSample: 8i64
                    samplesPerPixel: 4i64
                    hasAlpha: Bool::YES
                    isPlanar: Bool::NO
                    colorSpaceName: nsstring("NSDeviceRGBColorSpace")
                    bytesPerRow: (frame.width * 4) as i64
                    bitsPerPixel: 32i64
                ];

                if bitmap.is_null() {
                    return Err(RenderError::FrameProcessing(
                        "Failed to initialize NSBitmapImageRep".into(),
                    ));
                }

                // Copy our frame data to the bitmap with bounds validation
                let bitmap_data: *mut u8 = msg_send![bitmap, bitmapData];
                if bitmap_data.is_null() {
                    let _: () = msg_send![bitmap, release];
                    return Err(RenderError::FrameProcessing(
                        "Bitmap data pointer is null".into(),
                    ));
                }

                // Validate frame data size matches expected bitmap size
                let expected_size = (frame.width as usize) * (frame.height as usize) * 4;
                if frame.data.len() < expected_size {
                    let _: () = msg_send![bitmap, release];
                    return Err(RenderError::FrameProcessing(format!(
                        "Frame data size mismatch: expected {} bytes, got {}",
                        expected_size,
                        frame.data.len()
                    )));
                }

                // Safe to copy now - we verified frame has sufficient data
                std::ptr::copy_nonoverlapping(frame.data.as_ptr(), bitmap_data, expected_size);

                // Create NSImage and add the bitmap representation
                let image_class = class!(NSImage);
                let size = NSSize {
                    width: frame.width as f64,
                    height: frame.height as f64,
                };
                let image: *const AnyObject = msg_send![image_class, alloc];
                let image: *const AnyObject = msg_send![image, initWithSize: size];

                if image.is_null() {
                    let _: () = msg_send![bitmap, release];
                    return Err(RenderError::FrameProcessing(
                        "Failed to create NSImage".into(),
                    ));
                }

                let _: () = msg_send![image, addRepresentation: bitmap];
                let _: () = msg_send![bitmap, release];

                Ok(image)
            })
        }
    }

    /// Release an Objective-C object
    #[cfg(all(target_os = "macos", feature = "macos"))]
    unsafe fn release_obj(ptr: *mut c_void) {
        if !ptr.is_null() {
            let obj = ptr as *const AnyObject;
            let _: () = msg_send![obj, release];
        }
    }
}

impl Default for MacOSRenderer {
    fn default() -> Self {
        Self::new().expect("MacOSRenderer creation should not fail")
    }
}

impl WallpaperRenderer for MacOSRenderer {
    fn init(&mut self, _display_id: &DisplayId) -> Result<(), RenderError> {
        #[cfg(all(target_os = "macos", feature = "macos"))]
        {
            // Step 1: Get main screen frame
            let screen_frame = self.get_main_screen_frame()?;
            self.width = screen_frame.size.width as u32;
            self.height = screen_frame.size.height as u32;

            tracing::debug!(
                "Main screen frame: {}x{} at ({}, {})",
                self.width,
                self.height,
                screen_frame.origin.x,
                screen_frame.origin.y
            );

            // Step 2: Create the desktop-level window
            let window = self.create_window(screen_frame)?;
            self.window = Some(window);

            // Step 3: Create the image view
            let content_frame = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: screen_frame.size,
            };
            let image_view = self.create_image_view(content_frame)?;
            self.image_view = Some(image_view);

            // Step 4: Add image view to window's content view
            unsafe {
                let window_obj = window as *const AnyObject;
                let content_view: *const AnyObject = msg_send![window_obj, contentView];
                if !content_view.is_null() {
                    let _: () = msg_send![content_view, addSubview: image_view as *const AnyObject];
                }

                // Show the window
                let _: () = msg_send![window_obj, orderFront: std::ptr::null::<AnyObject>()];
            }

            self.initialized = true;
            tracing::info!(
                "macOS desktop window renderer initialized: {}x{}",
                self.width,
                self.height
            );

            Ok(())
        }

        #[cfg(not(all(target_os = "macos", feature = "macos")))]
        Err(RenderError::Platform(
            "macOS renderer not available on this platform".into(),
        ))
    }

    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        if !self.initialized {
            return Err(RenderError::Platform("Renderer not initialized".into()));
        }

        #[cfg(all(target_os = "macos", feature = "macos"))]
        {
            let image_view = self
                .image_view
                .ok_or_else(|| RenderError::Platform("No image view".into()))?;

            unsafe {
                autoreleasepool(|_| {
                    // Create NSImage from frame data
                    let image = self.create_image_from_frame(frame)?;

                    // Set the image on the image view
                    let image_view_obj = image_view as *const AnyObject;
                    let _: () = msg_send![image_view_obj, setImage: image];

                    // The image view now owns the image, but we should release our reference
                    let _: () = msg_send![image, release];

                    Ok(())
                })
            }
        }

        #[cfg(not(all(target_os = "macos", feature = "macos")))]
        {
            let _ = frame;
            Err(RenderError::Platform(
                "macOS renderer not available on this platform".into(),
            ))
        }
    }

    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        // On macOS, closing our window reveals the system wallpaper underneath
        tracing::debug!("Restoring macOS wallpaper (hiding render window)");

        #[cfg(all(target_os = "macos", feature = "macos"))]
        if let Some(window) = self.window {
            unsafe {
                let window_obj = window as *const AnyObject;
                let _: () = msg_send![window_obj, orderOut: std::ptr::null::<AnyObject>()];
            }
        }

        Ok(())
    }

    fn shutdown(&mut self) {
        #[cfg(all(target_os = "macos", feature = "macos"))]
        {
            // Release the image view
            if let Some(view) = self.image_view.take() {
                unsafe {
                    Self::release_obj(view);
                }
            }

            // Close and release the window
            if let Some(window) = self.window.take() {
                unsafe {
                    let window_obj = window as *const AnyObject;
                    let _: () = msg_send![window_obj, close];
                    Self::release_obj(window);
                }
            }
        }

        self.initialized = false;
        tracing::info!("macOS desktop window renderer shutdown complete");
    }
}

// ============================================================================
// Helper Types
// ============================================================================

/// NSPoint structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(all(target_os = "macos", feature = "macos"))]
struct NSPoint {
    x: f64,
    y: f64,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for NSPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
}

/// NSSize structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(all(target_os = "macos", feature = "macos"))]
struct NSSize {
    width: f64,
    height: f64,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for NSSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
}

/// NSRect structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[cfg(all(target_os = "macos", feature = "macos"))]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe impl Encode for NSRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[NSPoint::ENCODING, NSSize::ENCODING]);
}

/// Create an NSString from a Rust string
#[cfg(all(target_os = "macos", feature = "macos"))]
unsafe fn nsstring(s: &str) -> *const AnyObject {
    let cstring = std::ffi::CString::new(s).unwrap();
    msg_send![class!(NSString), stringWithUTF8String: cstring.as_ptr()]
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

        let renderer = renderer.unwrap();
        assert!(!renderer.initialized);
    }

    #[test]
    fn test_renderer_shutdown_without_init() {
        let mut renderer = MacOSRenderer::new().unwrap();
        // Should not panic
        renderer.shutdown();
        assert!(!renderer.initialized);
    }

    #[test]
    fn test_render_without_init() {
        let mut renderer = MacOSRenderer::new().unwrap();
        let frame = ProcessedFrame::new(vec![0u8; 100 * 100 * 4], 100, 100);

        let result = renderer.render(&frame);
        assert!(result.is_err());
    }

    // Full integration tests require macOS and a desktop session
    // Run with: cargo test --features macos -- --ignored

    #[test]
    #[ignore = "requires macOS desktop session"]
    #[cfg(all(target_os = "macos", feature = "macos"))]
    fn test_desktop_window_init_and_render() {
        let mut renderer = MacOSRenderer::new().unwrap();

        // Initialize
        let result = renderer.init(&DisplayId("test".to_string()));
        if result.is_err() {
            eprintln!("macOS init failed (may need desktop session): {:?}", result);
            return;
        }

        assert!(renderer.initialized);
        assert!(renderer.width > 0);
        assert!(renderer.height > 0);

        // Create a test frame (gradient)
        let width = 800;
        let height = 600;
        let mut data = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                data[idx] = (x * 255 / width) as u8; // R
                data[idx + 1] = (y * 255 / height) as u8; // G
                data[idx + 2] = 128; // B
                data[idx + 3] = 255; // A
            }
        }

        let frame = ProcessedFrame::new(data, width as u32, height as u32);

        // Render
        let result = renderer.render(&frame);
        assert!(result.is_ok());

        // Allow some time to see the result (in interactive testing)
        std::thread::sleep(std::time::Duration::from_millis(1000));

        // Cleanup
        renderer.shutdown();
        assert!(!renderer.initialized);
    }
}
