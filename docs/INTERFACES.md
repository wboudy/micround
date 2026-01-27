# Micround Core Data Types & Interfaces

This document defines the fundamental data types and interfaces that all components use.
These are the "contracts" between system layers and must be stable before implementation.

## Design Principles

1. **Zero-Copy Where Possible**: Use reference-counted smart pointers for frame data
2. **Type Safety**: Leverage Rust's type system to prevent invalid states
3. **Serializable Errors**: All errors carry context for logging/debugging
4. **Monotonic Timestamps**: Use monotonic clock for latency measurement, wall clock for display

---

## Frame Types

### RawFrame

Represents a frame as captured from the camera, in its native format.

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pixel format as delivered by the camera
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Motion JPEG compressed
    Mjpeg,
    /// YUV 4:2:2 packed (YUYV/YUY2)
    Yuyv,
    /// YUV 4:2:0 semi-planar (NV12)
    Nv12,
    /// RGB 24-bit packed (RGB888)
    Rgb24,
    /// BGR 24-bit packed (BGR888)
    Bgr24,
    /// RGBA 32-bit packed
    Rgba32,
    /// Unknown/unsupported format
    Unknown(u32),
}

impl PixelFormat {
    /// Returns bytes per pixel (None for compressed formats)
    pub fn bytes_per_pixel(&self) -> Option<usize> {
        match self {
            Self::Mjpeg => None, // Compressed
            Self::Yuyv => Some(2),
            Self::Nv12 => None, // Planar
            Self::Rgb24 | Self::Bgr24 => Some(3),
            Self::Rgba32 => Some(4),
            Self::Unknown(_) => None,
        }
    }
}

/// A frame as captured from the camera
#[derive(Clone)]
pub struct RawFrame {
    /// Frame pixel data (reference-counted for zero-copy sharing)
    pub data: Arc<[u8]>,
    /// Pixel format of the data
    pub format: PixelFormat,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Monotonic capture timestamp (for latency measurement)
    pub capture_time: Instant,
    /// Frame sequence number (for drop detection)
    pub sequence: u64,
    /// Source device identifier
    pub device_id: DeviceId,
}

impl RawFrame {
    /// Estimated size in bytes (for memory tracking)
    pub fn size_bytes(&self) -> usize {
        self.data.len()
    }
}
```

### ProcessedFrame

Represents a frame ready for display, always in RGBA format.

```rust
/// A frame ready for rendering, always RGBA8
#[derive(Clone)]
pub struct ProcessedFrame {
    /// RGBA pixel data (reference-counted)
    pub data: Arc<[u8]>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Original capture timestamp (preserved for latency tracking)
    pub capture_time: Instant,
    /// Processing completion timestamp
    pub process_time: Instant,
    /// Frame sequence (preserved from RawFrame)
    pub sequence: u64,
    /// Target display(s) for this frame
    pub targets: Vec<DisplayId>,
}

impl ProcessedFrame {
    /// Time spent in processing pipeline
    pub fn processing_latency(&self) -> Duration {
        self.process_time.duration_since(self.capture_time)
    }

    /// Size in bytes (always width * height * 4 for RGBA)
    pub fn size_bytes(&self) -> usize {
        (self.width * self.height * 4) as usize
    }
}
```

### FrameMetrics

Lightweight frame timing information for diagnostics.

```rust
/// Frame timing metrics for performance monitoring
#[derive(Debug, Clone, Copy)]
pub struct FrameMetrics {
    /// Sequence number
    pub sequence: u64,
    /// Time from capture to processing complete
    pub capture_to_process: Duration,
    /// Time from processing to render complete
    pub process_to_render: Duration,
    /// Total end-to-end latency
    pub total_latency: Duration,
    /// Whether this frame was dropped
    pub dropped: bool,
}
```

---

## Device Types

### DeviceId

Persistent identifier for cameras, survives reconnection.

```rust
/// Persistent camera identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceId {
    /// Platform-specific unique path (e.g., USB path, device node)
    pub path: String,
    /// Vendor ID (if available)
    pub vendor_id: Option<u16>,
    /// Product ID (if available)
    pub product_id: Option<u16>,
    /// Serial number (if available, most reliable for persistence)
    pub serial: Option<String>,
}

impl DeviceId {
    /// Create a new device ID from path only
    pub fn from_path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            vendor_id: None,
            product_id: None,
            serial: None,
        }
    }

    /// Best-effort persistent identifier string
    pub fn persistent_id(&self) -> String {
        if let Some(ref serial) = self.serial {
            format!("serial:{}", serial)
        } else if let (Some(vid), Some(pid)) = (self.vendor_id, self.product_id) {
            format!("usb:{:04x}:{:04x}", vid, pid)
        } else {
            format!("path:{}", self.path)
        }
    }
}
```

### CameraDevice

Represents an enumerated camera with its capabilities.

```rust
/// Resolution and frame rate capability
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraCapability {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: PixelFormat,
}

impl CameraCapability {
    /// Megapixels for this resolution
    pub fn megapixels(&self) -> f32 {
        (self.width * self.height) as f32 / 1_000_000.0
    }
}

/// An enumerated camera device
#[derive(Debug, Clone)]
pub struct CameraDevice {
    /// Unique identifier
    pub id: DeviceId,
    /// Human-readable name
    pub name: String,
    /// Available capture modes
    pub capabilities: Vec<CameraCapability>,
    /// Whether the device is currently available
    pub available: bool,
}

impl CameraDevice {
    /// Find the best capability matching target resolution
    pub fn best_capability(&self, target_width: u32, target_height: u32) -> Option<&CameraCapability> {
        self.capabilities
            .iter()
            .filter(|c| c.width >= target_width && c.height >= target_height)
            .min_by_key(|c| (c.width - target_width) + (c.height - target_height))
    }
}
```

### DisplayId and DisplayInfo

Represents monitor/display information.

```rust
/// Unique display identifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DisplayId(pub String);

/// Information about a display/monitor
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Unique identifier
    pub id: DisplayId,
    /// Human-readable name
    pub name: String,
    /// Position in virtual desktop space (x, y)
    pub position: (i32, i32),
    /// Resolution in pixels (width, height)
    pub resolution: (u32, u32),
    /// DPI scale factor (1.0 = 96 DPI)
    pub scale_factor: f32,
    /// Whether this is the primary display
    pub is_primary: bool,
    /// Refresh rate in Hz
    pub refresh_rate: u32,
}

impl DisplayInfo {
    /// Physical resolution accounting for DPI scaling
    pub fn physical_resolution(&self) -> (u32, u32) {
        let (w, h) = self.resolution;
        (
            (w as f32 * self.scale_factor) as u32,
            (h as f32 * self.scale_factor) as u32,
        )
    }
}
```

---

## Configuration Types

### ScaleMode

How to fit the camera feed to the display.

```rust
/// How to scale/fit the feed to the display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ScaleMode {
    /// Fit entire feed within display, letterbox/pillarbox as needed
    Fit,
    /// Fill entire display, cropping as needed (default)
    #[default]
    Fill,
    /// Stretch to fill, ignoring aspect ratio
    Stretch,
    /// Center at native resolution, no scaling
    Center,
}
```

### Rotation and Flip

Transform settings for the feed.

```rust
/// Feed rotation in 90-degree increments
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum Rotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Clockwise270,
}

impl Rotation {
    pub fn degrees(&self) -> u16 {
        match self {
            Self::None => 0,
            Self::Clockwise90 => 90,
            Self::Clockwise180 => 180,
            Self::Clockwise270 => 270,
        }
    }
}

/// Feed flip/mirror settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Flip {
    pub horizontal: bool,
    pub vertical: bool,
}
```

### TransformSettings

Complete transform configuration.

```rust
/// Complete feed transform settings
#[derive(Debug, Clone, PartialEq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct TransformSettings {
    pub scale_mode: ScaleMode,
    pub rotation: Rotation,
    pub flip: Flip,
}
```

### OverlaySettings

Configuration for text overlays.

```rust
/// Position for overlay elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub enum OverlayPosition {
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}

/// Overlay configuration
#[derive(Debug, Clone, PartialEq, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct OverlaySettings {
    /// Show timestamp
    pub show_timestamp: bool,
    /// Custom text (empty = none)
    pub custom_text: String,
    /// Overlay position
    pub position: OverlayPosition,
    /// Font size in points
    pub font_size: u8,
}
```

### UserSettings

All user-configurable options (persisted to config file).

```rust
/// All user-configurable settings
#[derive(Debug, Clone, PartialEq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct UserSettings {
    /// Selected camera (by persistent ID)
    pub camera_id: Option<String>,
    /// Selected resolution (width, height)
    pub resolution: Option<(u32, u32)>,
    /// Target display(s)
    pub target_displays: Vec<String>,
    /// Transform settings
    pub transform: TransformSettings,
    /// Overlay settings
    pub overlay: OverlaySettings,
    /// Launch at system startup
    pub launch_at_startup: bool,
    /// Auto-start feed on launch
    pub auto_start_feed: bool,
    /// Path to fallback wallpaper (original wallpaper before app started)
    pub fallback_wallpaper: Option<String>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            camera_id: None,
            resolution: None,
            target_displays: vec![],
            transform: TransformSettings::default(),
            overlay: OverlaySettings::default(),
            launch_at_startup: false,
            auto_start_feed: false,
            fallback_wallpaper: None,
        }
    }
}
```

### AppState

Runtime application state (not persisted).

```rust
/// Runtime application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Initial state, no camera connected
    Idle,
    /// Starting up (connecting to camera)
    Starting,
    /// Running normally, displaying feed
    Running,
    /// Feed paused (frozen frame displayed)
    Paused,
    /// Attempting to reconnect after error
    Reconnecting {
        attempt: u32,
        max_attempts: u32,
    },
    /// Unrecoverable error state
    Error,
    /// Shutting down
    Stopping,
}

impl AppState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }

    pub fn can_start(&self) -> bool {
        matches!(self, Self::Idle | Self::Error)
    }
}
```

---

## Command & Event Types

### Command

Actions from UI to core components.

```rust
/// Commands from UI to core engine
#[derive(Debug, Clone)]
pub enum Command {
    /// Start capturing and displaying
    Start,
    /// Stop capturing, restore wallpaper
    Stop,
    /// Pause display (freeze current frame)
    Pause,
    /// Resume from pause
    Resume,
    /// Take snapshot to clipboard
    SnapshotToClipboard,
    /// Take snapshot to file
    SnapshotToFile { path: std::path::PathBuf },
    /// Select a different camera
    SelectCamera { device_id: DeviceId },
    /// Change capture resolution
    SetResolution { width: u32, height: u32 },
    /// Change target display(s)
    SetTargetDisplays { displays: Vec<DisplayId> },
    /// Update transform settings
    SetTransform { settings: TransformSettings },
    /// Update overlay settings
    SetOverlay { settings: OverlaySettings },
    /// Quit application
    Quit,
}
```

### Event

Notifications from core to UI and other components.

```rust
use std::time::Instant;

/// Events emitted by core engine
#[derive(Debug, Clone)]
pub enum Event {
    /// Application state changed
    StateChanged {
        old: AppState,
        new: AppState
    },
    /// New camera detected
    CameraConnected {
        device: CameraDevice
    },
    /// Camera disconnected
    CameraDisconnected {
        device_id: DeviceId
    },
    /// Display configuration changed
    DisplaysChanged {
        displays: Vec<DisplayInfo>
    },
    /// Frame processed and rendered
    FrameRendered {
        metrics: FrameMetrics
    },
    /// Snapshot completed
    SnapshotComplete {
        path: Option<std::path::PathBuf>
    },
    /// Error occurred (non-fatal)
    Error {
        error: MicroundError,
        recoverable: bool,
    },
    /// Performance warning
    PerformanceWarning {
        message: String,
        metric: String,
        value: f64,
        threshold: f64,
    },
}
```

---

## Error Types

### MicroundError

Structured error type for all components.

```rust
use thiserror::Error;

/// Top-level error type
#[derive(Error, Debug, Clone)]
pub enum MicroundError {
    #[error("Camera error: {message}")]
    Camera {
        message: String,
        device_id: Option<DeviceId>,
        #[source]
        source: Option<CaptureError>,
    },

    #[error("Render error: {message}")]
    Render {
        message: String,
        #[source]
        source: Option<RenderError>,
    },

    #[error("Configuration error: {message}")]
    Config {
        message: String,
        path: Option<std::path::PathBuf>,
    },

    #[error("Display error: {message}")]
    Display {
        message: String,
        display_id: Option<DisplayId>,
    },

    #[error("Permission denied: {message}")]
    Permission {
        message: String,
        resource: String,
    },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
    },
}

/// Capture-specific errors
#[derive(Error, Debug, Clone)]
pub enum CaptureError {
    #[error("Device not found")]
    DeviceNotFound,
    #[error("Device busy")]
    DeviceBusy,
    #[error("Unsupported format: {0:?}")]
    UnsupportedFormat(PixelFormat),
    #[error("Capture timeout")]
    Timeout,
    #[error("Device disconnected")]
    Disconnected,
    #[error("Platform error: {0}")]
    Platform(String),
}

/// Render-specific errors
#[derive(Error, Debug, Clone)]
pub enum RenderError {
    #[error("GPU initialization failed")]
    GpuInitFailed,
    #[error("Surface creation failed")]
    SurfaceCreationFailed,
    #[error("Shader compilation failed")]
    ShaderCompilationFailed,
    #[error("Texture upload failed")]
    TextureUploadFailed,
    #[error("Platform error: {0}")]
    Platform(String),
}
```

---

## Interface Traits

### CaptureBackend

Interface for platform-specific camera capture.

```rust
/// Callback type for frame delivery
pub type FrameCallback = Box<dyn Fn(RawFrame) + Send + 'static>;

/// Settings for opening a capture device
#[derive(Debug, Clone)]
pub struct CaptureSettings {
    pub resolution: (u32, u32),
    pub preferred_fps: u32,
    pub preferred_format: Option<PixelFormat>,
    /// Number of buffers to request (lower = less latency, higher = more stability)
    pub buffer_count: u32,
}

impl Default for CaptureSettings {
    fn default() -> Self {
        Self {
            resolution: (1920, 1080),
            preferred_fps: 30,
            preferred_format: None,
            buffer_count: 2,
        }
    }
}

/// Platform-specific capture implementation
pub trait CaptureBackend: Send {
    /// Enumerate available camera devices
    fn enumerate_devices(&self) -> Result<Vec<CameraDevice>, CaptureError>;

    /// Open a specific device with given settings
    fn open(&mut self, device_id: &DeviceId, settings: CaptureSettings) -> Result<(), CaptureError>;

    /// Start capturing frames
    fn start(&mut self) -> Result<(), CaptureError>;

    /// Stop capturing (device remains open)
    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Close the device
    fn close(&mut self);

    /// Register frame callback (called from capture thread)
    fn set_frame_callback(&mut self, callback: FrameCallback);

    /// Get current device info (if open)
    fn current_device(&self) -> Option<&CameraDevice>;

    /// Check if device is still connected
    fn is_connected(&self) -> bool;
}
```

### WallpaperBackend

Interface for platform-specific wallpaper rendering.

```rust
/// Platform-specific wallpaper rendering
pub trait WallpaperBackend: Send {
    /// Initialize the wallpaper surface for given displays
    fn initialize(&mut self, displays: &[DisplayInfo]) -> Result<(), RenderError>;

    /// Render a processed frame to the wallpaper
    fn render(&mut self, frame: &ProcessedFrame) -> Result<(), RenderError>;

    /// Save and store current wallpaper for later restoration
    fn save_current_wallpaper(&mut self) -> Result<Option<String>, RenderError>;

    /// Restore previously saved wallpaper
    fn restore_wallpaper(&mut self, path: Option<&str>) -> Result<(), RenderError>;

    /// Clean up resources
    fn shutdown(&mut self);

    /// Get supported displays
    fn enumerate_displays(&self) -> Result<Vec<DisplayInfo>, RenderError>;
}
```

### FrameProcessor

Interface for frame processing pipeline.

```rust
/// Frame processing pipeline
pub trait FrameProcessor: Send {
    /// Process a raw frame into a display-ready frame
    fn process(&mut self, raw: RawFrame, transform: &TransformSettings) -> Result<ProcessedFrame, RenderError>;

    /// Get supported input formats
    fn supported_formats(&self) -> &[PixelFormat];

    /// Release resources
    fn shutdown(&mut self);
}
```

---

## Channel Types

For inter-component communication.

```rust
use std::sync::mpsc;

/// Channel types for component communication
pub type CommandSender = mpsc::Sender<Command>;
pub type CommandReceiver = mpsc::Receiver<Command>;

pub type EventSender = mpsc::Sender<Event>;
pub type EventReceiver = mpsc::Receiver<Event>;

/// Frame channel (bounded to prevent memory growth)
pub type FrameSender = crossbeam_channel::Sender<RawFrame>;
pub type FrameReceiver = crossbeam_channel::Receiver<RawFrame>;
```

---

## Usage Notes

### Frame Lifetime

1. `RawFrame` created by capture backend, `Arc<[u8]>` allows zero-copy sharing
2. Processing pipeline consumes `RawFrame`, produces `ProcessedFrame`
3. Renderer consumes `ProcessedFrame`, uploads to GPU texture
4. Frame memory released when last reference dropped

### Timestamp Flow

```
Camera Hardware → capture_time (Instant::now() at driver callback)
                      ↓
              [Processing Pipeline]
                      ↓
               process_time (Instant::now() when processing completes)
                      ↓
              [Wallpaper Render]
                      ↓
               render_time (computed as delta for metrics)
```

### Error Recovery

1. `CaptureError::Disconnected` → Transition to `AppState::Reconnecting`
2. `RenderError::*` → Log, attempt reinitialize once, then `AppState::Error`
3. `MicroundError::Permission` → Show user notification with guidance

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-27 | Initial interface design |
