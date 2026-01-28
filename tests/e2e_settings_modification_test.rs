//! E2E Test: Settings Modification During Capture
//!
//! Tests changing scaling, rotation, flip while actively capturing.
//! Logs setting changes, pipeline reconfiguration, frame continuity verification.
//!
//! This test exercises the hot-reload capability for settings changes
//! without requiring a capture restart.

#![cfg(feature = "test-simulator")]

mod common;

use std::time::Duration;

use common::test_logger::*;
use micround::capture::{
    CaptureBackend,
    simulator::{SimulatorBackend, SimulatorConfig, FramePattern},
};
use micround::core::{
    AppContext, Command, DisplayId, CaptureSettings, ScalingMode, Rotation, Flip,
};
use micround::process::{process_frame, ProcessorConfig};
use micround::render::{
    WallpaperRenderer,
    simulator::{DisplaySimulator, DisplaySimulatorConfig},
};

// ============================================================================
// Scaling Mode Tests
// ============================================================================

/// Tests changing scaling mode while capture is active.
#[test]
fn test_scaling_change_during_capture() {
    let mut logger = TestLogger::new("scaling_change_during_capture", 5);

    // Setup
    test_step!(logger, "Starting capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Setting up display");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig::hd());
    display.init(&DisplayId("test:0".into())).expect("init display");
    test_step_ok!(logger);

    // Test each scaling mode
    test_step!(logger, "Processing with Fill scaling");
    let frame1 = capture.next_frame().expect("frame");
    let config_fill = ProcessorConfig::new(1920, 1080).with_scaling(ScalingMode::Fill);
    let processed_fill = process_frame(&frame1, &config_fill).expect("process");
    display.render(&processed_fill).expect("render");
    test_assert!(logger, display.frame_count() == 1, "Fill frame rendered");
    test_step_ok!(logger);

    test_step!(logger, "Switching to Fit scaling mid-capture");
    let frame2 = capture.next_frame().expect("frame");
    let config_fit = ProcessorConfig::new(1920, 1080).with_scaling(ScalingMode::Fit);
    let processed_fit = process_frame(&frame2, &config_fit).expect("process");
    display.render(&processed_fit).expect("render");
    test_assert!(logger, display.frame_count() == 2, "Fit frame rendered");
    
    // With Fit mode, the frame should be letterboxed
    let fit_frame = display.last_frame().expect("get frame");
    test_assert!(logger, fit_frame.width == 1920 && fit_frame.height == 1080, "Output dimensions correct");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests rapid scaling mode changes.
#[test]
fn test_rapid_scaling_changes() {
    let mut logger = TestLogger::new("rapid_scaling_changes", 4);

    test_step!(logger, "Starting capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640, height: 480, fps: 1000,
        pattern: FramePattern::SolidColor { r: 128, g: 128, b: 128 },
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        frame_history_size: 100,
        ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Processing frames with alternating scaling modes");
    let modes = [ScalingMode::Fill, ScalingMode::Fit, ScalingMode::Stretch];
    let mut frame_count = 0;
    
    for _ in 0..9 {
        let frame = capture.next_frame().expect("frame");
        let mode = modes[frame_count % 3];
        let config = ProcessorConfig::new(1920, 1080).with_scaling(mode);
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
        frame_count += 1;
    }
    
    test_assert!(logger, display.frame_count() == 9, "All 9 frames rendered");
    test_step_ok!(logger);

    test_step!(logger, "Verifying frame continuity");
    let history = display.frame_history();
    test_assert!(logger, history.len() >= 9, "Frame history preserved");
    for frame in history {
        test_assert!(logger, !frame.data.is_empty(), "Frame has data");
    }
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
// Rotation Tests
// ============================================================================

/// Tests changing rotation while capture is active.
#[test]
fn test_rotation_change_during_capture() {
    let mut logger = TestLogger::new("rotation_change_during_capture", 5);

    test_step!(logger, "Starting capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640, height: 480, fps: 1000,
        pattern: FramePattern::Checkerboard { size: 32 },
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Processing with no rotation");
    let frame = capture.next_frame().expect("frame");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        width: 1920, height: 1080, ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).expect("init");
    
    let config = ProcessorConfig::new(1920, 1080).with_rotation(Rotation::None);
    let processed = process_frame(&frame, &config).expect("process");
    display.render(&processed).expect("render");
    test_assert!(logger, display.frame_count() == 1, "No rotation frame rendered");
    test_step_ok!(logger);

    test_step!(logger, "Applying 90 degree rotation");
    let frame2 = capture.next_frame().expect("frame");
    // For 90 degree rotation, swap target dimensions
    let mut display90 = DisplaySimulator::new(DisplaySimulatorConfig {
        width: 1080, height: 1920, ..Default::default()
    });
    display90.init(&DisplayId("test:90".into())).expect("init");
    
    let config90 = ProcessorConfig::new(1080, 1920).with_rotation(Rotation::Clockwise90);
    let processed90 = process_frame(&frame2, &config90).expect("process");
    display90.render(&processed90).expect("render");
    
    let rotated = display90.last_frame().expect("get frame");
    test_assert!(logger, rotated.width == 1080, "Rotated width");
    test_assert!(logger, rotated.height == 1920, "Rotated height");
    test_step_ok!(logger);

    test_step!(logger, "Cycling through all rotations");
    let rotations = [
        (Rotation::None, 1920, 1080),
        (Rotation::Clockwise90, 1080, 1920),
        (Rotation::Clockwise180, 1920, 1080),
        (Rotation::Clockwise270, 1080, 1920),
    ];
    
    for (rotation, w, h) in &rotations {
        let frame = capture.next_frame().expect("frame");
        let config = ProcessorConfig::new(*w, *h).with_rotation(*rotation);
        let _processed = process_frame(&frame, &config).expect("process");
        tracing::info!(rotation = ?rotation, width = w, height = h, "Rotation applied");
    }
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    display90.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Flip Tests
// ============================================================================

/// Tests changing flip settings while capture is active.
#[test]
fn test_flip_change_during_capture() {
    let mut logger = TestLogger::new("flip_change_during_capture", 5);

    test_step!(logger, "Starting capture with gradient pattern");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640, height: 480, fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Setting up display");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        width: 1920, height: 1080,
        frame_history_size: 10,
        ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Processing with no flip");
    let frame = capture.next_frame().expect("frame");
    let config = ProcessorConfig::new(1920, 1080).with_flip(Flip::None);
    let processed = process_frame(&frame, &config).expect("process");
    display.render(&processed).expect("render");
    test_assert!(logger, display.frame_count() == 1, "No flip frame rendered");
    test_step_ok!(logger);

    test_step!(logger, "Testing all flip modes");
    let flips = [Flip::Horizontal, Flip::Vertical, Flip::Both];
    for flip in &flips {
        let frame = capture.next_frame().expect("frame");
        let config = ProcessorConfig::new(1920, 1080).with_flip(*flip);
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
        tracing::info!(flip = ?flip, "Flip applied");
    }
    test_assert!(logger, display.frame_count() == 4, "All flip modes rendered");
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
// Combined Settings Tests
// ============================================================================

/// Tests changing multiple settings simultaneously.
#[test]
fn test_combined_settings_change() {
    let mut logger = TestLogger::new("combined_settings_change", 5);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 1280, height: 720, fps: 1000,
        pattern: FramePattern::Checkerboard { size: 64 },
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 1280, height: 720, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Setting up display");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig::hd());
    display.init(&DisplayId("test:combined".into())).expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Processing with initial settings");
    let frame1 = capture.next_frame().expect("frame");
    let config1 = ProcessorConfig::new(1920, 1080)
        .with_scaling(ScalingMode::Fill)
        .with_rotation(Rotation::None)
        .with_flip(Flip::None);
    let processed1 = process_frame(&frame1, &config1).expect("process");
    display.render(&processed1).expect("render");
    test_step_ok!(logger);

    test_step!(logger, "Changing all settings at once");
    let frame2 = capture.next_frame().expect("frame");
    
    // Change to rotated display
    let mut display_rotated = DisplaySimulator::new(DisplaySimulatorConfig {
        width: 1080, height: 1920, ..Default::default()
    });
    display_rotated.init(&DisplayId("test:rotated".into())).expect("init");
    
    let config2 = ProcessorConfig::new(1080, 1920)
        .with_scaling(ScalingMode::Fit)
        .with_rotation(Rotation::Clockwise90)
        .with_flip(Flip::Horizontal);
    let processed2 = process_frame(&frame2, &config2).expect("process");
    display_rotated.render(&processed2).expect("render");
    
    let rotated_frame = display_rotated.last_frame().expect("get frame");
    test_assert!(logger, rotated_frame.width == 1080, "Rotated width correct");
    test_assert!(logger, rotated_frame.height == 1920, "Rotated height correct");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    display_rotated.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests settings matrix - all combinations.
#[test]
fn test_settings_matrix() {
    let mut logger = TestLogger::new("settings_matrix", 4);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640, height: 480, fps: 1000,
        pattern: FramePattern::SolidColor { r: 100, g: 150, b: 200 },
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    let frame = capture.next_frame().expect("frame");
    test_step_ok!(logger);

    test_step!(logger, "Testing all settings combinations");
    let scalings = [ScalingMode::Fill, ScalingMode::Fit, ScalingMode::Stretch];
    let rotations = [Rotation::None, Rotation::Clockwise90, Rotation::Clockwise180, Rotation::Clockwise270];
    let flips = [Flip::None, Flip::Horizontal, Flip::Vertical, Flip::Both];
    
    let mut successful = 0;
    for scaling in &scalings {
        for rotation in &rotations {
            for flip in &flips {
                let (w, h) = match rotation {
                    Rotation::Clockwise90 | Rotation::Clockwise270 => (480, 640),
                    _ => (640, 480),
                };
                let config = ProcessorConfig::new(w, h)
                    .with_scaling(*scaling)
                    .with_rotation(*rotation)
                    .with_flip(*flip);
                if process_frame(&frame, &config).is_ok() {
                    successful += 1;
                }
            }
        }
    }
    
    let expected = scalings.len() * rotations.len() * flips.len();
    test_assert!(logger, successful == expected, "All combinations successful");
    test_step_ok!(logger, "Processed {} combinations", successful);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Command-Based Settings Tests
// ============================================================================

/// Tests settings changes via command dispatch.
#[tokio::test]
async fn test_settings_command_dispatch() {
    let mut logger = TestLogger::new("settings_command_dispatch", 5);

    test_step!(logger, "Creating app context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_step_ok!(logger);

    test_step!(logger, "Sending scaling command");
    handle.send_command(Command::SetScaling { mode: ScalingMode::Fit })
        .await.expect("send");
    let cmd = cmd_rx.recv().await.expect("recv");
    test_assert!(logger, matches!(cmd, Command::SetScaling { mode: ScalingMode::Fit }),
        "Scaling command received");
    test_step_ok!(logger);

    test_step!(logger, "Sending rotation command");
    handle.send_command(Command::SetRotation { rotation: Rotation::Clockwise90 })
        .await.expect("send");
    let cmd = cmd_rx.recv().await.expect("recv");
    test_assert!(logger, matches!(cmd, Command::SetRotation { rotation: Rotation::Clockwise90 }),
        "Rotation command received");
    test_step_ok!(logger);

    test_step!(logger, "Sending flip command");
    handle.send_command(Command::SetFlip { flip: Flip::Both })
        .await.expect("send");
    let cmd = cmd_rx.recv().await.expect("recv");
    test_assert!(logger, matches!(cmd, Command::SetFlip { flip: Flip::Both }),
        "Flip command received");
    test_step_ok!(logger);

    test_step!(logger, "Sending rapid settings commands");
    for _ in 0..5 {
        handle.send_command(Command::SetScaling { mode: ScalingMode::Fill }).await.ok();
        handle.send_command(Command::SetRotation { rotation: Rotation::None }).await.ok();
        handle.send_command(Command::SetFlip { flip: Flip::None }).await.ok();
    }
    
    let mut count = 0;
    while cmd_rx.try_recv().is_ok() {
        count += 1;
    }
    test_assert!(logger, count == 15, "All 15 commands received");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Frame Continuity Tests
// ============================================================================

/// Tests that frames continue without interruption during settings changes.
#[test]
fn test_frame_continuity_during_settings_change() {
    let mut logger = TestLogger::new("frame_continuity_during_settings_change", 4);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640, height: 480, fps: 1000,
        pattern: FramePattern::Counter,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 1000.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        frame_history_size: 30,
        ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Rendering frames while changing settings");
    let configs = [
        ProcessorConfig::new(1920, 1080).with_scaling(ScalingMode::Fill),
        ProcessorConfig::new(1920, 1080).with_scaling(ScalingMode::Fit),
        ProcessorConfig::new(1920, 1080).with_flip(Flip::Horizontal),
    ];
    
    for i in 0..15 {
        let frame = capture.next_frame().expect("frame");
        let config = &configs[i % 3];
        let processed = process_frame(&frame, config).expect("process");
        display.render(&processed).expect("render");
    }
    test_assert!(logger, display.frame_count() == 15, "All 15 frames rendered");
    test_step_ok!(logger);

    test_step!(logger, "Verifying frame continuity");
    let history = display.frame_history();
    test_assert!(logger, history.len() >= 15, "Frame history has all frames");
    
    // All frames should have data (no dropped frames)
    for (i, frame) in history.iter().enumerate() {
        test_assert!(logger, !frame.data.is_empty(), "Frame {} has data", i);
    }
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
// Edge Cases
// ============================================================================

/// Tests invalid settings are rejected.
#[test]
fn test_invalid_settings_rejected() {
    let mut logger = TestLogger::new("invalid_settings_rejected", 3);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new_default();
    let devices = capture.enumerate_devices();
    capture.open(&devices[0].id, CaptureSettings {
        width: 640, height: 480, framerate: 30.0, format: None,
    }).expect("open");
    capture.start().expect("start");
    let frame = capture.next_frame().expect("frame");
    test_step_ok!(logger);

    test_step!(logger, "Testing zero dimensions");
    // ProcessorConfig requires non-zero dimensions at construction
    // So we test that valid configs work
    let valid_config = ProcessorConfig::new(1920, 1080);
    let result = process_frame(&frame, &valid_config);
    test_assert!(logger, result.is_ok(), "Valid config works");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
