//! Frame scaling engine
//!
//! Resizes camera frames to fit display dimensions with multiple scaling modes.
#![allow(dead_code)] // Full API for frame scaling feature
//! Supports high-quality filtering for both upscaling and downscaling.
//!
//! # Scaling Modes
//!
//! - **Fit**: Scale to fit within bounds, maintaining aspect ratio (letterbox)
//! - **Fill**: Scale to cover entire bounds, cropping overflow (no letterbox)
//! - **Stretch**: Scale to exactly match bounds (may distort)
//! - **Center**: No scaling, center in bounds (crop or pad as needed)

use crate::core::ScalingMode;
use crate::process::decode::DecodedFrame;

use image::{ImageBuffer, Rgba, imageops::FilterType};

/// Maximum supported dimension to prevent overflow and resource exhaustion
/// Matches the limit in decode.rs for consistency
const MAX_DIMENSION: u32 = 32768;

/// Result of a scaling operation
pub struct ScaledFrame {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Source region that was used (for Fill/Center modes that crop)
    pub source_region: Option<Region>,
    /// Destination region where content is placed (for Fit/Center modes with letterbox)
    pub content_region: Option<Region>,
}

/// A rectangular region
#[derive(Debug, Clone, Copy)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Region {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Create a region covering the full dimensions
    pub fn full(width: u32, height: u32) -> Self {
        Self { x: 0, y: 0, width, height }
    }
}

/// Configuration for the scaling engine
#[derive(Debug, Clone)]
pub struct ScaleConfig {
    /// Scaling mode to use
    pub mode: ScalingMode,
    /// Filter type for resampling
    pub filter: ScaleFilter,
    /// Background color for letterbox areas (RGBA)
    pub background: [u8; 4],
}

impl Default for ScaleConfig {
    fn default() -> Self {
        Self {
            mode: ScalingMode::Fill,
            filter: ScaleFilter::Bilinear,
            background: [0, 0, 0, 255], // Black
        }
    }
}

/// Resampling filter type
#[derive(Debug, Clone, Copy, Default)]
pub enum ScaleFilter {
    /// Fast, lowest quality
    Nearest,
    /// Good balance of speed and quality
    #[default]
    Bilinear,
    /// High quality, slower (good for downscaling)
    Lanczos,
}

impl From<ScaleFilter> for FilterType {
    fn from(filter: ScaleFilter) -> FilterType {
        match filter {
            ScaleFilter::Nearest => FilterType::Nearest,
            ScaleFilter::Bilinear => FilterType::Triangle,
            ScaleFilter::Lanczos => FilterType::Lanczos3,
        }
    }
}

/// Errors that can occur during scaling
#[derive(Debug, thiserror::Error)]
pub enum ScaleError {
    #[error("Invalid source dimensions: {width}x{height}")]
    InvalidSourceDimensions { width: u32, height: u32 },

    #[error("Invalid target dimensions: {width}x{height}")]
    InvalidTargetDimensions { width: u32, height: u32 },

    #[error("Dimensions too large: {width}x{height} exceeds maximum {MAX_DIMENSION}x{MAX_DIMENSION}")]
    DimensionsTooLarge { width: u32, height: u32 },

    #[error("Buffer too small: need {needed}, got {actual}")]
    BufferTooSmall { needed: usize, actual: usize },

    #[error("Image processing error: {0}")]
    ImageError(String),
}

/// Safely calculate RGBA buffer size with overflow protection
/// Uses u64 arithmetic to prevent overflow before casting to usize
fn checked_buffer_size(width: u32, height: u32) -> Result<usize, ScaleError> {
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(ScaleError::DimensionsTooLarge { width, height });
    }
    // Use u64 arithmetic to prevent overflow
    let size = (width as u64) * (height as u64) * 4;
    // Check that it fits in usize (important for 32-bit platforms)
    if size > usize::MAX as u64 {
        return Err(ScaleError::DimensionsTooLarge { width, height });
    }
    Ok(size as usize)
}

/// Scale a frame to the target dimensions using the specified mode
///
/// # Arguments
/// * `source` - Source RGBA frame data
/// * `src_width` - Source width in pixels
/// * `src_height` - Source height in pixels
/// * `dst_width` - Target width in pixels
/// * `dst_height` - Target height in pixels
/// * `config` - Scaling configuration
pub fn scale_frame(
    source: &DecodedFrame,
    dst_width: u32,
    dst_height: u32,
    config: &ScaleConfig,
) -> Result<ScaledFrame, ScaleError> {
    if source.width == 0 || source.height == 0 {
        return Err(ScaleError::InvalidSourceDimensions {
            width: source.width,
            height: source.height,
        });
    }

    if dst_width == 0 || dst_height == 0 {
        return Err(ScaleError::InvalidTargetDimensions {
            width: dst_width,
            height: dst_height,
        });
    }

    // Early validation of target dimensions to prevent resource exhaustion
    // This check happens before any scaling calculations to fail fast
    if dst_width > MAX_DIMENSION || dst_height > MAX_DIMENSION {
        return Err(ScaleError::DimensionsTooLarge {
            width: dst_width,
            height: dst_height,
        });
    }

    // Also validate source dimensions
    if source.width > MAX_DIMENSION || source.height > MAX_DIMENSION {
        return Err(ScaleError::DimensionsTooLarge {
            width: source.width,
            height: source.height,
        });
    }

    match config.mode {
        ScalingMode::Fit => scale_fit(source, dst_width, dst_height, config),
        ScalingMode::Fill => scale_fill(source, dst_width, dst_height, config),
        ScalingMode::Stretch => scale_stretch(source, dst_width, dst_height, config),
        ScalingMode::Center => scale_center(source, dst_width, dst_height, config),
    }
}

/// Fit mode: Scale to fit within bounds, maintaining aspect ratio
fn scale_fit(
    source: &DecodedFrame,
    dst_width: u32,
    dst_height: u32,
    config: &ScaleConfig,
) -> Result<ScaledFrame, ScaleError> {
    let (scale_w, scale_h) = calculate_fit_scale(
        source.width, source.height,
        dst_width, dst_height,
    );

    // Calculate scaled dimensions
    let scaled_width = ((source.width as f64) * scale_w).round() as u32;
    let scaled_height = ((source.height as f64) * scale_h).round() as u32;

    // Scale the source image
    let src_img = ImageBuffer::<Rgba<u8>, _>::from_raw(
        source.width,
        source.height,
        source.data.clone(),
    ).ok_or_else(|| ScaleError::ImageError("Failed to create source image".into()))?;

    let scaled = image::imageops::resize(
        &src_img,
        scaled_width,
        scaled_height,
        config.filter.into(),
    );

    // Create output with background (safe allocation with overflow check)
    let buffer_size = checked_buffer_size(dst_width, dst_height)?;
    let mut output = vec![0u8; buffer_size];
    fill_background(&mut output, dst_width, dst_height, config.background);

    // Calculate position to center the scaled image
    let offset_x = (dst_width.saturating_sub(scaled_width)) / 2;
    let offset_y = (dst_height.saturating_sub(scaled_height)) / 2;

    // Copy scaled image to output
    copy_region(
        scaled.as_raw(),
        scaled_width,
        scaled_height,
        &mut output,
        dst_width,
        offset_x,
        offset_y,
    );

    Ok(ScaledFrame {
        data: output,
        width: dst_width,
        height: dst_height,
        source_region: None,
        content_region: Some(Region::new(offset_x, offset_y, scaled_width, scaled_height)),
    })
}

/// Fill mode: Scale to cover entire bounds, cropping overflow
fn scale_fill(
    source: &DecodedFrame,
    dst_width: u32,
    dst_height: u32,
    config: &ScaleConfig,
) -> Result<ScaledFrame, ScaleError> {
    let (scale_w, scale_h) = calculate_fill_scale(
        source.width, source.height,
        dst_width, dst_height,
    );

    // Calculate dimensions needed to cover the target
    let scaled_width = ((source.width as f64) * scale_w).round() as u32;
    let scaled_height = ((source.height as f64) * scale_h).round() as u32;

    // Scale the source image
    let src_img = ImageBuffer::<Rgba<u8>, _>::from_raw(
        source.width,
        source.height,
        source.data.clone(),
    ).ok_or_else(|| ScaleError::ImageError("Failed to create source image".into()))?;

    let scaled = image::imageops::resize(
        &src_img,
        scaled_width,
        scaled_height,
        config.filter.into(),
    );

    // Calculate crop region (center the crop)
    let crop_x = scaled_width.saturating_sub(dst_width) / 2;
    let crop_y = scaled_height.saturating_sub(dst_height) / 2;

    // Extract the center region (safe allocation with overflow check)
    let buffer_size = checked_buffer_size(dst_width, dst_height)?;
    let mut output = vec![0u8; buffer_size];
    extract_region(
        scaled.as_raw(),
        scaled_width,
        crop_x,
        crop_y,
        &mut output,
        dst_width,
        dst_height,
    );

    // Calculate what part of the source was used
    let src_crop_x = (crop_x as f64 / scale_w).round() as u32;
    let src_crop_y = (crop_y as f64 / scale_h).round() as u32;
    let src_crop_w = (dst_width as f64 / scale_w).round() as u32;
    let src_crop_h = (dst_height as f64 / scale_h).round() as u32;

    Ok(ScaledFrame {
        data: output,
        width: dst_width,
        height: dst_height,
        source_region: Some(Region::new(src_crop_x, src_crop_y, src_crop_w, src_crop_h)),
        content_region: None,
    })
}

/// Stretch mode: Scale to exactly match bounds (may distort)
fn scale_stretch(
    source: &DecodedFrame,
    dst_width: u32,
    dst_height: u32,
    config: &ScaleConfig,
) -> Result<ScaledFrame, ScaleError> {
    let src_img = ImageBuffer::<Rgba<u8>, _>::from_raw(
        source.width,
        source.height,
        source.data.clone(),
    ).ok_or_else(|| ScaleError::ImageError("Failed to create source image".into()))?;

    let scaled = image::imageops::resize(
        &src_img,
        dst_width,
        dst_height,
        config.filter.into(),
    );

    Ok(ScaledFrame {
        data: scaled.into_raw(),
        width: dst_width,
        height: dst_height,
        source_region: None,
        content_region: None,
    })
}

/// Center mode: No scaling, center in bounds
fn scale_center(
    source: &DecodedFrame,
    dst_width: u32,
    dst_height: u32,
    config: &ScaleConfig,
) -> Result<ScaledFrame, ScaleError> {
    // Safe allocation with overflow check
    let buffer_size = checked_buffer_size(dst_width, dst_height)?;
    let mut output = vec![0u8; buffer_size];
    fill_background(&mut output, dst_width, dst_height, config.background);

    // Calculate positioning
    let (src_x, src_y, src_w, src_h, dst_x, dst_y) = if source.width <= dst_width && source.height <= dst_height {
        // Source fits, center it
        let offset_x = (dst_width - source.width) / 2;
        let offset_y = (dst_height - source.height) / 2;
        (0, 0, source.width, source.height, offset_x, offset_y)
    } else {
        // Source larger, crop center
        let crop_x = source.width.saturating_sub(dst_width) / 2;
        let crop_y = source.height.saturating_sub(dst_height) / 2;
        let copy_w = source.width.min(dst_width);
        let copy_h = source.height.min(dst_height);
        (crop_x, crop_y, copy_w, copy_h, 0, 0)
    };

    // Copy the region
    for row in 0..src_h.min(dst_height) {
        let src_row_start = ((src_y + row) * source.width + src_x) as usize * 4;
        let dst_row_start = ((dst_y + row) * dst_width + dst_x) as usize * 4;
        let row_bytes = src_w.min(dst_width - dst_x) as usize * 4;

        if src_row_start + row_bytes <= source.data.len() && dst_row_start + row_bytes <= output.len() {
            output[dst_row_start..dst_row_start + row_bytes]
                .copy_from_slice(&source.data[src_row_start..src_row_start + row_bytes]);
        }
    }

    let content_region = if source.width <= dst_width && source.height <= dst_height {
        Some(Region::new(
            (dst_width - source.width) / 2,
            (dst_height - source.height) / 2,
            source.width,
            source.height,
        ))
    } else {
        None
    };

    let source_region = if source.width > dst_width || source.height > dst_height {
        Some(Region::new(src_x, src_y, src_w, src_h))
    } else {
        None
    };

    Ok(ScaledFrame {
        data: output,
        width: dst_width,
        height: dst_height,
        source_region,
        content_region,
    })
}

/// Calculate scale factors for Fit mode (scale to fit within bounds)
fn calculate_fit_scale(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> (f64, f64) {
    let scale_w = dst_w as f64 / src_w as f64;
    let scale_h = dst_h as f64 / src_h as f64;
    let scale = scale_w.min(scale_h);
    (scale, scale)
}

/// Calculate scale factors for Fill mode (scale to cover entire bounds)
fn calculate_fill_scale(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> (f64, f64) {
    let scale_w = dst_w as f64 / src_w as f64;
    let scale_h = dst_h as f64 / src_h as f64;
    let scale = scale_w.max(scale_h);
    (scale, scale)
}

/// Fill output buffer with background color
fn fill_background(output: &mut [u8], _width: u32, _height: u32, color: [u8; 4]) {
    for pixel in output.chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
}

/// Copy a region from source to destination at given offset
fn copy_region(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst: &mut [u8],
    dst_width: u32,
    offset_x: u32,
    offset_y: u32,
) {
    for row in 0..src_height {
        let src_start = (row * src_width * 4) as usize;
        let dst_start = ((offset_y + row) * dst_width + offset_x) as usize * 4;
        let row_bytes = (src_width * 4) as usize;

        if dst_start + row_bytes <= dst.len() && src_start + row_bytes <= src.len() {
            dst[dst_start..dst_start + row_bytes]
                .copy_from_slice(&src[src_start..src_start + row_bytes]);
        }
    }
}

/// Extract a region from source
fn extract_region(
    src: &[u8],
    src_width: u32,
    src_x: u32,
    src_y: u32,
    dst: &mut [u8],
    dst_width: u32,
    dst_height: u32,
) {
    for row in 0..dst_height {
        let src_start = ((src_y + row) * src_width + src_x) as usize * 4;
        let dst_start = (row * dst_width * 4) as usize;
        let row_bytes = (dst_width * 4) as usize;

        if src_start + row_bytes <= src.len() && dst_start + row_bytes <= dst.len() {
            dst[dst_start..dst_start + row_bytes]
                .copy_from_slice(&src[src_start..src_start + row_bytes]);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_frame(width: u32, height: u32) -> DecodedFrame {
        // Use u64 arithmetic to prevent overflow in test helper
        let size = (width as u64) * (height as u64) * 4;
        let mut data = vec![0u8; size as usize];
        // Fill with gradient pattern
        for y in 0..height {
            for x in 0..width {
                let idx = ((y as u64 * width as u64 + x as u64) * 4) as usize;
                data[idx] = (x % 256) as u8;     // R
                data[idx + 1] = (y % 256) as u8; // G
                data[idx + 2] = 128;             // B
                data[idx + 3] = 255;             // A
            }
        }
        DecodedFrame { data, width, height }
    }

    #[test]
    fn test_scale_fit_wider_source() {
        // 16:9 source → 4:3 target (letterbox on top/bottom)
        let source = make_test_frame(160, 90);
        let config = ScaleConfig { mode: ScalingMode::Fit, ..Default::default() };

        let result = scale_frame(&source, 120, 90, &config);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        assert_eq!(scaled.width, 120);
        assert_eq!(scaled.height, 90);
        assert!(scaled.content_region.is_some());
    }

    #[test]
    fn test_scale_fit_taller_source() {
        // 4:3 source → 16:9 target (letterbox on sides)
        let source = make_test_frame(120, 90);
        let config = ScaleConfig { mode: ScalingMode::Fit, ..Default::default() };

        let result = scale_frame(&source, 160, 90, &config);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        assert_eq!(scaled.width, 160);
        assert_eq!(scaled.height, 90);
    }

    #[test]
    fn test_scale_fill() {
        let source = make_test_frame(160, 90);
        let config = ScaleConfig { mode: ScalingMode::Fill, ..Default::default() };

        let result = scale_frame(&source, 120, 90, &config);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        assert_eq!(scaled.width, 120);
        assert_eq!(scaled.height, 90);
        // Fill mode crops, so source_region should be set
        assert!(scaled.source_region.is_some());
    }

    #[test]
    fn test_scale_stretch() {
        let source = make_test_frame(100, 100);
        let config = ScaleConfig { mode: ScalingMode::Stretch, ..Default::default() };

        let result = scale_frame(&source, 200, 100, &config);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        assert_eq!(scaled.width, 200);
        assert_eq!(scaled.height, 100);
        assert_eq!(scaled.data.len(), 200 * 100 * 4);
    }

    #[test]
    fn test_scale_center_smaller_source() {
        let source = make_test_frame(50, 50);
        let config = ScaleConfig { mode: ScalingMode::Center, ..Default::default() };

        let result = scale_frame(&source, 100, 100, &config);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        assert_eq!(scaled.width, 100);
        assert_eq!(scaled.height, 100);
        assert!(scaled.content_region.is_some());

        let region = scaled.content_region.unwrap();
        assert_eq!(region.x, 25); // Centered
        assert_eq!(region.y, 25);
        assert_eq!(region.width, 50);
        assert_eq!(region.height, 50);
    }

    #[test]
    fn test_scale_center_larger_source() {
        let source = make_test_frame(200, 200);
        let config = ScaleConfig { mode: ScalingMode::Center, ..Default::default() };

        let result = scale_frame(&source, 100, 100, &config);
        assert!(result.is_ok());

        let scaled = result.unwrap();
        assert_eq!(scaled.width, 100);
        assert_eq!(scaled.height, 100);
        assert!(scaled.source_region.is_some());

        let region = scaled.source_region.unwrap();
        assert_eq!(region.x, 50); // Center crop
        assert_eq!(region.y, 50);
    }

    #[test]
    fn test_invalid_source_dimensions() {
        let source = DecodedFrame { data: vec![], width: 0, height: 100 };
        let config = ScaleConfig::default();

        let result = scale_frame(&source, 100, 100, &config);
        assert!(matches!(result, Err(ScaleError::InvalidSourceDimensions { .. })));
    }

    #[test]
    fn test_invalid_target_dimensions() {
        let source = make_test_frame(100, 100);
        let config = ScaleConfig::default();

        let result = scale_frame(&source, 0, 100, &config);
        assert!(matches!(result, Err(ScaleError::InvalidTargetDimensions { .. })));
    }

    #[test]
    fn test_fit_scale_calculation() {
        // 1920x1080 → 1280x720 (same aspect ratio)
        let (sw, sh) = calculate_fit_scale(1920, 1080, 1280, 720);
        assert!((sw - 0.666).abs() < 0.01);
        assert_eq!(sw, sh);

        // 1920x1080 → 1280x960 (target is taller)
        let (sw, sh) = calculate_fit_scale(1920, 1080, 1280, 960);
        assert!((sw - 0.666).abs() < 0.01);
        assert_eq!(sw, sh);
    }

    #[test]
    fn test_fill_scale_calculation() {
        // 1920x1080 → 1280x720 (same aspect ratio)
        let (sw, sh) = calculate_fill_scale(1920, 1080, 1280, 720);
        assert!((sw - 0.666).abs() < 0.01);
        assert_eq!(sw, sh);

        // 1920x1080 → 1280x960 (target is taller)
        let (sw, sh) = calculate_fill_scale(1920, 1080, 1280, 960);
        assert!((sw - 0.888).abs() < 0.01); // Must cover 960 height
        assert_eq!(sw, sh);
    }

    #[test]
    fn test_background_fill() {
        let mut buf = vec![0u8; 16]; // 2x2 RGBA
        fill_background(&mut buf, 2, 2, [255, 0, 0, 255]);

        assert_eq!(buf[0..4], [255, 0, 0, 255]);
        assert_eq!(buf[4..8], [255, 0, 0, 255]);
        assert_eq!(buf[8..12], [255, 0, 0, 255]);
        assert_eq!(buf[12..16], [255, 0, 0, 255]);
    }

    #[test]
    fn test_region_creation() {
        let region = Region::new(10, 20, 100, 200);
        assert_eq!(region.x, 10);
        assert_eq!(region.y, 20);
        assert_eq!(region.width, 100);
        assert_eq!(region.height, 200);

        let full = Region::full(640, 480);
        assert_eq!(full.x, 0);
        assert_eq!(full.y, 0);
        assert_eq!(full.width, 640);
        assert_eq!(full.height, 480);
    }

    #[test]
    fn test_filter_conversion() {
        assert!(matches!(FilterType::from(ScaleFilter::Nearest), FilterType::Nearest));
        assert!(matches!(FilterType::from(ScaleFilter::Bilinear), FilterType::Triangle));
        assert!(matches!(FilterType::from(ScaleFilter::Lanczos), FilterType::Lanczos3));
    }

    #[test]
    fn test_dimensions_too_large() {
        // Dimensions exceeding MAX_DIMENSION should fail
        let source = make_test_frame(100, 100);
        let config = ScaleConfig::default();

        // Target dimensions larger than MAX_DIMENSION should fail
        let result = scale_frame(&source, MAX_DIMENSION + 1, 100, &config);
        assert!(matches!(result, Err(ScaleError::DimensionsTooLarge { .. })));

        let result = scale_frame(&source, 100, MAX_DIMENSION + 1, &config);
        assert!(matches!(result, Err(ScaleError::DimensionsTooLarge { .. })));
    }

    #[test]
    fn test_max_valid_dimensions() {
        // MAX_DIMENSION should be valid (though we don't allocate that much in tests)
        // Just verify the checked_buffer_size calculation doesn't overflow
        let result = checked_buffer_size(MAX_DIMENSION, 1);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (MAX_DIMENSION as usize) * 4);

        let result = checked_buffer_size(1, MAX_DIMENSION);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), (MAX_DIMENSION as usize) * 4);
    }
}
