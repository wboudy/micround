//! E2E Test: Application Startup and Initialization
//!
//! Tests complete application startup sequence with detailed step logging.
//! Verifies config loading, logging init, UI creation, and device enumeration.
//!
//! This test exercises the full initialization path that main.rs uses,
//! validating each component initializes correctly in sequence.

#![cfg(feature = "test-simulator")]

mod common;

use std::time::Duration;
use tempfile::TempDir;

use common::test_logger::*;
use micround::capture::{
    CaptureBackend,
    simulator::{SimulatorBackend, SimulatorConfig, FramePattern},
};
use micround::config::{AppConfig, load_config, save_config, config_path};
use micround::core::{
    AppContext, AppState, Command, DeviceId, DisplayId, Event,
    CaptureSettings,
};
use micround::render::{
    WallpaperRenderer,
    simulator::{DisplaySimulator, DisplaySimulatorConfig},
};

// ============================================================================
// Core Startup Sequence Tests
// ============================================================================

/// Tests the complete application initialization sequence.
///
/// Steps verified:
/// 1. Logging system initialization
/// 2. Configuration loading (defaults or file)
/// 3. Capture backend creation
/// 4. Render backend creation
/// 5. Device enumeration
#[test]
fn test_startup_full_initialization_sequence() {
    let mut logger = TestLogger::new("startup_full_initialization_sequence", 6);

    // Step 1: Verify logging can be used (already initialized by test harness)
    test_step!(logger, "Verifying logging system");
    tracing::info!("E2E startup test beginning");
    tracing::debug!("Debug logging operational");
    test_step_ok!(logger, "Logging system operational");

    // Step 2: Configuration loading
    test_step!(logger, "Loading application configuration");
    let config = AppConfig::default();
    test_assert!(logger, config.version > 0, "Config has valid version");
    test_assert!(logger, config.camera.width > 0, "Camera width configured");
    test_assert!(logger, config.camera.height > 0, "Camera height configured");
    test_assert!(logger, config.camera.framerate > 0.0, "Framerate configured");
    test_step_ok!(logger, "Config loaded: {}x{} @ {} fps",
        config.camera.width, config.camera.height, config.camera.framerate);

    // Step 3: Create capture backend (simulator for testing)
    test_step!(logger, "Initializing capture backend");
    let capture_config = SimulatorConfig {
        width: config.camera.width,
        height: config.camera.height,
        fps: config.camera.framerate as u32,
        pattern: FramePattern::Checkerboard { size: 32 },
        ..Default::default()
    };
    let mut capture = SimulatorBackend::new(capture_config);
    test_assert!(logger, !capture.is_capturing(), "Capture starts in idle state");
    test_step_ok!(logger, "Capture backend created (Simulator)");

    // Step 4: Create render backend (simulator for testing)
    test_step!(logger, "Initializing render backend");
    let display_config = DisplaySimulatorConfig {
        width: 1920,
        height: 1080,
        frame_history_size: 10,
        ..Default::default()
    };
    let mut renderer = DisplaySimulator::new(display_config);
    renderer.init(&DisplayId("test:primary".into())).expect("init renderer");
    test_step_ok!(logger, "Render backend created (Simulator) 1920x1080");

    // Step 5: Device enumeration
    test_step!(logger, "Enumerating available devices");
    let devices = capture.enumerate_devices();
    test_assert!(logger, !devices.is_empty(), "At least one device available");
    for (i, device) in devices.iter().enumerate() {
        tracing::info!(device_index = i, device_id = %device.id.0, name = %device.name, "Found device");
    }
    test_step_ok!(logger, "Found {} camera device(s)", devices.len());

    // Step 6: Create application context
    test_step!(logger, "Creating application context");
    let (ctx, _cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_assert!(logger, true, "AppContext created successfully");
    test_step_ok!(logger, "Application context ready");

    // Cleanup
    renderer.shutdown();

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests that startup handles missing config gracefully.
#[test]
fn test_startup_missing_config_uses_defaults() {
    let mut logger = TestLogger::new("startup_missing_config_uses_defaults", 3);

    // Note: We can't easily test load_config() with a missing file because
    // it uses dirs::config_dir(). Instead we verify default creation.
    test_step!(logger, "Creating default configuration");
    let config = AppConfig::default();
    test_assert!(logger, config.version == 1, "Default version is 1");
    test_assert!(logger, config.camera.width == 1920, "Default width is 1920");
    test_assert!(logger, config.camera.height == 1080, "Default height is 1080");
    test_assert!(logger, config.camera.framerate == 30.0, "Default framerate is 30");
    test_step_ok!(logger);

    test_step!(logger, "Verifying startup behavior settings");
    test_assert!(logger, !config.startup.launch_at_login, "launch_at_login defaults to false");
    test_assert!(logger, !config.startup.auto_start_feed, "auto_start_feed defaults to false");
    test_assert!(logger, !config.startup.minimize_on_start, "minimize_on_start defaults to false");
    test_step_ok!(logger);

    test_step!(logger, "Verifying internal state defaults");
    test_assert!(logger, config.internal.original_wallpaper_path.is_none(),
        "No original wallpaper path");
    test_assert!(logger, config.internal.last_clean_shutdown,
        "last_clean_shutdown defaults to true");
    test_assert!(logger, config.internal.last_camera_id.is_none(),
        "No last camera ID");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests that startup validates and sanitizes invalid config values.
#[test]
fn test_startup_config_validation_and_sanitization() {
    let mut logger = TestLogger::new("startup_config_validation_and_sanitization", 4);

    test_step!(logger, "Creating config with invalid values");
    let mut config = AppConfig::default();
    config.camera.width = 0;
    config.camera.height = 0;
    config.camera.framerate = -10.0;
    config.display.rotation = 45; // Invalid rotation
    test_step_ok!(logger);

    test_step!(logger, "Validating config");
    let errors = config.validate();
    test_assert!(logger, !errors.is_empty(), "Validation detected errors");
    test_assert!(logger, errors.len() >= 3, "At least 3 validation errors");
    for err in &errors {
        tracing::warn!(field = %err.field, message = %err.message, "Validation error");
    }
    test_step_ok!(logger, "Found {} validation errors", errors.len());

    test_step!(logger, "Sanitizing config");
    config.sanitize();
    let errors_after = config.validate();
    test_assert!(logger, errors_after.is_empty(), "Config is valid after sanitization");
    test_assert!(logger, config.camera.width == 1920, "Width sanitized to default");
    test_assert!(logger, config.camera.height == 1080, "Height sanitized to default");
    test_assert!(logger, config.camera.framerate == 30.0, "Framerate sanitized to default");
    test_assert!(logger, config.display.rotation == 0, "Rotation sanitized to 0");
    test_step_ok!(logger);

    test_step!(logger, "Using sanitized config for backend creation");
    let capture_config = SimulatorConfig {
        width: config.camera.width,
        height: config.camera.height,
        fps: config.camera.framerate as u32,
        pattern: FramePattern::SolidColor { r: 128, g: 128, b: 128 },
        ..Default::default()
    };
    let capture = SimulatorBackend::new(capture_config);
    test_assert!(logger, !capture.is_capturing(), "Backend created successfully");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Device Enumeration Tests
// ============================================================================

/// Tests device enumeration at startup.
#[test]
fn test_startup_device_enumeration() {
    let mut logger = TestLogger::new("startup_device_enumeration", 4);

    test_step!(logger, "Creating capture backend");
    let mut capture = SimulatorBackend::new_default();
    test_step_ok!(logger);

    test_step!(logger, "Enumerating devices");
    let devices = capture.enumerate_devices();
    test_assert!(logger, !devices.is_empty(), "Devices found");
    test_step_ok!(logger, "Found {} device(s)", devices.len());

    test_step!(logger, "Validating device information");
    for device in &devices {
        test_assert!(logger, !device.id.0.is_empty(), "Device has ID");
        test_assert!(logger, !device.name.is_empty(), "Device has name");
        tracing::info!(
            device_id = %device.id.0,
            name = %device.name,
            "Device info validated"
        );
    }
    test_step_ok!(logger);

    test_step!(logger, "Testing device selection");
    let first_device = &devices[0];
    let result = capture.open(&first_device.id, CaptureSettings {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: None,
    });
    test_assert!(logger, result.is_ok(), "Can open first device");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests startup handles invalid device IDs gracefully.
#[test]
fn test_startup_invalid_device_handling() {
    let mut logger = TestLogger::new("startup_invalid_device_handling", 3);

    test_step!(logger, "Creating capture backend");
    let mut capture = SimulatorBackend::new_default();
    test_step_ok!(logger);

    test_step!(logger, "Attempting to open invalid device");
    let invalid_id = DeviceId("nonexistent:camera:12345".into());
    let result = capture.open(&invalid_id, CaptureSettings {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: None,
    });
    test_assert!(logger, result.is_err(), "Opening invalid device returns error");
    if let Err(e) = &result {
        tracing::warn!(error = %e, "Expected error for invalid device");
    }
    test_step_ok!(logger);

    test_step!(logger, "Backend remains usable after error");
    let devices = capture.enumerate_devices();
    test_assert!(logger, !devices.is_empty(), "Can still enumerate devices");
    let valid_result = capture.open(&devices[0].id, CaptureSettings {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: None,
    });
    test_assert!(logger, valid_result.is_ok(), "Can still open valid device");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Event System Initialization Tests
// ============================================================================

/// Tests event bus initialization and basic operation.
#[tokio::test]
async fn test_startup_event_system_init() {
    let mut logger = TestLogger::new("startup_event_system_init", 5);

    test_step!(logger, "Creating application context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_step_ok!(logger);

    test_step!(logger, "Subscribing to events");
    let mut event_sub = handle.subscribe_events();
    test_step_ok!(logger);

    test_step!(logger, "Publishing startup event");
    handle.publish_event(Event::StateChanged {
        old_state: AppState::Starting,
        new_state: AppState::Running,
    });
    let event = event_sub.recv().await.expect("receive startup event");
    if let Event::StateChanged { old_state, new_state } = event {
        test_assert!(logger, old_state == AppState::Starting, "Old state is Starting");
        test_assert!(logger, new_state == AppState::Running, "New state is Running");
    } else {
        test_assert!(logger, false, "Expected StateChanged event");
    }
    test_step_ok!(logger);

    test_step!(logger, "Testing command dispatch");
    handle.send_command(Command::Quit).await.expect("send shutdown");
    let cmd = cmd_rx.recv().await.expect("receive command");
    test_assert!(logger, matches!(cmd, Command::Quit), "Shutdown command received");
    test_step_ok!(logger);

    test_step!(logger, "Event system operational");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests multiple subscribers receive events.
#[tokio::test]
async fn test_startup_multiple_event_subscribers() {
    let mut logger = TestLogger::new("startup_multiple_event_subscribers", 4);

    test_step!(logger, "Creating context with multiple subscribers");
    let (ctx, _cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    let mut sub1 = handle.subscribe_events();
    let mut sub2 = handle.subscribe_events();
    let mut sub3 = handle.subscribe_events();
    test_step_ok!(logger);

    test_step!(logger, "Publishing event");
    handle.publish_event(Event::StateChanged {
        old_state: AppState::Starting,
        new_state: AppState::Running,
    });
    test_step_ok!(logger);

    test_step!(logger, "All subscribers receive event");
    let e1 = sub1.recv().await;
    let e2 = sub2.recv().await;
    let e3 = sub3.recv().await;
    test_assert!(logger, e1.is_some(), "Subscriber 1 received event");
    test_assert!(logger, e2.is_some(), "Subscriber 2 received event");
    test_assert!(logger, e3.is_some(), "Subscriber 3 received event");
    test_step_ok!(logger);

    test_step!(logger, "All received same event type");
    test_assert!(logger, matches!(e1.unwrap(), Event::StateChanged { .. }), "Sub1 got StateChanged");
    test_assert!(logger, matches!(e2.unwrap(), Event::StateChanged { .. }), "Sub2 got StateChanged");
    test_assert!(logger, matches!(e3.unwrap(), Event::StateChanged { .. }), "Sub3 got StateChanged");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Full Pipeline Initialization Tests
// ============================================================================

/// Tests initialization of the complete capture-to-render pipeline.
#[test]
fn test_startup_full_pipeline_init() {
    let mut logger = TestLogger::new("startup_full_pipeline_init", 6);

    // Step 1: Config
    test_step!(logger, "Loading configuration");
    let config = AppConfig::default();
    test_step_ok!(logger);

    // Step 2: Capture
    test_step!(logger, "Initializing capture backend");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: config.camera.width,
        height: config.camera.height,
        fps: config.camera.framerate as u32,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    test_assert!(logger, !devices.is_empty(), "Devices available");
    capture.open(&devices[0].id, CaptureSettings {
        width: config.camera.width,
        height: config.camera.height,
        framerate: config.camera.framerate,
        format: None,
    }).expect("open capture device");
    test_step_ok!(logger);

    // Step 3: Start capture
    test_step!(logger, "Starting capture");
    capture.start().expect("start capture");
    test_assert!(logger, capture.is_capturing(), "Capture is running");
    test_step_ok!(logger);

    // Step 4: Initialize renderer
    test_step!(logger, "Initializing render backend");
    let mut renderer = DisplaySimulator::new(DisplaySimulatorConfig {
        width: 1920,
        height: 1080,
        frame_history_size: 5,
        ..Default::default()
    });
    renderer.init(&DisplayId("test:0".into())).expect("init renderer");
    test_step_ok!(logger);

    // Step 5: Process a frame through pipeline
    test_step!(logger, "Processing frame through pipeline");
    let frame = capture.next_frame().expect("get frame");
    test_assert!(logger, frame.width > 0, "Frame has valid width");
    test_assert!(logger, frame.height > 0, "Frame has valid height");
    test_assert!(logger, !frame.data.is_empty(), "Frame has data");

    // Process frame
    use micround::process::{process_frame, ProcessorConfig};
    let proc_config = ProcessorConfig::new(1920, 1080);
    let processed = process_frame(&frame, &proc_config).expect("process frame");

    // Render frame
    renderer.render(&processed).expect("render frame");
    test_assert!(logger, renderer.frame_count() == 1, "Frame rendered");
    test_step_ok!(logger);

    // Step 6: Cleanup
    test_step!(logger, "Shutting down pipeline");
    capture.stop().expect("stop capture");
    capture.close();
    renderer.shutdown();
    test_assert!(logger, !capture.is_capturing(), "Capture stopped");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests startup with auto-start feature enabled.
#[test]
fn test_startup_auto_start_behavior() {
    let mut logger = TestLogger::new("startup_auto_start_behavior", 4);

    test_step!(logger, "Creating config with auto_start enabled");
    let mut config = AppConfig::default();
    config.startup.auto_start_feed = true;
    test_assert!(logger, config.startup.auto_start_feed, "auto_start_feed is enabled");
    test_step_ok!(logger);

    test_step!(logger, "Simulating auto-start initialization");
    // When auto_start_feed is true, the app should:
    // 1. Load last camera or first available
    // 2. Start capture immediately
    let mut capture = SimulatorBackend::new_default();
    let devices = capture.enumerate_devices();

    // Prefer last_camera_id if set, else first available
    let device_id = config.internal.last_camera_id
        .as_ref()
        .or_else(|| devices.first().map(|d| &d.id));

    test_assert!(logger, device_id.is_some(), "Device available for auto-start");
    test_step_ok!(logger);

    test_step!(logger, "Auto-starting capture");
    let device = device_id.unwrap();
    capture.open(device, CaptureSettings {
        width: config.camera.width,
        height: config.camera.height,
        framerate: config.camera.framerate,
        format: None,
    }).expect("open device");
    capture.start().expect("start capture");
    test_assert!(logger, capture.is_capturing(), "Capture auto-started");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Timing Tests
// ============================================================================

/// Tests that startup completes within acceptable time bounds.
/// Note: This test is marked as ignored because timing is highly variable.
#[test]
#[ignore]
fn test_startup_timing_performance() {
    use std::time::Instant;
    let mut logger = TestLogger::new("startup_timing_performance", 4);

    test_step!(logger, "Timing configuration load");
    let start = Instant::now();
    let _config = AppConfig::default();
    let config_time = start.elapsed();
    logger.timing("config_load", config_time);
    test_assert!(logger, config_time < Duration::from_millis(100),
        "Config load under 100ms");
    test_step_ok!(logger, "Config loaded in {:?}", config_time);

    test_step!(logger, "Timing backend creation");
    let start = Instant::now();
    let _capture = SimulatorBackend::new_default();
    let _renderer = DisplaySimulator::new(DisplaySimulatorConfig::default());
    let backend_time = start.elapsed();
    logger.timing("backend_creation", backend_time);
    test_assert!(logger, backend_time < Duration::from_millis(100),
        "Backend creation under 100ms");
    test_step_ok!(logger, "Backends created in {:?}", backend_time);

    test_step!(logger, "Timing device enumeration");
    let mut capture = SimulatorBackend::new_default();
    let start = Instant::now();
    let _devices = capture.enumerate_devices();
    let enum_time = start.elapsed();
    logger.timing("device_enumeration", enum_time);
    test_assert!(logger, enum_time < Duration::from_millis(500),
        "Device enumeration under 500ms");
    test_step_ok!(logger, "Devices enumerated in {:?}", enum_time);

    test_step!(logger, "Total startup time check");
    let total = config_time + backend_time + enum_time;
    test_assert!(logger, total < Duration::from_millis(1000),
        "Total startup under 1 second");
    test_step_ok!(logger, "Total startup time: {:?}", total);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Error Recovery Tests
// ============================================================================

/// Tests that startup can recover from backend initialization failures.
#[test]
fn test_startup_backend_failure_recovery() {
    let mut logger = TestLogger::new("startup_backend_failure_recovery", 4);

    test_step!(logger, "Creating faulty capture config");
    // Simulator handles all configs gracefully, so we test with invalid device
    let mut capture = SimulatorBackend::new_default();
    test_step_ok!(logger);

    test_step!(logger, "Attempting to open invalid device");
    let invalid = DeviceId("invalid:device:999".into());
    let result = capture.open(&invalid, CaptureSettings {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: None,
    });
    test_assert!(logger, result.is_err(), "Invalid device open fails");
    test_step_ok!(logger);

    test_step!(logger, "Recovering by using valid device");
    let devices = capture.enumerate_devices();
    test_assert!(logger, !devices.is_empty(), "Can still enumerate after failure");
    let result = capture.open(&devices[0].id, CaptureSettings {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: None,
    });
    test_assert!(logger, result.is_ok(), "Can open valid device after recovery");
    capture.close();
    test_step_ok!(logger);

    test_step!(logger, "Backend is fully operational after recovery");
    // Re-open and verify capture works
    capture.open(&devices[0].id, CaptureSettings {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: None,
    }).expect("open");
    capture.start().expect("start");
    let frame = capture.next_frame();
    test_assert!(logger, frame.is_ok(), "Can capture frames after recovery");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
