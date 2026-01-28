//! Integration tests for the assertion utilities
//!
//! This test file verifies that the common test utilities work correctly.

mod common;

use common::assertions::*;
use micround::core::{PixelFormat, ErrorSeverity};
use micround::capture::CameraState;
use std::time::Duration;

// ============================================================================
// Frame Comparison Integration Tests
// ============================================================================

#[test]
fn test_frame_psnr_identical() {
    let frame = create_test_frame(100, 100, PixelFormat::Rgba32, 128);
    let psnr = frame_psnr(&frame, &frame).unwrap();
    assert!(psnr.is_infinite());
}

#[test]
fn test_frame_comparison_workflow() {
    // Create two similar frames
    let frame_a = create_test_frame(640, 480, PixelFormat::Rgba32, 100);
    let mut frame_b = create_test_frame(640, 480, PixelFormat::Rgba32, 100);

    // Modify a few bytes
    frame_b.data[0] = 101;
    frame_b.data[1000] = 99;

    // Should pass similarity check
    assert_frames_similar(&frame_a, &frame_b, 30.0);

    // Metadata should still match
    assert_frame_metadata_eq(&frame_a, &frame_b);
}

#[test]
fn test_gradient_frame_not_identical_to_solid() {
    let solid = create_test_frame(256, 10, PixelFormat::Rgba32, 128);
    let gradient = create_gradient_frame(256, 10, PixelFormat::Rgba32);

    // Metadata matches (same dimensions and format)
    assert_frame_metadata_eq(&solid, &gradient);

    // But PSNR should be low (very different pixel values)
    let psnr = frame_psnr(&solid, &gradient).unwrap();
    assert!(psnr < 30.0, "Solid and gradient should have low PSNR");
}

// ============================================================================
// Timing Integration Tests
// ============================================================================

#[test]
fn test_timing_utilities() {
    // Test that timed returns correct duration
    let (result, duration) = timed("quick_op", || {
        let sum: u64 = (0..1000).sum();
        sum
    });

    assert_eq!(result, 499500);
    assert!(duration < Duration::from_secs(1));
}

#[test]
fn test_timing_samples() {
    let stats = timed_samples("compute", 5, || {
        let _: Vec<u64> = (0..1000).collect();
    });

    assert_eq!(stats.samples, 5);
    assert!(stats.min <= stats.median);
    assert!(stats.median <= stats.max);
}

// ============================================================================
// State Transition Integration Tests
// ============================================================================

#[test]
fn test_full_camera_lifecycle() {
    let lifecycle = vec![
        CameraState::Disconnected,
        CameraState::Available,
        CameraState::Opening,
        CameraState::Ready,
        CameraState::Capturing,
        CameraState::Ready,      // stop
        CameraState::Capturing,  // restart
        CameraState::Ready,      // stop again
        CameraState::Available,  // close
        CameraState::Disconnected, // unplug
    ];

    assert_valid_camera_transition_sequence(&lifecycle);
}

#[test]
fn test_error_recovery_path() {
    let error_info = micround::capture::CameraErrorInfo::new("Timeout after 5000ms", true);

    let recovery_path = vec![
        CameraState::Capturing,
        CameraState::Error(error_info.clone()),
        CameraState::Opening,  // retry
        CameraState::Ready,
        CameraState::Capturing,
    ];

    assert_valid_camera_transition_sequence(&recovery_path);
}

// ============================================================================
// Error Assertion Integration Tests
// ============================================================================

#[test]
fn test_capture_error_assertions() {
    use micround::core::CaptureError;

    let timeout_err = CaptureError::Timeout(5000);
    assert_error_severity(&timeout_err, ErrorSeverity::Recoverable);
    assert_error_contains(&timeout_err, "5000");

    let perm_err = CaptureError::PermissionDenied("video0".into());
    assert_error_severity(&perm_err, ErrorSeverity::UserActionable);
    assert_user_message_contains(&perm_err, "permission");
}

#[test]
fn test_render_error_assertions() {
    use micround::core::RenderError;

    let gpu_err = RenderError::Gpu("out of memory".into());
    assert_error_severity(&gpu_err, ErrorSeverity::Fatal);
    assert_error_contains(&gpu_err, "memory");
}

#[test]
fn test_config_error_assertions() {
    use micround::core::ConfigError;

    let not_found = ConfigError::NotFound("/etc/micround.toml".into());
    assert_error_severity(&not_found, ErrorSeverity::Recoverable);
    assert_user_message_contains(&not_found, "default");
}

// ============================================================================
// Test Helpers Integration Tests
// ============================================================================

#[test]
fn test_checkerboard_pattern() {
    let frame = create_checkerboard_frame(80, 80, PixelFormat::Rgba32, 8);

    // Block (0,0) should be white
    let pixel_0_0 = frame.data[0];
    assert_eq!(pixel_0_0, 255);

    // Block (1,0) should be black
    let pixel_8_0 = frame.data[8 * 4]; // x=8, y=0
    assert_eq!(pixel_8_0, 0);

    // Block (1,1) should be white again
    let pixel_8_8 = frame.data[(8 * 80 + 8) * 4]; // x=8, y=8
    assert_eq!(pixel_8_8, 255);
}

#[test]
fn test_frame_creation_formats() {
    // Test different formats have correct data sizes
    let rgba = create_test_frame(100, 100, PixelFormat::Rgba32, 0);
    assert_eq!(rgba.data.len(), 100 * 100 * 4);

    let rgb = create_test_frame(100, 100, PixelFormat::Rgb24, 0);
    assert_eq!(rgb.data.len(), 100 * 100 * 3);

    let yuyv = create_test_frame(100, 100, PixelFormat::Yuyv, 0);
    assert_eq!(yuyv.data.len(), 100 * 100 * 2);
}
