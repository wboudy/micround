//! Geometric transforms for frames
//!
//! Implements rotation (90° increments) and flip (horizontal/vertical) transforms.
#![allow(dead_code)] // Transform pipeline infrastructure
//! These are applied before scaling to correct camera orientation mismatches.
//!
//! # Transform Order
//! When both flip and rotation are applied:
//! 1. Flip (horizontal/vertical/both)
//! 2. Rotate (0°/90°/180°/270° clockwise)

use crate::core::{Rotation, Flip};
use crate::process::decode::DecodedFrame;

/// Result of a transform operation
pub struct TransformedFrame {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Width in pixels (may differ from input for 90°/270° rotation)
    pub width: u32,
    /// Height in pixels (may differ from input for 90°/270° rotation)
    pub height: u32,
}

/// Errors that can occur during transforms
#[derive(Debug, thiserror::Error)]
pub enum TransformError {
    #[error("Invalid frame dimensions: {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
}

/// Apply geometric transforms to a frame
///
/// Transforms are applied in order: flip, then rotate.
///
/// # Arguments
/// * `source` - Source RGBA frame
/// * `flip` - Flip transformation to apply
/// * `rotation` - Rotation to apply (clockwise)
///
/// # Returns
/// Transformed frame (dimensions may change for 90°/270° rotation)
pub fn transform_frame(
    source: &DecodedFrame,
    flip: Flip,
    rotation: Rotation,
) -> Result<TransformedFrame, TransformError> {
    if source.width == 0 || source.height == 0 {
        return Err(TransformError::InvalidDimensions {
            width: source.width,
            height: source.height,
        });
    }

    // Apply flip first
    let flipped = apply_flip(&source.data, source.width, source.height, flip);
    
    // Then apply rotation
    let (rotated, out_width, out_height) = apply_rotation(
        &flipped,
        source.width,
        source.height,
        rotation,
    );

    Ok(TransformedFrame {
        data: rotated,
        width: out_width,
        height: out_height,
    })
}

/// Apply flip transformation
fn apply_flip(data: &[u8], width: u32, height: u32, flip: Flip) -> Vec<u8> {
    match flip {
        Flip::None => data.to_vec(),
        Flip::Horizontal => flip_horizontal(data, width, height),
        Flip::Vertical => flip_vertical(data, width, height),
        Flip::Both => flip_both(data, width, height),
    }
}

/// Apply rotation transformation
fn apply_rotation(
    data: &[u8],
    width: u32,
    height: u32,
    rotation: Rotation,
) -> (Vec<u8>, u32, u32) {
    match rotation {
        Rotation::None => (data.to_vec(), width, height),
        Rotation::Clockwise90 => rotate_90(data, width, height),
        Rotation::Clockwise180 => (rotate_180(data, width, height), width, height),
        Rotation::Clockwise270 => rotate_270(data, width, height),
    }
}

/// Flip horizontally (mirror across vertical axis)
fn flip_horizontal(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut output = vec![0u8; data.len()];

    for y in 0..height {
        for x in 0..width {
            let src_idx = ((y * width + x) * 4) as usize;
            let dst_idx = ((y * width + (width - 1 - x)) * 4) as usize;
            output[dst_idx..dst_idx + 4].copy_from_slice(&data[src_idx..src_idx + 4]);
        }
    }

    output
}

/// Flip vertically (mirror across horizontal axis)
fn flip_vertical(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut output = vec![0u8; data.len()];
    let row_bytes = (width * 4) as usize;

    for y in 0..height {
        let src_row = (y * width * 4) as usize;
        let dst_row = ((height - 1 - y) * width * 4) as usize;
        output[dst_row..dst_row + row_bytes].copy_from_slice(&data[src_row..src_row + row_bytes]);
    }

    output
}

/// Flip both horizontally and vertically (equivalent to 180° rotation)
fn flip_both(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    // flip_both is equivalent to rotate_180
    rotate_180(data, width, height)
}

/// Rotate 90° clockwise
/// Output dimensions are swapped (width becomes height, height becomes width)
fn rotate_90(data: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let new_width = height;
    let new_height = width;
    let mut output = vec![0u8; data.len()];

    for y in 0..height {
        for x in 0..width {
            let src_idx = ((y * width + x) * 4) as usize;
            // For 90° clockwise: (x, y) -> (height - 1 - y, x)
            let new_x = height - 1 - y;
            let new_y = x;
            let dst_idx = ((new_y * new_width + new_x) * 4) as usize;
            output[dst_idx..dst_idx + 4].copy_from_slice(&data[src_idx..src_idx + 4]);
        }
    }

    (output, new_width, new_height)
}

/// Rotate 180° clockwise
fn rotate_180(data: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut output = vec![0u8; data.len()];
    let total_pixels = (width * height) as usize;

    for i in 0..total_pixels {
        let src_idx = i * 4;
        let dst_idx = (total_pixels - 1 - i) * 4;
        output[dst_idx..dst_idx + 4].copy_from_slice(&data[src_idx..src_idx + 4]);
    }

    output
}

/// Rotate 270° clockwise (or 90° counter-clockwise)
/// Output dimensions are swapped
fn rotate_270(data: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    let new_width = height;
    let new_height = width;
    let mut output = vec![0u8; data.len()];

    for y in 0..height {
        for x in 0..width {
            let src_idx = ((y * width + x) * 4) as usize;
            // For 270° clockwise (90° CCW): (x, y) -> (y, width - 1 - x)
            let new_x = y;
            let new_y = width - 1 - x;
            let dst_idx = ((new_y * new_width + new_x) * 4) as usize;
            output[dst_idx..dst_idx + 4].copy_from_slice(&data[src_idx..src_idx + 4]);
        }
    }

    (output, new_width, new_height)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a simple test frame with a recognizable pattern
    /// Each pixel encodes its position: R=x, G=y, B=0, A=255
    fn make_test_frame(width: u32, height: u32) -> DecodedFrame {
        let size = (width * height * 4) as usize;
        let mut data = vec![0u8; size];
        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                data[idx] = x as u8;     // R = x position
                data[idx + 1] = y as u8; // G = y position
                data[idx + 2] = 0;       // B = 0
                data[idx + 3] = 255;     // A = 255
            }
        }
        DecodedFrame { data, width, height }
    }

    /// Get pixel value at position (returns (R, G, B, A))
    fn get_pixel(data: &[u8], width: u32, x: u32, y: u32) -> (u8, u8, u8, u8) {
        let idx = ((y * width + x) * 4) as usize;
        (data[idx], data[idx + 1], data[idx + 2], data[idx + 3])
    }

    #[test]
    fn test_no_transform() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::None, Rotation::None);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        assert_eq!(transformed.width, 4);
        assert_eq!(transformed.height, 3);
        assert_eq!(transformed.data, source.data);
    }

    #[test]
    fn test_flip_horizontal() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::Horizontal, Rotation::None);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        assert_eq!(transformed.width, 4);
        assert_eq!(transformed.height, 3);

        // Check that pixel at (0,0) now contains what was at (3,0)
        let (r, g, _, _) = get_pixel(&transformed.data, 4, 0, 0);
        assert_eq!(r, 3); // Was at x=3
        assert_eq!(g, 0); // Same y

        // Check that pixel at (3,0) now contains what was at (0,0)
        let (r, g, _, _) = get_pixel(&transformed.data, 4, 3, 0);
        assert_eq!(r, 0); // Was at x=0
        assert_eq!(g, 0); // Same y
    }

    #[test]
    fn test_flip_vertical() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::Vertical, Rotation::None);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        assert_eq!(transformed.width, 4);
        assert_eq!(transformed.height, 3);

        // Check that pixel at (0,0) now contains what was at (0,2)
        let (r, g, _, _) = get_pixel(&transformed.data, 4, 0, 0);
        assert_eq!(r, 0); // Same x
        assert_eq!(g, 2); // Was at y=2

        // Check that pixel at (0,2) now contains what was at (0,0)
        let (r, g, _, _) = get_pixel(&transformed.data, 4, 0, 2);
        assert_eq!(r, 0); // Same x
        assert_eq!(g, 0); // Was at y=0
    }

    #[test]
    fn test_flip_both() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::Both, Rotation::None);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        // Check that pixel at (0,0) now contains what was at (3,2)
        let (r, g, _, _) = get_pixel(&transformed.data, 4, 0, 0);
        assert_eq!(r, 3); // Was at x=3
        assert_eq!(g, 2); // Was at y=2
    }

    #[test]
    fn test_rotate_90() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::None, Rotation::Clockwise90);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        // 90° clockwise: dimensions swap
        assert_eq!(transformed.width, 3);  // Was height
        assert_eq!(transformed.height, 4); // Was width

        // Original (0,0) goes to (2, 0) in new orientation
        // Original (3,0) goes to (2, 3) in new orientation
        // Original (0,2) goes to (0, 0) in new orientation
        let (r, g, _, _) = get_pixel(&transformed.data, 3, 0, 0);
        assert_eq!(r, 0); // Was at x=0
        assert_eq!(g, 2); // Was at y=2 (bottom-left corner goes to top-left)
    }

    #[test]
    fn test_rotate_180() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::None, Rotation::Clockwise180);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        assert_eq!(transformed.width, 4);
        assert_eq!(transformed.height, 3);

        // Original (0,0) goes to (3, 2)
        let (r, g, _, _) = get_pixel(&transformed.data, 4, 0, 0);
        assert_eq!(r, 3); // Was at x=3
        assert_eq!(g, 2); // Was at y=2
    }

    #[test]
    fn test_rotate_270() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::None, Rotation::Clockwise270);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        // 270° clockwise: dimensions swap
        assert_eq!(transformed.width, 3);
        assert_eq!(transformed.height, 4);

        // Original (0,0) goes to (0, 3) in 270° rotation
        let (r, g, _, _) = get_pixel(&transformed.data, 3, 0, 0);
        assert_eq!(r, 3); // Was at x=3
        assert_eq!(g, 0); // Was at y=0
    }

    #[test]
    fn test_round_trip_rotation() {
        // Rotating 4 times should return to original
        let source = make_test_frame(4, 3);
        
        let r1 = transform_frame(&source, Flip::None, Rotation::Clockwise90).unwrap();
        let frame1 = DecodedFrame { data: r1.data, width: r1.width, height: r1.height };
        
        let r2 = transform_frame(&frame1, Flip::None, Rotation::Clockwise90).unwrap();
        let frame2 = DecodedFrame { data: r2.data, width: r2.width, height: r2.height };
        
        let r3 = transform_frame(&frame2, Flip::None, Rotation::Clockwise90).unwrap();
        let frame3 = DecodedFrame { data: r3.data, width: r3.width, height: r3.height };
        
        let r4 = transform_frame(&frame3, Flip::None, Rotation::Clockwise90).unwrap();

        assert_eq!(r4.width, source.width);
        assert_eq!(r4.height, source.height);
        assert_eq!(r4.data, source.data);
    }

    #[test]
    fn test_round_trip_horizontal_flip() {
        // Flipping horizontally twice should return to original
        let source = make_test_frame(4, 3);
        
        let r1 = transform_frame(&source, Flip::Horizontal, Rotation::None).unwrap();
        let frame1 = DecodedFrame { data: r1.data, width: r1.width, height: r1.height };
        
        let r2 = transform_frame(&frame1, Flip::Horizontal, Rotation::None).unwrap();

        assert_eq!(r2.data, source.data);
    }

    #[test]
    fn test_round_trip_vertical_flip() {
        // Flipping vertically twice should return to original
        let source = make_test_frame(4, 3);
        
        let r1 = transform_frame(&source, Flip::Vertical, Rotation::None).unwrap();
        let frame1 = DecodedFrame { data: r1.data, width: r1.width, height: r1.height };
        
        let r2 = transform_frame(&frame1, Flip::Vertical, Rotation::None).unwrap();

        assert_eq!(r2.data, source.data);
    }

    #[test]
    fn test_combined_flip_and_rotate() {
        let source = make_test_frame(4, 3);
        let result = transform_frame(&source, Flip::Horizontal, Rotation::Clockwise90);
        assert!(result.is_ok());

        let transformed = result.unwrap();
        assert_eq!(transformed.width, 3);
        assert_eq!(transformed.height, 4);
    }

    #[test]
    fn test_invalid_dimensions() {
        let source = DecodedFrame { data: vec![], width: 0, height: 100 };
        let result = transform_frame(&source, Flip::None, Rotation::None);
        assert!(matches!(result, Err(TransformError::InvalidDimensions { .. })));
    }

    #[test]
    fn test_all_8_combinations() {
        // Test all 4 rotations × 2 flip states (no flip vs horizontal)
        let source = make_test_frame(4, 3);
        let flips = [Flip::None, Flip::Horizontal, Flip::Vertical, Flip::Both];
        let rotations = [Rotation::None, Rotation::Clockwise90, Rotation::Clockwise180, Rotation::Clockwise270];

        for flip in &flips {
            for rotation in &rotations {
                let result = transform_frame(&source, *flip, *rotation);
                assert!(result.is_ok(), "Failed for flip={:?}, rotation={:?}", flip, rotation);
                
                let transformed = result.unwrap();
                // Verify dimensions are correct
                match rotation {
                    Rotation::None | Rotation::Clockwise180 => {
                        assert_eq!(transformed.width, source.width);
                        assert_eq!(transformed.height, source.height);
                    }
                    Rotation::Clockwise90 | Rotation::Clockwise270 => {
                        assert_eq!(transformed.width, source.height);
                        assert_eq!(transformed.height, source.width);
                    }
                }
            }
        }
    }
}
