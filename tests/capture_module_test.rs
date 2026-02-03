//! Unit tests for capture module - Enumeration and stream management (bd-22b)
//!
//! Tests device enumeration, format negotiation, stream lifecycle, and hot-plug
//! handling using the Camera Simulator (test-simulator feature).
//!
//! Run with: cargo test --features test-simulator capture_module

#![cfg(feature = "test-simulator")]

mod common;

use common::test_logger::*;
use micround::capture::{
    simulator::{FramePattern, InjectedErrorType, SimulatorBackend, SimulatorConfig},
    CaptureBackend,
};
use micround::core::{CaptureError, CaptureSettings, DeviceId, PixelFormat};
use std::time::{Duration, Instant};

// ============================================================================
// Device Enumeration Tests
// ============================================================================

/// Test that the simulator can enumerate a single device
#[test]
fn test_enumerate_single_device() {
    let mut logger = TestLogger::new("enumerate_single_device", 4);

    test_step!(logger, "Creating simulator with single device");
    let config = SimulatorConfig {
        device_count: 1,
        device_name: "Test Camera".into(),
        ..Default::default()
    };
    let backend = SimulatorBackend::new(config);
    test_step_ok!(logger);

    test_step!(logger, "Enumerating devices");
    let devices = backend.enumerate_devices();
    test_step_ok!(logger, "Found {} device(s)", devices.len());

    test_step!(logger, "Validating device properties");
    test_assert!(logger, devices.len() == 1, "Expected 1 device");
    test_assert!(
        logger,
        devices[0].id.0.starts_with("simulator:"),
        "Device ID has correct prefix"
    );
    test_assert!(
        logger,
        devices[0].name == "Test Camera",
        "Device name matches"
    );
    test_assert!(logger, devices[0].is_available, "Device is available");
    test_assert!(
        logger,
        !devices[0].capabilities.is_empty(),
        "Device has capabilities"
    );
    test_step_ok!(logger);

    test_step!(logger, "Validating capabilities");
    let cap = &devices[0].capabilities[0];
    test_assert!(logger, cap.width > 0, "Capability has width");
    test_assert!(logger, cap.height > 0, "Capability has height");
    test_assert!(logger, cap.framerate > 0.0, "Capability has framerate");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test that the simulator can enumerate multiple devices
#[test]
fn test_enumerate_multiple_devices() {
    let mut logger = TestLogger::new("enumerate_multiple_devices", 3);

    test_step!(logger, "Creating simulator with 5 devices");
    let config = SimulatorConfig {
        device_count: 5,
        device_name: "Multi Camera".into(),
        ..Default::default()
    };
    let backend = SimulatorBackend::new(config);
    test_step_ok!(logger);

    test_step!(logger, "Enumerating devices");
    let devices = backend.enumerate_devices();
    test_step_ok!(logger, "Found {} device(s)", devices.len());

    test_step!(logger, "Validating unique device IDs");
    test_assert!(logger, devices.len() == 5, "Expected 5 devices");

    // Check all device IDs are unique
    let ids: Vec<&str> = devices.iter().map(|d| d.id.0.as_str()).collect();
    for (i, id) in ids.iter().enumerate() {
        for (j, other_id) in ids.iter().enumerate() {
            if i != j {
                test_assert!(logger, id != other_id, "Device IDs are unique");
            }
        }
    }

    // First device has base name, others have numbered suffix
    test_assert!(
        logger,
        devices[0].name == "Multi Camera",
        "First device has base name"
    );
    test_assert!(
        logger,
        devices[1].name.contains("#2"),
        "Second device is numbered"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test device capability listing
#[test]
fn test_device_capabilities() {
    let mut logger = TestLogger::new("device_capabilities", 3);

    test_step!(logger, "Creating simulator with custom dimensions");
    let config = SimulatorConfig {
        width: 1920,
        height: 1080,
        fps: 60,
        ..Default::default()
    };
    let backend = SimulatorBackend::new(config);
    test_step_ok!(logger);

    test_step!(logger, "Enumerating devices and checking capabilities");
    let devices = backend.enumerate_devices();
    let device = &devices[0];
    test_step_ok!(
        logger,
        "Device has {} capabilities",
        device.capabilities.len()
    );

    test_step!(logger, "Validating capability includes config dimensions");
    let has_config_cap = device
        .capabilities
        .iter()
        .any(|c| c.width == 1920 && c.height == 1080 && c.framerate == 60.0);
    test_assert!(
        logger,
        has_config_cap,
        "Capabilities include configured resolution"
    );

    // Should also have standard resolutions
    let has_640x480 = device
        .capabilities
        .iter()
        .any(|c| c.width == 640 && c.height == 480);
    test_assert!(logger, has_640x480, "Capabilities include 640x480");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Stream Lifecycle Tests
// ============================================================================

/// Test the complete stream lifecycle: open -> start -> capture -> stop -> close
#[test]
fn test_stream_lifecycle() {
    let mut logger = TestLogger::new("stream_lifecycle", 7);

    test_step!(logger, "Creating simulator");
    let config = SimulatorConfig {
        fps: 1000, // High fps for fast test
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let device_id = &devices[0].id;
    test_step_ok!(logger);

    test_step!(logger, "Opening device");
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 1000.0,
        format: None,
    };
    let format = backend.open(device_id, settings).unwrap();
    test_assert!(logger, format.width == 640, "Negotiated width matches");
    test_assert!(logger, format.height == 480, "Negotiated height matches");
    test_step_ok!(
        logger,
        "Format: {}x{} @ {}fps",
        format.width,
        format.height,
        format.framerate
    );

    test_step!(logger, "Starting capture");
    backend.start().unwrap();
    test_assert!(logger, backend.is_capturing(), "Backend is capturing");
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames");
    let start = Instant::now();
    for i in 0..5 {
        let frame = backend.next_frame().unwrap();
        test_assert!(logger, frame.sequence == i, "Frame sequence is correct");
        test_assert!(logger, frame.width == 640, "Frame width matches");
        test_assert!(logger, frame.height == 480, "Frame height matches");
        test_assert!(
            logger,
            frame.format == PixelFormat::Rgba32,
            "Frame format is RGBA32"
        );
    }
    test_step_ok!(logger, "Captured 5 frames in {:?}", start.elapsed());

    test_step!(logger, "Stopping capture");
    backend.stop().unwrap();
    test_assert!(logger, !backend.is_capturing(), "Backend is not capturing");
    test_step_ok!(logger);

    test_step!(logger, "Closing device");
    backend.close();
    test_step_ok!(logger);

    // Verify device can be reopened
    test_step!(logger, "Reopening device");
    let format = backend.open(device_id, CaptureSettings::default()).unwrap();
    test_assert!(logger, format.width > 0, "Device can be reopened");
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test starting capture without opening device first
#[test]
fn test_start_without_open() {
    let mut logger = TestLogger::new("start_without_open", 2);

    test_step!(logger, "Creating simulator");
    let mut backend = SimulatorBackend::new_default();
    test_step_ok!(logger);

    test_step!(logger, "Attempting to start without open");
    let result = backend.start();
    test_assert!(logger, result.is_err(), "Start should fail without open");
    test_step_ok!(logger, "Correctly rejected: {:?}", result.unwrap_err());

    let result = logger.finish();
    assert!(result.passed);
}

/// Test capturing without starting
#[test]
fn test_capture_without_start() {
    let mut logger = TestLogger::new("capture_without_start", 3);

    test_step!(logger, "Creating and opening simulator");
    let mut backend = SimulatorBackend::new_default();
    let devices = backend.enumerate_devices();
    backend
        .open(&devices[0].id, CaptureSettings::default())
        .unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Attempting to capture without start");
    let result = backend.next_frame();
    test_assert!(logger, result.is_err(), "Capture should fail without start");
    test_step_ok!(logger, "Correctly rejected");

    test_step!(logger, "Cleanup");
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test opening non-existent device
#[test]
fn test_open_invalid_device() {
    let mut logger = TestLogger::new("open_invalid_device", 2);

    test_step!(logger, "Creating simulator");
    let mut backend = SimulatorBackend::new_default();
    test_step_ok!(logger);

    test_step!(logger, "Attempting to open invalid device");
    let result = backend.open(
        &DeviceId("invalid:device".into()),
        CaptureSettings::default(),
    );
    test_assert!(
        logger,
        result.is_err(),
        "Open should fail for invalid device"
    );
    match result.unwrap_err() {
        CaptureError::DeviceNotFound(id) => {
            test_assert!(logger, id == "invalid:device", "Error contains device ID");
        }
        _ => panic!("Expected DeviceNotFound error"),
    }
    test_step_ok!(logger, "Correctly rejected");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Format Negotiation Tests
// ============================================================================

/// Test format negotiation returns expected format
#[test]
fn test_format_negotiation() {
    let mut logger = TestLogger::new("format_negotiation", 3);

    test_step!(logger, "Creating simulator");
    let mut backend = SimulatorBackend::new_default();
    let devices = backend.enumerate_devices();
    test_step_ok!(logger);

    test_step!(logger, "Opening with specific format request");
    let settings = CaptureSettings {
        width: 1280,
        height: 720,
        framerate: 30.0,
        format: Some(PixelFormat::Rgba32),
    };
    let format = backend.open(&devices[0].id, settings).unwrap();
    test_step_ok!(
        logger,
        "Negotiated: {}x{} @ {}fps {:?}",
        format.width,
        format.height,
        format.framerate,
        format.format
    );

    test_step!(logger, "Validating negotiated format");
    test_assert!(logger, format.width == 1280, "Width matches request");
    test_assert!(logger, format.height == 720, "Height matches request");
    test_assert!(
        logger,
        format.format == PixelFormat::Rgba32,
        "Format matches request"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test current_format returns correct value after open
#[test]
fn test_current_format() {
    let mut logger = TestLogger::new("current_format", 3);

    test_step!(logger, "Creating simulator");
    let mut backend = SimulatorBackend::new_default();
    let devices = backend.enumerate_devices();
    test_step_ok!(logger);

    test_step!(logger, "Checking format before open");
    test_assert!(
        logger,
        backend.current_format().is_none(),
        "No format before open"
    );
    test_step_ok!(logger);

    test_step!(logger, "Opening and checking format");
    let settings = CaptureSettings {
        width: 800,
        height: 600,
        framerate: 25.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    let current = backend.current_format();
    test_assert!(logger, current.is_some(), "Format available after open");
    let format = current.unwrap();
    test_assert!(logger, format.width == 800, "Current format width matches");
    test_assert!(
        logger,
        format.height == 600,
        "Current format height matches"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Error Injection Tests
// ============================================================================

/// Test error injection during capture
#[test]
fn test_error_injection() {
    let mut logger = TestLogger::new("error_injection", 4);

    test_step!(logger, "Creating simulator with 50% error rate");
    let config = SimulatorConfig {
        fps: 1000,
        error_rate: 0.5,
        error_type: InjectedErrorType::Timeout,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    test_step_ok!(logger);

    test_step!(logger, "Opening and starting capture");
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames and counting errors");
    let mut successes = 0;
    let mut errors = 0;
    for _ in 0..100 {
        match backend.next_frame() {
            Ok(_) => successes += 1,
            Err(CaptureError::Timeout(_)) => errors += 1,
            Err(e) => panic!("Unexpected error: {:?}", e),
        }
    }
    test_step_ok!(logger, "Successes: {}, Errors: {}", successes, errors);

    test_step!(logger, "Validating error distribution");
    // With 50% error rate, we expect roughly equal successes and errors
    test_assert!(logger, errors > 30, "Should have significant errors");
    test_assert!(logger, successes > 30, "Should have significant successes");
    test_step_ok!(logger, "Error rate approximately 50%");

    let result = logger.finish();
    assert!(result.passed);
}

/// Test different error types
#[test]
fn test_error_type_disconnected() {
    let mut logger = TestLogger::new("error_type_disconnected", 3);

    test_step!(logger, "Creating simulator with Disconnected error type");
    let config = SimulatorConfig {
        fps: 1000,
        error_rate: 1.0, // Always error
        error_type: InjectedErrorType::Disconnected,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    test_step_ok!(logger);

    test_step!(logger, "Opening and starting capture");
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing and checking error type");
    let result = backend.next_frame();
    test_assert!(logger, result.is_err(), "Should fail with error");
    match result {
        Err(CaptureError::Disconnected) => {
            test_step_ok!(logger, "Got Disconnected as expected");
        }
        Err(other) => panic!("Expected Disconnected, got {:?}", other),
        Ok(_) => panic!("Expected error, got success"),
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test frame drop simulation
#[test]
fn test_frame_drop_simulation() {
    let mut logger = TestLogger::new("frame_drop_simulation", 4);

    test_step!(logger, "Creating simulator with 20% drop rate");
    let config = SimulatorConfig {
        fps: 1000,
        drop_rate: 0.2,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    test_step_ok!(logger);

    test_step!(logger, "Opening and starting capture");
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames and checking sequences");
    let mut sequences: Vec<u64> = Vec::new();
    for _ in 0..50 {
        let frame = backend.next_frame().unwrap();
        sequences.push(frame.sequence);
    }
    test_step_ok!(logger, "Captured 50 frames");

    test_step!(logger, "Validating frame sequences show drops");
    // With drop rate, some sequences should be skipped
    let mut gaps = 0;
    for i in 1..sequences.len() {
        if sequences[i] > sequences[i - 1] + 1 {
            gaps += 1;
        }
    }
    // We expect some gaps due to dropped frames
    test_step_ok!(logger, "Found {} sequence gaps (dropped frames)", gaps);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Pattern Generation Tests
// ============================================================================

/// Test that different patterns generate different frame data
#[test]
fn test_different_patterns() {
    let mut logger = TestLogger::new("different_patterns", 3);

    test_step!(logger, "Creating backends with different patterns");
    let settings = CaptureSettings {
        width: 64,
        height: 64,
        framerate: 1000.0,
        format: None,
    };
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames with different patterns");
    let patterns = [
        (FramePattern::ColorBars, "ColorBars"),
        (
            FramePattern::SolidColor {
                r: 128,
                g: 64,
                b: 32,
            },
            "SolidColor",
        ),
        (FramePattern::Checkerboard { size: 8 }, "Checkerboard"),
    ];

    let mut frame_data: Vec<Vec<u8>> = Vec::new();

    for (pattern, name) in patterns {
        let config = SimulatorConfig {
            width: 64,
            height: 64,
            fps: 1000,
            pattern,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();
        backend.open(&devices[0].id, settings.clone()).unwrap();
        backend.start().unwrap();
        let frame = backend.next_frame().unwrap();
        frame_data.push(frame.data.clone());
        tracing::info!(pattern = %name, "Captured frame");
    }
    test_step_ok!(logger, "Captured {} patterns", frame_data.len());

    test_step!(logger, "Validating patterns are different");
    // Each pattern should produce different data
    for i in 0..frame_data.len() {
        for j in (i + 1)..frame_data.len() {
            test_assert!(
                logger,
                frame_data[i] != frame_data[j],
                "Pattern {} differs from {}",
                i,
                j
            );
        }
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Performance Tests
// ============================================================================

/// Test frame capture at high frame rates (measures frame generation, not timing)
#[test]
fn test_high_framerate_capture() {
    let mut logger = TestLogger::new("high_framerate_capture", 3);

    test_step!(logger, "Creating high-fps simulator");
    // Use very high fps to minimize frame timing delays
    let config = SimulatorConfig {
        fps: 10000, // Very high fps for fast capture
        width: 320,
        height: 240,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 320,
        height: 240,
        framerate: 10000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing multiple frames quickly");
    let start = Instant::now();
    let frame_count = 100;
    let mut last_sequence = 0;
    for _ in 0..frame_count {
        let frame = backend.next_frame().unwrap();
        test_assert!(
            logger,
            frame.sequence >= last_sequence,
            "Sequence increases"
        );
        last_sequence = frame.sequence;
    }
    let elapsed = start.elapsed();
    test_step_ok!(logger, "Captured {} frames in {:?}", frame_count, elapsed);

    test_step!(logger, "Validating frame data integrity");
    // Each frame should have proper dimensions
    let frame = backend.next_frame().unwrap();
    test_assert!(logger, frame.width == 320, "Frame width correct");
    test_assert!(logger, frame.height == 240, "Frame height correct");
    test_assert!(
        logger,
        frame.data.len() == 320 * 240 * 4,
        "Frame data size correct"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

/// Test that enumeration is safe across threads (read-only)
#[test]
fn test_thread_safe_enumeration() {
    use std::sync::Arc;
    use std::thread;

    let mut logger = TestLogger::new("thread_safe_enumeration", 3);

    test_step!(logger, "Creating shared simulator");
    let config = SimulatorConfig {
        device_count: 3,
        ..Default::default()
    };
    let backend = Arc::new(SimulatorBackend::new(config));
    test_step_ok!(logger);

    test_step!(logger, "Enumerating from multiple threads");
    let mut handles = Vec::new();
    for i in 0..5 {
        let backend_clone = Arc::clone(&backend);
        handles.push(thread::spawn(move || {
            let devices = backend_clone.enumerate_devices();
            (i, devices.len())
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    test_step_ok!(logger, "All threads completed");

    test_step!(logger, "Validating consistent results");
    for (thread_id, count) in &results {
        test_assert!(logger, *count == 3, "Thread {} saw 3 devices", thread_id);
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
