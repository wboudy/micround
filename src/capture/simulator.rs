//! Camera Simulator for Testing
//!
//! Provides a simulated camera backend that generates frames with configurable
//! patterns. This enables testing without real hardware.
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
//! use micround::capture::simulator::{SimulatorBackend, SimulatorConfig, FramePattern};
//!
//! let config = SimulatorConfig {
//!     device_name: "Test Camera".into(),
//!     width: 640,
//!     height: 480,
//!     fps: 30,
//!     pattern: FramePattern::ColorBars,
//!     ..Default::default()
//! };
//!
//! let backend = SimulatorBackend::new(config);
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::capture::CaptureBackend;
use crate::core::{
    CameraCapability, CameraDevice, CaptureError, CaptureSettings, DeviceId, Frame,
    NegotiatedFormat, PixelFormat,
};

/// Frame pattern to generate
#[derive(Debug, Clone, Copy, Default)]
pub enum FramePattern {
    /// Solid color fill
    SolidColor { r: u8, g: u8, b: u8 },
    /// SMPTE color bars
    #[default]
    ColorBars,
    /// Horizontal gradient
    HorizontalGradient,
    /// Vertical gradient
    VerticalGradient,
    /// Checkerboard pattern
    Checkerboard { size: u32 },
    /// Moving diagonal line (animated)
    MovingLine,
    /// Counter display (shows frame number)
    Counter,
    /// Random noise
    Noise,
}

/// Configurable error type for injection
///
/// Allows fine-grained control over which error types the simulator injects
/// for testing different error recovery code paths.
#[derive(Debug, Clone, Default)]
pub enum InjectedErrorType {
    /// Timeout error (default) - simulates camera not responding
    #[default]
    Timeout,
    /// Device disconnected - simulates USB unplug
    Disconnected,
    /// Device busy - simulates another app using camera
    DeviceBusy,
    /// Platform error with custom message
    Platform(String),
    /// Cycle through all error types for comprehensive testing
    Cycling,
}

/// Configuration for the camera simulator
#[derive(Debug, Clone)]
pub struct SimulatorConfig {
    /// Name of the simulated camera
    pub device_name: String,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Target frames per second
    pub fps: u32,
    /// Pixel format to generate
    pub format: PixelFormat,
    /// Frame pattern to generate
    pub pattern: FramePattern,
    /// Simulate frame drops (0.0 = none, 1.0 = all)
    pub drop_rate: f32,
    /// Simulate capture latency (additional delay per frame)
    pub latency_ms: u32,
    /// Simulate occasional errors (0.0 = none, 1.0 = every frame)
    pub error_rate: f32,
    /// Type of error to inject when error_rate triggers
    pub error_type: InjectedErrorType,
    /// Number of simulated cameras to enumerate
    pub device_count: usize,
}

impl Default for SimulatorConfig {
    fn default() -> Self {
        Self {
            device_name: "Simulated Camera".into(),
            width: 640,
            height: 480,
            fps: 30,
            format: PixelFormat::Rgba32,
            pattern: FramePattern::ColorBars,
            drop_rate: 0.0,
            latency_ms: 0,
            error_rate: 0.0,
            error_type: InjectedErrorType::default(),
            device_count: 1,
        }
    }
}

impl SimulatorConfig {
    /// Create a config for high-resolution testing
    pub fn hd() -> Self {
        Self {
            device_name: "Simulated HD Camera".into(),
            width: 1920,
            height: 1080,
            fps: 30,
            ..Default::default()
        }
    }

    /// Create a config for stress testing
    pub fn stress_test() -> Self {
        Self {
            device_name: "Stress Test Camera".into(),
            width: 3840,
            height: 2160,
            fps: 60,
            drop_rate: 0.05,
            latency_ms: 5,
            error_rate: 0.01,
            ..Default::default()
        }
    }

    /// Create a config that simulates an unreliable camera
    pub fn unreliable() -> Self {
        Self {
            device_name: "Unreliable Camera".into(),
            width: 640,
            height: 480,
            fps: 30,
            drop_rate: 0.2,
            latency_ms: 50,
            error_rate: 0.1,
            ..Default::default()
        }
    }
}

/// Simulated camera backend for testing
pub struct SimulatorBackend {
    config: SimulatorConfig,
    is_open: bool,
    is_capturing: AtomicBool,
    current_format: Option<NegotiatedFormat>,
    frame_counter: AtomicU64,
    error_counter: AtomicU64,
    start_time: Option<Instant>,
    last_frame_time: Option<Instant>,
    rng_state: u64,
}

impl SimulatorBackend {
    /// Create a new simulator with the given configuration
    pub fn new(config: SimulatorConfig) -> Self {
        Self {
            config,
            is_open: false,
            is_capturing: AtomicBool::new(false),
            current_format: None,
            frame_counter: AtomicU64::new(0),
            error_counter: AtomicU64::new(0),
            start_time: None,
            last_frame_time: None,
            rng_state: 0x12345678,
        }
    }

    /// Create a new simulator with default configuration
    pub fn new_default() -> Self {
        Self::new(SimulatorConfig::default())
    }

    /// Get the current frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_counter.load(Ordering::Relaxed)
    }

    /// Simple pseudo-random number generator (xorshift)
    fn next_random(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }

    /// Check if we should drop this frame based on drop rate
    fn should_drop_frame(&mut self) -> bool {
        if self.config.drop_rate <= 0.0 {
            return false;
        }
        let threshold = (self.config.drop_rate * u32::MAX as f32) as u64;
        (self.next_random() & 0xFFFFFFFF) < threshold
    }

    /// Check if we should simulate an error
    fn should_error(&mut self) -> bool {
        if self.config.error_rate <= 0.0 {
            return false;
        }
        let threshold = (self.config.error_rate * u32::MAX as f32) as u64;
        (self.next_random() & 0xFFFFFFFF) < threshold
    }

    /// Generate an error based on the configured error type
    fn generate_error(&self) -> CaptureError {
        match &self.config.error_type {
            InjectedErrorType::Timeout => CaptureError::Timeout(100),
            InjectedErrorType::Disconnected => CaptureError::Disconnected,
            InjectedErrorType::DeviceBusy => CaptureError::DeviceBusy,
            InjectedErrorType::Platform(msg) => CaptureError::Platform(msg.clone()),
            InjectedErrorType::Cycling => {
                // Cycle through error types for comprehensive testing
                let error_num = self.error_counter.fetch_add(1, Ordering::Relaxed);
                match error_num % 4 {
                    0 => CaptureError::Timeout(100),
                    1 => CaptureError::Disconnected,
                    2 => CaptureError::DeviceBusy,
                    _ => CaptureError::Platform("Simulated platform error".into()),
                }
            }
        }
    }

    /// Generate a frame with the configured pattern
    fn generate_frame(&mut self) -> Frame {
        let frame_num = self.frame_counter.fetch_add(1, Ordering::Relaxed);
        let width = self.config.width;
        let height = self.config.height;
        let size = (width * height * 4) as usize; // RGBA

        let mut data = vec![0u8; size];

        match self.config.pattern {
            FramePattern::SolidColor { r, g, b } => {
                self.generate_solid(&mut data, width, height, r, g, b);
            }
            FramePattern::ColorBars => {
                self.generate_color_bars(&mut data, width, height);
            }
            FramePattern::HorizontalGradient => {
                self.generate_horizontal_gradient(&mut data, width, height);
            }
            FramePattern::VerticalGradient => {
                self.generate_vertical_gradient(&mut data, width, height);
            }
            FramePattern::Checkerboard { size: check_size } => {
                self.generate_checkerboard(&mut data, width, height, check_size);
            }
            FramePattern::MovingLine => {
                self.generate_moving_line(&mut data, width, height, frame_num);
            }
            FramePattern::Counter => {
                self.generate_counter(&mut data, width, height, frame_num);
            }
            FramePattern::Noise => {
                self.generate_noise(&mut data);
            }
        }

        let timestamp_ns = self
            .start_time
            .map(|t| t.elapsed().as_nanos() as u64)
            .unwrap_or(0);

        Frame {
            data,
            format: PixelFormat::Rgba32,
            width,
            height,
            timestamp_ns,
            sequence: frame_num,
        }
    }

    fn generate_solid(&self, data: &mut [u8], width: u32, height: u32, r: u8, g: u8, b: u8) {
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                data[idx] = r;
                data[idx + 1] = g;
                data[idx + 2] = b;
                data[idx + 3] = 255;
            }
        }
    }

    fn generate_color_bars(&self, data: &mut [u8], width: u32, height: u32) {
        // SMPTE color bars: white, yellow, cyan, green, magenta, red, blue, black
        let colors: [(u8, u8, u8); 8] = [
            (255, 255, 255), // White
            (255, 255, 0),   // Yellow
            (0, 255, 255),   // Cyan
            (0, 255, 0),     // Green
            (255, 0, 255),   // Magenta
            (255, 0, 0),     // Red
            (0, 0, 255),     // Blue
            (0, 0, 0),       // Black
        ];

        let bar_width = width / 8;

        for y in 0..height {
            for x in 0..width {
                let bar_idx = (x / bar_width).min(7) as usize;
                let (r, g, b) = colors[bar_idx];
                let idx = ((y * width + x) * 4) as usize;
                data[idx] = r;
                data[idx + 1] = g;
                data[idx + 2] = b;
                data[idx + 3] = 255;
            }
        }
    }

    fn generate_horizontal_gradient(&self, data: &mut [u8], width: u32, height: u32) {
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let v = ((x * 255) / width) as u8;
                data[idx] = v;
                data[idx + 1] = v;
                data[idx + 2] = v;
                data[idx + 3] = 255;
            }
        }
    }

    fn generate_vertical_gradient(&self, data: &mut [u8], width: u32, height: u32) {
        for y in 0..height {
            let v = ((y * 255) / height) as u8;
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                data[idx] = v;
                data[idx + 1] = v;
                data[idx + 2] = v;
                data[idx + 3] = 255;
            }
        }
    }

    fn generate_checkerboard(&self, data: &mut [u8], width: u32, height: u32, check_size: u32) {
        let check_size = check_size.max(1);
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                let checker = ((x / check_size) + (y / check_size)).is_multiple_of(2);
                let v = if checker { 255u8 } else { 0u8 };
                data[idx] = v;
                data[idx + 1] = v;
                data[idx + 2] = v;
                data[idx + 3] = 255;
            }
        }
    }

    fn generate_moving_line(&self, data: &mut [u8], width: u32, height: u32, frame_num: u64) {
        // Black background
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
            pixel[3] = 255;
        }

        // Draw diagonal line that moves with frame number
        let offset = (frame_num as i32 * 5) % (width as i32 + height as i32);
        for i in 0..width.max(height) {
            let x = ((i as i32 + offset) % width as i32) as u32;
            let y = i % height;
            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 < data.len() {
                data[idx] = 0;
                data[idx + 1] = 255;
                data[idx + 2] = 0;
                data[idx + 3] = 255;
            }
        }
    }

    fn generate_counter(&self, data: &mut [u8], width: u32, height: u32, frame_num: u64) {
        // Gray background
        for pixel in data.chunks_exact_mut(4) {
            pixel[0] = 128;
            pixel[1] = 128;
            pixel[2] = 128;
            pixel[3] = 255;
        }

        // Simple digit display (very basic, just for testing)
        let digits = format!("{:06}", frame_num);
        let digit_width = 8;
        let digit_height = 12;
        let start_x = (width - digits.len() as u32 * digit_width) / 2;
        let start_y = (height - digit_height) / 2;

        // Draw each digit as a simple block (just indicates frame is changing)
        for (i, _c) in digits.chars().enumerate() {
            let dx = start_x + i as u32 * digit_width;
            for dy in 0..digit_height {
                for ddx in 0..(digit_width - 1) {
                    let x = dx + ddx;
                    let y = start_y + dy;
                    if x < width && y < height {
                        let idx = ((y * width + x) * 4) as usize;
                        data[idx] = 255;
                        data[idx + 1] = 255;
                        data[idx + 2] = 255;
                        data[idx + 3] = 255;
                    }
                }
            }
        }
    }

    fn generate_noise(&mut self, data: &mut [u8]) {
        for pixel in data.chunks_exact_mut(4) {
            let r = self.next_random();
            pixel[0] = r as u8;
            pixel[1] = (r >> 8) as u8;
            pixel[2] = (r >> 16) as u8;
            pixel[3] = 255;
        }
    }

    /// Generate capabilities for the simulated camera
    fn generate_capabilities(&self) -> Vec<CameraCapability> {
        vec![
            CameraCapability {
                width: self.config.width,
                height: self.config.height,
                format: PixelFormat::Rgba32,
                framerate: self.config.fps as f32,
            },
            CameraCapability {
                width: 640,
                height: 480,
                format: PixelFormat::Rgba32,
                framerate: 30.0,
            },
            CameraCapability {
                width: 1280,
                height: 720,
                format: PixelFormat::Rgba32,
                framerate: 30.0,
            },
            CameraCapability {
                width: 1920,
                height: 1080,
                format: PixelFormat::Rgba32,
                framerate: 30.0,
            },
        ]
    }
}

impl CaptureBackend for SimulatorBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice> {
        (0..self.config.device_count)
            .map(|i| {
                let id = format!("simulator:{}", i);
                let name = if i == 0 {
                    self.config.device_name.clone()
                } else {
                    format!("{} #{}", self.config.device_name, i + 1)
                };

                CameraDevice {
                    id: DeviceId(id),
                    name,
                    manufacturer: Some("Micround Test Framework".into()),
                    capabilities: self.generate_capabilities(),
                    is_available: true,
                }
            })
            .collect()
    }

    fn open(
        &mut self,
        device_id: &DeviceId,
        settings: CaptureSettings,
    ) -> Result<NegotiatedFormat, CaptureError> {
        // Validate device ID
        if !device_id.0.starts_with("simulator:") {
            return Err(CaptureError::DeviceNotFound(device_id.0.clone()));
        }

        // Use requested settings
        let width = settings.width;
        let height = settings.height;
        let fps = settings.framerate as u32;

        let format = NegotiatedFormat {
            width,
            height,
            format: PixelFormat::Rgba32, // Simulator always produces RGBA
            framerate: settings.framerate,
            exact_match: true,
        };

        // Update config to match negotiated format
        self.config.width = width;
        self.config.height = height;
        self.config.fps = fps;

        self.current_format = Some(format.clone());
        self.is_open = true;
        self.frame_counter.store(0, Ordering::Relaxed);

        tracing::info!(
            device = %device_id.0,
            width = width,
            height = height,
            framerate = settings.framerate,
            "Simulator camera opened"
        );

        Ok(format)
    }

    fn start(&mut self) -> Result<(), CaptureError> {
        if !self.is_open {
            return Err(CaptureError::Platform("Camera not open".into()));
        }

        self.is_capturing.store(true, Ordering::Release);
        self.start_time = Some(Instant::now());
        self.last_frame_time = Some(Instant::now());
        self.rng_state = 0x12345678; // Reset RNG for reproducibility

        tracing::info!("Simulator capture started");
        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.is_capturing.store(false, Ordering::Release);
        tracing::info!(
            frames_captured = self.frame_counter.load(Ordering::Relaxed),
            "Simulator capture stopped"
        );
        Ok(())
    }

    fn close(&mut self) {
        self.is_capturing.store(false, Ordering::Release);
        self.is_open = false;
        self.current_format = None;
        tracing::info!("Simulator camera closed");
    }

    fn is_capturing(&self) -> bool {
        self.is_capturing.load(Ordering::Acquire)
    }

    fn current_format(&self) -> Option<NegotiatedFormat> {
        self.current_format.clone()
    }

    fn next_frame(&mut self) -> Result<Frame, CaptureError> {
        if !self.is_capturing() {
            return Err(CaptureError::Platform("Not capturing".into()));
        }

        // Simulate frame timing
        let frame_interval = Duration::from_secs_f64(1.0 / self.config.fps as f64);
        if let Some(last_time) = self.last_frame_time {
            let elapsed = last_time.elapsed();
            if elapsed < frame_interval {
                thread::sleep(frame_interval - elapsed);
            }
        }
        self.last_frame_time = Some(Instant::now());

        // Simulate additional latency
        if self.config.latency_ms > 0 {
            thread::sleep(Duration::from_millis(self.config.latency_ms as u64));
        }

        // Simulate errors using configurable error type
        if self.should_error() {
            return Err(self.generate_error());
        }

        // Simulate frame drops (just skip and return next frame)
        while self.should_drop_frame() {
            self.frame_counter.fetch_add(1, Ordering::Relaxed);
        }

        Ok(self.generate_frame())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulator_creation() {
        let config = SimulatorConfig::default();
        let backend = SimulatorBackend::new(config);
        assert!(!backend.is_open);
        assert!(!backend.is_capturing());
    }

    #[test]
    fn test_device_enumeration() {
        let config = SimulatorConfig {
            device_count: 3,
            ..Default::default()
        };
        let backend = SimulatorBackend::new(config);

        let devices = backend.enumerate_devices();
        assert_eq!(devices.len(), 3);
        assert!(devices[0].id.0.starts_with("simulator:"));
        assert!(!devices[0].capabilities.is_empty());
    }

    #[test]
    fn test_open_and_close() {
        let mut backend = SimulatorBackend::new_default();
        let devices = backend.enumerate_devices();
        let device_id = &devices[0].id;

        let settings = CaptureSettings {
            width: 800,
            height: 600,
            framerate: 25.0,
            format: None,
        };

        let format = backend.open(device_id, settings).unwrap();
        assert_eq!(format.width, 800);
        assert_eq!(format.height, 600);
        assert_eq!(format.framerate, 25.0);
        assert!(backend.is_open);

        backend.close();
        assert!(!backend.is_open);
    }

    #[test]
    fn test_capture_frames() {
        let config = SimulatorConfig {
            fps: 1000, // High FPS for fast test
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();

        let settings = CaptureSettings {
            width: 640,
            height: 480,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        assert!(backend.is_capturing());

        // Capture a few frames
        for i in 0..3 {
            let frame = backend.next_frame().unwrap();
            assert_eq!(frame.sequence, i);
            assert_eq!(frame.width, 640);
            assert_eq!(frame.height, 480);
            assert_eq!(frame.format, PixelFormat::Rgba32);
            assert_eq!(frame.data.len(), 640 * 480 * 4);
        }

        backend.stop().unwrap();
        assert!(!backend.is_capturing());
    }

    #[test]
    fn test_color_bars_pattern() {
        let config = SimulatorConfig {
            width: 80,
            height: 10,
            pattern: FramePattern::ColorBars,
            fps: 1000,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();

        let settings = CaptureSettings {
            width: 80,
            height: 10,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        let frame = backend.next_frame().unwrap();

        // First bar should be white (first 10 pixels)
        assert_eq!(frame.data[0], 255); // R
        assert_eq!(frame.data[1], 255); // G
        assert_eq!(frame.data[2], 255); // B

        // Second bar should be yellow (pixels 10-19)
        let idx = 10 * 4;
        assert_eq!(frame.data[idx], 255); // R
        assert_eq!(frame.data[idx + 1], 255); // G
        assert_eq!(frame.data[idx + 2], 0); // B

        backend.stop().unwrap();
    }

    #[test]
    fn test_solid_color_pattern() {
        let config = SimulatorConfig {
            width: 10,
            height: 10,
            pattern: FramePattern::SolidColor {
                r: 100,
                g: 150,
                b: 200,
            },
            fps: 1000,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();

        let settings = CaptureSettings {
            width: 10,
            height: 10,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        let frame = backend.next_frame().unwrap();

        // All pixels should be the same color
        for pixel in frame.data.chunks_exact(4) {
            assert_eq!(pixel[0], 100);
            assert_eq!(pixel[1], 150);
            assert_eq!(pixel[2], 200);
            assert_eq!(pixel[3], 255);
        }

        backend.stop().unwrap();
    }

    #[test]
    fn test_checkerboard_pattern() {
        let config = SimulatorConfig {
            width: 20,
            height: 20,
            pattern: FramePattern::Checkerboard { size: 10 },
            fps: 1000,
            ..Default::default()
        };
        let mut backend = SimulatorBackend::new(config);
        let devices = backend.enumerate_devices();

        let settings = CaptureSettings {
            width: 20,
            height: 20,
            framerate: 1000.0,
            format: None,
        };
        backend.open(&devices[0].id, settings).unwrap();
        backend.start().unwrap();

        let frame = backend.next_frame().unwrap();

        // Top-left 10x10 block should be white
        assert_eq!(frame.data[0], 255);
        // Second 10x10 block (starting at x=10) should be black
        let idx = 10 * 4;
        assert_eq!(frame.data[idx], 0);

        backend.stop().unwrap();
    }

    #[test]
    fn test_hd_config() {
        let config = SimulatorConfig::hd();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
    }

    #[test]
    fn test_stress_config() {
        let config = SimulatorConfig::stress_test();
        assert_eq!(config.width, 3840);
        assert_eq!(config.height, 2160);
        assert!(config.drop_rate > 0.0);
    }

    #[test]
    fn test_invalid_device_id() {
        let mut backend = SimulatorBackend::new_default();

        let result = backend.open(&DeviceId("invalid:0".into()), CaptureSettings::default());

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CaptureError::DeviceNotFound(_)
        ));
    }

    #[test]
    fn test_capture_without_start() {
        let mut backend = SimulatorBackend::new_default();
        let devices = backend.enumerate_devices();

        backend
            .open(&devices[0].id, CaptureSettings::default())
            .unwrap();

        // Should fail since we haven't called start()
        let result = backend.next_frame();
        assert!(result.is_err());
    }
}
