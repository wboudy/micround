//! Error Injector for Fault Tolerance Testing
//!
//! Provides controllable, deterministic error injection for testing error handling
//! and recovery paths in the application.
#![allow(dead_code)] // Test infrastructure
//!
//! # Feature Gate
//!
//! This module is only compiled when the `test-simulator` feature is enabled:
//! ```toml
//! [features]
//! test-simulator = []
//! ```
//!
//! # Philosophy
//!
//! The Error Injector provides deterministic error injection that:
//! - Is reproducible across runs (given the same seed)
//! - Supports all error types in the system
//! - Can be triggered by various conditions (time, count, pattern)
//! - Does NOT use mocks - works with real code paths
//!
//! # Usage
//!
//! ```ignore
//! use micround::core::error_injector::{ErrorInjector, InjectionTrigger};
//!
//! let injector = ErrorInjector::new()
//!     .with_trigger(InjectionTrigger::EveryN(5))
//!     .with_seed(12345);
//!
//! for i in 0..10 {
//!     if injector.should_inject() {
//!         return Err(injector.capture_error("Simulated device disconnect"));
//!     }
//!     // Normal operation...
//! }
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::core::{CaptureError, ConfigError, MicroundError, PlatformError, RenderError};

/// Trigger conditions for error injection
pub enum InjectionTrigger {
    /// Never inject errors
    Never,
    /// Always inject errors
    Always,
    /// Inject every N operations
    EveryN(u64),
    /// Inject with probability (0.0-1.0)
    Probability(f64),
    /// Inject on specific operation numbers (1-indexed)
    OnOperations(Vec<u64>),
    /// Inject after a duration since start
    AfterDuration(Duration),
    /// Inject between operation counts (inclusive)
    InRange { start: u64, end: u64 },
    /// Inject when a custom condition returns true
    Custom(Box<dyn Fn(u64) -> bool + Send + Sync>),
}

impl std::fmt::Debug for InjectionTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Never => write!(f, "Never"),
            Self::Always => write!(f, "Always"),
            Self::EveryN(n) => write!(f, "EveryN({})", n),
            Self::Probability(p) => write!(f, "Probability({})", p),
            Self::OnOperations(ops) => write!(f, "OnOperations({:?})", ops),
            Self::AfterDuration(d) => write!(f, "AfterDuration({:?})", d),
            Self::InRange { start, end } => write!(f, "InRange {{ start: {}, end: {} }}", start, end),
            Self::Custom(_) => write!(f, "Custom(<fn>)"),
        }
    }
}

impl Default for InjectionTrigger {
    fn default() -> Self {
        Self::Never
    }
}

/// Type of error to inject
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    /// Camera/capture related errors
    Capture(CaptureErrorKind),
    /// Rendering related errors
    Render(RenderErrorKind),
    /// Configuration related errors
    Config(ConfigErrorKind),
    /// Platform-specific errors
    Platform(PlatformErrorKind),
    /// Generic application errors
    Generic,
}

impl Default for ErrorType {
    fn default() -> Self {
        Self::Generic
    }
}

/// Specific capture error types (maps to CaptureError variants)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureErrorKind {
    /// Device not found
    DeviceNotFound,
    /// Device disconnected
    Disconnected,
    /// Permission denied
    PermissionDenied,
    /// Format negotiation failed
    FormatNegotiationFailed,
    /// Frame read timeout
    Timeout,
    /// Device busy
    DeviceBusy,
    /// No cameras available
    NoCameras,
    /// Platform error
    Platform,
}

/// Specific render error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderErrorKind {
    /// Display not found
    DisplayNotFound,
    /// Surface creation failed
    SurfaceCreation,
    /// Frame render failed
    RenderFailed,
    /// Initialization failed
    InitFailed,
    /// Generic render error
    Generic,
}

/// Specific config error types (maps to ConfigError variants)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigErrorKind {
    /// Failed to read config
    ReadFailed,
    /// Failed to write config
    WriteFailed,
    /// Invalid configuration
    Invalid,
    /// Config file not found
    NotFound,
}

/// Specific platform error types (maps to PlatformError variants)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformErrorKind {
    /// Operation not supported
    Unsupported,
    /// Command failed
    CommandFailed,
    /// Invalid state
    InvalidState,
    /// Resource not found
    ResourceNotFound,
    /// Permission denied
    PermissionDenied,
}

/// Statistics about error injection
#[derive(Debug, Clone, Default)]
pub struct InjectionStats {
    /// Total operations checked
    pub operations_checked: u64,
    /// Total errors injected
    pub errors_injected: u64,
    /// Last injection operation number
    pub last_injection_at: Option<u64>,
    /// Time of last injection
    pub last_injection_time: Option<Instant>,
}

/// Error Injector for fault tolerance testing
///
/// Provides deterministic, controllable error injection that works with
/// real code paths (no mocks).
pub struct ErrorInjector {
    /// Trigger condition
    trigger: InjectionTrigger,
    /// Type of error to generate
    error_type: ErrorType,
    /// Custom error message
    custom_message: Option<String>,
    /// Operation counter
    operation_count: AtomicU64,
    /// Errors injected counter
    errors_injected: AtomicU64,
    /// Random seed for deterministic behavior
    seed: u64,
    /// Start time (for duration-based triggers)
    start_time: Instant,
    /// Whether injector is active
    active: bool,
}

impl ErrorInjector {
    /// Create a new error injector (disabled by default)
    pub fn new() -> Self {
        Self {
            trigger: InjectionTrigger::Never,
            error_type: ErrorType::Generic,
            custom_message: None,
            operation_count: AtomicU64::new(0),
            errors_injected: AtomicU64::new(0),
            seed: 42,
            start_time: Instant::now(),
            active: false,
        }
    }

    /// Create an injector that injects every N operations
    pub fn every_n(n: u64) -> Self {
        Self::new().with_trigger(InjectionTrigger::EveryN(n)).activate()
    }

    /// Create an injector with a probability
    pub fn with_probability(p: f64) -> Self {
        Self::new().with_trigger(InjectionTrigger::Probability(p)).activate()
    }

    /// Create an injector for specific operations
    pub fn on_operations(ops: Vec<u64>) -> Self {
        Self::new().with_trigger(InjectionTrigger::OnOperations(ops)).activate()
    }

    /// Set the trigger condition
    pub fn with_trigger(mut self, trigger: InjectionTrigger) -> Self {
        self.trigger = trigger;
        self
    }

    /// Set the error type to inject
    pub fn with_error_type(mut self, error_type: ErrorType) -> Self {
        self.error_type = error_type;
        self
    }

    /// Set a custom error message
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.custom_message = Some(message.into());
        self
    }

    /// Set the random seed for deterministic behavior
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Activate the injector
    pub fn activate(mut self) -> Self {
        self.active = true;
        self
    }

    /// Deactivate the injector
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if an error should be injected and increment counter
    pub fn should_inject(&self) -> bool {
        if !self.active {
            return false;
        }

        let op = self.operation_count.fetch_add(1, Ordering::SeqCst) + 1;
        self.check_trigger(op)
    }

    /// Check if an error should be injected without incrementing counter
    pub fn peek_should_inject(&self) -> bool {
        if !self.active {
            return false;
        }

        let op = self.operation_count.load(Ordering::SeqCst) + 1;
        self.check_trigger(op)
    }

    /// Check trigger condition for operation number
    fn check_trigger(&self, operation: u64) -> bool {
        let should_inject = match &self.trigger {
            InjectionTrigger::Never => false,
            InjectionTrigger::Always => true,
            InjectionTrigger::EveryN(n) => *n > 0 && operation % *n == 0,
            InjectionTrigger::Probability(p) => {
                let hash = self.deterministic_random(operation);
                hash < *p
            }
            InjectionTrigger::OnOperations(ops) => ops.contains(&operation),
            InjectionTrigger::AfterDuration(d) => self.start_time.elapsed() >= *d,
            InjectionTrigger::InRange { start, end } => operation >= *start && operation <= *end,
            InjectionTrigger::Custom(f) => f(operation),
        };

        if should_inject {
            self.errors_injected.fetch_add(1, Ordering::SeqCst);
        }

        should_inject
    }

    /// Generate a deterministic "random" value in 0.0..1.0
    fn deterministic_random(&self, operation: u64) -> f64 {
        let mut hash = operation;
        hash = hash.wrapping_mul(0x517cc1b727220a95);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0x9e3779b97f4a7c15);
        hash ^= hash >> 33;
        hash = hash.wrapping_add(self.seed);
        hash ^= hash >> 27;
        (hash as f64) / (u64::MAX as f64)
    }

    /// Get current operation count
    pub fn operation_count(&self) -> u64 {
        self.operation_count.load(Ordering::SeqCst)
    }

    /// Get injection statistics
    pub fn stats(&self) -> InjectionStats {
        InjectionStats {
            operations_checked: self.operation_count.load(Ordering::SeqCst),
            errors_injected: self.errors_injected.load(Ordering::SeqCst),
            last_injection_at: None, // Would need more state to track
            last_injection_time: None,
        }
    }

    /// Reset counters
    pub fn reset(&self) {
        self.operation_count.store(0, Ordering::SeqCst);
        self.errors_injected.store(0, Ordering::SeqCst);
    }

    /// Generate a CaptureError based on the configured error type
    pub fn capture_error(&self, context: &str) -> CaptureError {
        let message = self.custom_message.clone()
            .unwrap_or_else(|| format!("Injected error at operation {}: {}",
                self.operation_count.load(Ordering::SeqCst), context));

        match &self.error_type {
            ErrorType::Capture(kind) => match kind {
                CaptureErrorKind::DeviceNotFound => CaptureError::DeviceNotFound(message),
                CaptureErrorKind::Disconnected => CaptureError::Disconnected,
                CaptureErrorKind::PermissionDenied => CaptureError::PermissionDenied(message),
                CaptureErrorKind::FormatNegotiationFailed => CaptureError::FormatNegotiationFailed(message),
                CaptureErrorKind::Timeout => CaptureError::Timeout(1000), // Default timeout value
                CaptureErrorKind::DeviceBusy => CaptureError::DeviceBusy,
                CaptureErrorKind::NoCameras => CaptureError::NoCameras,
                CaptureErrorKind::Platform => CaptureError::Platform(message),
            },
            _ => CaptureError::Platform(message),
        }
    }

    /// Generate a RenderError based on the configured error type
    pub fn render_error(&self, context: &str) -> RenderError {
        let message = self.custom_message.clone()
            .unwrap_or_else(|| format!("Injected error at operation {}: {}",
                self.operation_count.load(Ordering::SeqCst), context));

        match &self.error_type {
            ErrorType::Render(kind) => match kind {
                RenderErrorKind::DisplayNotFound => RenderError::DisplayNotFound(message),
                RenderErrorKind::SurfaceCreation => RenderError::SurfaceCreation(message),
                RenderErrorKind::RenderFailed => RenderError::Platform(message),
                RenderErrorKind::InitFailed => RenderError::Platform(message),
                RenderErrorKind::Generic => RenderError::Platform(message),
            },
            _ => RenderError::Platform(message),
        }
    }

    /// Generate a ConfigError based on the configured error type
    pub fn config_error(&self, context: &str) -> ConfigError {
        let message = self.custom_message.clone()
            .unwrap_or_else(|| format!("Injected error at operation {}: {}",
                self.operation_count.load(Ordering::SeqCst), context));

        match &self.error_type {
            ErrorType::Config(kind) => match kind {
                ConfigErrorKind::ReadFailed => ConfigError::ReadFailed(message),
                ConfigErrorKind::WriteFailed => ConfigError::WriteFailed(message),
                ConfigErrorKind::Invalid => ConfigError::Invalid(message),
                ConfigErrorKind::NotFound => ConfigError::NotFound(message),
            },
            _ => ConfigError::ReadFailed(message),
        }
    }

    /// Generate a PlatformError based on the configured error type
    pub fn platform_error(&self, context: &str) -> PlatformError {
        let message = self.custom_message.clone()
            .unwrap_or_else(|| format!("Injected error at operation {}: {}",
                self.operation_count.load(Ordering::SeqCst), context));

        match &self.error_type {
            ErrorType::Platform(kind) => match kind {
                PlatformErrorKind::Unsupported => PlatformError::Unsupported(message),
                PlatformErrorKind::CommandFailed => PlatformError::CommandFailed(message),
                PlatformErrorKind::InvalidState => PlatformError::InvalidState(message),
                PlatformErrorKind::ResourceNotFound => PlatformError::ResourceNotFound(message),
                PlatformErrorKind::PermissionDenied => PlatformError::PermissionDenied(message),
            },
            _ => PlatformError::CommandFailed(message),
        }
    }

    /// Generate a generic MicroundError
    pub fn generic_error(&self, context: &str) -> MicroundError {
        let message = self.custom_message.clone()
            .unwrap_or_else(|| format!("Injected error at operation {}: {}",
                self.operation_count.load(Ordering::SeqCst), context));
        MicroundError::Internal {
            message,
            context: crate::core::ErrorContext::new()
                .component("error_injector")
                .operation("inject"),
        }
    }
}

impl Default for ErrorInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating complex injection scenarios
pub struct ErrorInjectorBuilder {
    injector: ErrorInjector,
}

impl ErrorInjectorBuilder {
    pub fn new() -> Self {
        Self {
            injector: ErrorInjector::new(),
        }
    }

    /// Set trigger to inject every N operations
    pub fn every(mut self, n: u64) -> Self {
        self.injector.trigger = InjectionTrigger::EveryN(n);
        self
    }

    /// Set trigger to inject with probability
    pub fn probability(mut self, p: f64) -> Self {
        self.injector.trigger = InjectionTrigger::Probability(p.clamp(0.0, 1.0));
        self
    }

    /// Set trigger to inject on specific operations
    pub fn on_operations(mut self, ops: impl IntoIterator<Item = u64>) -> Self {
        self.injector.trigger = InjectionTrigger::OnOperations(ops.into_iter().collect());
        self
    }

    /// Set trigger to inject in a range of operations
    pub fn in_range(mut self, start: u64, end: u64) -> Self {
        self.injector.trigger = InjectionTrigger::InRange { start, end };
        self
    }

    /// Set trigger to always inject
    pub fn always(mut self) -> Self {
        self.injector.trigger = InjectionTrigger::Always;
        self
    }

    /// Inject capture device not found errors
    pub fn device_not_found(mut self) -> Self {
        self.injector.error_type = ErrorType::Capture(CaptureErrorKind::DeviceNotFound);
        self
    }

    /// Inject capture device disconnected errors
    pub fn device_disconnected(mut self) -> Self {
        self.injector.error_type = ErrorType::Capture(CaptureErrorKind::Disconnected);
        self
    }

    /// Inject capture permission denied errors
    pub fn permission_denied(mut self) -> Self {
        self.injector.error_type = ErrorType::Capture(CaptureErrorKind::PermissionDenied);
        self
    }

    /// Inject capture timeout errors
    pub fn timeout(mut self) -> Self {
        self.injector.error_type = ErrorType::Capture(CaptureErrorKind::Timeout);
        self
    }

    /// Inject render errors
    pub fn render_failed(mut self) -> Self {
        self.injector.error_type = ErrorType::Render(RenderErrorKind::RenderFailed);
        self
    }

    /// Inject display not found errors
    pub fn display_not_found(mut self) -> Self {
        self.injector.error_type = ErrorType::Render(RenderErrorKind::DisplayNotFound);
        self
    }

    /// Inject config read errors
    pub fn config_read_failed(mut self) -> Self {
        self.injector.error_type = ErrorType::Config(ConfigErrorKind::ReadFailed);
        self
    }

    /// Set custom error message
    pub fn message(mut self, msg: impl Into<String>) -> Self {
        self.injector.custom_message = Some(msg.into());
        self
    }

    /// Set deterministic seed
    pub fn seed(mut self, seed: u64) -> Self {
        self.injector.seed = seed;
        self
    }

    /// Build the configured injector
    pub fn build(mut self) -> ErrorInjector {
        self.injector.active = true;
        self.injector
    }
}

impl Default for ErrorInjectorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injector_disabled_by_default() {
        let injector = ErrorInjector::new();
        for _ in 0..100 {
            assert!(!injector.should_inject());
        }
    }

    #[test]
    fn test_always_trigger() {
        let injector = ErrorInjector::new()
            .with_trigger(InjectionTrigger::Always)
            .activate();

        for _ in 0..10 {
            assert!(injector.should_inject());
        }
    }

    #[test]
    fn test_never_trigger() {
        let injector = ErrorInjector::new()
            .with_trigger(InjectionTrigger::Never)
            .activate();

        for _ in 0..10 {
            assert!(!injector.should_inject());
        }
    }

    #[test]
    fn test_every_n_trigger() {
        let injector = ErrorInjector::every_n(3);

        // Operations: 1, 2, 3, 4, 5, 6, 7, 8, 9
        // Inject at: 3, 6, 9
        let results: Vec<bool> = (0..9).map(|_| injector.should_inject()).collect();
        assert_eq!(results, vec![false, false, true, false, false, true, false, false, true]);
    }

    #[test]
    fn test_on_operations_trigger() {
        let injector = ErrorInjector::on_operations(vec![2, 5, 7]);

        let results: Vec<bool> = (0..10).map(|_| injector.should_inject()).collect();
        // Operations 1-10, inject at 2, 5, 7
        assert_eq!(results[0], false); // op 1
        assert_eq!(results[1], true);  // op 2
        assert_eq!(results[2], false); // op 3
        assert_eq!(results[3], false); // op 4
        assert_eq!(results[4], true);  // op 5
        assert_eq!(results[5], false); // op 6
        assert_eq!(results[6], true);  // op 7
    }

    #[test]
    fn test_probability_trigger_deterministic() {
        let injector = ErrorInjector::with_probability(0.5).with_seed(12345);

        // Run twice with same seed, should get same results
        let results1: Vec<bool> = (0..20).map(|_| injector.should_inject()).collect();
        injector.reset();
        let results2: Vec<bool> = (0..20).map(|_| injector.should_inject()).collect();

        assert_eq!(results1, results2);
    }

    #[test]
    fn test_probability_distribution() {
        let injector = ErrorInjector::with_probability(0.5).with_seed(99999);

        let mut injections = 0;
        for _ in 0..1000 {
            if injector.should_inject() {
                injections += 1;
            }
        }

        // Should be roughly 500 with some variance
        assert!(injections > 400 && injections < 600,
            "Expected ~500 injections, got {}", injections);
    }

    #[test]
    fn test_in_range_trigger() {
        let injector = ErrorInjector::new()
            .with_trigger(InjectionTrigger::InRange { start: 3, end: 5 })
            .activate();

        let results: Vec<bool> = (0..7).map(|_| injector.should_inject()).collect();
        // Operations 1-7, inject at 3, 4, 5
        assert_eq!(results, vec![false, false, true, true, true, false, false]);
    }

    #[test]
    fn test_custom_trigger() {
        let injector = ErrorInjector::new()
            .with_trigger(InjectionTrigger::Custom(Box::new(|op| op % 2 == 0)))
            .activate();

        let results: Vec<bool> = (0..6).map(|_| injector.should_inject()).collect();
        // Inject on even operations: 2, 4, 6
        assert_eq!(results, vec![false, true, false, true, false, true]);
    }

    #[test]
    fn test_operation_count() {
        let injector = ErrorInjector::every_n(5);

        for _ in 0..10 {
            injector.should_inject();
        }

        assert_eq!(injector.operation_count(), 10);
    }

    #[test]
    fn test_reset() {
        let injector = ErrorInjector::every_n(5);

        for _ in 0..10 {
            injector.should_inject();
        }

        injector.reset();
        assert_eq!(injector.operation_count(), 0);
    }

    #[test]
    fn test_capture_error_generation() {
        let injector = ErrorInjector::new()
            .with_error_type(ErrorType::Capture(CaptureErrorKind::DeviceNotFound))
            .with_message("Camera XYZ not found");

        let err = injector.capture_error("test context");
        match err {
            CaptureError::DeviceNotFound(msg) => {
                assert!(msg.contains("Camera XYZ not found"));
            }
            _ => panic!("Expected DeviceNotFound error"),
        }
    }

    #[test]
    fn test_render_error_generation() {
        let injector = ErrorInjector::new()
            .with_error_type(ErrorType::Render(RenderErrorKind::DisplayNotFound))
            .with_message("Monitor disconnected");

        let err = injector.render_error("test context");
        match err {
            RenderError::DisplayNotFound(msg) => {
                assert!(msg.contains("Monitor disconnected"));
            }
            _ => panic!("Expected DisplayNotFound error"),
        }
    }

    #[test]
    fn test_builder_pattern() {
        let injector = ErrorInjectorBuilder::new()
            .every(10)
            .device_disconnected()
            .message("Simulated disconnect")
            .seed(42)
            .build();

        // Check it's active and configured
        assert!(injector.active);

        // Check every 10 works
        for i in 0..30 {
            let should = injector.should_inject();
            assert_eq!(should, (i + 1) % 10 == 0, "Failed at iteration {}", i);
        }
    }

    #[test]
    fn test_builder_probability() {
        let injector = ErrorInjectorBuilder::new()
            .probability(0.3)
            .timeout()
            .build();

        let mut count = 0;
        for _ in 0..1000 {
            if injector.should_inject() {
                count += 1;
            }
        }

        // Should be roughly 300 with variance
        assert!(count > 200 && count < 400, "Expected ~300, got {}", count);
    }

    #[test]
    fn test_stats() {
        let injector = ErrorInjector::every_n(3);

        for _ in 0..9 {
            injector.should_inject();
        }

        let stats = injector.stats();
        assert_eq!(stats.operations_checked, 9);
        assert_eq!(stats.errors_injected, 3); // at 3, 6, 9
    }

    #[test]
    fn test_deactivate() {
        let mut injector = ErrorInjector::new()
            .with_trigger(InjectionTrigger::Always)
            .activate();

        assert!(injector.should_inject());

        injector.deactivate();
        assert!(!injector.should_inject());
    }

    #[test]
    fn test_peek_without_increment() {
        let injector = ErrorInjector::on_operations(vec![1]);

        // Peek doesn't increment
        assert!(injector.peek_should_inject());
        assert!(injector.peek_should_inject());
        assert_eq!(injector.operation_count(), 0);

        // should_inject does increment
        assert!(injector.should_inject());
        assert_eq!(injector.operation_count(), 1);
        assert!(!injector.peek_should_inject()); // op 2 won't inject
    }
}
