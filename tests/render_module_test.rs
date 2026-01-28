//! Render Module Unit Tests
//!
//! Tests for the render module's WallpaperRenderer trait, DisplaySimulator,
//! and related types. Uses the simulator for headless testing.

use std::thread;
use std::time::Duration;

// Test-only common utilities
#[path = "common/mod.rs"]
mod common;

use common::assertions::{assert_completes_within, timed};

// ============================================================================
// Test Imports
// ============================================================================

use micround::core::{DisplayId, RenderError};
use micround::process::ProcessedFrame;
use micround::render::simulator::{
    CapturedFrame, DisplaySimulator, DisplaySimulatorConfig, RenderStats,
};
use micround::render::WallpaperRenderer;

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a solid color test frame
fn solid_frame(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> ProcessedFrame {
    let size = (width * height * 4) as usize;
    let mut data = Vec::with_capacity(size);
    for _ in 0..(width * height) {
        data.push(r);
        data.push(g);
        data.push(b);
        data.push(a);
    }
    ProcessedFrame::new(data, width, height)
}

/// Create a gradient test frame
fn gradient_frame(width: u32, height: u32) -> ProcessedFrame {
    let size = (width * height * 4) as usize;
    let mut data = Vec::with_capacity(size);
    for y in 0..height {
        for x in 0..width {
            let r = ((x as f32 / width as f32) * 255.0) as u8;
            let g = ((y as f32 / height as f32) * 255.0) as u8;
            let b = 128;
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(255);
        }
    }
    ProcessedFrame::new(data, width, height)
}

/// Create a checkerboard pattern frame
fn checkerboard_frame(width: u32, height: u32, tile_size: u32) -> ProcessedFrame {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let is_white = ((x / tile_size) + (y / tile_size)) % 2 == 0;
            let color = if is_white { 255 } else { 0 };
            data.push(color);
            data.push(color);
            data.push(color);
            data.push(255);
        }
    }
    ProcessedFrame::new(data, width, height)
}

/// Create a frame with specific corner colors
fn corners_frame(width: u32, height: u32) -> ProcessedFrame {
    let mut data = vec![128u8; (width * height * 4) as usize];

    // Top-left: Red
    data[0] = 255; data[1] = 0; data[2] = 0; data[3] = 255;

    // Top-right: Green
    let tr = ((width - 1) * 4) as usize;
    data[tr] = 0; data[tr + 1] = 255; data[tr + 2] = 0; data[tr + 3] = 255;

    // Bottom-left: Blue
    let bl = ((height - 1) * width * 4) as usize;
    data[bl] = 0; data[bl + 1] = 0; data[bl + 2] = 255; data[bl + 3] = 255;

    // Bottom-right: White
    let br = (((height - 1) * width + (width - 1)) * 4) as usize;
    data[br] = 255; data[br + 1] = 255; data[br + 2] = 255; data[br + 3] = 255;

    ProcessedFrame::new(data, width, height)
}

// ============================================================================
// DisplayId Tests
// ============================================================================

#[test]
fn test_display_id_creation() {
    let id = DisplayId("primary".to_string());
    assert_eq!(id.0, "primary");
}

#[test]
fn test_display_id_clone() {
    let id1 = DisplayId("display-1".to_string());
    let id2 = id1.clone();
    assert_eq!(id1.0, id2.0);
}

#[test]
fn test_display_id_debug() {
    let id = DisplayId("test-display".to_string());
    let debug = format!("{:?}", id);
    assert!(debug.contains("test-display"));
}

// ============================================================================
// DisplaySimulatorConfig Tests
// ============================================================================

#[test]
fn test_config_default() {
    let config = DisplaySimulatorConfig::default();
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert_eq!(config.frame_history_size, 10);
    assert_eq!(config.latency_ms, 0);
    assert_eq!(config.error_rate, 0.0);
    assert!(!config.strict_dimensions);
}

#[test]
fn test_config_hd_preset() {
    let config = DisplaySimulatorConfig::hd();
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert!(config.display_name.contains("HD"));
}

#[test]
fn test_config_uhd_preset() {
    let config = DisplaySimulatorConfig::uhd();
    assert_eq!(config.width, 3840);
    assert_eq!(config.height, 2160);
    assert!(config.display_name.contains("4K"));
}

#[test]
fn test_config_strict_preset() {
    let config = DisplaySimulatorConfig::strict();
    assert!(config.strict_dimensions);
    assert!(config.display_name.contains("Strict"));
}

#[test]
fn test_config_slow_preset() {
    let config = DisplaySimulatorConfig::slow();
    assert_eq!(config.latency_ms, 50);
    assert!(config.display_name.contains("Slow"));
}

#[test]
fn test_config_unreliable_preset() {
    let config = DisplaySimulatorConfig::unreliable();
    assert!(config.error_rate > 0.0);
    assert!(config.display_name.contains("Unreliable"));
}

#[test]
fn test_config_custom() {
    let config = DisplaySimulatorConfig {
        display_name: "Custom Display".to_string(),
        width: 800,
        height: 600,
        frame_history_size: 5,
        latency_ms: 10,
        error_rate: 0.05,
        strict_dimensions: true,
    };

    assert_eq!(config.display_name, "Custom Display");
    assert_eq!(config.width, 800);
    assert_eq!(config.height, 600);
    assert_eq!(config.frame_history_size, 5);
    assert_eq!(config.latency_ms, 10);
    assert_eq!(config.error_rate, 0.05);
    assert!(config.strict_dimensions);
}

#[test]
fn test_config_clone() {
    let config1 = DisplaySimulatorConfig::hd();
    let config2 = config1.clone();
    assert_eq!(config1.width, config2.width);
    assert_eq!(config1.height, config2.height);
}

#[test]
fn test_config_debug() {
    let config = DisplaySimulatorConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("DisplaySimulatorConfig"));
    assert!(debug.contains("1920"));
}

// ============================================================================
// RenderStats Tests
// ============================================================================

#[test]
fn test_render_stats_default() {
    let stats = RenderStats::default();
    assert_eq!(stats.frames_rendered, 0);
    assert_eq!(stats.errors, 0);
    assert_eq!(stats.avg_render_time_us, 0);
    assert_eq!(stats.min_render_time_us, 0);
    assert_eq!(stats.max_render_time_us, 0);
    assert!(stats.last_render_time.is_none());
}

#[test]
fn test_render_stats_clone() {
    let mut stats = RenderStats::default();
    stats.frames_rendered = 100;
    stats.errors = 5;

    let cloned = stats.clone();
    assert_eq!(cloned.frames_rendered, 100);
    assert_eq!(cloned.errors, 5);
}

#[test]
fn test_render_stats_debug() {
    let stats = RenderStats::default();
    let debug = format!("{:?}", stats);
    assert!(debug.contains("RenderStats"));
}

// ============================================================================
// CapturedFrame Tests
// ============================================================================

#[test]
fn test_captured_frame_pixel_at_valid() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 255, 128, 64, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    let pixel = captured.pixel_at(5, 5);
    assert_eq!(pixel, Some((255, 128, 64, 255)));
}

#[test]
fn test_captured_frame_pixel_at_bounds() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();

    // Valid bounds
    assert!(captured.pixel_at(0, 0).is_some());
    assert!(captured.pixel_at(9, 9).is_some());

    // Out of bounds
    assert!(captured.pixel_at(10, 0).is_none());
    assert!(captured.pixel_at(0, 10).is_none());
    assert!(captured.pixel_at(10, 10).is_none());
    assert!(captured.pixel_at(100, 100).is_none());
}

#[test]
fn test_captured_frame_is_solid_color_true() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(50, 50, 100, 150, 200, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.is_solid_color(), Some((100, 150, 200, 255)));
}

#[test]
fn test_captured_frame_is_solid_color_false() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = gradient_frame(50, 50);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert!(captured.is_solid_color().is_none());
}

#[test]
fn test_captured_frame_average_brightness_black() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.average_brightness(), 0);
}

#[test]
fn test_captured_frame_average_brightness_white() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 255, 255, 255, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.average_brightness(), 255);
}

#[test]
fn test_captured_frame_average_brightness_gray() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 128, 128, 128, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.average_brightness(), 128);
}

#[test]
fn test_captured_frame_average_brightness_colored() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    // R=255, G=0, B=0 -> avg = (255+0+0)/3 = 85
    let frame = solid_frame(10, 10, 255, 0, 0, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.average_brightness(), 85);
}

// ============================================================================
// DisplaySimulator Lifecycle Tests
// ============================================================================

#[test]
fn test_simulator_creation_default() {
    let sim = DisplaySimulator::default_config();
    // Not initialized yet - frame_count should be 0
    assert_eq!(sim.frame_count(), 0);
}

#[test]
fn test_simulator_creation_with_config() {
    let config = DisplaySimulatorConfig {
        width: 800,
        height: 600,
        ..Default::default()
    };
    let sim = DisplaySimulator::new(config);
    // Not initialized yet - frame_count should be 0
    assert_eq!(sim.frame_count(), 0);
}

#[test]
fn test_simulator_init() {
    let mut sim = DisplaySimulator::default_config();
    let display = DisplayId("test".to_string());

    let result = sim.init(&display);
    assert!(result.is_ok());
    // Verify initialized by trying to render
    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_ok());
}

#[test]
fn test_simulator_double_init_fails() {
    let mut sim = DisplaySimulator::default_config();
    let display = DisplayId("test".to_string());

    assert!(sim.init(&display).is_ok());
    assert!(sim.init(&display).is_err());
}

#[test]
fn test_simulator_shutdown() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    sim.shutdown();
    // Verify shutdown by trying to render (should fail)
    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_err());
}

#[test]
fn test_simulator_shutdown_without_init() {
    let mut sim = DisplaySimulator::default_config();
    // Should not panic
    sim.shutdown();
    // Verify not initialized by trying to render (should fail)
    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_err());
}

#[test]
fn test_simulator_render_after_shutdown_fails() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();
    sim.shutdown();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_err());
}

#[test]
fn test_simulator_reinit_after_shutdown() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();
    sim.shutdown();

    // Should be able to init again
    assert!(sim.init(&DisplayId("test2".into())).is_ok());
    // Verify initialized by trying to render
    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_ok());
}

// ============================================================================
// Render Operation Tests
// ============================================================================

#[test]
fn test_render_without_init_fails() {
    let mut sim = DisplaySimulator::default_config();
    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    let result = sim.render(&frame);
    assert!(result.is_err());
}

#[test]
fn test_render_basic() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(100, 100, 255, 128, 64, 255);
    let result = sim.render(&frame);

    assert!(result.is_ok());
    assert_eq!(sim.frame_count(), 1);
}

#[test]
fn test_render_multiple_frames() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    for i in 0..10 {
        let frame = solid_frame(50, 50, i * 25, 0, 0, 255);
        sim.render(&frame).unwrap();
    }

    assert_eq!(sim.frame_count(), 10);
}

#[test]
fn test_render_different_sizes() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    // Small frame
    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_ok());

    // Large frame
    let frame = solid_frame(1920, 1080, 255, 255, 255, 255);
    assert!(sim.render(&frame).is_ok());

    // Non-square frame
    let frame = solid_frame(100, 50, 128, 128, 128, 255);
    assert!(sim.render(&frame).is_ok());
}

#[test]
fn test_render_captures_data() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = corners_frame(10, 10);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();

    // Check corners
    assert_eq!(captured.pixel_at(0, 0), Some((255, 0, 0, 255))); // Red
    assert_eq!(captured.pixel_at(9, 0), Some((0, 255, 0, 255))); // Green
    assert_eq!(captured.pixel_at(0, 9), Some((0, 0, 255, 255))); // Blue
    assert_eq!(captured.pixel_at(9, 9), Some((255, 255, 255, 255))); // White
}

// ============================================================================
// Frame History Tests
// ============================================================================

#[test]
fn test_frame_history_size_limit() {
    let config = DisplaySimulatorConfig {
        frame_history_size: 5,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    // Render 10 frames
    for i in 0..10 {
        let frame = solid_frame(10, 10, i * 25, 0, 0, 255);
        sim.render(&frame).unwrap();
    }

    // Should only have last 5
    let history = sim.frame_history();
    assert_eq!(history.len(), 5);
}

#[test]
fn test_frame_history_order() {
    let config = DisplaySimulatorConfig {
        frame_history_size: 5,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    // Render frames with distinct colors
    for i in 0u8..5 {
        let frame = solid_frame(10, 10, i * 50, 0, 0, 255);
        sim.render(&frame).unwrap();
    }

    let history = sim.frame_history();

    // Oldest first
    assert_eq!(history[0].pixel_at(0, 0).unwrap().0, 0);
    assert_eq!(history[4].pixel_at(0, 0).unwrap().0, 200);
}

#[test]
fn test_last_frame() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    // No frames yet
    assert!(sim.last_frame().is_none());

    // After render
    let frame = solid_frame(10, 10, 123, 45, 67, 255);
    sim.render(&frame).unwrap();

    let last = sim.last_frame().unwrap();
    assert_eq!(last.pixel_at(0, 0), Some((123, 45, 67, 255)));
}

// ============================================================================
// Statistics Tests
// ============================================================================

#[test]
fn test_stats_after_render() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(100, 100, 0, 0, 0, 255);
    for _ in 0..5 {
        sim.render(&frame).unwrap();
    }

    let stats = sim.stats();
    assert_eq!(stats.frames_rendered, 5);
    assert_eq!(stats.errors, 0);
    assert!(stats.last_render_time.is_some());
}

#[test]
fn test_stats_min_max_avg() {
    let config = DisplaySimulatorConfig {
        latency_ms: 5, // Add small latency for measurable times
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    for _ in 0..3 {
        sim.render(&frame).unwrap();
    }

    let stats = sim.stats();
    assert!(stats.min_render_time_us > 0);
    assert!(stats.max_render_time_us >= stats.min_render_time_us);
    assert!(stats.avg_render_time_us >= stats.min_render_time_us);
    assert!(stats.avg_render_time_us <= stats.max_render_time_us);
}

// ============================================================================
// Reset Tests
// ============================================================================

#[test]
fn test_reset_clears_history() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    for _ in 0..5 {
        sim.render(&frame).unwrap();
    }

    assert!(!sim.frame_history().is_empty());

    sim.reset();

    assert!(sim.frame_history().is_empty());
}

#[test]
fn test_reset_clears_stats() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    for _ in 0..5 {
        sim.render(&frame).unwrap();
    }

    sim.reset();

    let stats = sim.stats();
    assert_eq!(stats.frames_rendered, 0);
}

#[test]
fn test_reset_clears_frame_count() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    for _ in 0..5 {
        sim.render(&frame).unwrap();
    }

    assert_eq!(sim.frame_count(), 5);

    sim.reset();

    assert_eq!(sim.frame_count(), 0);
}

// ============================================================================
// Strict Dimensions Tests
// ============================================================================

#[test]
fn test_strict_dimensions_matching() {
    let config = DisplaySimulatorConfig {
        width: 100,
        height: 100,
        strict_dimensions: true,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(100, 100, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_ok());
}

#[test]
fn test_strict_dimensions_wrong_width() {
    let config = DisplaySimulatorConfig {
        width: 100,
        height: 100,
        strict_dimensions: true,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(50, 100, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_err());
}

#[test]
fn test_strict_dimensions_wrong_height() {
    let config = DisplaySimulatorConfig {
        width: 100,
        height: 100,
        strict_dimensions: true,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(100, 50, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_err());
}

#[test]
fn test_non_strict_accepts_any_size() {
    let config = DisplaySimulatorConfig {
        width: 1920,
        height: 1080,
        strict_dimensions: false,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    // Different sizes should all work
    assert!(sim.render(&solid_frame(100, 100, 0, 0, 0, 255)).is_ok());
    assert!(sim.render(&solid_frame(50, 200, 0, 0, 0, 255)).is_ok());
    assert!(sim.render(&solid_frame(1, 1, 0, 0, 0, 255)).is_ok());
}

// ============================================================================
// Error Injection Tests
// ============================================================================

#[test]
fn test_error_injection_zero_rate() {
    let config = DisplaySimulatorConfig {
        error_rate: 0.0,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    // All should succeed
    for _ in 0..100 {
        assert!(sim.render(&frame).is_ok());
    }
}

#[test]
fn test_error_injection_full_rate() {
    let config = DisplaySimulatorConfig {
        error_rate: 1.0,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    // All should fail
    for _ in 0..10 {
        assert!(sim.render(&frame).is_err());
    }
}

#[test]
fn test_error_injection_deterministic_with_seed() {
    let config = DisplaySimulatorConfig {
        error_rate: 0.5,
        ..Default::default()
    };

    // Two simulators with same seed should have same error pattern
    let mut sim1 = DisplaySimulator::new(config.clone()).with_seed(12345);
    let mut sim2 = DisplaySimulator::new(config).with_seed(12345);

    sim1.init(&DisplayId("test".into())).unwrap();
    sim2.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    for _ in 0..20 {
        let r1 = sim1.render(&frame);
        let r2 = sim2.render(&frame);
        assert_eq!(r1.is_ok(), r2.is_ok());
    }
}

#[test]
fn test_error_injection_different_seeds() {
    let config = DisplaySimulatorConfig {
        error_rate: 0.5,
        ..Default::default()
    };

    // Different seeds should have different patterns
    let mut sim1 = DisplaySimulator::new(config.clone()).with_seed(1);
    let mut sim2 = DisplaySimulator::new(config).with_seed(999999);

    sim1.init(&DisplayId("test".into())).unwrap();
    sim2.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    let mut differences = 0;
    for _ in 0..100 {
        let r1 = sim1.render(&frame);
        let r2 = sim2.render(&frame);
        if r1.is_ok() != r2.is_ok() {
            differences += 1;
        }
    }

    // Should have at least some differences
    assert!(differences > 10, "Expected different error patterns");
}

#[test]
fn test_error_stats_tracked() {
    let config = DisplaySimulatorConfig {
        error_rate: 0.5,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config).with_seed(42);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    for _ in 0..100 {
        let _ = sim.render(&frame);
    }

    let stats = sim.stats();
    assert!(stats.errors > 0, "Should have some errors");
    assert!(stats.frames_rendered > 0, "Should have some successes");
    assert_eq!(stats.errors + stats.frames_rendered, 100);
}

// ============================================================================
// Latency Simulation Tests
// ============================================================================

#[test]
fn test_latency_simulation() {
    let config = DisplaySimulatorConfig {
        latency_ms: 20,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);

    let (_, duration) = timed("render_with_latency", || {
        sim.render(&frame).unwrap();
    });

    // Should take at least the configured latency
    assert!(duration.as_millis() >= 20, "Expected at least 20ms latency");
}

// ============================================================================
// WallpaperRenderer Trait Tests
// ============================================================================

#[test]
fn test_trait_impl_init() {
    let mut renderer: Box<dyn WallpaperRenderer> = Box::new(DisplaySimulator::default_config());
    let display = DisplayId("test".to_string());

    assert!(renderer.init(&display).is_ok());
}

#[test]
fn test_trait_impl_render() {
    let mut renderer: Box<dyn WallpaperRenderer> = Box::new(DisplaySimulator::default_config());
    renderer.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(100, 100, 255, 0, 0, 255);
    assert!(renderer.render(&frame).is_ok());
}

#[test]
fn test_trait_impl_shutdown() {
    let mut renderer: Box<dyn WallpaperRenderer> = Box::new(DisplaySimulator::default_config());
    renderer.init(&DisplayId("test".into())).unwrap();

    // Should not panic
    renderer.shutdown();
}

#[test]
fn test_trait_impl_restore() {
    let mut renderer: Box<dyn WallpaperRenderer> = Box::new(DisplaySimulator::default_config());
    renderer.init(&DisplayId("test".into())).unwrap();

    let config = micround::config::AppConfig::default();
    assert!(renderer.restore(&config).is_ok());
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

#[test]
fn test_simulator_send() {
    fn assert_send<T: Send>() {}
    assert_send::<DisplaySimulator>();
}

#[test]
fn test_concurrent_stats_access() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(10, 10, 0, 0, 0, 255);
    for _ in 0..10 {
        sim.render(&frame).unwrap();
    }

    // Stats should be accessible from same thread
    // (true multi-threaded access would require Arc<Mutex<>>)
    let stats1 = sim.stats();
    let stats2 = sim.stats();
    assert_eq!(stats1.frames_rendered, stats2.frames_rendered);
}

// ============================================================================
// Performance Tests
// ============================================================================

#[test]
fn test_render_performance() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(1920, 1080, 128, 128, 128, 255);

    assert_completes_within(Duration::from_secs(5), || {
        for _ in 0..100 {
            sim.render(&frame).unwrap();
        }
    });
}

#[test]
fn test_frame_history_access_performance() {
    let config = DisplaySimulatorConfig {
        frame_history_size: 10,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(50, 50, 0, 0, 0, 255);
    for _ in 0..10 {
        sim.render(&frame).unwrap();
    }

    // Accessing history should complete in reasonable time
    assert_completes_within(Duration::from_secs(2), || {
        for _ in 0..100 {
            let _ = sim.frame_history();
        }
    });
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_very_small_frame() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(1, 1, 255, 0, 0, 255);
    assert!(sim.render(&frame).is_ok());

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.width, 1);
    assert_eq!(captured.height, 1);
}

#[test]
fn test_large_frame() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    // 4K frame
    let frame = solid_frame(3840, 2160, 0, 0, 0, 255);
    assert!(sim.render(&frame).is_ok());

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.width, 3840);
    assert_eq!(captured.height, 2160);
}

#[test]
fn test_single_frame_history() {
    let config = DisplaySimulatorConfig {
        frame_history_size: 1,
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);
    sim.init(&DisplayId("test".into())).unwrap();

    for i in 0u8..5 {
        let frame = solid_frame(10, 10, i * 50, 0, 0, 255);
        sim.render(&frame).unwrap();
    }

    // Should only have the last frame
    let history = sim.frame_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].pixel_at(0, 0).unwrap().0, 200);
}

#[test]
fn test_empty_display_name() {
    let config = DisplaySimulatorConfig {
        display_name: "".to_string(),
        ..Default::default()
    };
    let mut sim = DisplaySimulator::new(config);

    // Should still work
    assert!(sim.init(&DisplayId("test".into())).is_ok());
}

#[test]
fn test_captured_frame_metadata() {
    let mut sim = DisplaySimulator::default_config();
    sim.init(&DisplayId("test".into())).unwrap();

    let frame = solid_frame(100, 50, 0, 0, 0, 255);
    sim.render(&frame).unwrap();

    let captured = sim.last_frame().unwrap();
    assert_eq!(captured.width, 100);
    assert_eq!(captured.height, 50);
    assert_eq!(captured.data.len(), 100 * 50 * 4);
}
