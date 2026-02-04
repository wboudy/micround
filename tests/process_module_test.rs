//! Unit tests for the process module
//!
//! Tests frame processing pipeline, transforms, scaling, and buffer management.
//!
//! Run with: cargo test --test process_module_test

use std::time::Duration;

use micround::core::{Flip, Frame, PixelFormat, Rotation, ScalingMode};
use micround::process::{
    process_frame, FrameBufferPool, FrameMetrics, PoolConfig, PoolStatsSnapshot, ProcessError,
    ProcessedFrame, ProcessorConfig, Region, ScaleConfig, ScaleFilter,
};

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a test frame with given format and dimensions
fn make_test_frame(width: u32, height: u32, format: PixelFormat) -> Frame {
    let bytes_per_pixel = match format {
        PixelFormat::Rgba32 => 4,
        PixelFormat::Rgb24 => 3,
        PixelFormat::Yuyv => 2,
        PixelFormat::Nv12 => 3, // Y + UV/2
        _ => 4,
    };

    let size = (width * height * bytes_per_pixel as u32) as usize;
    let mut data = vec![0u8; size];

    // Fill with recognizable pattern
    match format {
        PixelFormat::Rgba32 => {
            for i in 0..(width * height) as usize {
                let idx = i * 4;
                data[idx] = (i % 256) as u8; // R
                data[idx + 1] = (i / 256) as u8; // G
                data[idx + 2] = 128; // B
                data[idx + 3] = 255; // A
            }
        }
        PixelFormat::Rgb24 => {
            for i in 0..(width * height) as usize {
                let idx = i * 3;
                data[idx] = (i % 256) as u8; // R
                data[idx + 1] = (i / 256) as u8; // G
                data[idx + 2] = 128; // B
            }
        }
        PixelFormat::Yuyv => {
            for i in 0..((width * height) as usize / 2) {
                let idx = i * 4;
                data[idx] = 235; // Y0
                data[idx + 1] = 128; // U
                data[idx + 2] = 235; // Y1
                data[idx + 3] = 128; // V
            }
        }
        _ => {}
    }

    Frame {
        data,
        format,
        width,
        height,
        timestamp_ns: 1234567890,
        sequence: 42,
    }
}

// ============================================================================
// ProcessedFrame Tests
// ============================================================================

#[test]
fn test_processed_frame_new() {
    let data = vec![0u8; 100 * 100 * 4];
    let frame = ProcessedFrame::new(data.clone(), 100, 100);

    assert_eq!(frame.width, 100);
    assert_eq!(frame.height, 100);
    assert_eq!(frame.data.len(), 100 * 100 * 4);
    assert!(frame.metrics.is_none());
}

#[test]
fn test_processed_frame_with_metrics() {
    let data = vec![0u8; 50 * 50 * 4];
    let metrics = FrameMetrics {
        decode_time: Duration::from_millis(5),
        transform_time: Duration::from_millis(2),
        scale_time: Duration::from_millis(3),
        total_time: Duration::from_millis(10),
        decode_executed: true,
        transform_executed: true,
        scale_executed: true,
    };

    let frame = ProcessedFrame::with_metrics(data, 50, 50, metrics);

    assert_eq!(frame.width, 50);
    assert_eq!(frame.height, 50);
    assert!(frame.metrics.is_some());

    let m = frame.metrics.unwrap();
    assert_eq!(m.decode_time, Duration::from_millis(5));
    assert!(m.decode_executed);
}

// ============================================================================
// ProcessorConfig Tests
// ============================================================================

#[test]
fn test_processor_config_default() {
    let config = ProcessorConfig::default();

    assert_eq!(config.target_width, 1920);
    assert_eq!(config.target_height, 1080);
    assert_eq!(config.scaling, ScalingMode::Fill);
    assert_eq!(config.rotation, Rotation::None);
    assert_eq!(config.flip, Flip::None);
    assert_eq!(config.background, [0, 0, 0, 255]);
    assert!(!config.collect_metrics);
}

#[test]
fn test_processor_config_new() {
    let config = ProcessorConfig::new(1280, 720);

    assert_eq!(config.target_width, 1280);
    assert_eq!(config.target_height, 720);
    // Other fields should be default
    assert_eq!(config.scaling, ScalingMode::Fill);
}

#[test]
fn test_processor_config_builder() {
    let config = ProcessorConfig::new(1920, 1080)
        .with_scaling(ScalingMode::Fit)
        .with_rotation(Rotation::Clockwise90)
        .with_flip(Flip::Horizontal)
        .with_filter(ScaleFilter::Lanczos)
        .with_background([255, 0, 0, 255])
        .with_metrics(true);

    assert_eq!(config.scaling, ScalingMode::Fit);
    assert_eq!(config.rotation, Rotation::Clockwise90);
    assert_eq!(config.flip, Flip::Horizontal);
    assert!(matches!(config.filter, ScaleFilter::Lanczos));
    assert_eq!(config.background, [255, 0, 0, 255]);
    assert!(config.collect_metrics);
}

#[test]
fn test_processor_config_all_scaling_modes() {
    for mode in [
        ScalingMode::Fit,
        ScalingMode::Fill,
        ScalingMode::Stretch,
        ScalingMode::Center,
    ] {
        let config = ProcessorConfig::new(100, 100).with_scaling(mode);
        assert_eq!(config.scaling, mode);
    }
}

#[test]
fn test_processor_config_all_rotations() {
    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        let config = ProcessorConfig::new(100, 100).with_rotation(rotation);
        assert_eq!(config.rotation, rotation);
    }
}

#[test]
fn test_processor_config_all_flips() {
    for flip in [Flip::None, Flip::Horizontal, Flip::Vertical, Flip::Both] {
        let config = ProcessorConfig::new(100, 100).with_flip(flip);
        assert_eq!(config.flip, flip);
    }
}

// ============================================================================
// process_frame Tests
// ============================================================================

#[test]
fn test_process_frame_basic_rgba() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(200, 150);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    assert_eq!(processed.width, 200);
    assert_eq!(processed.height, 150);
    assert_eq!(processed.data.len(), 200 * 150 * 4);
}

#[test]
fn test_process_frame_basic_rgb24() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgb24);
    let config = ProcessorConfig::new(100, 100);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    assert_eq!(processed.data.len(), 100 * 100 * 4); // Output is always RGBA
}

#[test]
fn test_process_frame_with_metrics() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(200, 150).with_metrics(true);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    assert!(processed.metrics.is_some());

    let metrics = processed.metrics.unwrap();
    assert!(metrics.decode_executed);
    assert!(metrics.total_time > Duration::ZERO);
}

#[test]
fn test_process_frame_with_rotation() {
    let frame = make_test_frame(100, 80, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(200, 150)
        .with_rotation(Rotation::Clockwise90)
        .with_metrics(true);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    let metrics = processed.metrics.unwrap();
    assert!(metrics.transform_executed);
}

#[test]
fn test_process_frame_with_flip() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(100, 100)
        .with_flip(Flip::Horizontal)
        .with_metrics(true);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let metrics = result.unwrap().metrics.unwrap();
    assert!(metrics.transform_executed);
}

#[test]
fn test_process_frame_no_transform_optimization() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(200, 150)
        .with_rotation(Rotation::None)
        .with_flip(Flip::None)
        .with_metrics(true);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let metrics = result.unwrap().metrics.unwrap();
    assert!(!metrics.transform_executed); // Should be skipped
}

#[test]
fn test_process_frame_no_scale_optimization() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(100, 100)
        .with_scaling(ScalingMode::Fill)
        .with_metrics(true);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let metrics = result.unwrap().metrics.unwrap();
    assert!(!metrics.scale_executed); // Should be skipped
}

#[test]
fn test_process_frame_invalid_frame_width() {
    let frame = Frame {
        data: vec![],
        format: PixelFormat::Rgba32,
        width: 0,
        height: 100,
        timestamp_ns: 0,
        sequence: 0,
    };
    let config = ProcessorConfig::new(100, 100);

    let result = process_frame(&frame, &config);
    assert!(matches!(result, Err(ProcessError::InvalidFrame { .. })));
}

#[test]
fn test_process_frame_invalid_frame_height() {
    let frame = Frame {
        data: vec![],
        format: PixelFormat::Rgba32,
        width: 100,
        height: 0,
        timestamp_ns: 0,
        sequence: 0,
    };
    let config = ProcessorConfig::new(100, 100);

    let result = process_frame(&frame, &config);
    assert!(matches!(result, Err(ProcessError::InvalidFrame { .. })));
}

#[test]
fn test_process_frame_invalid_config_width() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(0, 100);

    let result = process_frame(&frame, &config);
    assert!(matches!(result, Err(ProcessError::InvalidConfig { .. })));
}

#[test]
fn test_process_frame_invalid_config_height() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(100, 0);

    let result = process_frame(&frame, &config);
    assert!(matches!(result, Err(ProcessError::InvalidConfig { .. })));
}

#[test]
fn test_process_frame_all_scaling_modes() {
    let frame = make_test_frame(160, 90, PixelFormat::Rgba32);

    for mode in [
        ScalingMode::Fit,
        ScalingMode::Fill,
        ScalingMode::Stretch,
        ScalingMode::Center,
    ] {
        let config = ProcessorConfig::new(320, 240).with_scaling(mode);
        let result = process_frame(&frame, &config);
        assert!(result.is_ok(), "Failed for mode {:?}", mode);

        let processed = result.unwrap();
        assert_eq!(processed.width, 320);
        assert_eq!(processed.height, 240);
    }
}

#[test]
fn test_process_frame_combined_transforms() {
    let frame = make_test_frame(100, 80, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(200, 150)
        .with_rotation(Rotation::Clockwise180)
        .with_flip(Flip::Both);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());
}

// ============================================================================
// ScaleFilter Tests
// ============================================================================

#[test]
fn test_scale_filter_default() {
    let filter: ScaleFilter = Default::default();
    assert!(matches!(filter, ScaleFilter::Bilinear));
}

#[test]
fn test_scale_filter_variants() {
    let _nearest = ScaleFilter::Nearest;
    let _bilinear = ScaleFilter::Bilinear;
    let _lanczos = ScaleFilter::Lanczos;
}

#[test]
fn test_scale_filter_debug() {
    assert_eq!(format!("{:?}", ScaleFilter::Nearest), "Nearest");
    assert_eq!(format!("{:?}", ScaleFilter::Bilinear), "Bilinear");
    assert_eq!(format!("{:?}", ScaleFilter::Lanczos), "Lanczos");
}

// ============================================================================
// ScaleConfig Tests
// ============================================================================

#[test]
fn test_scale_config_default() {
    let config = ScaleConfig::default();

    assert_eq!(config.mode, ScalingMode::Fill);
    assert!(matches!(config.filter, ScaleFilter::Bilinear));
    assert_eq!(config.background, [0, 0, 0, 255]);
}

// ============================================================================
// Region Tests
// ============================================================================

#[test]
fn test_region_new() {
    let region = Region::new(10, 20, 100, 50);

    assert_eq!(region.x, 10);
    assert_eq!(region.y, 20);
    assert_eq!(region.width, 100);
    assert_eq!(region.height, 50);
}

#[test]
fn test_region_full() {
    let region = Region::full(1920, 1080);

    assert_eq!(region.x, 0);
    assert_eq!(region.y, 0);
    assert_eq!(region.width, 1920);
    assert_eq!(region.height, 1080);
}

#[test]
fn test_region_debug() {
    let region = Region::new(0, 0, 100, 100);
    let debug_str = format!("{:?}", region);
    assert!(debug_str.contains("Region"));
}

#[test]
fn test_region_copy() {
    let region1 = Region::new(5, 10, 20, 30);
    let region2 = region1; // Copy
    assert_eq!(region2.x, 5);
    assert_eq!(region2.y, 10);
}

// ============================================================================
// PoolConfig Tests
// ============================================================================

#[test]
fn test_pool_config_default() {
    let config = PoolConfig::default();

    assert_eq!(config.capacity, 4);
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert_eq!(config.bytes_per_pixel, 4);
}

#[test]
fn test_pool_config_for_resolution() {
    let config = PoolConfig::for_resolution(1280, 720);

    assert_eq!(config.width, 1280);
    assert_eq!(config.height, 720);
    assert_eq!(config.capacity, 4); // Default capacity
}

#[test]
fn test_pool_config_buffer_size() {
    let config = PoolConfig::for_resolution(100, 100);
    assert_eq!(config.buffer_size(), 100 * 100 * 4);
}

#[test]
fn test_pool_config_total_memory() {
    let config = PoolConfig {
        capacity: 4,
        width: 100,
        height: 100,
        bytes_per_pixel: 4,
        ..Default::default()
    };
    assert_eq!(config.total_memory(), 100 * 100 * 4 * 4);
}

// ============================================================================
// PoolStatsSnapshot Tests
// ============================================================================

#[test]
fn test_pool_stats_snapshot_failure_rate() {
    let snapshot = PoolStatsSnapshot {
        acquisitions: 90,
        failures: 10,
        in_use: 0,
        peak_in_use: 2,
    };

    let rate = snapshot.failure_rate();
    assert!((rate - 10.0).abs() < 0.01);
}

#[test]
fn test_pool_stats_snapshot_failure_rate_zero() {
    let snapshot = PoolStatsSnapshot {
        acquisitions: 0,
        failures: 0,
        in_use: 0,
        peak_in_use: 0,
    };

    assert_eq!(snapshot.failure_rate(), 0.0);
}

#[test]
fn test_pool_stats_snapshot_failure_rate_all_failures() {
    let snapshot = PoolStatsSnapshot {
        acquisitions: 0,
        failures: 100,
        in_use: 0,
        peak_in_use: 0,
    };

    assert_eq!(snapshot.failure_rate(), 100.0);
}

// ============================================================================
// FrameBufferPool Tests
// ============================================================================

#[test]
fn test_frame_buffer_pool_creation() {
    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 3,
        width: 64,
        height: 64,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    assert_eq!(pool.available(), 3);
    assert_eq!(pool.config().capacity, 3);
}

#[test]
fn test_frame_buffer_pool_for_resolution() {
    let pool = FrameBufferPool::for_resolution(640, 480);
    assert_eq!(pool.config().width, 640);
    assert_eq!(pool.config().height, 480);
}

#[test]
fn test_frame_buffer_pool_acquire_release() {
    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 2,
        width: 4,
        height: 4,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    assert_eq!(pool.available(), 2);

    let buf1 = pool.try_acquire();
    assert!(buf1.is_some());
    assert_eq!(pool.available(), 1);

    let buf2 = pool.try_acquire();
    assert!(buf2.is_some());
    assert_eq!(pool.available(), 0);

    // Pool exhausted
    let buf3 = pool.try_acquire();
    assert!(buf3.is_none());

    // Release one
    drop(buf1);
    assert_eq!(pool.available(), 1);

    // Can acquire again
    let buf4 = pool.try_acquire();
    assert!(buf4.is_some());
}

#[test]
fn test_frame_buffer_pool_stats() {
    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 1,
        width: 4,
        height: 4,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    let stats = pool.stats().snapshot();
    assert_eq!(stats.acquisitions, 0);
    assert_eq!(stats.failures, 0);

    let buf = pool.try_acquire();
    assert!(buf.is_some());

    let stats = pool.stats().snapshot();
    assert_eq!(stats.acquisitions, 1);
    assert_eq!(stats.in_use, 1);

    // Try acquire when exhausted
    let buf2 = pool.try_acquire();
    assert!(buf2.is_none());

    let stats = pool.stats().snapshot();
    assert_eq!(stats.failures, 1);
}

// ============================================================================
// FrameBuffer Tests
// ============================================================================

#[test]
fn test_frame_buffer_dimensions() {
    let pool = FrameBufferPool::for_resolution(320, 240);
    let buf = pool.try_acquire().unwrap();

    assert_eq!(buf.dimensions(), (320, 240));
}

#[test]
fn test_frame_buffer_size() {
    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 1,
        width: 10,
        height: 10,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    let buf = pool.try_acquire().unwrap();
    assert_eq!(buf.size(), 10 * 10 * 4);
}

#[test]
fn test_frame_buffer_data_access() {
    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 1,
        width: 2,
        height: 2,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    let mut buf = pool.try_acquire().unwrap();

    // Write
    let data = buf.data_mut();
    data[0] = 255;
    data[1] = 128;

    // Read back
    assert_eq!(buf.data()[0], 255);
    assert_eq!(buf.data()[1], 128);
}

#[test]
fn test_frame_buffer_copy_from() {
    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 1,
        width: 2,
        height: 2,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    let mut buf = pool.try_acquire().unwrap();
    let src = vec![1, 2, 3, 4, 5, 6, 7, 8];
    buf.copy_from(&src);

    assert_eq!(&buf.data()[..8], &src[..]);
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

#[test]
fn test_frame_buffer_pool_concurrent() {
    use std::sync::Arc;
    use std::thread;

    let pool = FrameBufferPool::new(PoolConfig {
        capacity: 4,
        width: 4,
        height: 4,
        bytes_per_pixel: 4,
        ..Default::default()
    });

    let handles: Vec<_> = (0..8)
        .map(|i| {
            let pool = Arc::clone(&pool);
            thread::spawn(move || {
                for _ in 0..10 {
                    if let Some(mut buf) = pool.try_acquire() {
                        buf.data_mut()[0] = i as u8;
                        thread::sleep(Duration::from_micros(50));
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // All buffers should be released
    assert_eq!(pool.available(), 4);
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_process_frame_1x1() {
    let frame = make_test_frame(1, 1, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(1, 1);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    assert_eq!(processed.width, 1);
    assert_eq!(processed.height, 1);
}

#[test]
fn test_process_frame_large_upscale() {
    let frame = make_test_frame(10, 10, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(1000, 1000);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    assert_eq!(processed.width, 1000);
    assert_eq!(processed.height, 1000);
}

#[test]
fn test_process_frame_large_downscale() {
    let frame = make_test_frame(1000, 1000, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(10, 10);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());

    let processed = result.unwrap();
    assert_eq!(processed.width, 10);
    assert_eq!(processed.height, 10);
}

#[test]
fn test_process_frame_non_square() {
    let frame = make_test_frame(16, 9, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(1920, 1080);

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());
}

#[test]
fn test_process_frame_all_rotations_and_scales() {
    let frame = make_test_frame(100, 80, PixelFormat::Rgba32);

    for rotation in [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ] {
        let config = ProcessorConfig::new(200, 150).with_rotation(rotation);
        let result = process_frame(&frame, &config);
        assert!(result.is_ok(), "Failed for rotation {:?}", rotation);
    }
}

#[test]
fn test_process_frame_all_filters() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);

    for filter in [
        ScaleFilter::Nearest,
        ScaleFilter::Bilinear,
        ScaleFilter::Lanczos,
    ] {
        let config = ProcessorConfig::new(200, 150).with_filter(filter);
        let result = process_frame(&frame, &config);
        assert!(result.is_ok(), "Failed for filter {:?}", filter);
    }
}

#[test]
fn test_process_frame_custom_background() {
    let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
    let config = ProcessorConfig::new(200, 150)
        .with_scaling(ScalingMode::Fit)
        .with_background([255, 0, 0, 255]); // Red background

    let result = process_frame(&frame, &config);
    assert!(result.is_ok());
}

// ============================================================================
// FrameMetrics Tests
// ============================================================================

#[test]
fn test_frame_metrics_clone() {
    let metrics = FrameMetrics {
        decode_time: Duration::from_millis(5),
        transform_time: Duration::from_millis(2),
        scale_time: Duration::from_millis(3),
        total_time: Duration::from_millis(10),
        decode_executed: true,
        transform_executed: true,
        scale_executed: false,
    };

    let cloned = metrics.clone();
    assert_eq!(cloned.decode_time, Duration::from_millis(5));
    assert_eq!(cloned.transform_time, Duration::from_millis(2));
    assert_eq!(cloned.scale_time, Duration::from_millis(3));
    assert!(cloned.decode_executed);
    assert!(cloned.transform_executed);
    assert!(!cloned.scale_executed);
}

#[test]
fn test_frame_metrics_debug() {
    let metrics = FrameMetrics {
        decode_time: Duration::ZERO,
        transform_time: Duration::ZERO,
        scale_time: Duration::ZERO,
        total_time: Duration::ZERO,
        decode_executed: false,
        transform_executed: false,
        scale_executed: false,
    };

    let debug_str = format!("{:?}", metrics);
    assert!(debug_str.contains("FrameMetrics"));
}
