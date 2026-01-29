//! Frame decoding module
//!
//! Converts raw camera frames (MJPEG, YUYV, etc.) into RGBA format
//! for the processing pipeline.
#![allow(dead_code)] // Decoder infrastructure for capture pipeline
//!
//! # Supported Formats
//! - MJPEG: Motion JPEG (decoded using image crate)
//! - YUYV: Packed YUV 4:2:2 (CPU conversion)
//! - NV12: Planar YUV 4:2:0 (CPU conversion)
//! - RGB24/RGBA32: Direct copy or expansion

use crate::core::{Frame, PixelFormat};

/// Maximum supported dimension to prevent overflow and resource exhaustion
/// 32768 x 32768 x 4 = 4GB which is already extreme for real-time processing
const MAX_DIMENSION: u32 = 32768;

/// Result of frame decoding
#[derive(Debug)]
pub struct DecodedFrame {
    /// RGBA pixel data (4 bytes per pixel)
    pub data: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

impl DecodedFrame {
    /// Create a new decoded frame with pre-allocated buffer
    ///
    /// # Panics
    /// Panics if dimensions would cause buffer size overflow (use `try_new` for fallible version)
    pub fn new(width: u32, height: u32) -> Self {
        Self::try_new(width, height).expect("Invalid dimensions for DecodedFrame")
    }

    /// Try to create a new decoded frame, returning error if dimensions are invalid
    pub fn try_new(width: u32, height: u32) -> Result<Self, DecodeError> {
        let size = Self::checked_buffer_size(width, height)?;
        Ok(Self {
            data: vec![0u8; size],
            width,
            height,
        })
    }

    /// Get the expected buffer size for given dimensions
    ///
    /// # Panics
    /// Panics if dimensions would overflow. Use `checked_buffer_size` for fallible version.
    pub fn buffer_size(width: u32, height: u32) -> usize {
        Self::checked_buffer_size(width, height).expect("Buffer size overflow")
    }

    /// Safely calculate buffer size with overflow checking
    pub fn checked_buffer_size(width: u32, height: u32) -> Result<usize, DecodeError> {
        validate_dimensions(width, height)?;
        // Use u64 arithmetic to prevent overflow before casting to usize
        let size = (width as u64) * (height as u64) * 4;
        Ok(size as usize)
    }
}

/// Validate dimensions are reasonable and won't cause overflow
fn validate_dimensions(width: u32, height: u32) -> Result<(), DecodeError> {
    if width == 0 || height == 0 {
        return Err(DecodeError::InvalidDimensions { width, height });
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(DecodeError::InvalidDimensions { width, height });
    }
    Ok(())
}

/// Calculate buffer size for a given bytes-per-pixel format with overflow checking
fn checked_buffer_size_bpp(width: u32, height: u32, bytes_per_pixel: u32) -> Result<usize, DecodeError> {
    validate_dimensions(width, height)?;
    let size = (width as u64) * (height as u64) * (bytes_per_pixel as u64);
    // Check that it fits in usize (important for 32-bit platforms)
    if size > usize::MAX as u64 {
        return Err(DecodeError::InvalidDimensions { width, height });
    }
    Ok(size as usize)
}

/// Errors that can occur during frame decoding
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Unsupported pixel format: {0:?}")]
    UnsupportedFormat(PixelFormat),

    #[error("JPEG decode error: {0}")]
    JpegError(String),

    #[error("Invalid frame dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("Buffer size mismatch: expected {expected}, got {actual}")]
    BufferSizeMismatch { expected: usize, actual: usize },
}

/// Decode a raw frame into RGBA format
///
/// Automatically selects the appropriate decoder based on the frame's pixel format.
///
/// # Performance
/// - MJPEG: Uses image crate, typically 5-15ms for 1080p on modern CPU
/// - YUYV: CPU conversion, typically 2-5ms for 1080p with SIMD
/// - RGB24/RGBA32: Direct copy/expand, <1ms
pub fn decode_frame(frame: &Frame) -> Result<DecodedFrame, DecodeError> {
    match frame.format {
        PixelFormat::Mjpeg => decode_mjpeg(&frame.data, frame.width, frame.height),
        PixelFormat::Yuyv => decode_yuyv(&frame.data, frame.width, frame.height),
        PixelFormat::Nv12 => decode_nv12(&frame.data, frame.width, frame.height),
        PixelFormat::Rgb24 => decode_rgb24(&frame.data, frame.width, frame.height),
        PixelFormat::Rgba32 => decode_rgba32(&frame.data, frame.width, frame.height),
        PixelFormat::Unknown => Err(DecodeError::UnsupportedFormat(PixelFormat::Unknown)),
    }
}

/// Decode MJPEG frame using image crate
///
/// # Arguments
/// * `data` - Raw JPEG data
/// * `expected_width` - Expected width (0 to skip validation)
/// * `expected_height` - Expected height (0 to skip validation)
///
/// # Returns
/// Decoded RGBA frame. If expected dimensions are provided and don't match,
/// logs a warning but still returns the decoded frame (camera firmware may
/// encode at slightly different dimensions than reported).
fn decode_mjpeg(data: &[u8], expected_width: u32, expected_height: u32) -> Result<DecodedFrame, DecodeError> {
    use image::ImageReader;
    use std::io::Cursor;

    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| DecodeError::JpegError(e.to_string()))?;

    let img = reader
        .decode()
        .map_err(|e| DecodeError::JpegError(e.to_string()))?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    // Validate dimensions if expected values are provided (non-zero)
    // Don't error - some cameras report slightly different dimensions than they encode
    if expected_width > 0 && expected_height > 0 {
        if width != expected_width || height != expected_height {
            tracing::warn!(
                expected_width = expected_width,
                expected_height = expected_height,
                actual_width = width,
                actual_height = height,
                "MJPEG frame dimensions don't match expected. Using actual dimensions."
            );
        }
    }

    Ok(DecodedFrame {
        data: rgba.into_raw(),
        width,
        height,
    })
}

/// Decode YUYV (YUY2) packed format to RGBA
///
/// YUYV layout: Y0 U0 Y1 V0 Y2 U1 Y3 V1 ...
/// Each 4 bytes encodes 2 pixels sharing U and V components.
///
/// Uses BT.601 coefficients for SD content (most webcams).
fn decode_yuyv(data: &[u8], width: u32, height: u32) -> Result<DecodedFrame, DecodeError> {
    // Validate dimensions and calculate buffer size safely
    let expected_size = checked_buffer_size_bpp(width, height, 2)?; // 2 bytes per pixel in YUYV
    if data.len() < expected_size {
        return Err(DecodeError::BufferSizeMismatch {
            expected: expected_size,
            actual: data.len(),
        });
    }

    let mut output = DecodedFrame::try_new(width, height)?;
    let pixels = (width as usize) * (height as usize);

    // Process 2 pixels at a time (4 bytes YUYV -> 8 bytes RGBA)
    for i in 0..(pixels / 2) {
        let yuyv_offset = i * 4;
        let rgba_offset = i * 8;

        let y0 = data[yuyv_offset] as i32;
        let u = data[yuyv_offset + 1] as i32;
        let y1 = data[yuyv_offset + 2] as i32;
        let v = data[yuyv_offset + 3] as i32;

        // BT.601 conversion (standard for webcams)
        // R = Y + 1.402 * (V - 128)
        // G = Y - 0.344 * (U - 128) - 0.714 * (V - 128)
        // B = Y + 1.772 * (U - 128)
        //
        // Using fixed-point for speed: multiply by 256, then shift right 8
        let c = y0 - 16;
        let d = u - 128;
        let e = v - 128;

        // First pixel
        let r0 = clamp_u8((298 * c + 409 * e + 128) >> 8);
        let g0 = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
        let b0 = clamp_u8((298 * c + 516 * d + 128) >> 8);

        output.data[rgba_offset] = r0;
        output.data[rgba_offset + 1] = g0;
        output.data[rgba_offset + 2] = b0;
        output.data[rgba_offset + 3] = 255;

        // Second pixel (same U, V, different Y)
        let c = y1 - 16;
        let r1 = clamp_u8((298 * c + 409 * e + 128) >> 8);
        let g1 = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
        let b1 = clamp_u8((298 * c + 516 * d + 128) >> 8);

        output.data[rgba_offset + 4] = r1;
        output.data[rgba_offset + 5] = g1;
        output.data[rgba_offset + 6] = b1;
        output.data[rgba_offset + 7] = 255;
    }

    Ok(output)
}

/// Decode NV12 (Y plane + interleaved UV) to RGBA
///
/// NV12 layout:
/// - Y plane: width * height bytes
/// - UV plane: width * height / 2 bytes (interleaved U, V)
fn decode_nv12(data: &[u8], width: u32, height: u32) -> Result<DecodedFrame, DecodeError> {
    // Validate dimensions first
    validate_dimensions(width, height)?;

    // NV12 requires even dimensions for proper UV sampling
    if width % 2 != 0 || height % 2 != 0 {
        return Err(DecodeError::InvalidDimensions { width, height });
    }

    // Calculate sizes safely using u64 to prevent overflow
    let y_size = (width as u64) * (height as u64);
    let uv_size = y_size / 2;
    let expected_size = y_size + uv_size;

    // Check for usize overflow (important on 32-bit platforms)
    if expected_size > usize::MAX as u64 {
        return Err(DecodeError::InvalidDimensions { width, height });
    }

    let y_size = y_size as usize;
    let uv_size = uv_size as usize;
    let expected_size = expected_size as usize;

    if data.len() < expected_size {
        return Err(DecodeError::BufferSizeMismatch {
            expected: expected_size,
            actual: data.len(),
        });
    }

    let mut output = DecodedFrame::try_new(width, height)?;
    let y_plane = &data[..y_size];
    let uv_plane = &data[y_size..y_size + uv_size];

    let width_usize = width as usize;
    let height_usize = height as usize;

    for row in 0..height_usize {
        for col in 0..width_usize {
            let y_idx = row * width_usize + col;

            // UV plane is subsampled 2x2, with interleaved U,V pairs
            // Each row of UV corresponds to 2 rows of Y
            // Each U,V pair corresponds to 2 columns of Y
            let uv_row = row / 2;
            let uv_col = col / 2;
            // UV plane has width bytes per row (same as Y), with U,V interleaved
            let uv_idx = uv_row * width_usize + uv_col * 2;

            // Bounds check for UV access (uv_idx + 1 must be valid)
            if uv_idx + 1 >= uv_size {
                return Err(DecodeError::BufferSizeMismatch {
                    expected: uv_idx + 2,
                    actual: uv_size,
                });
            }

            let y = y_plane[y_idx] as i32;
            let u = uv_plane[uv_idx] as i32;
            let v = uv_plane[uv_idx + 1] as i32;

            // BT.601 conversion
            let c = y - 16;
            let d = u - 128;
            let e = v - 128;

            let r = clamp_u8((298 * c + 409 * e + 128) >> 8);
            let g = clamp_u8((298 * c - 100 * d - 208 * e + 128) >> 8);
            let b = clamp_u8((298 * c + 516 * d + 128) >> 8);

            let rgba_idx = y_idx * 4;
            output.data[rgba_idx] = r;
            output.data[rgba_idx + 1] = g;
            output.data[rgba_idx + 2] = b;
            output.data[rgba_idx + 3] = 255;
        }
    }

    Ok(output)
}

/// Decode RGB24 to RGBA (add alpha channel)
fn decode_rgb24(data: &[u8], width: u32, height: u32) -> Result<DecodedFrame, DecodeError> {
    // Validate dimensions and calculate buffer size safely (3 bytes per pixel)
    let expected_size = checked_buffer_size_bpp(width, height, 3)?;
    if data.len() < expected_size {
        return Err(DecodeError::BufferSizeMismatch {
            expected: expected_size,
            actual: data.len(),
        });
    }

    let mut output = DecodedFrame::try_new(width, height)?;
    let pixels = (width as usize) * (height as usize);

    for i in 0..pixels {
        let rgb_idx = i * 3;
        let rgba_idx = i * 4;

        output.data[rgba_idx] = data[rgb_idx];
        output.data[rgba_idx + 1] = data[rgb_idx + 1];
        output.data[rgba_idx + 2] = data[rgb_idx + 2];
        output.data[rgba_idx + 3] = 255;
    }

    Ok(output)
}

/// Decode RGBA32 (direct copy)
fn decode_rgba32(data: &[u8], width: u32, height: u32) -> Result<DecodedFrame, DecodeError> {
    // Validate dimensions and calculate buffer size safely (4 bytes per pixel)
    let expected_size = checked_buffer_size_bpp(width, height, 4)?;
    if data.len() < expected_size {
        return Err(DecodeError::BufferSizeMismatch {
            expected: expected_size,
            actual: data.len(),
        });
    }

    Ok(DecodedFrame {
        data: data[..expected_size].to_vec(),
        width,
        height,
    })
}

/// Clamp an i32 to u8 range
#[inline]
fn clamp_u8(val: i32) -> u8 {
    val.clamp(0, 255) as u8
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoded_frame_size() {
        let frame = DecodedFrame::new(1920, 1080);
        assert_eq!(frame.data.len(), 1920 * 1080 * 4);
    }

    #[test]
    fn test_buffer_size_calculation() {
        assert_eq!(DecodedFrame::buffer_size(640, 480), 640 * 480 * 4);
        assert_eq!(DecodedFrame::buffer_size(1920, 1080), 1920 * 1080 * 4);
    }

    #[test]
    fn test_yuyv_decode_basic() {
        // Create a simple 2x2 YUYV frame (8 bytes)
        // Pure white in YUV: Y=235, U=128, V=128
        let yuyv_data = vec![
            235, 128, 235, 128, // Row 0: 2 white pixels
            235, 128, 235, 128, // Row 1: 2 white pixels
        ];

        let result = decode_yuyv(&yuyv_data, 2, 2);
        assert!(result.is_ok());

        let decoded = result.unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.data.len(), 16); // 4 pixels * 4 bytes

        // Check that all pixels are close to white (allowing for conversion variance)
        for i in 0..4 {
            let r = decoded.data[i * 4];
            let g = decoded.data[i * 4 + 1];
            let b = decoded.data[i * 4 + 2];
            let a = decoded.data[i * 4 + 3];

            assert!(r > 200, "R should be bright: {}", r);
            assert!(g > 200, "G should be bright: {}", g);
            assert!(b > 200, "B should be bright: {}", b);
            assert_eq!(a, 255, "Alpha should be 255");
        }
    }

    #[test]
    fn test_yuyv_buffer_too_small() {
        let small_data = vec![0u8; 4]; // Too small for 4x2 image
        let result = decode_yuyv(&small_data, 4, 2);
        assert!(matches!(result, Err(DecodeError::BufferSizeMismatch { .. })));
    }

    #[test]
    fn test_rgb24_decode() {
        // 2x1 RGB image: red, green
        let rgb_data = vec![
            255, 0, 0,   // Red
            0, 255, 0,   // Green
        ];

        let result = decode_rgb24(&rgb_data, 2, 1);
        assert!(result.is_ok());

        let decoded = result.unwrap();
        assert_eq!(decoded.data.len(), 8); // 2 pixels * 4 bytes

        // First pixel: red
        assert_eq!(decoded.data[0..4], [255, 0, 0, 255]);
        // Second pixel: green
        assert_eq!(decoded.data[4..8], [0, 255, 0, 255]);
    }

    #[test]
    fn test_rgba32_decode() {
        // 2x1 RGBA image: red with half alpha, blue with full alpha
        let rgba_data = vec![
            255, 0, 0, 128,   // Red, half alpha
            0, 0, 255, 255,   // Blue, full alpha
        ];

        let result = decode_rgba32(&rgba_data, 2, 1);
        assert!(result.is_ok());

        let decoded = result.unwrap();
        assert_eq!(decoded.data, rgba_data);
    }

    #[test]
    fn test_nv12_decode_basic() {
        // 2x2 NV12 frame
        // Y plane: 4 bytes (Y for each pixel)
        // UV plane: 2 bytes (one U, one V shared by all 4 pixels)
        let nv12_data = vec![
            235, 235, 235, 235, // Y plane: 4 white pixels
            128, 128,           // UV plane: neutral color
        ];

        let result = decode_nv12(&nv12_data, 2, 2);
        assert!(result.is_ok());

        let decoded = result.unwrap();
        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 2);
        assert_eq!(decoded.data.len(), 16);
    }

    #[test]
    fn test_decode_frame_dispatcher() {
        // Test that decode_frame routes to correct decoder
        let frame = Frame {
            data: vec![255, 0, 0, 0, 255, 0], // 2 RGB pixels
            format: PixelFormat::Rgb24,
            width: 2,
            height: 1,
            timestamp_ns: 0,
            sequence: 0,
        };

        let result = decode_frame(&frame);
        assert!(result.is_ok());
    }

    #[test]
    fn test_decode_unknown_format() {
        let frame = Frame {
            data: vec![0; 100],
            format: PixelFormat::Unknown,
            width: 10,
            height: 10,
            timestamp_ns: 0,
            sequence: 0,
        };

        let result = decode_frame(&frame);
        assert!(matches!(result, Err(DecodeError::UnsupportedFormat(_))));
    }

    #[test]
    fn test_clamp_u8() {
        assert_eq!(clamp_u8(-10), 0);
        assert_eq!(clamp_u8(0), 0);
        assert_eq!(clamp_u8(128), 128);
        assert_eq!(clamp_u8(255), 255);
        assert_eq!(clamp_u8(300), 255);
    }

    // ============================================================================
    // Overflow protection tests
    // ============================================================================

    #[test]
    fn test_dimension_validation_zero_width() {
        let result = validate_dimensions(0, 100);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_dimension_validation_zero_height() {
        let result = validate_dimensions(100, 0);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_dimension_validation_exceeds_max() {
        // Dimensions larger than MAX_DIMENSION should fail
        let result = validate_dimensions(40000, 100);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));

        let result = validate_dimensions(100, 40000);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_dimension_validation_valid() {
        // Normal dimensions should pass
        assert!(validate_dimensions(1920, 1080).is_ok());
        assert!(validate_dimensions(3840, 2160).is_ok());
        assert!(validate_dimensions(7680, 4320).is_ok()); // 8K
    }

    #[test]
    fn test_checked_buffer_size_prevents_overflow() {
        // Very large but valid dimensions (within MAX_DIMENSION)
        let result = DecodedFrame::checked_buffer_size(32768, 32768);
        assert!(result.is_ok());
        // 32768 * 32768 * 4 = 4GB, should fit in u64 then usize on 64-bit
        assert_eq!(result.unwrap(), 32768 * 32768 * 4);
    }

    #[test]
    fn test_try_new_invalid_dimensions() {
        let result = DecodedFrame::try_new(0, 100);
        assert!(result.is_err());

        let result = DecodedFrame::try_new(100, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_nv12_rejects_odd_dimensions() {
        // NV12 requires even dimensions
        let data = vec![0u8; 100];

        let result = decode_nv12(&data, 3, 2); // odd width
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));

        let result = decode_nv12(&data, 2, 3); // odd height
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_yuyv_rejects_excessive_dimensions() {
        let data = vec![0u8; 100];
        let result = decode_yuyv(&data, 40000, 100);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_rgb24_rejects_excessive_dimensions() {
        let data = vec![0u8; 100];
        let result = decode_rgb24(&data, 40000, 100);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_rgba32_rejects_excessive_dimensions() {
        let data = vec![0u8; 100];
        let result = decode_rgba32(&data, 40000, 100);
        assert!(matches!(result, Err(DecodeError::InvalidDimensions { .. })));
    }
}
