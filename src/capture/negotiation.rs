//! Format negotiation for camera streams
//!
//! Implements the algorithm for selecting the best available capture format
//! when the exact requested settings aren't available.

use crate::core::{CameraCapability, CaptureSettings, NegotiatedFormat, PixelFormat};

/// Priority order for pixel formats (lower is better)
/// MJPEG is preferred for hardware decode efficiency
fn format_priority(format: PixelFormat) -> u32 {
    match format {
        PixelFormat::Mjpeg => 0,    // Best: hardware decode possible
        PixelFormat::Yuyv => 1,     // Good: common, efficient
        PixelFormat::Nv12 => 2,     // Good: common on some hardware
        PixelFormat::Rgb24 => 3,    // OK: no conversion needed for display
        PixelFormat::Rgba32 => 4,   // OK: larger but display-ready
        PixelFormat::Unknown => 99, // Avoid if possible
    }
}

/// Calculate how well a capability matches the requested settings
/// Lower score is better
#[derive(Debug, Clone, Copy, PartialEq)]
struct MatchScore {
    /// Exact resolution match
    exact_resolution: bool,
    /// Exact format match (if format was specified)
    exact_format: bool,
    /// Difference in total pixels (lower is better)
    pixel_diff: i64,
    /// Format priority score
    format_priority: u32,
    /// Framerate difference
    fps_diff: f32,
}

impl MatchScore {
    fn calculate(cap: &CameraCapability, settings: &CaptureSettings) -> Self {
        let requested_pixels = (settings.width * settings.height) as i64;
        let actual_pixels = (cap.width * cap.height) as i64;

        Self {
            exact_resolution: cap.width == settings.width && cap.height == settings.height,
            exact_format: settings.format.is_none_or(|f| f == cap.format),
            pixel_diff: (actual_pixels - requested_pixels).abs(),
            format_priority: format_priority(cap.format),
            fps_diff: (cap.framerate - settings.framerate).abs(),
        }
    }

    /// Compare two scores - returns true if self is better than other
    fn is_better_than(&self, other: &MatchScore) -> bool {
        // Priority 1: Exact resolution match
        if self.exact_resolution != other.exact_resolution {
            return self.exact_resolution;
        }

        // Priority 2: Exact format match (if specified)
        if self.exact_format != other.exact_format {
            return self.exact_format;
        }

        // Priority 3: Prefer better format priority
        if self.format_priority != other.format_priority {
            return self.format_priority < other.format_priority;
        }

        // Priority 4: Closer to requested resolution
        if self.pixel_diff != other.pixel_diff {
            return self.pixel_diff < other.pixel_diff;
        }

        // Priority 5: Closer framerate
        self.fps_diff < other.fps_diff
    }
}

/// Negotiate the best available format for the given settings
///
/// # Format Priority (for microscopes)
/// 1. MJPEG at requested resolution (hardware decode possible)
/// 2. YUYV/YUY2 at requested resolution (common, efficient)
/// 3. Any format at requested resolution
/// 4. MJPEG at closest resolution
/// 5. YUYV at closest resolution
/// 6. Any format at closest resolution (last resort)
///
/// # Returns
/// - `Some(NegotiatedFormat)` if a suitable format was found
/// - `None` if no capabilities are available
pub fn negotiate_format(
    capabilities: &[CameraCapability],
    settings: &CaptureSettings,
) -> Option<NegotiatedFormat> {
    if capabilities.is_empty() {
        return None;
    }

    // Score all capabilities and pick the best
    // This considers both exact matches AND format priority
    let mut best_cap = &capabilities[0];
    let mut best_score = MatchScore::calculate(best_cap, settings);

    for cap in &capabilities[1..] {
        let score = MatchScore::calculate(cap, settings);
        if score.is_better_than(&best_score) {
            best_cap = cap;
            best_score = score;
        }
    }

    // Determine if this is an exact match
    let is_exact = best_cap.width == settings.width
        && best_cap.height == settings.height
        && settings.format.is_none_or(|f| f == best_cap.format)
        && (best_cap.framerate - settings.framerate).abs() < 1.0;

    Some(NegotiatedFormat::from_capability(best_cap, is_exact))
}

/// Check if a format is acceptable for streaming
/// (filters out Unknown and problematic formats)
pub fn is_acceptable_format(format: PixelFormat) -> bool {
    !matches!(format, PixelFormat::Unknown)
}

/// Filter capabilities to only those with acceptable formats
pub fn filter_acceptable_capabilities(caps: &[CameraCapability]) -> Vec<CameraCapability> {
    caps.iter()
        .filter(|c| is_acceptable_format(c.format))
        .cloned()
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cap(width: u32, height: u32, fps: f32, format: PixelFormat) -> CameraCapability {
        CameraCapability {
            width,
            height,
            framerate: fps,
            format,
        }
    }

    fn default_settings() -> CaptureSettings {
        CaptureSettings {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: None,
        }
    }

    #[test]
    fn test_exact_match() {
        let caps = vec![
            make_cap(1280, 720, 30.0, PixelFormat::Mjpeg),
            make_cap(1920, 1080, 30.0, PixelFormat::Mjpeg),
            make_cap(1920, 1080, 60.0, PixelFormat::Yuyv),
        ];

        let result = negotiate_format(&caps, &default_settings());
        assert!(result.is_some());
        let negotiated = result.unwrap();
        assert!(negotiated.exact_match);
        assert_eq!(negotiated.width, 1920);
        assert_eq!(negotiated.height, 1080);
        assert_eq!(negotiated.format, PixelFormat::Mjpeg);
    }

    #[test]
    fn test_prefer_mjpeg_over_yuyv() {
        let caps = vec![
            make_cap(1920, 1080, 30.0, PixelFormat::Yuyv),
            make_cap(1920, 1080, 30.0, PixelFormat::Mjpeg),
        ];

        let result = negotiate_format(&caps, &default_settings());
        let negotiated = result.unwrap();
        // Both have exact resolution, but MJPEG should be preferred
        assert_eq!(negotiated.format, PixelFormat::Mjpeg);
    }

    #[test]
    fn test_prefer_exact_resolution_over_format() {
        let caps = vec![
            make_cap(1920, 1080, 30.0, PixelFormat::Yuyv),
            make_cap(1280, 720, 30.0, PixelFormat::Mjpeg),
        ];

        let result = negotiate_format(&caps, &default_settings());
        let negotiated = result.unwrap();
        // Exact resolution should be preferred even with worse format
        assert_eq!(negotiated.width, 1920);
        assert_eq!(negotiated.height, 1080);
    }

    #[test]
    fn test_fallback_to_lower_resolution() {
        let caps = vec![
            make_cap(1280, 720, 30.0, PixelFormat::Mjpeg),
            make_cap(640, 480, 30.0, PixelFormat::Mjpeg),
        ];

        let settings = CaptureSettings {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: None,
        };

        let result = negotiate_format(&caps, &settings);
        let negotiated = result.unwrap();
        assert!(!negotiated.exact_match);
        // Should prefer 720p as it's closer to 1080p
        assert_eq!(negotiated.width, 1280);
        assert_eq!(negotiated.height, 720);
    }

    #[test]
    fn test_specific_format_requested() {
        let caps = vec![
            make_cap(1920, 1080, 30.0, PixelFormat::Mjpeg),
            make_cap(1920, 1080, 30.0, PixelFormat::Yuyv),
        ];

        let settings = CaptureSettings {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: Some(PixelFormat::Yuyv), // Specifically request YUYV
        };

        let result = negotiate_format(&caps, &settings);
        let negotiated = result.unwrap();
        assert_eq!(negotiated.format, PixelFormat::Yuyv);
    }

    #[test]
    fn test_empty_capabilities() {
        let result = negotiate_format(&[], &default_settings());
        assert!(result.is_none());
    }

    #[test]
    fn test_prefer_closer_framerate() {
        let caps = vec![
            make_cap(1920, 1080, 60.0, PixelFormat::Mjpeg),
            make_cap(1920, 1080, 25.0, PixelFormat::Mjpeg),
        ];

        let settings = CaptureSettings {
            width: 1920,
            height: 1080,
            framerate: 30.0,
            format: None,
        };

        let result = negotiate_format(&caps, &settings);
        let negotiated = result.unwrap();
        // 25 fps is closer to 30 than 60 is
        assert!((negotiated.framerate - 25.0).abs() < 0.1);
    }

    #[test]
    fn test_filter_acceptable_capabilities() {
        let caps = vec![
            make_cap(1920, 1080, 30.0, PixelFormat::Mjpeg),
            make_cap(1920, 1080, 30.0, PixelFormat::Unknown),
            make_cap(1280, 720, 30.0, PixelFormat::Yuyv),
        ];

        let filtered = filter_acceptable_capabilities(&caps);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|c| c.format != PixelFormat::Unknown));
    }

    #[test]
    fn test_format_priority_ordering() {
        assert!(format_priority(PixelFormat::Mjpeg) < format_priority(PixelFormat::Yuyv));
        assert!(format_priority(PixelFormat::Yuyv) < format_priority(PixelFormat::Rgb24));
        assert!(format_priority(PixelFormat::Rgb24) < format_priority(PixelFormat::Unknown));
    }
}
