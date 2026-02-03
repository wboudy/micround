//! Linux-specific tests for Micround
//!
//! Tests V4L2 device access, X11 root window operations, and Wayland detection.
//! These tests use conditional compilation and skip gracefully when resources
//! aren't available (e.g., no video device, no display server).
//!
//! Run with: cargo test --features linux -- --ignored platform_linux

#![cfg(target_os = "linux")]

mod common;

use common::test_logger::*;
use std::env;
use std::fs;
use std::path::Path;

// ============================================================================
// V4L2 Device Access Tests
// ============================================================================

/// Test that we can enumerate /dev/video* devices
#[test]
fn test_v4l2_device_enumeration() {
    let mut logger = TestLogger::new("v4l2_device_enumeration", 3);

    test_step!(logger, "Scanning /dev for video devices");
    let video_devices: Vec<_> = fs::read_dir("/dev")
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|s| s.starts_with("video"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();

    test_step_ok!(logger, "Found {} video device(s)", video_devices.len());

    test_step!(logger, "Checking device paths");
    for device in &video_devices {
        let path = device.path();
        test_assert!(logger, path.exists(), "Device exists: {:?}", path);
    }
    test_step_ok!(logger, "All device paths validated");

    test_step!(logger, "Verifying /dev/video0 presence (if any devices)");
    if video_devices.is_empty() {
        logger.step_skip("No video devices found - skipping detailed checks");
    } else {
        // Check if at least video0 exists when we have devices
        let video0 = Path::new("/dev/video0");
        if video0.exists() {
            test_step_ok!(logger, "/dev/video0 exists");
        } else {
            test_step_ok!(logger, "/dev/video0 not found, but other devices exist");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test video device permissions (user should be in video group)
#[test]
fn test_v4l2_device_permissions() {
    let mut logger = TestLogger::new("v4l2_device_permissions", 3);

    test_step!(logger, "Checking if /dev/video0 exists");
    let video0 = Path::new("/dev/video0");
    if !video0.exists() {
        logger.step_skip("No video device available for permission test");
        let result = logger.finish();
        assert!(result.passed);
        return;
    }
    test_step_ok!(logger);

    test_step!(logger, "Checking device metadata");
    let metadata = fs::metadata(video0);
    test_assert!(logger, metadata.is_ok(), "Can read device metadata");
    test_step_ok!(logger);

    test_step!(logger, "Verifying read access");
    // Try to open the device for reading
    // This will fail if user is not in video group
    let can_read = fs::File::open(video0).is_ok();
    if can_read {
        test_step_ok!(logger, "User has read access to video device");
    } else {
        // Not a failure - just a configuration note
        logger.step_skip("No read access (user may not be in 'video' group)");
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test V4L2 backend device enumeration (requires linux feature)
#[test]
#[cfg(feature = "linux")]
fn test_v4l2_backend_enumeration() {
    use micround::capture::{create_enumerator, CameraEnumerator};

    let mut logger = TestLogger::new("v4l2_backend_enumeration", 3);

    test_step!(logger, "Creating V4L2 enumerator");
    let enumerator = create_enumerator();
    test_step_ok!(logger);

    test_step!(logger, "Listing camera devices");
    let devices = enumerator.enumerate().unwrap_or_default();
    test_step_ok!(logger, "Found {} camera(s)", devices.len());

    test_step!(logger, "Validating device info");
    for device in &devices {
        test_assert!(logger, !device.id.0.is_empty(), "Device has ID");
        test_assert!(logger, !device.name.is_empty(), "Device has name");
        // Capabilities may be empty if device is busy
    }
    test_step_ok!(logger, "All devices have valid info");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// X11 Root Window Tests
// ============================================================================

/// Test X11 display connection
#[test]
#[ignore = "requires X11 display"]
fn test_x11_display_connection() {
    let mut logger = TestLogger::new("x11_display_connection", 3);

    test_step!(logger, "Checking DISPLAY environment variable");
    let display_env = env::var("DISPLAY");
    if display_env.is_err() {
        logger.step_skip("DISPLAY not set - not running under X11");
        let result = logger.finish();
        assert!(result.passed);
        return;
    }
    test_step_ok!(logger, "DISPLAY={}", display_env.unwrap());

    test_step!(logger, "Connecting to X11 server");
    #[cfg(feature = "linux")]
    {
        use x11rb::connect;
        use x11rb::connection::Connection;

        match connect(None) {
            Ok((conn, screen_num)) => {
                test_step_ok!(logger, "Connected to screen {}", screen_num);

                test_step!(logger, "Querying screen info");
                let setup = conn.setup();
                let screen = &setup.roots[screen_num];
                test_assert!(logger, screen.width_in_pixels > 0, "Screen has width");
                test_assert!(logger, screen.height_in_pixels > 0, "Screen has height");
                test_step_ok!(
                    logger,
                    "Screen: {}x{} pixels",
                    screen.width_in_pixels,
                    screen.height_in_pixels
                );
            }
            Err(e) => {
                logger.step_skip(&format!("Could not connect to X11: {}", e));
            }
        }
    }

    #[cfg(not(feature = "linux"))]
    {
        logger.step_skip("X11 support not compiled in");
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test X11 root window access
#[test]
#[ignore = "requires X11 display"]
#[cfg(feature = "linux")]
fn test_x11_root_window_access() {
    use x11rb::connect;
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::ConnectionExt;

    let mut logger = TestLogger::new("x11_root_window_access", 4);

    test_step!(logger, "Connecting to X11");
    let (conn, screen_num) = match connect(None) {
        Ok(c) => c,
        Err(e) => {
            logger.step_skip(&format!("No X11 connection: {}", e));
            let result = logger.finish();
            assert!(result.passed);
            return;
        }
    };
    test_step_ok!(logger);

    test_step!(logger, "Getting root window");
    let setup = conn.setup();
    let screen = &setup.roots[screen_num];
    let root = screen.root;
    test_assert!(logger, root != 0, "Root window ID is valid");
    test_step_ok!(logger, "Root window ID: {}", root);

    test_step!(logger, "Querying root window geometry");
    match conn.get_geometry(root) {
        Ok(cookie) => match cookie.reply() {
            Ok(geom) => {
                test_step_ok!(
                    logger,
                    "Root geometry: {}x{} at ({},{})",
                    geom.width,
                    geom.height,
                    geom.x,
                    geom.y
                );
            }
            Err(e) => {
                logger.step_skip(&format!("Could not get geometry reply: {}", e));
            }
        },
        Err(e) => {
            logger.step_skip(&format!("Could not query geometry: {}", e));
        }
    }

    test_step!(logger, "Checking desktop type atom");
    match conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE_DESKTOP") {
        Ok(cookie) => match cookie.reply() {
            Ok(reply) => {
                test_assert!(logger, reply.atom != 0, "Desktop atom exists");
                test_step_ok!(logger, "Desktop atom ID: {}", reply.atom);
            }
            Err(e) => {
                logger.step_skip(&format!("Could not get atom reply: {}", e));
            }
        },
        Err(e) => {
            logger.step_skip(&format!("Could not intern atom: {}", e));
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test X11 renderer initialization
#[test]
#[ignore = "requires X11 display"]
#[cfg(feature = "linux")]
fn test_x11_renderer_init() {
    use micround::core::DisplayId;
    use micround::render::linux::X11Renderer;
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("x11_renderer_init", 3);

    test_step!(logger, "Creating X11 renderer");
    let renderer = X11Renderer::new();
    test_assert!(logger, renderer.is_ok(), "Renderer created");
    let mut renderer = renderer.unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Initializing renderer");
    let display_id = DisplayId("test".to_string());
    match renderer.init(&display_id) {
        Ok(()) => {
            test_step_ok!(logger, "Renderer initialized successfully");

            test_step!(logger, "Shutting down renderer");
            renderer.shutdown();
            test_step_ok!(logger);
        }
        Err(e) => {
            logger.step_skip(&format!("Could not initialize (may need display): {:?}", e));
            test_step!(logger, "Skipping shutdown");
            logger.step_skip("Renderer not initialized");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Wayland Detection Tests
// ============================================================================

/// Test Wayland session detection
#[test]
fn test_wayland_session_detection() {
    let mut logger = TestLogger::new("wayland_session_detection", 3);

    test_step!(logger, "Checking XDG_SESSION_TYPE");
    let session_type = env::var("XDG_SESSION_TYPE");
    match &session_type {
        Ok(t) => test_step_ok!(logger, "XDG_SESSION_TYPE={}", t),
        Err(_) => test_step_ok!(logger, "XDG_SESSION_TYPE not set"),
    }

    test_step!(logger, "Checking WAYLAND_DISPLAY");
    let wayland_display = env::var("WAYLAND_DISPLAY");
    match &wayland_display {
        Ok(d) => test_step_ok!(logger, "WAYLAND_DISPLAY={}", d),
        Err(_) => test_step_ok!(logger, "WAYLAND_DISPLAY not set (not Wayland session)"),
    }

    test_step!(logger, "Determining session type");
    let is_wayland = session_type
        .as_ref()
        .map(|s| s == "wayland")
        .unwrap_or(false)
        || wayland_display.is_ok();
    let is_x11 =
        session_type.as_ref().map(|s| s == "x11").unwrap_or(false) || env::var("DISPLAY").is_ok();

    if is_wayland && is_x11 {
        test_step_ok!(
            logger,
            "Running under XWayland (both Wayland and X11 available)"
        );
    } else if is_wayland {
        test_step_ok!(logger, "Running under pure Wayland");
    } else if is_x11 {
        test_step_ok!(logger, "Running under X11");
    } else {
        test_step_ok!(logger, "No display server detected (headless/TTY)");
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test Wayland socket detection
#[test]
fn test_wayland_socket_detection() {
    let mut logger = TestLogger::new("wayland_socket_detection", 2);

    test_step!(logger, "Checking for Wayland runtime dir");
    let runtime_dir = env::var("XDG_RUNTIME_DIR");
    match &runtime_dir {
        Ok(dir) => {
            test_step_ok!(logger, "XDG_RUNTIME_DIR={}", dir);

            test_step!(logger, "Looking for Wayland sockets");
            let wayland_socket = Path::new(dir).join("wayland-0");
            let wayland_display = env::var("WAYLAND_DISPLAY")
                .map(|d| Path::new(dir).join(&d))
                .ok();

            if wayland_socket.exists() {
                test_step_ok!(logger, "Found wayland-0 socket");
            } else if let Some(ref custom) = wayland_display {
                if custom.exists() {
                    test_step_ok!(logger, "Found custom Wayland socket: {:?}", custom);
                } else {
                    test_step_ok!(logger, "No Wayland sockets found (not running Wayland)");
                }
            } else {
                test_step_ok!(logger, "No Wayland sockets found (not running Wayland)");
            }
        }
        Err(_) => {
            test_step_ok!(logger, "XDG_RUNTIME_DIR not set");
            test_step!(logger, "Skipping socket check");
            logger.step_skip("Cannot check sockets without runtime dir");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Server Detection (Combined)
// ============================================================================

/// Test comprehensive display server detection
#[test]
fn test_display_server_detection() {
    let mut logger = TestLogger::new("display_server_detection", 4);

    test_step!(logger, "Gathering environment info");
    let display = env::var("DISPLAY").ok();
    let wayland_display = env::var("WAYLAND_DISPLAY").ok();
    let session_type = env::var("XDG_SESSION_TYPE").ok();
    let desktop = env::var("XDG_CURRENT_DESKTOP").ok();
    test_step_ok!(logger);

    test_step!(logger, "Analyzing display server configuration");
    let report = format!(
        "DISPLAY={:?}, WAYLAND_DISPLAY={:?}, SESSION_TYPE={:?}, DESKTOP={:?}",
        display, wayland_display, session_type, desktop
    );
    test_step_ok!(logger, "{}", report);

    test_step!(logger, "Determining best rendering strategy");
    let strategy = if wayland_display.is_some() && display.is_some() {
        "XWayland - use X11 backend with XWayland compositor"
    } else if wayland_display.is_some() {
        "Pure Wayland - wallpaper requires Wayland-specific approach (not yet implemented)"
    } else if display.is_some() {
        "X11 - use X11 root window rendering"
    } else {
        "Headless - no display server available"
    };
    test_step_ok!(logger, "{}", strategy);

    test_step!(logger, "Checking desktop environment");
    match desktop {
        Some(de) => {
            let de_lower = de.to_lowercase();
            let notes = if de_lower.contains("gnome") {
                "GNOME: May need special handling for Mutter compositor"
            } else if de_lower.contains("kde") || de_lower.contains("plasma") {
                "KDE: Plasma shell manages desktop, may need plugin approach"
            } else if de_lower.contains("xfce") {
                "XFCE: xfdesktop manages desktop, root window may work"
            } else {
                "Unknown DE: Try standard X11 approach first"
            };
            test_step_ok!(logger, "{} - {}", de, notes);
        }
        None => {
            test_step_ok!(logger, "No desktop environment detected");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}
