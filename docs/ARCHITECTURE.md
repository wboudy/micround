# Micround Architecture & Technology Decisions

This document captures the foundational technology decisions for the Micround project.

## Technology Stack

### 1. Programming Language: Rust

**Decision**: Use Rust as the primary implementation language.

**Rationale**:
- **Memory safety without GC**: No garbage collection pauses that could impact latency
- **Performance**: Zero-cost abstractions enable C/C++ level performance
- **Cross-platform tooling**: Cargo and cross-compilation support simplify multi-platform builds
- **Strong type system**: Catches bugs at compile time, reducing runtime errors
- **Concurrency safety**: Ownership model prevents data races in multi-threaded capture/render pipeline
- **Ecosystem**: Growing ecosystem with crates for video capture, graphics, and UI

**Tradeoffs acknowledged**:
- Steeper learning curve than Go or Python
- Longer initial development time
- Smaller contributor pool (mitigated by clear documentation)

### 2. UI Framework: egui + native window

**Decision**: Use egui for settings UI, with platform-native system tray integration.

**Rationale**:
- **Immediate mode**: Simple mental model, easy to iterate
- **Performance**: GPU-accelerated rendering with minimal overhead
- **Cross-platform**: Single codebase for Windows, macOS, Linux
- **Integration**: Works well with wgpu for graphics rendering
- **Lightweight**: Minimal dependencies, fast compile times

**Alternatives considered**:
- **iced**: More Elm-like, but less mature ecosystem
- **tauri**: Web-based UI adds latency and complexity
- **native bindings**: Maximum native feel but 3x maintenance burden

**System tray**: Use platform-native tray integration via:
- Windows: `windows-rs` + Shell_NotifyIcon
- macOS: `objc` crate + NSStatusItem
- Linux: `libappindicator` or D-Bus StatusNotifierItem

### 3. Video Capture: Direct Platform APIs

**Decision**: Use platform-native video capture APIs directly.

**Rationale**:
- **Lowest latency**: No abstraction layer overhead
- **Maximum control**: Can tune buffer counts, formats, timing
- **Hardware access**: Direct path to hardware acceleration
- **UVC coverage**: All platforms support UVC cameras natively

**Implementation**:
| Platform | API | Crate/Binding |
|----------|-----|---------------|
| Windows | Media Foundation | `windows-rs` |
| macOS | AVFoundation | `objc` + `block` crates |
| Linux | V4L2 | `v4l` crate |

**Abstraction layer**: A thin `CaptureBackend` trait will unify the platform implementations:
```rust
pub trait CaptureBackend {
    fn enumerate_devices(&self) -> Vec<CameraDevice>;
    fn open(&mut self, device_id: &DeviceId, settings: CaptureSettings) -> Result<()>;
    fn start(&mut self) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn close(&mut self);
    fn subscribe_frames(&mut self, callback: Box<dyn Fn(Frame) + Send>);
}
```

### 4. Graphics/Rendering: wgpu

**Decision**: Use wgpu for frame rendering and GPU-accelerated processing.

**Rationale**:
- **Cross-platform abstraction**: Single API over Vulkan, Metal, D3D12, WebGPU
- **Rust-native**: First-class Rust support, not a binding
- **Modern**: Compute shaders for frame processing
- **Well-maintained**: Active development, used by Firefox and other major projects

**Rendering pipeline**:
1. Camera frame → GPU texture upload
2. Compute shader: color conversion, scaling, transforms
3. Fragment shader: render to wallpaper surface
4. Compositor presents frame

**Platform-specific surfaces**:
- Windows: HWND surface via WorkerW injection
- macOS: NSView surface on desktop-level NSWindow
- Linux: X11 window surface or root window pixmap

### 5. Build System: Cargo + platform scripts

**Decision**: Use Cargo as primary build tool with platform-specific helper scripts.

**Configuration**:
- `Cargo.toml` with feature flags for platform-specific code
- `build.rs` for platform-specific build steps
- `scripts/` directory for installer/package generation

**Feature flags**:
```toml
[features]
default = []
windows = ["windows-rs", "wgpu/dx12"]
macos = ["objc", "block", "wgpu/metal"]
linux = ["v4l", "wgpu/vulkan"]
```

### 6. Error Handling: thiserror + anyhow

**Decision**: Use `thiserror` for library errors, `anyhow` for application errors.

**Rationale**:
- **thiserror**: Derives std::error::Error with minimal boilerplate
- **anyhow**: Easy error context and backtraces for debugging
- **Separation**: Library code uses typed errors; app code uses context-rich errors

### 7. Logging: tracing

**Decision**: Use the `tracing` crate for structured logging.

**Rationale**:
- **Structured**: Key-value pairs, not just strings
- **Spans**: Track latency across async operations
- **Filtering**: Runtime log level configuration
- **Ecosystem**: Wide adoption, good tooling

## Dependency Summary

### Core Dependencies
| Crate | Purpose | Version Policy |
|-------|---------|----------------|
| `wgpu` | GPU rendering | Latest stable |
| `egui` | Settings UI | Latest stable |
| `tracing` | Logging | Latest stable |
| `thiserror` | Error types | Latest stable |
| `anyhow` | App errors | Latest stable |
| `serde` | Serialization | Latest stable |
| `toml` | Config files | Latest stable |

### Platform-Specific
| Platform | Crates |
|----------|--------|
| Windows | `windows-rs` |
| macOS | `objc`, `block`, `cocoa` |
| Linux | `v4l`, `x11rb` or `xcb` |

## Architecture Principles

1. **Separation of Concerns**: Capture, processing, and rendering are independent modules
2. **Platform Abstraction**: Core logic is platform-agnostic; backends implement traits
3. **Fail Gracefully**: Errors propagate with context; app never crashes, always recovers
4. **Observable**: Comprehensive logging and metrics for debugging
5. **Testable**: Mock backends enable unit testing without hardware
6. **Privacy by Design**: No network, no recording, local-only operation (see PRIVACY.md)

## Privacy Architecture

Privacy is a foundational constraint, not an afterthought. See `docs/PRIVACY.md` for full details.

### Forbidden in Codebase
- Network client libraries (reqwest, hyper, etc.)
- Video file encoding/writing (except snapshot feature)
- Telemetry or analytics

### Required Patterns
- Frame buffers zeroed on drop
- No frame data in logs
- Paths sanitized before logging

## Performance Budget

| Metric | Target | Measurement |
|--------|--------|-------------|
| CPU usage | <10% single core | `tracing` spans + OS monitoring |
| GPU usage | <15% | GPU profiler |
| Memory | <200 MB resident | Valgrind/Instruments |
| Latency | <100ms p95 | Frame timestamp tracking |

## Next Steps

1. Set up Cargo workspace with feature flags
2. Implement `CaptureBackend` trait
3. Implement Windows Media Foundation backend (primary target)
4. Implement basic wgpu rendering pipeline
5. Create Windows prototype (bd-3re)
