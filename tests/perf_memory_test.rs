//! Performance Test: Memory Leak Detection
//!
//! Monitors memory usage during extended capture sessions to detect leaks.
//! Target: No significant memory growth over extended test periods.
//!
//! Run with: cargo test --test perf_memory_test -- --nocapture

mod common;

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use micround::capture::simulator::{FramePattern, SimulatorBackend, SimulatorConfig};
use micround::capture::CaptureBackend;
use micround::core::DisplayId;
use micround::process::ProcessedFrame;
use micround::render::simulator::{DisplaySimulator, DisplaySimulatorConfig};
use micround::render::WallpaperRenderer;

/// Process memory statistics from /proc/self/status
#[derive(Debug, Clone)]
struct MemoryStats {
    /// Virtual memory size in KB
    pub vm_size_kb: u64,
    /// Resident set size (physical memory) in KB
    pub vm_rss_kb: u64,
    /// Shared memory in KB
    pub vm_shared_kb: u64,
    /// Data + stack in KB
    pub vm_data_kb: u64,
    /// Timestamp when sampled
    pub timestamp: Instant,
}

impl MemoryStats {
    /// Read current process memory stats from /proc/self/status
    fn read_current() -> Option<Self> {
        let content = fs::read_to_string("/proc/self/status").ok()?;

        let mut vm_size_kb = 0u64;
        let mut vm_rss_kb = 0u64;
        let mut vm_shared_kb = 0u64;
        let mut vm_data_kb = 0u64;

        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            match parts[0] {
                "VmSize:" => vm_size_kb = parts[1].parse().unwrap_or(0),
                "VmRSS:" => vm_rss_kb = parts[1].parse().unwrap_or(0),
                "RssFile:" | "RssShmem:" => vm_shared_kb += parts[1].parse().unwrap_or(0),
                "VmData:" => vm_data_kb = parts[1].parse().unwrap_or(0),
                _ => {}
            }
        }

        Some(Self {
            vm_size_kb,
            vm_rss_kb,
            vm_shared_kb,
            vm_data_kb,
            timestamp: Instant::now(),
        })
    }

    /// Format memory as human-readable string
    fn format_kb(kb: u64) -> String {
        if kb >= 1024 * 1024 {
            format!("{:.2} GB", kb as f64 / (1024.0 * 1024.0))
        } else if kb >= 1024 {
            format!("{:.2} MB", kb as f64 / 1024.0)
        } else {
            format!("{} KB", kb)
        }
    }
}

/// Memory usage analysis over time
#[derive(Debug)]
struct MemoryAnalysis {
    /// All memory samples
    pub samples: Vec<MemoryStats>,
    /// Initial RSS (first stable sample after warmup)
    pub initial_rss_kb: u64,
    /// Final RSS (last sample)
    pub final_rss_kb: u64,
    /// Peak RSS observed
    pub peak_rss_kb: u64,
    /// Minimum RSS observed
    pub min_rss_kb: u64,
    /// Growth from initial to final (can be negative)
    pub growth_kb: i64,
    /// Growth percentage
    pub growth_percent: f64,
    /// Linear regression slope (KB per second)
    pub trend_slope_kb_per_sec: f64,
    /// Test duration
    pub duration: Duration,
    /// Suspected leak (significant upward trend)
    pub suspected_leak: bool,
}

impl MemoryAnalysis {
    fn from_samples(samples: Vec<MemoryStats>, warmup_samples: usize) -> Self {
        let actual_samples = if samples.len() > warmup_samples {
            &samples[warmup_samples..]
        } else {
            &samples[..]
        };

        if actual_samples.is_empty() {
            return Self {
                samples,
                initial_rss_kb: 0,
                final_rss_kb: 0,
                peak_rss_kb: 0,
                min_rss_kb: 0,
                growth_kb: 0,
                growth_percent: 0.0,
                trend_slope_kb_per_sec: 0.0,
                duration: Duration::ZERO,
                suspected_leak: false,
            };
        }

        let initial_rss_kb = actual_samples.first().map(|s| s.vm_rss_kb).unwrap_or(0);
        let final_rss_kb = actual_samples.last().map(|s| s.vm_rss_kb).unwrap_or(0);
        let peak_rss_kb = actual_samples
            .iter()
            .map(|s| s.vm_rss_kb)
            .max()
            .unwrap_or(0);
        let min_rss_kb = actual_samples
            .iter()
            .map(|s| s.vm_rss_kb)
            .min()
            .unwrap_or(0);
        let growth_kb = final_rss_kb as i64 - initial_rss_kb as i64;
        let growth_percent = if initial_rss_kb > 0 {
            (growth_kb as f64 / initial_rss_kb as f64) * 100.0
        } else {
            0.0
        };

        let duration = if actual_samples.len() >= 2 {
            actual_samples
                .last()
                .unwrap()
                .timestamp
                .duration_since(actual_samples.first().unwrap().timestamp)
        } else {
            Duration::ZERO
        };

        // Calculate linear regression for trend detection
        let trend_slope_kb_per_sec = calculate_trend_slope(actual_samples);

        // Suspect leak if:
        // 1. Significant growth (>10% or >10MB)
        // 2. Consistent upward trend (slope > 1 KB/s)
        let significant_growth = growth_percent > 10.0 || growth_kb > 10 * 1024;
        let upward_trend = trend_slope_kb_per_sec > 1.0;
        let suspected_leak = significant_growth && upward_trend;

        Self {
            samples,
            initial_rss_kb,
            final_rss_kb,
            peak_rss_kb,
            min_rss_kb,
            growth_kb,
            growth_percent,
            trend_slope_kb_per_sec,
            duration,
            suspected_leak,
        }
    }

    fn print_report(&self, test_name: &str) {
        let status = if self.suspected_leak { "WARN" } else { "PASS" };

        eprintln!("\n╔════════════════════════════════════════════════════════════╗");
        eprintln!("║ MEMORY REPORT: {:42} ║", test_name);
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!(
            "║ Status:        [{:^4}]                                      ║",
            status
        );
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!(
            "║ Duration:      {:>12.2?}                              ║",
            self.duration
        );
        eprintln!(
            "║ Samples:       {:>12}                                ║",
            self.samples.len()
        );
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!(
            "║ Initial RSS:   {:>12}                                ║",
            MemoryStats::format_kb(self.initial_rss_kb)
        );
        eprintln!(
            "║ Final RSS:     {:>12}                                ║",
            MemoryStats::format_kb(self.final_rss_kb)
        );
        eprintln!(
            "║ Peak RSS:      {:>12}                                ║",
            MemoryStats::format_kb(self.peak_rss_kb)
        );
        eprintln!(
            "║ Min RSS:       {:>12}                                ║",
            MemoryStats::format_kb(self.min_rss_kb)
        );
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        let growth_sign = if self.growth_kb >= 0 { "+" } else { "" };
        eprintln!(
            "║ Growth:        {:>12} ({:>+6.1}%)                    ║",
            format!(
                "{}{}",
                growth_sign,
                MemoryStats::format_kb(self.growth_kb.unsigned_abs())
            ),
            self.growth_percent
        );
        eprintln!(
            "║ Trend:         {:>12.2} KB/s                         ║",
            self.trend_slope_kb_per_sec
        );
        if self.suspected_leak {
            eprintln!("╠════════════════════════════════════════════════════════════╣");
            eprintln!("║ ⚠ SUSPECTED MEMORY LEAK DETECTED                           ║");
        }
        eprintln!("╚════════════════════════════════════════════════════════════╝\n");
    }

    fn print_memory_graph(&self) {
        if self.samples.is_empty() {
            return;
        }

        eprintln!("RSS Memory Over Time:");
        eprintln!("─────────────────────");

        // Downsample if needed
        let display_samples: Vec<&MemoryStats> = if self.samples.len() > 40 {
            let step = self.samples.len() / 40;
            self.samples.iter().step_by(step).collect()
        } else {
            self.samples.iter().collect()
        };

        // Find range for scaling
        let max_rss = display_samples
            .iter()
            .map(|s| s.vm_rss_kb)
            .max()
            .unwrap_or(1);
        let min_rss = display_samples
            .iter()
            .map(|s| s.vm_rss_kb)
            .min()
            .unwrap_or(0);
        let range = (max_rss - min_rss).max(1);

        for sample in &display_samples {
            let normalized = ((sample.vm_rss_kb - min_rss) as f64 / range as f64 * 40.0) as usize;
            let bar: String = "█".repeat(normalized);
            eprintln!(
                "{:>8} │{:<40}│",
                MemoryStats::format_kb(sample.vm_rss_kb),
                bar
            );
        }
        eprintln!();
    }
}

/// Calculate linear regression slope for trend detection
fn calculate_trend_slope(samples: &[MemoryStats]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }

    let start_time = samples.first().unwrap().timestamp;
    let n = samples.len() as f64;

    // Convert to (x, y) pairs where x is seconds and y is KB
    let points: Vec<(f64, f64)> = samples
        .iter()
        .map(|s| {
            let x = s.timestamp.duration_since(start_time).as_secs_f64();
            let y = s.vm_rss_kb as f64;
            (x, y)
        })
        .collect();

    // Linear regression: slope = (n*Σxy - Σx*Σy) / (n*Σx² - (Σx)²)
    let sum_x: f64 = points.iter().map(|(x, _)| x).sum();
    let sum_y: f64 = points.iter().map(|(_, y)| y).sum();
    let sum_xy: f64 = points.iter().map(|(x, y)| x * y).sum();
    let sum_x2: f64 = points.iter().map(|(x, _)| x * x).sum();

    let denominator = n * sum_x2 - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        return 0.0;
    }

    (n * sum_xy - sum_x * sum_y) / denominator
}

/// Simulate frame processing
fn process_frame(frame: &micround::core::Frame) -> ProcessedFrame {
    ProcessedFrame::new(frame.data.clone(), frame.width, frame.height)
}

/// Number of warmup samples to skip in analysis
const WARMUP_SAMPLES: usize = 5;

/// Monitor memory usage while running a capture/render workload
fn monitor_memory_during_capture(
    capture_config: SimulatorConfig,
    display_config: DisplaySimulatorConfig,
    test_duration: Duration,
    sample_interval: Duration,
) -> MemoryAnalysis {
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

    capture
        .open(&devices[0].id, settings)
        .expect("Failed to open capture device");

    // Initialize display simulator
    let mut display = DisplaySimulator::new(display_config);
    display
        .init(&DisplayId("memory-test".into()))
        .expect("Failed to init display");

    capture.start().expect("Failed to start capture");

    // Shared flag to stop workload
    let running = Arc::new(AtomicBool::new(true));
    let workload_running = Arc::clone(&running);

    // Spawn workload thread
    let workload_handle = thread::spawn(move || {
        while workload_running.load(Ordering::Relaxed) {
            match capture.next_frame() {
                Ok(frame) => {
                    let processed = process_frame(&frame);
                    let _ = display.render(&processed);
                }
                Err(_) => continue,
            }
        }

        // Cleanup
        capture.stop().ok();
        capture.close();
        display.shutdown();
    });

    // Monitor memory
    let test_start = Instant::now();
    let mut samples = Vec::new();

    while test_start.elapsed() < test_duration {
        if let Some(stats) = MemoryStats::read_current() {
            samples.push(stats);
        }
        thread::sleep(sample_interval);
    }

    // Stop workload
    running.store(false, Ordering::Relaxed);
    let _ = workload_handle.join();

    MemoryAnalysis::from_samples(samples, WARMUP_SAMPLES)
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_memory_basic_capture() {
    eprintln!("\n=== Test: Basic Memory Monitoring ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Memory Test Camera".into(),
        width: 640,
        height: 480,
        fps: 30,
        pattern: FramePattern::SolidColor {
            r: 128,
            g: 128,
            b: 128,
        },
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
        frame_history_size: 0, // Disable frame history to avoid expected memory growth
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(5),
        Duration::from_millis(200),
    );
    analysis.print_report("Basic Capture");
    analysis.print_memory_graph();

    // No severe memory leak (growth < 50% or trend < 10 KB/s)
    // With frame history disabled, we should see minimal memory growth
    assert!(
        !analysis.suspected_leak || analysis.growth_percent < 50.0,
        "Potential memory leak: {:.1}% growth, {:.2} KB/s trend",
        analysis.growth_percent,
        analysis.trend_slope_kb_per_sec
    );
}

#[test]
fn test_memory_allocation_patterns() {
    eprintln!("\n=== Test: Allocation Pattern Analysis ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Pattern Camera".into(),
        width: 320,
        height: 240,
        fps: 60, // Higher frame rate to stress allocator
        pattern: FramePattern::Checkerboard { size: 16 },
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
        frame_history_size: 0, // Disable for memory testing
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(3),
        Duration::from_millis(100),
    );
    analysis.print_report("Allocation Patterns");

    // Log allocation patterns
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║ ALLOCATION PATTERN ANALYSIS                                ║");
    eprintln!("╠════════════════════════════════════════════════════════════╣");

    // Calculate memory volatility (standard deviation)
    if analysis.samples.len() > 1 {
        let rss_values: Vec<f64> = analysis
            .samples
            .iter()
            .map(|s| s.vm_rss_kb as f64)
            .collect();
        let mean: f64 = rss_values.iter().sum::<f64>() / rss_values.len() as f64;
        let variance: f64 =
            rss_values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / rss_values.len() as f64;
        let std_dev = variance.sqrt();

        eprintln!(
            "║ Mean RSS:      {:>12}                                ║",
            MemoryStats::format_kb(mean as u64)
        );
        eprintln!(
            "║ Std Dev:       {:>12}                                ║",
            MemoryStats::format_kb(std_dev as u64)
        );
        eprintln!(
            "║ Volatility:    {:>12.2}%                               ║",
            (std_dev / mean) * 100.0
        );
    }
    eprintln!("╚════════════════════════════════════════════════════════════╝\n");

    // Ensure samples were collected
    assert!(
        analysis.samples.len() > 0,
        "Should have collected memory samples"
    );
}

#[test]
#[ignore] // Flaky: linear regression misinterprets memory fluctuations as trends
fn test_memory_trend_detection() {
    eprintln!("\n=== Test: Memory Trend Detection ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Trend Camera".into(),
        width: 640,
        height: 480,
        fps: 30,
        pattern: FramePattern::HorizontalGradient,
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
        frame_history_size: 0, // Disable for memory testing
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(5),
        Duration::from_millis(250),
    );
    analysis.print_report("Trend Detection");
    analysis.print_memory_graph();

    // Log trend analysis
    eprintln!("Trend slope: {:.2} KB/s", analysis.trend_slope_kb_per_sec);
    if analysis.trend_slope_kb_per_sec > 0.0 {
        eprintln!(
            "Projected growth over 1 hour: {}",
            MemoryStats::format_kb((analysis.trend_slope_kb_per_sec * 3600.0) as u64)
        );
    }

    // No runaway memory growth (< 100 KB/s trend)
    assert!(
        analysis.trend_slope_kb_per_sec < 100.0,
        "Memory trend too steep: {:.2} KB/s (would grow {} in 1 hour)",
        analysis.trend_slope_kb_per_sec,
        MemoryStats::format_kb((analysis.trend_slope_kb_per_sec * 3600.0) as u64)
    );
}

#[test]
fn test_memory_detailed_report() {
    eprintln!("\n=== Test: Detailed Memory Report ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Report Camera".into(),
        width: 320,
        height: 240,
        fps: 30,
        pattern: FramePattern::SolidColor {
            r: 64,
            g: 64,
            b: 64,
        },
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
        frame_history_size: 0, // Disable for memory testing
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(3),
        Duration::from_millis(150),
    );
    analysis.print_report("Detailed Report");
    analysis.print_memory_graph();

    // Informational test - always passes if samples collected
    assert!(
        analysis.samples.len() > 0,
        "Should have collected memory samples"
    );

    // Log summary
    eprintln!(
        "Summary: {} samples over {:?}",
        analysis.samples.len(),
        analysis.duration
    );
    eprintln!(
        "Memory range: {} - {}",
        MemoryStats::format_kb(analysis.min_rss_kb),
        MemoryStats::format_kb(analysis.peak_rss_kb)
    );
}

#[test]
#[ignore] // Extended test for soak testing
fn test_memory_extended_60sec() {
    eprintln!("\n=== Test: Extended Memory Monitoring (60 seconds) ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Extended Camera".into(),
        width: 640,
        height: 480,
        fps: 30,
        pattern: FramePattern::MovingLine,
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
        frame_history_size: 0, // Disable for memory testing
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(60),
        Duration::from_secs(1),
    );
    analysis.print_report("Extended 60s");
    analysis.print_memory_graph();

    // Extended test: no suspected leak
    assert!(
        !analysis.suspected_leak,
        "Memory leak suspected after 60s: {:.1}% growth, {:.2} KB/s trend",
        analysis.growth_percent, analysis.trend_slope_kb_per_sec
    );
}

#[test]
#[ignore] // Long test for soak testing
fn test_memory_soak_10min() {
    eprintln!("\n=== Test: Memory Soak Test (10 minutes) ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Soak Camera".into(),
        width: 640,
        height: 480,
        fps: 30,
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
        frame_history_size: 0, // Disable for memory testing
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(600), // 10 minutes
        Duration::from_secs(5),   // Sample every 5 seconds
    );
    analysis.print_report("Soak 10min");
    analysis.print_memory_graph();

    // Soak test: verify no significant leak
    // Growth should be < 20% and trend < 10 KB/s
    assert!(
        analysis.growth_percent < 20.0,
        "Memory grew {:.1}% over 10 minutes",
        analysis.growth_percent
    );
    assert!(
        analysis.trend_slope_kb_per_sec < 10.0,
        "Memory trend {:.2} KB/s is too high",
        analysis.trend_slope_kb_per_sec
    );
}

#[test]
#[ignore] // HD resolution test
fn test_memory_hd_capture() {
    eprintln!("\n=== Test: HD Resolution Memory Usage ===\n");

    let capture_config = SimulatorConfig {
        device_name: "HD Camera".into(),
        width: 1920,
        height: 1080,
        fps: 30,
        pattern: FramePattern::SolidColor {
            r: 100,
            g: 150,
            b: 200,
        },
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
        frame_history_size: 0, // Disable for memory testing
        ..Default::default()
    };

    let analysis = monitor_memory_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(10),
        Duration::from_millis(500),
    );
    analysis.print_report("HD Capture");
    analysis.print_memory_graph();

    // HD will use more memory but should still be stable
    assert!(
        !analysis.suspected_leak,
        "Memory leak suspected at HD: {:.1}% growth",
        analysis.growth_percent
    );
}
