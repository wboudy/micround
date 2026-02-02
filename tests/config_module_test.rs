//! Config Module Unit Tests
//!
//! Tests for configuration loading, saving, validation, and persistence.

use std::fs;

use tempfile::TempDir;

// ============================================================================
// Test Imports
// ============================================================================

use micround::config::{
    config_backup_path, config_dir, config_path, AppConfig, CameraConfig, ConfigValidationError,
    DisplayConfig, InternalConfig, StartupConfig, CONFIG_VERSION,
};
use micround::core::{DeviceId, DisplayId, Flip, Rotation, ScalingMode};

// ============================================================================
// AppConfig Tests
// ============================================================================

#[test]
fn test_app_config_default() {
    let config = AppConfig::default();

    assert_eq!(config.version, CONFIG_VERSION);
    assert_eq!(config.camera.width, 1920);
    assert_eq!(config.camera.height, 1080);
    assert_eq!(config.camera.framerate, 30.0);
    assert!(config.camera.device_id.is_none());
    assert!(config.display.display_id.is_none());
    assert_eq!(config.display.scaling_mode, ScalingMode::default());
    assert_eq!(config.display.rotation, 0);
    assert!(!config.display.flip_horizontal);
    assert!(!config.display.flip_vertical);
    assert!(!config.startup.launch_at_login);
    assert!(!config.startup.auto_start_feed);
    assert!(config.internal.last_clean_shutdown);
}

#[test]
fn test_app_config_clone() {
    let mut config = AppConfig::default();
    config.camera.width = 1280;
    config.startup.launch_at_login = true;

    let cloned = config.clone();

    assert_eq!(cloned.camera.width, 1280);
    assert!(cloned.startup.launch_at_login);
}

#[test]
fn test_app_config_debug() {
    let config = AppConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("AppConfig"));
}

#[test]
fn test_app_config_validate_valid() {
    let config = AppConfig::default();
    let errors = config.validate();
    assert!(errors.is_empty());
}

#[test]
fn test_app_config_validate_zero_width() {
    let mut config = AppConfig::default();
    config.camera.width = 0;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].field.contains("width"));
}

#[test]
fn test_app_config_validate_zero_height() {
    let mut config = AppConfig::default();
    config.camera.height = 0;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].field.contains("height"));
}

#[test]
fn test_app_config_validate_zero_dimensions() {
    let mut config = AppConfig::default();
    config.camera.width = 0;
    config.camera.height = 0;

    // Only one error since width/height are checked together
    let errors = config.validate();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_app_config_validate_negative_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = -1.0;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].field.contains("framerate"));
}

#[test]
fn test_app_config_validate_zero_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = 0.0;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].field.contains("framerate"));
}

#[test]
fn test_app_config_validate_excessive_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = 500.0;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].field.contains("framerate"));
}

#[test]
fn test_app_config_validate_nan_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = f32::NAN;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_app_config_validate_infinity_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = f32::INFINITY;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_app_config_validate_invalid_rotation() {
    let mut config = AppConfig::default();
    config.display.rotation = 45;

    let errors = config.validate();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].field.contains("rotation"));
}

#[test]
fn test_app_config_validate_valid_rotations() {
    for rotation in [0, 90, 180, 270] {
        let mut config = AppConfig::default();
        config.display.rotation = rotation;

        let errors = config.validate();
        assert!(errors.is_empty(), "rotation {} should be valid", rotation);
    }
}

#[test]
fn test_app_config_validate_multiple_errors() {
    let mut config = AppConfig::default();
    config.camera.width = 0;
    config.camera.framerate = -5.0;
    config.display.rotation = 45;

    let errors = config.validate();
    assert_eq!(errors.len(), 3);
}

#[test]
fn test_app_config_sanitize_zero_width() {
    let mut config = AppConfig::default();
    config.camera.width = 0;

    config.sanitize();

    assert_eq!(config.camera.width, 1920);
}

#[test]
fn test_app_config_sanitize_zero_height() {
    let mut config = AppConfig::default();
    config.camera.height = 0;

    config.sanitize();

    assert_eq!(config.camera.height, 1080);
}

#[test]
fn test_app_config_sanitize_invalid_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = -10.0;

    config.sanitize();

    assert_eq!(config.camera.framerate, 30.0);
}

#[test]
fn test_app_config_sanitize_excessive_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = 500.0;

    config.sanitize();

    assert_eq!(config.camera.framerate, 30.0);
}

#[test]
fn test_app_config_sanitize_invalid_rotation() {
    let mut config = AppConfig::default();
    config.display.rotation = 45;

    config.sanitize();

    assert_eq!(config.display.rotation, 0);
}

#[test]
fn test_app_config_sanitize_valid_config_unchanged() {
    let config_before = AppConfig::default();
    let mut config = config_before.clone();

    config.sanitize();

    assert_eq!(config.camera.width, config_before.camera.width);
    assert_eq!(config.camera.height, config_before.camera.height);
    assert_eq!(config.camera.framerate, config_before.camera.framerate);
    assert_eq!(config.display.rotation, config_before.display.rotation);
}

// ============================================================================
// CameraConfig Tests
// ============================================================================

#[test]
fn test_camera_config_default() {
    let config = CameraConfig::default();

    assert!(config.device_id.is_none());
    assert_eq!(config.width, 1920);
    assert_eq!(config.height, 1080);
    assert_eq!(config.framerate, 30.0);
}

#[test]
fn test_camera_config_clone() {
    let mut config = CameraConfig::default();
    config.device_id = Some(DeviceId("test-camera".to_string()));
    config.width = 1280;

    let cloned = config.clone();

    assert_eq!(cloned.device_id, Some(DeviceId("test-camera".to_string())));
    assert_eq!(cloned.width, 1280);
}

#[test]
fn test_camera_config_debug() {
    let config = CameraConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("CameraConfig"));
}

#[test]
fn test_camera_config_with_device_id() {
    let mut config = CameraConfig::default();
    config.device_id = Some(DeviceId("USB\\VID_1234".to_string()));

    assert_eq!(
        config.device_id,
        Some(DeviceId("USB\\VID_1234".to_string()))
    );
}

// ============================================================================
// DisplayConfig Tests
// ============================================================================

#[test]
fn test_display_config_default() {
    let config = DisplayConfig::default();

    assert!(config.display_id.is_none());
    assert_eq!(config.scaling_mode, ScalingMode::default());
    assert_eq!(config.rotation, 0);
    assert!(!config.flip_horizontal);
    assert!(!config.flip_vertical);
}

#[test]
fn test_display_config_clone() {
    let mut config = DisplayConfig::default();
    config.rotation = 90;
    config.flip_horizontal = true;

    let cloned = config.clone();

    assert_eq!(cloned.rotation, 90);
    assert!(cloned.flip_horizontal);
}

#[test]
fn test_display_config_debug() {
    let config = DisplayConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("DisplayConfig"));
}

#[test]
fn test_display_config_rotation_enum_none() {
    let mut config = DisplayConfig::default();
    config.rotation = 0;
    assert_eq!(config.rotation_enum(), Rotation::None);
}

#[test]
fn test_display_config_rotation_enum_90() {
    let mut config = DisplayConfig::default();
    config.rotation = 90;
    assert_eq!(config.rotation_enum(), Rotation::Clockwise90);
}

#[test]
fn test_display_config_rotation_enum_180() {
    let mut config = DisplayConfig::default();
    config.rotation = 180;
    assert_eq!(config.rotation_enum(), Rotation::Clockwise180);
}

#[test]
fn test_display_config_rotation_enum_270() {
    let mut config = DisplayConfig::default();
    config.rotation = 270;
    assert_eq!(config.rotation_enum(), Rotation::Clockwise270);
}

#[test]
fn test_display_config_rotation_enum_invalid() {
    let mut config = DisplayConfig::default();
    config.rotation = 45;
    // Invalid rotations default to None
    assert_eq!(config.rotation_enum(), Rotation::None);
}

#[test]
fn test_display_config_flip_enum_none() {
    let mut config = DisplayConfig::default();
    config.flip_horizontal = false;
    config.flip_vertical = false;
    assert_eq!(config.flip_enum(), Flip::None);
}

#[test]
fn test_display_config_flip_enum_horizontal() {
    let mut config = DisplayConfig::default();
    config.flip_horizontal = true;
    config.flip_vertical = false;
    assert_eq!(config.flip_enum(), Flip::Horizontal);
}

#[test]
fn test_display_config_flip_enum_vertical() {
    let mut config = DisplayConfig::default();
    config.flip_horizontal = false;
    config.flip_vertical = true;
    assert_eq!(config.flip_enum(), Flip::Vertical);
}

#[test]
fn test_display_config_flip_enum_both() {
    let mut config = DisplayConfig::default();
    config.flip_horizontal = true;
    config.flip_vertical = true;
    assert_eq!(config.flip_enum(), Flip::Both);
}

#[test]
fn test_display_config_with_display_id() {
    let mut config = DisplayConfig::default();
    config.display_id = Some(DisplayId("primary".to_string()));

    assert_eq!(config.display_id, Some(DisplayId("primary".to_string())));
}

// ============================================================================
// StartupConfig Tests
// ============================================================================

#[test]
fn test_startup_config_default() {
    let config = StartupConfig::default();

    assert!(!config.launch_at_login);
    assert!(!config.auto_start_feed);
    assert!(!config.minimize_on_start);
}

#[test]
fn test_startup_config_clone() {
    let mut config = StartupConfig::default();
    config.launch_at_login = true;
    config.auto_start_feed = true;

    let cloned = config.clone();

    assert!(cloned.launch_at_login);
    assert!(cloned.auto_start_feed);
}

#[test]
fn test_startup_config_debug() {
    let config = StartupConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("StartupConfig"));
}

// ============================================================================
// InternalConfig Tests
// ============================================================================

#[test]
fn test_internal_config_default() {
    let config = InternalConfig::default();

    assert!(config.original_wallpaper_path.is_none());
    assert!(config.last_clean_shutdown);
    assert!(config.last_camera_id.is_none());
}

#[test]
fn test_internal_config_clone() {
    let mut config = InternalConfig::default();
    config.original_wallpaper_path = Some("/path/to/wallpaper.jpg".to_string());
    config.last_clean_shutdown = false;

    let cloned = config.clone();

    assert_eq!(
        cloned.original_wallpaper_path,
        Some("/path/to/wallpaper.jpg".to_string())
    );
    assert!(!cloned.last_clean_shutdown);
}

#[test]
fn test_internal_config_debug() {
    let config = InternalConfig::default();
    let debug = format!("{:?}", config);
    assert!(debug.contains("InternalConfig"));
}

#[test]
fn test_internal_config_with_camera_id() {
    let mut config = InternalConfig::default();
    config.last_camera_id = Some(DeviceId("camera-1".to_string()));

    assert_eq!(
        config.last_camera_id,
        Some(DeviceId("camera-1".to_string()))
    );
}

// ============================================================================
// ConfigValidationError Tests
// ============================================================================

#[test]
fn test_config_validation_error_creation() {
    let error = ConfigValidationError {
        field: "camera.width".to_string(),
        message: "Must be non-zero".to_string(),
    };

    assert_eq!(error.field, "camera.width");
    assert_eq!(error.message, "Must be non-zero");
}

#[test]
fn test_config_validation_error_clone() {
    let error = ConfigValidationError {
        field: "display.rotation".to_string(),
        message: "Invalid value".to_string(),
    };

    let cloned = error.clone();

    assert_eq!(cloned.field, error.field);
    assert_eq!(cloned.message, error.message);
}

#[test]
fn test_config_validation_error_debug() {
    let error = ConfigValidationError {
        field: "test".to_string(),
        message: "test message".to_string(),
    };
    let debug = format!("{:?}", error);
    assert!(debug.contains("ConfigValidationError"));
    assert!(debug.contains("test"));
}

// ============================================================================
// Path Functions Tests
// ============================================================================

#[test]
fn test_config_dir() {
    let dir = config_dir();
    assert!(dir.to_string_lossy().contains("micround"));
}

#[test]
fn test_config_path() {
    let path = config_path();
    assert!(path.ends_with("config.toml"));
    assert!(path.to_string_lossy().contains("micround"));
}

#[test]
fn test_config_backup_path() {
    let path = config_backup_path();
    assert!(path.ends_with("config.toml.bak"));
    assert!(path.to_string_lossy().contains("micround"));
}

#[test]
fn test_paths_are_related() {
    let dir = config_dir();
    let config = config_path();
    let backup = config_backup_path();

    // Config and backup should be in the config dir
    assert_eq!(config.parent(), Some(dir.as_path()));
    assert_eq!(backup.parent(), Some(dir.as_path()));
}

// ============================================================================
// Serialization Tests
// ============================================================================

#[test]
fn test_serialization_default() {
    let config = AppConfig::default();
    let toml_str = toml::to_string_pretty(&config).expect("serialize");

    assert!(toml_str.contains("version = 1"));
    assert!(toml_str.contains("[camera]"));
    assert!(toml_str.contains("[display]"));
    assert!(toml_str.contains("[startup]"));
    assert!(toml_str.contains("[internal]"));
}

#[test]
fn test_serialization_with_device_ids() {
    let mut config = AppConfig::default();
    config.camera.device_id = Some(DeviceId("test-camera".to_string()));
    config.display.display_id = Some(DisplayId("primary".to_string()));

    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    assert!(toml_str.contains("test-camera"));
    assert!(toml_str.contains("primary"));
}

#[test]
fn test_serialization_with_all_options() {
    let mut config = AppConfig::default();
    config.camera.device_id = Some(DeviceId("cam".to_string()));
    config.camera.width = 1280;
    config.camera.height = 720;
    config.camera.framerate = 60.0;
    config.display.display_id = Some(DisplayId("monitor".to_string()));
    config.display.scaling_mode = ScalingMode::Fit;
    config.display.rotation = 90;
    config.display.flip_horizontal = true;
    config.display.flip_vertical = true;
    config.startup.launch_at_login = true;
    config.startup.auto_start_feed = true;
    config.startup.minimize_on_start = true;
    config.internal.original_wallpaper_path = Some("/path/wallpaper.jpg".to_string());
    config.internal.last_clean_shutdown = false;
    config.internal.last_camera_id = Some(DeviceId("last-cam".to_string()));

    let toml_str = toml::to_string_pretty(&config).expect("serialize");

    // Verify key values are in the serialized output
    assert!(toml_str.contains("width = 1280"));
    assert!(toml_str.contains("height = 720"));
    assert!(toml_str.contains("framerate = 60"));
    assert!(toml_str.contains("rotation = 90"));
    assert!(toml_str.contains("flip_horizontal = true"));
    assert!(toml_str.contains("flip_vertical = true"));
    assert!(toml_str.contains("launch_at_login = true"));
}

// ============================================================================
// Deserialization Tests
// ============================================================================

#[test]
fn test_deserialization_minimal() {
    let toml_str = r#"
        version = 1
    "#;

    let config: AppConfig = toml::from_str(toml_str).expect("deserialize");

    // Should have all defaults
    assert_eq!(config.version, 1);
    assert_eq!(config.camera.width, 1920);
    assert_eq!(config.camera.height, 1080);
}

#[test]
fn test_deserialization_partial_camera() {
    let toml_str = r#"
        version = 1

        [camera]
        width = 1280
    "#;

    let config: AppConfig = toml::from_str(toml_str).expect("deserialize");

    assert_eq!(config.camera.width, 1280);
    // Other fields should have defaults
    assert_eq!(config.camera.height, 1080);
    assert_eq!(config.camera.framerate, 30.0);
}

#[test]
fn test_deserialization_all_scaling_modes() {
    for (mode_str, expected_mode) in [
        ("Fit", ScalingMode::Fit),
        ("Fill", ScalingMode::Fill),
        ("Stretch", ScalingMode::Stretch),
        ("Center", ScalingMode::Center),
    ] {
        let toml_str = format!(
            r#"
            version = 1

            [display]
            scaling_mode = "{}"
        "#,
            mode_str
        );

        let config: AppConfig = toml::from_str(&toml_str).expect("deserialize");
        assert_eq!(
            config.display.scaling_mode, expected_mode,
            "mode: {}",
            mode_str
        );
    }
}

#[test]
fn test_deserialization_forward_compatibility() {
    let toml_str = r#"
        version = 1
        unknown_root_field = "ignored"

        [camera]
        width = 1920
        height = 1080
        framerate = 30.0
        unknown_camera_field = "also ignored"

        [display]
        unknown_display_field = 123

        [future_section]
        new_feature = true
        another_new_thing = "value"
    "#;

    let config: AppConfig = toml::from_str(toml_str).expect("deserialize");

    // Should parse successfully despite unknown fields
    assert_eq!(config.camera.width, 1920);
    assert_eq!(config.display.rotation, 0);
}

#[test]
fn test_deserialization_invalid_toml_fails() {
    let invalid_toml = r#"
        version = 1
        [camera
        width = broken
    "#;

    let result: Result<AppConfig, _> = toml::from_str(invalid_toml);
    assert!(result.is_err());
}

// ============================================================================
// Roundtrip Tests
// ============================================================================

#[test]
fn test_roundtrip_default() {
    let original = AppConfig::default();
    let toml_str = toml::to_string_pretty(&original).expect("serialize");
    let loaded: AppConfig = toml::from_str(&toml_str).expect("deserialize");

    assert_eq!(loaded.version, original.version);
    assert_eq!(loaded.camera.width, original.camera.width);
    assert_eq!(loaded.camera.height, original.camera.height);
    assert_eq!(loaded.camera.framerate, original.camera.framerate);
}

#[test]
fn test_roundtrip_with_device_ids() {
    let mut original = AppConfig::default();
    original.camera.device_id = Some(DeviceId("camera-123".to_string()));
    original.display.display_id = Some(DisplayId("display-456".to_string()));

    let toml_str = toml::to_string_pretty(&original).expect("serialize");
    let loaded: AppConfig = toml::from_str(&toml_str).expect("deserialize");

    assert_eq!(loaded.camera.device_id, original.camera.device_id);
    assert_eq!(loaded.display.display_id, original.display.display_id);
}

#[test]
fn test_roundtrip_all_settings() {
    let mut original = AppConfig::default();
    original.camera.width = 640;
    original.camera.height = 480;
    original.camera.framerate = 15.0;
    original.display.rotation = 270;
    original.display.flip_horizontal = true;
    original.display.flip_vertical = true;
    original.startup.launch_at_login = true;
    original.startup.auto_start_feed = true;
    original.startup.minimize_on_start = true;
    original.internal.last_clean_shutdown = false;
    original.internal.original_wallpaper_path = Some("/test/path.jpg".to_string());

    let toml_str = toml::to_string_pretty(&original).expect("serialize");
    let loaded: AppConfig = toml::from_str(&toml_str).expect("deserialize");

    assert_eq!(loaded.camera.width, 640);
    assert_eq!(loaded.camera.height, 480);
    assert_eq!(loaded.camera.framerate, 15.0);
    assert_eq!(loaded.display.rotation, 270);
    assert!(loaded.display.flip_horizontal);
    assert!(loaded.display.flip_vertical);
    assert!(loaded.startup.launch_at_login);
    assert!(loaded.startup.auto_start_feed);
    assert!(loaded.startup.minimize_on_start);
    assert!(!loaded.internal.last_clean_shutdown);
    assert_eq!(
        loaded.internal.original_wallpaper_path,
        Some("/test/path.jpg".to_string())
    );
}

// ============================================================================
// File I/O Tests
// ============================================================================

#[test]
fn test_file_write_and_read() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut config = AppConfig::default();
    config.camera.width = 1280;
    config.startup.launch_at_login = true;

    // Write
    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    fs::write(&config_file, &toml_str).expect("write");

    // Read
    let read_str = fs::read_to_string(&config_file).expect("read");
    let loaded: AppConfig = toml::from_str(&read_str).expect("deserialize");

    assert_eq!(loaded.camera.width, 1280);
    assert!(loaded.startup.launch_at_login);
}

#[test]
fn test_file_overwrite() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    // Write first config
    let mut config1 = AppConfig::default();
    config1.camera.width = 1920;
    let toml1 = toml::to_string_pretty(&config1).expect("serialize");
    fs::write(&config_file, &toml1).expect("write");

    // Write second config
    let mut config2 = AppConfig::default();
    config2.camera.width = 1280;
    let toml2 = toml::to_string_pretty(&config2).expect("serialize");
    fs::write(&config_file, &toml2).expect("write");

    // Read back
    let read_str = fs::read_to_string(&config_file).expect("read");
    let loaded: AppConfig = toml::from_str(&read_str).expect("deserialize");

    assert_eq!(loaded.camera.width, 1280);
}

#[test]
fn test_file_with_utf8_content() {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("config.toml");

    let mut config = AppConfig::default();
    config.internal.original_wallpaper_path = Some("/Users/日本語/壁紙.jpg".to_string());

    // Write
    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    fs::write(&config_file, &toml_str).expect("write");

    // Read
    let read_str = fs::read_to_string(&config_file).expect("read");
    let loaded: AppConfig = toml::from_str(&read_str).expect("deserialize");

    assert_eq!(
        loaded.internal.original_wallpaper_path,
        Some("/Users/日本語/壁紙.jpg".to_string())
    );
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_empty_string_device_id() {
    let mut config = AppConfig::default();
    config.camera.device_id = Some(DeviceId("".to_string()));

    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    let loaded: AppConfig = toml::from_str(&toml_str).expect("deserialize");

    assert_eq!(loaded.camera.device_id, Some(DeviceId("".to_string())));
}

#[test]
fn test_very_long_path() {
    let long_path = "/".to_string() + &"a".repeat(1000) + "/wallpaper.jpg";

    let mut config = AppConfig::default();
    config.internal.original_wallpaper_path = Some(long_path.clone());

    let toml_str = toml::to_string_pretty(&config).expect("serialize");
    let loaded: AppConfig = toml::from_str(&toml_str).expect("deserialize");

    assert_eq!(loaded.internal.original_wallpaper_path, Some(long_path));
}

#[test]
fn test_max_framerate_boundary() {
    let mut config = AppConfig::default();
    config.camera.framerate = 240.0;

    let errors = config.validate();
    assert!(errors.is_empty(), "240 fps should be valid");
}

#[test]
fn test_just_over_max_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = 240.1;

    let errors = config.validate();
    assert!(!errors.is_empty(), "240.1 fps should be invalid");
}

#[test]
fn test_very_small_framerate() {
    let mut config = AppConfig::default();
    config.camera.framerate = 0.1;

    let errors = config.validate();
    assert!(errors.is_empty(), "0.1 fps should be valid");
}

#[test]
fn test_very_large_dimensions() {
    let mut config = AppConfig::default();
    config.camera.width = 8192;
    config.camera.height = 4320;

    let errors = config.validate();
    assert!(errors.is_empty(), "8K resolution should be valid");
}

#[test]
fn test_all_flip_combinations() {
    for h in [false, true] {
        for v in [false, true] {
            let mut config = DisplayConfig::default();
            config.flip_horizontal = h;
            config.flip_vertical = v;

            let flip = config.flip_enum();
            match (h, v) {
                (false, false) => assert_eq!(flip, Flip::None),
                (true, false) => assert_eq!(flip, Flip::Horizontal),
                (false, true) => assert_eq!(flip, Flip::Vertical),
                (true, true) => assert_eq!(flip, Flip::Both),
            }
        }
    }
}

#[test]
fn test_all_rotation_values() {
    for rot in [0, 90, 180, 270] {
        let mut config = DisplayConfig::default();
        config.rotation = rot;

        let rotation = config.rotation_enum();
        match rot {
            0 => assert_eq!(rotation, Rotation::None),
            90 => assert_eq!(rotation, Rotation::Clockwise90),
            180 => assert_eq!(rotation, Rotation::Clockwise180),
            270 => assert_eq!(rotation, Rotation::Clockwise270),
            _ => unreachable!(),
        }
    }
}

// ============================================================================
// Constants Tests
// ============================================================================

#[test]
fn test_config_version_constant() {
    assert_eq!(CONFIG_VERSION, 1);
}

#[test]
fn test_default_uses_config_version() {
    let config = AppConfig::default();
    assert_eq!(config.version, CONFIG_VERSION);
}
