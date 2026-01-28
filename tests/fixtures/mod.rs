//! Test fixtures for Micround
//!
//! This module provides sample data for reproducible testing.
//!
//! # Structure
//!
//! ```text
//! tests/fixtures/
//! ├── mod.rs              # This file - fixture module root
//! ├── frames/             # Sample frame data generators
//! │   └── mod.rs          # Frame fixture generators
//! ├── configs/            # Sample configuration files
//! │   ├── valid_config.toml
//! │   ├── minimal_config.toml
//! │   ├── invalid_config.toml
//! │   ├── legacy_v0_config.toml
//! │   ├── edge_cases_config.toml
//! │   └── windows_paths_config.toml
//! ├── devices/            # Device descriptor JSON files
//! │   ├── cameras.json
//! │   ├── displays.json
//! │   ├── multi_monitor.json
//! │   └── edge_case_devices.json
//! └── README.md           # Documentation
//! ```
//!
//! # Usage
//!
//! ```ignore
//! mod fixtures;
//!
//! // Frame fixtures
//! let frame = fixtures::frames::rgba_color_bars_640x480();
//! let frame = fixtures::frames::yuyv_gradient_1280x720();
//!
//! // Config fixtures
//! let config_toml = fixtures::load_config("valid_config.toml");
//!
//! // Device fixtures
//! let cameras = fixtures::load_devices("cameras.json");
//! ```

pub mod frames;

use std::path::{Path, PathBuf};
use std::fs;
use serde::de::DeserializeOwned;

// ============================================================================
// Path Resolution
// ============================================================================

/// Get the path to the fixtures directory
pub fn fixtures_dir() -> PathBuf {
    // Works whether running from project root or tests directory
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    manifest_dir.join("tests").join("fixtures")
}

/// Get the path to a specific fixture subdirectory
pub fn fixtures_subdir(subdir: &str) -> PathBuf {
    fixtures_dir().join(subdir)
}

// ============================================================================
// Config Loading
// ============================================================================

/// Load a TOML config fixture by name
///
/// # Example
/// ```ignore
/// let toml_str = load_config_str("valid_config.toml")?;
/// let config: AppConfig = toml::from_str(&toml_str)?;
/// ```
pub fn load_config_str(name: &str) -> Result<String, std::io::Error> {
    let path = fixtures_subdir("configs").join(name);
    fs::read_to_string(&path)
}

/// Load and parse a TOML config fixture
///
/// Returns the raw TOML value for flexible testing.
pub fn load_config_toml(name: &str) -> Result<toml::Value, FixtureError> {
    let content = load_config_str(name)?;
    toml::from_str(&content).map_err(|e| FixtureError::ParseError(e.to_string()))
}

/// List available config fixtures
pub fn list_config_fixtures() -> Vec<String> {
    let dir = fixtures_subdir("configs");
    if let Ok(entries) = fs::read_dir(&dir) {
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "toml"))
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect()
    } else {
        vec![]
    }
}

// ============================================================================
// Device Loading
// ============================================================================

/// Load a JSON device fixture by name
pub fn load_devices_str(name: &str) -> Result<String, std::io::Error> {
    let path = fixtures_subdir("devices").join(name);
    fs::read_to_string(&path)
}

/// Load and parse a JSON device fixture
pub fn load_devices_json(name: &str) -> Result<serde_json::Value, FixtureError> {
    let content = load_devices_str(name)?;
    serde_json::from_str(&content).map_err(|e| FixtureError::ParseError(e.to_string()))
}

/// Load a JSON fixture and deserialize to a specific type
pub fn load_devices_as<T: DeserializeOwned>(name: &str) -> Result<T, FixtureError> {
    let content = load_devices_str(name)?;
    serde_json::from_str(&content).map_err(|e| FixtureError::ParseError(e.to_string()))
}

/// List available device fixtures
pub fn list_device_fixtures() -> Vec<String> {
    let dir = fixtures_subdir("devices");
    if let Ok(entries) = fs::read_dir(&dir) {
        entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect()
    } else {
        vec![]
    }
}

// ============================================================================
// Camera Device Types (for deserialization)
// ============================================================================

/// Camera device descriptor from fixtures
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CameraDeviceFixture {
    pub id: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub is_available: bool,
    pub capabilities: Vec<CameraCapabilityFixture>,
}

/// Camera capability from fixtures
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CameraCapabilityFixture {
    pub width: u32,
    pub height: u32,
    pub framerate: f32,
    pub format: String,
}

/// Cameras fixture file structure
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CamerasFixture {
    pub description: String,
    pub devices: Vec<CameraDeviceFixture>,
}

// ============================================================================
// Display Types (for deserialization)
// ============================================================================

/// Display bounds
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DisplayBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Display descriptor from fixtures
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DisplayFixture {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub bounds: DisplayBounds,
    pub dpi: u32,
    pub refresh_rate: f32,
}

/// Displays fixture file structure
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DisplaysFixture {
    pub description: String,
    pub displays: Vec<DisplayFixture>,
}

// ============================================================================
// Error Type
// ============================================================================

/// Error loading or parsing fixtures
#[derive(Debug)]
pub enum FixtureError {
    IoError(std::io::Error),
    ParseError(String),
}

impl std::fmt::Display for FixtureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {}", e),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for FixtureError {}

impl From<std::io::Error> for FixtureError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Load the standard cameras fixture
pub fn load_cameras() -> Result<CamerasFixture, FixtureError> {
    load_devices_as("cameras.json")
}

/// Load the standard displays fixture
pub fn load_displays() -> Result<DisplaysFixture, FixtureError> {
    load_devices_as("displays.json")
}

/// Load the multi-monitor displays fixture
pub fn load_multi_monitor() -> Result<DisplaysFixture, FixtureError> {
    load_devices_as("multi_monitor.json")
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixtures_dir_exists() {
        let dir = fixtures_dir();
        assert!(dir.exists(), "Fixtures directory should exist: {:?}", dir);
    }

    #[test]
    fn test_load_valid_config() {
        let content = load_config_str("valid_config.toml").unwrap();
        assert!(content.contains("version = 1"));
        assert!(content.contains("[camera]"));
    }

    #[test]
    fn test_load_cameras_json() {
        let cameras = load_cameras().unwrap();
        assert!(!cameras.devices.is_empty());
        assert!(cameras.devices.iter().any(|d| d.name.contains("Logitech")));
    }

    #[test]
    fn test_load_displays_json() {
        let displays = load_displays().unwrap();
        assert!(!displays.displays.is_empty());
        assert!(displays.displays.iter().any(|d| d.is_primary));
    }

    #[test]
    fn test_list_fixtures() {
        let configs = list_config_fixtures();
        assert!(configs.iter().any(|f| f == "valid_config.toml"));

        let devices = list_device_fixtures();
        assert!(devices.iter().any(|f| f == "cameras.json"));
    }

    #[test]
    fn test_frame_fixtures_available() {
        // Test that frame generators work
        let frame = frames::rgba_color_bars_640x480();
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);

        let frame = frames::yuyv_gradient_1280x720();
        assert_eq!(frame.width, 1280);
        assert_eq!(frame.height, 720);
    }
}
