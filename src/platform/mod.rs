//! Cross-platform abstraction layer
//!
//! This module provides platform-agnostic traits and types that hide OS-specific
//! differences. Core application logic uses these abstractions while platform-specific
//! backends provide the actual implementations.
//!
//! # Abstractions
#![allow(dead_code)] // Platform abstraction layer
#![allow(unused_imports)] // Library API re-exports
//!
//! - **Display**: Screen enumeration, geometry, DPI, change detection
//! - **DesktopWindow**: Low-level window at desktop level for wallpaper rendering
//! - **SystemEvents**: Sleep/wake, display connect/disconnect, power state
//! - **Paths**: Platform-appropriate config, log, and data directories
//! - **Wallpaper**: Backup and restore of user's original wallpaper

pub mod autostart;
pub mod display;
pub mod window;
pub mod system;
pub mod paths;
pub mod wallpaper;

#[cfg(test)]
pub mod mock;

pub use display::*;
pub use window::*;
pub use system::*;
pub use paths::*;
pub use wallpaper::{capture_wallpaper, restore_wallpaper, restore_wallpaper_from_path, WallpaperInfo};
pub use autostart::{
    is_autostart_enabled, enable_autostart, disable_autostart, set_autostart,
    AutostartError, AutostartResult,
};
