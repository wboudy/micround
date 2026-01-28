//! Test assertion utilities and comparison helpers
//!
//! Reusable test utilities that don't rely on mocks. These provide detailed
//! logging and rich failure messages for debugging test failures.
//!
//! # Categories
//!
//! - **Frame Comparison**: PSNR, similarity, metadata comparison
//! - **Timing Assertions**: Duration bounds, timed operations
//! - **State Assertions**: State machine transition validation
//! - **Error Assertions**: Severity and message content checks

use std::time::{Duration, Instant};
use std::fmt;

// Re-export types we need for assertions
pub use micround::core::{
    Frame, PixelFormat, ErrorSeverity, MicroundError,
    CaptureError, RenderError, ConfigError, PlatformError,
};
pub use micround::capture::CameraState;

// ============================================================================
// Frame Comparison Utilities
// ============================================================================

/// Calculate Peak Signal-to-Noise Ratio between two frames
///
/// PSNR is a common metric for image quality comparison.
/// Higher values indicate more similar images.
///
/// # Returns
/// - `Ok(f32)`: PSNR value in dB (typical range: 20-50 dB)
/// - `Err`: If frames have different dimensions or formats
///
/// # PSNR Interpretation
/// - > 40 dB: Excellent quality, nearly identical
/// - 30-40 dB: Good quality, minor differences
/// - 20-30 dB: Acceptable quality, visible differences
/// - < 20 dB: Poor quality, significant differences
pub fn frame_psnr(a: &Frame, b: &Frame) -> Result<f32, FrameCompareError> {
    // Validate frames have same dimensions
    if a.width != b.width || a.height != b.height {
        return Err(FrameCompareError::DimensionMismatch {
            a_dims: (a.width, a.height),
            b_dims: (b.width, b.height),
        });
    }

    // Validate same format
    if a.format != b.format {
        return Err(FrameCompareError::FormatMismatch {
            a_format: a.format,
            b_format: b.format,
        });
    }

    // Validate same data length
    if a.data.len() != b.data.len() {
        return Err(FrameCompareError::DataLengthMismatch {
            a_len: a.data.len(),
            b_len: b.data.len(),
        });
    }

    if a.data.is_empty() {
        return Err(FrameCompareError::EmptyFrame);
    }

    // Calculate MSE (Mean Squared Error)
    let mse: f64 = a.data.iter()
        .zip(b.data.iter())
        .map(|(&x, &y)| {
            let diff = (x as f64) - (y as f64);
            diff * diff
        })
        .sum::<f64>() / a.data.len() as f64;

    // If MSE is 0, frames are identical (infinite PSNR)
    if mse < f64::EPSILON {
        return Ok(f32::INFINITY);
    }

    // PSNR = 10 * log10(MAX^2 / MSE) = 20 * log10(MAX / sqrt(MSE))
    // For 8-bit data, MAX = 255
    let max_pixel_value: f64 = 255.0;
    let psnr = 20.0 * (max_pixel_value / mse.sqrt()).log10();

    Ok(psnr as f32)
}

/// Compare two frames with configurable tolerance
///
/// # Arguments
/// - `a`, `b`: Frames to compare
/// - `tolerance`: PSNR threshold in dB. Frames with PSNR >= tolerance are considered similar.
///
/// # Panics
/// Panics with detailed error message if frames are not similar within tolerance.
pub fn assert_frames_similar(a: &Frame, b: &Frame, tolerance_db: f32) {
    match frame_psnr(a, b) {
        Ok(psnr) => {
            if psnr < tolerance_db {
                panic!(
                    "Frames are not similar enough.\n\
                    PSNR: {:.2} dB (threshold: {:.2} dB)\n\
                    Frame A: {}x{} {:?}\n\
                    Frame B: {}x{} {:?}",
                    psnr, tolerance_db,
                    a.width, a.height, a.format,
                    b.width, b.height, b.format
                );
            }
            // Log success for debugging
            eprintln!(
                "[ASSERT] Frames similar: PSNR {:.2} dB >= {:.2} dB threshold ✓",
                psnr, tolerance_db
            );
        }
        Err(e) => {
            panic!("Cannot compare frames: {}", e);
        }
    }
}

/// Compare frame dimensions and format only (ignoring pixel data)
///
/// Useful for testing frame transformations where the data changes
/// but dimensions/format should match expected output.
pub fn assert_frame_metadata_eq(a: &Frame, b: &Frame) {
    let mut errors = Vec::new();

    if a.width != b.width {
        errors.push(format!("width: {} != {}", a.width, b.width));
    }
    if a.height != b.height {
        errors.push(format!("height: {} != {}", a.height, b.height));
    }
    if a.format != b.format {
        errors.push(format!("format: {:?} != {:?}", a.format, b.format));
    }

    if !errors.is_empty() {
        panic!(
            "Frame metadata mismatch:\n  {}\n\
            Frame A: {}x{} {:?}\n\
            Frame B: {}x{} {:?}",
            errors.join("\n  "),
            a.width, a.height, a.format,
            b.width, b.height, b.format
        );
    }

    eprintln!(
        "[ASSERT] Frame metadata matches: {}x{} {:?} ✓",
        a.width, a.height, a.format
    );
}

/// Assert that frame data is exactly equal (byte-for-byte)
pub fn assert_frames_identical(a: &Frame, b: &Frame) {
    // First check metadata
    assert_frame_metadata_eq(a, b);

    // Then check data length
    if a.data.len() != b.data.len() {
        panic!(
            "Frame data length mismatch: {} vs {} bytes",
            a.data.len(), b.data.len()
        );
    }

    // Find first difference if any
    for (i, (&x, &y)) in a.data.iter().zip(b.data.iter()).enumerate() {
        if x != y {
            panic!(
                "Frame data differs at byte {}: 0x{:02x} != 0x{:02x}\n\
                Frame dimensions: {}x{} {:?}",
                i, x, y,
                a.width, a.height, a.format
            );
        }
    }

    eprintln!(
        "[ASSERT] Frames identical: {}x{} {:?}, {} bytes ✓",
        a.width, a.height, a.format, a.data.len()
    );
}

/// Frame comparison error
#[derive(Debug, Clone)]
pub enum FrameCompareError {
    DimensionMismatch { a_dims: (u32, u32), b_dims: (u32, u32) },
    FormatMismatch { a_format: PixelFormat, b_format: PixelFormat },
    DataLengthMismatch { a_len: usize, b_len: usize },
    EmptyFrame,
}

impl fmt::Display for FrameCompareError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionMismatch { a_dims, b_dims } => {
                write!(f, "Dimension mismatch: {}x{} vs {}x{}",
                    a_dims.0, a_dims.1, b_dims.0, b_dims.1)
            }
            Self::FormatMismatch { a_format, b_format } => {
                write!(f, "Format mismatch: {:?} vs {:?}", a_format, b_format)
            }
            Self::DataLengthMismatch { a_len, b_len } => {
                write!(f, "Data length mismatch: {} vs {} bytes", a_len, b_len)
            }
            Self::EmptyFrame => write!(f, "Cannot compare empty frames"),
        }
    }
}

impl std::error::Error for FrameCompareError {}

// ============================================================================
// Timing Assertion Utilities
// ============================================================================

/// Assert that an operation completes within the specified duration
///
/// # Panics
/// Panics if the operation takes longer than `max_duration`.
///
/// # Example
/// ```ignore
/// let result = assert_completes_within(Duration::from_millis(100), || {
///     expensive_operation()
/// });
/// ```
pub fn assert_completes_within<F, T>(max_duration: Duration, f: F) -> T
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();

    if elapsed > max_duration {
        panic!(
            "Operation took too long.\n\
            Elapsed: {:?}\n\
            Maximum: {:?}\n\
            Exceeded by: {:?}",
            elapsed,
            max_duration,
            elapsed - max_duration
        );
    }

    eprintln!(
        "[TIMING] Operation completed in {:?} (limit: {:?}) ✓",
        elapsed, max_duration
    );

    result
}

/// Measure and log operation duration, returning result and timing
///
/// Useful for performance tracking in tests without asserting bounds.
///
/// # Example
/// ```ignore
/// let (result, duration) = timed("decode_frame", || decoder.decode(&frame));
/// println!("Decode took {:?}", duration);
/// ```
pub fn timed<F, T>(name: &str, f: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();

    eprintln!("[TIMING] {}: {:?}", name, elapsed);

    (result, elapsed)
}

/// Timing statistics for multiple runs
#[derive(Debug, Clone)]
pub struct TimingStats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub median: Duration,
    pub samples: usize,
}

impl fmt::Display for TimingStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "min={:?}, max={:?}, mean={:?}, median={:?}, n={}",
            self.min, self.max, self.mean, self.median, self.samples
        )
    }
}

/// Run an operation multiple times and collect timing statistics
///
/// Useful for performance benchmarking in tests.
pub fn timed_samples<F, T>(name: &str, iterations: usize, mut f: F) -> TimingStats
where
    F: FnMut() -> T,
{
    let mut durations = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        let _ = f();
        durations.push(start.elapsed());
    }

    durations.sort();

    let min = durations.first().copied().unwrap_or(Duration::ZERO);
    let max = durations.last().copied().unwrap_or(Duration::ZERO);
    let sum: Duration = durations.iter().sum();
    let mean = sum / iterations as u32;
    let median = durations.get(iterations / 2).copied().unwrap_or(Duration::ZERO);

    let stats = TimingStats {
        min,
        max,
        mean,
        median,
        samples: iterations,
    };

    eprintln!("[TIMING] {}: {}", name, stats);

    stats
}

/// Assert that average timing is within bounds
pub fn assert_timing_within(stats: &TimingStats, max_mean: Duration, max_p50: Duration) {
    let mut failures = Vec::new();

    if stats.mean > max_mean {
        failures.push(format!(
            "Mean {:?} exceeds limit {:?}",
            stats.mean, max_mean
        ));
    }

    if stats.median > max_p50 {
        failures.push(format!(
            "Median {:?} exceeds limit {:?}",
            stats.median, max_p50
        ));
    }

    if !failures.is_empty() {
        panic!(
            "Timing assertion failed:\n  {}\nStats: {}",
            failures.join("\n  "),
            stats
        );
    }

    eprintln!(
        "[ASSERT] Timing within bounds: mean={:?} <= {:?}, median={:?} <= {:?} ✓",
        stats.mean, max_mean, stats.median, max_p50
    );
}

// ============================================================================
// State Assertion Utilities
// ============================================================================

/// Valid camera state transitions
///
/// Returns true if transition from `from` to `to` is valid according
/// to the state machine diagram in capture/state.rs.
pub fn is_valid_camera_transition(from: &CameraState, to: &CameraState) -> bool {
    use CameraState::*;

    matches!((from, to),
        // From Disconnected
        (Disconnected, Available) |

        // From Available
        (Available, Opening) |
        (Available, Disconnected) |

        // From Opening
        (Opening, Ready) |
        (Opening, Error(_)) |
        (Opening, Disconnected) |

        // From Ready
        (Ready, Capturing) |
        (Ready, Available) | // close()
        (Ready, Disconnected) |

        // From Capturing
        (Capturing, Ready) | // stop_capture()
        (Capturing, Error(_)) |
        (Capturing, Disconnected) |

        // From Error
        (Error(_), Available) | // recover() then close
        (Error(_), Opening) | // retry open
        (Error(_), Disconnected)
    )
}

/// Assert that a state transition is valid
///
/// # Panics
/// Panics if the transition is not valid.
pub fn assert_valid_camera_transition(from: &CameraState, to: &CameraState) {
    if !is_valid_camera_transition(from, to) {
        panic!(
            "Invalid camera state transition.\n\
            From: {:?}\n\
            To: {:?}\n\
            This transition is not allowed by the state machine.",
            from, to
        );
    }

    eprintln!(
        "[ASSERT] Valid transition: {:?} -> {:?} ✓",
        from, to
    );
}

/// Assert that a sequence of state transitions is valid
pub fn assert_valid_camera_transition_sequence(states: &[CameraState]) {
    if states.len() < 2 {
        return; // Nothing to validate
    }

    for (i, window) in states.windows(2).enumerate() {
        let from = &window[0];
        let to = &window[1];

        if !is_valid_camera_transition(from, to) {
            panic!(
                "Invalid camera state transition at step {}.\n\
                Sequence so far: {:?}\n\
                Invalid: {:?} -> {:?}",
                i,
                &states[..=i],
                from, to
            );
        }
    }

    eprintln!(
        "[ASSERT] Valid transition sequence ({} states) ✓",
        states.len()
    );
}

// ============================================================================
// Error Assertion Utilities
// ============================================================================

/// Assert that an error has the expected severity
pub fn assert_error_severity<E>(err: &E, expected: ErrorSeverity)
where
    E: HasSeverity,
{
    let actual = err.severity();
    if actual != expected {
        panic!(
            "Error severity mismatch.\n\
            Expected: {:?}\n\
            Actual: {:?}\n\
            Error: {:?}",
            expected, actual, err
        );
    }

    eprintln!("[ASSERT] Error severity matches: {:?} ✓", expected);
}

/// Assert that an error message contains expected text
pub fn assert_error_contains<E>(err: &E, text: &str)
where
    E: fmt::Display,
{
    let message = format!("{}", err);
    if !message.contains(text) {
        panic!(
            "Error message does not contain expected text.\n\
            Expected to contain: {:?}\n\
            Actual message: {:?}",
            text, message
        );
    }

    eprintln!(
        "[ASSERT] Error contains {:?} ✓",
        text
    );
}

/// Assert that a user message contains expected text
pub fn assert_user_message_contains<E>(err: &E, text: &str)
where
    E: HasUserMessage,
{
    let message = err.user_message();
    if !message.contains(text) {
        panic!(
            "User message does not contain expected text.\n\
            Expected to contain: {:?}\n\
            Actual message: {:?}",
            text, message
        );
    }

    eprintln!(
        "[ASSERT] User message contains {:?} ✓",
        text
    );
}

/// Trait for types that have a severity method
pub trait HasSeverity: fmt::Debug {
    fn severity(&self) -> ErrorSeverity;
}

/// Trait for types that have a user_message method
pub trait HasUserMessage {
    fn user_message(&self) -> String;
}

// Implement for error types
impl HasSeverity for MicroundError {
    fn severity(&self) -> ErrorSeverity {
        MicroundError::severity(self)
    }
}

impl HasUserMessage for MicroundError {
    fn user_message(&self) -> String {
        MicroundError::user_message(self)
    }
}

impl HasSeverity for CaptureError {
    fn severity(&self) -> ErrorSeverity {
        CaptureError::severity(self)
    }
}

impl HasUserMessage for CaptureError {
    fn user_message(&self) -> String {
        CaptureError::user_message(self)
    }
}

impl HasSeverity for RenderError {
    fn severity(&self) -> ErrorSeverity {
        RenderError::severity(self)
    }
}

impl HasUserMessage for RenderError {
    fn user_message(&self) -> String {
        RenderError::user_message(self)
    }
}

impl HasSeverity for ConfigError {
    fn severity(&self) -> ErrorSeverity {
        ConfigError::severity(self)
    }
}

impl HasUserMessage for ConfigError {
    fn user_message(&self) -> String {
        ConfigError::user_message(self)
    }
}

impl HasSeverity for PlatformError {
    fn severity(&self) -> ErrorSeverity {
        PlatformError::severity(self)
    }
}

impl HasUserMessage for PlatformError {
    fn user_message(&self) -> String {
        PlatformError::user_message(self)
    }
}

// ============================================================================
// Test Helpers
// ============================================================================

/// Create a test frame with given dimensions and solid color
///
/// Useful for creating test fixtures without external files.
pub fn create_test_frame(width: u32, height: u32, format: PixelFormat, color: u8) -> Frame {
    let bytes_per_pixel = match format {
        PixelFormat::Rgba32 => 4,
        PixelFormat::Rgb24 => 3,
        PixelFormat::Yuyv => 2,
        PixelFormat::Nv12 => 1, // Y plane only for solid color
        PixelFormat::Mjpeg => 1, // Would need actual MJPEG data
        PixelFormat::Unknown => 1,
    };

    let data = vec![color; (width * height * bytes_per_pixel) as usize];

    Frame {
        data,
        format,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// Create a test frame with a gradient pattern
///
/// Creates a horizontal gradient from 0 to 255 for each row.
pub fn create_gradient_frame(width: u32, height: u32, format: PixelFormat) -> Frame {
    let bytes_per_pixel = match format {
        PixelFormat::Rgba32 => 4,
        PixelFormat::Rgb24 => 3,
        PixelFormat::Yuyv => 2,
        _ => 1,
    };

    let mut data = Vec::with_capacity((width * height * bytes_per_pixel) as usize);

    for y in 0..height {
        for x in 0..width {
            let value = ((x as f32 / width as f32) * 255.0) as u8;
            for _ in 0..bytes_per_pixel {
                data.push(value);
            }
        }
        // Slight variation per row
        let _ = y; // suppress unused warning
    }

    Frame {
        data,
        format,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// Create a test frame with a checkerboard pattern
pub fn create_checkerboard_frame(width: u32, height: u32, format: PixelFormat, block_size: u32) -> Frame {
    let bytes_per_pixel = match format {
        PixelFormat::Rgba32 => 4,
        PixelFormat::Rgb24 => 3,
        PixelFormat::Yuyv => 2,
        _ => 1,
    };

    let mut data = Vec::with_capacity((width * height * bytes_per_pixel) as usize);

    for y in 0..height {
        for x in 0..width {
            let block_x = x / block_size;
            let block_y = y / block_size;
            let value = if (block_x + block_y) % 2 == 0 { 255 } else { 0 };

            for _ in 0..bytes_per_pixel {
                data.push(value);
            }
        }
    }

    Frame {
        data,
        format,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

// ============================================================================
// Module Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Frame comparison tests
    mod frame_comparison {
        use super::*;

        #[test]
        fn test_identical_frames_have_infinite_psnr() {
            let frame = create_test_frame(100, 100, PixelFormat::Rgba32, 128);
            let psnr = frame_psnr(&frame, &frame).unwrap();
            assert!(psnr.is_infinite(), "PSNR of identical frames should be infinite");
        }

        #[test]
        fn test_similar_frames_have_high_psnr() {
            let mut frame_a = create_test_frame(100, 100, PixelFormat::Rgba32, 128);
            let mut frame_b = frame_a.data.clone();

            // Add small noise to frame_b
            for i in (0..frame_b.len()).step_by(100) {
                frame_b[i] = frame_b[i].saturating_add(1);
            }

            let frame_b = Frame {
                data: frame_b,
                format: frame_a.format,
                width: frame_a.width,
                height: frame_a.height,
                timestamp_ns: 0,
                sequence: 0,
            };

            let psnr = frame_psnr(&frame_a, &frame_b).unwrap();
            assert!(psnr > 40.0, "Small differences should have PSNR > 40 dB, got {}", psnr);
        }

        #[test]
        fn test_different_dimensions_fail() {
            let frame_a = create_test_frame(100, 100, PixelFormat::Rgba32, 128);
            let frame_b = create_test_frame(200, 100, PixelFormat::Rgba32, 128);

            let result = frame_psnr(&frame_a, &frame_b);
            assert!(matches!(result, Err(FrameCompareError::DimensionMismatch { .. })));
        }

        #[test]
        fn test_different_formats_fail() {
            let frame_a = create_test_frame(100, 100, PixelFormat::Rgba32, 128);
            let frame_b = create_test_frame(100, 100, PixelFormat::Rgb24, 128);

            let result = frame_psnr(&frame_a, &frame_b);
            assert!(matches!(result, Err(FrameCompareError::FormatMismatch { .. })));
        }

        #[test]
        fn test_assert_frames_similar() {
            let frame = create_test_frame(100, 100, PixelFormat::Rgba32, 128);
            // Should not panic for identical frames
            assert_frames_similar(&frame, &frame, 30.0);
        }

        #[test]
        fn test_assert_frame_metadata_eq() {
            let frame_a = create_test_frame(640, 480, PixelFormat::Rgba32, 0);
            let frame_b = create_test_frame(640, 480, PixelFormat::Rgba32, 255);
            // Should not panic - same dimensions/format, different data
            assert_frame_metadata_eq(&frame_a, &frame_b);
        }

        #[test]
        #[should_panic(expected = "width")]
        fn test_assert_frame_metadata_panics_on_width_mismatch() {
            let frame_a = create_test_frame(640, 480, PixelFormat::Rgba32, 0);
            let frame_b = create_test_frame(320, 480, PixelFormat::Rgba32, 0);
            assert_frame_metadata_eq(&frame_a, &frame_b);
        }
    }

    // Timing tests
    mod timing {
        use super::*;
        use std::thread;

        #[test]
        fn test_assert_completes_within_fast_operation() {
            let result = assert_completes_within(Duration::from_secs(1), || {
                42
            });
            assert_eq!(result, 42);
        }

        #[test]
        #[should_panic(expected = "took too long")]
        fn test_assert_completes_within_slow_operation() {
            assert_completes_within(Duration::from_millis(10), || {
                thread::sleep(Duration::from_millis(50));
            });
        }

        #[test]
        fn test_timed_returns_duration() {
            let (result, duration) = timed("test_op", || {
                thread::sleep(Duration::from_millis(10));
                "done"
            });
            assert_eq!(result, "done");
            assert!(duration >= Duration::from_millis(10));
        }

        #[test]
        fn test_timed_samples_collects_stats() {
            let stats = timed_samples("fast_op", 10, || {
                // Fast operation
                1 + 1
            });

            assert_eq!(stats.samples, 10);
            assert!(stats.min <= stats.mean);
            assert!(stats.mean <= stats.max);
        }
    }

    // State transition tests
    mod state_transitions {
        use super::*;

        #[test]
        fn test_valid_transitions() {
            // Normal happy path
            assert!(is_valid_camera_transition(&CameraState::Disconnected, &CameraState::Available));
            assert!(is_valid_camera_transition(&CameraState::Available, &CameraState::Opening));
            assert!(is_valid_camera_transition(&CameraState::Opening, &CameraState::Ready));
            assert!(is_valid_camera_transition(&CameraState::Ready, &CameraState::Capturing));
            assert!(is_valid_camera_transition(&CameraState::Capturing, &CameraState::Ready));
            assert!(is_valid_camera_transition(&CameraState::Ready, &CameraState::Available));
        }

        #[test]
        fn test_invalid_transitions() {
            // Can't jump straight to capturing
            assert!(!is_valid_camera_transition(&CameraState::Available, &CameraState::Capturing));
            // Can't go backwards to Opening
            assert!(!is_valid_camera_transition(&CameraState::Capturing, &CameraState::Opening));
            // Can't go from Disconnected to Ready
            assert!(!is_valid_camera_transition(&CameraState::Disconnected, &CameraState::Ready));
        }

        #[test]
        fn test_assert_valid_transition_sequence() {
            let sequence = vec![
                CameraState::Disconnected,
                CameraState::Available,
                CameraState::Opening,
                CameraState::Ready,
                CameraState::Capturing,
                CameraState::Ready,
                CameraState::Available,
            ];
            assert_valid_camera_transition_sequence(&sequence);
        }

        #[test]
        #[should_panic(expected = "Invalid camera state transition")]
        fn test_assert_invalid_transition_panics() {
            assert_valid_camera_transition(&CameraState::Available, &CameraState::Capturing);
        }
    }

    // Error assertion tests
    mod error_assertions {
        use super::*;
        use micround::core::ErrorContext;

        #[test]
        fn test_assert_error_severity() {
            let err = CaptureError::Timeout(1000);
            assert_error_severity(&err, ErrorSeverity::Recoverable);

            let err = CaptureError::PermissionDenied("camera".into());
            assert_error_severity(&err, ErrorSeverity::UserActionable);
        }

        #[test]
        fn test_assert_error_contains() {
            let err = CaptureError::DeviceNotFound("webcam".into());
            assert_error_contains(&err, "webcam");
        }

        #[test]
        fn test_assert_user_message_contains() {
            let err = CaptureError::PermissionDenied("video0".into());
            assert_user_message_contains(&err, "permission");
        }

        #[test]
        #[should_panic(expected = "severity mismatch")]
        fn test_wrong_severity_panics() {
            let err = CaptureError::Timeout(1000);
            assert_error_severity(&err, ErrorSeverity::Fatal);
        }
    }

    // Test helper tests
    mod test_helpers {
        use super::*;

        #[test]
        fn test_create_test_frame() {
            let frame = create_test_frame(100, 50, PixelFormat::Rgba32, 128);
            assert_eq!(frame.width, 100);
            assert_eq!(frame.height, 50);
            assert_eq!(frame.format, PixelFormat::Rgba32);
            assert_eq!(frame.data.len(), 100 * 50 * 4);
            assert!(frame.data.iter().all(|&b| b == 128));
        }

        #[test]
        fn test_create_gradient_frame() {
            let frame = create_gradient_frame(256, 10, PixelFormat::Rgba32);
            assert_eq!(frame.width, 256);
            assert_eq!(frame.height, 10);
            // First pixel should be near 0, last should be near 255
            assert!(frame.data[0] < 5);
            assert!(frame.data[frame.data.len() - 4] > 250);
        }

        #[test]
        fn test_create_checkerboard_frame() {
            let frame = create_checkerboard_frame(100, 100, PixelFormat::Rgba32, 10);
            assert_eq!(frame.width, 100);
            assert_eq!(frame.height, 100);
            // First block should be white (255)
            assert_eq!(frame.data[0], 255);
            // Block at (10,0) should be black (0)
            assert_eq!(frame.data[10 * 4], 0);
        }
    }
}
