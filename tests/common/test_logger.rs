//! Test logging framework with step tracking
//!
//! Provides detailed, structured logging for test execution to aid debugging
//! test failures and track performance.
//!
//! # Features
//!
//! - Step-by-step progress logging
//! - Assertion tracking with pass/fail indicators
//! - Timing measurements per step and total
//! - Multiple output formats (Console, JSON, Tracing)
//! - Environment variable configuration
//!
//! # Example Output
//!
//! ```text
//! [TEST] test_frame_conversion_mjpeg_to_rgba
//! [STEP 1/5] Loading fixture: frames/mjpeg_640x480.bin (153,442 bytes)
//! [STEP 2/5] Creating decoder with format=MJPEG
//! [STEP 3/5] Decoding frame... OK (12.3ms)
//! [STEP 4/5] Converting to RGBA... OK (2.1ms)
//! [STEP 5/5] Comparing with expected output
//! [ASSERT] Frame dimensions match: 640x480 == 640x480 ✓
//! [ASSERT] PSNR: 45.2 dB (threshold: 40.0 dB) ✓
//! [TIMING] Total: 14.8ms
//! [RESULT] PASS
//! ```
//!
//! # Usage
//!
//! ```ignore
//! let mut logger = TestLogger::new("test_decode_frame", 3);
//!
//! logger.step("Loading fixture");
//! let frame = load_fixture();
//! logger.step_ok(&format!("Loaded {} bytes", frame.len()));
//!
//! logger.step("Decoding");
//! let result = decode(&frame);
//! logger.step_ok("Decoded successfully");
//!
//! logger.step("Validating output");
//! logger.assert_pass("Dimensions match");
//!
//! let result = logger.finish();
//! assert!(result.passed);
//! ```

use std::env;
use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// ============================================================================
// Configuration
// ============================================================================

/// Environment variable for log level
pub const ENV_LOG_LEVEL: &str = "MICROUND_TEST_LOG_LEVEL";

/// Environment variable for JSON output file
pub const ENV_LOG_FILE: &str = "MICROUND_TEST_LOG_FILE";

/// Environment variable to enable/disable timing
pub const ENV_LOG_TIMING: &str = "MICROUND_TEST_LOG_TIMING";

/// Environment variable to enable/disable color output
pub const ENV_LOG_COLOR: &str = "MICROUND_TEST_LOG_COLOR";

/// Global flag for whether colors are enabled
static COLORS_ENABLED: AtomicBool = AtomicBool::new(true);

/// Log level for test output
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Parse from environment variable
    pub fn from_env() -> Self {
        env::var(ENV_LOG_LEVEL)
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "debug" => Some(LogLevel::Debug),
                "info" => Some(LogLevel::Info),
                "warn" | "warning" => Some(LogLevel::Warn),
                "error" => Some(LogLevel::Error),
                _ => None,
            })
            .unwrap_or(LogLevel::Info)
    }
}

/// Check if timing output is enabled
pub fn timing_enabled() -> bool {
    env::var(ENV_LOG_TIMING)
        .map(|s| !matches!(s.to_lowercase().as_str(), "false" | "0" | "no" | "off"))
        .unwrap_or(true)
}

/// Check if colors are enabled
pub fn colors_enabled() -> bool {
    // Check env var first
    if let Ok(val) = env::var(ENV_LOG_COLOR) {
        return !matches!(val.to_lowercase().as_str(), "false" | "0" | "no" | "off");
    }

    // Check global flag
    COLORS_ENABLED.load(Ordering::Relaxed)
}

/// Enable or disable colors globally
pub fn set_colors_enabled(enabled: bool) {
    COLORS_ENABLED.store(enabled, Ordering::Relaxed);
}

// ============================================================================
// ANSI Colors
// ============================================================================

mod colors {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";

    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";

    pub const BG_GREEN: &str = "\x1b[42m";
    pub const BG_RED: &str = "\x1b[41m";
}

/// Apply color if colors are enabled
fn color(text: &str, color_code: &str) -> String {
    if colors_enabled() {
        format!("{}{}{}", color_code, text, colors::RESET)
    } else {
        text.to_string()
    }
}

// ============================================================================
// Step Status
// ============================================================================

/// Status of a test step
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    /// Step in progress
    InProgress,
    /// Step completed successfully
    Ok(String),
    /// Step completed with error
    Error(String),
    /// Step skipped
    Skipped(String),
}

impl StepStatus {
    /// Check if step succeeded
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }

    /// Check if step failed
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// Record of a completed step
#[derive(Debug, Clone)]
pub struct StepRecord {
    pub number: usize,
    pub description: String,
    pub status: StepStatus,
    pub duration: Duration,
}

// ============================================================================
// Assertion Record
// ============================================================================

/// Record of an assertion
#[derive(Debug, Clone)]
pub struct AssertionRecord {
    pub description: String,
    pub passed: bool,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

// ============================================================================
// Test Result
// ============================================================================

/// Final result of a test
#[derive(Debug, Clone)]
pub struct TestResult {
    pub test_name: String,
    pub passed: bool,
    pub total_duration: Duration,
    pub steps: Vec<StepRecord>,
    pub assertions: Vec<AssertionRecord>,
    pub failure_reason: Option<String>,
}

impl TestResult {
    /// Check if all steps passed
    pub fn all_steps_ok(&self) -> bool {
        self.steps.iter().all(|s| s.status.is_ok() || matches!(s.status, StepStatus::Skipped(_)))
    }

    /// Check if all assertions passed
    pub fn all_assertions_passed(&self) -> bool {
        self.assertions.iter().all(|a| a.passed)
    }

    /// Get count of passed assertions
    pub fn passed_assertion_count(&self) -> usize {
        self.assertions.iter().filter(|a| a.passed).count()
    }

    /// Get count of failed assertions
    pub fn failed_assertion_count(&self) -> usize {
        self.assertions.iter().filter(|a| !a.passed).count()
    }

    /// Export to JSON for CI parsing
    pub fn to_json(&self) -> String {
        let steps: Vec<serde_json::Value> = self.steps.iter().map(|s| {
            serde_json::json!({
                "number": s.number,
                "description": s.description,
                "status": match &s.status {
                    StepStatus::InProgress => "in_progress",
                    StepStatus::Ok(_) => "ok",
                    StepStatus::Error(_) => "error",
                    StepStatus::Skipped(_) => "skipped",
                },
                "details": match &s.status {
                    StepStatus::Ok(d) | StepStatus::Error(d) | StepStatus::Skipped(d) => d.clone(),
                    StepStatus::InProgress => String::new(),
                },
                "duration_ms": s.duration.as_millis(),
            })
        }).collect();

        let assertions: Vec<serde_json::Value> = self.assertions.iter().map(|a| {
            serde_json::json!({
                "description": a.description,
                "passed": a.passed,
                "expected": a.expected,
                "actual": a.actual,
            })
        }).collect();

        serde_json::json!({
            "test_name": self.test_name,
            "passed": self.passed,
            "total_duration_ms": self.total_duration.as_millis(),
            "steps": steps,
            "assertions": assertions,
            "failure_reason": self.failure_reason,
        }).to_string()
    }
}

// ============================================================================
// Test Logger
// ============================================================================

/// Test logger with step tracking and structured output
pub struct TestLogger {
    test_name: String,
    total_steps: usize,
    current_step: usize,
    start_time: Instant,
    step_start: Option<Instant>,
    steps: Vec<StepRecord>,
    assertions: Vec<AssertionRecord>,
    log_level: LogLevel,
    output_json: bool,
}

impl TestLogger {
    /// Create a new test logger
    ///
    /// # Arguments
    /// - `test_name`: Name of the test (shown in output header)
    /// - `total_steps`: Total number of expected steps (for progress display)
    pub fn new(test_name: &str, total_steps: usize) -> Self {
        let logger = Self {
            test_name: test_name.to_string(),
            total_steps,
            current_step: 0,
            start_time: Instant::now(),
            step_start: None,
            steps: Vec::with_capacity(total_steps),
            assertions: Vec::new(),
            log_level: LogLevel::from_env(),
            output_json: env::var(ENV_LOG_FILE).is_ok(),
        };

        logger.print_header();
        logger
    }

    /// Create a logger for a test with unknown step count
    pub fn new_dynamic(test_name: &str) -> Self {
        Self::new(test_name, 0)
    }

    fn print_header(&self) {
        let header = format!("[TEST] {}", self.test_name);
        eprintln!("{}", color(&header, colors::BOLD));
    }

    /// Start a new step
    pub fn step(&mut self, description: &str) {
        // Complete previous step if any
        if self.step_start.is_some() && !self.steps.is_empty() {
            let last = self.steps.last_mut().unwrap();
            if matches!(last.status, StepStatus::InProgress) {
                let duration = self.step_start.unwrap().elapsed();
                last.duration = duration;
                last.status = StepStatus::Ok(String::new());
            }
        }

        self.current_step += 1;
        self.step_start = Some(Instant::now());

        let step_str = if self.total_steps > 0 {
            format!("[STEP {}/{}]", self.current_step, self.total_steps)
        } else {
            format!("[STEP {}]", self.current_step)
        };

        if self.log_level <= LogLevel::Info {
            eprintln!(
                "{} {}",
                color(&step_str, colors::CYAN),
                description
            );
        }

        self.steps.push(StepRecord {
            number: self.current_step,
            description: description.to_string(),
            status: StepStatus::InProgress,
            duration: Duration::ZERO,
        });
    }

    /// Mark current step as successful
    pub fn step_ok(&mut self, details: &str) {
        let duration = self.step_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);

        if let Some(step) = self.steps.last_mut() {
            step.status = StepStatus::Ok(details.to_string());
            step.duration = duration;
        }

        if self.log_level <= LogLevel::Info && timing_enabled() {
            let time_str = format!("({:.1}ms)", duration.as_secs_f64() * 1000.0);
            eprintln!(
                "       {} {} {}",
                color("OK", colors::GREEN),
                details,
                color(&time_str, colors::DIM)
            );
        }
    }

    /// Mark current step as failed
    pub fn step_err(&mut self, error: &str) {
        let duration = self.step_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);

        if let Some(step) = self.steps.last_mut() {
            step.status = StepStatus::Error(error.to_string());
            step.duration = duration;
        }

        eprintln!(
            "       {} {}",
            color("ERROR", colors::RED),
            error
        );
    }

    /// Skip current step
    pub fn step_skip(&mut self, reason: &str) {
        let duration = self.step_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);

        if let Some(step) = self.steps.last_mut() {
            step.status = StepStatus::Skipped(reason.to_string());
            step.duration = duration;
        }

        if self.log_level <= LogLevel::Info {
            eprintln!(
                "       {} {}",
                color("SKIPPED", colors::YELLOW),
                reason
            );
        }
    }

    /// Record a passing assertion
    pub fn assert_pass(&mut self, description: &str) {
        self.assertions.push(AssertionRecord {
            description: description.to_string(),
            passed: true,
            expected: None,
            actual: None,
        });

        if self.log_level <= LogLevel::Info {
            eprintln!(
                "{} {} {}",
                color("[ASSERT]", colors::MAGENTA),
                description,
                color("✓", colors::GREEN)
            );
        }
    }

    /// Record a failing assertion
    pub fn assert_fail(&mut self, description: &str, expected: &str, actual: &str) {
        self.assertions.push(AssertionRecord {
            description: description.to_string(),
            passed: false,
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        });

        eprintln!(
            "{} {} {}",
            color("[ASSERT]", colors::MAGENTA),
            description,
            color("✗", colors::RED)
        );
        eprintln!(
            "         {} {}",
            color("Expected:", colors::DIM),
            expected
        );
        eprintln!(
            "         {} {}",
            color("Actual:", colors::DIM),
            actual
        );
    }

    /// Record a conditional assertion
    pub fn assert_eq<T: PartialEq + fmt::Debug>(&mut self, description: &str, expected: &T, actual: &T) {
        if expected == actual {
            self.assert_pass(description);
        } else {
            self.assert_fail(
                description,
                &format!("{:?}", expected),
                &format!("{:?}", actual),
            );
        }
    }

    /// Log a timing measurement
    pub fn timing(&self, name: &str, duration: Duration) {
        if timing_enabled() && self.log_level <= LogLevel::Info {
            eprintln!(
                "{} {}: {:.1}ms",
                color("[TIMING]", colors::BLUE),
                name,
                duration.as_secs_f64() * 1000.0
            );
        }
    }

    /// Log a debug message
    pub fn debug(&self, message: &str) {
        if self.log_level <= LogLevel::Debug {
            eprintln!(
                "{} {}",
                color("[DEBUG]", colors::DIM),
                message
            );
        }
    }

    /// Log an info message
    pub fn info(&self, message: &str) {
        if self.log_level <= LogLevel::Info {
            eprintln!(
                "{} {}",
                color("[INFO]", colors::WHITE),
                message
            );
        }
    }

    /// Log a warning message
    pub fn warn(&self, message: &str) {
        if self.log_level <= LogLevel::Warn {
            eprintln!(
                "{} {}",
                color("[WARN]", colors::YELLOW),
                message
            );
        }
    }

    /// Log an error message
    pub fn error(&self, message: &str) {
        eprintln!(
            "{} {}",
            color("[ERROR]", colors::RED),
            message
        );
    }

    /// Finish the test and produce a result
    pub fn finish(mut self) -> TestResult {
        // Complete any in-progress step
        if let Some(step) = self.steps.last_mut() {
            if matches!(step.status, StepStatus::InProgress) {
                let duration = self.step_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO);
                step.duration = duration;
                step.status = StepStatus::Ok(String::new());
            }
        }

        let total_duration = self.start_time.elapsed();

        // Determine pass/fail
        let all_steps_ok = self.steps.iter().all(|s| !s.status.is_error());
        let all_assertions_passed = self.assertions.iter().all(|a| a.passed);
        let passed = all_steps_ok && all_assertions_passed;

        let failure_reason = if !passed {
            if !all_steps_ok {
                self.steps
                    .iter()
                    .find(|s| s.status.is_error())
                    .map(|s| format!("Step {} failed: {}", s.number, s.description))
            } else {
                self.assertions
                    .iter()
                    .find(|a| !a.passed)
                    .map(|a| format!("Assertion failed: {}", a.description))
            }
        } else {
            None
        };

        // Print result
        if timing_enabled() {
            eprintln!(
                "{} Total: {:.1}ms",
                color("[TIMING]", colors::BLUE),
                total_duration.as_secs_f64() * 1000.0
            );
        }

        let result_str = if passed {
            color("[RESULT] PASS", &format!("{}{}", colors::BG_GREEN, colors::BOLD))
        } else {
            color("[RESULT] FAIL", &format!("{}{}", colors::BG_RED, colors::BOLD))
        };
        eprintln!("{}", result_str);
        eprintln!();

        let result = TestResult {
            test_name: self.test_name,
            passed,
            total_duration,
            steps: self.steps,
            assertions: self.assertions,
            failure_reason,
        };

        // Write JSON if configured
        if self.output_json {
            if let Ok(path) = env::var(ENV_LOG_FILE) {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                {
                    let _ = writeln!(file, "{}", result.to_json());
                }
            }
        }

        result
    }

    /// Finish with explicit failure
    pub fn finish_failed(mut self, reason: &str) -> TestResult {
        self.error(reason);
        let total_duration = self.start_time.elapsed();

        eprintln!(
            "{}",
            color("[RESULT] FAIL", &format!("{}{}", colors::BG_RED, colors::BOLD))
        );
        eprintln!();

        TestResult {
            test_name: self.test_name,
            passed: false,
            total_duration,
            steps: self.steps,
            assertions: self.assertions,
            failure_reason: Some(reason.to_string()),
        }
    }
}

// ============================================================================
// Macros
// ============================================================================

/// Start a new test step
///
/// # Example
/// ```ignore
/// test_step!(logger, "Loading fixture");
/// test_step!(logger, "Processing {} items", count);
/// ```
#[macro_export]
macro_rules! test_step {
    ($logger:expr, $desc:literal) => {
        $logger.step($desc)
    };
    ($logger:expr, $desc:literal, $($arg:tt)*) => {
        $logger.step(&format!($desc, $($arg)*))
    };
}

/// Mark current step as successful
///
/// # Example
/// ```ignore
/// test_step_ok!(logger, "Loaded {} bytes", size);
/// ```
#[macro_export]
macro_rules! test_step_ok {
    ($logger:expr) => {
        $logger.step_ok("")
    };
    ($logger:expr, $details:literal) => {
        $logger.step_ok($details)
    };
    ($logger:expr, $details:literal, $($arg:tt)*) => {
        $logger.step_ok(&format!($details, $($arg)*))
    };
}

/// Mark current step as failed
///
/// # Example
/// ```ignore
/// test_step_err!(logger, "Decode failed: {}", error);
/// ```
#[macro_export]
macro_rules! test_step_err {
    ($logger:expr, $error:literal) => {
        $logger.step_err($error)
    };
    ($logger:expr, $error:literal, $($arg:tt)*) => {
        $logger.step_err(&format!($error, $($arg)*))
    };
}

/// Record an assertion
///
/// # Example
/// ```ignore
/// test_assert!(logger, width == 640, "Width matches");
/// test_assert!(logger, result.is_ok(), "Operation succeeded");
/// ```
#[macro_export]
macro_rules! test_assert {
    ($logger:expr, $cond:expr, $desc:literal) => {
        if $cond {
            $logger.assert_pass($desc);
        } else {
            $logger.assert_fail($desc, "true", "false");
        }
    };
    ($logger:expr, $cond:expr, $desc:literal, $($arg:tt)*) => {
        let desc = format!($desc, $($arg)*);
        if $cond {
            $logger.assert_pass(&desc);
        } else {
            $logger.assert_fail(&desc, "true", "false");
        }
    };
}

/// Assert equality with logging
///
/// # Example
/// ```ignore
/// test_assert_eq!(logger, actual, expected, "Values match");
/// ```
#[macro_export]
macro_rules! test_assert_eq {
    ($logger:expr, $actual:expr, $expected:expr, $desc:literal) => {
        $logger.assert_eq($desc, &$expected, &$actual)
    };
    ($logger:expr, $actual:expr, $expected:expr, $desc:literal, $($arg:tt)*) => {
        $logger.assert_eq(&format!($desc, $($arg)*), &$expected, &$actual)
    };
}

/// Execute and time an operation
///
/// # Example
/// ```ignore
/// let result = test_timed!(logger, "decode", {
///     decoder.decode(&frame)
/// });
/// ```
#[macro_export]
macro_rules! test_timed {
    ($logger:expr, $name:literal, $body:block) => {{
        let start = std::time::Instant::now();
        let result = $body;
        $logger.timing($name, start.elapsed());
        result
    }};
}

// ============================================================================
// Re-export macros
// ============================================================================

pub use crate::{test_step, test_step_ok, test_step_err, test_assert, test_assert_eq, test_timed};

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_basic() {
        let mut logger = TestLogger::new("test_logger_basic", 2);

        logger.step("First step");
        logger.step_ok("completed");

        logger.step("Second step");
        logger.step_ok("also completed");

        let result = logger.finish();
        assert!(result.passed);
        assert_eq!(result.steps.len(), 2);
    }

    #[test]
    fn test_logger_with_assertions() {
        let mut logger = TestLogger::new("test_logger_with_assertions", 1);

        logger.step("Testing assertions");
        logger.assert_pass("Basic assertion");
        logger.assert_eq("Equality check", &42, &42);

        let result = logger.finish();
        assert!(result.passed);
        assert_eq!(result.assertions.len(), 2);
        assert!(result.all_assertions_passed());
    }

    #[test]
    fn test_logger_failure() {
        let mut logger = TestLogger::new("test_logger_failure", 1);

        logger.step("Testing failure");
        logger.step_err("intentional failure");

        let result = logger.finish();
        assert!(!result.passed);
        assert!(result.failure_reason.is_some());
    }

    #[test]
    fn test_logger_assertion_failure() {
        let mut logger = TestLogger::new("test_assertion_failure", 1);

        logger.step("Testing assertion failure");
        logger.assert_fail("Intentional failure", "expected", "actual");

        let result = logger.finish();
        assert!(!result.passed);
        assert_eq!(result.failed_assertion_count(), 1);
    }

    #[test]
    fn test_result_to_json() {
        let result = TestResult {
            test_name: "test_json".to_string(),
            passed: true,
            total_duration: Duration::from_millis(100),
            steps: vec![StepRecord {
                number: 1,
                description: "Test step".to_string(),
                status: StepStatus::Ok("done".to_string()),
                duration: Duration::from_millis(50),
            }],
            assertions: vec![AssertionRecord {
                description: "Test assertion".to_string(),
                passed: true,
                expected: None,
                actual: None,
            }],
            failure_reason: None,
        };

        let json = result.to_json();
        assert!(json.contains("test_json"));
        assert!(json.contains("\"passed\":true"));
        assert!(json.contains("Test step"));
    }

    #[test]
    fn test_dynamic_steps() {
        let mut logger = TestLogger::new_dynamic("test_dynamic");

        logger.step("Step 1");
        logger.step_ok("done");

        logger.step("Step 2");
        logger.step_ok("done");

        logger.step("Step 3");
        logger.step_ok("done");

        let result = logger.finish();
        assert!(result.passed);
        assert_eq!(result.steps.len(), 3);
    }

    #[test]
    fn test_skip_step() {
        let mut logger = TestLogger::new("test_skip", 2);

        logger.step("First step");
        logger.step_skip("Not needed for this test");

        logger.step("Second step");
        logger.step_ok("done");

        let result = logger.finish();
        assert!(result.passed); // Skipped steps don't cause failure
    }

    #[test]
    fn test_timing() {
        let mut logger = TestLogger::new("test_timing", 1);

        logger.step("Timed operation");
        std::thread::sleep(Duration::from_millis(10));
        logger.step_ok("done");

        let result = logger.finish();
        assert!(result.total_duration >= Duration::from_millis(10));
        assert!(result.steps[0].duration >= Duration::from_millis(10));
    }

    #[test]
    fn test_macros() {
        let mut logger = TestLogger::new("test_macros", 3);

        test_step!(logger, "Using step macro");
        test_step_ok!(logger, "done");

        test_step!(logger, "Testing {} items", 5);
        test_step_ok!(logger);

        test_step!(logger, "Assertions");
        test_assert!(logger, 1 + 1 == 2, "Math works");
        test_assert_eq!(logger, "hello", "hello", "Strings match");

        let result = logger.finish();
        assert!(result.passed);
    }
}
