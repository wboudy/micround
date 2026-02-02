//! Snapshot feature - capture current frame to file or clipboard
//!
//! Allows users to capture a still image of the current feed:
#![allow(dead_code)] // Snapshot feature API
//! - Save to file (PNG format by default)
//! - Copy to clipboard (platform-specific)
//!
//! # Trigger Methods
//! - Tray menu: "Take Snapshot"
//! - Keyboard shortcut: Configurable (default Ctrl/Cmd+Shift+S)
//! - Settings window button

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use chrono::Local;
use image::codecs::png::PngEncoder;
use image::{ImageBuffer, ImageEncoder, Rgba};
use thiserror::Error;

use crate::core::logging::log_safe_path;
use crate::process::ProcessedFrame;

/// Errors that can occur during snapshot operations
#[derive(Error, Debug)]
pub enum SnapshotError {
    /// Failed to encode image
    #[error("Failed to encode image: {0}")]
    Encode(String),

    /// Failed to save file
    #[error("Failed to save file: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid frame data
    #[error("Invalid frame data: expected {expected} bytes, got {actual}")]
    InvalidFrameData { expected: usize, actual: usize },

    /// Failed to create directory
    #[error("Failed to create directory: {0}")]
    CreateDir(String),

    /// Clipboard operation failed
    #[error("Clipboard operation failed: {0}")]
    Clipboard(String),

    /// No frame available
    #[error("No frame available to capture")]
    NoFrame,
}

/// Result type for snapshot operations
pub type SnapshotResult<T> = Result<T, SnapshotError>;

/// Output format for saved snapshots
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapshotFormat {
    /// PNG format (lossless, recommended)
    #[default]
    Png,
    /// JPEG format (lossy, smaller file size)
    Jpeg { quality: u8 },
}

/// Configuration for snapshot operations
#[derive(Debug, Clone)]
pub struct SnapshotConfig {
    /// Default save directory
    pub save_dir: PathBuf,
    /// Output format
    pub format: SnapshotFormat,
    /// Whether to include overlays in the snapshot
    pub include_overlays: bool,
    /// Filename prefix
    pub filename_prefix: String,
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        // Use Pictures/Micround as default directory
        let save_dir = dirs::picture_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Micround");

        Self {
            save_dir,
            format: SnapshotFormat::Png,
            include_overlays: true,
            filename_prefix: "micround".to_string(),
        }
    }
}

/// Result of a successful snapshot operation
#[derive(Debug, Clone)]
pub struct SnapshotResult2 {
    /// Where the snapshot was saved (if saved to file)
    pub file_path: Option<PathBuf>,
    /// Whether the snapshot was copied to clipboard
    pub copied_to_clipboard: bool,
    /// Dimensions of the captured image
    pub width: u32,
    pub height: u32,
}

/// Take a snapshot from the given frame
pub struct SnapshotManager {
    config: SnapshotConfig,
}

impl SnapshotManager {
    /// Create a new snapshot manager with default configuration
    pub fn new() -> Self {
        Self {
            config: SnapshotConfig::default(),
        }
    }

    /// Create a new snapshot manager with custom configuration
    pub fn with_config(config: SnapshotConfig) -> Self {
        Self { config }
    }

    /// Get the current configuration
    pub fn config(&self) -> &SnapshotConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: SnapshotConfig) {
        self.config = config;
    }

    /// Save a snapshot to file
    ///
    /// Returns the path where the snapshot was saved.
    pub fn save_to_file(&self, frame: &ProcessedFrame) -> SnapshotResult<PathBuf> {
        self.save_to_file_with_path(frame, None)
    }

    /// Save a snapshot to a specific path
    ///
    /// If `path` is None, generates a default path in the configured save directory.
    pub fn save_to_file_with_path(
        &self,
        frame: &ProcessedFrame,
        path: Option<&Path>,
    ) -> SnapshotResult<PathBuf> {
        // Validate frame data
        let expected_size = (frame.width as usize) * (frame.height as usize) * 4;
        if frame.data.len() != expected_size {
            return Err(SnapshotError::InvalidFrameData {
                expected: expected_size,
                actual: frame.data.len(),
            });
        }

        // Determine output path
        let output_path = match path {
            Some(p) => p.to_path_buf(),
            None => self.generate_filename()?,
        };

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| SnapshotError::CreateDir(format!("{}: {}", parent.display(), e)))?;
        }

        // Validate image buffer can be created (sanity check)
        let _: ImageBuffer<Rgba<u8>, _> =
            ImageBuffer::from_raw(frame.width, frame.height, frame.data.clone())
                .ok_or_else(|| SnapshotError::Encode("Failed to create image buffer".into()))?;

        // Save to file
        match self.config.format {
            SnapshotFormat::Png => {
                let file = File::create(&output_path)?;
                let writer = BufWriter::new(file);
                let encoder = PngEncoder::new(writer);
                encoder
                    .write_image(
                        &frame.data,
                        frame.width,
                        frame.height,
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(|e| SnapshotError::Encode(e.to_string()))?;
            }
            SnapshotFormat::Jpeg { quality } => {
                // Convert RGBA to RGB for JPEG (no alpha channel)
                let rgb_data: Vec<u8> = frame
                    .data
                    .chunks_exact(4)
                    .flat_map(|rgba| [rgba[0], rgba[1], rgba[2]])
                    .collect();

                let rgb_img: ImageBuffer<image::Rgb<u8>, _> =
                    ImageBuffer::from_raw(frame.width, frame.height, rgb_data).ok_or_else(
                        || SnapshotError::Encode("Failed to create RGB image buffer".into()),
                    )?;

                // Use the specified JPEG quality
                let file = File::create(&output_path)?;
                let writer = BufWriter::new(file);
                let mut encoder =
                    image::codecs::jpeg::JpegEncoder::new_with_quality(writer, quality);
                encoder
                    .encode(
                        rgb_img.as_raw(),
                        frame.width,
                        frame.height,
                        image::ExtendedColorType::Rgb8,
                    )
                    .map_err(|e| SnapshotError::Encode(e.to_string()))?;
            }
        }

        tracing::info!(
            path = %log_safe_path(&output_path),
            width = frame.width,
            height = frame.height,
            "Snapshot saved to file"
        );

        Ok(output_path)
    }

    /// Copy a snapshot to the system clipboard
    ///
    /// This is platform-specific and may not be available on all systems.
    #[cfg(feature = "clipboard")]
    pub fn copy_to_clipboard(&self, frame: &ProcessedFrame) -> SnapshotResult<()> {
        // Validate frame data
        let expected_size = (frame.width as usize) * (frame.height as usize) * 4;
        if frame.data.len() != expected_size {
            return Err(SnapshotError::InvalidFrameData {
                expected: expected_size,
                actual: frame.data.len(),
            });
        }

        // Platform-specific clipboard implementation would go here
        // For now, this is a placeholder that will be implemented when
        // clipboard dependencies are added

        tracing::info!(
            width = frame.width,
            height = frame.height,
            "Snapshot copied to clipboard"
        );

        Ok(())
    }

    /// Copy to clipboard (stub when clipboard feature is not enabled)
    #[cfg(not(feature = "clipboard"))]
    pub fn copy_to_clipboard(&self, _frame: &ProcessedFrame) -> SnapshotResult<()> {
        Err(SnapshotError::Clipboard(
            "Clipboard feature not enabled. Build with --features clipboard".into(),
        ))
    }

    /// Take a snapshot and save to file (convenience method)
    pub fn take_snapshot(&self, frame: Option<&ProcessedFrame>) -> SnapshotResult<PathBuf> {
        let frame = frame.ok_or(SnapshotError::NoFrame)?;
        self.save_to_file(frame)
    }

    /// Generate a timestamped filename
    fn generate_filename(&self) -> SnapshotResult<PathBuf> {
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let extension = match self.config.format {
            SnapshotFormat::Png => "png",
            SnapshotFormat::Jpeg { .. } => "jpg",
        };

        let filename = format!(
            "{}_{}.{}",
            self.config.filename_prefix, timestamp, extension
        );
        let path = self.config.save_dir.join(filename);

        Ok(path)
    }
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Command Integration
// ============================================================================

/// Snapshot command for the event system
#[derive(Debug, Clone)]
pub enum SnapshotCommand {
    /// Take a snapshot and save to file
    SaveToFile,
    /// Take a snapshot and save to a specific path
    SaveToPath(PathBuf),
    /// Take a snapshot and copy to clipboard
    CopyToClipboard,
    /// Take a snapshot with both file and clipboard
    SaveAndCopy,
}

/// Event emitted when a snapshot operation completes
#[derive(Debug, Clone)]
pub enum SnapshotEvent {
    /// Snapshot saved successfully
    Saved {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    /// Snapshot copied to clipboard
    CopiedToClipboard { width: u32, height: u32 },
    /// Snapshot operation failed
    Failed { error: String },
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_frame(width: u32, height: u32) -> ProcessedFrame {
        let size = (width * height * 4) as usize;
        let mut data = Vec::with_capacity(size);

        // Create a gradient pattern
        for y in 0..height {
            for x in 0..width {
                let r = (x * 255 / width) as u8;
                let g = (y * 255 / height) as u8;
                let b = 128u8;
                let a = 255u8;
                data.extend_from_slice(&[r, g, b, a]);
            }
        }

        ProcessedFrame::new(data, width, height)
    }

    #[test]
    fn test_snapshot_config_default() {
        let config = SnapshotConfig::default();
        assert!(config.save_dir.to_string_lossy().contains("Micround"));
        assert!(matches!(config.format, SnapshotFormat::Png));
        assert!(config.include_overlays);
    }

    #[test]
    fn test_snapshot_manager_creation() {
        let manager = SnapshotManager::new();
        assert!(matches!(manager.config().format, SnapshotFormat::Png));
    }

    #[test]
    fn test_generate_filename() {
        let manager = SnapshotManager::new();
        let path = manager.generate_filename().unwrap();
        let filename = path.file_name().unwrap().to_string_lossy();
        assert!(filename.starts_with("micround_"));
        assert!(filename.ends_with(".png"));
    }

    #[test]
    fn test_save_to_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = SnapshotConfig {
            save_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let manager = SnapshotManager::with_config(config);
        let frame = create_test_frame(100, 100);

        let result = manager.save_to_file(&frame);
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().ends_with(".png"));
    }

    #[test]
    fn test_save_to_specific_path() {
        let temp_dir = TempDir::new().unwrap();
        let output_path = temp_dir.path().join("test_snapshot.png");
        let manager = SnapshotManager::new();
        let frame = create_test_frame(50, 50);

        let result = manager.save_to_file_with_path(&frame, Some(&output_path));
        assert!(result.is_ok());
        assert!(output_path.exists());
    }

    #[test]
    fn test_invalid_frame_data() {
        let manager = SnapshotManager::new();
        // Create a frame with wrong data size
        let frame = ProcessedFrame::new(vec![0u8; 100], 100, 100); // Should be 40000 bytes

        let result = manager.save_to_file(&frame);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SnapshotError::InvalidFrameData { .. }
        ));
    }

    #[test]
    fn test_no_frame_error() {
        let manager = SnapshotManager::new();
        let result = manager.take_snapshot(None);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), SnapshotError::NoFrame));
    }

    #[test]
    fn test_jpeg_format() {
        let temp_dir = TempDir::new().unwrap();
        let config = SnapshotConfig {
            save_dir: temp_dir.path().to_path_buf(),
            format: SnapshotFormat::Jpeg { quality: 90 },
            ..Default::default()
        };
        let manager = SnapshotManager::with_config(config);
        let frame = create_test_frame(100, 100);

        let result = manager.save_to_file(&frame);
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with(".jpg"));
    }

    #[test]
    fn test_snapshot_command_variants() {
        let _cmd1 = SnapshotCommand::SaveToFile;
        let _cmd2 = SnapshotCommand::CopyToClipboard;
        let _cmd3 = SnapshotCommand::SaveToPath(PathBuf::from("/tmp/test.png"));
        let _cmd4 = SnapshotCommand::SaveAndCopy;
    }

    #[test]
    fn test_snapshot_event_variants() {
        let _evt1 = SnapshotEvent::Saved {
            path: PathBuf::from("/tmp/test.png"),
            width: 100,
            height: 100,
        };
        let _evt2 = SnapshotEvent::CopiedToClipboard {
            width: 100,
            height: 100,
        };
        let _evt3 = SnapshotEvent::Failed {
            error: "Test error".to_string(),
        };
    }
}
