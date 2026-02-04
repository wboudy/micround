//! Platform-specific path abstractions
//!
//! Provides platform-appropriate directories for application data, configuration,
//! logs, and temporary files.
//!
//! # Platform Directories
//!
//! | Purpose | Windows | macOS | Linux |
//! |---------|---------|-------|-------|
//! | Config  | %APPDATA%\Micround | ~/Library/Application Support/Micround | ~/.config/micround |
//! | Data    | %LOCALAPPDATA%\Micround | ~/Library/Application Support/Micround | ~/.local/share/micround |
//! | Cache   | %LOCALAPPDATA%\Micround\cache | ~/Library/Caches/Micround | ~/.cache/micround |
//! | Logs    | %LOCALAPPDATA%\Micround\logs | ~/Library/Logs/Micround | ~/.local/state/micround/logs |

use std::path::PathBuf;

/// Application name used for directory naming
const APP_NAME: &str = "Micround";

/// Error type for path operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum PathError {
    #[error("Could not determine home directory")]
    NoHomeDir,

    #[error("Could not determine {0} directory")]
    NoDirFor(&'static str),

    #[error("Failed to create directory: {0}")]
    CreateFailed(String),

    #[error("Path is not valid UTF-8")]
    InvalidUtf8,
}

impl PathError {
    /// Get the severity of this error
    pub fn severity(&self) -> crate::core::ErrorSeverity {
        match self {
            Self::NoHomeDir | Self::NoDirFor(_) => crate::core::ErrorSeverity::Fatal,
            Self::CreateFailed(_) => crate::core::ErrorSeverity::UserActionable,
            Self::InvalidUtf8 => crate::core::ErrorSeverity::Fatal,
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::NoHomeDir => {
                "Unable to find your home directory. Please check your system configuration.".into()
            }
            Self::NoDirFor(purpose) => {
                format!("Unable to find the {} directory. Please check your system configuration.", purpose)
            }
            Self::CreateFailed(_) => {
                "Unable to create application directory. Please check that you have write permissions.".into()
            }
            Self::InvalidUtf8 => {
                "A file path contains invalid characters. Please use standard characters in directory names.".into()
            }
        }
    }
}

/// Application directories for different types of data
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Configuration files (user-editable settings)
    pub config: PathBuf,
    /// Application data (non-config persistent data)
    pub data: PathBuf,
    /// Cache files (can be deleted without data loss)
    pub cache: PathBuf,
    /// Log files
    pub logs: PathBuf,
}

impl AppPaths {
    /// Get platform-appropriate application paths
    pub fn new() -> Result<Self, PathError> {
        Ok(Self {
            config: config_dir()?,
            data: data_dir()?,
            cache: cache_dir()?,
            logs: logs_dir()?,
        })
    }

    /// Ensure all directories exist
    pub fn ensure_dirs(&self) -> Result<(), PathError> {
        for dir in [&self.config, &self.data, &self.cache, &self.logs] {
            if !dir.exists() {
                std::fs::create_dir_all(dir).map_err(|e| PathError::CreateFailed(e.to_string()))?;
            }
        }
        Ok(())
    }

    /// Get path to the main config file
    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    /// Get path to store original wallpaper reference
    pub fn wallpaper_backup_file(&self) -> PathBuf {
        self.data.join("original_wallpaper.txt")
    }

    /// Get path for crash recovery state
    pub fn state_file(&self) -> PathBuf {
        self.data.join("state.json")
    }
}

/// Get the configuration directory
///
/// - Windows: %APPDATA%\Micround
/// - macOS: ~/Library/Application Support/Micround
/// - Linux: $XDG_CONFIG_HOME/micround or ~/.config/micround
pub fn config_dir() -> Result<PathBuf, PathError> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .map(|p| p.join(APP_NAME))
            .map_err(|_| PathError::NoDirFor("APPDATA"))
    }

    #[cfg(target_os = "macos")]
    {
        dirs::config_dir()
            .map(|p| p.join(APP_NAME))
            .ok_or(PathError::NoDirFor("Application Support"))
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".config")
            })
            .join(APP_NAME.to_lowercase())
            .pipe(Ok)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        dirs::config_dir()
            .map(|p| p.join(APP_NAME))
            .ok_or(PathError::NoDirFor("config"))
    }
}

/// Get the application data directory
///
/// - Windows: %LOCALAPPDATA%\Micround
/// - macOS: ~/Library/Application Support/Micround
/// - Linux: $XDG_DATA_HOME/micround or ~/.local/share/micround
pub fn data_dir() -> Result<PathBuf, PathError> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join(APP_NAME))
            .map_err(|_| PathError::NoDirFor("LOCALAPPDATA"))
    }

    #[cfg(target_os = "macos")]
    {
        // macOS uses the same dir for config and data
        config_dir()
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".local")
                    .join("share")
            })
            .join(APP_NAME.to_lowercase())
            .pipe(Ok)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        dirs::data_dir()
            .map(|p| p.join(APP_NAME))
            .ok_or(PathError::NoDirFor("data"))
    }
}

/// Get the cache directory
///
/// - Windows: %LOCALAPPDATA%\Micround\cache
/// - macOS: ~/Library/Caches/Micround
/// - Linux: $XDG_CACHE_HOME/micround or ~/.cache/micround
pub fn cache_dir() -> Result<PathBuf, PathError> {
    #[cfg(target_os = "windows")]
    {
        data_dir().map(|p| p.join("cache"))
    }

    #[cfg(target_os = "macos")]
    {
        dirs::cache_dir()
            .map(|p| p.join(APP_NAME))
            .ok_or(PathError::NoDirFor("Caches"))
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".cache")
            })
            .join(APP_NAME.to_lowercase())
            .pipe(Ok)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        dirs::cache_dir()
            .map(|p| p.join(APP_NAME))
            .ok_or(PathError::NoDirFor("cache"))
    }
}

/// Get the logs directory
///
/// - Windows: %LOCALAPPDATA%\Micround\logs
/// - macOS: ~/Library/Logs/Micround
/// - Linux: $XDG_STATE_HOME/micround/logs or ~/.local/state/micround/logs
pub fn logs_dir() -> Result<PathBuf, PathError> {
    #[cfg(target_os = "windows")]
    {
        data_dir().map(|p| p.join("logs"))
    }

    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .map(|p| p.join("Library").join("Logs").join(APP_NAME))
            .ok_or(PathError::NoHomeDir)
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("~"))
                    .join(".local")
                    .join("state")
            })
            .join(APP_NAME.to_lowercase())
            .join("logs")
            .pipe(Ok)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        data_dir().map(|p| p.join("logs"))
    }
}

/// Get a temporary directory for this application
///
/// Creates a subdirectory in the system temp dir to avoid conflicts.
pub fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(APP_NAME.to_lowercase())
}

/// Helper trait for pipeline-style transformations
trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}

impl<T> Pipe for T {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_paths_creation() {
        let paths = AppPaths::new();
        assert!(paths.is_ok());

        let paths = paths.unwrap();
        assert!(paths.config.to_string_lossy().contains("icround"));
        assert!(paths.logs.to_string_lossy().contains("icround"));
    }

    #[test]
    fn test_config_file_path() {
        let paths = AppPaths::new().unwrap();
        let config_file = paths.config_file();
        assert!(config_file.to_string_lossy().ends_with("config.toml"));
    }

    #[test]
    fn test_temp_dir() {
        let temp = temp_dir();
        assert!(temp.to_string_lossy().contains("micround"));
    }
}
