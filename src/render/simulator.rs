//! Display Simulator for Testing
//!
//! Provides a simulated display backend that captures rendered frames to memory
//! buffers for verification. This enables headless testing without a real display.
//!
//! # Feature Gate
//!
//! This module is only compiled when the `test-simulator` feature is enabled:
//! ```toml
//! [features]
//! test-simulator = []
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use micround::render::simulator::{DisplaySimulator, DisplaySimulatorConfig};
//!
//! let config = DisplaySimulatorConfig {
//!     width: 1920,
//!     height: 1080,
//!     ..Default::default()
//! };
//!
//! let mut simulator = DisplaySimulator::new(config);
//! simulator.init(&DisplayId("test".into()))?;
//! simulator.render(&frame)?;
//!
//! // Verify rendered output
//! let captured = simulator.last_frame().unwrap();
//! assert_eq!(captured.width, 1920);
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::AppConfig;
use crate::core::{DisplayId, RenderError};
use crate::process::ProcessedFrame;
use crate::render::WallpaperRenderer;

/// Configuration for the display simulator
#[derive(Debug, Clone)]
pub struct DisplaySimulatorConfig {
    /// Display name for identification
    pub display_name: String,
    /// Simulated display width
    pub width: u32,
    /// Simulated display height
    pub height: u32,
    /// Number of frames to retain in history
    pub frame_history_size: usize,
    /// Simulate render latency (additional delay per frame)
    pub latency_ms: u32,
    /// Simulate occasional errors (0.0 = none, 1.0 = all)
    pub error_rate: f32,
    /// Validate frame dimensions match expected size
    pub strict_dimensions: bool,
}

impl Default for DisplaySimulatorConfig {
    fn default() -> Self {
        Self {
            display_name: "Simulated Display".into(),
            width: 1920,
            height: 1080,
            frame_history_size: 10,
            latency_ms: 0,
            error_rate: 0.0,
            strict_dimensions: false,
        }
    }
}

impl DisplaySimulatorConfig {
    /// Create a config for standard HD testing
    pub fn hd() -> Self {
        Self {
            display_name: "Simulated HD Display".into(),
            width: 1920,
            height: 1080,
            ..Default::default()
        }
    }

    /// Create a config for 4K testing
    pub fn uhd() -> Self {
        Self {
            display_name: "Simulated 4K Display".into(),
            width: 3840,
            height: 2160,
            ..Default::default()
        }
    }

    /// Create a config for stress testing with validation
    pub fn strict() -> Self {
        Self {
            display_name: "Strict Validation Display".into(),
            width: 1920,
            height: 1080,
            strict_dimensions: true,
            ..Default::default()
        }
    }

    /// Create a config that simulates slow display
    pub fn slow() -> Self {
        Self {
            display_name: "Slow Display".into(),
            latency_ms: 50,
            ..Default::default()
        }
    }

    /// Create a config that simulates unreliable display
    pub fn unreliable() -> Self {
        Self {
            display_name: "Unreliable Display".into(),
            error_rate: 0.1,
            ..Default::default()
        }
    }
}

/// Statistics about rendered frames
#[derive(Debug, Clone, Default)]
pub struct RenderStats {
    /// Total frames rendered
    pub frames_rendered: u64,
    /// Total render errors
    pub errors: u64,
    /// Average render time in microseconds
    pub avg_render_time_us: u64,
    /// Maximum render time in microseconds
    pub max_render_time_us: u64,
    /// Minimum render time in microseconds
    pub min_render_time_us: u64,
    /// Last render timestamp
    pub last_render_time: Option<Instant>,
}

/// A captured frame with metadata
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// Frame data (RGBA)
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// When this frame was rendered
    pub rendered_at: Instant,
    /// Render duration
    pub render_duration: Duration,
}

impl CapturedFrame {
    /// Get pixel at (x, y) as RGBA tuple
    pub fn pixel_at(&self, x: u32, y: u32) -> Option<(u8, u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        if idx + 3 < self.data.len() {
            Some((
                self.data[idx],
                self.data[idx + 1],
                self.data[idx + 2],
                self.data[idx + 3],
            ))
        } else {
            None
        }
    }

    /// Check if frame is solid color
    pub fn is_solid_color(&self) -> Option<(u8, u8, u8, u8)> {
        if self.data.len() < 4 {
            return None;
        }
        let reference = (
            self.data[0],
            self.data[1],
            self.data[2],
            self.data[3],
        );
        for chunk in self.data.chunks_exact(4) {
            if chunk[0] != reference.0
                || chunk[1] != reference.1
                || chunk[2] != reference.2
                || chunk[3] != reference.3
            {
                return None;
            }
        }
        Some(reference)
    }

    /// Calculate average brightness (0-255)
    pub fn average_brightness(&self) -> u8 {
        if self.data.is_empty() {
            return 0;
        }
        let total: u64 = self.data.chunks_exact(4)
            .map(|c| (c[0] as u64 + c[1] as u64 + c[2] as u64) / 3)
            .sum();
        let pixel_count = (self.data.len() / 4) as u64;
        if pixel_count == 0 {
            return 0;
        }
        (total / pixel_count) as u8
    }
}

/// Simulated display backend for headless testing
pub struct DisplaySimulator {
    config: DisplaySimulatorConfig,
    initialized: bool,
    display_id: Option<DisplayId>,
    /// Frame history for inspection
    frame_history: Arc<Mutex<VecDeque<CapturedFrame>>>,
    /// Render statistics
    stats: Arc<Mutex<RenderStats>>,
    /// Frame counter for error injection
    frame_counter: AtomicU64,
    /// Total render time accumulator
    total_render_time_us: AtomicU64,
    /// Random seed for deterministic error injection
    error_seed: u64,
}

impl DisplaySimulator {
    /// Create a new display simulator
    pub fn new(config: DisplaySimulatorConfig) -> Self {
        Self {
            config,
            initialized: false,
            display_id: None,
            frame_history: Arc::new(Mutex::new(VecDeque::new())),
            stats: Arc::new(Mutex::new(RenderStats::default())),
            frame_counter: AtomicU64::new(0),
            total_render_time_us: AtomicU64::new(0),
            error_seed: 42,
        }
    }

    /// Create with default configuration
    pub fn default_config() -> Self {
        Self::new(DisplaySimulatorConfig::default())
    }

    /// Set the random seed for deterministic error injection
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.error_seed = seed;
        self
    }

    /// Get the most recently rendered frame
    pub fn last_frame(&self) -> Option<CapturedFrame> {
        self.frame_history.lock().ok()?.back().cloned()
    }

    /// Get all captured frames in history
    pub fn frame_history(&self) -> Vec<CapturedFrame> {
        self.frame_history
            .lock()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get render statistics
    pub fn stats(&self) -> RenderStats {
        self.stats.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Clear frame history and reset statistics
    pub fn reset(&self) {
        if let Ok(mut history) = self.frame_history.lock() {
            history.clear();
        }
        if let Ok(mut stats) = self.stats.lock() {
            *stats = RenderStats::default();
        }
        self.frame_counter.store(0, Ordering::SeqCst);
        self.total_render_time_us.store(0, Ordering::SeqCst);
    }

    /// Get the number of frames rendered
    pub fn frame_count(&self) -> u64 {
        self.frame_counter.load(Ordering::SeqCst)
    }

    /// Check if an error should be injected (deterministic)
    fn should_inject_error(&self, frame_num: u64) -> bool {
        if self.config.error_rate <= 0.0 {
            return false;
        }
        // Use a simple deterministic hash for reproducible errors
        // Mix the seed in early to ensure good distribution even for frame_num=0
        let mut hash = frame_num.wrapping_add(self.error_seed);
        hash = hash.wrapping_mul(0x517cc1b727220a95);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0x9e3779b97f4a7c15);
        hash ^= hash >> 33;
        hash ^= hash >> 27;

        // Convert to 0.0-1.0 range
        let normalized = (hash as f64) / (u64::MAX as f64);
        normalized < self.config.error_rate as f64
    }
}

impl WallpaperRenderer for DisplaySimulator {
    fn init(&mut self, display: &DisplayId) -> Result<(), RenderError> {
        if self.initialized {
            return Err(RenderError::Platform(
                "Display simulator already initialized".into(),
            ));
        }

        self.display_id = Some(display.clone());
        self.initialized = true;
        self.reset();

        let display_name = &display.0;
        tracing::info!(
            display = %display_name,
            name = %self.config.display_name,
            width = self.config.width,
            height = self.config.height,
            "Display simulator initialized"
        );

        Ok(())
    }

    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError> {
        if !self.initialized {
            return Err(RenderError::Platform(
                "Display simulator not initialized".into(),
            ));
        }

        let frame_num = self.frame_counter.fetch_add(1, Ordering::SeqCst);
        let start_time = Instant::now();

        // Check for error injection
        if self.should_inject_error(frame_num) {
            if let Ok(mut stats) = self.stats.lock() {
                stats.errors += 1;
            }
            return Err(RenderError::Platform(format!(
                "Simulated render error at frame {}",
                frame_num
            )));
        }

        // Validate dimensions if strict mode
        if self.config.strict_dimensions {
            if frame.width != self.config.width || frame.height != self.config.height {
                return Err(RenderError::Platform(format!(
                    "Frame dimensions {}x{} don't match display {}x{}",
                    frame.width, frame.height, self.config.width, self.config.height
                )));
            }
        }

        // Simulate render latency
        if self.config.latency_ms > 0 {
            std::thread::sleep(Duration::from_millis(self.config.latency_ms as u64));
        }

        let render_duration = start_time.elapsed();
        let render_time_us = render_duration.as_micros() as u64;

        // Capture the frame
        let captured = CapturedFrame {
            data: frame.data.clone(),
            width: frame.width,
            height: frame.height,
            rendered_at: Instant::now(),
            render_duration,
        };

        // Store in history
        if let Ok(mut history) = self.frame_history.lock() {
            history.push_back(captured);
            while history.len() > self.config.frame_history_size {
                history.pop_front();
            }
        }

        // Update statistics
        let prev_total = self.total_render_time_us.fetch_add(render_time_us, Ordering::SeqCst);
        if let Ok(mut stats) = self.stats.lock() {
            stats.frames_rendered += 1;
            stats.last_render_time = Some(Instant::now());

            if stats.min_render_time_us == 0 || render_time_us < stats.min_render_time_us {
                stats.min_render_time_us = render_time_us;
            }
            if render_time_us > stats.max_render_time_us {
                stats.max_render_time_us = render_time_us;
            }
            stats.avg_render_time_us = (prev_total + render_time_us) / stats.frames_rendered;
        }

        tracing::trace!(
            frame = frame_num,
            width = frame.width,
            height = frame.height,
            render_time_us = render_time_us,
            "Frame rendered to simulator"
        );

        Ok(())
    }

    fn restore(&mut self, _config: &AppConfig) -> Result<(), RenderError> {
        tracing::info!("Display simulator restore called (no-op)");
        Ok(())
    }

    fn shutdown(&mut self) {
        if self.initialized {
            let stats = self.stats();
            tracing::info!(
                frames_rendered = stats.frames_rendered,
                errors = stats.errors,
                avg_render_time_us = stats.avg_render_time_us,
                "Display simulator shutdown"
            );
        }
        self.initialized = false;
        self.display_id = None;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_frame(width: u32, height: u32, color: (u8, u8, u8, u8)) -> ProcessedFrame {
        let size = (width * height * 4) as usize;
        let mut data = Vec::with_capacity(size);
        for _ in 0..(width * height) {
            data.push(color.0);
            data.push(color.1);
            data.push(color.2);
            data.push(color.3);
        }
        ProcessedFrame::new(data, width, height)
    }

    #[test]
    fn test_simulator_creation() {
        let sim = DisplaySimulator::default_config();
        assert!(!sim.initialized);
        assert_eq!(sim.frame_count(), 0);
    }

    #[test]
    fn test_simulator_init() {
        let mut sim = DisplaySimulator::default_config();
        let display = DisplayId("test-display".into());

        assert!(sim.init(&display).is_ok());
        assert!(sim.initialized);

        // Double init should fail
        assert!(sim.init(&display).is_err());
    }

    #[test]
    fn test_render_without_init() {
        let mut sim = DisplaySimulator::default_config();
        let frame = make_test_frame(100, 100, (255, 0, 0, 255));

        assert!(sim.render(&frame).is_err());
    }

    #[test]
    fn test_basic_render() {
        let mut sim = DisplaySimulator::new(DisplaySimulatorConfig::default());
        sim.init(&DisplayId("test".into())).unwrap();

        let frame = make_test_frame(100, 100, (255, 128, 64, 255));
        sim.render(&frame).unwrap();

        assert_eq!(sim.frame_count(), 1);

        let captured = sim.last_frame().unwrap();
        assert_eq!(captured.width, 100);
        assert_eq!(captured.height, 100);
    }

    #[test]
    fn test_frame_history() {
        let config = DisplaySimulatorConfig {
            frame_history_size: 3,
            ..Default::default()
        };
        let mut sim = DisplaySimulator::new(config);
        sim.init(&DisplayId("test".into())).unwrap();

        // Render 5 frames
        for i in 0..5 {
            let frame = make_test_frame(10, 10, (i * 50, 0, 0, 255));
            sim.render(&frame).unwrap();
        }

        // Should only have last 3
        let history = sim.frame_history();
        assert_eq!(history.len(), 3);

        // Verify we have the right frames (last 3)
        assert_eq!(history[0].pixel_at(0, 0).unwrap().0, 100); // i=2
        assert_eq!(history[1].pixel_at(0, 0).unwrap().0, 150); // i=3
        assert_eq!(history[2].pixel_at(0, 0).unwrap().0, 200); // i=4
    }

    #[test]
    fn test_pixel_inspection() {
        let mut sim = DisplaySimulator::default_config();
        sim.init(&DisplayId("test".into())).unwrap();

        // Create a gradient frame
        let width = 10u32;
        let height = 10u32;
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                data.push((x * 25) as u8);  // R
                data.push((y * 25) as u8);  // G
                data.push(128);              // B
                data.push(255);              // A
            }
        }
        let frame = ProcessedFrame::new(data, width, height);
        sim.render(&frame).unwrap();

        let captured = sim.last_frame().unwrap();

        // Check specific pixels
        assert_eq!(captured.pixel_at(0, 0), Some((0, 0, 128, 255)));
        assert_eq!(captured.pixel_at(5, 5), Some((125, 125, 128, 255)));
        assert_eq!(captured.pixel_at(9, 9), Some((225, 225, 128, 255)));

        // Out of bounds
        assert_eq!(captured.pixel_at(10, 10), None);
    }

    #[test]
    fn test_solid_color_detection() {
        let mut sim = DisplaySimulator::default_config();
        sim.init(&DisplayId("test".into())).unwrap();

        // Solid red frame
        let frame = make_test_frame(10, 10, (255, 0, 0, 255));
        sim.render(&frame).unwrap();

        let captured = sim.last_frame().unwrap();
        assert_eq!(captured.is_solid_color(), Some((255, 0, 0, 255)));

        // Non-solid frame
        let mut data = vec![0u8; 400];
        data[0] = 100; // Make first pixel different
        let frame = ProcessedFrame::new(data, 10, 10);
        sim.render(&frame).unwrap();

        let captured = sim.last_frame().unwrap();
        assert_eq!(captured.is_solid_color(), None);
    }

    #[test]
    fn test_brightness_calculation() {
        let mut sim = DisplaySimulator::default_config();
        sim.init(&DisplayId("test".into())).unwrap();

        // Black frame
        let frame = make_test_frame(10, 10, (0, 0, 0, 255));
        sim.render(&frame).unwrap();
        assert_eq!(sim.last_frame().unwrap().average_brightness(), 0);

        // White frame
        let frame = make_test_frame(10, 10, (255, 255, 255, 255));
        sim.render(&frame).unwrap();
        assert_eq!(sim.last_frame().unwrap().average_brightness(), 255);

        // Gray frame
        let frame = make_test_frame(10, 10, (128, 128, 128, 255));
        sim.render(&frame).unwrap();
        assert_eq!(sim.last_frame().unwrap().average_brightness(), 128);
    }

    #[test]
    fn test_strict_dimensions() {
        let config = DisplaySimulatorConfig {
            width: 100,
            height: 100,
            strict_dimensions: true,
            ..Default::default()
        };
        let mut sim = DisplaySimulator::new(config);
        sim.init(&DisplayId("test".into())).unwrap();

        // Correct size should work
        let frame = make_test_frame(100, 100, (0, 0, 0, 255));
        assert!(sim.render(&frame).is_ok());

        // Wrong size should fail
        let frame = make_test_frame(50, 50, (0, 0, 0, 255));
        assert!(sim.render(&frame).is_err());
    }

    #[test]
    fn test_error_injection() {
        let config = DisplaySimulatorConfig {
            error_rate: 0.5, // 50% error rate
            ..Default::default()
        };
        let mut sim = DisplaySimulator::new(config).with_seed(12345);
        sim.init(&DisplayId("test".into())).unwrap();

        let frame = make_test_frame(10, 10, (0, 0, 0, 255));

        let mut errors = 0;
        let mut successes = 0;
        for _ in 0..100 {
            match sim.render(&frame) {
                Ok(_) => successes += 1,
                Err(_) => errors += 1,
            }
        }

        // With 50% error rate, we should have roughly half errors
        // Allow some variance
        assert!(errors > 30 && errors < 70, "Expected ~50 errors, got {}", errors);
        assert!(successes > 30 && successes < 70);
    }

    #[test]
    fn test_render_statistics() {
        let mut sim = DisplaySimulator::default_config();
        sim.init(&DisplayId("test".into())).unwrap();

        let frame = make_test_frame(10, 10, (0, 0, 0, 255));
        for _ in 0..5 {
            sim.render(&frame).unwrap();
        }

        let stats = sim.stats();
        assert_eq!(stats.frames_rendered, 5);
        assert_eq!(stats.errors, 0);
        assert!(stats.last_render_time.is_some());
    }

    #[test]
    fn test_reset() {
        let mut sim = DisplaySimulator::default_config();
        sim.init(&DisplayId("test".into())).unwrap();

        let frame = make_test_frame(10, 10, (0, 0, 0, 255));
        for _ in 0..5 {
            sim.render(&frame).unwrap();
        }

        assert_eq!(sim.frame_count(), 5);
        assert!(!sim.frame_history().is_empty());

        sim.reset();

        assert_eq!(sim.frame_count(), 0);
        assert!(sim.frame_history().is_empty());
    }

    #[test]
    fn test_shutdown() {
        let mut sim = DisplaySimulator::default_config();
        sim.init(&DisplayId("test".into())).unwrap();

        let frame = make_test_frame(10, 10, (0, 0, 0, 255));
        sim.render(&frame).unwrap();

        sim.shutdown();
        assert!(!sim.initialized);

        // Should fail after shutdown
        assert!(sim.render(&frame).is_err());
    }

    #[test]
    fn test_config_presets() {
        let hd = DisplaySimulatorConfig::hd();
        assert_eq!(hd.width, 1920);
        assert_eq!(hd.height, 1080);

        let uhd = DisplaySimulatorConfig::uhd();
        assert_eq!(uhd.width, 3840);
        assert_eq!(uhd.height, 2160);

        let strict = DisplaySimulatorConfig::strict();
        assert!(strict.strict_dimensions);

        let slow = DisplaySimulatorConfig::slow();
        assert_eq!(slow.latency_ms, 50);

        let unreliable = DisplaySimulatorConfig::unreliable();
        assert!(unreliable.error_rate > 0.0);
    }
}
