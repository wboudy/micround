//! Cross-platform abstraction layer
//!
//! This module provides platform-agnostic traits and types that hide OS-specific
//! differences. Core application logic uses these abstractions while platform-specific
//! backends provide the actual implementations.
//!
//! # Abstractions
//!
//! - **Display**: Screen enumeration, geometry, DPI, change detection
//! - **DesktopWindow**: Low-level window at desktop level for wallpaper rendering
//! - **SystemEvents**: Sleep/wake, display connect/disconnect, power state
//! - **Paths**: Platform-appropriate config, log, and data directories
//! - **Wallpaper**: Backup and restore of user's original wallpaper

pub mod autostart;
pub mod display;
pub mod paths;
pub mod permissions;
pub mod system;
pub mod wallpaper;
pub mod window;

#[cfg(test)]
pub mod mock;

pub use autostart::{
    disable_autostart, enable_autostart, is_autostart_enabled, set_autostart, AutostartError,
    AutostartResult,
};
pub use display::*;
pub use paths::*;
pub use permissions::{
    create_permission_handler, ActivityGuard, ActivityType, CameraPermission, PermissionError,
    PermissionHandler,
};
pub use system::*;
pub use wallpaper::{
    capture_wallpaper, restore_wallpaper, restore_wallpaper_from_path, WallpaperInfo,
};
pub use window::*;
