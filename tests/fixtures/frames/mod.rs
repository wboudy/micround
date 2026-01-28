//! Test frame fixtures
//!
//! Provides sample frames in various formats for testing the processing pipeline.
//! Frames are generated programmatically to ensure reproducibility and avoid
//! storing large binary files in git.
//!
//! # Available Fixtures
//!
//! - **Test patterns**: Color bars, gradients, checkerboards
//! - **Formats**: RGBA, RGB24, YUYV, NV12, MJPEG
//! - **Resolutions**: 640x480, 1280x720, 1920x1080
//!
//! # Usage
//!
//! ```ignore
//! use fixtures::frames::*;
//!
//! let frame = rgba_test_pattern_640x480();
//! let frame = yuyv_gradient_1280x720();
//! let frame = nv12_checkerboard_1920x1080();
//! ```

use micround::core::{Frame, PixelFormat};

// ============================================================================
// Frame Metadata
// ============================================================================

/// Metadata about a generated frame fixture
#[derive(Debug, Clone)]
pub struct FrameFixtureMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub checksum: Option<u32>,
}

// ============================================================================
// RGBA Test Frames (32-bit, 4 bytes per pixel)
// ============================================================================

/// RGBA 640x480 color bars test pattern
///
/// Classic SMPTE-style color bars used for display calibration.
/// Pattern: White, Yellow, Cyan, Green, Magenta, Red, Blue, Black
pub fn rgba_color_bars_640x480() -> Frame {
    let width = 640u32;
    let height = 480u32;
    let bar_width = width / 8;

    let colors: [[u8; 4]; 8] = [
        [255, 255, 255, 255], // White
        [255, 255, 0, 255],   // Yellow
        [0, 255, 255, 255],   // Cyan
        [0, 255, 0, 255],     // Green
        [255, 0, 255, 255],   // Magenta
        [255, 0, 0, 255],     // Red
        [0, 0, 255, 255],     // Blue
        [0, 0, 0, 255],       // Black
    ];

    let mut data = Vec::with_capacity((width * height * 4) as usize);

    for _y in 0..height {
        for x in 0..width {
            let bar_index = (x / bar_width).min(7) as usize;
            data.extend_from_slice(&colors[bar_index]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Rgba32,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// RGBA 1280x720 horizontal gradient
///
/// Smooth gradient from black (left) to white (right).
/// Useful for testing color interpolation and scaling artifacts.
pub fn rgba_gradient_1280x720() -> Frame {
    let width = 1280u32;
    let height = 720u32;

    let mut data = Vec::with_capacity((width * height * 4) as usize);

    for _y in 0..height {
        for x in 0..width {
            let value = ((x as f32 / (width - 1) as f32) * 255.0) as u8;
            data.extend_from_slice(&[value, value, value, 255]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Rgba32,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// RGBA 1920x1080 checkerboard pattern
///
/// 64x64 pixel blocks alternating black and white.
/// Useful for testing scaling, rotation, and edge detection.
pub fn rgba_checkerboard_1920x1080() -> Frame {
    let width = 1920u32;
    let height = 1080u32;
    let block_size = 64u32;

    let mut data = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height {
        for x in 0..width {
            let block_x = x / block_size;
            let block_y = y / block_size;
            let value = if (block_x + block_y) % 2 == 0 { 255u8 } else { 0u8 };
            data.extend_from_slice(&[value, value, value, 255]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Rgba32,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// RGBA frame with known pixel values for precise testing
///
/// Each corner has a distinct color, center is gray.
/// Useful for testing transformations (flip, rotate).
pub fn rgba_corner_markers_100x100() -> Frame {
    let width = 100u32;
    let height = 100u32;

    let mut data = vec![128u8; (width * height * 4) as usize]; // Gray background

    // Mark corners with distinct colors
    let set_pixel = |data: &mut [u8], x: u32, y: u32, r: u8, g: u8, b: u8| {
        let idx = ((y * width + x) * 4) as usize;
        data[idx] = r;
        data[idx + 1] = g;
        data[idx + 2] = b;
        data[idx + 3] = 255;
    };

    // 5x5 pixel markers in each corner
    for dy in 0..5 {
        for dx in 0..5 {
            // Top-left: Red
            set_pixel(&mut data, dx, dy, 255, 0, 0);
            // Top-right: Green
            set_pixel(&mut data, width - 1 - dx, dy, 0, 255, 0);
            // Bottom-left: Blue
            set_pixel(&mut data, dx, height - 1 - dy, 0, 0, 255);
            // Bottom-right: Yellow
            set_pixel(&mut data, width - 1 - dx, height - 1 - dy, 255, 255, 0);
        }
    }

    Frame {
        data,
        format: PixelFormat::Rgba32,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

// ============================================================================
// YUYV Test Frames (16-bit, 2 bytes per pixel, packed YUV 4:2:2)
// ============================================================================

/// Convert RGB to YUV (BT.601)
fn rgb_to_yuv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let r = r as f32;
    let g = g as f32;
    let b = b as f32;

    let y = (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 255.0) as u8;
    let u = (128.0 - 0.169 * r - 0.331 * g + 0.5 * b).clamp(0.0, 255.0) as u8;
    let v = (128.0 + 0.5 * r - 0.419 * g - 0.081 * b).clamp(0.0, 255.0) as u8;

    (y, u, v)
}

/// YUYV 640x480 color bars
///
/// YUYV packs two pixels as Y0 U Y1 V (4 bytes for 2 pixels).
/// U and V are shared between adjacent horizontal pixels.
pub fn yuyv_color_bars_640x480() -> Frame {
    let width = 640u32;
    let height = 480u32;
    let bar_width = width / 8;

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

    // Convert to YUV
    let yuv_colors: Vec<(u8, u8, u8)> = colors
        .iter()
        .map(|&(r, g, b)| rgb_to_yuv(r, g, b))
        .collect();

    // YUYV: 2 bytes per pixel
    let mut data = Vec::with_capacity((width * height * 2) as usize);

    for _y in 0..height {
        for x in (0..width).step_by(2) {
            let bar_index0 = (x / bar_width).min(7) as usize;
            let bar_index1 = ((x + 1) / bar_width).min(7) as usize;

            let (y0, u0, v0) = yuv_colors[bar_index0];
            let (y1, u1, v1) = yuv_colors[bar_index1];

            // Average U and V for the pixel pair
            let u = ((u0 as u16 + u1 as u16) / 2) as u8;
            let v = ((v0 as u16 + v1 as u16) / 2) as u8;

            data.extend_from_slice(&[y0, u, y1, v]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Yuyv,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// YUYV 1280x720 gradient
pub fn yuyv_gradient_1280x720() -> Frame {
    let width = 1280u32;
    let height = 720u32;

    let mut data = Vec::with_capacity((width * height * 2) as usize);

    for _y in 0..height {
        for x in (0..width).step_by(2) {
            // Grayscale gradient - Y varies, U=V=128
            let y0 = ((x as f32 / (width - 1) as f32) * 255.0) as u8;
            let y1 = (((x + 1) as f32 / (width - 1) as f32) * 255.0) as u8;

            data.extend_from_slice(&[y0, 128, y1, 128]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Yuyv,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// YUYV with odd width (edge case)
///
/// Tests handling of non-even widths which require padding in YUYV.
pub fn yuyv_odd_width_641x480() -> Frame {
    let width = 641u32; // Odd!
    let height = 480u32;

    // For YUYV, we need even width - pad to 642
    let padded_width = (width + 1) & !1;

    let mut data = Vec::with_capacity((padded_width * height * 2) as usize);

    for _y in 0..height {
        for x in (0..padded_width).step_by(2) {
            let y0 = if x < width { 128 } else { 0 };
            let y1 = if x + 1 < width { 128 } else { 0 };
            data.extend_from_slice(&[y0, 128, y1, 128]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Yuyv,
        width: padded_width, // Actual data width is padded
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

// ============================================================================
// NV12 Test Frames (12 bits per pixel, planar Y + interleaved UV)
// ============================================================================

/// NV12 640x480 color bars
///
/// NV12 layout:
/// - Y plane: width * height bytes (full resolution)
/// - UV plane: width * height / 2 bytes (half resolution, interleaved U V U V...)
pub fn nv12_color_bars_640x480() -> Frame {
    let width = 640u32;
    let height = 480u32;
    let bar_width = width / 8;

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

    let yuv_colors: Vec<(u8, u8, u8)> = colors
        .iter()
        .map(|&(r, g, b)| rgb_to_yuv(r, g, b))
        .collect();

    // Y plane (full resolution)
    let y_size = (width * height) as usize;
    // UV plane (half resolution in each dimension)
    let uv_size = (width * height / 2) as usize;

    let mut data = Vec::with_capacity(y_size + uv_size);

    // Y plane
    for _y in 0..height {
        for x in 0..width {
            let bar_index = (x / bar_width).min(7) as usize;
            let (y, _, _) = yuv_colors[bar_index];
            data.push(y);
        }
    }

    // UV plane (interleaved, half resolution)
    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let bar_index = (x / bar_width).min(7) as usize;
            let (_, u, v) = yuv_colors[bar_index];
            data.push(u);
            data.push(v);
        }
    }

    Frame {
        data,
        format: PixelFormat::Nv12,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// NV12 1920x1080 checkerboard
pub fn nv12_checkerboard_1920x1080() -> Frame {
    let width = 1920u32;
    let height = 1080u32;
    let block_size = 64u32;

    let y_size = (width * height) as usize;
    let uv_size = (width * height / 2) as usize;

    let mut data = Vec::with_capacity(y_size + uv_size);

    // Y plane
    for y in 0..height {
        for x in 0..width {
            let block_x = x / block_size;
            let block_y = y / block_size;
            let value = if (block_x + block_y) % 2 == 0 { 235 } else { 16 }; // Video range
            data.push(value);
        }
    }

    // UV plane (grayscale = 128 for both)
    for _ in 0..uv_size {
        data.push(128);
    }

    Frame {
        data,
        format: PixelFormat::Nv12,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

// ============================================================================
// RGB24 Test Frames (24-bit, 3 bytes per pixel)
// ============================================================================

/// RGB24 640x480 gradient
pub fn rgb24_gradient_640x480() -> Frame {
    let width = 640u32;
    let height = 480u32;

    let mut data = Vec::with_capacity((width * height * 3) as usize);

    for _y in 0..height {
        for x in 0..width {
            let value = ((x as f32 / (width - 1) as f32) * 255.0) as u8;
            data.extend_from_slice(&[value, value, value]);
        }
    }

    Frame {
        data,
        format: PixelFormat::Rgb24,
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

// ============================================================================
// Corrupted/Edge Case Frames
// ============================================================================

/// Frame with truncated data (for error handling tests)
pub fn corrupted_truncated_frame() -> Frame {
    Frame {
        data: vec![0u8; 100], // Way too small for 640x480
        format: PixelFormat::Rgba32,
        width: 640,
        height: 480,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// Frame with zero dimensions
pub fn corrupted_zero_dimensions() -> Frame {
    Frame {
        data: vec![],
        format: PixelFormat::Rgba32,
        width: 0,
        height: 0,
        timestamp_ns: 0,
        sequence: 0,
    }
}

/// Frame with mismatched format (data doesn't match declared format)
pub fn corrupted_format_mismatch() -> Frame {
    // RGBA32 needs 4 bytes per pixel, but we provide 3
    let width = 100u32;
    let height = 100u32;
    Frame {
        data: vec![128u8; (width * height * 3) as usize], // RGB data
        format: PixelFormat::Rgba32, // But claiming RGBA
        width,
        height,
        timestamp_ns: 0,
        sequence: 0,
    }
}

// ============================================================================
// Frame Factory Functions
// ============================================================================

/// Get a frame by name for dynamic fixture loading
pub fn get_frame_by_name(name: &str) -> Option<Frame> {
    match name {
        "rgba_color_bars_640x480" => Some(rgba_color_bars_640x480()),
        "rgba_gradient_1280x720" => Some(rgba_gradient_1280x720()),
        "rgba_checkerboard_1920x1080" => Some(rgba_checkerboard_1920x1080()),
        "rgba_corner_markers_100x100" => Some(rgba_corner_markers_100x100()),
        "yuyv_color_bars_640x480" => Some(yuyv_color_bars_640x480()),
        "yuyv_gradient_1280x720" => Some(yuyv_gradient_1280x720()),
        "yuyv_odd_width_641x480" => Some(yuyv_odd_width_641x480()),
        "nv12_color_bars_640x480" => Some(nv12_color_bars_640x480()),
        "nv12_checkerboard_1920x1080" => Some(nv12_checkerboard_1920x1080()),
        "rgb24_gradient_640x480" => Some(rgb24_gradient_640x480()),
        "corrupted_truncated" => Some(corrupted_truncated_frame()),
        "corrupted_zero_dimensions" => Some(corrupted_zero_dimensions()),
        "corrupted_format_mismatch" => Some(corrupted_format_mismatch()),
        _ => None,
    }
}

/// List all available frame fixtures
pub fn list_frame_fixtures() -> Vec<FrameFixtureMeta> {
    vec![
        FrameFixtureMeta {
            name: "rgba_color_bars_640x480",
            description: "SMPTE color bars in RGBA32",
            width: 640,
            height: 480,
            format: PixelFormat::Rgba32,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "rgba_gradient_1280x720",
            description: "Horizontal grayscale gradient",
            width: 1280,
            height: 720,
            format: PixelFormat::Rgba32,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "rgba_checkerboard_1920x1080",
            description: "64x64 black/white checkerboard",
            width: 1920,
            height: 1080,
            format: PixelFormat::Rgba32,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "rgba_corner_markers_100x100",
            description: "Corner color markers for transform testing",
            width: 100,
            height: 100,
            format: PixelFormat::Rgba32,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "yuyv_color_bars_640x480",
            description: "Color bars in YUYV format",
            width: 640,
            height: 480,
            format: PixelFormat::Yuyv,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "yuyv_gradient_1280x720",
            description: "Grayscale gradient in YUYV",
            width: 1280,
            height: 720,
            format: PixelFormat::Yuyv,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "nv12_color_bars_640x480",
            description: "Color bars in NV12 format",
            width: 640,
            height: 480,
            format: PixelFormat::Nv12,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "nv12_checkerboard_1920x1080",
            description: "Checkerboard in NV12 format",
            width: 1920,
            height: 1080,
            format: PixelFormat::Nv12,
            checksum: None,
        },
        FrameFixtureMeta {
            name: "rgb24_gradient_640x480",
            description: "Grayscale gradient in RGB24",
            width: 640,
            height: 480,
            format: PixelFormat::Rgb24,
            checksum: None,
        },
    ]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_color_bars_dimensions() {
        let frame = rgba_color_bars_640x480();
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert_eq!(frame.format, PixelFormat::Rgba32);
        assert_eq!(frame.data.len(), 640 * 480 * 4);
    }

    #[test]
    fn test_rgba_checkerboard_pattern() {
        let frame = rgba_checkerboard_1920x1080();
        // First pixel (0,0) should be white
        assert_eq!(frame.data[0], 255);
        // Pixel at (64,0) should be black (second block)
        assert_eq!(frame.data[64 * 4], 0);
    }

    #[test]
    fn test_yuyv_data_size() {
        let frame = yuyv_color_bars_640x480();
        // YUYV: 2 bytes per pixel
        assert_eq!(frame.data.len(), 640 * 480 * 2);
    }

    #[test]
    fn test_nv12_data_size() {
        let frame = nv12_color_bars_640x480();
        // NV12: Y plane (w*h) + UV plane (w*h/2)
        let expected = 640 * 480 + 640 * 480 / 2;
        assert_eq!(frame.data.len(), expected);
    }

    #[test]
    fn test_corner_markers() {
        let frame = rgba_corner_markers_100x100();
        // Top-left should be red
        assert_eq!(&frame.data[0..4], &[255, 0, 0, 255]);
        // Top-right should be green
        let top_right_idx = (99 * 4) as usize;
        assert_eq!(&frame.data[top_right_idx..top_right_idx + 4], &[0, 255, 0, 255]);
    }

    #[test]
    fn test_get_frame_by_name() {
        assert!(get_frame_by_name("rgba_color_bars_640x480").is_some());
        assert!(get_frame_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_list_fixtures() {
        let fixtures = list_frame_fixtures();
        assert!(fixtures.len() >= 9);
    }
}
