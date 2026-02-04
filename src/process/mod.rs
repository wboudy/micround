//! Frame processing pipeline
//!
//! Transforms raw camera frames into display-ready images.
//! Includes color conversion, scaling, rotation, and overlay compositing.
//!
//! # Pipeline Stages
//!
//! ```text
//! RawFrame (from capture)
//!     │
//!     ▼
//! ┌─────────────┐
//! │   Decode    │  (convert to RGBA)
//! └─────────────┘
//!     │
//!     ▼
//! ┌─────────────┐
//! │  Transform  │  (rotate, flip)
//! └─────────────┘
//!     │
//!     ▼
//! ┌─────────────┐
//! │   Scale     │  (fit/fill/stretch/center)
//! └─────────────┘
//!     │
//!     ▼
//! ProcessedFrame (to render)
//! ```
//!
//! # Dynamic Pipeline Optimization
//!
//! Stages are skipped when they would be no-ops:
//! - Transform: skipped if rotation is None and flip is None
//! - Scale: skipped if source dimensions match target and mode is Fill/Stretch

pub mod buffer;
pub mod decode;
pub mod gpu;
pub mod overlay;
pub mod scale;
pub mod transform;

pub use buffer::{FrameBuffer, FrameBufferPool, PoolConfig, PoolStats, PoolStatsSnapshot};
pub use decode::{decode_frame, DecodeError, DecodedFrame};
pub use gpu::{
    GpuContext, GpuError, GpuProcessConfig, GpuProcessMetrics, GpuProcessedFrame, GpuProcessor,
};
pub use overlay::{
    composite_overlays, Color, Overlay, OverlayConfig, OverlayContent, OverlayError,
    OverlayPosition, OverlayStyle, TextSize,
};
pub use scale::{scale_frame, Region, ScaleConfig, ScaleError, ScaleFilter, ScaledFrame};
pub use transform::{transform_frame, TransformError, TransformedFrame};

use crate::core::{Flip, Frame, Rotation, ScalingMode};
use std::time::{Duration, Instant};

/// Processed frame ready for rendering
#[derive(Debug)]
pub struct ProcessedFrame {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Processing metrics for this frame
    pub metrics: Option<FrameMetrics>,
}

impl ProcessedFrame {
    /// Create a new processed frame
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            data,
            width,
            height,
            metrics: None,
        }
    }

    /// Create a processed frame with metrics
    pub fn with_metrics(data: Vec<u8>, width: u32, height: u32, metrics: FrameMetrics) -> Self {
        Self {
            data,
            width,
            height,
            metrics: Some(metrics),
        }
    }
}

/// Metrics for a single frame's processing
#[derive(Debug, Clone)]
pub struct FrameMetrics {
    /// Time spent decoding
    pub decode_time: Duration,
    /// Time spent on geometric transforms
    pub transform_time: Duration,
    /// Time spent scaling
    pub scale_time: Duration,
    /// Total processing time
    pub total_time: Duration,
    /// Whether decode stage was executed
    pub decode_executed: bool,
    /// Whether transform stage was executed
    pub transform_executed: bool,
    /// Whether scale stage was executed
    pub scale_executed: bool,
}

impl FrameMetrics {
    /// Create empty metrics
    fn new() -> Self {
        Self {
            decode_time: Duration::ZERO,
            transform_time: Duration::ZERO,
            scale_time: Duration::ZERO,
            total_time: Duration::ZERO,
            decode_executed: false,
            transform_executed: false,
            scale_executed: false,
        }
    }
}

/// Frame processor configuration
#[derive(Debug, Clone)]
pub struct ProcessorConfig {
    /// Target width for output frame
    pub target_width: u32,
    /// Target height for output frame
    pub target_height: u32,
    /// Scaling mode
    pub scaling: ScalingMode,
    /// Rotation to apply
    pub rotation: Rotation,
    /// Flip to apply
    pub flip: Flip,
    /// Scale filter quality
    pub filter: ScaleFilter,
    /// Background color for letterboxing (RGBA)
    pub background: [u8; 4],
    /// Whether to collect timing metrics
    pub collect_metrics: bool,
}

impl Default for ProcessorConfig {
    fn default() -> Self {
        Self {
            target_width: 1920,
            target_height: 1080,
            scaling: ScalingMode::Fill,
            rotation: Rotation::None,
            flip: Flip::None,
            filter: ScaleFilter::Bilinear,
            background: [0, 0, 0, 255], // Black
            collect_metrics: false,
        }
    }
}

impl ProcessorConfig {
    /// Create a new processor config with target dimensions
    pub fn new(target_width: u32, target_height: u32) -> Self {
        Self {
            target_width,
            target_height,
            ..Default::default()
        }
    }

    /// Check if transform stage can be skipped (no-op)
    fn transform_is_noop(&self) -> bool {
        self.rotation == Rotation::None && self.flip == Flip::None
    }

    /// Check if scale stage can be skipped for given source dimensions
    fn scale_is_noop(&self, src_width: u32, src_height: u32) -> bool {
        // Can only skip if dimensions match AND mode doesn't require processing
        if src_width != self.target_width || src_height != self.target_height {
            return false;
        }
        // Fill and Stretch with matching dimensions are no-ops
        // Fit and Center might add letterboxing, so we can't skip
        matches!(self.scaling, ScalingMode::Fill | ScalingMode::Stretch)
    }
}

/// Errors that can occur during frame processing
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("Decode error: {0}")]
    Decode(#[from] DecodeError),

    #[error("Transform error: {0}")]
    Transform(#[from] TransformError),

    #[error("Scale error: {0}")]
    Scale(#[from] ScaleError),

    #[error("Invalid frame: {reason}")]
    InvalidFrame { reason: String },

    #[error("Invalid configuration: {reason}")]
    InvalidConfig { reason: String },
}

/// Process a raw frame into a display-ready frame
///
/// This function chains together the decode, transform, and scale stages
/// to convert a raw camera frame into an RGBA frame ready for rendering.
///
/// # Pipeline
///
/// 1. **Decode**: Convert raw format (MJPEG, YUYV, etc.) to RGBA
/// 2. **Transform**: Apply rotation and flip (skipped if both are None)
/// 3. **Scale**: Resize to target dimensions (skipped if dimensions match)
///
/// # Arguments
///
/// * `frame` - Raw frame from camera capture
/// * `config` - Processing configuration
///
/// # Returns
///
/// Processed frame ready for rendering, with optional timing metrics
///
/// # Example
///
/// ```ignore
/// let config = ProcessorConfig::new(1920, 1080);
/// let processed = process_frame(&raw_frame, &config)?;
/// render_to_wallpaper(&processed);
/// ```
pub fn process_frame(
    frame: &Frame,
    config: &ProcessorConfig,
) -> Result<ProcessedFrame, ProcessError> {
    // Validate inputs
    if frame.width == 0 || frame.height == 0 {
        return Err(ProcessError::InvalidFrame {
            reason: format!("Invalid frame dimensions: {}x{}", frame.width, frame.height),
        });
    }

    if config.target_width == 0 || config.target_height == 0 {
        return Err(ProcessError::InvalidConfig {
            reason: format!(
                "Invalid target dimensions: {}x{}",
                config.target_width, config.target_height
            ),
        });
    }

    let total_start = if config.collect_metrics {
        Some(Instant::now())
    } else {
        None
    };
    let mut metrics = FrameMetrics::new();

    // Stage 1: Decode to RGBA
    let decode_start = if config.collect_metrics {
        Some(Instant::now())
    } else {
        None
    };
    let decoded = decode_frame(frame)?;
    if let Some(start) = decode_start {
        metrics.decode_time = start.elapsed();
        metrics.decode_executed = true;
    }

    // Track current frame state (will change through pipeline)
    let mut current_data = decoded.data;
    let mut current_width = decoded.width;
    let mut current_height = decoded.height;

    // Stage 2: Transform (rotation and flip) - skip if no-op
    if !config.transform_is_noop() {
        let transform_start = if config.collect_metrics {
            Some(Instant::now())
        } else {
            None
        };

        let decoded_for_transform = DecodedFrame {
            data: current_data,
            width: current_width,
            height: current_height,
        };

        let transformed = transform_frame(&decoded_for_transform, config.flip, config.rotation)?;

        current_data = transformed.data;
        current_width = transformed.width;
        current_height = transformed.height;

        if let Some(start) = transform_start {
            metrics.transform_time = start.elapsed();
            metrics.transform_executed = true;
        }
    }

    // Stage 3: Scale - skip if no-op
    if !config.scale_is_noop(current_width, current_height) {
        let scale_start = if config.collect_metrics {
            Some(Instant::now())
        } else {
            None
        };

        let decoded_for_scale = DecodedFrame {
            data: current_data,
            width: current_width,
            height: current_height,
        };

        let scale_config = ScaleConfig {
            mode: config.scaling,
            filter: config.filter,
            background: config.background,
        };

        let scaled = scale_frame(
            &decoded_for_scale,
            config.target_width,
            config.target_height,
            &scale_config,
        )?;

        current_data = scaled.data;
        current_width = scaled.width;
        current_height = scaled.height;

        if let Some(start) = scale_start {
            metrics.scale_time = start.elapsed();
            metrics.scale_executed = true;
        }
    }

    // Calculate total time
    if let Some(start) = total_start {
        metrics.total_time = start.elapsed();
    }

    Ok(ProcessedFrame {
        data: current_data,
        width: current_width,
        height: current_height,
        metrics: if config.collect_metrics {
            Some(metrics)
        } else {
            None
        },
    })
}

/// Builder pattern for ProcessorConfig
impl ProcessorConfig {
    /// Set scaling mode
    pub fn with_scaling(mut self, mode: ScalingMode) -> Self {
        self.scaling = mode;
        self
    }

    /// Set rotation
    pub fn with_rotation(mut self, rotation: Rotation) -> Self {
        self.rotation = rotation;
        self
    }

    /// Set flip
    pub fn with_flip(mut self, flip: Flip) -> Self {
        self.flip = flip;
        self
    }

    /// Set scale filter
    pub fn with_filter(mut self, filter: ScaleFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Set background color
    pub fn with_background(mut self, color: [u8; 4]) -> Self {
        self.background = color;
        self
    }

    /// Enable metrics collection
    pub fn with_metrics(mut self, enabled: bool) -> Self {
        self.collect_metrics = enabled;
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::PixelFormat;

    /// Create a test frame with given format and dimensions
    fn make_test_frame(width: u32, height: u32, format: PixelFormat) -> Frame {
        let bytes_per_pixel = match format {
            PixelFormat::Rgba32 => 4,
            PixelFormat::Rgb24 => 3,
            PixelFormat::Yuyv => 2,
            PixelFormat::Nv12 => 3, // Y + UV/2
            _ => 4,                 // Default
        };

        let size = (width * height * bytes_per_pixel as u32) as usize;
        let mut data = vec![0u8; size];

        // Fill with recognizable pattern based on format
        match format {
            PixelFormat::Rgba32 => {
                for i in 0..(width * height) as usize {
                    let idx = i * 4;
                    data[idx] = (i % 256) as u8; // R
                    data[idx + 1] = (i / 256) as u8; // G
                    data[idx + 2] = 128; // B
                    data[idx + 3] = 255; // A
                }
            }
            PixelFormat::Rgb24 => {
                for i in 0..(width * height) as usize {
                    let idx = i * 3;
                    data[idx] = (i % 256) as u8; // R
                    data[idx + 1] = (i / 256) as u8; // G
                    data[idx + 2] = 128; // B
                }
            }
            PixelFormat::Yuyv => {
                // Fill with white pixels (Y=235, U=128, V=128)
                for i in 0..((width * height) as usize / 2) {
                    let idx = i * 4;
                    data[idx] = 235; // Y0
                    data[idx + 1] = 128; // U
                    data[idx + 2] = 235; // Y1
                    data[idx + 3] = 128; // V
                }
            }
            _ => {}
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

    #[test]
    fn test_process_frame_basic() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(200, 150);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let processed = result.unwrap();
        assert_eq!(processed.width, 200);
        assert_eq!(processed.height, 150);
        assert_eq!(processed.data.len(), 200 * 150 * 4);
    }

    #[test]
    fn test_process_frame_with_metrics() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(200, 150).with_metrics(true);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let processed = result.unwrap();
        assert!(processed.metrics.is_some());

        let metrics = processed.metrics.unwrap();
        assert!(metrics.decode_executed);
        assert!(metrics.total_time > Duration::ZERO);
    }

    #[test]
    fn test_process_frame_rgb24_input() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgb24);
        let config = ProcessorConfig::new(100, 100);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let processed = result.unwrap();
        // Output should be RGBA
        assert_eq!(processed.data.len(), 100 * 100 * 4);
    }

    #[test]
    fn test_process_frame_yuyv_input() {
        let frame = make_test_frame(100, 100, PixelFormat::Yuyv);
        let config = ProcessorConfig::new(100, 100);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let processed = result.unwrap();
        assert_eq!(processed.width, 100);
        assert_eq!(processed.height, 100);
    }

    #[test]
    fn test_process_frame_with_rotation() {
        let frame = make_test_frame(100, 80, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(200, 150)
            .with_rotation(Rotation::Clockwise90)
            .with_metrics(true);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let processed = result.unwrap();
        assert_eq!(processed.width, 200);
        assert_eq!(processed.height, 150);

        // Transform should have been executed
        let metrics = processed.metrics.unwrap();
        assert!(metrics.transform_executed);
    }

    #[test]
    fn test_process_frame_with_flip() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(100, 100)
            .with_flip(Flip::Horizontal)
            .with_metrics(true);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let metrics = result.unwrap().metrics.unwrap();
        assert!(metrics.transform_executed);
    }

    #[test]
    fn test_transform_noop_optimization() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(200, 150)
            .with_rotation(Rotation::None)
            .with_flip(Flip::None)
            .with_metrics(true);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        // Transform should NOT have been executed
        let metrics = result.unwrap().metrics.unwrap();
        assert!(!metrics.transform_executed);
    }

    #[test]
    fn test_scale_noop_optimization() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        // Same dimensions with Fill mode should skip scaling
        let config = ProcessorConfig::new(100, 100)
            .with_scaling(ScalingMode::Fill)
            .with_metrics(true);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        // Scale should NOT have been executed
        let metrics = result.unwrap().metrics.unwrap();
        assert!(!metrics.scale_executed);
    }

    #[test]
    fn test_scale_not_skipped_for_fit_mode() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        // Same dimensions but Fit mode might add letterboxing
        let config = ProcessorConfig::new(100, 100)
            .with_scaling(ScalingMode::Fit)
            .with_metrics(true);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        // Scale SHOULD be executed for Fit mode even with same dimensions
        let metrics = result.unwrap().metrics.unwrap();
        assert!(metrics.scale_executed);
    }

    #[test]
    fn test_different_scaling_modes() {
        let frame = make_test_frame(160, 90, PixelFormat::Rgba32);

        for mode in [
            ScalingMode::Fit,
            ScalingMode::Fill,
            ScalingMode::Stretch,
            ScalingMode::Center,
        ] {
            let config = ProcessorConfig::new(320, 240).with_scaling(mode);
            let result = process_frame(&frame, &config);
            assert!(result.is_ok(), "Failed for mode {:?}", mode);

            let processed = result.unwrap();
            assert_eq!(processed.width, 320);
            assert_eq!(processed.height, 240);
        }
    }

    #[test]
    fn test_invalid_frame_dimensions() {
        let frame = Frame {
            data: vec![],
            format: PixelFormat::Rgba32,
            width: 0,
            height: 100,
            timestamp_ns: 0,
            sequence: 0,
        };
        let config = ProcessorConfig::new(100, 100);

        let result = process_frame(&frame, &config);
        assert!(matches!(result, Err(ProcessError::InvalidFrame { .. })));
    }

    #[test]
    fn test_invalid_config_dimensions() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(0, 100);

        let result = process_frame(&frame, &config);
        assert!(matches!(result, Err(ProcessError::InvalidConfig { .. })));
    }

    #[test]
    fn test_config_builder() {
        let config = ProcessorConfig::new(1920, 1080)
            .with_scaling(ScalingMode::Fit)
            .with_rotation(Rotation::Clockwise90)
            .with_flip(Flip::Horizontal)
            .with_filter(ScaleFilter::Lanczos)
            .with_background([255, 0, 0, 255])
            .with_metrics(true);

        assert_eq!(config.target_width, 1920);
        assert_eq!(config.target_height, 1080);
        assert_eq!(config.scaling, ScalingMode::Fit);
        assert_eq!(config.rotation, Rotation::Clockwise90);
        assert_eq!(config.flip, Flip::Horizontal);
        assert!(matches!(config.filter, ScaleFilter::Lanczos));
        assert_eq!(config.background, [255, 0, 0, 255]);
        assert!(config.collect_metrics);
    }

    #[test]
    fn test_combined_rotation_and_flip() {
        let frame = make_test_frame(100, 80, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(200, 150)
            .with_rotation(Rotation::Clockwise180)
            .with_flip(Flip::Both);

        let result = process_frame(&frame, &config);
        assert!(result.is_ok());

        let processed = result.unwrap();
        assert_eq!(processed.width, 200);
        assert_eq!(processed.height, 150);
    }

    #[test]
    fn test_default_config() {
        let config = ProcessorConfig::default();

        assert_eq!(config.target_width, 1920);
        assert_eq!(config.target_height, 1080);
        assert_eq!(config.scaling, ScalingMode::Fill);
        assert_eq!(config.rotation, Rotation::None);
        assert_eq!(config.flip, Flip::None);
        assert_eq!(config.background, [0, 0, 0, 255]);
        assert!(!config.collect_metrics);
    }

    #[test]
    fn test_frame_metrics_structure() {
        let frame = make_test_frame(100, 100, PixelFormat::Rgba32);
        let config = ProcessorConfig::new(200, 150)
            .with_rotation(Rotation::Clockwise90)
            .with_metrics(true);

        let result = process_frame(&frame, &config);
        let metrics = result.unwrap().metrics.unwrap();

        // All stages should be executed
        assert!(metrics.decode_executed);
        assert!(metrics.transform_executed);
        assert!(metrics.scale_executed);

        // Total should be >= sum of parts
        assert!(metrics.total_time >= metrics.decode_time);
    }
}
