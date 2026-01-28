//! Integration tests for the test logging framework

mod common;

use common::test_logger::*;
use std::time::Duration;

#[test]
fn test_logger_full_workflow() {
    let mut logger = TestLogger::new("test_full_workflow", 4);

    // Step 1: Setup
    test_step!(logger, "Setting up test environment");
    let data = vec![1, 2, 3, 4, 5];
    test_step_ok!(logger, "Created {} test items", data.len());

    // Step 2: Process
    test_step!(logger, "Processing data");
    let sum: i32 = data.iter().sum();
    test_step_ok!(logger, "Sum = {}", sum);

    // Step 3: Validate
    test_step!(logger, "Validating results");
    test_assert!(logger, sum == 15, "Sum is correct");
    test_assert!(logger, data.len() == 5, "Data length is correct");
    test_assert_eq!(logger, sum, 15, "Sum equals expected");

    // Step 4: Cleanup
    test_step!(logger, "Cleaning up");
    test_step_ok!(logger, "done");

    let result = logger.finish();

    assert!(result.passed);
    assert_eq!(result.steps.len(), 4);
    assert_eq!(result.assertions.len(), 3);
    assert!(result.all_steps_ok());
    assert!(result.all_assertions_passed());
}

#[test]
fn test_logger_with_timing() {
    let mut logger = TestLogger::new("test_timing_workflow", 2);

    test_step!(logger, "Timed operation");
    let result = test_timed!(logger, "expensive_op", {
        std::thread::sleep(Duration::from_millis(20));
        42
    });
    test_assert!(logger, result == 42, "Operation returned correct value");
    test_step_ok!(logger, "completed");

    test_step!(logger, "Fast operation");
    test_step_ok!(logger);

    let result = logger.finish();
    assert!(result.passed);
    assert!(result.total_duration >= Duration::from_millis(20));
}

#[test]
fn test_logger_step_failure_handling() {
    let mut logger = TestLogger::new("test_step_failure", 2);

    test_step!(logger, "Successful step");
    test_step_ok!(logger, "done");

    test_step!(logger, "Failing step");
    test_step_err!(logger, "intentional failure for testing");

    let result = logger.finish();

    assert!(!result.passed);
    assert!(result.failure_reason.is_some());
    assert!(result.failure_reason.as_ref().unwrap().contains("failed"));
}

#[test]
fn test_logger_assertion_failure_handling() {
    let mut logger = TestLogger::new("test_assertion_failure", 1);

    test_step!(logger, "Testing assertions");
    test_assert!(logger, true, "This passes");
    test_assert!(logger, false, "This fails"); // Intentional failure
    test_step_ok!(logger);

    let result = logger.finish();

    assert!(!result.passed);
    assert_eq!(result.passed_assertion_count(), 1);
    assert_eq!(result.failed_assertion_count(), 1);
}

#[test]
fn test_logger_dynamic_steps() {
    let mut logger = TestLogger::new_dynamic("test_dynamic_workflow");

    for i in 1..=5 {
        test_step!(logger, "Processing item {}", i);
        test_step_ok!(logger, "item {} processed", i);
    }

    let result = logger.finish();

    assert!(result.passed);
    assert_eq!(result.steps.len(), 5);
}

#[test]
fn test_result_json_output() {
    let mut logger = TestLogger::new("test_json_output", 2);

    test_step!(logger, "First step");
    test_assert!(logger, true, "Test assertion");
    test_step_ok!(logger);

    test_step!(logger, "Second step");
    test_step_ok!(logger);

    let result = logger.finish();
    let json = result.to_json();

    // Verify JSON structure
    assert!(json.contains("\"test_name\":\"test_json_output\""));
    assert!(json.contains("\"passed\":true"));
    assert!(json.contains("First step"));
    assert!(json.contains("Test assertion"));
}

#[test]
fn test_logger_finish_failed() {
    let mut logger = TestLogger::new("test_explicit_failure", 1);

    test_step!(logger, "Starting");
    // Simulate finding a critical error
    let result = logger.finish_failed("Critical error occurred");

    assert!(!result.passed);
    assert_eq!(result.failure_reason, Some("Critical error occurred".to_string()));
}

#[test]
fn test_logger_skip_step() {
    let mut logger = TestLogger::new("test_skip_workflow", 3);

    test_step!(logger, "Required step");
    test_step_ok!(logger);

    test_step!(logger, "Optional step");
    logger.step_skip("Not needed for this configuration");

    test_step!(logger, "Final step");
    test_step_ok!(logger);

    let result = logger.finish();

    assert!(result.passed); // Skipped steps don't cause failure
    assert_eq!(result.steps.len(), 3);
}

#[test]
fn test_logger_multiple_assertions_per_step() {
    let mut logger = TestLogger::new("test_multiple_assertions", 1);

    test_step!(logger, "Comprehensive validation");

    let frame_width = 640;
    let frame_height = 480;
    let pixel_format = "RGBA32";
    let data_size = 640 * 480 * 4;

    test_assert_eq!(logger, frame_width, 640, "Width matches");
    test_assert_eq!(logger, frame_height, 480, "Height matches");
    test_assert_eq!(logger, pixel_format, "RGBA32", "Format matches");
    test_assert_eq!(logger, data_size, 1228800, "Data size correct");
    test_assert!(logger, data_size == frame_width * frame_height * 4, "Size formula correct");

    test_step_ok!(logger, "All {} assertions passed", 5);

    let result = logger.finish();

    assert!(result.passed);
    assert_eq!(result.assertions.len(), 5);
}

/// This test is timing-sensitive and may fail under system load.
/// Run with: cargo test test_step_timing_accuracy -- --ignored
#[test]
#[ignore]
fn test_step_timing_accuracy() {
    let mut logger = TestLogger::new("test_step_timing", 1);

    test_step!(logger, "Timed step");
    std::thread::sleep(Duration::from_millis(50));
    test_step_ok!(logger);

    let result = logger.finish();

    // Step duration should be at least 50ms
    assert!(result.steps[0].duration >= Duration::from_millis(50));
    // But not too much more (accounting for scheduling)
    assert!(result.steps[0].duration < Duration::from_millis(200));
}
