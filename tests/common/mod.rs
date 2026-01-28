//! Common test utilities for Micround
//!
//! This module provides shared test utilities that can be used across all
//! test modules without relying on mocks.
//!
//! # Usage
//!
//! ```ignore
//! mod common;
//! use common::assertions::*;
//! use common::test_logger::*;
//! ```

pub mod assertions;
pub mod test_logger;

// Re-export commonly used items at the module level
pub use assertions::{
    // Frame comparison
    frame_psnr,
    assert_frames_similar,
    assert_frame_metadata_eq,
    assert_frames_identical,
    FrameCompareError,

    // Timing
    assert_completes_within,
    timed,
    timed_samples,
    assert_timing_within,
    TimingStats,

    // State transitions
    is_valid_camera_transition,
    assert_valid_camera_transition,
    assert_valid_camera_transition_sequence,

    // Error assertions
    assert_error_severity,
    assert_error_contains,
    assert_user_message_contains,
    HasSeverity,
    HasUserMessage,

    // Test helpers
    create_test_frame,
    create_gradient_frame,
    create_checkerboard_frame,
};
