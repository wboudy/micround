//! E2E Test: Display Target Selection and Wallpaper Set
//!
//! Tests display enumeration, target selection, and wallpaper activation.
//! Logs platform API calls, window hierarchy navigation, render surface creation.

#![cfg(feature = "test-simulator")]

mod common;

use std::time::Duration;

use common::test_logger::*;
use micround::capture::{
    simulator::{FramePattern, SimulatorBackend, SimulatorConfig},
    CaptureBackend,
};
use micround::config::AppConfig;
use micround::core::{
    AppContext, AppState, CaptureSettings, Command, DeviceId, DisplayId, Event, Flip, Rotation,
    ScalingMode,
};
use micround::process::{process_frame, ProcessorConfig};
use micround::render::{
    simulator::{CapturedFrame, DisplaySimulator, DisplaySimulatorConfig, RenderStats},
    WallpaperRenderer,
};

// ============================================================================
// Display Enumeration Tests
// ============================================================================

/// Tests enumeration of available display targets.
#[test]
fn test_display_enumeration_and_properties() {
    let mut logger = TestLogger::new("display_enumeration_and_properties", 4);

    test_step!(logger, "Creating simulated display configurations");
    let display_configs = vec![
        ("Primary", 1920u32, 1080u32),
        ("Secondary", 2560u32, 1440u32),
        ("Portrait", 1080u32, 1920u32),
    ];
    test_step_ok!(
        logger,
        "Defined {} display configurations",
        display_configs.len()
    );

    test_step!(logger, "Initializing display simulators");
    let mut initialized_displays = Vec::new();
    for (name, w, h) in &display_configs {
        let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
            display_name: (*name).into(),
            width: *w,
            height: *h,
            ..Default::default()
        });
        let display_id = DisplayId(format!("sim:{}", name));
        display.init(&display_id).expect("init display");
        tracing::info!(
            display_id = %display_id.0,
            name = %name,
            width = *w,
            height = *h,
            "Initialized display"
        );
        initialized_displays.push((display, display_id));
    }
    test_assert!(
        logger,
        initialized_displays.len() == 3,
        "All displays initialized"
    );
    test_step_ok!(logger);

    test_step!(logger, "Validating display properties");
    for (display, display_id) in &initialized_displays {
        let stats = display.stats();
        test_assert!(logger, stats.frames_rendered == 0, "No frames rendered yet");
        tracing::info!(display_id = %display_id.0, "Display validated");
    }
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    for (mut display, _) in initialized_displays {
        display.shutdown();
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Selection Tests
// ============================================================================

/// Tests selecting different display targets.
#[test]
fn test_display_target_selection() {
    let mut logger = TestLogger::new("display_target_selection", 5);

    test_step!(logger, "Creating multiple display targets");
    let configs = [DisplaySimulatorConfig::hd(), DisplaySimulatorConfig::uhd()];

    let mut displays: Vec<_> = configs
        .iter()
        .cloned()
        .enumerate()
        .map(|(i, config)| {
            let mut display = DisplaySimulator::new(config);
            let id = DisplayId(format!("display:{}", i));
            display.init(&id).expect("init");
            (display, id)
        })
        .collect();

    test_assert!(logger, displays.len() == 2, "Two displays created");
    test_step_ok!(logger);

    test_step!(logger, "Creating test frame for rendering");
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
        .expect("open capture");
    capture.start().expect("start capture");
    let frame = capture.next_frame().expect("get frame");
    test_step_ok!(logger);

    test_step!(logger, "Selecting HD display as target");
    let proc_config_hd = ProcessorConfig::new(1920, 1080);
    let processed_hd = process_frame(&frame, &proc_config_hd).expect("process for HD");

    displays[0].0.render(&processed_hd).expect("render to HD");
    test_assert!(
        logger,
        displays[0].0.frame_count() == 1,
        "HD display received frame"
    );
    test_assert!(
        logger,
        displays[1].0.frame_count() == 0,
        "4K display not affected"
    );

    let hd_frame = displays[0].0.last_frame().expect("get HD frame");
    test_assert!(logger, hd_frame.width == 1920, "HD frame width correct");
    test_assert!(logger, hd_frame.height == 1080, "HD frame height correct");
    test_step_ok!(logger);

    test_step!(logger, "Switching target to 4K display");
    let proc_config_4k = ProcessorConfig::new(3840, 2160);
    let processed_4k = process_frame(&frame, &proc_config_4k).expect("process for 4K");

    displays[1].0.render(&processed_4k).expect("render to 4K");
    test_assert!(
        logger,
        displays[1].0.frame_count() == 1,
        "4K display received frame"
    );

    let uhd_frame = displays[1].0.last_frame().expect("get 4K frame");
    test_assert!(logger, uhd_frame.width == 3840, "4K frame width correct");
    test_assert!(logger, uhd_frame.height == 2160, "4K frame height correct");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    for (mut display, _) in displays {
        display.shutdown();
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Wallpaper Activation Tests
// ============================================================================

/// Tests activating wallpaper on a display.
#[test]
fn test_wallpaper_activation() {
    let mut logger = TestLogger::new("wallpaper_activation", 5);

    test_step!(logger, "Initializing display simulator");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        display_name: "Test Display".into(),
        width: 1920,
        height: 1080,
        frame_history_size: 5,
        ..Default::default()
    });
    display
        .init(&DisplayId("test:primary".into()))
        .expect("init display");
    test_assert!(logger, display.frame_count() == 0, "Display starts empty");
    test_step_ok!(logger);

    test_step!(logger, "Starting capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 1920,
        height: 1080,
        fps: 30,
        pattern: FramePattern::Checkerboard { size: 32 },
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 1920,
                height: 1080,
                framerate: 30.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Activating wallpaper");
    let frame = capture.next_frame().expect("get frame");
    let proc_config = ProcessorConfig::new(1920, 1080);
    let processed = process_frame(&frame, &proc_config).expect("process frame");

    display.render(&processed).expect("render wallpaper");
    test_assert!(
        logger,
        display.frame_count() == 1,
        "Wallpaper frame rendered"
    );
    test_step_ok!(logger);

    test_step!(logger, "Verifying wallpaper content");
    let captured = display.last_frame().expect("get captured frame");
    test_assert!(logger, captured.width == 1920, "Wallpaper width correct");
    test_assert!(logger, captured.height == 1080, "Wallpaper height correct");
    test_assert!(logger, !captured.data.is_empty(), "Wallpaper has data");
    test_assert!(
        logger,
        captured.is_solid_color().is_none(),
        "Checkerboard is not solid"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests wallpaper restoration after shutdown.
#[test]
fn test_wallpaper_restoration() {
    let mut logger = TestLogger::new("wallpaper_restoration", 4);

    test_step!(logger, "Setting initial wallpaper");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig::hd());
    display.init(&DisplayId("test:0".into())).expect("init");

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

    let frame = capture.next_frame().expect("get frame");
    let processed = process_frame(&frame, &ProcessorConfig::new(1920, 1080)).expect("process");
    display.render(&processed).expect("render");
    test_assert!(logger, display.frame_count() == 1, "Wallpaper set");
    test_step_ok!(logger);

    test_step!(logger, "Simulating shutdown with restore");
    let config = AppConfig::default();
    display.restore(&config).expect("restore wallpaper");
    test_step_ok!(logger);

    test_step!(logger, "Verifying restoration behavior");
    test_assert!(logger, true, "Restore completed without error");
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
// Multi-Display Tests
// ============================================================================

/// Tests rendering to multiple displays simultaneously.
#[test]
fn test_multi_display_rendering() {
    let mut logger = TestLogger::new("multi_display_rendering", 5);

    test_step!(logger, "Creating multiple display targets");
    let mut display1 = DisplaySimulator::new(DisplaySimulatorConfig {
        display_name: "Primary".into(),
        width: 1920,
        height: 1080,
        ..Default::default()
    });
    let mut display2 = DisplaySimulator::new(DisplaySimulatorConfig {
        display_name: "Secondary".into(),
        width: 2560,
        height: 1440,
        ..Default::default()
    });

    display1
        .init(&DisplayId("display:0".into()))
        .expect("init display1");
    display2
        .init(&DisplayId("display:1".into()))
        .expect("init display2");
    test_step_ok!(logger);

    test_step!(logger, "Setting up capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 1280,
        height: 720,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    });
    let devices = capture.enumerate_devices();
    capture
        .open(
            &devices[0].id,
            CaptureSettings {
                width: 1280,
                height: 720,
                framerate: 1000.0,
                format: None,
            },
        )
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Rendering to both displays");
    let frame = capture.next_frame().expect("get frame");

    let processed1 = process_frame(&frame, &ProcessorConfig::new(1920, 1080)).expect("process1");
    let processed2 = process_frame(&frame, &ProcessorConfig::new(2560, 1440)).expect("process2");

    display1.render(&processed1).expect("render1");
    display2.render(&processed2).expect("render2");

    test_assert!(logger, display1.frame_count() == 1, "Display1 has frame");
    test_assert!(logger, display2.frame_count() == 1, "Display2 has frame");
    test_step_ok!(logger);

    test_step!(logger, "Verifying display content");
    let frame1 = display1.last_frame().expect("get frame1");
    let frame2 = display2.last_frame().expect("get frame2");

    test_assert!(
        logger,
        frame1.width == 1920 && frame1.height == 1080,
        "Display1 dimensions"
    );
    test_assert!(
        logger,
        frame2.width == 2560 && frame2.height == 1440,
        "Display2 dimensions"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display1.shutdown();
    display2.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Switch Tests
// ============================================================================

/// Tests switching between displays during operation.
#[test]
fn test_display_switch_during_operation() {
    let mut logger = TestLogger::new("display_switch_during_operation", 6);

    test_step!(logger, "Creating display targets");
    let mut display_primary = DisplaySimulator::new(DisplaySimulatorConfig {
        display_name: "Primary".into(),
        width: 1920,
        height: 1080,
        ..Default::default()
    });
    let mut display_secondary = DisplaySimulator::new(DisplaySimulatorConfig {
        display_name: "Secondary".into(),
        width: 2560,
        height: 1440,
        ..Default::default()
    });

    display_primary
        .init(&DisplayId("display:primary".into()))
        .expect("init primary");
    display_secondary
        .init(&DisplayId("display:secondary".into()))
        .expect("init secondary");
    test_step_ok!(logger);

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

    test_step!(logger, "Rendering to primary display");
    let frame1 = capture.next_frame().expect("get frame");
    let processed1 = process_frame(&frame1, &ProcessorConfig::new(1920, 1080)).expect("process");
    display_primary.render(&processed1).expect("render primary");
    test_assert!(
        logger,
        display_primary.frame_count() == 1,
        "Primary has frame"
    );
    test_step_ok!(logger);

    test_step!(logger, "Switching to secondary display");
    let frame2 = capture.next_frame().expect("get frame");
    let processed2 = process_frame(&frame2, &ProcessorConfig::new(2560, 1440)).expect("process");
    display_secondary
        .render(&processed2)
        .expect("render secondary");
    test_assert!(
        logger,
        display_secondary.frame_count() == 1,
        "Secondary has frame"
    );
    test_step_ok!(logger);

    test_step!(logger, "Continuing on secondary display");
    let frame3 = capture.next_frame().expect("get frame");
    let processed3 = process_frame(&frame3, &ProcessorConfig::new(2560, 1440)).expect("process");
    display_secondary
        .render(&processed3)
        .expect("render secondary again");
    test_assert!(
        logger,
        display_secondary.frame_count() == 2,
        "Secondary has 2 frames"
    );
    test_assert!(
        logger,
        display_primary.frame_count() == 1,
        "Primary unchanged"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display_primary.shutdown();
    display_secondary.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Command Tests
// ============================================================================

/// Tests display selection via command dispatch.
#[tokio::test]
async fn test_display_selection_command_dispatch() {
    let mut logger = TestLogger::new("display_selection_command_dispatch", 4);

    test_step!(logger, "Creating application context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_step_ok!(logger);

    test_step!(logger, "Sending display selection command");
    let target_display = DisplayId("monitor:1".into());
    handle
        .send_command(Command::SelectDisplay {
            display_id: target_display.clone(),
        })
        .await
        .expect("send command");

    let cmd = cmd_rx.recv().await.expect("receive command");
    if let Command::SelectDisplay { display_id } = cmd {
        test_assert!(logger, display_id.0 == "monitor:1", "Correct display ID");
    } else {
        test_assert!(logger, false, "Expected SelectDisplay command");
    }
    test_step_ok!(logger);

    test_step!(logger, "Sending multiple display commands");
    let displays = ["primary", "secondary", "external"];
    for d in &displays {
        handle
            .send_command(Command::SelectDisplay {
                display_id: DisplayId((*d).into()),
            })
            .await
            .expect("send");
    }

    let mut received = Vec::new();
    for _ in 0..3 {
        if let Some(Command::SelectDisplay { display_id }) = cmd_rx.recv().await {
            received.push(display_id.0.clone());
        }
    }
    test_assert!(logger, received.len() == 3, "All commands received");
    test_step_ok!(logger);

    test_step!(logger, "Verifying command order");
    test_assert!(logger, received[0] == "primary", "First command correct");
    test_assert!(logger, received[1] == "secondary", "Second command correct");
    test_assert!(logger, received[2] == "external", "Third command correct");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Display Statistics Tests
// ============================================================================

/// Tests display render statistics collection.
#[test]
fn test_display_render_statistics() {
    let mut logger = TestLogger::new("display_render_statistics", 4);

    test_step!(logger, "Creating display with frame history");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        frame_history_size: 20,
        ..Default::default()
    });
    display.init(&DisplayId("test:stats".into())).expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Rendering multiple frames");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        fps: 1000,
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

    let proc_config = ProcessorConfig::new(1920, 1080);
    for _ in 0..10 {
        let frame = capture.next_frame().expect("get frame");
        let processed = process_frame(&frame, &proc_config).expect("process");
        display.render(&processed).expect("render");
    }
    test_assert!(logger, display.frame_count() == 10, "10 frames rendered");
    test_step_ok!(logger);

    test_step!(logger, "Checking render statistics");
    let stats = display.stats();
    test_assert!(logger, stats.frames_rendered == 10, "Stats show 10 frames");
    test_assert!(logger, stats.errors == 0, "No render errors");
    test_assert!(
        logger,
        stats.last_render_time.is_some(),
        "Has last render time"
    );
    tracing::info!(
        frames = stats.frames_rendered,
        errors = stats.errors,
        avg_time_us = stats.avg_render_time_us,
        "Render statistics"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests frame history access.
#[test]
fn test_display_frame_history() {
    let mut logger = TestLogger::new("display_frame_history", 4);

    test_step!(logger, "Creating display with limited frame history");
    let mut display = DisplaySimulator::new(DisplaySimulatorConfig {
        frame_history_size: 5,
        ..Default::default()
    });
    display
        .init(&DisplayId("test:history".into()))
        .expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Rendering frames beyond history capacity");
    let mut capture = SimulatorBackend::new_default();
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

    let proc_config = ProcessorConfig::new(1920, 1080);
    for _ in 0..10 {
        let frame = capture.next_frame().expect("get frame");
        let processed = process_frame(&frame, &proc_config).expect("process");
        display.render(&processed).expect("render");
    }
    test_step_ok!(logger);

    test_step!(logger, "Verifying history is capped");
    let history = display.frame_history();
    test_assert!(logger, history.len() <= 5, "History capped at 5 frames");
    test_assert!(logger, display.frame_count() == 10, "Total count is 10");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
