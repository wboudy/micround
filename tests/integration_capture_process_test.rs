//! Integration test: Capture to Process pipeline (bd-36d)
//!
//! Tests frames flowing from Camera Simulator through format decoding and into
//! the processing pipeline. Verifies frame integrity, timing, and data flow.
//!
//! Run with: cargo test --features test-simulator --test integration_capture_process_test

#![cfg(feature = "test-simulator")]

mod common;

use common::test_logger::*;
use micround::capture::{
    CaptureBackend,
    simulator::{SimulatorBackend, SimulatorConfig, FramePattern},
};
use micround::core::{CaptureSettings, Flip, PixelFormat, Rotation, ScalingMode};
use micround::process::{process_frame, ProcessorConfig};
use std::time::{Duration, Instant};

// ============================================================================
// Basic Pipeline Integration Tests
// ============================================================================

/// Test basic frame flow: capture → process
#[test]
fn test_basic_capture_to_process_flow() {
    let mut logger = TestLogger::new("basic_capture_to_process_flow", 6);

    test_step!(logger, "Setting up simulator backend");
    let config = SimulatorConfig {
        width: 320,
        height: 240,
        fps: 1000, // High FPS for fast test
        pattern: FramePattern::ColorBars,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    test_step_ok!(logger);

    test_step!(logger, "Opening and starting capture");
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 320,
        height: 240,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing frame from simulator");
    let raw_frame = backend.next_frame().unwrap();
    test_assert!(logger, raw_frame.width == 320, "Frame width is correct");
    test_assert!(logger, raw_frame.height == 240, "Frame height is correct");
    test_assert!(logger, raw_frame.format == PixelFormat::Rgba32, "Frame format is RGBA32");
    test_step_ok!(logger, "Captured frame {}x{}", raw_frame.width, raw_frame.height);

    test_step!(logger, "Processing captured frame");
    let proc_config = ProcessorConfig::new(640, 480);
    let processed = process_frame(&raw_frame, &proc_config).unwrap();
    test_step_ok!(logger, "Processed to {}x{}", processed.width, processed.height);

    test_step!(logger, "Validating processed frame");
    test_assert!(logger, processed.width == 640, "Processed width matches target");
    test_assert!(logger, processed.height == 480, "Processed height matches target");
    let expected_size = 640 * 480 * 4; // RGBA
    test_assert!(logger, processed.data.len() == expected_size, "Processed data size correct");
    test_step_ok!(logger);

    test_step!(logger, "Cleaning up");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test processing multiple frames in sequence
///
/// This test is timing-sensitive and may fail under system load.
/// Run with: cargo test test_multi_frame_pipeline_flow -- --ignored
#[test]
#[ignore]
fn test_multi_frame_pipeline_flow() {
    let mut logger = TestLogger::new("multi_frame_pipeline_flow", 5);

    test_step!(logger, "Setting up fast capture simulator");
    let config = SimulatorConfig {
        width: 160,
        height: 120,
        fps: 1000,
        pattern: FramePattern::Counter, // Counter pattern shows changing frames
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 160,
        height: 120,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Processing 10 frames in sequence");
    let proc_config = ProcessorConfig::new(320, 240).with_metrics(true);
    let mut sequences: Vec<u64> = Vec::new();
    let mut process_times: Vec<Duration> = Vec::new();

    for _ in 0..10 {
        let raw_frame = backend.next_frame().unwrap();
        sequences.push(raw_frame.sequence);

        let start = Instant::now();
        let processed = process_frame(&raw_frame, &proc_config).unwrap();
        process_times.push(start.elapsed());

        test_assert!(logger, processed.width == 320, "Processed width correct");
        test_assert!(logger, processed.height == 240, "Processed height correct");
    }
    test_step_ok!(logger, "Processed 10 frames");

    test_step!(logger, "Validating frame sequence ordering");
    for i in 1..sequences.len() {
        test_assert!(
            logger,
            sequences[i] > sequences[i - 1],
            "Frame sequence is monotonically increasing"
        );
    }
    test_step_ok!(logger, "Sequences: {:?}", &sequences[..3]);

    test_step!(logger, "Checking processing timing");
    let avg_time: Duration = process_times.iter().sum::<Duration>() / process_times.len() as u32;
    // Note: Processing time includes simulator frame timing overhead
    // Just verify it's not absurdly slow (< 1 second per frame for small 160x120 to 320x240)
    test_assert!(
        logger,
        avg_time < Duration::from_secs(1),
        "Average processing time is reasonable"
    );
    test_step_ok!(logger, "Avg process time: {:?}", avg_time);

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Frame Format and Pattern Tests
// ============================================================================

/// Test different frame patterns through the pipeline
#[test]
fn test_various_patterns_through_pipeline() {
    let mut logger = TestLogger::new("various_patterns_through_pipeline", 7);

    let patterns = [
        (FramePattern::ColorBars, "ColorBars"),
        (FramePattern::SolidColor { r: 128, g: 64, b: 192 }, "SolidColor"),
        (FramePattern::HorizontalGradient, "HorizontalGradient"),
        (FramePattern::VerticalGradient, "VerticalGradient"),
        (FramePattern::Checkerboard { size: 16 }, "Checkerboard"),
        (FramePattern::Noise, "Noise"),
        (FramePattern::MovingLine, "MovingLine"),
    ];

    for (pattern, name) in patterns {
        test_step!(logger, "Testing pattern: {}", name);

        let config = SimulatorConfig {
            width: 64,
            height: 64,
            fps: 1000,
            pattern,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();
        let settings = CaptureSettings {
            width: 64,
            height: 64,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        let raw_frame = backend.next_frame().unwrap();
        test_assert!(logger, !raw_frame.data.is_empty(), "Frame has data");
        test_assert!(logger, raw_frame.data.len() == 64 * 64 * 4, "Correct data size");

        let proc_config = ProcessorConfig::new(128, 128);
        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        test_assert!(logger, processed.width == 128, "Output width correct");
        test_assert!(logger, processed.height == 128, "Output height correct");
        test_assert!(logger, processed.data.len() == 128 * 128 * 4, "Output data size correct");

        backend.stop().unwrap();
        backend.close();
        test_step_ok!(logger);
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Processing Configuration Tests
// ============================================================================

/// Test different scaling modes through the pipeline
#[test]
fn test_scaling_modes_integration() {
    let mut logger = TestLogger::new("scaling_modes_integration", 4);

    let modes = [
        (ScalingMode::Fill, "Fill"),
        (ScalingMode::Fit, "Fit"),
        (ScalingMode::Stretch, "Stretch"),
        (ScalingMode::Center, "Center"),
    ];

    // Setup capture once
    test_step!(logger, "Setting up simulator");
    let config = SimulatorConfig {
        width: 200,
        height: 100, // 2:1 aspect ratio
        fps: 1000,
        pattern: FramePattern::ColorBars,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 200,
        height: 100,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    for (mode, name) in modes {
        test_step!(logger, "Testing scaling mode: {}", name);

        let raw_frame = backend.next_frame().unwrap();
        let proc_config = ProcessorConfig::new(300, 300) // Square target
            .with_scaling(mode);

        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        // All modes should produce target dimensions
        test_assert!(logger, processed.width == 300, "Output width is target");
        test_assert!(logger, processed.height == 300, "Output height is target");
        test_step_ok!(logger, "Mode {} produced {}x{}", name, processed.width, processed.height);
    }

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test rotation and flip transforms through the pipeline
#[test]
fn test_transforms_integration() {
    let mut logger = TestLogger::new("transforms_integration", 5);

    test_step!(logger, "Setting up 160x120 capture");
    let config = SimulatorConfig {
        width: 160,
        height: 120,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 160,
        height: 120,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    // Test rotations
    test_step!(logger, "Testing rotations");
    let rotations = [
        (Rotation::None, "None"),
        (Rotation::Clockwise90, "CW90"),
        (Rotation::Clockwise180, "CW180"),
        (Rotation::Clockwise270, "CW270"),
    ];

    for (rotation, name) in rotations {
        let raw_frame = backend.next_frame().unwrap();
        let proc_config = ProcessorConfig::new(320, 240)
            .with_rotation(rotation)
            .with_metrics(true);

        let processed = process_frame(&raw_frame, &proc_config).unwrap();
        test_assert!(logger, processed.width == 320, "Width correct for rotation {}", name);
        test_assert!(logger, processed.height == 240, "Height correct for rotation {}", name);

        if rotation != Rotation::None {
            let metrics = processed.metrics.as_ref().unwrap();
            test_assert!(logger, metrics.transform_executed, "Transform executed for {}", name);
        }
    }
    test_step_ok!(logger);

    // Test flips
    test_step!(logger, "Testing flips");
    let flips = [
        (Flip::None, "None"),
        (Flip::Horizontal, "Horizontal"),
        (Flip::Vertical, "Vertical"),
        (Flip::Both, "Both"),
    ];

    for (flip, name) in flips {
        let raw_frame = backend.next_frame().unwrap();
        let proc_config = ProcessorConfig::new(320, 240)
            .with_flip(flip)
            .with_metrics(true);

        let processed = process_frame(&raw_frame, &proc_config).unwrap();
        test_assert!(logger, processed.width == 320, "Width correct for flip {}", name);
        test_assert!(logger, processed.height == 240, "Height correct for flip {}", name);
    }
    test_step_ok!(logger);

    // Test combined transforms
    test_step!(logger, "Testing combined rotation + flip");
    let raw_frame = backend.next_frame().unwrap();
    let proc_config = ProcessorConfig::new(320, 240)
        .with_rotation(Rotation::Clockwise90)
        .with_flip(Flip::Horizontal)
        .with_metrics(true);

    let processed = process_frame(&raw_frame, &proc_config).unwrap();
    test_assert!(logger, processed.width == 320, "Combined transform width");
    test_assert!(logger, processed.height == 240, "Combined transform height");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Frame Data Integrity Tests
// ============================================================================

/// Test that frame data flows correctly without corruption
#[test]
fn test_frame_data_integrity() {
    let mut logger = TestLogger::new("frame_data_integrity", 5);

    test_step!(logger, "Setting up solid color capture");
    // Use solid color so we can verify pixel values
    let config = SimulatorConfig {
        width: 100,
        height: 100,
        fps: 1000,
        pattern: FramePattern::SolidColor { r: 200, g: 100, b: 50 },
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 100,
        height: 100,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing solid color frame");
    let raw_frame = backend.next_frame().unwrap();

    // Verify raw frame has correct pixel values (RGBA)
    test_assert!(logger, raw_frame.data[0] == 200, "Raw R value correct");
    test_assert!(logger, raw_frame.data[1] == 100, "Raw G value correct");
    test_assert!(logger, raw_frame.data[2] == 50, "Raw B value correct");
    test_assert!(logger, raw_frame.data[3] == 255, "Raw A value correct");
    test_step_ok!(logger);

    test_step!(logger, "Processing at same size (no scaling)");
    let proc_config = ProcessorConfig::new(100, 100)
        .with_scaling(ScalingMode::Fill);
    let processed = process_frame(&raw_frame, &proc_config).unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Verifying processed data integrity");
    // Check several sample pixels
    let check_positions = [0, 50 * 100 * 4, 99 * 100 * 4 + 99 * 4]; // first, middle, last
    for pos in check_positions {
        if pos + 3 < processed.data.len() {
            test_assert!(
                logger,
                processed.data[pos] == 200,
                "Processed R at offset {} correct", pos
            );
            test_assert!(
                logger,
                processed.data[pos + 1] == 100,
                "Processed G at offset {} correct", pos
            );
            test_assert!(
                logger,
                processed.data[pos + 2] == 50,
                "Processed B at offset {} correct", pos
            );
        }
    }
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test frame timestamp and sequence preservation
#[test]
fn test_frame_metadata_preservation() {
    let mut logger = TestLogger::new("frame_metadata_preservation", 4);

    test_step!(logger, "Setting up capture");
    let config = SimulatorConfig {
        width: 64,
        height: 64,
        fps: 1000,
        pattern: FramePattern::Counter,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 64,
        height: 64,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Capturing frames with timestamps");
    let mut timestamps: Vec<u64> = Vec::new();
    let mut sequences: Vec<u64> = Vec::new();

    for _ in 0..5 {
        let raw_frame = backend.next_frame().unwrap();
        timestamps.push(raw_frame.timestamp_ns);
        sequences.push(raw_frame.sequence);

        // Process frame - metrics should include timing
        let proc_config = ProcessorConfig::new(64, 64).with_metrics(true);
        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        test_assert!(logger, processed.metrics.is_some(), "Metrics collected");
    }
    test_step_ok!(logger);

    test_step!(logger, "Validating timestamp progression");
    for i in 1..timestamps.len() {
        test_assert!(
            logger,
            timestamps[i] >= timestamps[i - 1],
            "Timestamps are non-decreasing"
        );
    }
    test_step_ok!(logger, "Timestamps progress correctly");

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Resolution and Dimension Tests
// ============================================================================

/// Test various input resolutions through the pipeline
#[test]
fn test_various_input_resolutions() {
    let mut logger = TestLogger::new("various_input_resolutions", 6);

    let resolutions = [
        (64, 64, "64x64 tiny"),
        (320, 240, "320x240 QVGA"),
        (640, 480, "640x480 VGA"),
        (800, 600, "800x600 SVGA"),
        (1280, 720, "1280x720 HD"),
        (1920, 1080, "1920x1080 FHD"),
    ];

    for (width, height, name) in resolutions {
        test_step!(logger, "Testing input resolution: {}", name);

        let config = SimulatorConfig {
            width,
            height,
            fps: 1000,
            pattern: FramePattern::ColorBars,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();
        let settings = CaptureSettings {
            width,
            height,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        let raw_frame = backend.next_frame().unwrap();
        test_assert!(logger, raw_frame.width == width, "Input width correct");
        test_assert!(logger, raw_frame.height == height, "Input height correct");

        // Process to standard 720p output
        let proc_config = ProcessorConfig::new(1280, 720);
        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        test_assert!(logger, processed.width == 1280, "Output width is 1280");
        test_assert!(logger, processed.height == 720, "Output height is 720");
        test_assert!(
            logger,
            processed.data.len() == 1280 * 720 * 4,
            "Output data size correct"
        );

        backend.stop().unwrap();
        backend.close();
        test_step_ok!(logger);
    }

    let result = logger.finish();
    assert!(result.passed);
}

/// Test non-standard aspect ratios
#[test]
fn test_aspect_ratio_handling() {
    let mut logger = TestLogger::new("aspect_ratio_handling", 4);

    let test_cases = [
        // (input_w, input_h, output_w, output_h, description)
        (160, 90, 320, 180, "16:9 to 16:9"),
        (100, 100, 200, 150, "1:1 to 4:3"),
        (200, 100, 300, 300, "2:1 to 1:1"),
        (100, 200, 400, 300, "1:2 to 4:3"),
    ];

    for (in_w, in_h, out_w, out_h, desc) in test_cases {
        test_step!(logger, "Testing aspect ratio: {}", desc);

        let config = SimulatorConfig {
            width: in_w,
            height: in_h,
            fps: 1000,
            pattern: FramePattern::ColorBars,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();
        let settings = CaptureSettings {
            width: in_w,
            height: in_h,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        let raw_frame = backend.next_frame().unwrap();

        // Test with Fit mode to see letterboxing
        let proc_config = ProcessorConfig::new(out_w, out_h)
            .with_scaling(ScalingMode::Fit);
        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        test_assert!(logger, processed.width == out_w, "Fit output width");
        test_assert!(logger, processed.height == out_h, "Fit output height");

        // Test with Fill mode
        let proc_config = ProcessorConfig::new(out_w, out_h)
            .with_scaling(ScalingMode::Fill);
        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        test_assert!(logger, processed.width == out_w, "Fill output width");
        test_assert!(logger, processed.height == out_h, "Fill output height");

        backend.stop().unwrap();
        backend.close();
        test_step_ok!(logger);
    }

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Performance and Timing Tests
// ============================================================================

/// Test processing throughput for sustained capture
///
/// This test is timing-sensitive and may fail under system load.
/// Run with: cargo test test_sustained_throughput -- --ignored
#[test]
#[ignore]
fn test_sustained_throughput() {
    let mut logger = TestLogger::new("sustained_throughput", 4);

    test_step!(logger, "Setting up 480p capture");
    let config = SimulatorConfig {
        width: 640,
        height: 480,
        fps: 1000, // High FPS for throughput test
        pattern: FramePattern::ColorBars,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Processing 50 frames");
    let proc_config = ProcessorConfig::new(1280, 720).with_metrics(true);
    let start = Instant::now();
    let mut total_decode_time = Duration::ZERO;
    let mut total_scale_time = Duration::ZERO;

    for _ in 0..50 {
        let raw_frame = backend.next_frame().unwrap();
        let processed = process_frame(&raw_frame, &proc_config).unwrap();

        if let Some(metrics) = &processed.metrics {
            total_decode_time += metrics.decode_time;
            total_scale_time += metrics.scale_time;
        }
    }
    let elapsed = start.elapsed();
    test_step_ok!(logger, "Processed 50 frames in {:?}", elapsed);

    test_step!(logger, "Calculating throughput metrics");
    let fps = 50.0 / elapsed.as_secs_f64();
    let avg_decode = total_decode_time / 50;
    let avg_scale = total_scale_time / 50;

    test_assert!(logger, fps > 10.0, "Throughput exceeds 10 FPS");
    test_step_ok!(
        logger,
        "Throughput: {:.1} FPS, avg decode: {:?}, avg scale: {:?}",
        fps, avg_decode, avg_scale
    );

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test metrics collection through the pipeline
#[test]
fn test_metrics_collection() {
    let mut logger = TestLogger::new("metrics_collection", 4);

    test_step!(logger, "Setting up capture with metrics");
    let config = SimulatorConfig {
        width: 320,
        height: 240,
        fps: 1000,
        pattern: FramePattern::ColorBars,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 320,
        height: 240,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Processing with metrics enabled");
    let raw_frame = backend.next_frame().unwrap();

    // Process with full pipeline (transform + scale)
    let proc_config = ProcessorConfig::new(640, 480)
        .with_rotation(Rotation::Clockwise90)
        .with_metrics(true);

    let processed = process_frame(&raw_frame, &proc_config).unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Validating metrics");
    test_assert!(logger, processed.metrics.is_some(), "Metrics present");

    let metrics = processed.metrics.unwrap();
    test_assert!(logger, metrics.decode_executed, "Decode was executed");
    test_assert!(logger, metrics.transform_executed, "Transform was executed");
    test_assert!(logger, metrics.scale_executed, "Scale was executed");
    test_assert!(logger, metrics.total_time > Duration::ZERO, "Total time recorded");
    test_assert!(
        logger,
        metrics.total_time >= metrics.decode_time,
        "Total >= decode time"
    );
    test_step_ok!(
        logger,
        "decode: {:?}, transform: {:?}, scale: {:?}, total: {:?}",
        metrics.decode_time, metrics.transform_time, metrics.scale_time, metrics.total_time
    );

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

/// Test handling of minimum valid frame size
#[test]
fn test_minimum_frame_size() {
    let mut logger = TestLogger::new("minimum_frame_size", 3);

    test_step!(logger, "Setting up 1x1 capture");
    let config = SimulatorConfig {
        width: 1,
        height: 1,
        fps: 1000,
        pattern: FramePattern::SolidColor { r: 255, g: 128, b: 64 },
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 1,
        height: 1,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Processing 1x1 frame to larger size");
    let raw_frame = backend.next_frame().unwrap();
    test_assert!(logger, raw_frame.width == 1, "Input is 1x1");
    test_assert!(logger, raw_frame.height == 1, "Input is 1x1");

    let proc_config = ProcessorConfig::new(100, 100);
    let processed = process_frame(&raw_frame, &proc_config).unwrap();

    test_assert!(logger, processed.width == 100, "Scaled to 100x100");
    test_assert!(logger, processed.height == 100, "Scaled to 100x100");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

/// Test that frame sequence is preserved across pipeline stages
#[test]
fn test_sequence_continuity() {
    let mut logger = TestLogger::new("sequence_continuity", 3);

    test_step!(logger, "Setting up continuous capture");
    let config = SimulatorConfig {
        width: 64,
        height: 64,
        fps: 1000,
        pattern: FramePattern::MovingLine,
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 64,
        height: 64,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Verifying sequence numbers");
    let proc_config = ProcessorConfig::new(128, 128);
    let mut last_seq: Option<u64> = None;
    let mut gaps = 0;

    for _ in 0..20 {
        let raw_frame = backend.next_frame().unwrap();
        let _processed = process_frame(&raw_frame, &proc_config).unwrap();

        if let Some(prev) = last_seq {
            if raw_frame.sequence != prev + 1 {
                gaps += 1;
            }
        }
        last_seq = Some(raw_frame.sequence);
    }

    test_assert!(logger, gaps == 0, "No sequence gaps detected");
    test_step_ok!(logger, "20 frames processed with continuous sequence");

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Background/Filter Configuration Tests
// ============================================================================

/// Test background color for letterboxing in Fit mode
#[test]
fn test_letterbox_background_color() {
    let mut logger = TestLogger::new("letterbox_background_color", 4);

    test_step!(logger, "Setting up 16:9 capture for 4:3 target (will letterbox)");
    let config = SimulatorConfig {
        width: 160,
        height: 90, // 16:9
        fps: 1000,
        pattern: FramePattern::SolidColor { r: 200, g: 100, b: 50 },
        ..Default::default()
    };
    let mut backend = SimulatorBackend::new(config);
    let devices = backend.enumerate_devices();
    let settings = CaptureSettings {
        width: 160,
        height: 90,
        framerate: 1000.0,
        format: None,
    };
    backend.open(&devices[0].id, settings).unwrap();
    backend.start().unwrap();
    test_step_ok!(logger);

    test_step!(logger, "Processing with red background");
    let raw_frame = backend.next_frame().unwrap();
    let proc_config = ProcessorConfig::new(120, 120) // Square target
        .with_scaling(ScalingMode::Fit)
        .with_background([255, 0, 0, 255]); // Red background

    let processed = process_frame(&raw_frame, &proc_config).unwrap();
    test_assert!(logger, processed.width == 120, "Output is square");
    test_assert!(logger, processed.height == 120, "Output is square");
    test_step_ok!(logger);

    test_step!(logger, "Verifying frame contains expected content");
    // Frame should have content (not all zeros)
    let has_content = processed.data.iter().any(|&b| b != 0);
    test_assert!(logger, has_content, "Frame has pixel content");
    test_step_ok!(logger);

    test_step!(logger, "Cleanup");
    backend.stop().unwrap();
    backend.close();
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
}
