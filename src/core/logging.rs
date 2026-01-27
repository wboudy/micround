//! Logging infrastructure for Micround
//!
//! Provides structured logging with file output, rotation, and privacy-aware filtering.
//!
//! # Privacy Constraints
//! - NEVER log frame data or pixel values
//! - NEVER log file paths that might reveal user data
//! - DO log: device IDs, resolutions, frame counts, timings, errors

use std::path::PathBuf;
use tracing::Level;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Maximum log file size before rotation (10 MB)
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

/// Number of rotated log files to keep
const MAX_LOG_FILES: usize = 3;

/// Log file name
const LOG_FILE_NAME: &str = "micround.log";

/// Get the platform-specific log directory
pub fn log_directory() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // %LOCALAPPDATA%/Micround/logs/
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Micround")
            .join("logs")
    }

    #[cfg(target_os = "macos")]
    {
        // ~/Library/Logs/Micround/
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Logs")
            .join("Micround")
    }

    #[cfg(target_os = "linux")]
    {
        // XDG_DATA_HOME/Micround/logs/ or ~/.local/share/Micround/logs/
        dirs::data_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local")
                    .join("share")
            })
            .join("Micround")
            .join("logs")
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        PathBuf::from("logs")
    }
}

/// Initialize the logging system
///
/// Sets up:
/// - Console output with colors (when available)
/// - File output with rotation
/// - Per-component filtering via RUST_LOG env var
///
/// # Example
/// ```ignore
/// use micround::core::logging;
///
/// logging::init(Some(tracing::Level::DEBUG))?;
///
/// tracing::info!(component = "capture", "Starting camera capture");
/// ```
pub fn init(default_level: Option<Level>) -> Result<(), LoggingError> {
    let log_dir = log_directory();

    // Create log directory if it doesn't exist
    std::fs::create_dir_all(&log_dir).map_err(|e| LoggingError::IoError(e.to_string()))?;

    // Rotate existing logs if needed
    rotate_logs(&log_dir)?;

    let log_file_path = log_dir.join(LOG_FILE_NAME);

    // Create the log file
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .map_err(|e| LoggingError::IoError(e.to_string()))?;

    // Set up environment filter
    // Allows runtime configuration via RUST_LOG env var
    // Default: info for micround, warn for dependencies
    let default_directive = match default_level {
        Some(Level::TRACE) => "micround=trace,warn",
        Some(Level::DEBUG) => "micround=debug,warn",
        Some(Level::INFO) => "micround=info,warn",
        Some(Level::WARN) => "micround=warn,warn",
        Some(Level::ERROR) => "micround=error,error",
        None => "micround=info,warn",
    };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_directive));

    // Console layer - human-readable format
    let console_layer = fmt::layer()
        .with_target(true)
        .with_level(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .compact();

    // File layer - structured format with timestamps
    let file_layer = fmt::layer()
        .with_writer(log_file)
        .with_target(true)
        .with_level(true)
        .with_thread_ids(true)
        .with_file(false) // Don't log source file paths
        .with_line_number(false)
        .with_ansi(false) // No ANSI colors in file
        .with_span_events(FmtSpan::CLOSE); // Log span durations

    // Initialize the subscriber
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .try_init()
        .map_err(|e: tracing_subscriber::util::TryInitError| LoggingError::InitError(e.to_string()))?;

    tracing::info!(
        log_dir = %log_dir.display(),
        level = %default_level.unwrap_or(Level::INFO),
        "Logging initialized"
    );

    Ok(())
}

/// Initialize logging for testing (console only, no file)
#[cfg(test)]
pub fn init_test() {
    let _ = tracing_subscriber::fmt()
        .with_test_writer()
        .with_max_level(Level::DEBUG)
        .try_init();
}

/// Rotate log files if the current one exceeds MAX_LOG_SIZE
fn rotate_logs(log_dir: &PathBuf) -> Result<(), LoggingError> {
    let log_file = log_dir.join(LOG_FILE_NAME);

    if !log_file.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(&log_file).map_err(|e| LoggingError::IoError(e.to_string()))?;

    if metadata.len() < MAX_LOG_SIZE {
        return Ok(());
    }

    // Rotate: micround.log.2 -> micround.log.3, etc.
    for i in (1..MAX_LOG_FILES).rev() {
        let from = log_dir.join(format!("{}.{}", LOG_FILE_NAME, i));
        let to = log_dir.join(format!("{}.{}", LOG_FILE_NAME, i + 1));
        if from.exists() {
            let _ = std::fs::rename(&from, &to);
        }
    }

    // Current log becomes .1
    let rotated = log_dir.join(format!("{}.1", LOG_FILE_NAME));
    std::fs::rename(&log_file, &rotated).map_err(|e| LoggingError::IoError(e.to_string()))?;

    // Delete oldest if we have too many
    let oldest = log_dir.join(format!("{}.{}", LOG_FILE_NAME, MAX_LOG_FILES + 1));
    let _ = std::fs::remove_file(&oldest);

    Ok(())
}

/// Logging-specific errors
#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    #[error("Failed to initialize logging: {0}")]
    InitError(String),

    #[error("IO error: {0}")]
    IoError(String),
}

/// Convenience macros for component-tagged logging
///
/// Usage:
/// ```ignore
/// log_capture!(info, "Camera connected", device_id = %id);
/// log_render!(debug, "Frame rendered", latency_ms = elapsed);
/// log_process!(trace, "Color conversion complete");
/// ```
#[macro_export]
macro_rules! log_capture {
    ($level:ident, $($arg:tt)*) => {
        tracing::$level!(target: "micround::capture", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_render {
    ($level:ident, $($arg:tt)*) => {
        tracing::$level!(target: "micround::render", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_process {
    ($level:ident, $($arg:tt)*) => {
        tracing::$level!(target: "micround::process", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_ui {
    ($level:ident, $($arg:tt)*) => {
        tracing::$level!(target: "micround::ui", $($arg)*)
    };
}

#[macro_export]
macro_rules! log_config {
    ($level:ident, $($arg:tt)*) => {
        tracing::$level!(target: "micround::config", $($arg)*)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_directory_is_valid() {
        let dir = log_directory();
        // Should be a reasonable path, not empty
        assert!(!dir.as_os_str().is_empty());
    }

    #[test]
    fn test_rotate_logs_empty_dir() {
        let temp_dir = std::env::temp_dir().join("micround_test_logs");
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Should succeed even with no log file
        assert!(rotate_logs(&temp_dir).is_ok());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
