# Micround Testing Guide

> **AGENT NOTICE**: Agents must NOT run local build/test commands.
> All testing happens via GitHub Actions CI. See `AGENTS.md` for
> the CI-first workflow. The commands below are for human developers only.

This document describes the testing infrastructure, conventions, and best practices for the Micround project.

## Test Categories

### Unit Tests
Located within source files in `src/*/` modules as `#[cfg(test)]` modules.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_function_scenario() {
        // Test implementation
    }
}
```

### Integration Tests
Located in `tests/` directory:
- `tests/assertions_test.rs` - Frame comparison and assertion utilities
- `tests/event_bus_flow.rs` - Event system integration
- `tests/fixtures_test.rs` - Fixture loading validation
- `tests/test_logger_test.rs` - Test logging system

### Test Utilities
Located in `tests/common/`:
- `assertions.rs` - Frame comparison, timing assertions
- `test_logger.rs` - Structured test logging with step tracking
- `mod.rs` - Re-exports for easy imports

### Test Fixtures
Located in `tests/fixtures/`:
- `frames/` - Programmatically generated test frames
- `configs/` - TOML configuration files for testing
- `devices/` - JSON device descriptor fixtures

See `tests/fixtures/README.md` for detailed fixture documentation.

## Running Tests

### All Tests
```bash
# Default feature set
cargo test

# With Linux features
cargo test --features linux

# With Windows features
cargo test --features windows

# With macOS features
cargo test --features macos
```

### Specific Test Modules
```bash
# Run decode tests
cargo test decode::

# Run capture tests
cargo test capture::

# Run process tests
cargo test process::
```

### Skip Flaky Tests
Some tests are timing-sensitive and may fail under load:
```bash
cargo test -- --skip test_step_timing --skip test_start_and_stop_monitor
```

### Run Ignored Tests
Tests requiring hardware (camera, display) are ignored by default:
```bash
cargo test --features linux -- --ignored
```

## Test Naming Convention

Tests follow the pattern: `test_{component}_{function}_{scenario}`

Examples:
- `test_decoded_frame_size` - Tests DecodedFrame size calculation
- `test_yuyv_decode_basic` - Tests basic YUYV decoding
- `test_render_without_init` - Tests render behavior when not initialized

## Code Coverage

### Local Coverage
```bash
# Install cargo-tarpaulin
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --features linux --out Html

# View report
open tarpaulin-report.html
```

### CI Coverage
Coverage is automatically collected on GitHub Actions and uploaded to Codecov.
See `.github/workflows/ci.yml` for configuration.

### Coverage Targets
- Project-wide: 50% minimum
- New code (patches): 40% minimum

## CI/CD Pipeline

GitHub Actions runs on every push/PR to `main` or `develop`:

1. **Lint** - Format check (`cargo fmt`) and Clippy warnings
2. **Test (Linux)** - Build and test with `--features linux`
3. **Test (Windows)** - Build and test with `--features windows`
4. **Test (macOS)** - Build and test with `--features macos`
5. **Coverage** - Generate coverage report with tarpaulin
6. **Docs** - Build Rust documentation

All jobs must pass for CI to succeed.

## Writing Tests

### Basic Test Structure
```rust
#[test]
fn test_feature_behavior() {
    // Arrange
    let input = create_test_input();

    // Act
    let result = function_under_test(input);

    // Assert
    assert!(result.is_ok());
    assert_eq!(result.unwrap().value, expected_value);
}
```

### Using Test Fixtures
```rust
use crate::tests::fixtures::frames::*;

#[test]
fn test_decode_yuyv_frame() {
    let frame = yuyv_color_bars_640x480();
    let result = decode_yuyv(&frame.data, frame.width, frame.height);
    assert!(result.is_ok());
}
```

### Using Assertions
```rust
use crate::tests::common::assertions::*;

#[test]
fn test_frame_similarity() {
    let frame1 = create_test_frame();
    let frame2 = process(frame1.clone());

    assert_frame_metadata_eq(&frame1, &frame2);
    assert_frames_similar(&frame1, &frame2, 0.95); // 95% PSNR threshold
}
```

### Using Test Logger
```rust
use crate::tests::common::test_logger::*;

#[test]
fn test_complex_workflow() {
    let logger = TestLogger::new("complex_workflow");

    logger.step("Initialize components");
    let component = Component::new();
    logger.assertion("Component created", component.is_valid());

    logger.step("Process data");
    let result = component.process(data);
    logger.assertion("Processing succeeded", result.is_ok());

    logger.finish();
}
```

## Mocking (Future)

Mock implementations will be provided in `src/platform/mock.rs`:
- `MockCaptureBackend` - Simulates camera capture
- `MockWallpaperRenderer` - Headless rendering
- `MockDisplayEnumerator` - Virtual displays

## Platform-Specific Tests

Tests requiring platform features should be gated:

```rust
#[test]
#[cfg(feature = "linux")]
fn test_v4l2_enumeration() {
    // Linux-specific test
}

#[test]
#[cfg(target_os = "windows")]
fn test_windows_wallpaper() {
    // Windows-specific test
}
```

## Debugging Failing Tests

### Enable Debug Logging
```bash
RUST_LOG=debug cargo test test_name -- --nocapture
```

### Run Single Test with Backtrace
```bash
RUST_BACKTRACE=1 cargo test test_name -- --nocapture
```

### Verbose Output
```bash
cargo test -- --nocapture --test-threads=1
```

## Best Practices

1. **Deterministic** - Tests should produce consistent results
2. **Isolated** - Tests should not depend on each other
3. **Fast** - Keep tests quick; use fixtures over real I/O
4. **Descriptive** - Test names should explain what's being tested
5. **Focused** - Each test should verify one specific behavior
6. **Documented** - Add comments for non-obvious test logic

## Known Limitations

- Camera tests require actual hardware (`--ignored` flag)
- X11 tests require display (`--ignored` flag)
- Timing tests may be flaky under load
- Coverage excludes `tests/` directory and `main.rs`
