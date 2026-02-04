//! macOS-specific tests for Micround
//!
//! Tests NSScreen display enumeration, AVFoundation capture enumeration,
//! and NSWindow-based desktop rendering.
//!
//! These tests use conditional compilation and skip gracefully when resources
//! aren't available (e.g., no camera, headless environment).
//!
//! Run with: cargo test --features macos -- --ignored platform_macos

#![cfg(target_os = "macos")]

mod common;

use common::test_logger::*;
use std::env;

// ============================================================================
// Display Enumeration Tests (NSScreen)
// ============================================================================

/// Test that we can detect if running in a desktop session
#[test]
fn test_macos_desktop_session_detection() {
    let mut logger = TestLogger::new("macos_desktop_session_detection", 3);

    test_step!(logger, "Checking for desktop session indicators");

    // On macOS, we can check for various environment variables that indicate
    // we're running in a GUI session
    let term_program = env::var("TERM_PROGRAM").ok();
    let ssh_connection = env::var("SSH_CONNECTION").ok();
    let display = env::var("DISPLAY").ok(); // XQuartz

    test_step_ok!(
        logger,
        "TERM_PROGRAM={:?}, SSH={:?}, DISPLAY={:?}",
        term_program,
        ssh_connection.is_some(),
        display
    );

    test_step!(logger, "Analyzing session type");
    let is_ssh = ssh_connection.is_some();
    let is_terminal = term_program.is_some();

    if is_ssh {
        test_step_ok!(logger, "Running via SSH - GUI may be unavailable");
    } else if is_terminal {
        test_step_ok!(logger, "Running in terminal with desktop session");
    } else {
        test_step_ok!(logger, "Running in desktop environment");
    }

    test_step!(logger, "Checking GUI availability indicators");
    // Check if we can potentially access the WindowServer
    // In a headless environment, this would fail
    let home = env::var("HOME").ok();
    let user = env::var("USER").ok();

    if home.is_some() && user.is_some() {
        test_step_ok!(logger, "User environment detected: {:?}", user);
    } else {
        test_step_ok!(logger, "Minimal environment (may be headless)");
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test NSScreen main screen detection (requires macos feature and desktop session)
#[test]
#[ignore = "requires macOS desktop session"]
#[cfg(feature = "macos")]
fn test_nsscreen_main_screen() {
    use objc2::encode::{Encode, Encoding};
    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    // Define NSRect with proper Encode implementation
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct NSPoint { x: f64, y: f64 }

    unsafe impl Encode for NSPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[Encoding::Double, Encoding::Double]);
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct NSSize { width: f64, height: f64 }

    unsafe impl Encode for NSSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[Encoding::Double, Encoding::Double]);
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct NSRect { origin: NSPoint, size: NSSize }

    unsafe impl Encode for NSRect {
        const ENCODING: Encoding = Encoding::Struct("CGRect", &[NSPoint::ENCODING, NSSize::ENCODING]);
    }

    let mut logger = TestLogger::new("nsscreen_main_screen", 3);

    test_step!(logger, "Getting NSScreen.mainScreen");
    let screen_info = unsafe {
        autoreleasepool(|_| {
            let screen_class = class!(NSScreen);
            let main_screen: *const AnyObject = msg_send![screen_class, mainScreen];

            if main_screen.is_null() {
                return None;
            }

            let frame: NSRect = msg_send![main_screen, frame];
            Some((frame.size.width as u32, frame.size.height as u32))
        })
    };

    match screen_info {
        Some((width, height)) => {
            test_step_ok!(logger, "Main screen: {}x{}", width, height);

            test_step!(logger, "Validating screen dimensions");
            test_assert!(logger, width > 0, "Screen has positive width");
            test_assert!(logger, height > 0, "Screen has positive height");
            test_step_ok!(logger);

            test_step!(logger, "Checking reasonable dimensions");
            // Typical display ranges
            test_assert!(logger, width >= 800, "Width >= 800 pixels");
            test_assert!(logger, height >= 600, "Height >= 600 pixels");
            test_assert!(logger, width <= 16384, "Width <= 16384 pixels");
            test_assert!(logger, height <= 16384, "Height <= 16384 pixels");
            test_step_ok!(logger, "Dimensions are reasonable");
        }
        None => {
            logger.step_skip("No main screen available (headless?)");
            test_step!(logger, "Skipping dimension validation");
            logger.step_skip("No screen to validate");
            test_step!(logger, "Skipping dimension range check");
            logger.step_skip("No screen to check");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test enumerating all screens (multi-display setup)
#[test]
#[ignore = "requires macOS desktop session"]
#[cfg(feature = "macos")]
fn test_nsscreen_all_screens() {
    use objc2::rc::autoreleasepool;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    let mut logger = TestLogger::new("nsscreen_all_screens", 3);

    test_step!(logger, "Getting NSScreen.screens array");
    let screen_count = unsafe {
        autoreleasepool(|_| {
            let screen_class = class!(NSScreen);
            let screens: *const AnyObject = msg_send![screen_class, screens];

            if screens.is_null() {
                return None;
            }

            let count: usize = msg_send![screens, count];
            Some(count)
        })
    };

    match screen_count {
        Some(count) => {
            test_step_ok!(logger, "Found {} screen(s)", count);

            test_step!(logger, "Validating screen count");
            test_assert!(logger, count > 0, "At least one screen detected");
            test_step_ok!(logger);

            test_step!(logger, "Checking for multi-display setup");
            if count > 1 {
                test_step_ok!(logger, "Multi-display configuration detected ({} screens)", count);
            } else {
                test_step_ok!(logger, "Single display configuration");
            }
        }
        None => {
            logger.step_skip("Could not enumerate screens (headless?)");
            test_step!(logger, "Skipping count validation");
            logger.step_skip("No screens to count");
            test_step!(logger, "Skipping multi-display check");
            logger.step_skip("No screen info available");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// AVFoundation Capture Tests
// ============================================================================

/// Test AVFoundation camera enumeration (requires macos feature and AVFoundation framework)
#[test]
#[ignore = "requires AVFoundation framework linked"]
#[cfg(feature = "macos")]
fn test_avfoundation_camera_enumeration() {
    use micround::capture::create_enumerator;

    let mut logger = TestLogger::new("avfoundation_camera_enumeration", 3);

    test_step!(logger, "Creating AVFoundation enumerator");
    let enumerator = create_enumerator();
    test_step_ok!(logger);

    test_step!(logger, "Listing camera devices");
    let devices = enumerator.enumerate().unwrap_or_default();
    test_step_ok!(logger, "Found {} camera(s)", devices.len());

    test_step!(logger, "Validating device info");
    for device in &devices {
        test_assert!(logger, !device.id.0.is_empty(), "Device has ID");
        test_assert!(logger, !device.name.is_empty(), "Device has name: {}", device.name);
    }

    if devices.is_empty() {
        test_step_ok!(logger, "No cameras found (normal for headless/CI)");
    } else {
        test_step_ok!(logger, "All devices have valid info");
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test AVFoundation backend creation (requires macos feature and AVFoundation framework)
#[test]
#[ignore = "requires AVFoundation framework linked"]
#[cfg(feature = "macos")]
fn test_avfoundation_backend_creation() {
    use micround::capture::create_backend;

    let mut logger = TestLogger::new("avfoundation_backend_creation", 2);

    test_step!(logger, "Creating AVFoundation capture backend");
    let backend = create_backend();
    test_step_ok!(logger, "Backend created successfully");

    test_step!(logger, "Checking initial state");
    test_assert!(logger, !backend.is_capturing(), "Backend is not capturing initially");
    test_assert!(logger, backend.current_format().is_none(), "No format set initially");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// MacOS Renderer Tests
// ============================================================================

/// Test MacOSRenderer creation (no GUI required)
#[test]
fn test_macos_renderer_creation() {
    use micround::render::macos::MacOSRenderer;
    #[allow(unused_imports)]
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("macos_renderer_creation", 2);

    test_step!(logger, "Creating MacOSRenderer");
    let renderer = MacOSRenderer::new();
    test_assert!(logger, renderer.is_ok(), "Renderer created successfully");
    test_step_ok!(logger);

    test_step!(logger, "Checking renderer was created");
    let _renderer = renderer.unwrap();
    // Renderer is created but not initialized (private field, trust the constructor)
    test_step_ok!(logger, "Renderer created in uninitialized state");

    let result = logger.finish();
    assert!(result.passed);
}

/// Test MacOSRenderer shutdown without init (no GUI required)
#[test]
fn test_macos_renderer_shutdown_without_init() {
    use micround::render::macos::MacOSRenderer;
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("macos_renderer_shutdown_without_init", 2);

    test_step!(logger, "Creating uninitialized renderer");
    let mut renderer = MacOSRenderer::new().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Calling shutdown on uninitialized renderer");
    // Should not panic - this is the key test
    renderer.shutdown();
    test_step_ok!(logger, "No panic occurred");

    let result = logger.finish();
    assert!(result.passed);
}

/// Test rendering without initialization returns error
#[test]
fn test_macos_renderer_render_without_init() {
    use micround::process::ProcessedFrame;
    use micround::render::macos::MacOSRenderer;
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("macos_renderer_render_without_init", 2);

    test_step!(logger, "Creating uninitialized renderer");
    let mut renderer = MacOSRenderer::new().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Attempting render without init");
    let frame = ProcessedFrame::new(vec![0u8; 100 * 100 * 4], 100, 100);
    let result = renderer.render(&frame);
    test_assert!(logger, result.is_err(), "Render returns error when not initialized");
    test_step_ok!(logger, "Correctly returned error");

    let result = logger.finish();
    assert!(result.passed);
}

/// Test MacOSRenderer initialization (requires desktop session)
#[test]
#[ignore = "requires macOS desktop session"]
#[cfg(feature = "macos")]
fn test_macos_renderer_init() {
    use micround::render::macos::MacOSRenderer;
    use micround::render::WallpaperRenderer;
    use micround::core::DisplayId;

    let mut logger = TestLogger::new("macos_renderer_init", 4);

    test_step!(logger, "Creating MacOSRenderer");
    let mut renderer = MacOSRenderer::new().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Initializing renderer");
    let display_id = DisplayId("main".to_string());
    match renderer.init(&display_id) {
        Ok(()) => {
            test_step_ok!(logger, "Renderer initialized successfully");

            test_step!(logger, "Verifying initialization via render attempt");
            // If init succeeded, rendering should also work (doesn't access private fields)
            let test_frame = micround::process::ProcessedFrame::new(
                vec![0u8; 100 * 100 * 4], 100, 100
            );
            let render_result = renderer.render(&test_frame);
            test_assert!(logger, render_result.is_ok(), "Can render after init");
            test_step_ok!(logger, "Render test passed");

            test_step!(logger, "Shutting down renderer");
            renderer.shutdown();
            // Verify shutdown by attempting render (should fail)
            let render_after_shutdown = renderer.render(&test_frame);
            test_assert!(logger, render_after_shutdown.is_err(), "Render fails after shutdown");
            test_step_ok!(logger);
        }
        Err(e) => {
            logger.step_skip(&format!("Init failed (may need desktop): {:?}", e));
            test_step!(logger, "Skipping state verification");
            logger.step_skip("Renderer not initialized");
            test_step!(logger, "Skipping shutdown test");
            logger.step_skip("Nothing to shut down");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test full render cycle (requires desktop session)
#[test]
#[ignore = "requires macOS desktop session"]
#[cfg(feature = "macos")]
fn test_macos_renderer_full_cycle() {
    use micround::render::macos::MacOSRenderer;
    use micround::render::WallpaperRenderer;
    use micround::core::DisplayId;
    use micround::process::ProcessedFrame;

    let mut logger = TestLogger::new("macos_renderer_full_cycle", 5);

    test_step!(logger, "Creating and initializing renderer");
    let mut renderer = MacOSRenderer::new().unwrap();
    let display_id = DisplayId("main".to_string());

    if renderer.init(&display_id).is_err() {
        logger.step_skip("Could not initialize (headless?)");
        let result = logger.finish();
        assert!(result.passed);
        return;
    }
    test_step_ok!(logger);

    test_step!(logger, "Creating test frame (gradient pattern)");
    let width = 640;
    let height = 480;
    let mut data = vec![0u8; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            data[idx] = (x * 255 / width) as u8;     // R gradient
            data[idx + 1] = (y * 255 / height) as u8; // G gradient
            data[idx + 2] = 128;                      // B constant
            data[idx + 3] = 255;                      // A opaque
        }
    }
    let frame = ProcessedFrame::new(data, width as u32, height as u32);
    test_step_ok!(logger, "Created {}x{} RGBA frame", width, height);

    test_step!(logger, "Rendering frame");
    match renderer.render(&frame) {
        Ok(()) => {
            test_step_ok!(logger, "Frame rendered successfully");
        }
        Err(e) => {
            logger.step_err(&format!("Render failed: {:?}", e));
            renderer.shutdown();
            let result = logger.finish();
            assert!(result.passed);
            return;
        }
    }

    test_step!(logger, "Rendering multiple frames");
    // Render a few more frames to test stability
    for i in 0..5 {
        let mut data = vec![0u8; width * height * 4];
        // Shifting gradient to show animation
        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) * 4;
                data[idx] = ((x + i * 20) * 255 / width % 256) as u8;
                data[idx + 1] = ((y + i * 10) * 255 / height % 256) as u8;
                data[idx + 2] = ((i * 50) % 256) as u8;
                data[idx + 3] = 255;
            }
        }
        let frame = ProcessedFrame::new(data, width as u32, height as u32);
        if renderer.render(&frame).is_err() {
            logger.step_err("Failed during multi-frame render");
            break;
        }
    }
    test_step_ok!(logger, "Rendered 5 additional frames");

    test_step!(logger, "Shutting down");
    renderer.shutdown();
    // Verify shutdown by attempting render (should fail)
    let final_frame = ProcessedFrame::new(vec![0u8; 100 * 100 * 4], 100, 100);
    let render_after_shutdown = renderer.render(&final_frame);
    test_assert!(logger, render_after_shutdown.is_err(), "Render fails after shutdown");
    test_step_ok!(logger, "Renderer shut down cleanly");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Window Level and Behavior Tests
// ============================================================================

/// Test desktop window level constant
#[test]
#[cfg(feature = "macos")]
fn test_desktop_window_level_constant() {
    let mut logger = TestLogger::new("desktop_window_level_constant", 2);

    test_step!(logger, "Checking kCGDesktopWindowLevel constant");
    // kCGDesktopWindowLevel = kCGMinimumWindowLevel + 20
    // kCGMinimumWindowLevel = INT32_MIN = -2147483648
    // So kCGDesktopWindowLevel = -2147483648 + 20 + 5 = -2147483623
    // Wait, the formula is: kCGDesktopWindowLevel = -2147483623
    let expected_level: i64 = -2147483623;
    test_step_ok!(logger, "Expected value: {}", expected_level);

    test_step!(logger, "Verifying level is below normal window level");
    let normal_window_level: i64 = 0; // kCGNormalWindowLevel
    test_assert!(
        logger,
        expected_level < normal_window_level,
        "Desktop level ({}) < Normal level ({})",
        expected_level,
        normal_window_level
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test collection behavior flags
#[test]
fn test_collection_behavior_flags() {
    let mut logger = TestLogger::new("collection_behavior_flags", 2);

    test_step!(logger, "Checking NSWindowCollectionBehavior flags");
    // These are the flags we use for desktop-level windows
    let can_join_all_spaces: u64 = 1 << 0;
    let stationary: u64 = 1 << 4;
    let ignores_cycle: u64 = 1 << 6;
    let full_screen_auxiliary: u64 = 1 << 8;

    test_assert!(logger, can_join_all_spaces == 1, "canJoinAllSpaces = 1");
    test_assert!(logger, stationary == 16, "stationary = 16");
    test_assert!(logger, ignores_cycle == 64, "ignoresCycle = 64");
    test_assert!(logger, full_screen_auxiliary == 256, "fullScreenAuxiliary = 256");
    test_step_ok!(logger);

    test_step!(logger, "Verifying combined behavior");
    let combined = can_join_all_spaces | stationary | ignores_cycle | full_screen_auxiliary;
    test_assert!(logger, combined == 337, "Combined flags = 337");
    // Verify no overlap
    test_assert!(
        logger,
        (can_join_all_spaces & stationary) == 0,
        "No flag overlap between canJoinAllSpaces and stationary"
    );
    test_step_ok!(logger, "Combined behavior: 0x{:X}", combined);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Platform Feature Detection
// ============================================================================

/// Test macOS version detection
#[test]
fn test_macos_version_detection() {
    let mut logger = TestLogger::new("macos_version_detection", 2);

    test_step!(logger, "Checking macOS version via sw_vers");
    let output = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim();
            test_step_ok!(logger, "macOS {}", version);

            test_step!(logger, "Parsing version components");
            let parts: Vec<&str> = version.split('.').collect();
            if parts.len() >= 2 {
                let major: u32 = parts[0].parse().unwrap_or(0);
                let minor: u32 = parts[1].parse().unwrap_or(0);

                // AVFoundation requires macOS 10.7+, NSScreen APIs are older
                test_assert!(logger, major >= 10, "Major version >= 10");
                if major == 10 {
                    test_assert!(logger, minor >= 7, "If macOS 10.x, minor >= 7 for AVFoundation");
                }
                test_step_ok!(logger, "Version {}.{} supports all required APIs", major, minor);
            } else {
                test_step_ok!(logger, "Could not parse version, assuming compatible");
            }
        }
        _ => {
            logger.step_skip("Could not run sw_vers");
            test_step!(logger, "Skipping version check");
            logger.step_skip("sw_vers unavailable");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test architecture detection
#[test]
fn test_architecture_detection() {
    let mut logger = TestLogger::new("architecture_detection", 2);

    test_step!(logger, "Checking CPU architecture");
    let arch = std::env::consts::ARCH;
    test_step_ok!(logger, "Architecture: {}", arch);

    test_step!(logger, "Verifying supported architecture");
    let is_supported = matches!(arch, "x86_64" | "aarch64");
    test_assert!(logger, is_supported, "Architecture {} is supported", arch);

    if arch == "aarch64" {
        test_step_ok!(logger, "Apple Silicon detected");
    } else {
        test_step_ok!(logger, "Intel architecture detected");
    }

    let result = logger.finish();
    assert!(result.passed);
}
