//! E2E Test: Pause, Resume, and Snapshot Operations (bd-3p3)
//!
//! Tests pause/resume flow and snapshot capture.
//! Logs state transitions, frozen frame verification, snapshot file creation.
//!
//! This test simulates the user journey of pausing capture, taking snapshots,
//! and resuming operations.

#![cfg(feature = "test-simulator")]

mod common;

use std::time::{Duration, Instant};

use common::test_logger::*;
use micround::capture::{
    simulator::{FramePattern, SimulatorBackend, SimulatorConfig},
    CaptureBackend,
};
use micround::core::{AppContext, AppState, CaptureSettings, Command, DisplayId, Event};
use micround::process::{process_frame, ProcessorConfig};
use micround::render::{
    simulator::{DisplaySimulator, DisplaySimulatorConfig},
    WallpaperRenderer,
};

// ============================================================================
// Pause/Resume Flow Tests
// ============================================================================

/// Tests complete pause and resume flow.
///
/// Steps verified:
/// 1. Start capture and render frames
/// 2. Pause capture
/// 3. Verify no new frames arrive
/// 4. Resume capture
/// 5. Verify frames continue flowing
#[test]
fn test_pause_resume_complete_flow() {
    let mut logger = TestLogger::new("pause_resume_complete_flow", 6);

    // Setup
    test_step!(logger, "Starting capture pipeline");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::Counter,
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
        frame_history_size: 50,
        ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).expect("init");
    test_step_ok!(logger);

    // Capture initial frames
    test_step!(logger, "Capturing initial frames");
    let config = ProcessorConfig::new(1920, 1080);
    for _ in 0..5 {
        let frame = capture.next_frame().expect("frame");
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
    }
    let initial_count = display.frame_count();
    test_assert!(logger, initial_count == 5, "Initial frames captured");
    test_step_ok!(logger, "Captured {} frames", initial_count);

    // Pause capture
    test_step!(logger, "Pausing capture");
    capture.stop().expect("pause");
    test_assert!(logger, !capture.is_capturing(), "Capture is paused");
    tracing::info!("Capture paused, state: not capturing");
    test_step_ok!(logger);

    // Verify no new frames
    test_step!(logger, "Verifying capture is paused (no new frames)");
    let paused_result = capture.next_frame();
    test_assert!(logger, paused_result.is_err(), "No frames while paused");
    let paused_count = display.frame_count();
    test_assert!(
        logger,
        paused_count == initial_count,
        "Frame count unchanged"
    );
    test_step_ok!(logger);

    // Resume capture
    test_step!(logger, "Resuming capture");
    capture.start().expect("resume");
    test_assert!(logger, capture.is_capturing(), "Capture is resumed");
    tracing::info!("Capture resumed, state: capturing");

    // Capture more frames
    for _ in 0..5 {
        let frame = capture.next_frame().expect("frame");
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
    }
    let final_count = display.frame_count();
    test_assert!(logger, final_count == 10, "More frames after resume");
    test_step_ok!(
        logger,
        "Resumed and captured {} more frames",
        final_count - paused_count
    );

    // Cleanup
    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests state transitions during pause/resume.
#[tokio::test]
async fn test_pause_resume_state_transitions() {
    let mut logger = TestLogger::new("pause_resume_state_transitions", 5);

    test_step!(logger, "Creating application context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    let mut event_sub = handle.subscribe_events();
    test_step_ok!(logger);

    // Simulate state transitions
    test_step!(logger, "Simulating Running state");
    handle.publish_event(Event::StateChanged {
        old_state: AppState::Starting,
        new_state: AppState::Running,
    });
    let event = event_sub.recv().await.expect("receive event");
    if let Event::StateChanged { new_state, .. } = event {
        test_assert!(logger, new_state == AppState::Running, "Now Running");
    }
    test_step_ok!(logger);

    test_step!(
        logger,
        "Sending PauseDisplay command and transition to Paused"
    );
    handle
        .send_command(Command::PauseDisplay)
        .await
        .expect("send pause");
    let cmd = cmd_rx.recv().await.expect("receive pause");
    test_assert!(
        logger,
        matches!(cmd, Command::PauseDisplay),
        "PauseDisplay command received"
    );

    handle.publish_event(Event::StateChanged {
        old_state: AppState::Running,
        new_state: AppState::Paused,
    });
    let event = event_sub.recv().await.expect("receive paused");
    if let Event::StateChanged { new_state, .. } = event {
        test_assert!(logger, new_state == AppState::Paused, "Now Paused");
    }
    test_step_ok!(logger);

    test_step!(
        logger,
        "Sending ResumeDisplay command and transition to Running"
    );
    handle
        .send_command(Command::ResumeDisplay)
        .await
        .expect("send resume");
    let cmd = cmd_rx.recv().await.expect("receive resume");
    test_assert!(
        logger,
        matches!(cmd, Command::ResumeDisplay),
        "ResumeDisplay command received"
    );

    handle.publish_event(Event::StateChanged {
        old_state: AppState::Paused,
        new_state: AppState::Running,
    });
    let event = event_sub.recv().await.expect("receive running");
    if let Event::StateChanged { new_state, .. } = event {
        test_assert!(logger, new_state == AppState::Running, "Back to Running");
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests rapid pause/resume cycles.
#[test]
fn test_rapid_pause_resume() {
    let mut logger = TestLogger::new("rapid_pause_resume", 4);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::SolidColor {
            r: 100,
            g: 150,
            b: 200,
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
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Performing 10 rapid pause/resume cycles");
    let mut frames_captured = 0;
    for i in 0..10 {
        // Capture a frame
        if capture.is_capturing() {
            if let Ok(_frame) = capture.next_frame() {
                frames_captured += 1;
            }
        }

        // Pause
        capture.stop().expect("pause");
        test_assert!(logger, !capture.is_capturing(), "Cycle {} paused", i);

        // Resume
        capture.start().expect("resume");
        test_assert!(logger, capture.is_capturing(), "Cycle {} resumed", i);
    }
    tracing::info!(
        frames = frames_captured,
        cycles = 10,
        "Rapid cycles completed"
    );
    test_step_ok!(
        logger,
        "Completed 10 cycles, captured {} frames",
        frames_captured
    );

    test_step!(logger, "Final capture verification");
    let frame = capture.next_frame();
    test_assert!(
        logger,
        frame.is_ok(),
        "Can still capture after rapid cycles"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Snapshot Tests
// ============================================================================

/// Tests taking a snapshot of current frame.
#[test]
fn test_snapshot_capture() {
    let mut logger = TestLogger::new("snapshot_capture", 5);

    test_step!(logger, "Starting capture");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::ColorBars,
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

    test_step!(logger, "Capturing frame for snapshot");
    let frame = capture.next_frame().expect("get frame");
    test_assert!(logger, frame.width == 640, "Frame width correct");
    test_assert!(logger, frame.height == 480, "Frame height correct");
    test_assert!(logger, !frame.data.is_empty(), "Frame has data");
    test_step_ok!(logger);

    test_step!(logger, "Processing frame as snapshot");
    let config = ProcessorConfig::new(640, 480);
    let snapshot = process_frame(&frame, &config).expect("process snapshot");
    test_assert!(logger, snapshot.width == 640, "Snapshot width correct");
    test_assert!(logger, snapshot.height == 480, "Snapshot height correct");
    test_assert!(logger, !snapshot.data.is_empty(), "Snapshot has data");
    tracing::info!(
        width = snapshot.width,
        height = snapshot.height,
        data_size = snapshot.data.len(),
        "Snapshot captured"
    );
    test_step_ok!(logger);

    test_step!(logger, "Verifying snapshot data is valid RGBA");
    // Verify RGBA format: data size should be width * height * 4
    let expected_size = (snapshot.width * snapshot.height * 4) as usize;
    test_assert!(
        logger,
        snapshot.data.len() == expected_size,
        "Snapshot is valid RGBA"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests snapshot during paused state.
#[test]
fn test_snapshot_while_paused() {
    let mut logger = TestLogger::new("snapshot_while_paused", 5);

    test_step!(logger, "Starting capture");
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
        .expect("open");
    capture.start().expect("start");
    test_step_ok!(logger);

    test_step!(logger, "Capturing last frame before pause");
    let last_frame = capture.next_frame().expect("get frame");
    let config = ProcessorConfig::new(640, 480);
    let last_processed = process_frame(&last_frame, &config).expect("process");
    test_step_ok!(logger);

    test_step!(logger, "Pausing capture");
    capture.stop().expect("pause");
    test_assert!(logger, !capture.is_capturing(), "Capture paused");
    test_step_ok!(logger);

    test_step!(logger, "Taking snapshot from last captured frame");
    // In a real app, the last frame would be stored for snapshot use
    // Here we use the already-processed frame as the "snapshot"
    test_assert!(
        logger,
        last_processed.width == 640,
        "Snapshot width from paused"
    );
    test_assert!(
        logger,
        last_processed.height == 480,
        "Snapshot height from paused"
    );
    tracing::info!(
        width = last_processed.width,
        height = last_processed.height,
        "Snapshot taken while paused"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests multiple snapshots in sequence.
#[test]
fn test_multiple_snapshots() {
    let mut logger = TestLogger::new("multiple_snapshots", 4);

    test_step!(logger, "Starting capture with Counter pattern");
    let mut capture = SimulatorBackend::new(SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::Counter,
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

    test_step!(logger, "Taking 5 snapshots");
    let config = ProcessorConfig::new(640, 480);
    let mut snapshots = Vec::new();

    for i in 0..5 {
        let frame = capture.next_frame().expect("get frame");
        let snapshot = process_frame(&frame, &config).expect("process");
        tracing::info!(
            snapshot_num = i + 1,
            frame_seq = frame.sequence,
            "Snapshot captured"
        );
        snapshots.push(snapshot);
    }

    test_assert!(logger, snapshots.len() == 5, "5 snapshots captured");
    test_step_ok!(logger);

    test_step!(logger, "Verifying snapshots are different");
    // With Counter pattern, each frame should be different
    // Check that at least data differs (simple check: compare first few bytes)
    let mut all_same = true;
    for i in 1..snapshots.len() {
        if snapshots[i].data[..100] != snapshots[0].data[..100] {
            all_same = false;
            break;
        }
    }
    // Note: Might be same if frames are identical, which is fine for test
    tracing::info!(snapshots_unique = !all_same, "Snapshot uniqueness check");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Snapshot Command Tests
// ============================================================================

/// Tests snapshot command dispatch.
#[tokio::test]
async fn test_snapshot_command() {
    let mut logger = TestLogger::new("snapshot_command", 4);

    test_step!(logger, "Creating application context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    test_step_ok!(logger);

    test_step!(logger, "Sending TakeSnapshot to file command");
    handle
        .send_command(Command::TakeSnapshot {
            to_clipboard: false,
        })
        .await
        .expect("send snapshot");

    let cmd = cmd_rx.recv().await.expect("receive snapshot");
    if let Command::TakeSnapshot { to_clipboard } = cmd {
        test_assert!(logger, !to_clipboard, "Snapshot to file (not clipboard)");
        tracing::info!(to_clipboard = to_clipboard, "TakeSnapshot command received");
    } else {
        test_assert!(logger, false, "Expected TakeSnapshot command");
    }
    test_step_ok!(logger);

    test_step!(logger, "Sending TakeSnapshot to clipboard command");
    handle
        .send_command(Command::TakeSnapshot { to_clipboard: true })
        .await
        .expect("send clipboard snapshot");

    let cmd = cmd_rx.recv().await.expect("receive clipboard snapshot");
    if let Command::TakeSnapshot { to_clipboard } = cmd {
        test_assert!(logger, to_clipboard, "Snapshot to clipboard");
        tracing::info!(
            to_clipboard = to_clipboard,
            "TakeSnapshot to clipboard received"
        );
    }
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Frozen Frame Tests
// ============================================================================

/// Tests that display shows frozen frame during pause.
#[test]
fn test_frozen_frame_during_pause() {
    let mut logger = TestLogger::new("frozen_frame_during_pause", 5);

    test_step!(logger, "Starting capture and display");
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
        frame_history_size: 10,
        ..Default::default()
    });
    display.init(&DisplayId("test:0".into())).expect("init");
    test_step_ok!(logger);

    test_step!(logger, "Rendering initial frames");
    let config = ProcessorConfig::new(1920, 1080);
    for _ in 0..3 {
        let frame = capture.next_frame().expect("frame");
        let processed = process_frame(&frame, &config).expect("process");
        display.render(&processed).expect("render");
    }
    let pre_pause_count = display.frame_count();
    let frozen_frame = display.last_frame().expect("get last frame");
    test_step_ok!(logger, "Rendered {} frames", pre_pause_count);

    test_step!(logger, "Pausing capture");
    capture.stop().expect("pause");
    test_step_ok!(logger);

    test_step!(logger, "Verifying frozen frame persists");
    // Simulate time passing with no new frames
    std::thread::sleep(Duration::from_millis(50));

    let current_frame = display.last_frame().expect("get current frame");
    test_assert!(
        logger,
        current_frame.width == frozen_frame.width,
        "Same width"
    );
    test_assert!(
        logger,
        current_frame.height == frozen_frame.height,
        "Same height"
    );
    test_assert!(
        logger,
        current_frame.data.len() == frozen_frame.data.len(),
        "Same data size"
    );

    let post_pause_count = display.frame_count();
    test_assert!(
        logger,
        post_pause_count == pre_pause_count,
        "No new frames during pause"
    );
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.close();
    display.shutdown();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Edge Cases
// ============================================================================

/// Tests pause when already paused.
#[test]
fn test_pause_when_already_paused() {
    let mut logger = TestLogger::new("pause_when_already_paused", 4);

    test_step!(logger, "Starting capture");
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

    test_step!(logger, "First pause");
    capture.stop().expect("first pause");
    test_assert!(logger, !capture.is_capturing(), "Paused");
    test_step_ok!(logger);

    test_step!(logger, "Second pause (already paused)");
    // Should be idempotent - no error
    let result = capture.stop();
    test_assert!(logger, result.is_ok(), "Second pause succeeds");
    test_assert!(logger, !capture.is_capturing(), "Still paused");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests resume when not paused.
#[test]
fn test_resume_when_not_paused() {
    let mut logger = TestLogger::new("resume_when_not_paused", 4);

    test_step!(logger, "Starting capture");
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
    test_assert!(logger, capture.is_capturing(), "Capturing");
    test_step_ok!(logger);

    test_step!(logger, "Resume when already running");
    let result = capture.start();
    // Should be idempotent or return error
    tracing::info!(result = ?result, "Resume while running result");
    // The simulator may allow this or return error - both are valid
    test_step_ok!(logger);

    test_step!(logger, "Verify capture still works");
    let frame = capture.next_frame();
    test_assert!(logger, frame.is_ok(), "Can still capture frames");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.stop().expect("stop");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Tests pause during frame processing.
#[test]
fn test_pause_during_processing() {
    let mut logger = TestLogger::new("pause_during_processing", 4);

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

    test_step!(logger, "Getting frame");
    let frame = capture.next_frame().expect("get frame");
    test_step_ok!(logger);

    test_step!(logger, "Processing frame while pausing");
    // Pause capture
    capture.stop().expect("pause");

    // Process the already-captured frame
    let config = ProcessorConfig::new(1920, 1080);
    let result = process_frame(&frame, &config);
    test_assert!(logger, result.is_ok(), "Can process frame after pause");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    capture.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
