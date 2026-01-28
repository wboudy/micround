//! Soak Test Suite
//!
//! Long-running stability tests that verify the application can run for extended
//! periods without degradation. Critical for validating reliability requirements.
//!
//! # Test Scenarios
//!
//! 1. **Continuous Operation**: Run capture/render pipeline for extended period
//! 2. **Camera Reconnection**: Simulate camera disconnects and verify recovery
//! 3. **Display Changes**: Simulate resolution changes and monitor adaptation
//!
//! # Running Soak Tests
//!
//! Soak tests are marked `#[ignore]` since they take significant time.
//!
//! Run all soak tests:
//! ```bash
//! cargo test --features test-simulator --test soak_test -- --ignored --nocapture
//! ```
//!
//! Run specific scenario:
//! ```bash
//! cargo test --features test-simulator --test soak_test soak_1_hour -- --ignored --nocapture
//! ```

#[allow(dead_code)]
mod common;

use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use micround::capture::simulator::{FramePattern, InjectedErrorType, SimulatorBackend, SimulatorConfig};
use micround::capture::CaptureBackend;
use micround::core::DisplayId;
use micround::process::ProcessedFrame;
use micround::render::simulator::{DisplaySimulator, DisplaySimulatorConfig};
use micround::render::WallpaperRenderer;

// ============================================================================
// Metrics Collection
// ============================================================================

/// A single metrics sample
#[derive(Debug, Clone)]
struct MetricsSample {
    /// Time since test start
    pub elapsed: Duration,
    /// Resident set size in KB (Linux only)
    pub rss_kb: u64,
    /// Frames rendered since last sample
    pub frames_since_last: u64,
    /// Errors since last sample
    pub errors_since_last: u64,
    /// Is camera connected
    pub camera_connected: bool,
}

/// Soak test metrics collector
struct MetricsCollector {
    samples: Arc<Mutex<VecDeque<MetricsSample>>>,
    sample_interval: Duration,
    max_samples: usize,
    running: Arc<AtomicBool>,
    frame_counter: Arc<AtomicU64>,
    error_counter: Arc<AtomicU64>,
    camera_connected: Arc<AtomicBool>,
    test_start: Instant,
}

impl MetricsCollector {
    fn new(sample_interval: Duration, max_samples: usize) -> Self {
        Self {
            samples: Arc::new(Mutex::new(VecDeque::new())),
            sample_interval,
            max_samples,
            running: Arc::new(AtomicBool::new(false)),
            frame_counter: Arc::new(AtomicU64::new(0)),
            error_counter: Arc::new(AtomicU64::new(0)),
            camera_connected: Arc::new(AtomicBool::new(true)),
            test_start: Instant::now(),
        }
    }

    fn start_collection(&self) -> thread::JoinHandle<()> {
        let samples = Arc::clone(&self.samples);
        let running = Arc::clone(&self.running);
        let frame_counter = Arc::clone(&self.frame_counter);
        let error_counter = Arc::clone(&self.error_counter);
        let camera_connected = Arc::clone(&self.camera_connected);
        let interval = self.sample_interval;
        let max_samples = self.max_samples;
        let test_start = self.test_start;

        running.store(true, Ordering::SeqCst);

        thread::spawn(move || {
            let mut last_frames = 0u64;
            let mut last_errors = 0u64;

            while running.load(Ordering::SeqCst) {
                thread::sleep(interval);

                let current_frames = frame_counter.load(Ordering::SeqCst);
                let current_errors = error_counter.load(Ordering::SeqCst);

                let sample = MetricsSample {
                    elapsed: test_start.elapsed(),
                    rss_kb: read_rss_kb(),
                    frames_since_last: current_frames.saturating_sub(last_frames),
                    errors_since_last: current_errors.saturating_sub(last_errors),
                    camera_connected: camera_connected.load(Ordering::SeqCst),
                };

                last_frames = current_frames;
                last_errors = current_errors;

                if let Ok(mut samples_guard) = samples.lock() {
                    samples_guard.push_back(sample);
                    while samples_guard.len() > max_samples {
                        samples_guard.pop_front();
                    }
                }
            }
        })
    }

    fn stop_collection(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn get_samples(&self) -> Vec<MetricsSample> {
        self.samples
            .lock()
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn increment_frames(&self) {
        self.frame_counter.fetch_add(1, Ordering::SeqCst);
    }

    fn increment_errors(&self) {
        self.error_counter.fetch_add(1, Ordering::SeqCst);
    }

    fn set_camera_connected(&self, connected: bool) {
        self.camera_connected.store(connected, Ordering::SeqCst);
    }

    fn total_frames(&self) -> u64 {
        self.frame_counter.load(Ordering::SeqCst)
    }

    fn total_errors(&self) -> u64 {
        self.error_counter.load(Ordering::SeqCst)
    }
}

/// Read process RSS in KB (Linux only)
fn read_rss_kb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = fs::read_to_string("/proc/self/statm") {
            let parts: Vec<&str> = statm.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(pages) = parts[1].parse::<u64>() {
                    return pages * 4; // 4KB pages
                }
            }
        }
    }
    0
}

// ============================================================================
// Soak Test Report
// ============================================================================

/// Soak test result summary
struct SoakTestReport {
    test_name: String,
    scenario: String,
    duration: Duration,
    total_frames: u64,
    total_errors: u64,
    initial_rss_kb: u64,
    final_rss_kb: u64,
    peak_rss_kb: u64,
    memory_growth_percent: f64,
    avg_fps: f64,
    recovery_attempts: u64,
    recovery_successes: u64,
    passed: bool,
    failure_reasons: Vec<String>,
}

impl SoakTestReport {
    fn print(&self) {
        let status = if self.passed { "PASS" } else { "FAIL" };

        eprintln!("\n╔══════════════════════════════════════════════════════════════════════╗");
        eprintln!("║ SOAK TEST REPORT                                                     ║");
        eprintln!("╠══════════════════════════════════════════════════════════════════════╣");
        eprintln!("║ Test:     {:58} ║", self.test_name);
        eprintln!("║ Scenario: {:58} ║", self.scenario);
        eprintln!("║ Status:   [{:^4}]                                                     ║", status);
        eprintln!("╠══════════════════════════════════════════════════════════════════════╣");
        eprintln!("║ Duration: {:>12.2?}                                               ║", self.duration);
        eprintln!("║ Frames:   {:>12}                                                  ║", self.total_frames);
        eprintln!("║ Errors:   {:>12}                                                  ║", self.total_errors);
        eprintln!("║ Avg FPS:  {:>12.1}                                                  ║", self.avg_fps);
        eprintln!("╠══════════════════════════════════════════════════════════════════════╣");
        eprintln!("║ MEMORY                                                               ║");
        eprintln!("║   Initial:  {:>10} KB                                             ║", self.initial_rss_kb);
        eprintln!("║   Final:    {:>10} KB                                             ║", self.final_rss_kb);
        eprintln!("║   Peak:     {:>10} KB                                             ║", self.peak_rss_kb);
        eprintln!("║   Growth:   {:>10.1}%                                              ║", self.memory_growth_percent);

        if self.recovery_attempts > 0 {
            eprintln!("╠══════════════════════════════════════════════════════════════════════╣");
            eprintln!("║ RECOVERY                                                             ║");
            eprintln!("║   Attempts:  {:>10}                                               ║", self.recovery_attempts);
            eprintln!("║   Successes: {:>10}                                               ║", self.recovery_successes);
            let rate = (self.recovery_successes as f64 / self.recovery_attempts.max(1) as f64) * 100.0;
            eprintln!("║   Rate:      {:>10.1}%                                              ║", rate);
        }

        if !self.failure_reasons.is_empty() {
            eprintln!("╠══════════════════════════════════════════════════════════════════════╣");
            eprintln!("║ FAILURES                                                             ║");
            for reason in &self.failure_reasons {
                eprintln!("║   - {:64} ║", reason);
            }
        }

        eprintln!("╚══════════════════════════════════════════════════════════════════════╝\n");
    }

    fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let mut file = File::create(path)?;
        writeln!(file, "# Soak Test Report")?;
        writeln!(file, "Test: {}", self.test_name)?;
        writeln!(file, "Scenario: {}", self.scenario)?;
        writeln!(file, "Status: {}", if self.passed { "PASS" } else { "FAIL" })?;
        writeln!(file, "Duration: {:?}", self.duration)?;
        writeln!(file, "Total Frames: {}", self.total_frames)?;
        writeln!(file, "Total Errors: {}", self.total_errors)?;
        writeln!(file, "Avg FPS: {:.1}", self.avg_fps)?;
        writeln!(file, "Initial RSS: {} KB", self.initial_rss_kb)?;
        writeln!(file, "Final RSS: {} KB", self.final_rss_kb)?;
        writeln!(file, "Peak RSS: {} KB", self.peak_rss_kb)?;
        writeln!(file, "Memory Growth: {:.1}%", self.memory_growth_percent)?;
        if !self.failure_reasons.is_empty() {
            writeln!(file, "Failure Reasons:")?;
            for reason in &self.failure_reasons {
                writeln!(file, "  - {}", reason)?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Soak Test Runner
// ============================================================================

/// Simulate frame processing
fn process_frame(frame: &micround::core::Frame) -> ProcessedFrame {
    ProcessedFrame::new(frame.data.clone(), frame.width, frame.height)
}

/// Run continuous capture/render for specified duration
fn run_continuous_test(
    test_name: &str,
    duration: Duration,
    capture_config: SimulatorConfig,
    display_config: DisplaySimulatorConfig,
) -> SoakTestReport {
    eprintln!("\n=== Starting Soak Test: {} ===", test_name);
    eprintln!("Duration: {:?}", duration);

    let metrics = MetricsCollector::new(Duration::from_secs(10), 10000);
    let collector_handle = metrics.start_collection();

    // Initialize capture backend
    let mut capture = SimulatorBackend::new(capture_config.clone());
    let devices = capture.enumerate_devices();

    let settings = micround::core::CaptureSettings {
        width: capture_config.width,
        height: capture_config.height,
        framerate: capture_config.fps as f32,
        format: Some(micround::core::PixelFormat::Rgba32),
    };

    capture.open(&devices[0].id, settings).expect("Failed to open capture device");

    // Initialize display simulator
    let mut display = DisplaySimulator::new(display_config);
    display.init(&DisplayId("soak-test".into())).expect("Failed to init display");

    capture.start().expect("Failed to start capture");

    // Take initial memory sample
    let initial_rss_kb = read_rss_kb();

    let test_start = Instant::now();
    let mut peak_rss_kb = initial_rss_kb;

    // Run the capture/render loop
    while test_start.elapsed() < duration {
        match capture.next_frame() {
            Ok(frame) => {
                let processed = process_frame(&frame);
                match display.render(&processed) {
                    Ok(_) => metrics.increment_frames(),
                    Err(_) => metrics.increment_errors(),
                }
            }
            Err(_) => metrics.increment_errors(),
        }

        // Periodically check memory
        if test_start.elapsed().as_secs() % 60 == 0 {
            let current_rss = read_rss_kb();
            if current_rss > peak_rss_kb {
                peak_rss_kb = current_rss;
            }
        }
    }

    // Stop collection and cleanup
    metrics.stop_collection();
    let _ = collector_handle.join();

    capture.stop().ok();
    capture.close();
    display.shutdown();

    // Calculate results
    let final_rss_kb = read_rss_kb();
    let total_frames = metrics.total_frames();
    let total_errors = metrics.total_errors();
    let elapsed = test_start.elapsed();
    let avg_fps = total_frames as f64 / elapsed.as_secs_f64();

    let memory_growth_percent = if initial_rss_kb > 0 {
        ((final_rss_kb as f64 - initial_rss_kb as f64) / initial_rss_kb as f64) * 100.0
    } else {
        0.0
    };

    // Determine pass/fail
    let mut failure_reasons = Vec::new();

    // Check memory growth (<10%)
    if memory_growth_percent > 10.0 {
        failure_reasons.push(format!("Memory growth {:.1}% exceeds 10% threshold", memory_growth_percent));
    }

    // Check for excessive errors (<1% of frames)
    let error_rate = total_errors as f64 / total_frames.max(1) as f64;
    if error_rate > 0.01 {
        failure_reasons.push(format!("Error rate {:.2}% exceeds 1% threshold", error_rate * 100.0));
    }

    // Check frame rate (should be reasonable)
    if avg_fps < 10.0 {
        failure_reasons.push(format!("Avg FPS {:.1} is too low (expected >10)", avg_fps));
    }

    let passed = failure_reasons.is_empty();

    SoakTestReport {
        test_name: test_name.to_string(),
        scenario: "Continuous Operation".to_string(),
        duration: elapsed,
        total_frames,
        total_errors,
        initial_rss_kb,
        final_rss_kb,
        peak_rss_kb,
        memory_growth_percent,
        avg_fps,
        recovery_attempts: 0,
        recovery_successes: 0,
        passed,
        failure_reasons,
    }
}

/// Run camera reconnection test
fn run_reconnection_test(
    test_name: &str,
    duration: Duration,
    reconnect_interval: Duration,
    capture_config: SimulatorConfig,
    display_config: DisplaySimulatorConfig,
) -> SoakTestReport {
    eprintln!("\n=== Starting Reconnection Test: {} ===", test_name);
    eprintln!("Duration: {:?}, Reconnect interval: {:?}", duration, reconnect_interval);

    let metrics = MetricsCollector::new(Duration::from_secs(10), 10000);
    let collector_handle = metrics.start_collection();

    // Initialize display simulator (keeps running throughout)
    let mut display = DisplaySimulator::new(display_config.clone());
    display.init(&DisplayId("soak-test".into())).expect("Failed to init display");

    let initial_rss_kb = read_rss_kb();
    let test_start = Instant::now();
    let mut peak_rss_kb = initial_rss_kb;
    let mut last_reconnect = Instant::now();
    let mut recovery_attempts = 0u64;
    let mut recovery_successes = 0u64;
    let mut capture: Option<SimulatorBackend> = None;

    // Initial connection
    let connect_camera = |config: &SimulatorConfig| -> Option<SimulatorBackend> {
        let mut cap = SimulatorBackend::new(config.clone());
        let devices = cap.enumerate_devices();
        if devices.is_empty() {
            return None;
        }
        let settings = micround::core::CaptureSettings {
            width: config.width,
            height: config.height,
            framerate: config.fps as f32,
            format: Some(micround::core::PixelFormat::Rgba32),
        };
        cap.open(&devices[0].id, settings).ok()?;
        cap.start().ok()?;
        Some(cap)
    };

    capture = connect_camera(&capture_config);
    metrics.set_camera_connected(capture.is_some());

    // Run the test loop
    while test_start.elapsed() < duration {
        // Check if it's time to reconnect
        if last_reconnect.elapsed() >= reconnect_interval {
            eprintln!("[{:?}] Disconnecting camera...", test_start.elapsed());
            metrics.set_camera_connected(false);

            // Disconnect
            if let Some(mut cap) = capture.take() {
                cap.stop().ok();
                cap.close();
            }

            // Wait briefly
            thread::sleep(Duration::from_secs(2));

            // Reconnect
            recovery_attempts += 1;
            eprintln!("[{:?}] Reconnecting camera...", test_start.elapsed());
            capture = connect_camera(&capture_config);

            if capture.is_some() {
                recovery_successes += 1;
                eprintln!("[{:?}] Camera reconnected successfully", test_start.elapsed());
            } else {
                eprintln!("[{:?}] Camera reconnection FAILED", test_start.elapsed());
            }

            metrics.set_camera_connected(capture.is_some());
            last_reconnect = Instant::now();
        }

        // Process frames if connected
        if let Some(ref mut cap) = capture {
            match cap.next_frame() {
                Ok(frame) => {
                    let processed = process_frame(&frame);
                    match display.render(&processed) {
                        Ok(_) => metrics.increment_frames(),
                        Err(_) => metrics.increment_errors(),
                    }
                }
                Err(_) => metrics.increment_errors(),
            }
        } else {
            // Camera disconnected, wait briefly
            thread::sleep(Duration::from_millis(100));
        }

        // Periodically check memory
        let current_rss = read_rss_kb();
        if current_rss > peak_rss_kb {
            peak_rss_kb = current_rss;
        }
    }

    // Cleanup
    metrics.stop_collection();
    let _ = collector_handle.join();

    if let Some(mut cap) = capture {
        cap.stop().ok();
        cap.close();
    }
    display.shutdown();

    // Calculate results
    let final_rss_kb = read_rss_kb();
    let total_frames = metrics.total_frames();
    let total_errors = metrics.total_errors();
    let elapsed = test_start.elapsed();
    let avg_fps = total_frames as f64 / elapsed.as_secs_f64();

    let memory_growth_percent = if initial_rss_kb > 0 {
        ((final_rss_kb as f64 - initial_rss_kb as f64) / initial_rss_kb as f64) * 100.0
    } else {
        0.0
    };

    // Determine pass/fail
    let mut failure_reasons = Vec::new();

    // Check recovery rate (100% required)
    if recovery_attempts > 0 && recovery_successes < recovery_attempts {
        failure_reasons.push(format!(
            "Recovery rate {}/{} ({:.1}%) - should be 100%",
            recovery_successes,
            recovery_attempts,
            (recovery_successes as f64 / recovery_attempts as f64) * 100.0
        ));
    }

    // Check memory growth
    if memory_growth_percent > 15.0 {
        failure_reasons.push(format!("Memory growth {:.1}% exceeds 15% threshold", memory_growth_percent));
    }

    let passed = failure_reasons.is_empty();

    SoakTestReport {
        test_name: test_name.to_string(),
        scenario: "Camera Reconnection".to_string(),
        duration: elapsed,
        total_frames,
        total_errors,
        initial_rss_kb,
        final_rss_kb,
        peak_rss_kb,
        memory_growth_percent,
        avg_fps,
        recovery_attempts,
        recovery_successes,
        passed,
        failure_reasons,
    }
}

// ============================================================================
// Tests - Short versions for CI
// ============================================================================

#[test]
fn test_soak_infrastructure() {
    eprintln!("\n=== Test: Soak Test Infrastructure ===\n");

    // Quick test to verify the soak test infrastructure works
    let capture_config = SimulatorConfig {
        device_name: "Soak Camera".into(),
        width: 320,
        height: 240,
        fps: 60,
        pattern: FramePattern::SolidColor { r: 100, g: 100, b: 100 },
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

    let report = run_continuous_test(
        "Infrastructure Test",
        Duration::from_secs(5),
        capture_config,
        display_config,
    );
    report.print();

    assert!(report.total_frames > 0, "Should have rendered frames");
    assert!(report.avg_fps > 1.0, "Should have reasonable FPS");
}

#[test]
fn test_soak_quick_reconnection() {
    eprintln!("\n=== Test: Quick Reconnection Test ===\n");

    let capture_config = SimulatorConfig {
        device_name: "Reconnect Camera".into(),
        width: 320,
        height: 240,
        fps: 60,
        pattern: FramePattern::SolidColor { r: 128, g: 128, b: 128 },
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

    let report = run_reconnection_test(
        "Quick Reconnection",
        Duration::from_secs(15),
        Duration::from_secs(5), // Reconnect every 5 seconds
        capture_config,
        display_config,
    );
    report.print();

    // Should have at least 2 reconnection attempts
    assert!(report.recovery_attempts >= 2, "Should have attempted reconnection");
    // 100% recovery expected
    assert_eq!(
        report.recovery_successes, report.recovery_attempts,
        "All reconnections should succeed"
    );
}

// ============================================================================
// Tests - Full soak tests (ignored by default)
// ============================================================================

#[test]
#[ignore] // 1-hour soak test
fn test_soak_1_hour() {
    eprintln!("\n=== SOAK TEST: 1 Hour Continuous ===\n");

    let capture_config = SimulatorConfig {
        device_name: "1H Soak Camera".into(),
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
        ..Default::default()
    };

    let report = run_continuous_test(
        "1-Hour Continuous",
        Duration::from_secs(3600),
        capture_config,
        display_config,
    );
    report.print();

    // Save report to file
    let _ = report.save_to_file("soak_1h_report.txt");

    assert!(report.passed, "1-hour soak test failed: {:?}", report.failure_reasons);
}

#[test]
#[ignore] // 8-hour reconnection soak test
fn test_soak_8_hour_reconnection() {
    eprintln!("\n=== SOAK TEST: 8 Hour Camera Reconnection ===\n");

    let capture_config = SimulatorConfig {
        device_name: "8H Reconnect Camera".into(),
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

    let report = run_reconnection_test(
        "8-Hour Reconnection",
        Duration::from_secs(8 * 3600), // 8 hours
        Duration::from_secs(3600),      // Reconnect every hour
        capture_config,
        display_config,
    );
    report.print();

    // Save report
    let _ = report.save_to_file("soak_8h_reconnect_report.txt");

    assert!(report.passed, "8-hour reconnection test failed: {:?}", report.failure_reasons);
    assert_eq!(
        report.recovery_successes, report.recovery_attempts,
        "100% recovery rate required"
    );
}

#[test]
#[ignore] // 24-hour continuous soak test
fn test_soak_24_hour() {
    eprintln!("\n=== SOAK TEST: 24 Hour Continuous ===\n");

    let capture_config = SimulatorConfig {
        device_name: "24H Soak Camera".into(),
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
        ..Default::default()
    };

    let report = run_continuous_test(
        "24-Hour Continuous",
        Duration::from_secs(24 * 3600),
        capture_config,
        display_config,
    );
    report.print();

    // Save report
    let _ = report.save_to_file("soak_24h_report.txt");

    assert!(report.passed, "24-hour soak test failed: {:?}", report.failure_reasons);
}

#[test]
#[ignore] // 10-minute stress test with errors
fn test_soak_10min_with_errors() {
    eprintln!("\n=== SOAK TEST: 10 Min With Error Injection ===\n");

    // Configure some errors to verify recovery
    let capture_config = SimulatorConfig {
        device_name: "Error Soak Camera".into(),
        width: 640,
        height: 480,
        fps: 30,
        pattern: FramePattern::SolidColor { r: 64, g: 128, b: 192 },
        drop_rate: 0.01, // 1% frame drops
        latency_ms: 0,
        error_rate: 0.001, // 0.1% errors
        error_type: InjectedErrorType::Timeout,
        ..Default::default()
    };

    let display_config = DisplaySimulatorConfig {
        width: 640,
        height: 480,
        latency_ms: 0,
        error_rate: 0.0,
        ..Default::default()
    };

    let report = run_continuous_test(
        "10-Min Error Injection",
        Duration::from_secs(600),
        capture_config,
        display_config,
    );
    report.print();

    // This test allows some errors but still should render mostly successfully
    assert!(report.total_frames > 1000, "Should have rendered many frames despite errors");
}
