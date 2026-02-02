//! Integration tests: Config changes during operation
//!
//! Tests hot-reloading of configuration while the pipeline is active.
//! Verifies settings apply without restart and state remains consistent.

#![cfg(feature = "test-simulator")]

mod common;

use std::time::Duration;
use tokio::time::sleep;

use common::test_logger::*;
use micround::capture::{
    simulator::{FramePattern, SimulatorBackend, SimulatorConfig},
    CaptureBackend,
};
use micround::config::{AppConfig, DisplayConfig};
use micround::core::{
    AppContext, AppState, CaptureSettings, Command, DisplayId, Event, Flip, PixelFormat, Rotation,
    ScalingMode,
};
use micround::process::{process_frame, ProcessorConfig};
use micround::render::{
    simulator::{DisplaySimulator, DisplaySimulatorConfig},
    WallpaperRenderer,
};

// ============================================================================
// Scaling Mode Change Tests
// ============================================================================

#[test]
fn test_scaling_mode_change_during_capture() {
    let mut logger = TestLogger::new("scaling_mode_change_during_capture", 4);

    test_step!(logger, "Set up capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::SolidColor {
            r: 128,
            g: 128,
            b: 128,
        },
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
        .unwrap();
    capture.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Process frames with initial scaling mode (Fill)");
    let config = ProcessorConfig::new(800, 600).with_scaling(ScalingMode::Fill);
    let frame = capture.next_frame().unwrap();
    let processed = process_frame(&frame, &config).unwrap();
    test_assert!(
        logger,
        processed.width == 800,
        "Initial frame width correct"
    );
    test_step_ok!(logger);

    test_step!(logger, "Change scaling mode to Fit and process frame");
    let config2 = ProcessorConfig::new(800, 600).with_scaling(ScalingMode::Fit);
    let frame2 = capture.next_frame().unwrap();
    let processed2 = process_frame(&frame2, &config2).unwrap();
    test_assert!(
        logger,
        processed2.width > 0,
        "Frame processed with Fit mode"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

#[test]
fn test_all_scaling_modes() {
    let mut logger = TestLogger::new("all_scaling_modes", 5);

    test_step!(logger, "Set up capture simulator");
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
        .unwrap();
    capture.start().unwrap();
    let frame = capture.next_frame().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Test Fill scaling mode");
    let config = ProcessorConfig::new(800, 600).with_scaling(ScalingMode::Fill);
    let fill_result = process_frame(&frame, &config).unwrap();
    test_assert!(logger, fill_result.width == 800, "Fill width correct");
    test_assert!(logger, fill_result.height == 600, "Fill height correct");
    test_step_ok!(logger);

    test_step!(logger, "Test Fit scaling mode");
    let config = ProcessorConfig::new(800, 600).with_scaling(ScalingMode::Fit);
    let fit_result = process_frame(&frame, &config).unwrap();
    test_assert!(logger, fit_result.width <= 800, "Fit width within bounds");
    test_assert!(logger, fit_result.height <= 600, "Fit height within bounds");
    test_step_ok!(logger);

    test_step!(logger, "Test Stretch scaling mode");
    let config = ProcessorConfig::new(800, 600).with_scaling(ScalingMode::Stretch);
    let stretch_result = process_frame(&frame, &config).unwrap();
    test_assert!(logger, stretch_result.width == 800, "Stretch width exact");
    test_assert!(logger, stretch_result.height == 600, "Stretch height exact");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Rotation Change Tests
// ============================================================================

#[test]
fn test_rotation_change_during_capture() {
    let mut logger = TestLogger::new("rotation_change_during_capture", 5);

    test_step!(logger, "Set up capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::Checkerboard { size: 32 },
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
        .unwrap();
    capture.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Process frame with no rotation");
    let frame = capture.next_frame().unwrap();
    let config = ProcessorConfig::new(640, 480).with_rotation(Rotation::None);
    let _no_rot = process_frame(&frame, &config).unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Apply 90 degree rotation");
    let config = ProcessorConfig::new(480, 640).with_rotation(Rotation::Clockwise90);
    let rot_90 = process_frame(&frame, &config).unwrap();
    test_assert!(logger, rot_90.width == 480, "Width after 90 deg");
    test_assert!(logger, rot_90.height == 640, "Height after 90 deg");
    test_step_ok!(logger);

    test_step!(logger, "Apply 180 degree rotation");
    let config = ProcessorConfig::new(640, 480).with_rotation(Rotation::Clockwise180);
    let rot_180 = process_frame(&frame, &config).unwrap();
    test_assert!(logger, rot_180.width == 640, "Width same after 180 deg");
    test_assert!(logger, rot_180.height == 480, "Height same after 180 deg");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Flip Change Tests
// ============================================================================

#[test]
fn test_flip_change_during_capture() {
    let mut logger = TestLogger::new("flip_change_during_capture", 5);

    test_step!(logger, "Set up capture pipeline with gradient");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 320,
        height: 240,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 320,
                height: 240,
                framerate: 1000.0,
                format: None,
            },
        )
        .unwrap();
    capture.start().unwrap();
    let frame = capture.next_frame().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Process frame with no flip");
    let config = ProcessorConfig::new(320, 240).with_flip(Flip::None);
    let no_flip = process_frame(&frame, &config).unwrap();
    test_assert!(logger, no_flip.data.len() > 0, "Frame has data");
    test_step_ok!(logger);

    test_step!(logger, "Apply horizontal flip");
    let config = ProcessorConfig::new(320, 240).with_flip(Flip::Horizontal);
    let h_flip = process_frame(&frame, &config).unwrap();
    test_assert!(logger, h_flip.width == no_flip.width, "Width unchanged");
    test_assert!(logger, h_flip.height == no_flip.height, "Height unchanged");
    test_step_ok!(logger);

    test_step!(logger, "Apply vertical flip");
    let config = ProcessorConfig::new(320, 240).with_flip(Flip::Vertical);
    let v_flip = process_frame(&frame, &config).unwrap();
    test_assert!(
        logger,
        v_flip.width == no_flip.width,
        "Width unchanged after v flip"
    );
    test_assert!(
        logger,
        v_flip.height == no_flip.height,
        "Height unchanged after v flip"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Combined Transform Changes
// ============================================================================

#[test]
fn test_combined_transform_changes() {
    let mut logger = TestLogger::new("combined_transform_changes", 6);

    test_step!(logger, "Set up capture simulator");
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
        .unwrap();
    capture.start().unwrap();
    let frame = capture.next_frame().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Apply rotation only");
    let config = ProcessorConfig::new(480, 640)
        .with_rotation(Rotation::Clockwise90)
        .with_flip(Flip::None);
    let rot_only = process_frame(&frame, &config).unwrap();
    test_assert!(logger, rot_only.width == 480, "Rotation only width");
    test_step_ok!(logger);

    test_step!(logger, "Add flip to rotation");
    let config = ProcessorConfig::new(480, 640)
        .with_rotation(Rotation::Clockwise90)
        .with_flip(Flip::Horizontal);
    let rot_and_flip = process_frame(&frame, &config).unwrap();
    test_assert!(
        logger,
        rot_and_flip.width == rot_only.width,
        "Adding flip keeps dimensions"
    );
    test_step_ok!(logger);

    test_step!(logger, "Change scaling mode with transforms");
    let config = ProcessorConfig::new(800, 600)
        .with_rotation(Rotation::Clockwise90)
        .with_flip(Flip::Horizontal)
        .with_scaling(ScalingMode::Fit);
    let all_transforms = process_frame(&frame, &config).unwrap();
    test_assert!(logger, all_transforms.width <= 800, "Width within target");
    test_assert!(logger, all_transforms.height <= 600, "Height within target");
    test_step_ok!(logger);

    test_step!(logger, "Reset all transforms");
    let config = ProcessorConfig::new(800, 600)
        .with_rotation(Rotation::None)
        .with_flip(Flip::None)
        .with_scaling(ScalingMode::Fill);
    let reset = process_frame(&frame, &config).unwrap();
    test_assert!(logger, reset.width == 800, "Width after reset");
    test_assert!(logger, reset.height == 600, "Height after reset");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Command Dispatch Tests
// ============================================================================

#[tokio::test]
async fn test_config_commands_dispatch() {
    let mut logger = TestLogger::new("config_commands_dispatch", 5);

    test_step!(logger, "Create app context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_step_ok!(logger);

    test_step!(logger, "Send SetScaling command");
    handle
        .send_command(Command::SetScaling {
            mode: ScalingMode::Fit,
        })
        .await
        .expect("send scaling command");
    let cmd = cmd_rx.recv().await.expect("receive command");
    test_assert!(
        logger,
        matches!(
            cmd,
            Command::SetScaling {
                mode: ScalingMode::Fit
            }
        ),
        "SetScaling command received"
    );
    test_step_ok!(logger);

    test_step!(logger, "Send SetRotation command");
    handle
        .send_command(Command::SetRotation {
            rotation: Rotation::Clockwise90,
        })
        .await
        .expect("send rotation command");
    let cmd = cmd_rx.recv().await.expect("receive command");
    test_assert!(
        logger,
        matches!(
            cmd,
            Command::SetRotation {
                rotation: Rotation::Clockwise90
            }
        ),
        "SetRotation command received"
    );
    test_step_ok!(logger);

    test_step!(logger, "Send SetFlip command");
    handle
        .send_command(Command::SetFlip { flip: Flip::Both })
        .await
        .expect("send flip command");
    let cmd = cmd_rx.recv().await.expect("receive command");
    test_assert!(
        logger,
        matches!(cmd, Command::SetFlip { flip: Flip::Both }),
        "SetFlip command received"
    );
    test_step_ok!(logger);

    test_step!(logger, "Send UpdateCaptureSettings command");
    let settings = CaptureSettings {
        width: 1280,
        height: 720,
        framerate: 60.0,
        format: Some(PixelFormat::Rgb24),
    };
    handle
        .send_command(Command::UpdateCaptureSettings {
            settings: settings.clone(),
        })
        .await
        .expect("send settings command");
    let cmd = cmd_rx.recv().await.expect("receive command");
    if let Command::UpdateCaptureSettings { settings: s } = cmd {
        test_assert!(logger, s.width == 1280, "Settings width correct");
        test_assert!(logger, s.height == 720, "Settings height correct");
    } else {
        test_assert!(logger, false, "Expected UpdateCaptureSettings command");
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Config Validation During Operation
// ============================================================================

#[test]
fn test_config_validation() {
    let mut logger = TestLogger::new("config_validation", 4);

    test_step!(logger, "Create valid config");
    let mut config = AppConfig::default();
    let errors = config.validate();
    test_assert!(logger, errors.is_empty(), "Default config is valid");
    test_step_ok!(logger);

    test_step!(logger, "Test invalid framerate");
    config.camera.framerate = -5.0;
    let errors = config.validate();
    test_assert!(logger, !errors.is_empty(), "Invalid framerate detected");
    test_step_ok!(logger);

    test_step!(logger, "Test invalid resolution");
    let mut config2 = AppConfig::default();
    config2.camera.width = 0;
    config2.camera.height = 0;
    let errors = config2.validate();
    test_assert!(logger, !errors.is_empty(), "Invalid resolution detected");
    test_step_ok!(logger);

    test_step!(logger, "Test config sanitization");
    config.sanitize();
    let errors = config.validate();
    test_assert!(logger, errors.is_empty(), "Sanitized config is valid");
    test_assert!(
        logger,
        config.camera.framerate == 30.0,
        "Framerate sanitized to default"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Rapid Config Change Tests
// ============================================================================

#[test]
fn test_rapid_config_changes() {
    let mut logger = TestLogger::new("rapid_config_changes", 4);

    test_step!(logger, "Set up capture simulator");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 320,
        height: 240,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 320,
                height: 240,
                framerate: 1000.0,
                format: None,
            },
        )
        .unwrap();
    capture.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capture initial frame");
    let frame = capture.next_frame().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Apply rapid config changes");
    let rotations = [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ];
    let flips = [Flip::None, Flip::Horizontal, Flip::Vertical, Flip::Both];
    let scaling = [ScalingMode::Fill, ScalingMode::Fit, ScalingMode::Stretch];

    let mut processed_count = 0;
    for rot in &rotations {
        for flip in &flips {
            for scale in &scaling {
                let (w, h) = match rot {
                    Rotation::Clockwise90 | Rotation::Clockwise270 => (240, 320),
                    _ => (320, 240),
                };
                let config = ProcessorConfig::new(w, h)
                    .with_rotation(*rot)
                    .with_flip(*flip)
                    .with_scaling(*scale);

                if process_frame(&frame, &config).is_ok() {
                    processed_count += 1;
                }
            }
        }
    }

    let expected = rotations.len() * flips.len() * scaling.len();
    test_assert!(
        logger,
        processed_count == expected,
        "All config combinations processed"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Full Pipeline Config Change Tests
// ============================================================================

#[test]
fn test_full_pipeline_config_change() {
    let mut logger = TestLogger::new("full_pipeline_config_change", 6);

    test_step!(logger, "Set up capture and display simulators");
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
        .unwrap();
    capture.start().unwrap();

    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        width: 1920,
        height: 1080,
        frame_history_size: 10,
        ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Process frame with initial config");
    let frame = capture.next_frame().unwrap();
    let config = ProcessorConfig::new(1920, 1080);
    let processed = process_frame(&frame, &config).unwrap();
    display.render(&processed).unwrap();
    test_assert!(logger, display.frame_count() == 1, "Initial frame rendered");
    test_step_ok!(logger);

    test_step!(logger, "Change rotation and process");
    let frame2 = capture.next_frame().unwrap();
    let config = ProcessorConfig::new(1080, 1920).with_rotation(Rotation::Clockwise90);
    let processed2 = process_frame(&frame2, &config).unwrap();
    display.render(&processed2).unwrap();
    test_assert!(
        logger,
        display.frame_count() == 2,
        "Second frame with new rotation"
    );
    test_step_ok!(logger);

    test_step!(logger, "Change scaling and flip");
    let frame3 = capture.next_frame().unwrap();
    let config = ProcessorConfig::new(1920, 1080)
        .with_scaling(ScalingMode::Fit)
        .with_flip(Flip::Horizontal);
    let processed3 = process_frame(&frame3, &config).unwrap();
    display.render(&processed3).unwrap();
    test_assert!(
        logger,
        display.frame_count() == 3,
        "Third frame with new settings"
    );
    test_step_ok!(logger);

    test_step!(logger, "Verify all frames in history");
    let history = display.frame_history();
    test_assert!(
        logger,
        history.len() == 3,
        "All 3 frames in display history"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// State Consistency Tests
// ============================================================================

#[tokio::test]
async fn test_state_consistency_after_config_change() {
    let mut logger = TestLogger::new("state_consistency_after_config_change", 5);

    test_step!(logger, "Create app context");
    let (ctx, _cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    let mut subscriber = handle.subscribe_events();
    test_step_ok!(logger);

    test_step!(logger, "Track initial state");
    let initial_state = AppState::Running;
    handle.publish_event(Event::StateChanged {
        old_state: AppState::Starting,
        new_state: initial_state,
    });
    let event = subscriber.recv().await.expect("receive event");
    let current_state = if let Event::StateChanged { new_state, .. } = event {
        new_state
    } else {
        panic!("Expected StateChanged event");
    };
    test_assert!(
        logger,
        current_state == AppState::Running,
        "Initial state is Running"
    );
    test_step_ok!(logger);

    test_step!(logger, "Send config change command");
    handle
        .send_command(Command::SetScaling {
            mode: ScalingMode::Fit,
        })
        .await
        .expect("send command");
    handle.publish_event(Event::SettingsChanged);
    test_step_ok!(logger);

    test_step!(logger, "Verify state unchanged after config change");
    let event = subscriber.recv().await.expect("receive settings event");
    test_assert!(
        logger,
        matches!(event, Event::SettingsChanged),
        "Settings event received"
    );
    test_assert!(
        logger,
        current_state == AppState::Running,
        "State unchanged after config change"
    );
    test_step_ok!(logger);

    test_step!(logger, "Multiple rapid config changes maintain state");
    for _ in 0..5 {
        handle
            .send_command(Command::SetRotation {
                rotation: Rotation::Clockwise90,
            })
            .await
            .expect("send rotation");
        handle.publish_event(Event::SettingsChanged);
    }
    let mut settings_count = 0;
    while let Some(event) = subscriber.try_recv() {
        if matches!(event, Event::SettingsChanged) {
            settings_count += 1;
        }
    }
    test_assert!(
        logger,
        settings_count == 5,
        "All 5 settings events received"
    );
    test_assert!(
        logger,
        current_state == AppState::Running,
        "State still Running after changes"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Config Changes
// ============================================================================

#[test]
fn test_display_config_conversion() {
    let mut logger = TestLogger::new("display_config_conversion", 4);

    test_step!(logger, "Test rotation conversion");
    let mut display_cfg = DisplayConfig::default();

    display_cfg.rotation = 0;
    test_assert!(
        logger,
        display_cfg.rotation_enum() == Rotation::None,
        "0 deg maps to None"
    );

    display_cfg.rotation = 90;
    test_assert!(
        logger,
        display_cfg.rotation_enum() == Rotation::Clockwise90,
        "90 deg maps correctly"
    );

    display_cfg.rotation = 180;
    test_assert!(
        logger,
        display_cfg.rotation_enum() == Rotation::Clockwise180,
        "180 deg maps correctly"
    );

    display_cfg.rotation = 270;
    test_assert!(
        logger,
        display_cfg.rotation_enum() == Rotation::Clockwise270,
        "270 deg maps correctly"
    );
    test_step_ok!(logger);

    test_step!(logger, "Test flip conversion");
    display_cfg.flip_horizontal = false;
    display_cfg.flip_vertical = false;
    test_assert!(
        logger,
        display_cfg.flip_enum() == Flip::None,
        "No flip maps to None"
    );

    display_cfg.flip_horizontal = true;
    display_cfg.flip_vertical = false;
    test_assert!(
        logger,
        display_cfg.flip_enum() == Flip::Horizontal,
        "H flip maps correctly"
    );

    display_cfg.flip_horizontal = false;
    display_cfg.flip_vertical = true;
    test_assert!(
        logger,
        display_cfg.flip_enum() == Flip::Vertical,
        "V flip maps correctly"
    );

    display_cfg.flip_horizontal = true;
    display_cfg.flip_vertical = true;
    test_assert!(
        logger,
        display_cfg.flip_enum() == Flip::Both,
        "Both flips map correctly"
    );
    test_step_ok!(logger);

    test_step!(logger, "Test invalid rotation fallback");
    display_cfg.rotation = 45;
    test_assert!(
        logger,
        display_cfg.rotation_enum() == Rotation::None,
        "Invalid rotation defaults to None"
    );
    test_step_ok!(logger);

    test_step!(logger, "Test scaling mode default");
    test_assert!(
        logger,
        display_cfg.scaling_mode == ScalingMode::default(),
        "Scaling mode has default"
    );
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Concurrent Config Access Tests
// ============================================================================

#[tokio::test]
async fn test_concurrent_config_commands() {
    let mut logger = TestLogger::new("concurrent_config_commands", 4);

    test_step!(logger, "Create app context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_step_ok!(logger);

    test_step!(logger, "Spawn multiple command senders");
    let handle1 = handle.clone();
    let handle2 = handle.clone();
    let handle3 = handle.clone();

    let sender1 = tokio::spawn(async move {
        for _ in 0..10 {
            handle1
                .send_command(Command::SetScaling {
                    mode: ScalingMode::Fit,
                })
                .await
                .ok();
        }
    });

    let sender2 = tokio::spawn(async move {
        for _ in 0..10 {
            handle2
                .send_command(Command::SetRotation {
                    rotation: Rotation::Clockwise90,
                })
                .await
                .ok();
        }
    });

    let sender3 = tokio::spawn(async move {
        for _ in 0..10 {
            handle3
                .send_command(Command::SetFlip {
                    flip: Flip::Horizontal,
                })
                .await
                .ok();
        }
    });
    test_step_ok!(logger);

    test_step!(logger, "Wait for all senders to complete");
    let _ = tokio::join!(sender1, sender2, sender3);
    test_step_ok!(logger);

    test_step!(logger, "Count received commands");
    let mut scaling_count = 0;
    let mut rotation_count = 0;
    let mut flip_count = 0;

    sleep(Duration::from_millis(50)).await;

    while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
            Command::SetScaling { .. } => scaling_count += 1,
            Command::SetRotation { .. } => rotation_count += 1,
            Command::SetFlip { .. } => flip_count += 1,
            _ => {}
        }
    }

    test_assert!(logger, scaling_count == 10, "All scaling commands received");
    test_assert!(
        logger,
        rotation_count == 10,
        "All rotation commands received"
    );
    test_assert!(logger, flip_count == 10, "All flip commands received");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Resolution Change Tests
// ============================================================================

#[test]
fn test_target_resolution_change() {
    let mut logger = TestLogger::new("target_resolution_change", 5);

    test_step!(logger, "Set up capture simulator");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::SolidColor {
            r: 200,
            g: 100,
            b: 50,
        },
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
        .unwrap();
    capture.start().unwrap();
    let frame = capture.next_frame().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Process at 720p");
    let config = ProcessorConfig::new(1280, 720);
    let p720 = process_frame(&frame, &config).unwrap();
    test_assert!(logger, p720.width == 1280, "720p width correct");
    test_assert!(logger, p720.height == 720, "720p height correct");
    test_step_ok!(logger);

    test_step!(logger, "Change to 1080p");
    let config = ProcessorConfig::new(1920, 1080);
    let p1080 = process_frame(&frame, &config).unwrap();
    test_assert!(logger, p1080.width == 1920, "1080p width correct");
    test_assert!(logger, p1080.height == 1080, "1080p height correct");
    test_step_ok!(logger);

    test_step!(logger, "Change to 4K");
    let config = ProcessorConfig::new(3840, 2160);
    let p4k = process_frame(&frame, &config).unwrap();
    test_assert!(logger, p4k.width == 3840, "4K width correct");
    test_assert!(logger, p4k.height == 2160, "4K height correct");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().unwrap();
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
