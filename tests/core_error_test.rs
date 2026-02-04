//! Unit tests for core/error.rs
//!
//! Tests all error types, severity classification, ErrorContext builder,
//! and user_message generation.
//!
//! Run with: cargo test --test core_error_test

use micround::core::{
    CaptureError, ConfigError, ErrorContext, ErrorSeverity, MicroundError, PlatformError,
    RenderError,
};

// ============================================================================
// ErrorSeverity Tests
// ============================================================================

#[test]
fn test_error_severity_variants_exist() {
    let _recoverable = ErrorSeverity::Recoverable;
    let _user_actionable = ErrorSeverity::UserActionable;
    let _fatal = ErrorSeverity::Fatal;
}

#[test]
fn test_error_severity_equality() {
    assert_eq!(ErrorSeverity::Recoverable, ErrorSeverity::Recoverable);
    assert_eq!(ErrorSeverity::UserActionable, ErrorSeverity::UserActionable);
    assert_eq!(ErrorSeverity::Fatal, ErrorSeverity::Fatal);

    assert_ne!(ErrorSeverity::Recoverable, ErrorSeverity::Fatal);
    assert_ne!(ErrorSeverity::UserActionable, ErrorSeverity::Recoverable);
}

#[test]
fn test_error_severity_clone() {
    let s1 = ErrorSeverity::UserActionable;
    let s2 = s1.clone();
    assert_eq!(s1, s2);
}

#[test]
fn test_error_severity_copy() {
    let s1 = ErrorSeverity::Fatal;
    let s2 = s1; // Copy
    assert_eq!(s1, s2);
}

#[test]
fn test_error_severity_display() {
    assert_eq!(format!("{}", ErrorSeverity::Recoverable), "recoverable");
    assert_eq!(
        format!("{}", ErrorSeverity::UserActionable),
        "user-actionable"
    );
    assert_eq!(format!("{}", ErrorSeverity::Fatal), "fatal");
}

#[test]
fn test_error_severity_debug() {
    assert_eq!(format!("{:?}", ErrorSeverity::Recoverable), "Recoverable");
    assert_eq!(
        format!("{:?}", ErrorSeverity::UserActionable),
        "UserActionable"
    );
    assert_eq!(format!("{:?}", ErrorSeverity::Fatal), "Fatal");
}

// ============================================================================
// ErrorContext Tests
// ============================================================================

#[test]
fn test_error_context_new() {
    let ctx = ErrorContext::new();
    assert!(ctx.component.is_none());
    assert!(ctx.operation.is_none());
    assert!(ctx.device_id.is_none());
    assert!(ctx.display_id.is_none());
    assert!(ctx.extra.is_empty());
}

#[test]
fn test_error_context_default() {
    let ctx: ErrorContext = Default::default();
    assert!(ctx.component.is_none());
    assert!(ctx.operation.is_none());
}

#[test]
fn test_error_context_component() {
    let ctx = ErrorContext::new().component("capture");
    assert_eq!(ctx.component, Some("capture".to_string()));
}

#[test]
fn test_error_context_operation() {
    let ctx = ErrorContext::new().operation("start_stream");
    assert_eq!(ctx.operation, Some("start_stream".to_string()));
}

#[test]
fn test_error_context_device() {
    let ctx = ErrorContext::new().device("camera-0");
    assert_eq!(ctx.device_id, Some("camera-0".to_string()));
}

#[test]
fn test_error_context_display() {
    let ctx = ErrorContext::new().display("HDMI-1");
    assert_eq!(ctx.display_id, Some("HDMI-1".to_string()));
}

#[test]
fn test_error_context_with_extra() {
    let ctx = ErrorContext::new()
        .with("key1", "value1")
        .with("key2", "value2");

    assert_eq!(ctx.extra.len(), 2);
    assert_eq!(ctx.extra[0], ("key1".to_string(), "value1".to_string()));
    assert_eq!(ctx.extra[1], ("key2".to_string(), "value2".to_string()));
}

#[test]
fn test_error_context_builder_chain() {
    let ctx = ErrorContext::new()
        .component("render")
        .operation("create_surface")
        .device("camera-1")
        .display("DP-0")
        .with("width", "1920")
        .with("height", "1080");

    assert_eq!(ctx.component, Some("render".to_string()));
    assert_eq!(ctx.operation, Some("create_surface".to_string()));
    assert_eq!(ctx.device_id, Some("camera-1".to_string()));
    assert_eq!(ctx.display_id, Some("DP-0".to_string()));
    assert_eq!(ctx.extra.len(), 2);
}

#[test]
fn test_error_context_display_formatting() {
    let ctx = ErrorContext::new()
        .component("capture")
        .operation("next_frame")
        .device("video0");

    let display = format!("{}", ctx);
    assert!(display.contains("component=capture"));
    assert!(display.contains("operation=next_frame"));
    assert!(display.contains("device=video0"));
}

#[test]
fn test_error_context_display_with_extras() {
    let ctx = ErrorContext::new()
        .component("test")
        .with("framerate", "30");

    let display = format!("{}", ctx);
    assert!(display.contains("component=test"));
    assert!(display.contains("framerate=30"));
}

#[test]
fn test_error_context_display_empty() {
    let ctx = ErrorContext::new();
    let display = format!("{}", ctx);
    assert_eq!(display, "");
}

#[test]
fn test_error_context_clone() {
    let ctx1 = ErrorContext::new()
        .component("test")
        .operation("clone_test")
        .with("key", "value");

    let ctx2 = ctx1.clone();
    assert_eq!(ctx2.component, ctx1.component);
    assert_eq!(ctx2.operation, ctx1.operation);
    assert_eq!(ctx2.extra, ctx1.extra);
}

// ============================================================================
// CaptureError Tests
// ============================================================================

#[test]
fn test_capture_error_device_not_found() {
    let err = CaptureError::DeviceNotFound("camera-xyz".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("camera-xyz"));
    assert!(err.user_message().contains("not available"));
}

#[test]
fn test_capture_error_device_busy() {
    let err = CaptureError::DeviceBusy;

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("being used"));
    assert!(err.user_message().contains("another application"));
}

#[test]
fn test_capture_error_format_negotiation_failed() {
    let err = CaptureError::FormatNegotiationFailed("no matching format".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("video format"));
}

#[test]
fn test_capture_error_timeout() {
    let err = CaptureError::Timeout(5000);

    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    assert!(err.user_message().contains("not responding"));
    assert!(err.user_message().contains("reconnect"));
}

#[test]
fn test_capture_error_permission_denied() {
    let err = CaptureError::PermissionDenied("video0".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("permission"));
    assert!(err.user_message().contains("system settings"));
}

#[test]
fn test_capture_error_disconnected() {
    let err = CaptureError::Disconnected;

    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    assert!(err.user_message().contains("disconnected"));
    assert!(err.user_message().contains("reconnect"));
}

#[test]
fn test_capture_error_no_cameras() {
    let err = CaptureError::NoCameras;

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("No cameras found"));
}

#[test]
fn test_capture_error_platform() {
    let err = CaptureError::Platform("V4L2 ioctl failed".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Fatal);
    assert!(err.user_message().contains("system error"));
    assert!(err.user_message().contains("V4L2 ioctl failed"));
}

#[test]
fn test_capture_error_display_formatting() {
    let err = CaptureError::Timeout(1500);
    let display = format!("{}", err);
    assert!(display.contains("Capture timeout"));
    assert!(display.contains("1500ms"));
}

#[test]
fn test_capture_error_with_context() {
    let err = CaptureError::NoCameras;
    let ctx = ErrorContext::new()
        .component("capture")
        .operation("enumerate");

    let micround_err = err.with_context(ctx);

    assert!(matches!(micround_err, MicroundError::Capture { .. }));
    assert_eq!(micround_err.severity(), ErrorSeverity::UserActionable);
}

#[test]
fn test_capture_error_clone() {
    let err1 = CaptureError::Timeout(1000);
    let err2 = err1.clone();

    assert!(matches!(err2, CaptureError::Timeout(1000)));
}

// ============================================================================
// RenderError Tests
// ============================================================================

#[test]
fn test_render_error_surface_creation() {
    let err = RenderError::SurfaceCreation("EGL initialization failed".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Fatal);
    assert!(err.user_message().contains("display surface"));
    assert!(err.user_message().contains("graphics drivers"));
}

#[test]
fn test_render_error_display_not_found() {
    let err = RenderError::DisplayNotFound("HDMI-2".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("HDMI-2"));
    assert!(err.user_message().contains("not available"));
}

#[test]
fn test_render_error_gpu() {
    let err = RenderError::Gpu("shader compilation failed".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Fatal);
    assert!(err.user_message().contains("graphics error"));
    assert!(err.user_message().contains("update"));
}

#[test]
fn test_render_error_wallpaper_integration() {
    let err = RenderError::WallpaperIntegration("compositor not supported".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("wallpaper"));
    assert!(err.user_message().contains("not support"));
}

#[test]
fn test_render_error_frame_processing() {
    let err = RenderError::FrameProcessing("decode failed".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    assert!(err.user_message().contains("processing video frame"));
    assert!(err.user_message().contains("Skipping"));
}

#[test]
fn test_render_error_platform() {
    let err = RenderError::Platform("X11 connection lost".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Fatal);
    assert!(err.user_message().contains("system error"));
}

#[test]
fn test_render_error_display_formatting() {
    let err = RenderError::Gpu("buffer overflow".to_string());
    let display = format!("{}", err);
    assert!(display.contains("GPU error"));
    assert!(display.contains("buffer overflow"));
}

#[test]
fn test_render_error_with_context() {
    let err = RenderError::DisplayNotFound("DP-1".to_string());
    let ctx = ErrorContext::new().display("DP-1");

    let micround_err = err.with_context(ctx);

    assert!(matches!(micround_err, MicroundError::Render { .. }));
    assert_eq!(micround_err.severity(), ErrorSeverity::UserActionable);
}

#[test]
fn test_render_error_clone() {
    let err1 = RenderError::Gpu("test".to_string());
    let err2 = err1.clone();

    assert!(matches!(err2, RenderError::Gpu(_)));
}

// ============================================================================
// ConfigError Tests
// ============================================================================

#[test]
fn test_config_error_read_failed() {
    let err = ConfigError::ReadFailed("permission denied".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("Unable to read settings"));
    assert!(err.user_message().contains("default"));
}

#[test]
fn test_config_error_write_failed() {
    let err = ConfigError::WriteFailed("disk full".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("Unable to save settings"));
    assert!(err.user_message().contains("write permissions"));
}

#[test]
fn test_config_error_invalid() {
    let err = ConfigError::Invalid("missing required field".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("corrupted"));
    assert!(err.user_message().contains("default"));
}

#[test]
fn test_config_error_not_found() {
    let err = ConfigError::NotFound("/home/user/.config/micround/config.toml".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    assert!(err.user_message().contains("not found"));
    assert!(err.user_message().contains("default"));
}

#[test]
fn test_config_error_display_formatting() {
    let err = ConfigError::Invalid("bad json".to_string());
    let display = format!("{}", err);
    assert!(display.contains("Invalid configuration"));
    assert!(display.contains("bad json"));
}

#[test]
fn test_config_error_with_context() {
    let err = ConfigError::NotFound("config.toml".to_string());
    let ctx = ErrorContext::new().component("config").operation("load");

    let micround_err = err.with_context(ctx);

    assert!(matches!(micround_err, MicroundError::Config { .. }));
    assert_eq!(micround_err.severity(), ErrorSeverity::Recoverable);
}

#[test]
fn test_config_error_clone() {
    let err1 = ConfigError::ReadFailed("test".to_string());
    let err2 = err1.clone();

    assert!(matches!(err2, ConfigError::ReadFailed(_)));
}

// ============================================================================
// PlatformError Tests
// ============================================================================

#[test]
fn test_platform_error_unsupported() {
    let err = PlatformError::Unsupported("live wallpaper on Wayland".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("not available"));
    assert!(err.user_message().contains("live wallpaper on Wayland"));
}

#[test]
fn test_platform_error_command_failed() {
    let err = PlatformError::CommandFailed("gsettings returned error".to_string());

    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    assert!(err.user_message().contains("system command failed"));
}

#[test]
fn test_platform_error_invalid_state() {
    let err = PlatformError::InvalidState("window already exists".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("cannot be completed"));
}

#[test]
fn test_platform_error_resource_not_found() {
    let err = PlatformError::ResourceNotFound("shader file".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("not found"));
}

#[test]
fn test_platform_error_permission_denied() {
    let err = PlatformError::PermissionDenied("root required".to_string());

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("Permission denied"));
}

#[test]
fn test_platform_error_display_formatting() {
    let err = PlatformError::Unsupported("feature X".to_string());
    let display = format!("{}", err);
    assert!(display.contains("Operation not supported"));
    assert!(display.contains("feature X"));
}

#[test]
fn test_platform_error_clone() {
    let err1 = PlatformError::Unsupported("test".to_string());
    let err2 = err1.clone();

    assert!(matches!(err2, PlatformError::Unsupported(_)));
}

// ============================================================================
// MicroundError Tests
// ============================================================================

#[test]
fn test_micround_error_capture_variant() {
    let source = CaptureError::Timeout(1000);
    let context = ErrorContext::new().component("capture");
    let err = MicroundError::Capture { source, context };

    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    assert!(err.user_message().contains("not responding"));

    let ctx = err.context();
    assert_eq!(ctx.component, Some("capture".to_string()));
}

#[test]
fn test_micround_error_render_variant() {
    let source = RenderError::Gpu("memory exhausted".to_string());
    let context = ErrorContext::new().component("render").display("HDMI-1");
    let err = MicroundError::Render { source, context };

    assert_eq!(err.severity(), ErrorSeverity::Fatal);
    assert!(err.user_message().contains("graphics"));

    let ctx = err.context();
    assert_eq!(ctx.component, Some("render".to_string()));
    assert_eq!(ctx.display_id, Some("HDMI-1".to_string()));
}

#[test]
fn test_micround_error_config_variant() {
    let source = ConfigError::Invalid("parse error".to_string());
    let context = ErrorContext::new().operation("load");
    let err = MicroundError::Config { source, context };

    assert_eq!(err.severity(), ErrorSeverity::UserActionable);
    assert!(err.user_message().contains("corrupted"));

    let ctx = err.context();
    assert_eq!(ctx.operation, Some("load".to_string()));
}

#[test]
fn test_micround_error_internal_variant() {
    let err = MicroundError::Internal {
        message: "unexpected state".to_string(),
        context: ErrorContext::new().component("state_machine"),
    };

    assert_eq!(err.severity(), ErrorSeverity::Fatal);
    assert!(err.user_message().contains("unexpected error"));
    assert!(err.user_message().contains("restart"));

    let ctx = err.context();
    assert_eq!(ctx.component, Some("state_machine".to_string()));
}

#[test]
fn test_micround_error_display_formatting() {
    let source = CaptureError::NoCameras;
    let context = ErrorContext::new();
    let err = MicroundError::Capture { source, context };

    let display = format!("{}", err);
    assert!(display.contains("Capture error"));
    assert!(display.contains("No cameras available"));
}

#[test]
fn test_micround_error_clone() {
    let source = CaptureError::Disconnected;
    let context = ErrorContext::new().device("camera-0");
    let err1 = MicroundError::Capture { source, context };
    let err2 = err1.clone();

    assert!(matches!(err2, MicroundError::Capture { .. }));
}

#[test]
fn test_micround_error_severity_from_capture() {
    // Test all capture error severities propagate correctly
    let tests = vec![
        (CaptureError::Timeout(100), ErrorSeverity::Recoverable),
        (CaptureError::Disconnected, ErrorSeverity::Recoverable),
        (
            CaptureError::DeviceNotFound("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (CaptureError::DeviceBusy, ErrorSeverity::UserActionable),
        (CaptureError::NoCameras, ErrorSeverity::UserActionable),
        (
            CaptureError::PermissionDenied("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (
            CaptureError::FormatNegotiationFailed("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (CaptureError::Platform("x".into()), ErrorSeverity::Fatal),
    ];

    for (source, expected_severity) in tests {
        let err = MicroundError::Capture {
            source,
            context: ErrorContext::new(),
        };
        assert_eq!(
            err.severity(),
            expected_severity,
            "Severity mismatch for {:?}",
            err
        );
    }
}

#[test]
fn test_micround_error_severity_from_render() {
    let tests = vec![
        (
            RenderError::FrameProcessing("x".into()),
            ErrorSeverity::Recoverable,
        ),
        (
            RenderError::DisplayNotFound("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (
            RenderError::WallpaperIntegration("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (
            RenderError::SurfaceCreation("x".into()),
            ErrorSeverity::Fatal,
        ),
        (RenderError::Gpu("x".into()), ErrorSeverity::Fatal),
        (RenderError::Platform("x".into()), ErrorSeverity::Fatal),
    ];

    for (source, expected_severity) in tests {
        let err = MicroundError::Render {
            source,
            context: ErrorContext::new(),
        };
        assert_eq!(
            err.severity(),
            expected_severity,
            "Severity mismatch for {:?}",
            err
        );
    }
}

#[test]
fn test_micround_error_severity_from_config() {
    let tests = vec![
        (
            ConfigError::NotFound("x".into()),
            ErrorSeverity::Recoverable,
        ),
        (
            ConfigError::ReadFailed("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (
            ConfigError::WriteFailed("x".into()),
            ErrorSeverity::UserActionable,
        ),
        (
            ConfigError::Invalid("x".into()),
            ErrorSeverity::UserActionable,
        ),
    ];

    for (source, expected_severity) in tests {
        let err = MicroundError::Config {
            source,
            context: ErrorContext::new(),
        };
        assert_eq!(
            err.severity(),
            expected_severity,
            "Severity mismatch for {:?}",
            err
        );
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_error_context_with_empty_strings() {
    let ctx = ErrorContext::new()
        .component("")
        .operation("")
        .device("")
        .display("");

    assert_eq!(ctx.component, Some("".to_string()));
    let display = format!("{}", ctx);
    assert!(display.contains("component="));
}

#[test]
fn test_error_context_with_special_characters() {
    let ctx = ErrorContext::new()
        .component("capture/v4l2")
        .device("/dev/video0")
        .with("path", "/home/user/config.toml");

    let display = format!("{}", ctx);
    assert!(display.contains("capture/v4l2"));
    assert!(display.contains("/dev/video0"));
    assert!(display.contains("/home/user/config.toml"));
}

#[test]
fn test_error_context_with_unicode() {
    let ctx = ErrorContext::new()
        .component("カメラ")
        .operation("キャプチャ");

    let display = format!("{}", ctx);
    assert!(display.contains("カメラ"));
    assert!(display.contains("キャプチャ"));
}

#[test]
fn test_capture_error_timeout_zero() {
    let err = CaptureError::Timeout(0);
    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
    let display = format!("{}", err);
    assert!(display.contains("0ms"));
}

#[test]
fn test_capture_error_timeout_large() {
    let err = CaptureError::Timeout(u64::MAX);
    assert_eq!(err.severity(), ErrorSeverity::Recoverable);
}

#[test]
fn test_multiple_context_extras() {
    let ctx = ErrorContext::new()
        .with("key1", "value1")
        .with("key2", "value2")
        .with("key3", "value3")
        .with("key4", "value4")
        .with("key5", "value5");

    assert_eq!(ctx.extra.len(), 5);

    let display = format!("{}", ctx);
    for i in 1..=5 {
        assert!(display.contains(&format!("key{}=value{}", i, i)));
    }
}

#[test]
fn test_error_context_order_preserved() {
    let ctx = ErrorContext::new()
        .component("A")
        .operation("B")
        .device("C")
        .display("D");

    let display = format!("{}", ctx);
    let component_pos = display.find("component=").unwrap();
    let operation_pos = display.find("operation=").unwrap();
    let device_pos = display.find("device=").unwrap();
    let display_pos = display.find("display=").unwrap();

    assert!(component_pos < operation_pos);
    assert!(operation_pos < device_pos);
    assert!(device_pos < display_pos);
}

// ============================================================================
// Error Display Tests (thiserror integration)
// ============================================================================

#[test]
fn test_capture_error_all_variants_display() {
    let errors = vec![
        (
            CaptureError::DeviceNotFound("cam".into()),
            "Camera device not found: cam",
        ),
        (CaptureError::DeviceBusy, "Camera device is busy"),
        (
            CaptureError::FormatNegotiationFailed("no match".into()),
            "Failed to negotiate capture format: no match",
        ),
        (
            CaptureError::Timeout(100),
            "Capture timeout: no frame received within 100ms",
        ),
        (
            CaptureError::PermissionDenied("video0".into()),
            "Permission denied: video0",
        ),
        (CaptureError::Disconnected, "Camera was disconnected"),
        (CaptureError::NoCameras, "No cameras available"),
        (
            CaptureError::Platform("io error".into()),
            "Platform error: io error",
        ),
    ];

    for (err, expected_substring) in errors {
        let display = format!("{}", err);
        assert!(
            display.contains(expected_substring),
            "Expected '{}' to contain '{}'",
            display,
            expected_substring
        );
    }
}

#[test]
fn test_render_error_all_variants_display() {
    let errors = vec![
        (
            RenderError::SurfaceCreation("egl".into()),
            "Failed to create render surface: egl",
        ),
        (
            RenderError::DisplayNotFound("HDMI".into()),
            "Display not found: HDMI",
        ),
        (RenderError::Gpu("memory".into()), "GPU error: memory"),
        (
            RenderError::WallpaperIntegration("wayland".into()),
            "Wallpaper integration failed: wayland",
        ),
        (
            RenderError::FrameProcessing("decode".into()),
            "Frame processing failed: decode",
        ),
        (RenderError::Platform("x11".into()), "Platform error: x11"),
    ];

    for (err, expected_substring) in errors {
        let display = format!("{}", err);
        assert!(
            display.contains(expected_substring),
            "Expected '{}' to contain '{}'",
            display,
            expected_substring
        );
    }
}

#[test]
fn test_config_error_all_variants_display() {
    let errors = vec![
        (
            ConfigError::ReadFailed("permission".into()),
            "Failed to read config file: permission",
        ),
        (
            ConfigError::WriteFailed("disk full".into()),
            "Failed to write config file: disk full",
        ),
        (
            ConfigError::Invalid("bad toml".into()),
            "Invalid configuration: bad toml",
        ),
        (
            ConfigError::NotFound("path/to/file".into()),
            "Config file not found at: path/to/file",
        ),
    ];

    for (err, expected_substring) in errors {
        let display = format!("{}", err);
        assert!(
            display.contains(expected_substring),
            "Expected '{}' to contain '{}'",
            display,
            expected_substring
        );
    }
}

#[test]
fn test_platform_error_all_variants_display() {
    let errors = vec![
        (
            PlatformError::Unsupported("feature".into()),
            "Operation not supported on this platform: feature",
        ),
        (
            PlatformError::CommandFailed("cmd".into()),
            "Platform command failed: cmd",
        ),
        (
            PlatformError::InvalidState("state".into()),
            "Invalid platform state: state",
        ),
        (
            PlatformError::ResourceNotFound("res".into()),
            "Resource not found: res",
        ),
        (
            PlatformError::PermissionDenied("perm".into()),
            "Permission denied: perm",
        ),
    ];

    for (err, expected_substring) in errors {
        let display = format!("{}", err);
        assert!(
            display.contains(expected_substring),
            "Expected '{}' to contain '{}'",
            display,
            expected_substring
        );
    }
}

#[test]
fn test_micround_error_all_variants_display() {
    let capture_err = MicroundError::Capture {
        source: CaptureError::Timeout(100),
        context: ErrorContext::new(),
    };
    assert!(format!("{}", capture_err).contains("Capture error"));

    let render_err = MicroundError::Render {
        source: RenderError::Gpu("test".into()),
        context: ErrorContext::new(),
    };
    assert!(format!("{}", render_err).contains("Render error"));

    let config_err = MicroundError::Config {
        source: ConfigError::Invalid("test".into()),
        context: ErrorContext::new(),
    };
    assert!(format!("{}", config_err).contains("Configuration error"));

    let internal_err = MicroundError::Internal {
        message: "test message".into(),
        context: ErrorContext::new(),
    };
    let display = format!("{}", internal_err);
    assert!(display.contains("Internal error"));
    assert!(display.contains("test message"));
}
