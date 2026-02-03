//! Integration tests for test fixtures
//!
//! Verifies that all fixtures load correctly and have expected properties.

mod fixtures;

use fixtures::frames::*;
use fixtures::*;
use micround::core::PixelFormat;

// ============================================================================
// Frame Fixture Tests
// ============================================================================

#[test]
fn test_all_frame_fixtures_generate() {
    // RGBA frames
    let _ = rgba_color_bars_640x480();
    let _ = rgba_gradient_1280x720();
    let _ = rgba_checkerboard_1920x1080();
    let _ = rgba_corner_markers_100x100();

    // YUYV frames
    let _ = yuyv_color_bars_640x480();
    let _ = yuyv_gradient_1280x720();
    let _ = yuyv_odd_width_641x480();

    // NV12 frames
    let _ = nv12_color_bars_640x480();
    let _ = nv12_checkerboard_1920x1080();

    // RGB24
    let _ = rgb24_gradient_640x480();

    // Corrupted
    let _ = corrupted_truncated_frame();
    let _ = corrupted_zero_dimensions();
    let _ = corrupted_format_mismatch();
}

#[test]
fn test_rgba_frame_data_sizes() {
    let frame = rgba_color_bars_640x480();
    assert_eq!(frame.data.len(), 640 * 480 * 4, "RGBA32: 4 bytes per pixel");

    let frame = rgba_gradient_1280x720();
    assert_eq!(frame.data.len(), 1280 * 720 * 4);

    let frame = rgba_checkerboard_1920x1080();
    assert_eq!(frame.data.len(), 1920 * 1080 * 4);
}

#[test]
fn test_yuyv_frame_data_sizes() {
    let frame = yuyv_color_bars_640x480();
    assert_eq!(frame.data.len(), 640 * 480 * 2, "YUYV: 2 bytes per pixel");

    let frame = yuyv_gradient_1280x720();
    assert_eq!(frame.data.len(), 1280 * 720 * 2);
}

#[test]
fn test_nv12_frame_data_sizes() {
    let frame = nv12_color_bars_640x480();
    // NV12: Y plane (w*h) + UV plane (w*h/2) = w*h * 1.5
    let expected = (640 * 480) + (640 * 480 / 2);
    assert_eq!(frame.data.len(), expected, "NV12: 1.5 bytes per pixel");

    let frame = nv12_checkerboard_1920x1080();
    let expected = (1920 * 1080) + (1920 * 1080 / 2);
    assert_eq!(frame.data.len(), expected);
}

#[test]
fn test_color_bar_values() {
    let frame = rgba_color_bars_640x480();

    // First bar (white) - first pixel
    assert_eq!(&frame.data[0..4], &[255, 255, 255, 255]);

    // Approximate position of second bar (yellow) - around x=80
    let bar2_start = 80 * 4;
    assert_eq!(&frame.data[bar2_start..bar2_start + 4], &[255, 255, 0, 255]);

    // Last bar (black) - near the end of a row
    let last_bar_start = 600 * 4; // x=600 is in the 8th bar
    assert_eq!(
        &frame.data[last_bar_start..last_bar_start + 4],
        &[0, 0, 0, 255]
    );
}

#[test]
fn test_gradient_values() {
    let frame = rgba_gradient_1280x720();

    // First pixel should be near black (0)
    assert!(frame.data[0] < 5, "Left edge should be dark");

    // Last pixel of first row should be near white (255)
    let last_pixel = (1279 * 4) as usize;
    assert!(frame.data[last_pixel] > 250, "Right edge should be bright");
}

#[test]
fn test_corner_markers_positions() {
    let frame = rgba_corner_markers_100x100();

    // Top-left (0,0) should be red
    assert_eq!(
        &frame.data[0..4],
        &[255, 0, 0, 255],
        "Top-left should be red"
    );

    // Top-right (99,0) should be green
    let tr = (99 * 4) as usize;
    assert_eq!(
        &frame.data[tr..tr + 4],
        &[0, 255, 0, 255],
        "Top-right should be green"
    );

    // Bottom-left (0,99) should be blue
    let bl = (99 * 100 * 4) as usize;
    assert_eq!(
        &frame.data[bl..bl + 4],
        &[0, 0, 255, 255],
        "Bottom-left should be blue"
    );

    // Bottom-right (99,99) should be yellow
    let br = ((99 * 100 + 99) * 4) as usize;
    assert_eq!(
        &frame.data[br..br + 4],
        &[255, 255, 0, 255],
        "Bottom-right should be yellow"
    );
}

#[test]
fn test_frame_by_name() {
    assert!(get_frame_by_name("rgba_color_bars_640x480").is_some());
    assert!(get_frame_by_name("yuyv_gradient_1280x720").is_some());
    assert!(get_frame_by_name("nv12_checkerboard_1920x1080").is_some());
    assert!(get_frame_by_name("corrupted_truncated").is_some());
    assert!(get_frame_by_name("nonexistent").is_none());
}

#[test]
fn test_list_frame_fixtures() {
    let fixtures = list_frame_fixtures();
    assert!(fixtures.len() >= 9, "Should have at least 9 frame fixtures");

    // Check metadata
    for meta in fixtures {
        assert!(!meta.name.is_empty());
        assert!(meta.width > 0);
        assert!(meta.height > 0);
    }
}

// ============================================================================
// Config Fixture Tests
// ============================================================================

#[test]
fn test_load_valid_config() {
    let content = load_config_str("valid_config.toml").expect("Should load valid config");
    assert!(content.contains("version = 1"));
    assert!(content.contains("[camera]"));
    assert!(content.contains("[display]"));
    assert!(content.contains("[startup]"));
}

#[test]
fn test_load_minimal_config() {
    let content = load_config_str("minimal_config.toml").expect("Should load minimal config");
    assert!(content.contains("version = 1"));
    // Should be very short
    assert!(content.len() < 100);
}

#[test]
fn test_load_edge_cases_config() {
    let content = load_config_str("edge_cases_config.toml").expect("Should load edge cases");
    // Contains Unicode
    assert!(content.contains("日本語"));
    // Contains high values
    assert!(content.contains("7680"));
    assert!(content.contains("4320"));
}

#[test]
fn test_parse_valid_config_toml() {
    let value = load_config_toml("valid_config.toml").expect("Should parse");
    assert!(value.is_table());
    assert!(value.get("version").is_some());
    assert!(value.get("camera").is_some());
}

#[test]
fn test_invalid_config_has_errors() {
    // Invalid config should load as string but fail to parse
    let content = load_config_str("invalid_config.toml").expect("Should load file");
    let result = toml::from_str::<toml::Value>(&content);
    assert!(result.is_err(), "Invalid config should fail to parse");
}

#[test]
fn test_list_config_fixtures() {
    let configs = list_config_fixtures();
    assert!(configs.contains(&"valid_config.toml".to_string()));
    assert!(configs.contains(&"minimal_config.toml".to_string()));
    assert!(configs.len() >= 5);
}

// ============================================================================
// Device Fixture Tests
// ============================================================================

#[test]
fn test_load_cameras() {
    let cameras = load_cameras().expect("Should load cameras");
    assert_eq!(cameras.devices.len(), 4, "Should have 4 camera devices");

    // Check first camera
    let logitech = cameras
        .devices
        .iter()
        .find(|c| c.name.contains("C270"))
        .expect("Should have C270");
    assert!(logitech.is_available);
    assert!(logitech.capabilities.len() >= 4);
}

#[test]
fn test_load_displays() {
    let displays = load_displays().expect("Should load displays");
    assert_eq!(displays.displays.len(), 2, "Should have 2 displays");

    // Check primary
    let primary = displays
        .displays
        .iter()
        .find(|d| d.is_primary)
        .expect("Should have primary display");
    assert_eq!(primary.bounds.width, 3840);
    assert_eq!(primary.bounds.height, 2160);
}

#[test]
fn test_load_multi_monitor() {
    let displays = load_multi_monitor().expect("Should load multi-monitor");
    assert_eq!(displays.displays.len(), 4, "Should have 4 displays");

    // Check for negative X coordinate (TV)
    let has_negative_x = displays.displays.iter().any(|d| d.bounds.x < 0);
    assert!(has_negative_x, "Should have display with negative X");

    // Check for portrait mode (width < height)
    let has_portrait = displays
        .displays
        .iter()
        .any(|d| d.bounds.width < d.bounds.height);
    assert!(has_portrait, "Should have portrait display");
}

#[test]
fn test_load_devices_json() {
    let json = load_devices_json("cameras.json").expect("Should load as JSON");
    assert!(json.is_object());
    assert!(json.get("devices").is_some());
}

#[test]
fn test_camera_capabilities() {
    let cameras = load_cameras().expect("Should load cameras");

    for camera in cameras.devices {
        for cap in camera.capabilities {
            assert!(cap.width > 0, "Width should be positive");
            assert!(cap.height > 0, "Height should be positive");
            assert!(cap.framerate > 0.0, "Framerate should be positive");
            assert!(!cap.format.is_empty(), "Format should not be empty");
        }
    }
}

#[test]
fn test_list_device_fixtures() {
    let devices = list_device_fixtures();
    assert!(devices.contains(&"cameras.json".to_string()));
    assert!(devices.contains(&"displays.json".to_string()));
    assert!(devices.contains(&"multi_monitor.json".to_string()));
    assert!(devices.len() >= 3);
}

// ============================================================================
// Integration: Fixtures + Assertions
// ============================================================================

#[test]
fn test_frames_with_assertions() {
    // Use frame fixtures with assertion utilities from common module
    let frame_a = rgba_color_bars_640x480();
    let frame_b = rgba_color_bars_640x480();

    // Same generator should produce identical frames
    assert_eq!(
        frame_a.data, frame_b.data,
        "Same generator should produce identical frames"
    );
}

#[test]
fn test_corrupted_frames_invalid() {
    let truncated = corrupted_truncated_frame();
    let expected_size = (truncated.width * truncated.height * 4) as usize; // RGBA32
    assert!(
        truncated.data.len() < expected_size,
        "Truncated frame should be too small"
    );

    let zero_dim = corrupted_zero_dimensions();
    assert_eq!(zero_dim.width, 0);
    assert_eq!(zero_dim.height, 0);
    assert!(zero_dim.data.is_empty());
}
