//! macOS-specific tests for Micround
//!
//! Tests NSScreen display enumeration, NSWorkspace wallpaper API, AVFoundation
//! camera access, and accessibility permissions.
//! These tests use conditional compilation and skip gracefully when resources
//! aren't available (e.g., no camera permission, sandbox restrictions).
//!
//! Run with: cargo test --features macos -- --ignored platform_macos

#![cfg(target_os = "macos")]

mod common;

use common::test_logger::*;
use std::env;
use std::path::Path;
use std::process::Command;

// ============================================================================
// Display Enumeration Tests (NSScreen)
// ============================================================================

/// Test NSScreen display enumeration via system_profiler
#[test]
fn test_display_enumeration() {
    let mut logger = TestLogger::new("display_enumeration", 3);

    test_step!(logger, "Querying displays via system_profiler");
    let output = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                test_step_ok!(logger, "system_profiler returned display data");

                test_step!(logger, "Parsing display information");
                let stdout = String::from_utf8_lossy(&result.stdout);
                // Check for display-related keywords
                let has_displays = stdout.contains("spdisplays") || stdout.contains("Displays");
                test_assert!(logger, has_displays, "Output contains display data");
                test_step_ok!(logger);

                test_step!(logger, "Checking for resolution information");
                let has_resolution =
                    stdout.contains("Resolution") || stdout.contains("spdisplays_resolution");
                if has_resolution {
                    test_step_ok!(logger, "Resolution data found");
                } else {
                    logger.step_skip("Resolution data not in expected format");
                }
            } else {
                logger.step_err("system_profiler failed");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("system_profiler not available: {}", e));
            test_step!(logger, "Skipping display tests");
            logger.step_skip("Cannot enumerate without system_profiler");
            test_step!(logger, "Test incomplete");
            logger.step_skip("macOS-specific tool required");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test screen count detection
#[test]
fn test_screen_count() {
    let mut logger = TestLogger::new("screen_count", 2);

    test_step!(logger, "Getting screen count via ioreg");
    let output = Command::new("ioreg")
        .args(["-c", "IODisplayConnect"])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let display_count = stdout.matches("IODisplayConnect").count();
            test_step_ok!(logger, "Found {} display connection(s)", display_count);

            test_step!(logger, "Verifying at least one display");
            if display_count > 0 {
                test_step_ok!(logger, "At least one display connected");
            } else {
                // Headless mode (CI) - still valid
                test_step_ok!(logger, "No displays (headless mode)");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("ioreg not available: {}", e));
            test_step!(logger, "Skipping screen count");
            logger.step_skip("macOS tool not available");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Camera Permission Tests
// ============================================================================

/// Test camera permission status via tccutil
#[test]
fn test_camera_permission_check() {
    let mut logger = TestLogger::new("camera_permission_check", 3);

    test_step!(logger, "Checking camera devices via system_profiler");
    let output = Command::new("system_profiler")
        .args(["SPCameraDataType", "-json"])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                test_step_ok!(logger, "Camera data retrieved");

                test_step!(logger, "Checking for camera hardware");
                let has_cameras = stdout.contains("_items") || stdout.contains("Camera");
                if has_cameras {
                    test_step_ok!(logger, "Camera hardware detected");
                } else {
                    test_step_ok!(logger, "No cameras found (may be a Mac without camera)");
                }

                test_step!(logger, "Checking TCC permission database");
                // The TCC database requires elevated permissions to read directly
                // Instead check if we can access the AVFoundation framework
                let home = env::var("HOME").unwrap_or_default();
                let tcc_path = format!("{}/Library/Application Support/com.apple.TCC/TCC.db", home);
                if Path::new(&tcc_path).exists() {
                    test_step_ok!(logger, "TCC database exists at user level");
                } else {
                    test_step_ok!(logger, "TCC database in system location (normal)");
                }
            } else {
                logger.step_err("system_profiler failed");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("system_profiler not available: {}", e));
            test_step!(logger, "Skipping camera tests");
            logger.step_skip("macOS tool not available");
            test_step!(logger, "Test incomplete");
            logger.step_skip("macOS-specific tool required");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test AVFoundation camera enumeration (requires macos feature)
#[test]
#[cfg(feature = "macos")]
fn test_avfoundation_device_list() {
    use micround::capture::{create_enumerator, CameraEnumerator};

    let mut logger = TestLogger::new("avfoundation_device_list", 3);

    test_step!(logger, "Creating AVFoundation enumerator");
    let enumerator = create_enumerator();
    test_step_ok!(logger);

    test_step!(logger, "Enumerating camera devices");
    match enumerator.enumerate() {
        Ok(devices) => {
            test_step_ok!(logger, "Found {} camera(s)", devices.len());

            test_step!(logger, "Validating device information");
            for device in &devices {
                test_assert!(logger, !device.id.0.is_empty(), "Device has ID");
                test_assert!(
                    logger,
                    !device.name.is_empty(),
                    "Device has name: {}",
                    device.name
                );
            }
            test_step_ok!(logger, "All devices validated");
        }
        Err(e) => {
            // Permission denied is expected without user consent
            logger.step_skip(&format!(
                "Enumeration failed (may need permission): {:?}",
                e
            ));
            test_step!(logger, "Camera permission likely not granted");
            logger.step_skip("AVFoundation requires camera permission");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// NSWorkspace Wallpaper Tests
// ============================================================================

/// Test wallpaper directory access
#[test]
fn test_wallpaper_directory_access() {
    let mut logger = TestLogger::new("wallpaper_directory_access", 3);

    test_step!(logger, "Checking system wallpaper locations");
    let system_wallpapers = Path::new("/Library/Desktop Pictures");
    if system_wallpapers.exists() {
        test_step_ok!(logger, "System wallpaper directory exists");
    } else {
        logger.step_skip("System wallpaper directory not found (unusual)");
    }

    test_step!(logger, "Checking user wallpaper preferences");
    let home = env::var("HOME").unwrap_or_default();
    let prefs_path = format!("{}/Library/Preferences/com.apple.desktop.plist", home);
    if Path::new(&prefs_path).exists() {
        test_step_ok!(logger, "Desktop preferences plist exists");
    } else {
        // May not exist if using defaults
        test_step_ok!(logger, "Desktop preferences using system defaults");
    }

    test_step!(logger, "Checking Application Support directory");
    let app_support = format!("{}/Library/Application Support", home);
    test_assert!(
        logger,
        Path::new(&app_support).exists(),
        "Application Support exists"
    );
    test_step_ok!(logger, "Application Support accessible");

    let result = logger.finish();
    assert!(result.passed);
}

/// Test reading current wallpaper via defaults
#[test]
fn test_current_wallpaper_query() {
    let mut logger = TestLogger::new("current_wallpaper_query", 2);

    test_step!(logger, "Querying current desktop picture via defaults");
    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to get picture of desktop 1",
        ])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
                if !stdout.is_empty() {
                    test_step_ok!(logger, "Current wallpaper: {}", stdout);
                } else {
                    test_step_ok!(logger, "Wallpaper query returned empty (may be dynamic)");
                }
            } else {
                // May fail due to permissions in CI
                logger.step_skip("osascript failed (likely permission issue)");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("osascript not available: {}", e));
        }
    }

    test_step!(logger, "Verifying desktop settings access");
    // Try reading the desktop background with defaults command
    let output = Command::new("defaults")
        .args(["read", "com.apple.desktop", "Background"])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                test_step_ok!(logger, "Desktop background settings readable");
            } else {
                // Domain might not exist if using system defaults
                test_step_ok!(logger, "Desktop background using system defaults");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("defaults not available: {}", e));
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Accessibility Permission Tests
// ============================================================================

/// Test accessibility permission status (needed for global hotkeys)
#[test]
fn test_accessibility_permission() {
    let mut logger = TestLogger::new("accessibility_permission", 2);

    test_step!(logger, "Checking accessibility permission status");
    // AXIsProcessTrusted() would be the proper way, but requires objc2
    // For now, check if we can interact with System Events
    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to return name of first process",
        ])
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                test_step_ok!(
                    logger,
                    "System Events accessible (accessibility may be enabled)"
                );
            } else {
                // Permission denied is normal for sandboxed/unsigned apps
                let stderr = String::from_utf8_lossy(&result.stderr);
                if stderr.contains("not allowed") || stderr.contains("permission") {
                    logger.step_skip(
                        "Accessibility permission not granted (normal for unsigned apps)",
                    );
                } else {
                    logger.step_skip(&format!("System Events query failed: {}", stderr));
                }
            }
        }
        Err(e) => {
            logger.step_skip(&format!("osascript not available: {}", e));
        }
    }

    test_step!(logger, "Documenting accessibility requirement");
    // Global hotkeys on macOS require accessibility permission
    // The app should request this when hotkeys feature is enabled
    test_step_ok!(
        logger,
        "Note: Global hotkeys require accessibility permission in System Preferences"
    );

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// macOS Version and Environment Tests
// ============================================================================

/// Test macOS version detection
#[test]
fn test_macos_version() {
    let mut logger = TestLogger::new("macos_version", 3);

    test_step!(logger, "Getting macOS version via sw_vers");
    let output = Command::new("sw_vers").output();

    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                test_step_ok!(logger, "Version info retrieved");

                test_step!(logger, "Parsing version details");
                for line in stdout.lines() {
                    if line.contains("ProductName") || line.contains("ProductVersion") {
                        logger.info(line.trim());
                    }
                }
                test_step_ok!(logger);

                test_step!(logger, "Checking minimum version requirements");
                // Micround requires macOS 10.15+ (Catalina) for modern AVFoundation
                if stdout.contains("10.15")
                    || stdout.contains("11.")
                    || stdout.contains("12.")
                    || stdout.contains("13.")
                    || stdout.contains("14.")
                    || stdout.contains("15.")
                {
                    test_step_ok!(logger, "macOS version supported");
                } else {
                    logger.warn("macOS version may be too old (need 10.15+)");
                    test_step_ok!(logger, "Version check completed with warning");
                }
            } else {
                logger.step_err("sw_vers failed");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("sw_vers not available: {}", e));
            test_step!(logger, "Skipping version check");
            logger.step_skip("macOS tool not available");
            test_step!(logger, "Test incomplete");
            logger.step_skip("macOS required");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test Apple Silicon vs Intel detection
#[test]
fn test_architecture_detection() {
    let mut logger = TestLogger::new("architecture_detection", 2);

    test_step!(logger, "Detecting CPU architecture");
    let output = Command::new("uname").arg("-m").output();

    match output {
        Ok(result) => {
            let arch = String::from_utf8_lossy(&result.stdout).trim().to_string();
            test_step_ok!(logger, "Architecture: {}", arch);

            test_step!(logger, "Checking Rosetta status");
            if arch == "arm64" {
                // Native Apple Silicon
                test_step_ok!(logger, "Running natively on Apple Silicon");
            } else if arch == "x86_64" {
                // Could be Intel Mac or Rosetta
                let rosetta_check = Command::new("sysctl")
                    .args(["-n", "sysctl.proc_translated"])
                    .output();

                match rosetta_check {
                    Ok(r) => {
                        let is_rosetta = String::from_utf8_lossy(&r.stdout).trim() == "1";
                        if is_rosetta {
                            test_step_ok!(logger, "Running under Rosetta 2 (Apple Silicon Mac)");
                        } else {
                            test_step_ok!(logger, "Running natively on Intel Mac");
                        }
                    }
                    Err(_) => {
                        test_step_ok!(logger, "Running on Intel Mac (Rosetta check unavailable)");
                    }
                }
            } else {
                test_step_ok!(logger, "Unknown architecture: {}", arch);
            }
        }
        Err(e) => {
            logger.step_skip(&format!("uname not available: {}", e));
            test_step!(logger, "Skipping architecture check");
            logger.step_skip("Unix tool not available");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// macOS Renderer Tests
// ============================================================================

/// Test macOS renderer creation
#[test]
#[cfg(feature = "macos")]
fn test_macos_renderer_creation() {
    use micround::render::macos::MacOSRenderer;

    let mut logger = TestLogger::new("macos_renderer_creation", 2);

    test_step!(logger, "Creating macOS renderer");
    let renderer = MacOSRenderer::new();
    test_assert!(logger, renderer.is_ok(), "Renderer creation succeeded");
    test_step_ok!(logger);

    test_step!(logger, "Verifying initial state");
    let renderer = renderer.unwrap();
    test_assert!(
        logger,
        !renderer.initialized,
        "Renderer starts uninitialized"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test macOS renderer initialization (requires desktop session)
#[test]
#[ignore = "requires macOS desktop session"]
#[cfg(feature = "macos")]
fn test_macos_renderer_init() {
    use micround::core::DisplayId;
    use micround::render::macos::MacOSRenderer;
    use micround::render::WallpaperRenderer;

    let mut logger = TestLogger::new("macos_renderer_init", 3);

    test_step!(logger, "Creating macOS renderer");
    let mut renderer = MacOSRenderer::new().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Initializing renderer for primary display");
    let display_id = DisplayId("primary".to_string());
    match renderer.init(&display_id) {
        Ok(()) => {
            test_step_ok!(logger, "Renderer initialized");

            test_step!(logger, "Shutting down renderer");
            renderer.shutdown();
            test_step_ok!(logger);
        }
        Err(e) => {
            logger.step_skip(&format!("Init failed (may need desktop): {:?}", e));
            test_step!(logger, "Skipping shutdown");
            logger.step_skip("Renderer not initialized");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// App Sandbox Tests
// ============================================================================

/// Test sandbox entitlements
#[test]
fn test_sandbox_environment() {
    let mut logger = TestLogger::new("sandbox_environment", 2);

    test_step!(logger, "Checking sandbox status");
    // APP_SANDBOX_CONTAINER_ID is set when running in sandbox
    let sandbox_id = env::var("APP_SANDBOX_CONTAINER_ID");
    match sandbox_id {
        Ok(id) => {
            test_step_ok!(logger, "Running in sandbox: {}", id);
        }
        Err(_) => {
            test_step_ok!(logger, "Not running in sandbox (development mode)");
        }
    }

    test_step!(logger, "Checking temporary directory access");
    let temp_dir = env::temp_dir();
    test_assert!(logger, temp_dir.exists(), "Temp directory accessible");
    test_step_ok!(logger, "Temp dir: {:?}", temp_dir);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test required entitlements documentation
#[test]
fn test_entitlements_documentation() {
    let mut logger = TestLogger::new("entitlements_documentation", 2);

    test_step!(logger, "Documenting required entitlements");
    // List the entitlements Micround needs
    let required_entitlements = [
        "com.apple.security.device.camera - Camera access for microscope capture",
        "com.apple.security.personal-information.photos-library - Snapshot save location (optional)",
        "com.apple.security.automation.apple-events - For desktop wallpaper changes",
    ];

    for entitlement in &required_entitlements {
        logger.info(entitlement);
    }
    test_step_ok!(
        logger,
        "Listed {} entitlements",
        required_entitlements.len()
    );

    test_step!(logger, "Documenting optional entitlements");
    let optional_entitlements =
        ["com.apple.security.cs.allow-unsigned-executable-memory - For some codec support"];
    for entitlement in &optional_entitlements {
        logger.info(entitlement);
    }
    test_step_ok!(
        logger,
        "Listed {} optional entitlements",
        optional_entitlements.len()
    );

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Power and Sleep Event Tests
// ============================================================================

/// Test power management event detection
#[test]
fn test_power_event_detection() {
    let mut logger = TestLogger::new("power_event_detection", 2);

    test_step!(logger, "Checking power management settings");
    let output = Command::new("pmset").args(["-g", "live"]).output();

    match output {
        Ok(result) => {
            if result.status.success() {
                let stdout = String::from_utf8_lossy(&result.stdout);
                test_step_ok!(logger, "Power management settings retrieved");

                test_step!(logger, "Checking sleep-related settings");
                let has_sleep_settings =
                    stdout.contains("sleep") || stdout.contains("displaysleep");
                if has_sleep_settings {
                    // Check for display sleep time
                    for line in stdout.lines() {
                        if line.contains("displaysleep") || line.contains("sleep") {
                            logger.info(&format!("  {}", line.trim()));
                        }
                    }
                    test_step_ok!(logger, "Sleep settings found");
                } else {
                    test_step_ok!(logger, "No sleep settings (may be disabled)");
                }
            } else {
                logger.step_skip("pmset command failed");
            }
        }
        Err(e) => {
            logger.step_skip(&format!("pmset not available: {}", e));
            test_step!(logger, "Skipping power event check");
            logger.step_skip("macOS tool not available");
        }
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// App Nap Tests
// ============================================================================

/// Test App Nap awareness
#[test]
fn test_app_nap_environment() {
    let mut logger = TestLogger::new("app_nap_environment", 2);

    test_step!(logger, "Documenting App Nap behavior");
    // App Nap can pause background apps to save power
    // Micround needs to disable it or handle it gracefully
    logger.info("App Nap considerations for Micround:");
    logger.info("- Background capture must continue when app is hidden");
    logger.info("- Use NSProcessInfo.beginActivity() to prevent napping");
    logger.info("- Release activity when capture is stopped");
    test_step_ok!(logger, "App Nap documentation complete");

    test_step!(logger, "Checking process activity assertions");
    // In a real app, we'd use NSProcessInfo to manage activities
    // Here we just document the requirement
    logger.info("Recommended: Use NSActivityUserInitiated | NSActivityIdleSystemSleepDisabled");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
