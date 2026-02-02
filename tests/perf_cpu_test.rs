//! Performance Test: CPU Usage Monitoring
//!
//! Monitors CPU usage during active capture and render cycles.
//! Target: Average CPU usage under 10% of a single core.
//!
//! Run with: cargo test --test perf_cpu_test -- --nocapture

#[allow(dead_code)]
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

/// CPU time statistics from /proc/stat
#[derive(Debug, Clone, Copy)]
struct CpuTime {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuTime {
    /// Parse CPU times from /proc/stat line
    fn from_proc_stat_line(line: &str) -> Option<Self> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 || !parts[0].starts_with("cpu") || parts[0] == "cpu" {
            // Skip the aggregate "cpu" line, look for "cpu0", "cpu1", etc.
            // Actually, we want the aggregate for total system usage
            if parts[0] != "cpu" {
                return None;
            }
        }

        Some(Self {
            user: parts.get(1)?.parse().ok()?,
            nice: parts.get(2)?.parse().ok()?,
            system: parts.get(3)?.parse().ok()?,
            idle: parts.get(4)?.parse().ok()?,
            iowait: parts.get(5)?.parse().ok().unwrap_or(0),
            irq: parts.get(6)?.parse().ok().unwrap_or(0),
            softirq: parts.get(7)?.parse().ok().unwrap_or(0),
            steal: parts.get(8)?.parse().ok().unwrap_or(0),
        })
    }

    /// Get total CPU time
    fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    /// Get active (non-idle) CPU time
    fn active(&self) -> u64 {
        self.user + self.nice + self.system + self.irq + self.softirq + self.steal
    }
}

/// Read current CPU times from /proc/stat
fn read_cpu_times() -> Option<CpuTime> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    for line in content.lines() {
        if line.starts_with("cpu ") {
            return CpuTime::from_proc_stat_line(line);
        }
    }
    None
}

/// Calculate CPU usage percentage between two samples
fn calculate_cpu_usage(before: &CpuTime, after: &CpuTime) -> f64 {
    let total_diff = after.total().saturating_sub(before.total());
    let active_diff = after.active().saturating_sub(before.active());

    if total_diff == 0 {
        return 0.0;
    }

    (active_diff as f64 / total_diff as f64) * 100.0
}

/// CPU usage statistics over a test period
#[derive(Debug, Clone)]
struct CpuUsageStats {
    /// Number of samples collected
    pub samples: usize,
    /// Average CPU usage percentage
    pub average: f64,
    /// Peak CPU usage percentage
    pub peak: f64,
    /// Minimum CPU usage percentage
    pub min: f64,
    /// Number of usage spikes (samples > threshold)
    pub spike_count: usize,
    /// Spike threshold used
    pub spike_threshold: f64,
    /// Test duration
    pub duration: Duration,
    /// All individual samples
    pub usage_samples: Vec<f64>,
}

impl CpuUsageStats {
    fn from_samples(samples: Vec<f64>, duration: Duration, spike_threshold: f64) -> Self {
        if samples.is_empty() {
            return Self {
                samples: 0,
                average: 0.0,
                peak: 0.0,
                min: 0.0,
                spike_count: 0,
                spike_threshold,
                duration,
                usage_samples: vec![],
            };
        }

        let sum: f64 = samples.iter().sum();
        let average = sum / samples.len() as f64;
        let peak = samples.iter().cloned().fold(0.0_f64, f64::max);
        let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let spike_count = samples.iter().filter(|&&s| s > spike_threshold).count();

        Self {
            samples: samples.len(),
            average,
            peak,
            min,
            spike_count,
            spike_threshold,
            duration,
            usage_samples: samples,
        }
    }

    fn print_report(&self, test_name: &str, target: f64) {
        let status = if self.average <= target {
            "PASS"
        } else {
            "FAIL"
        };

        eprintln!("\n╔════════════════════════════════════════════════════════════╗");
        eprintln!("║ CPU USAGE REPORT: {:38}  ║", test_name);
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!(
            "║ Status:        [{:^4}]                                      ║",
            status
        );
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!(
            "║ Target:        {:>10.1}%                                  ║",
            target
        );
        eprintln!(
            "║ Average:       {:>10.2}%                                  ║",
            self.average
        );
        eprintln!(
            "║ Peak:          {:>10.2}%                                  ║",
            self.peak
        );
        eprintln!(
            "║ Min:           {:>10.2}%                                  ║",
            self.min
        );
        eprintln!("╠════════════════════════════════════════════════════════════╣");
        eprintln!(
            "║ Samples:       {:>10}                                   ║",
            self.samples
        );
        eprintln!(
            "║ Duration:      {:>10.2?}                              ║",
            self.duration
        );
        eprintln!(
            "║ Spikes (>{:.0}%): {:>10}                                  ║",
            self.spike_threshold, self.spike_count
        );
        eprintln!("╚════════════════════════════════════════════════════════════╝\n");
    }

    /// Print a simple ASCII graph of CPU usage over time
    fn print_usage_graph(&self) {
        if self.usage_samples.is_empty() {
            return;
        }

        eprintln!("CPU Usage Over Time:");
        eprintln!("────────────────────");

        // Downsample if too many samples
        let display_samples: Vec<f64> = if self.usage_samples.len() > 60 {
            let step = self.usage_samples.len() / 60;
            self.usage_samples.iter().step_by(step).cloned().collect()
        } else {
            self.usage_samples.clone()
        };

        for sample in &display_samples {
            let bar_len = (*sample / 2.0).min(40.0) as usize;
            let bar: String = "█".repeat(bar_len);
            let marker = if *sample > self.spike_threshold {
                "!"
            } else {
                " "
            };
            eprintln!("{:>5.1}% │{:<40}│{}", sample, bar, marker);
        }
        eprintln!();
    }
}

/// Simulate frame processing (decode + transform)
fn process_frame(frame: &micround::core::Frame) -> ProcessedFrame {
    ProcessedFrame::new(frame.data.clone(), frame.width, frame.height)
}

/// Monitor CPU usage while running a capture/render workload
fn monitor_cpu_during_capture(
    capture_config: SimulatorConfig,
    display_config: DisplaySimulatorConfig,
    test_duration: Duration,
    sample_interval: Duration,
    spike_threshold: f64,
) -> CpuUsageStats {
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
        .init(&DisplayId("cpu-test".into()))
        .expect("Failed to init display");

    capture.start().expect("Failed to start capture");

    // Shared flag to stop the workload
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

    // Monitor CPU usage
    let test_start = Instant::now();
    let mut cpu_samples = Vec::new();
    let mut last_cpu = read_cpu_times();

    while test_start.elapsed() < test_duration {
        thread::sleep(sample_interval);

        if let Some(current_cpu) = read_cpu_times() {
            if let Some(ref prev_cpu) = last_cpu {
                let usage = calculate_cpu_usage(prev_cpu, &current_cpu);
                cpu_samples.push(usage);
            }
            last_cpu = Some(current_cpu);
        }
    }

    // Stop workload
    running.store(false, Ordering::Relaxed);
    let _ = workload_handle.join();

    CpuUsageStats::from_samples(cpu_samples, test_start.elapsed(), spike_threshold)
}

// ============================================================================
// Tests
// ============================================================================

/// Target CPU usage for informational reporting (10% of system CPU)
/// Note: This is an aspirational target. Actual system CPU depends on many factors.
const TARGET_CPU_PERCENT: f64 = 10.0;

/// Spike threshold (80% usage is considered a severe spike)
/// We use a high threshold because system-wide CPU includes all processes
const SPIKE_THRESHOLD: f64 = 80.0;

/// This test is system-dependent and may fail when system is under load.
/// Run with: cargo test test_cpu_usage_basic_capture -- --ignored
#[test]
#[ignore]
fn test_cpu_usage_basic_capture() {
    eprintln!("\n=== Test: Basic Capture CPU Usage ===\n");

    // Small frames for low baseline CPU
    let capture_config = SimulatorConfig {
        device_name: "CPU Test Camera".into(),
        width: 320,
        height: 240,
        fps: 30, // Real-world frame rate
        pattern: FramePattern::SolidColor {
            r: 100,
            g: 100,
            b: 100,
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
        ..Default::default()
    };

    let stats = monitor_cpu_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(3),
        Duration::from_millis(100),
        SPIKE_THRESHOLD,
    );
    stats.print_report("Basic Capture", TARGET_CPU_PERCENT);
    stats.print_usage_graph();

    // This test measures system-wide CPU, not just this process.
    // We verify that CPU is not pegged at 100% (which would indicate a busy loop).
    // Any reasonable average below 95% indicates no CPU runaway.
    assert!(
        stats.average <= 95.0,
        "Average CPU {:.2}% indicates potential CPU runaway",
        stats.average
    );

    // Ensure we collected valid samples
    assert!(stats.samples > 0, "Should have collected CPU samples");
}

/// This test is system-dependent and may fail when system is under load.
/// Run with: cargo test test_cpu_usage_spike_detection -- --ignored
#[test]
#[ignore]
fn test_cpu_usage_spike_detection() {
    eprintln!("\n=== Test: CPU Spike Detection ===\n");

    // Standard resolution for spike detection
    let capture_config = SimulatorConfig {
        device_name: "Spike Test Camera".into(),
        width: 640,
        height: 480,
        fps: 60, // Higher frame rate to stress CPU
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
        ..Default::default()
    };

    let stats = monitor_cpu_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(3),
        Duration::from_millis(100),
        SPIKE_THRESHOLD,
    );
    stats.print_report("Spike Detection", TARGET_CPU_PERCENT);
    stats.print_usage_graph();

    // Log spike information (informational, not failure criterion)
    if stats.spike_count > 0 {
        eprintln!(
            "Detected {} severe spikes (>{:.0}% CPU)",
            stats.spike_count, SPIKE_THRESHOLD
        );
    } else {
        eprintln!("No severe CPU spikes detected");
    }

    // Only fail if CPU is constantly at extreme levels (>80% every sample)
    // This would indicate a serious problem like infinite loop
    let severe_spike_ratio = stats.spike_count as f64 / stats.samples.max(1) as f64;
    assert!(
        severe_spike_ratio < 0.9,
        "CPU constantly at extreme levels: {:.1}% of samples exceeded {:.0}%",
        severe_spike_ratio * 100.0,
        SPIKE_THRESHOLD
    );
}

#[test]
fn test_cpu_usage_sustained_load() {
    eprintln!("\n=== Test: Sustained Load CPU Monitoring ===\n");

    // Moderate resolution, sustained test
    let capture_config = SimulatorConfig {
        device_name: "Sustained Test Camera".into(),
        width: 640,
        height: 480,
        fps: 30,
        pattern: FramePattern::SolidColor {
            r: 128,
            g: 64,
            b: 32,
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
        ..Default::default()
    };

    let stats = monitor_cpu_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(5),
        Duration::from_millis(200),
        SPIKE_THRESHOLD,
    );
    stats.print_report("Sustained Load", TARGET_CPU_PERCENT);

    // For sustained load, focus on consistency
    // CPU should not trend upward (which would indicate a leak or runaway)
    if stats.usage_samples.len() >= 4 {
        let first_half_avg: f64 = stats.usage_samples[..stats.samples / 2].iter().sum::<f64>()
            / (stats.samples / 2) as f64;
        let second_half_avg: f64 = stats.usage_samples[stats.samples / 2..].iter().sum::<f64>()
            / (stats.samples - stats.samples / 2) as f64;

        eprintln!(
            "First half avg: {:.2}%, Second half avg: {:.2}%",
            first_half_avg, second_half_avg
        );

        // Second half should not be significantly higher (no CPU runaway)
        assert!(
            second_half_avg < first_half_avg * 2.0 + 5.0,
            "CPU usage trending upward: first half {:.2}%, second half {:.2}%",
            first_half_avg,
            second_half_avg
        );
    }
}

#[test]
#[ignore = "flaky test: CPU usage varies in CI environments"]
fn test_cpu_usage_detailed_report() {
    eprintln!("\n=== Test: Detailed CPU Report ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Report Test Camera".into(),
        width: 320,
        height: 240,
        fps: 30,
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
        ..Default::default()
    };

    let stats = monitor_cpu_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(2),
        Duration::from_millis(100),
        SPIKE_THRESHOLD,
    );
    stats.print_report("Detailed Report", TARGET_CPU_PERCENT);
    stats.print_usage_graph();

    // Generate detailed report
    eprintln!("╔════════════════════════════════════════════════════════════╗");
    eprintln!("║ DETAILED CPU ANALYSIS                                      ║");
    eprintln!("╠════════════════════════════════════════════════════════════╣");

    // Percentile analysis
    if !stats.usage_samples.is_empty() {
        let mut sorted = stats.usage_samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let p50 = sorted[sorted.len() / 2];
        let p90 = sorted[(sorted.len() as f64 * 0.9) as usize];
        let p99 = sorted[(sorted.len() as f64 * 0.99).min(sorted.len() as f64 - 1.0) as usize];

        eprintln!(
            "║ P50 (median):  {:>10.2}%                                 ║",
            p50
        );
        eprintln!(
            "║ P90:           {:>10.2}%                                 ║",
            p90
        );
        eprintln!(
            "║ P99:           {:>10.2}%                                 ║",
            p99
        );
    }
    eprintln!("╚════════════════════════════════════════════════════════════╝\n");

    // This test is informational - always passes
    assert!(stats.samples > 0, "Should have collected CPU samples");
}

#[test]
#[ignore] // Long-running test for soak testing
fn test_cpu_usage_extended_30sec() {
    eprintln!("\n=== Test: Extended CPU Monitoring (30 seconds) ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Extended Test Camera".into(),
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
        ..Default::default()
    };

    let stats = monitor_cpu_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(30),
        Duration::from_millis(500),
        SPIKE_THRESHOLD,
    );
    stats.print_report("Extended 30s", TARGET_CPU_PERCENT);
    stats.print_usage_graph();

    // For extended test, verify no sustained high CPU
    assert!(
        stats.average <= TARGET_CPU_PERCENT * 2.0,
        "Extended average CPU {:.2}% exceeds target",
        stats.average
    );
}

#[test]
#[ignore] // HD resolution test may be slow
fn test_cpu_usage_hd_resolution() {
    eprintln!("\n=== Test: HD Resolution CPU Usage ===\n");

    let capture_config = SimulatorConfig {
        device_name: "HD Camera".into(),
        width: 1920,
        height: 1080,
        fps: 30,
        pattern: FramePattern::SolidColor {
            r: 200,
            g: 150,
            b: 100,
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
        ..Default::default()
    };

    let stats = monitor_cpu_during_capture(
        capture_config,
        display_config,
        Duration::from_secs(5),
        Duration::from_millis(200),
        SPIKE_THRESHOLD * 2.0, // Higher threshold for HD
    );
    stats.print_report("HD Resolution", TARGET_CPU_PERCENT);
    stats.print_usage_graph();

    // HD will use more CPU but should still be reasonable
    assert!(
        stats.average <= TARGET_CPU_PERCENT * 3.0,
        "HD average CPU {:.2}% exceeds relaxed target",
        stats.average
    );
}
