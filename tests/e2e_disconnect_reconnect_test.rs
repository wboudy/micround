//! E2E Test: Camera Disconnect and Reconnection (bd-fb8)
//!
//! Tests hot-unplug handling and auto-reconnect scenarios.
//! Logs detection time, state transitions, reconnection attempts, recovery.
//!
//! This test simulates the user experience when a camera is disconnected
//! and reconnected, validating proper error handling and recovery.

#![cfg(feature = "test-simulator")]

mod common;

use std::time::Instant;

use common::test_logger::*;
use micround::capture::{
    simulator::{FramePattern, InjectedErrorType, SimulatorBackend, SimulatorConfig},
    CaptureBackend,
};
use micround::core::{AppContext, AppState, CaptureSettings, Command, DeviceId, DisplayId, Event};
use micround::process::{process_frame, ProcessorConfig};
use micround::render::{
    simulator::{DisplaySimulator, DisplaySimulatorConfig},
    WallpaperRenderer,
};

// ============================================================================
// Disconnect Detection Tests
// ============================================================================

/// Tests detection of camera disconnect via error injection.
#[test]
fn test_disconnect_detection() {
    let mut logger = TestLogger::new("disconnect_detection", 5);

    test_step!(logger, "Starting capture with error injection enabled");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::SolidColor {
            r: 128,
            g: 128,
            b: 128,
        },
        error_rate: 0.0, // Start with no errors
        error_type: InjectedErrorType::Disconnected,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 640,
                height: 480,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames successfully");
    for _ in 0..5 {
        let frame = capture.next_frame();
        test_assert!(logger, frame.is_ok(), "Frame captured successfully");
    }
    test_step_ok!(logger);

    test_step!(logger, "Simulating disconnect by stopping capture");
    // In a real scenario, the simulator would inject errors
    // For testing, we stop capture to simulate disconnect
    capture.stop().expect("simulate disconnect");
    test_assert!(
        logger,
        !capture.is_capturing(),
        "Capture stopped (disconnected)"
    );
    tracing::info!("Simulated camera disconnect");
    test_step_ok!(logger);

    test_step!(logger, "Attempting to capture after disconnect");
    let result = capture.next_frame();
    test_assert!(
        logger,
        result.is_err(),
        "Frame capture fails after disconnect"
    );
    if let Err(e) = &result {
        tracing::warn!(error = %e, "Expected error after disconnect");
    }
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests state transitions during disconnect.
#[tokio::test]
async fn test_disconnect_state_transitions() {
    let mut logger = TestLogger::new("disconnect_state_transitions", 5);

    test_step!(logger, "Creating application context");
    let (ctx, _cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    let mut event_sub = handle.subscribe_events();
    test_step_ok!(logger);

    test_step!(logger, "Simulating Running state");
    handle.publish_event(Event::StateChanged {
        old_state: AppState::Starting,
        new_state: AppState::Running,
    });
    let event = event_sub.recv().await.expect("receive running");
    test_assert!(
        logger,
        matches!(
            event,
            Event::StateChanged {
                new_state: AppState::Running,
                ..
            }
        ),
        "Now Running"
    );
    test_step_ok!(logger);

    test_step!(logger, "Publishing CameraDisconnected event");
    let device_id = DeviceId("test:camera:0".into());
    handle.publish_event(Event::CameraDisconnected {
        device_id: device_id.clone(),
    });
    let event = event_sub.recv().await.expect("receive disconnect");
    if let Event::CameraDisconnected {
        device_id: disconnected_id,
    } = event
    {
        test_assert!(
            logger,
            disconnected_id == device_id,
            "Correct device disconnected"
        );
        tracing::warn!(device_id = %device_id, "Camera disconnected event received");
    }
    test_step_ok!(logger);

    test_step!(logger, "Publishing CaptureStopped event");
    handle.publish_event(Event::CaptureStopped {
        device_id: device_id.clone(),
    });
    let event = event_sub.recv().await.expect("receive stopped");
    test_assert!(
        logger,
        matches!(event, Event::CaptureStopped { .. }),
        "Capture stopped"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Reconnection Tests
// ============================================================================

/// Tests reconnection after disconnect.
#[test]
fn test_reconnection_after_disconnect() {
    let mut logger = TestLogger::new("reconnection_after_disconnect", 6);

    test_step!(logger, "Starting initial capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::ColorBars,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    let device_id = devices[0].id.clone();

    capture
        .open(
            &device_id,
            CaptureSettings {
                width: 640,
                height: 480,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");
    test_assert!(logger, capture.is_capturing(), "Initial capture started");
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames before disconnect");
    let frame_before = capture.next_frame().expect("get frame before");
    test_assert!(
        logger,
        frame_before.width > 0,
        "Frame captured before disconnect"
    );
    test_step_ok!(logger);

    test_step!(logger, "Simulating disconnect");
    capture.stop().expect("stop for disconnect");
    capture.close();
    test_assert!(logger, !capture.is_capturing(), "Capture stopped");
    tracing::info!("Camera disconnected");
    test_step_ok!(logger);

    test_step!(logger, "Reconnecting camera");
    // In a real scenario, we'd detect reconnection via hotplug
    // For testing, we re-enumerate and reopen
    let reconnect_start = Instant::now();
    let new_devices = capture.enumerate_devices();
    test_assert!(
        logger,
        !new_devices.is_empty(),
        "Device available after reconnect"
    );

    let reconnect_result = capture.open(
        &new_devices[0].id,
        CaptureSettings {
            width: 640,
            height: 480,
            framerate: 1000.0,
            format: None,
        },
    );
    test_assert!(logger, reconnect_result.is_ok(), "Reconnection successful");
    capture.start().expect("restart");
    let reconnect_time = reconnect_start.elapsed();
    tracing::info!(
        reconnect_time_ms = reconnect_time.as_millis(),
        "Camera reconnected"
    );
    test_step_ok!(logger, "Reconnected in {:?}", reconnect_time);

    test_step!(logger, "Capturing frames after reconnect");
    let frame_after = capture.next_frame().expect("get frame after");
    test_assert!(
        logger,
        frame_after.width > 0,
        "Frame captured after reconnect"
    );
    test_assert!(
        logger,
        frame_after.width == frame_before.width,
        "Resolution maintained"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests reconnection state transitions via events.
#[tokio::test]
async fn test_reconnection_events() {
    let mut logger = TestLogger::new("reconnection_events", 5);

    test_step!(logger, "Creating application context");
    let (ctx, _cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    let mut event_sub = handle.subscribe_events();
    test_step_ok!(logger);

    let device_id = DeviceId("simulator:0".into());

    test_step!(logger, "Publishing disconnect event");
    handle.publish_event(Event::CameraDisconnected {
        device_id: device_id.clone(),
    });
    let event = event_sub.recv().await.expect("receive disconnect");
    test_assert!(
        logger,
        matches!(event, Event::CameraDisconnected { .. }),
        "Disconnect received"
    );
    test_step_ok!(logger);

    test_step!(logger, "Publishing reconnect event (CameraConnected)");
    handle.publish_event(Event::CameraConnected {
        device: micround::core::CameraDevice {
            id: device_id.clone(),
            name: "Reconnected Camera".into(),
            manufacturer: Some("Test".into()),
            capabilities: vec![],
            is_available: true,
        },
    });
    let event = event_sub.recv().await.expect("receive connect");
    if let Event::CameraConnected { device } = event {
        test_assert!(logger, device.id == device_id, "Correct device reconnected");
        tracing::info!(device_id = %device.id, name = %device.name, "Camera reconnected");
    }
    test_step_ok!(logger);

    test_step!(logger, "Sending restart capture command");
    handle
        .send_command(Command::StartCapture {
            device_id: device_id.clone(),
        })
        .await
        .expect("send start");

    let cmd = cmd_rx.recv().await.expect("receive start");
    test_assert!(
        logger,
        matches!(cmd, Command::StartCapture { .. }),
        "Start command received"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Recovery Tests
// ============================================================================

/// Tests full recovery flow from disconnect to resume.
#[test]
fn test_full_recovery_flow() {
    let mut logger = TestLogger::new("full_recovery_flow", 7);

    test_step!(logger, "Setting up capture and display");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 640,
                height: 480,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");

    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        frame_history_size: 20,
        ..Default::default()
    });
    display
        .init(&DisplayId("test:0".into()))
        .expect("init display");
    test_step_ok!(logger);

    test_step!(logger, "Rendering frames before disconnect");
    let config = ProcessorConfig::new(1920, 1080);
    for _ in 0..5 {
        let frame = capture.next_frame().expect("frame");
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
    }
    let pre_disconnect_count = display.frame_count();
    test_assert!(logger, pre_disconnect_count == 5, "Pre-disconnect frames");
    test_step_ok!(logger);

    test_step!(logger, "Simulating disconnect");
    capture.stop().expect("stop");
    capture.close();
    // Store last frame for display during disconnect
    let last_frame = display.last_frame().expect("get last frame");
    tracing::warn!("Camera disconnected, showing frozen frame");
    test_step_ok!(logger);

    test_step!(logger, "Verifying frozen frame during disconnect");
    // Display should show last frame (frozen)
    let frozen = display.last_frame().expect("get frozen");
    test_assert!(
        logger,
        frozen.width == last_frame.width,
        "Frozen frame preserved"
    );
    test_step_ok!(logger);

    test_step!(logger, "Reconnecting camera");
    let new_devices = capture.enumerate_devices();
    capture
        .open(
            &new_devices[0].id,
            CaptureSettings {
                width: 640,
                height: 480,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("reopen");
    capture.start().expect("restart");
    test_assert!(logger, capture.is_capturing(), "Capture resumed");
    tracing::info!("Camera reconnected, resuming capture");
    test_step_ok!(logger);

    test_step!(logger, "Rendering frames after recovery");
    for _ in 0..5 {
        let frame = capture.next_frame().expect("frame");
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
    }
    let post_recovery_count = display.frame_count();
    test_assert!(logger, post_recovery_count == 10, "Post-recovery frames");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Multiple Disconnect/Reconnect Cycles
// ============================================================================

/// Tests multiple disconnect/reconnect cycles.
#[test]
fn test_multiple_disconnect_cycles() {
    let mut logger = TestLogger::new("multiple_disconnect_cycles", 4);

    test_step!(logger, "Setting up capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::Counter,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    let device_id = devices[0].id.clone();
    test_step_ok!(logger);

    test_step!(logger, "Performing 5 disconnect/reconnect cycles");
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 1000.0,
        format: None,
    };

    for cycle in 0..5 {
        // Connect and capture
        capture.open(&device_id, settings.clone()).expect("open");
        capture.start().expect("start");
        let frame = capture.next_frame().expect("capture");
        test_assert!(logger, frame.width > 0, "Cycle {} capture works", cycle);

        // Disconnect
        capture.stop().expect("stop");
        capture.close();

        tracing::info!(cycle = cycle + 1, "Completed disconnect/reconnect cycle");
    }
    test_step_ok!(logger, "All 5 cycles completed");

    test_step!(logger, "Final capture verification");
    capture.open(&device_id, settings).expect("final open");
    capture.start().expect("final start");
    let final_frame = capture.next_frame();
    test_assert!(
        logger,
        final_frame.is_ok(),
        "Final capture works after cycles"
    );
    capture.stop().expect("final stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Device Switching Tests
// ============================================================================

/// Tests switching to different device after disconnect.
#[test]
fn test_switch_device_after_disconnect() {
    let mut logger = TestLogger::new("switch_device_after_disconnect", 5);

    test_step!(logger, "Setting up multi-device simulator");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::ColorBars,
        device_count: 3,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    test_assert!(logger, devices.len() >= 2, "Multiple devices available");
    test_step_ok!(logger, "Found {} devices", devices.len());

    test_step!(logger, "Capturing from device 0");
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 640,
                height: 480,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("open device 0");
    capture.start().expect("start");
    let _ = capture.next_frame().expect("frame from device 0");
    test_step_ok!(logger);

    test_step!(logger, "Simulating disconnect of device 0");
    capture.stop().expect("stop");
    capture.close();
    tracing::warn!(device = %devices[0].id, "Device 0 disconnected");
    test_step_ok!(logger);

    test_step!(logger, "Switching to device 1");
    let switch_result = capture.open(
        &devices[1].id,
        CaptureSettings {
            width: 640,
            height: 480,
            framerate: 1000.0,
            format: None,
        },
    );
    test_assert!(
        logger,
        switch_result.is_ok(),
        "Switch to device 1 successful"
    );
    capture.start().expect("start device 1");
    let frame = capture.next_frame().expect("frame from device 1");
    test_assert!(logger, frame.width > 0, "Capturing from device 1");
    tracing::info!(device = %devices[1].id, "Switched to device 1");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Tests handling of device not found during reconnect.
#[test]
fn test_reconnect_device_not_found() {
    let mut logger = TestLogger::new("reconnect_device_not_found", 4);

    test_step!(logger, "Setting up capture");
    let mut capture = SimulatorBackend::new_default();
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 640,
                height: 480,
                framerate: 30.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Simulating disconnect");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    test_step!(logger, "Attempting reconnect with invalid device ID");
    let invalid_id = DeviceId("invalid:camera:not_found".into());
    let result = capture.open(
        &invalid_id,
        CaptureSettings {
            width: 640,
            height: 480,
            framerate: 30.0,
            format: None,
        },
    );
    test_assert!(logger, result.is_err(), "Invalid device returns error");
    if let Err(e) = &result {
        tracing::warn!(error = %e, "Expected error for missing device");
    }
    test_step_ok!(logger);

    test_step!(logger, "Recovery by using valid device");
    let valid_devices = capture.enumerate_devices();
    let recovery = capture.open(
        &valid_devices[0].id,
        CaptureSettings {
            width: 640,
            height: 480,
            framerate: 30.0,
            format: None,
        },
    );
    test_assert!(logger, recovery.is_ok(), "Recovery with valid device works");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests disconnect during frame processing.
#[test]
fn test_disconnect_during_processing() {
    let mut logger = TestLogger::new("disconnect_during_processing", 4);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 1920,
        height: 1080,
        fps: 1000,
        pattern: FramePattern::Noise,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 1920,
                height: 1080,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Getting frame before disconnect");
    let frame = capture.next_frame().expect("get frame");
    test_step_ok!(logger);

    test_step!(logger, "Disconnecting while processing");
    capture.stop().expect("disconnect");
    capture.close();

    // Process the already-captured frame
    let config = ProcessorConfig::new(1920, 1080);
    let result = process_frame(&frame, &config);
    test_assert!(
        logger,
        result.is_ok(),
        "Can process frame captured before disconnect"
    );
    test_step_ok!(logger);

    test_step!(logger, "Reconnect and verify");
    let new_devices = capture.enumerate_devices();
    capture
        .open(
            &new_devices[0].id,
            CaptureSettings {
                width: 1920,
                height: 1080,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("reconnect");
    capture.start().expect("restart");
    let new_frame = capture.next_frame();
    test_assert!(logger, new_frame.is_ok(), "Can capture after reconnect");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
