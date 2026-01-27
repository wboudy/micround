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

pub mod display;
pub mod window;
pub mod system;
pub mod paths;

#[cfg(test)]
pub mod mock;

pub use display::*;
pub use window::*;
pub use system::*;
pub use paths::*;
