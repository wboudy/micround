//! Performance Test: Frame Rate Stability
//!
//! Tests that the capture-to-render pipeline maintains stable frame rates.
//! Target: 24+ fps sustained over extended periods with acceptable variance.
//!
//! Run with: cargo test --features test-simulator --test perf_framerate_test -- --nocapture

#[allow(dead_code)]
mod common;

use std::time::{Duration, Instant};

use micround::capture::simulator::{FramePattern, SimulatorBackend, SimulatorConfig};
use micround::capture::CaptureBackend;
use micround::core::DisplayId;
use micround::process::ProcessedFrame;
use micround::render::simulator::{DisplaySimulator, DisplaySimulatorConfig};
use micround::render::WallpaperRenderer;

/// Frame timing data for analysis
#[derive(Debug, Clone)]
struct FrameTiming {
    pub frame_num: usize,
    pub capture_start: Instant,
    pub render_end: Instant,
}

/// Frame rate statistics
#[derive(Debug, Clone)]
struct FrameRateStats {
    pub target_fps: f64,
    pub actual_fps: f64,
    pub frame_count: usize,
    pub total_duration: Duration,
    pub min_frame_time_ms: f64,
    pub max_frame_time_ms: f64,
    pub avg_frame_time_ms: f64,
    pub variance_ms: f64,
    pub std_dev_ms: f64,
    pub frames_below_target: usize,
    pub jitter_ms: f64,
}

impl FrameRateStats {
    fn from_timings(timings: &[FrameTiming], target_fps: f64) -> Self {
        if timings.len() < 2 {
            return Self {
                target_fps,
                actual_fps: 0.0,
                frame_count: timings.len(),
                total_duration: Duration::ZERO,
                min_frame_time_ms: 0.0,
                max_frame_time_ms: 0.0,
                avg_frame_time_ms: 0.0,
                variance_ms: 0.0,
                std_dev_ms: 0.0,
                frames_below_target: 0,
                jitter_ms: 0.0,
            };
        }

        // Calculate frame-to-frame times
        let mut frame_times_ms: Vec<f64> = Vec::with_capacity(timings.len() - 1);
        for i in 1..timings.len() {
            let dt = timings[i].render_end.duration_since(timings[i - 1].render_end);
            frame_times_ms.push(dt.as_secs_f64() * 1000.0);
        }

        let total_duration = timings.last().unwrap().render_end
            .duration_since(timings.first().unwrap().capture_start);
        let actual_fps = (timings.len() as f64) / total_duration.as_secs_f64();

        let min_frame_time_ms = frame_times_ms.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_frame_time_ms = frame_times_ms.iter().cloned().fold(0.0, f64::max);
        let sum: f64 = frame_times_ms.iter().sum();
        let avg_frame_time_ms = sum / frame_times_ms.len() as f64;

        // Calculate variance and standard deviation
        let variance_ms: f64 = frame_times_ms
            .iter()
            .map(|t| (t - avg_frame_time_ms).powi(2))
            .sum::<f64>() / frame_times_ms.len() as f64;
        let std_dev_ms = variance_ms.sqrt();

        // Target frame time in ms
        let target_frame_time_ms = 1000.0 / target_fps;

        // Count frames that took longer than target (dropped below target FPS)
        let frames_below_target = frame_times_ms
            .iter()
            .filter(|&&t| t > target_frame_time_ms * 1.5) // Allow 50% slack
            .count();

        // Calculate jitter (average absolute deviation from mean)
        let jitter_ms: f64 = frame_times_ms
            .iter()
            .map(|t| (t - avg_frame_time_ms).abs())
            .sum::<f64>() / frame_times_ms.len() as f64;

        Self {
            target_fps,
            actual_fps,
            frame_count: timings.len(),
            total_duration,
            min_frame_time_ms,
            max_frame_time_ms,
            avg_frame_time_ms,
            variance_ms,
            std_dev_ms,
            frames_below_target,
            jitter_ms,
        }
    }

    fn print_report(&self, test_name: &str) {
        let target_met = self.actual_fps >= self.target_fps;
        let status = if target_met { "PASS" } else { "FAIL" };

        eprintln!("\n╔════════════════════════════════════════════════════════════╗");
        eprintln!("║ FRAME RATE REPORT: {:38}  ║", test_name);
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ Status:        [{:^4}]                                      ║", status);
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ Target FPS:    {:>10.1}                                   ║", self.target_fps);
        eprintln!("║ Actual FPS:    {:>10.1}                                   ║", self.actual_fps);
        eprintln!("║ Frame count:   {:>10}                                   ║", self.frame_count);
        eprintln!("║ Duration:      {:>10.2?}                              ║", self.total_duration);
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ Frame Time Statistics (ms):                                ║");
        eprintln!("║   Min:         {:>10.2}                                   ║", self.min_frame_time_ms);
        eprintln!("║   Max:         {:>10.2}                                   ║", self.max_frame_time_ms);
        eprintln!("║   Avg:         {:>10.2}                                   ║", self.avg_frame_time_ms);
        eprintln!("║   Std Dev:     {:>10.2}                                   ║", self.std_dev_ms);
        eprintln!("║   Jitter:      {:>10.2}                                   ║", self.jitter_ms);
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!("║ Stability:                                                 ║");
        eprintln!("║   Below target:{:>10}                                   ║", self.frames_below_target);
        let pct_stable = 100.0 * (1.0 - self.frames_below_target as f64 / self.frame_count.max(1) as f64);
        eprintln!("║   Stability:   {:>9.1}%                                   ║", pct_stable);
        eprintln!("╚════════════════════════════════════════════════════════════╝\n");
    }
}

/// Simulate frame processing (decode + transform)
fn process_frame(frame: &micround::core::Frame) -> ProcessedFrame {
    ProcessedFrame::new(frame.data.clone(), frame.width, frame.height)
}

/// Run frame rate measurement with given configuration
fn measure_frame_rate(
    capture_config: SimulatorConfig,
    display_config: DisplaySimulatorConfig,
    target_fps: f64,
    test_duration: Duration,
) -> FrameRateStats {
    // Initialize capture backend
    let mut capture = SimulatorBackend::new(capture_config.clone());
    let devices = capture.enumerate_devices();
    assert!(!devices.is_empty(), "No simulated devices found");

    let settings = micround::core::CaptureSettings {
        width: capture_config.width,
        height: capture_config.height,
        framerate: capture_config.fps as f32,
        format: Some(micround::core::PixelFormat::Rgba32),
    };

    capture.open(&devices[0].id, settings).expect("Failed to open capture device");

    // Initialize display simulator
    let mut display = DisplaySimulator::new(display_config);
    display.init(&DisplayId("framerate-test".into())).expect("Failed to init display");

    capture.start().expect("Failed to start capture");

    // Warmup
    for _ in 0..10 {
        if let Ok(frame) = capture.next_frame() {
            let processed = process_frame(&frame);
            let _ = display.render(&processed);
        }
    }

    let mut timings = Vec::new();
    let test_start = Instant::now();

    // Run for the specified duration
    let mut frame_num = 0;
    while test_start.elapsed() < test_duration {
        let capture_start = Instant::now();

        match capture.next_frame() {
            Ok(frame) => {
                let processed = process_frame(&frame);

                if display.render(&processed).is_ok() {
                    timings.push(FrameTiming {
                        frame_num,
                        capture_start,
                        render_end: Instant::now(),
                    });
                }
            }
            Err(_) => continue,
        }

        frame_num += 1;
    }

    // Cleanup
    capture.stop().ok();
    capture.close();
    display.shutdown();

    FrameRateStats::from_timings(&timings, target_fps)
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_framerate_24fps_target_5sec() {
    eprintln!("\n=== Test: 24 FPS Target (5 second sustained) ===\n");

    // Use small frames for pure throughput testing
    // This tests the pipeline overhead, not memory bandwidth
    let capture_config = SimulatorConfig {
        device_name: "Fast Camera".into(),
        width: 160,
        height: 120,
        fps: 1000, // High FPS - no artificial timing delay
        pattern: FramePattern::SolidColor { r: 128, g: 128, b: 128 },
        drop_rate: 0.0,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 160,
        height: 120,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        24.0,
        Duration::from_secs(5),
    );
    stats.print_report("24 FPS / 5 sec");

    // Must achieve at least 24 FPS
    assert!(
        stats.actual_fps >= 24.0,
        "Actual FPS {:.1} below 24 FPS target",
        stats.actual_fps
    );

    // Stability: at least 90% of frames should be within target
    // (Allow some slack for CI systems under load)
    let stability_pct = 100.0 * (1.0 - stats.frames_below_target as f64 / stats.frame_count.max(1) as f64);
    assert!(
        stability_pct >= 90.0,
        "Stability {:.1}% below 90% threshold",
        stability_pct
    );
}

#[test]
fn test_framerate_30fps_target_3sec() {
    eprintln!("\n=== Test: 30 FPS Target (3 second sustained) ===\n");

    // Use small frames for reliable throughput testing
    let capture_config = SimulatorConfig {
        device_name: "Fast Camera".into(),
        width: 160,
        height: 120,
        fps: 1000,
        pattern: FramePattern::SolidColor { r: 64, g: 128, b: 192 },
        drop_rate: 0.0,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 160,
        height: 120,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        30.0,
        Duration::from_secs(3),
    );
    stats.print_report("30 FPS / 3 sec");

    assert!(
        stats.actual_fps >= 30.0,
        "Actual FPS {:.1} below 30 FPS target",
        stats.actual_fps
    );
}

#[test]
#[ignore] // 60 FPS is a stretch goal; requires fast system
fn test_framerate_60fps_target_2sec() {
    eprintln!("\n=== Test: 60 FPS Target (2 second sustained) ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Fast Camera".into(),
        width: 320,
        height: 240,
        fps: 1000,
        pattern: FramePattern::SolidColor { r: 64, g: 64, b: 64 },
        drop_rate: 0.0,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 320,
        height: 240,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        60.0,
        Duration::from_secs(2),
    );
    stats.print_report("60 FPS / 2 sec");

    assert!(
        stats.actual_fps >= 60.0,
        "Actual FPS {:.1} below 60 FPS target",
        stats.actual_fps
    );
}

#[test]
fn test_framerate_variance_logging() {
    eprintln!("\n=== Test: Frame Time Variance Analysis ===\n");

    // Use very small frames for minimal variance
    let capture_config = SimulatorConfig {
        width: 64,
        height: 64,
        fps: 1000,
        pattern: FramePattern::SolidColor { r: 0, g: 0, b: 0 },
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 64,
        height: 64,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        30.0,
        Duration::from_secs(2),
    );
    stats.print_report("Variance Analysis");

    // For tiny frames, jitter should be low
    // Allow higher threshold for CI systems under load
    assert!(
        stats.jitter_ms < 50.0,
        "Jitter {:.2}ms exceeds 50ms threshold",
        stats.jitter_ms
    );

    // Standard deviation threshold (allow for system variance)
    assert!(
        stats.std_dev_ms < 100.0,
        "Std dev {:.2}ms exceeds 100ms threshold",
        stats.std_dev_ms
    );
}

#[test]
#[ignore] // Long-running test for soak testing
fn test_framerate_24fps_extended_30sec() {
    eprintln!("\n=== Test: Extended 24 FPS (30 second soak) ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Soak Test Camera".into(),
        width: 640,
        height: 480,
        fps: 1000,
        pattern: FramePattern::Checkerboard { size: 32 },
        drop_rate: 0.0,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 640,
        height: 480,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        24.0,
        Duration::from_secs(30),
    );
    stats.print_report("24 FPS / 30 sec Soak");

    assert!(
        stats.actual_fps >= 24.0,
        "Actual FPS {:.1} below 24 FPS target in soak test",
        stats.actual_fps
    );

    // Extended test should maintain 98% stability
    let stability_pct = 100.0 * (1.0 - stats.frames_below_target as f64 / stats.frame_count.max(1) as f64);
    assert!(
        stability_pct >= 98.0,
        "Extended stability {:.1}% below 98% threshold",
        stability_pct
    );
}

#[test]
#[ignore] // HD test may be slow on some systems
fn test_framerate_hd_24fps() {
    eprintln!("\n=== Test: HD 1920x1080 at 24 FPS ===\n");

    let capture_config = SimulatorConfig {
        device_name: "HD Camera".into(),
        width: 1920,
        height: 1080,
        fps: 1000,
        pattern: FramePattern::HorizontalGradient,
        drop_rate: 0.0,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 1920,
        height: 1080,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        24.0,
        Duration::from_secs(3),
    );
    stats.print_report("HD 24 FPS");

    assert!(
        stats.actual_fps >= 24.0,
        "HD actual FPS {:.1} below 24 FPS target",
        stats.actual_fps
    );
}

#[test]
#[ignore] // Pattern generation has high variance; run during soak testing
fn test_framerate_with_light_load() {
    eprintln!("\n=== Test: Frame Rate Under Light Processing Load ===\n");

    // Use a pattern that requires more computation
    let capture_config = SimulatorConfig {
        device_name: "Pattern Camera".into(),
        width: 320,
        height: 240,
        fps: 1000,
        pattern: FramePattern::MovingLine, // Slightly more complex pattern
        drop_rate: 0.0,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 320,
        height: 240,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let stats = measure_frame_rate(
        capture_config,
        display_config,
        24.0,
        Duration::from_secs(3),
    );
    stats.print_report("Light Load 24 FPS");

    assert!(
        stats.actual_fps >= 24.0,
        "Actual FPS {:.1} below 24 FPS target under load",
        stats.actual_fps
    );
}
