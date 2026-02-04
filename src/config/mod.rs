//! Configuration and persistence
//!
//! Handles loading, saving, and managing application settings.
//!
//! # File Format
//!
//! Configuration is stored as TOML for human readability:
//!
//! ```toml
//! version = 1
//!
//! [camera]
//! device_id = "USB\\VID_1234&PID_5678\\..."
//! width = 1920
//! height = 1080
//! framerate = 30.0
//!
//! [display]
//! target = "primary"
//! scaling_mode = "fill"
//! rotation = 0
//! flip_horizontal = false
//! flip_vertical = false
//!
//! [startup]
//! launch_at_login = false
//! auto_start_feed = true
//!
//! [internal]
//! original_wallpaper_path = "/path/to/original.jpg"
//! last_clean_shutdown = true
//! ```
//!
//! # Storage Location
//!
//! - Windows: `%APPDATA%\Micround\config.toml`
//! - macOS: `~/Library/Application Support/Micround/config.toml`
//! - Linux: `~/.config/micround/config.toml`

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::core::{ConfigError, DeviceId, DisplayId, Flip, Rotation, ScalingMode};

/// Current configuration schema version
pub const CONFIG_VERSION: u32 = 1;

/// Application configuration
///
/// All fields have defaults and the config is forward-compatible
/// (unknown fields are ignored on load).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Configuration schema version (for migrations)
    pub version: u32,
    /// Camera settings
    pub camera: CameraConfig,
    /// Display settings
    pub display: DisplayConfig,
    /// Startup behavior
    pub startup: StartupConfig,
    /// Internal state (not user-editable)
    pub internal: InternalConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            camera: CameraConfig::default(),
            display: DisplayConfig::default(),
            startup: StartupConfig::default(),
            internal: InternalConfig::default(),
        }
    }
}

impl AppConfig {
    /// Validate the configuration, returning errors for invalid values
    pub fn validate(&self) -> Vec<ConfigValidationError> {
        let mut errors = Vec::new();

        // Validate camera settings
        if self.camera.width == 0 || self.camera.height == 0 {
            errors.push(ConfigValidationError {
                field: "camera.width/height".into(),
                message: "Resolution must be non-zero".into(),
            });
        }

        if self.camera.framerate <= 0.0
            || self.camera.framerate > 240.0
            || !self.camera.framerate.is_finite()
        {
            errors.push(ConfigValidationError {
                field: "camera.framerate".into(),
                message: "Framerate must be a finite number between 0 and 240".into(),
            });
        }

        // Validate rotation
        if !matches!(self.display.rotation, 0 | 90 | 180 | 270) {
            errors.push(ConfigValidationError {
                field: "display.rotation".into(),
                message: "Rotation must be 0, 90, 180, or 270".into(),
            });
        }

        errors
    }

    /// Apply defaults for any invalid values
    pub fn sanitize(&mut self) {
        if self.camera.width == 0 {
            self.camera.width = 1920;
        }
        if self.camera.height == 0 {
            self.camera.height = 1080;
        }
        if self.camera.framerate <= 0.0
            || self.camera.framerate > 240.0
            || !self.camera.framerate.is_finite()
        {
            self.camera.framerate = 30.0;
        }
        if !matches!(self.display.rotation, 0 | 90 | 180 | 270) {
            self.display.rotation = 0;
        }
    }
}

/// Camera-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraConfig {
    /// Selected camera device ID (None = auto-select first available)
    pub device_id: Option<DeviceId>,
    /// Preferred capture width
    pub width: u32,
    /// Preferred capture height
    pub height: u32,
    /// Preferred framerate
    pub framerate: f32,
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self {
            device_id: None,
            width: 1920,
            height: 1080,
            framerate: 30.0,
        }
    }
}

/// Display-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Target display ID (None = primary display)
    pub display_id: Option<DisplayId>,
    /// Scaling mode
    pub scaling_mode: ScalingMode,
    /// Rotation in degrees (0, 90, 180, 270)
    pub rotation: u32,
    /// Horizontal flip
    pub flip_horizontal: bool,
    /// Vertical flip
    pub flip_vertical: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            display_id: None,
            scaling_mode: ScalingMode::default(),
            rotation: 0,
            flip_horizontal: false,
            flip_vertical: false,
        }
    }
}

impl DisplayConfig {
    /// Convert rotation degrees to Rotation enum
    pub fn rotation_enum(&self) -> Rotation {
        match self.rotation {
            90 => Rotation::Clockwise90,
            180 => Rotation::Clockwise180,
            270 => Rotation::Clockwise270,
            _ => Rotation::None,
        }
    }

    /// Convert flip booleans to Flip enum
    pub fn flip_enum(&self) -> Flip {
        match (self.flip_horizontal, self.flip_vertical) {
            (true, true) => Flip::Both,
            (true, false) => Flip::Horizontal,
            (false, true) => Flip::Vertical,
            (false, false) => Flip::None,
        }
    }
}

/// Startup behavior configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupConfig {
    /// Launch at system login
    pub launch_at_login: bool,
    /// Automatically start camera feed on launch
    pub auto_start_feed: bool,
    /// Minimize to tray on startup (if auto-starting)
    pub minimize_on_start: bool,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            auto_start_feed: false,
            minimize_on_start: false,
        }
    }
}

/// Internal state (managed by application, not user-editable)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InternalConfig {
    /// Path to original wallpaper (for restore on exit)
    pub original_wallpaper_path: Option<String>,
    /// Whether last shutdown was clean
    pub last_clean_shutdown: bool,
    /// Last used camera (for reconnection)
    pub last_camera_id: Option<DeviceId>,
}

impl Default for InternalConfig {
    fn default() -> Self {
        Self {
            original_wallpaper_path: None,
            last_clean_shutdown: true,
            last_camera_id: None,
        }
    }
}

/// Validation error for a config field
#[derive(Debug, Clone)]
pub struct ConfigValidationError {
    pub field: String,
    pub message: String,
}

/// Get the configuration directory path
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("micround")
}

/// Get the config file path
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Get the backup config file path
pub fn config_backup_path() -> PathBuf {
    config_dir().join("config.toml.bak")
}

/// Load configuration from disk
///
/// # Behavior
/// - File missing: Returns defaults
/// - File corrupted: Creates backup, returns defaults, logs warning
/// - Unknown fields: Ignored (forward compatibility)
/// - Invalid values: Sanitized to defaults, logs warning
pub fn load_config() -> Result<AppConfig, ConfigError> {
    let path = config_path();

    // If file doesn't exist, return defaults
    if !path.exists() {
        tracing::info!("Config file not found, using defaults");
        return Ok(AppConfig::default());
    }

    // Read file
    let contents = fs::read_to_string(&path).map_err(|e| {
        tracing::warn!(error = %e, "Failed to read config file");
        ConfigError::ReadFailed(e.to_string())
    })?;

    // Parse TOML
    let mut config: AppConfig = match toml::from_str(&contents) {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to parse config file, creating backup");
            // Create backup of corrupted file
            let backup_path = config_backup_path();
            if let Err(backup_err) = fs::copy(&path, &backup_path) {
                tracing::warn!(error = %backup_err, "Failed to create config backup");
            }
            return Ok(AppConfig::default());
        }
    };

    // Validate and sanitize
    let errors = config.validate();
    if !errors.is_empty() {
        for error in &errors {
            tracing::warn!(
                field = %error.field,
                message = %error.message,
                "Invalid config value, using default"
            );
        }
        config.sanitize();
    }

    // Handle version migrations
    if config.version < CONFIG_VERSION {
        tracing::info!(
            old_version = config.version,
            new_version = CONFIG_VERSION,
            "Migrating config to new version"
        );
        config = migrate_config(config);
    }

    Ok(config)
}

/// Save configuration to disk
///
/// Uses atomic write (write to temp, rename) to prevent corruption.
pub fn save_config(config: &AppConfig) -> Result<(), ConfigError> {
    let path = config_path();
    let dir = config_dir();

    // Ensure directory exists
    fs::create_dir_all(&dir).map_err(|e| {
        ConfigError::WriteFailed(format!("Failed to create config directory: {}", e))
    })?;

    // Serialize to TOML
    let contents = toml::to_string_pretty(config)
        .map_err(|e| ConfigError::WriteFailed(format!("Failed to serialize config: {}", e)))?;

    // Write to temp file first (atomic write)
    let temp_path = dir.join("config.toml.tmp");
    {
        let mut file = fs::File::create(&temp_path)
            .map_err(|e| ConfigError::WriteFailed(format!("Failed to create temp file: {}", e)))?;
        file.write_all(contents.as_bytes())
            .map_err(|e| ConfigError::WriteFailed(format!("Failed to write temp file: {}", e)))?;
        file.sync_all()
            .map_err(|e| ConfigError::WriteFailed(format!("Failed to sync temp file: {}", e)))?;
    }

    // Rename temp to final (atomic on most filesystems)
    fs::rename(&temp_path, &path)
        .map_err(|e| ConfigError::WriteFailed(format!("Failed to rename config file: {}", e)))?;

    tracing::debug!(path = %path.display(), "Config saved");
    Ok(())
}

/// Migrate config from older version to current
fn migrate_config(mut config: AppConfig) -> AppConfig {
    // Currently only version 1, no migrations needed
    // Future migrations would go here:
    // if config.version == 1 { migrate_v1_to_v2(&mut config); }
    config.version = CONFIG_VERSION;
    config
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn with_temp_config_dir<F>(f: F)
    where
        F: FnOnce(&TempDir),
    {
        let temp_dir = TempDir::new().unwrap();
        // Note: We can't easily override dirs::config_dir(), so we test
        // the serialization/deserialization directly
        f(&temp_dir);
    }

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.camera.width, 1920);
        assert_eq!(config.camera.height, 1080);
        assert_eq!(config.camera.framerate, 30.0);
        assert!(!config.startup.launch_at_login);
    }

    #[test]
    fn test_config_serialization() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).expect("serialize");
        assert!(toml_str.contains("version = 1"));
        assert!(toml_str.contains("[camera]"));
        assert!(toml_str.contains("[display]"));
    }

    #[test]
    fn test_config_deserialization() {
        let toml_str = r#"
            version = 1

            [camera]
            width = 1280
            height = 720
            framerate = 60.0

            [display]
            scaling_mode = "Fit"
            rotation = 90
            flip_horizontal = true

            [startup]
            launch_at_login = true

            [internal]
            last_clean_shutdown = false
        "#;

        let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(config.camera.width, 1280);
        assert_eq!(config.camera.height, 720);
        assert_eq!(config.camera.framerate, 60.0);
        assert_eq!(config.display.rotation, 90);
        assert!(config.display.flip_horizontal);
        assert!(config.startup.launch_at_login);
        assert!(!config.internal.last_clean_shutdown);
    }

    #[test]
    fn test_config_forward_compatibility() {
        // Config with unknown fields should still parse
        let toml_str = r#"
            version = 1
            unknown_field = "ignored"

            [camera]
            width = 1920
            height = 1080
            framerate = 30.0
            future_setting = true

            [display]

            [some_future_section]
            new_feature = "value"
        "#;

        let config: AppConfig = toml::from_str(toml_str).expect("deserialize");
        assert_eq!(config.camera.width, 1920);
    }

    #[test]
    fn test_config_validation() {
        let mut config = AppConfig::default();
        config.camera.width = 0;
        config.camera.framerate = -5.0;
        config.display.rotation = 45; // Invalid

        let errors = config.validate();
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn test_config_sanitize() {
        let mut config = AppConfig::default();
        config.camera.width = 0;
        config.camera.framerate = -5.0;
        config.display.rotation = 45;

        config.sanitize();

        assert_eq!(config.camera.width, 1920);
        assert_eq!(config.camera.framerate, 30.0);
        assert_eq!(config.display.rotation, 0);
    }

    #[test]
    fn test_rotation_enum_conversion() {
        let mut display = DisplayConfig::default();

        display.rotation = 0;
        assert_eq!(display.rotation_enum(), Rotation::None);

        display.rotation = 90;
        assert_eq!(display.rotation_enum(), Rotation::Clockwise90);

        display.rotation = 180;
        assert_eq!(display.rotation_enum(), Rotation::Clockwise180);

        display.rotation = 270;
        assert_eq!(display.rotation_enum(), Rotation::Clockwise270);
    }

    #[test]
    fn test_flip_enum_conversion() {
        let mut display = DisplayConfig::default();

        display.flip_horizontal = false;
        display.flip_vertical = false;
        assert_eq!(display.flip_enum(), Flip::None);

        display.flip_horizontal = true;
        display.flip_vertical = false;
        assert_eq!(display.flip_enum(), Flip::Horizontal);

        display.flip_horizontal = false;
        display.flip_vertical = true;
        assert_eq!(display.flip_enum(), Flip::Vertical);

        display.flip_horizontal = true;
        display.flip_vertical = true;
        assert_eq!(display.flip_enum(), Flip::Both);
    }

    #[test]
    fn test_config_path() {
        let path = config_path();
        assert!(path.ends_with("config.toml"));
        assert!(path.to_string_lossy().contains("micround"));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        with_temp_config_dir(|temp_dir| {
            let config_file = temp_dir.path().join("config.toml");

            let mut config = AppConfig::default();
            config.camera.width = 1280;
            config.camera.height = 720;
            config.startup.launch_at_login = true;

            // Serialize
            let toml_str = toml::to_string_pretty(&config).expect("serialize");
            fs::write(&config_file, &toml_str).expect("write");

            // Read back and deserialize
            let read_str = fs::read_to_string(&config_file).expect("read");
            let loaded: AppConfig = toml::from_str(&read_str).expect("deserialize");

            assert_eq!(loaded.camera.width, 1280);
            assert_eq!(loaded.camera.height, 720);
            assert!(loaded.startup.launch_at_login);
        });
    }
}
