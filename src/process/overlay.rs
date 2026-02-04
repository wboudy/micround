//! Overlay compositing system
//!
//! Renders text and graphics overlays onto processed frames.
//! Supports timestamp, custom text, and debug information.
//!
//! # Overlay Types
//!
//! - **Timestamp**: Current date/time with configurable format
//! - **CustomText**: User-defined string
//! - **FrameCounter**: Sequence number (debug mode)
//!
//! # Positioning
//!
//! Overlays use a 9-point grid for positioning:
//!
//! ```text
//! ┌────────────────────────┐
//! │ TL      TC      TR     │
//! │                        │
//! │ ML      MC      MR     │
//! │                        │
//! │ BL      BC      BR     │
//! └────────────────────────┘
//! ```
//!
//! # Text Rendering
//!
//! This module includes a basic 7-segment style digit renderer for timestamps
//! and a simple character set for basic text. For production use with full
//! font support, consider adding fontdue or ab_glyph dependencies.

use std::time::{SystemTime, UNIX_EPOCH};

/// Position on a 9-point grid
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayPosition {
    TopLeft,
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    #[default]
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl OverlayPosition {
    /// Calculate the pixel coordinates for an overlay of given size
    ///
    /// # Arguments
    /// * `frame_width` - Width of the frame
    /// * `frame_height` - Height of the frame
    /// * `overlay_width` - Width of the overlay content
    /// * `overlay_height` - Height of the overlay content
    /// * `padding` - Padding from the edge
    ///
    /// # Returns
    /// (x, y) coordinates for the top-left of the overlay
    pub fn calculate_position(
        &self,
        frame_width: u32,
        frame_height: u32,
        overlay_width: u32,
        overlay_height: u32,
        padding: u32,
    ) -> (u32, u32) {
        let x = match self {
            Self::TopLeft | Self::MiddleLeft | Self::BottomLeft => padding,
            Self::TopCenter | Self::MiddleCenter | Self::BottomCenter => {
                (frame_width.saturating_sub(overlay_width)) / 2
            }
            Self::TopRight | Self::MiddleRight | Self::BottomRight => {
                frame_width.saturating_sub(overlay_width + padding)
            }
        };

        let y = match self {
            Self::TopLeft | Self::TopCenter | Self::TopRight => padding,
            Self::MiddleLeft | Self::MiddleCenter | Self::MiddleRight => {
                (frame_height.saturating_sub(overlay_height)) / 2
            }
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight => {
                frame_height.saturating_sub(overlay_height + padding)
            }
        };

        (x, y)
    }
}

/// RGBA color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::new(r, g, b, a)
    }

    // Common colors
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const RED: Self = Self::rgb(255, 0, 0);
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    pub const BLUE: Self = Self::rgb(0, 0, 255);
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

/// Text size preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSize {
    Small, // ~12px equivalent
    #[default]
    Medium, // ~16px equivalent
    Large, // ~24px equivalent
}

impl TextSize {
    /// Get the pixel height for this size
    pub fn pixel_height(&self) -> u32 {
        match self {
            Self::Small => 12,
            Self::Medium => 16,
            Self::Large => 24,
        }
    }

    /// Get the character width for this size (monospace)
    pub fn char_width(&self) -> u32 {
        match self {
            Self::Small => 7,
            Self::Medium => 10,
            Self::Large => 14,
        }
    }
}

/// Style configuration for overlays
#[derive(Debug, Clone)]
pub struct OverlayStyle {
    /// Text/foreground color
    pub color: Color,
    /// Background/shadow color
    pub shadow_color: Color,
    /// Text size
    pub size: TextSize,
    /// Opacity (0-255)
    pub opacity: u8,
    /// Padding from edge in pixels
    pub padding: u32,
    /// Whether to draw a shadow/outline for visibility
    pub draw_shadow: bool,
    /// Shadow offset in pixels
    pub shadow_offset: u32,
}

impl Default for OverlayStyle {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            shadow_color: Color::rgba(0, 0, 0, 180),
            size: TextSize::Medium,
            opacity: 200,
            padding: 10,
            draw_shadow: true,
            shadow_offset: 1,
        }
    }
}

/// Type of overlay content
#[derive(Debug, Clone)]
pub enum OverlayContent {
    /// Current timestamp with format string
    /// Format: %H:%M:%S for time, %Y-%m-%d for date
    Timestamp { format: String },
    /// Custom user text
    Text { content: String },
    /// Frame counter (for debugging)
    FrameCounter { current: u64 },
    /// Combined (multiple pieces of info)
    Combined { parts: Vec<OverlayContent> },
}

impl Default for OverlayContent {
    fn default() -> Self {
        Self::Timestamp {
            format: "%H:%M:%S".into(),
        }
    }
}

impl OverlayContent {
    /// Create a timestamp overlay with default format
    pub fn timestamp() -> Self {
        Self::Timestamp {
            format: "%H:%M:%S".into(),
        }
    }

    /// Create a timestamp with date
    pub fn timestamp_with_date() -> Self {
        Self::Timestamp {
            format: "%Y-%m-%d %H:%M:%S".into(),
        }
    }

    /// Create custom text overlay
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text { content: s.into() }
    }

    /// Create frame counter overlay
    pub fn frame_counter(current: u64) -> Self {
        Self::FrameCounter { current }
    }

    /// Render this content to a string
    pub fn render_string(&self) -> String {
        match self {
            Self::Timestamp { format } => format_timestamp(format),
            Self::Text { content } => content.clone(),
            Self::FrameCounter { current } => format!("Frame: {}", current),
            Self::Combined { parts } => parts
                .iter()
                .map(|p| p.render_string())
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }
}

/// A complete overlay definition
#[derive(Debug, Clone)]
pub struct Overlay {
    /// Content to display
    pub content: OverlayContent,
    /// Position on the frame
    pub position: OverlayPosition,
    /// Visual style
    pub style: OverlayStyle,
    /// Whether this overlay is enabled
    pub enabled: bool,
}

impl Default for Overlay {
    fn default() -> Self {
        Self {
            content: OverlayContent::default(),
            position: OverlayPosition::default(),
            style: OverlayStyle::default(),
            enabled: true,
        }
    }
}

impl Overlay {
    /// Create a new overlay with given content and position
    pub fn new(content: OverlayContent, position: OverlayPosition) -> Self {
        Self {
            content,
            position,
            ..Default::default()
        }
    }

    /// Builder: set style
    pub fn with_style(mut self, style: OverlayStyle) -> Self {
        self.style = style;
        self
    }

    /// Builder: set enabled state
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Overlay configuration for the processing pipeline
#[derive(Debug, Clone, Default)]
pub struct OverlayConfig {
    /// List of overlays to render
    pub overlays: Vec<Overlay>,
    /// Global enable/disable
    pub enabled: bool,
}

impl OverlayConfig {
    /// Create a new config with no overlays
    pub fn new() -> Self {
        Self {
            overlays: Vec::new(),
            enabled: true,
        }
    }

    /// Create a config with a timestamp overlay
    pub fn with_timestamp() -> Self {
        Self {
            overlays: vec![Overlay::new(
                OverlayContent::timestamp(),
                OverlayPosition::BottomLeft,
            )],
            enabled: true,
        }
    }

    /// Add an overlay
    pub fn add_overlay(&mut self, overlay: Overlay) {
        self.overlays.push(overlay);
    }
}

/// Composite overlays onto a frame
///
/// # Arguments
/// * `frame_data` - Mutable RGBA frame data
/// * `width` - Frame width
/// * `height` - Frame height
/// * `config` - Overlay configuration
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(OverlayError)` if compositing fails
pub fn composite_overlays(
    frame_data: &mut [u8],
    width: u32,
    height: u32,
    config: &OverlayConfig,
) -> Result<(), OverlayError> {
    if !config.enabled || config.overlays.is_empty() {
        return Ok(());
    }

    let expected_size = (width * height * 4) as usize;
    if frame_data.len() < expected_size {
        return Err(OverlayError::BufferTooSmall {
            expected: expected_size,
            actual: frame_data.len(),
        });
    }

    for overlay in &config.overlays {
        if !overlay.enabled {
            continue;
        }

        let text = overlay.content.render_string();
        if text.is_empty() {
            continue;
        }

        render_text_overlay(
            frame_data,
            width,
            height,
            &text,
            &overlay.position,
            &overlay.style,
        );
    }

    Ok(())
}

/// Errors that can occur during overlay compositing
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("Buffer too small: expected {expected}, got {actual}")]
    BufferTooSmall { expected: usize, actual: usize },

    #[error("Invalid overlay configuration: {0}")]
    InvalidConfig(String),
}

// ============================================================================
// Internal Text Rendering
// ============================================================================

/// Format a timestamp using a simplified format string
///
/// # Note
/// This implementation displays UTC time. For local time support,
/// consider adding the `chrono` crate dependency. This keeps the
/// binary smaller for users who don't need timestamp overlays.
fn format_timestamp(format: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    let total_secs = now.as_secs();
    let hours = (total_secs / 3600) % 24;
    let minutes = (total_secs / 60) % 60;
    let seconds = total_secs % 60;

    // Calculate date components
    let days_since_epoch = total_secs / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch as i64);

    // Simple format string replacement
    format
        .replace("%H", &format!("{:02}", hours))
        .replace("%M", &format!("{:02}", minutes))
        .replace("%S", &format!("{:02}", seconds))
        .replace("%Y", &format!("{:04}", year))
        .replace("%m", &format!("{:02}", month))
        .replace("%d", &format!("{:02}", day))
}

/// Convert days since epoch to (year, month, day)
fn days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Simplified algorithm - good enough for display purposes
    let remaining = days + 719468; // Days from year 0 to epoch
    let era = remaining / 146097;
    let doe = remaining - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year as i32, m as u32, d as u32)
}

/// Render a text overlay onto the frame
fn render_text_overlay(
    frame_data: &mut [u8],
    width: u32,
    height: u32,
    text: &str,
    position: &OverlayPosition,
    style: &OverlayStyle,
) {
    let char_width = style.size.char_width();
    let char_height = style.size.pixel_height();
    let text_width = text.len() as u32 * char_width;
    let text_height = char_height;

    let (x, y) = position.calculate_position(width, height, text_width, text_height, style.padding);

    // Draw shadow first if enabled
    if style.draw_shadow {
        let shadow_x = x + style.shadow_offset;
        let shadow_y = y + style.shadow_offset;
        render_text_at(
            frame_data,
            width,
            height,
            text,
            shadow_x,
            shadow_y,
            &style.shadow_color,
            style.opacity,
            &style.size,
        );
    }

    // Draw main text
    render_text_at(
        frame_data,
        width,
        height,
        text,
        x,
        y,
        &style.color,
        style.opacity,
        &style.size,
    );
}

/// Render text at specific coordinates
fn render_text_at(
    frame_data: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    text: &str,
    start_x: u32,
    start_y: u32,
    color: &Color,
    opacity: u8,
    size: &TextSize,
) {
    let char_width = size.char_width();
    let char_height = size.pixel_height();

    for (i, ch) in text.chars().enumerate() {
        let char_x = start_x + (i as u32 * char_width);
        render_char(
            frame_data,
            frame_width,
            frame_height,
            ch,
            char_x,
            start_y,
            char_width,
            char_height,
            color,
            opacity,
        );
    }
}

/// Render a single character using a simple bitmap font
fn render_char(
    frame_data: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    ch: char,
    x: u32,
    y: u32,
    char_width: u32,
    char_height: u32,
    color: &Color,
    opacity: u8,
) {
    let bitmap = get_char_bitmap(ch);
    let bitmap_height = bitmap.len() as u32;
    let bitmap_width = if bitmap.is_empty() {
        0
    } else {
        bitmap[0].len() as u32
    };

    // Scale bitmap to char dimensions
    for py in 0..char_height {
        for px in 0..char_width {
            // Map to bitmap coordinates
            let bx = (px * bitmap_width / char_width) as usize;
            let by = (py * bitmap_height / char_height) as usize;

            if by < bitmap.len() && bx < bitmap[by].len() && bitmap[by][bx] {
                let fx = x + px;
                let fy = y + py;

                if fx < frame_width && fy < frame_height {
                    let idx = ((fy * frame_width + fx) * 4) as usize;
                    if idx + 3 < frame_data.len() {
                        blend_pixel(&mut frame_data[idx..idx + 4], color, opacity);
                    }
                }
            }
        }
    }
}

/// Alpha-blend a color onto a pixel
fn blend_pixel(pixel: &mut [u8], color: &Color, opacity: u8) {
    let alpha = ((color.a as u32) * (opacity as u32)) / 255;
    let inv_alpha = 255 - alpha;

    pixel[0] = ((color.r as u32 * alpha + pixel[0] as u32 * inv_alpha) / 255) as u8;
    pixel[1] = ((color.g as u32 * alpha + pixel[1] as u32 * inv_alpha) / 255) as u8;
    pixel[2] = ((color.b as u32 * alpha + pixel[2] as u32 * inv_alpha) / 255) as u8;
    // Keep destination alpha
}

/// Get a simple bitmap for a character (5x7 monospace style)
fn get_char_bitmap(ch: char) -> &'static [&'static [bool]] {
    // Simple 5x7 bitmap font for common characters
    match ch {
        '0' => &[
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[true, false, false, true, true],
            &[true, false, true, false, true],
            &[true, true, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, false],
        ],
        '1' => &[
            &[false, false, true, false, false],
            &[false, true, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, true, true, true, false],
        ],
        '2' => &[
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[false, false, false, false, true],
            &[false, false, true, true, false],
            &[false, true, false, false, false],
            &[true, false, false, false, false],
            &[true, true, true, true, true],
        ],
        '3' => &[
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[false, false, false, false, true],
            &[false, false, true, true, false],
            &[false, false, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, false],
        ],
        '4' => &[
            &[false, false, false, true, false],
            &[false, false, true, true, false],
            &[false, true, false, true, false],
            &[true, false, false, true, false],
            &[true, true, true, true, true],
            &[false, false, false, true, false],
            &[false, false, false, true, false],
        ],
        '5' => &[
            &[true, true, true, true, true],
            &[true, false, false, false, false],
            &[true, true, true, true, false],
            &[false, false, false, false, true],
            &[false, false, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, false],
        ],
        '6' => &[
            &[false, false, true, true, false],
            &[false, true, false, false, false],
            &[true, false, false, false, false],
            &[true, true, true, true, false],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, false],
        ],
        '7' => &[
            &[true, true, true, true, true],
            &[false, false, false, false, true],
            &[false, false, false, true, false],
            &[false, false, true, false, false],
            &[false, true, false, false, false],
            &[false, true, false, false, false],
            &[false, true, false, false, false],
        ],
        '8' => &[
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, false],
        ],
        '9' => &[
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[false, true, true, true, true],
            &[false, false, false, false, true],
            &[false, false, false, true, false],
            &[false, true, true, false, false],
        ],
        ':' => &[
            &[false, false, false, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, false, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, false, false, false],
        ],
        '-' => &[
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[true, true, true, true, true],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
        ],
        ' ' => &[
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, false, false, false, false],
        ],
        '|' => &[
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
            &[false, false, true, false, false],
        ],
        'F' => &[
            &[true, true, true, true, true],
            &[true, false, false, false, false],
            &[true, false, false, false, false],
            &[true, true, true, true, false],
            &[true, false, false, false, false],
            &[true, false, false, false, false],
            &[true, false, false, false, false],
        ],
        'r' => &[
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[true, false, true, true, false],
            &[true, true, false, false, true],
            &[true, false, false, false, false],
            &[true, false, false, false, false],
            &[true, false, false, false, false],
        ],
        'a' => &[
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, true, true, true, false],
            &[false, false, false, false, true],
            &[false, true, true, true, true],
            &[true, false, false, false, true],
            &[false, true, true, true, true],
        ],
        'm' => &[
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[true, true, false, true, false],
            &[true, false, true, false, true],
            &[true, false, true, false, true],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
        ],
        'e' => &[
            &[false, false, false, false, false],
            &[false, false, false, false, false],
            &[false, true, true, true, false],
            &[true, false, false, false, true],
            &[true, true, true, true, true],
            &[true, false, false, false, false],
            &[false, true, true, true, false],
        ],
        _ => &[
            // Default: filled rectangle for unknown chars
            &[true, true, true, true, true],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[true, false, false, false, true],
            &[true, true, true, true, true],
        ],
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_top_left() {
        let pos = OverlayPosition::TopLeft;
        let (x, y) = pos.calculate_position(800, 600, 100, 20, 10);
        assert_eq!(x, 10);
        assert_eq!(y, 10);
    }

    #[test]
    fn test_position_bottom_right() {
        let pos = OverlayPosition::BottomRight;
        let (x, y) = pos.calculate_position(800, 600, 100, 20, 10);
        assert_eq!(x, 690); // 800 - 100 - 10
        assert_eq!(y, 570); // 600 - 20 - 10
    }

    #[test]
    fn test_position_center() {
        let pos = OverlayPosition::MiddleCenter;
        let (x, y) = pos.calculate_position(800, 600, 100, 20, 10);
        assert_eq!(x, 350); // (800 - 100) / 2
        assert_eq!(y, 290); // (600 - 20) / 2
    }

    #[test]
    fn test_overlay_content_timestamp() {
        let content = OverlayContent::timestamp();
        let rendered = content.render_string();
        // Should be in format HH:MM:SS
        assert_eq!(rendered.len(), 8);
        assert!(rendered.contains(':'));
    }

    #[test]
    fn test_overlay_content_text() {
        let content = OverlayContent::text("Hello World");
        let rendered = content.render_string();
        assert_eq!(rendered, "Hello World");
    }

    #[test]
    fn test_overlay_content_frame_counter() {
        let content = OverlayContent::frame_counter(12345);
        let rendered = content.render_string();
        assert_eq!(rendered, "Frame: 12345");
    }

    #[test]
    fn test_color_creation() {
        let c = Color::rgb(255, 128, 64);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 128);
        assert_eq!(c.b, 64);
        assert_eq!(c.a, 255);

        let c2 = Color::rgba(100, 100, 100, 128);
        assert_eq!(c2.a, 128);
    }

    #[test]
    fn test_text_size_dimensions() {
        assert!(TextSize::Small.pixel_height() < TextSize::Medium.pixel_height());
        assert!(TextSize::Medium.pixel_height() < TextSize::Large.pixel_height());
    }

    #[test]
    fn test_composite_disabled() {
        let mut frame = vec![0u8; 100 * 100 * 4];
        let original = frame.clone();

        let config = OverlayConfig {
            overlays: vec![Overlay::default()],
            enabled: false,
        };

        let result = composite_overlays(&mut frame, 100, 100, &config);
        assert!(result.is_ok());
        // Frame should be unchanged
        assert_eq!(frame, original);
    }

    #[test]
    fn test_composite_empty_overlays() {
        let mut frame = vec![0u8; 100 * 100 * 4];
        let original = frame.clone();

        let config = OverlayConfig::new();

        let result = composite_overlays(&mut frame, 100, 100, &config);
        assert!(result.is_ok());
        assert_eq!(frame, original);
    }

    #[test]
    fn test_composite_buffer_too_small() {
        let mut frame = vec![0u8; 100]; // Way too small

        let config = OverlayConfig::with_timestamp();

        let result = composite_overlays(&mut frame, 100, 100, &config);
        assert!(matches!(result, Err(OverlayError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_composite_with_timestamp() {
        let mut frame = vec![128u8; 200 * 200 * 4]; // Gray background
        let config = OverlayConfig::with_timestamp();

        let result = composite_overlays(&mut frame, 200, 200, &config);
        assert!(result.is_ok());

        // Frame should be modified (some pixels different)
        let gray_count: usize = frame.chunks(4).filter(|p| p[0] == 128).count();
        assert!(gray_count < 200 * 200); // Not all pixels are gray anymore
    }

    #[test]
    fn test_blend_pixel() {
        let mut pixel = [100, 100, 100, 255];
        let color = Color::WHITE;
        blend_pixel(&mut pixel, &color, 128); // 50% opacity

        // Should be roughly halfway between 100 and 255
        assert!(pixel[0] > 150 && pixel[0] < 200);
    }

    #[test]
    fn test_char_bitmap_digits() {
        for d in '0'..='9' {
            let bitmap = get_char_bitmap(d);
            assert_eq!(bitmap.len(), 7); // 7 rows
            assert!(bitmap.iter().all(|row| row.len() == 5)); // 5 columns
        }
    }

    #[test]
    fn test_overlay_builder() {
        let overlay = Overlay::new(OverlayContent::text("Test"), OverlayPosition::TopRight)
            .with_style(OverlayStyle {
                color: Color::RED,
                ..Default::default()
            })
            .enabled(true);

        assert_eq!(overlay.position, OverlayPosition::TopRight);
        assert_eq!(overlay.style.color, Color::RED);
        assert!(overlay.enabled);
    }

    #[test]
    fn test_format_timestamp() {
        let result = format_timestamp("%H:%M:%S");
        // Should produce something like "HH:MM:SS"
        assert_eq!(result.len(), 8);
        assert_eq!(&result[2..3], ":");
        assert_eq!(&result[5..6], ":");
    }

    #[test]
    fn test_overlay_all_positions() {
        let positions = [
            OverlayPosition::TopLeft,
            OverlayPosition::TopCenter,
            OverlayPosition::TopRight,
            OverlayPosition::MiddleLeft,
            OverlayPosition::MiddleCenter,
            OverlayPosition::MiddleRight,
            OverlayPosition::BottomLeft,
            OverlayPosition::BottomCenter,
            OverlayPosition::BottomRight,
        ];

        for pos in positions {
            let (x, y) = pos.calculate_position(800, 600, 100, 20, 10);
            assert!(x < 800);
            assert!(y < 600);
        }
    }
}
