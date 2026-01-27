//! Configuration and persistence
//!
//! Handles loading, saving, and managing application settings.

use serde::{Deserialize, Serialize};
use crate::core::{DeviceId, DisplayId, ScalingMode, Rotation, Flip, ConfigError};

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Selected camera device
    pub camera_id: Option<DeviceId>,
    /// Target display
    pub display_id: Option<DisplayId>,
    /// Scaling mode
    #[serde(default)]
    pub scaling: ScalingMode,
    /// Rotation
    #[serde(default)]
    pub rotation: Rotation,
    /// Flip
    #[serde(default)]
    pub flip: Flip,
    /// Launch at system startup
    #[serde(default)]
    pub launch_at_startup: bool,
    /// Auto-start feed on launch
    #[serde(default)]
    pub auto_start: bool,
    /// Path to original wallpaper (for restore)
    pub original_wallpaper: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            camera_id: None,
            display_id: None,
            scaling: ScalingMode::default(),
            rotation: Rotation::default(),
            flip: Flip::default(),
            launch_at_startup: false,
            auto_start: false,
            original_wallpaper: None,
        }
    }
}

/// Load configuration from disk
pub fn load_config() -> Result<AppConfig, ConfigError> {
    // TODO: Implement config loading from ~/.config/micround/config.toml
    Ok(AppConfig::default())
}

/// Save configuration to disk
pub fn save_config(_config: &AppConfig) -> Result<(), ConfigError> {
    // TODO: Implement config saving
    Ok(())
}

/// Get the config file path
pub fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("micround")
        .join("config.toml")
}
