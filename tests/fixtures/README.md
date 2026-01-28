# Test Fixtures

This directory contains test data fixtures for reproducible testing of Micround.

## Structure

```
fixtures/
├── mod.rs              # Rust module with fixture loaders
├── README.md           # This file
├── frames/             # Sample frame data generators
│   └── mod.rs          # Frame fixture generators
├── configs/            # Sample configuration files
│   ├── valid_config.toml       # Full valid configuration
│   ├── minimal_config.toml     # Minimum required fields only
│   ├── invalid_config.toml     # Malformed for error testing
│   ├── legacy_v0_config.toml   # Old format for migration testing
│   ├── edge_cases_config.toml  # Unicode paths, special chars
│   └── windows_paths_config.toml # Windows path handling
└── devices/            # Device descriptor JSON files
    ├── cameras.json            # Sample camera devices
    ├── displays.json           # Simple dual-monitor setup
    ├── multi_monitor.json      # Complex 4-monitor setup
    └── edge_case_devices.json  # Unusual device configurations
```

## Usage

### Frame Fixtures

Frames are generated programmatically to avoid storing large binary files:

```rust
use fixtures::frames::*;

// RGBA frames
let frame = rgba_color_bars_640x480();
let frame = rgba_gradient_1280x720();
let frame = rgba_checkerboard_1920x1080();
let frame = rgba_corner_markers_100x100();

// YUYV frames
let frame = yuyv_color_bars_640x480();
let frame = yuyv_gradient_1280x720();

// NV12 frames
let frame = nv12_color_bars_640x480();
let frame = nv12_checkerboard_1920x1080();

// RGB24 frames
let frame = rgb24_gradient_640x480();

// Corrupted frames (for error handling)
let frame = corrupted_truncated_frame();
let frame = corrupted_zero_dimensions();
let frame = corrupted_format_mismatch();

// Dynamic loading by name
let frame = get_frame_by_name("rgba_color_bars_640x480");
```

### Config Fixtures

```rust
use fixtures::*;

// Load as raw string
let toml_str = load_config_str("valid_config.toml")?;

// Load as parsed TOML
let value = load_config_toml("valid_config.toml")?;

// List available fixtures
let configs = list_config_fixtures();
```

### Device Fixtures

```rust
use fixtures::*;

// Load cameras
let cameras: CamerasFixture = load_cameras()?;
for camera in cameras.devices {
    println!("{}: {}", camera.id, camera.name);
}

// Load displays
let displays: DisplaysFixture = load_displays()?;
let displays: DisplaysFixture = load_multi_monitor()?;

// Load as raw JSON
let json = load_devices_json("cameras.json")?;
```

## Frame Fixture Details

### Available Patterns

| Name | Description | Use Case |
|------|-------------|----------|
| `color_bars` | SMPTE-style color bars (8 colors) | Color accuracy, format conversion |
| `gradient` | Horizontal grayscale gradient | Scaling artifacts, interpolation |
| `checkerboard` | Alternating black/white blocks | Edge detection, transformation |
| `corner_markers` | Colored corners (R/G/B/Y) on gray | Rotation, flip verification |

### Available Formats

| Format | Bytes/Pixel | Notes |
|--------|-------------|-------|
| RGBA32 | 4 | Standard output format |
| RGB24 | 3 | No alpha channel |
| YUYV | 2 | Packed YUV 4:2:2 |
| NV12 | 1.5 | Planar Y + interleaved UV |

### Standard Resolutions

- 640x480 (VGA)
- 1280x720 (720p)
- 1920x1080 (1080p)

## Config Fixture Details

| File | Purpose |
|------|---------|
| `valid_config.toml` | Complete valid configuration with all fields |
| `minimal_config.toml` | Only required fields (tests defaults) |
| `invalid_config.toml` | Malformed TOML (tests error handling) |
| `legacy_v0_config.toml` | Old format (tests migration) |
| `edge_cases_config.toml` | Unicode, special chars, extreme values |
| `windows_paths_config.toml` | Windows-specific path formats |

## Device Fixture Details

### cameras.json

Contains 4 sample cameras:
- Logitech HD Webcam C270 (720p max)
- Logitech C920 HD Pro (1080p)
- Generic USB Microscope (typical microscope)
- Disconnected camera (tests unavailable devices)

### displays.json

Simple dual-monitor setup:
- 4K primary (163 DPI)
- 1080p secondary (144Hz gaming monitor)

### multi_monitor.json

Complex 4-monitor setup with:
- 4K primary (centered vertically)
- 1440p gaming (165Hz, offset)
- TV (negative X coordinates)
- Vertical monitor (portrait mode)

## Regenerating Fixtures

Frame fixtures are generated at test runtime, so no regeneration needed.

For config/device fixtures, edit the files directly. All fixture files are
committed to git for reproducibility.

## Adding New Fixtures

1. **Frames**: Add generator functions to `frames/mod.rs`
2. **Configs**: Add `.toml` files to `configs/`
3. **Devices**: Add `.json` files to `devices/`
4. **Loaders**: Update `mod.rs` if new types are needed
