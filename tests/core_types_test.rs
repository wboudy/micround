//! Unit tests for core/types.rs
//!
//! Tests DeviceId, DisplayId, CaptureSettings, ScalingMode, Rotation, Flip,
//! NegotiatedFormat, and related types.
//!
//! Run with: cargo test --test core_types_test

use std::collections::HashSet;

use micround::core::{
    CameraCapability, CameraDevice, CaptureSettings, DeviceId, DisplayId, Flip, Frame,
    NegotiatedFormat, PixelFormat, Rotation, ScalingMode,
};

// ============================================================================
// DeviceId Tests
// ============================================================================

#[test]
fn test_device_id_creation() {
    let id = DeviceId("camera-0".to_string());
    assert_eq!(id.0, "camera-0");
}

#[test]
fn test_device_id_clone() {
    let id1 = DeviceId("usb-device-1".to_string());
    let id2 = id1.clone();
    assert_eq!(id1, id2);
}

#[test]
fn test_device_id_equality() {
    let id1 = DeviceId("camera".to_string());
    let id2 = DeviceId("camera".to_string());
    let id3 = DeviceId("other".to_string());

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_device_id_hash() {
    let id1 = DeviceId("device-a".to_string());
    let id2 = DeviceId("device-a".to_string());
    let id3 = DeviceId("device-b".to_string());

    let mut set = HashSet::new();
    set.insert(id1.clone());

    // Same value should not increase set size
    assert!(set.contains(&id2));
    set.insert(id2);
    assert_eq!(set.len(), 1);

    // Different value should increase set size
    set.insert(id3);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_device_id_debug() {
    let id = DeviceId("test-camera".to_string());
    let debug_str = format!("{:?}", id);
    assert!(debug_str.contains("DeviceId"));
    assert!(debug_str.contains("test-camera"));
}

#[test]
fn test_device_id_serialize_deserialize() {
    let id = DeviceId("serialized-device".to_string());
    let json = serde_json::to_string(&id).expect("serialize failed");
    let deserialized: DeviceId = serde_json::from_str(&json).expect("deserialize failed");
    assert_eq!(id, deserialized);
}

#[test]
fn test_device_id_empty_string() {
    let id = DeviceId(String::new());
    assert_eq!(id.0, "");
}

#[test]
fn test_device_id_unicode() {
    let id = DeviceId("カメラ-1".to_string());
    assert_eq!(id.0, "カメラ-1");

    // Serialization should work with unicode
    let json = serde_json::to_string(&id).unwrap();
    let deserialized: DeviceId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, deserialized);
}

// ============================================================================
// DisplayId Tests
// ============================================================================

#[test]
fn test_display_id_creation() {
    let id = DisplayId("monitor-0".to_string());
    assert_eq!(id.0, "monitor-0");
}

#[test]
fn test_display_id_clone() {
    let id1 = DisplayId("display-primary".to_string());
    let id2 = id1.clone();
    assert_eq!(id1, id2);
}

#[test]
fn test_display_id_equality() {
    let id1 = DisplayId("HDMI-1".to_string());
    let id2 = DisplayId("HDMI-1".to_string());
    let id3 = DisplayId("DP-0".to_string());

    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
}

#[test]
fn test_display_id_hash() {
    let id1 = DisplayId("display-x".to_string());
    let id2 = DisplayId("display-x".to_string());
    let id3 = DisplayId("display-y".to_string());

    let mut set = HashSet::new();
    set.insert(id1.clone());

    assert!(set.contains(&id2));
    set.insert(id2);
    assert_eq!(set.len(), 1);

    set.insert(id3);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_display_id_serialize_deserialize() {
    let id = DisplayId("serialized-display".to_string());
    let json = serde_json::to_string(&id).expect("serialize failed");
    let deserialized: DisplayId = serde_json::from_str(&json).expect("deserialize failed");
    assert_eq!(id, deserialized);
}

// ============================================================================
// PixelFormat Tests
// ============================================================================

#[test]
fn test_pixel_format_variants() {
    // Ensure all variants exist and can be created
    let formats = [
        PixelFormat::Mjpeg,
        PixelFormat::Yuyv,
        PixelFormat::Nv12,
        PixelFormat::Rgb24,
        PixelFormat::Rgba32,
        PixelFormat::Unknown,
    ];

    // All should be distinct
    for (i, f1) in formats.iter().enumerate() {
        for (j, f2) in formats.iter().enumerate() {
            if i == j {
                assert_eq!(f1, f2);
            } else {
                assert_ne!(f1, f2);
            }
        }
    }
}

#[test]
fn test_pixel_format_copy() {
    let format1 = PixelFormat::Rgba32;
    let format2 = format1; // Copy
    assert_eq!(format1, format2);
}

#[test]
fn test_pixel_format_clone() {
    let format1 = PixelFormat::Mjpeg;
    let format2 = format1.clone();
    assert_eq!(format1, format2);
}

#[test]
fn test_pixel_format_debug() {
    let format = PixelFormat::Yuyv;
    let debug_str = format!("{:?}", format);
    assert_eq!(debug_str, "Yuyv");
}

#[test]
fn test_pixel_format_serialize_all_variants() {
    let formats = [
        PixelFormat::Mjpeg,
        PixelFormat::Yuyv,
        PixelFormat::Nv12,
        PixelFormat::Rgb24,
        PixelFormat::Rgba32,
        PixelFormat::Unknown,
    ];

    for format in formats {
        let json = serde_json::to_string(&format).expect("serialize failed");
        let deserialized: PixelFormat = serde_json::from_str(&json).expect("deserialize failed");
        assert_eq!(format, deserialized);
    }
}

// ============================================================================
// CameraCapability Tests
// ============================================================================

#[test]
fn test_camera_capability_creation() {
    let cap = CameraCapability {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Mjpeg,
    };

    assert_eq!(cap.width, 1920);
    assert_eq!(cap.height, 1080);
    assert_eq!(cap.framerate, 30.0);
    assert_eq!(cap.format, PixelFormat::Mjpeg);
}

#[test]
fn test_camera_capability_clone() {
    let cap1 = CameraCapability {
        width: 640,
        height: 480,
        framerate: 60.0,
        format: PixelFormat::Yuyv,
    };
    let cap2 = cap1.clone();

    assert_eq!(cap2.width, 640);
    assert_eq!(cap2.height, 480);
    assert_eq!(cap2.framerate, 60.0);
    assert_eq!(cap2.format, PixelFormat::Yuyv);
}

#[test]
fn test_camera_capability_serialize() {
    let cap = CameraCapability {
        width: 3840,
        height: 2160,
        framerate: 24.0,
        format: PixelFormat::Nv12,
    };

    let json = serde_json::to_string(&cap).unwrap();
    let deserialized: CameraCapability = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.width, 3840);
    assert_eq!(deserialized.height, 2160);
    assert_eq!(deserialized.framerate, 24.0);
    assert_eq!(deserialized.format, PixelFormat::Nv12);
}

// ============================================================================
// CameraDevice Tests
// ============================================================================

#[test]
fn test_camera_device_creation() {
    let device = CameraDevice {
        id: DeviceId("usb-cam-0".to_string()),
        name: "HD Webcam".to_string(),
        manufacturer: Some("Logitech".to_string()),
        capabilities: vec![
            CameraCapability {
                width: 1920,
                height: 1080,
                framerate: 30.0,
                format: PixelFormat::Mjpeg,
            },
            CameraCapability {
                width: 640,
                height: 480,
                framerate: 60.0,
                format: PixelFormat::Yuyv,
            },
        ],
        is_available: true,
    };

    assert_eq!(device.id.0, "usb-cam-0");
    assert_eq!(device.name, "HD Webcam");
    assert_eq!(device.manufacturer, Some("Logitech".to_string()));
    assert_eq!(device.capabilities.len(), 2);
    assert!(device.is_available);
}

#[test]
fn test_camera_device_no_manufacturer() {
    let device = CameraDevice {
        id: DeviceId("generic-cam".to_string()),
        name: "Generic Camera".to_string(),
        manufacturer: None,
        capabilities: vec![],
        is_available: false,
    };

    assert!(device.manufacturer.is_none());
    assert!(device.capabilities.is_empty());
    assert!(!device.is_available);
}

#[test]
fn test_camera_device_serialize() {
    let device = CameraDevice {
        id: DeviceId("test-device".to_string()),
        name: "Test Camera".to_string(),
        manufacturer: Some("Test Corp".to_string()),
        capabilities: vec![CameraCapability {
            width: 800,
            height: 600,
            framerate: 25.0,
            format: PixelFormat::Rgb24,
        }],
        is_available: true,
    };

    let json = serde_json::to_string(&device).unwrap();
    let deserialized: CameraDevice = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id.0, "test-device");
    assert_eq!(deserialized.name, "Test Camera");
    assert_eq!(deserialized.manufacturer, Some("Test Corp".to_string()));
    assert_eq!(deserialized.capabilities.len(), 1);
    assert!(deserialized.is_available);
}

// ============================================================================
// CaptureSettings Tests
// ============================================================================

#[test]
fn test_capture_settings_default() {
    let settings = CaptureSettings::default();

    assert_eq!(settings.width, 1920);
    assert_eq!(settings.height, 1080);
    assert_eq!(settings.framerate, 30.0);
    assert!(settings.format.is_none());
}

#[test]
fn test_capture_settings_custom() {
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 60.0,
        format: Some(PixelFormat::Rgba32),
    };

    assert_eq!(settings.width, 640);
    assert_eq!(settings.height, 480);
    assert_eq!(settings.framerate, 60.0);
    assert_eq!(settings.format, Some(PixelFormat::Rgba32));
}

#[test]
fn test_capture_settings_clone() {
    let settings1 = CaptureSettings {
        width: 1280,
        height: 720,
        framerate: 24.0,
        format: Some(PixelFormat::Mjpeg),
    };
    let settings2 = settings1.clone();

    assert_eq!(settings2.width, 1280);
    assert_eq!(settings2.height, 720);
    assert_eq!(settings2.framerate, 24.0);
    assert_eq!(settings2.format, Some(PixelFormat::Mjpeg));
}

#[test]
fn test_capture_settings_serialize() {
    let settings = CaptureSettings {
        width: 3840,
        height: 2160,
        framerate: 30.0,
        format: Some(PixelFormat::Nv12),
    };

    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: CaptureSettings = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.width, 3840);
    assert_eq!(deserialized.height, 2160);
    assert_eq!(deserialized.framerate, 30.0);
    assert_eq!(deserialized.format, Some(PixelFormat::Nv12));
}

#[test]
fn test_capture_settings_serialize_none_format() {
    let settings = CaptureSettings {
        width: 640,
        height: 480,
        framerate: 15.0,
        format: None,
    };

    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: CaptureSettings = serde_json::from_str(&json).unwrap();

    assert!(deserialized.format.is_none());
}

// ============================================================================
// Frame Tests
// ============================================================================

#[test]
fn test_frame_creation() {
    let data = vec![0u8; 1920 * 1080 * 4]; // RGBA data
    let frame = Frame {
        data,
        format: PixelFormat::Rgba32,
        width: 1920,
        height: 1080,
        timestamp_ns: 1234567890,
        sequence: 42,
    };

    assert_eq!(frame.data.len(), 1920 * 1080 * 4);
    assert_eq!(frame.format, PixelFormat::Rgba32);
    assert_eq!(frame.width, 1920);
    assert_eq!(frame.height, 1080);
    assert_eq!(frame.timestamp_ns, 1234567890);
    assert_eq!(frame.sequence, 42);
}

#[test]
fn test_frame_empty_data() {
    let frame = Frame {
        data: vec![],
        format: PixelFormat::Unknown,
        width: 0,
        height: 0,
        timestamp_ns: 0,
        sequence: 0,
    };

    assert!(frame.data.is_empty());
    assert_eq!(frame.width, 0);
    assert_eq!(frame.height, 0);
}

#[test]
fn test_frame_data_access() {
    let mut data = vec![0u8; 640 * 480 * 3]; // RGB data
    data[0] = 255; // Red channel first pixel
    data[1] = 128; // Green channel first pixel
    data[2] = 64;  // Blue channel first pixel

    let frame = Frame {
        data,
        format: PixelFormat::Rgb24,
        width: 640,
        height: 480,
        timestamp_ns: 0,
        sequence: 0,
    };

    assert_eq!(frame.data[0], 255);
    assert_eq!(frame.data[1], 128);
    assert_eq!(frame.data[2], 64);
}

// ============================================================================
// ScalingMode Tests
// ============================================================================

#[test]
fn test_scaling_mode_default() {
    let mode: ScalingMode = Default::default();
    assert_eq!(mode, ScalingMode::Fill);
}

#[test]
fn test_scaling_mode_variants() {
    let modes = [
        ScalingMode::Fit,
        ScalingMode::Fill,
        ScalingMode::Stretch,
        ScalingMode::Center,
    ];

    // All variants should be distinct
    for (i, m1) in modes.iter().enumerate() {
        for (j, m2) in modes.iter().enumerate() {
            if i == j {
                assert_eq!(m1, m2);
            } else {
                assert_ne!(m1, m2);
            }
        }
    }
}

#[test]
fn test_scaling_mode_copy() {
    let mode1 = ScalingMode::Fit;
    let mode2 = mode1; // Copy
    assert_eq!(mode1, mode2);
}

#[test]
fn test_scaling_mode_clone() {
    let mode1 = ScalingMode::Stretch;
    let mode2 = mode1.clone();
    assert_eq!(mode1, mode2);
}

#[test]
fn test_scaling_mode_debug() {
    assert_eq!(format!("{:?}", ScalingMode::Fit), "Fit");
    assert_eq!(format!("{:?}", ScalingMode::Fill), "Fill");
    assert_eq!(format!("{:?}", ScalingMode::Stretch), "Stretch");
    assert_eq!(format!("{:?}", ScalingMode::Center), "Center");
}

#[test]
fn test_scaling_mode_serialize() {
    let modes = [
        ScalingMode::Fit,
        ScalingMode::Fill,
        ScalingMode::Stretch,
        ScalingMode::Center,
    ];

    for mode in modes {
        let json = serde_json::to_string(&mode).unwrap();
        let deserialized: ScalingMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, deserialized);
    }
}

// ============================================================================
// Rotation Tests
// ============================================================================

#[test]
fn test_rotation_default() {
    let rotation: Rotation = Default::default();
    assert_eq!(rotation, Rotation::None);
}

#[test]
fn test_rotation_variants() {
    let rotations = [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ];

    for (i, r1) in rotations.iter().enumerate() {
        for (j, r2) in rotations.iter().enumerate() {
            if i == j {
                assert_eq!(r1, r2);
            } else {
                assert_ne!(r1, r2);
            }
        }
    }
}

#[test]
fn test_rotation_copy() {
    let rot1 = Rotation::Clockwise90;
    let rot2 = rot1; // Copy
    assert_eq!(rot1, rot2);
}

#[test]
fn test_rotation_clone() {
    let rot1 = Rotation::Clockwise180;
    let rot2 = rot1.clone();
    assert_eq!(rot1, rot2);
}

#[test]
fn test_rotation_debug() {
    assert_eq!(format!("{:?}", Rotation::None), "None");
    assert_eq!(format!("{:?}", Rotation::Clockwise90), "Clockwise90");
    assert_eq!(format!("{:?}", Rotation::Clockwise180), "Clockwise180");
    assert_eq!(format!("{:?}", Rotation::Clockwise270), "Clockwise270");
}

#[test]
fn test_rotation_serialize() {
    let rotations = [
        Rotation::None,
        Rotation::Clockwise90,
        Rotation::Clockwise180,
        Rotation::Clockwise270,
    ];

    for rotation in rotations {
        let json = serde_json::to_string(&rotation).unwrap();
        let deserialized: Rotation = serde_json::from_str(&json).unwrap();
        assert_eq!(rotation, deserialized);
    }
}

// ============================================================================
// Flip Tests
// ============================================================================

#[test]
fn test_flip_default() {
    let flip: Flip = Default::default();
    assert_eq!(flip, Flip::None);
}

#[test]
fn test_flip_variants() {
    let flips = [
        Flip::None,
        Flip::Horizontal,
        Flip::Vertical,
        Flip::Both,
    ];

    for (i, f1) in flips.iter().enumerate() {
        for (j, f2) in flips.iter().enumerate() {
            if i == j {
                assert_eq!(f1, f2);
            } else {
                assert_ne!(f1, f2);
            }
        }
    }
}

#[test]
fn test_flip_copy() {
    let flip1 = Flip::Horizontal;
    let flip2 = flip1; // Copy
    assert_eq!(flip1, flip2);
}

#[test]
fn test_flip_clone() {
    let flip1 = Flip::Both;
    let flip2 = flip1.clone();
    assert_eq!(flip1, flip2);
}

#[test]
fn test_flip_debug() {
    assert_eq!(format!("{:?}", Flip::None), "None");
    assert_eq!(format!("{:?}", Flip::Horizontal), "Horizontal");
    assert_eq!(format!("{:?}", Flip::Vertical), "Vertical");
    assert_eq!(format!("{:?}", Flip::Both), "Both");
}

#[test]
fn test_flip_serialize() {
    let flips = [
        Flip::None,
        Flip::Horizontal,
        Flip::Vertical,
        Flip::Both,
    ];

    for flip in flips {
        let json = serde_json::to_string(&flip).unwrap();
        let deserialized: Flip = serde_json::from_str(&json).unwrap();
        assert_eq!(flip, deserialized);
    }
}

// ============================================================================
// NegotiatedFormat Tests
// ============================================================================

#[test]
fn test_negotiated_format_creation() {
    let format = NegotiatedFormat {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Mjpeg,
        exact_match: true,
    };

    assert_eq!(format.width, 1920);
    assert_eq!(format.height, 1080);
    assert_eq!(format.framerate, 30.0);
    assert_eq!(format.format, PixelFormat::Mjpeg);
    assert!(format.exact_match);
}

#[test]
fn test_negotiated_format_from_capability_exact() {
    let cap = CameraCapability {
        width: 1280,
        height: 720,
        framerate: 60.0,
        format: PixelFormat::Yuyv,
    };

    let negotiated = NegotiatedFormat::from_capability(&cap, true);

    assert_eq!(negotiated.width, 1280);
    assert_eq!(negotiated.height, 720);
    assert_eq!(negotiated.framerate, 60.0);
    assert_eq!(negotiated.format, PixelFormat::Yuyv);
    assert!(negotiated.exact_match);
}

#[test]
fn test_negotiated_format_from_capability_not_exact() {
    let cap = CameraCapability {
        width: 640,
        height: 480,
        framerate: 30.0,
        format: PixelFormat::Nv12,
    };

    let negotiated = NegotiatedFormat::from_capability(&cap, false);

    assert_eq!(negotiated.width, 640);
    assert_eq!(negotiated.height, 480);
    assert_eq!(negotiated.framerate, 30.0);
    assert_eq!(negotiated.format, PixelFormat::Nv12);
    assert!(!negotiated.exact_match);
}

#[test]
fn test_negotiated_format_resolution_matches_true() {
    let format = NegotiatedFormat {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Rgba32,
        exact_match: true,
    };

    assert!(format.resolution_matches(1920, 1080));
}

#[test]
fn test_negotiated_format_resolution_matches_false_width() {
    let format = NegotiatedFormat {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Rgba32,
        exact_match: true,
    };

    assert!(!format.resolution_matches(1280, 1080));
}

#[test]
fn test_negotiated_format_resolution_matches_false_height() {
    let format = NegotiatedFormat {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Rgba32,
        exact_match: true,
    };

    assert!(!format.resolution_matches(1920, 720));
}

#[test]
fn test_negotiated_format_resolution_matches_both_wrong() {
    let format = NegotiatedFormat {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Rgba32,
        exact_match: true,
    };

    assert!(!format.resolution_matches(640, 480));
}

#[test]
fn test_negotiated_format_clone() {
    let format1 = NegotiatedFormat {
        width: 800,
        height: 600,
        framerate: 25.0,
        format: PixelFormat::Rgb24,
        exact_match: false,
    };
    let format2 = format1.clone();

    assert_eq!(format2.width, 800);
    assert_eq!(format2.height, 600);
    assert_eq!(format2.framerate, 25.0);
    assert_eq!(format2.format, PixelFormat::Rgb24);
    assert!(!format2.exact_match);
}

#[test]
fn test_negotiated_format_serialize() {
    let format = NegotiatedFormat {
        width: 3840,
        height: 2160,
        framerate: 24.0,
        format: PixelFormat::Nv12,
        exact_match: true,
    };

    let json = serde_json::to_string(&format).unwrap();
    let deserialized: NegotiatedFormat = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.width, 3840);
    assert_eq!(deserialized.height, 2160);
    assert_eq!(deserialized.framerate, 24.0);
    assert_eq!(deserialized.format, PixelFormat::Nv12);
    assert!(deserialized.exact_match);
}

#[test]
fn test_negotiated_format_debug() {
    let format = NegotiatedFormat {
        width: 1920,
        height: 1080,
        framerate: 30.0,
        format: PixelFormat::Mjpeg,
        exact_match: true,
    };

    let debug_str = format!("{:?}", format);
    assert!(debug_str.contains("NegotiatedFormat"));
    assert!(debug_str.contains("1920"));
    assert!(debug_str.contains("1080"));
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_capture_settings_zero_dimensions() {
    let settings = CaptureSettings {
        width: 0,
        height: 0,
        framerate: 0.0,
        format: None,
    };

    assert_eq!(settings.width, 0);
    assert_eq!(settings.height, 0);
    assert_eq!(settings.framerate, 0.0);
}

#[test]
fn test_capture_settings_max_dimensions() {
    let settings = CaptureSettings {
        width: u32::MAX,
        height: u32::MAX,
        framerate: f32::MAX,
        format: Some(PixelFormat::Rgba32),
    };

    assert_eq!(settings.width, u32::MAX);
    assert_eq!(settings.height, u32::MAX);
    assert_eq!(settings.framerate, f32::MAX);
}

#[test]
fn test_negotiated_format_resolution_matches_zero() {
    let format = NegotiatedFormat {
        width: 0,
        height: 0,
        framerate: 0.0,
        format: PixelFormat::Unknown,
        exact_match: false,
    };

    assert!(format.resolution_matches(0, 0));
    assert!(!format.resolution_matches(1, 0));
    assert!(!format.resolution_matches(0, 1));
}

#[test]
fn test_frame_large_sequence_number() {
    let frame = Frame {
        data: vec![],
        format: PixelFormat::Unknown,
        width: 0,
        height: 0,
        timestamp_ns: u64::MAX,
        sequence: u64::MAX,
    };

    assert_eq!(frame.timestamp_ns, u64::MAX);
    assert_eq!(frame.sequence, u64::MAX);
}

#[test]
fn test_camera_capability_fractional_framerate() {
    let cap = CameraCapability {
        width: 1920,
        height: 1080,
        framerate: 29.97, // NTSC standard
        format: PixelFormat::Mjpeg,
    };

    assert!((cap.framerate - 29.97).abs() < 0.001);
}
