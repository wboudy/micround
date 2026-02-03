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

// Test utilities are re-exported for convenience; not all are used in every test file
#![allow(unused_imports)]

pub mod assertions;
pub mod test_logger;

// Re-export commonly used items at the module level
pub use assertions::{
    // Timing
    assert_completes_within,
    assert_error_contains,
    // Error assertions
    assert_error_severity,
    assert_frame_metadata_eq,
    assert_frames_identical,
    assert_frames_similar,
    assert_timing_within,
    assert_user_message_contains,
    assert_valid_camera_transition,
    assert_valid_camera_transition_sequence,

    create_checkerboard_frame,
    create_gradient_frame,
    // Test helpers
    create_test_frame,
    // Frame comparison
    frame_psnr,
    // State transitions
    is_valid_camera_transition,
    timed,
    timed_samples,
    FrameCompareError,

    HasSeverity,
    HasUserMessage,

    TimingStats,
};
