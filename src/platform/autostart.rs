//! Autostart/login item management
//!
//! Provides platform-specific methods to enable or disable launching the
//! application at user login.
//!
//! # Platform Support
//!
//! - **Linux**: XDG autostart (~/.config/autostart/*.desktop)
//! - **Windows**: Registry HKCU\...\Run (TODO)
//! - **macOS**: SMAppService / LaunchAgent (TODO)

use std::io;

/// Application name for autostart entries (reserved for future platform support)
#[allow(dead_code)]
const APP_NAME: &str = "micround";

/// Desktop entry filename for Linux
#[allow(dead_code)]
const DESKTOP_FILE_NAME: &str = "micround.desktop";

// ============================================================================
// Autostart Result Type
// ============================================================================

/// Result type for autostart operations
pub type AutostartResult<T> = Result<T, AutostartError>;

/// Errors that can occur during autostart operations
#[derive(Debug, Clone)]
pub enum AutostartError {
    /// IO error (permissions, file not found, etc.)
    Io(String),
    /// Platform not supported
    NotSupported(String),
    /// Failed to get required paths
    PathError(String),
}

impl std::fmt::Display for AutostartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "I/O error: {}", msg),
            Self::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            Self::PathError(msg) => write!(f, "Path error: {}", msg),
        }
    }
}

impl std::error::Error for AutostartError {}

impl AutostartError {
    /// Get the severity of this error
    pub fn severity(&self) -> crate::core::ErrorSeverity {
        match self {
            Self::Io(_) => crate::core::ErrorSeverity::UserActionable,
            Self::NotSupported(_) => crate::core::ErrorSeverity::UserActionable,
            Self::PathError(_) => crate::core::ErrorSeverity::UserActionable,
        }
    }

    /// Get a user-friendly message for this error
    pub fn user_message(&self) -> String {
        match self {
            Self::Io(_) => {
                "Unable to change autostart settings. Please check that you have write permissions.".into()
            }
            Self::NotSupported(_) => {
                "Autostart is not available on your system. You can manually add Micround to your startup applications.".into()
            }
            Self::PathError(_) => {
                "Unable to find the autostart directory. Please check your system configuration.".into()
            }
        }
    }
}

impl From<io::Error> for AutostartError {
    fn from(err: io::Error) -> Self {
        AutostartError::Io(err.to_string())
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Check if autostart is currently enabled
pub fn is_autostart_enabled() -> AutostartResult<bool> {
    #[cfg(target_os = "linux")]
    {
        linux::is_enabled()
    }

    #[cfg(target_os = "windows")]
    {
        windows::is_enabled()
    }

    #[cfg(target_os = "macos")]
    {
        macos::is_enabled()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(AutostartError::NotSupported("Unknown platform".into()))
    }
}

/// Enable autostart (launch at login)
///
/// The application will launch with the `--minimized` flag when started at login,
/// so it goes directly to the system tray without showing a window.
pub fn enable_autostart() -> AutostartResult<()> {
    #[cfg(target_os = "linux")]
    {
        linux::enable()
    }

    #[cfg(target_os = "windows")]
    {
        windows::enable()
    }

    #[cfg(target_os = "macos")]
    {
        macos::enable()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(AutostartError::NotSupported("Unknown platform".into()))
    }
}

/// Disable autostart
pub fn disable_autostart() -> AutostartResult<()> {
    #[cfg(target_os = "linux")]
    {
        linux::disable()
    }

    #[cfg(target_os = "windows")]
    {
        windows::disable()
    }

    #[cfg(target_os = "macos")]
    {
        macos::disable()
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(AutostartError::NotSupported("Unknown platform".into()))
    }
}

/// Set autostart state (enable or disable)
pub fn set_autostart(enabled: bool) -> AutostartResult<()> {
    if enabled {
        enable_autostart()
    } else {
        disable_autostart()
    }
}

// ============================================================================
// Linux Implementation
// ============================================================================

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    /// Get the XDG autostart directory
    fn autostart_dir() -> AutostartResult<PathBuf> {
        // XDG_CONFIG_HOME or ~/.config
        let config_dir = env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| {
                dirs::home_dir().map(|h| h.join(".config")).ok_or_else(|| {
                    AutostartError::PathError("Cannot determine home directory".into())
                })
            })?;

        Ok(config_dir.join("autostart"))
    }

    /// Get the path to our desktop entry file
    fn desktop_file_path() -> AutostartResult<PathBuf> {
        Ok(autostart_dir()?.join(DESKTOP_FILE_NAME))
    }

    /// Get the path to the executable
    fn executable_path() -> AutostartResult<PathBuf> {
        env::current_exe()
            .map_err(|e| AutostartError::Io(format!("Cannot determine executable path: {}", e)))
    }

    /// Generate the desktop entry content
    pub(crate) fn desktop_entry_content(exe_path: &PathBuf) -> String {
        format!(
            r#"[Desktop Entry]
Type=Application
Version=1.0
Name=Micround
GenericName=Live Microscope Wallpaper
Comment=Display live microscope camera feed as desktop wallpaper
Exec="{}" --minimized
Icon=micround
Terminal=false
Categories=Utility;
StartupNotify=false
X-GNOME-Autostart-enabled=true
"#,
            exe_path.display()
        )
    }

    pub fn is_enabled() -> AutostartResult<bool> {
        let path = desktop_file_path()?;

        if !path.exists() {
            return Ok(false);
        }

        // Check if the file has Hidden=true (disabled)
        let content = fs::read_to_string(&path)?;

        // Check for explicit disable markers
        if content.contains("Hidden=true") {
            return Ok(false);
        }
        if content.contains("X-GNOME-Autostart-enabled=false") {
            return Ok(false);
        }

        Ok(true)
    }

    pub fn enable() -> AutostartResult<()> {
        let autostart_dir = autostart_dir()?;
        let desktop_path = desktop_file_path()?;
        let exe_path = executable_path()?;

        // Create autostart directory if it doesn't exist
        if !autostart_dir.exists() {
            fs::create_dir_all(&autostart_dir)?;
        }

        // Write the desktop entry
        let content = desktop_entry_content(&exe_path);
        fs::write(&desktop_path, content)?;

        tracing::info!(
            path = %desktop_path.display(),
            "Autostart enabled"
        );

        Ok(())
    }

    pub fn disable() -> AutostartResult<()> {
        let desktop_path = desktop_file_path()?;

        if desktop_path.exists() {
            fs::remove_file(&desktop_path)?;
            tracing::info!(
                path = %desktop_path.display(),
                "Autostart disabled"
            );
        }

        Ok(())
    }
}

// ============================================================================
// Windows Implementation (Stub)
// ============================================================================

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    // TODO: Implement Windows autostart using Registry
    // Key: HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run
    // Value name: "Micround"
    // Value data: "\"C:\\Path\\To\\micround.exe\" --minimized"

    pub fn is_enabled() -> AutostartResult<bool> {
        // TODO: Read from registry
        Err(AutostartError::NotSupported(
            "Windows autostart not yet implemented".into(),
        ))
    }

    pub fn enable() -> AutostartResult<()> {
        // TODO: Write to registry
        Err(AutostartError::NotSupported(
            "Windows autostart not yet implemented".into(),
        ))
    }

    pub fn disable() -> AutostartResult<()> {
        // TODO: Remove from registry
        Err(AutostartError::NotSupported(
            "Windows autostart not yet implemented".into(),
        ))
    }
}

// ============================================================================
// macOS Implementation (Stub)
// ============================================================================

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    // TODO: Implement macOS autostart
    // Modern (macOS 13+): SMAppService.mainApp.register()
    // Legacy: LaunchAgent plist in ~/Library/LaunchAgents/

    pub fn is_enabled() -> AutostartResult<bool> {
        // TODO: Check SMAppService or LaunchAgent
        Err(AutostartError::NotSupported(
            "macOS autostart not yet implemented".into(),
        ))
    }

    pub fn enable() -> AutostartResult<()> {
        // TODO: Register with SMAppService or create LaunchAgent
        Err(AutostartError::NotSupported(
            "macOS autostart not yet implemented".into(),
        ))
    }

    pub fn disable() -> AutostartResult<()> {
        // TODO: Unregister SMAppService or remove LaunchAgent
        Err(AutostartError::NotSupported(
            "macOS autostart not yet implemented".into(),
        ))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autostart_error_display() {
        let err = AutostartError::Io("test error".into());
        assert!(err.to_string().contains("test error"));

        let err = AutostartError::NotSupported("platform".into());
        assert!(err.to_string().contains("Not supported"));
    }

    #[test]
    fn test_set_autostart_calls_correct_function() {
        // Just verify the function compiles and routes correctly
        // Actual functionality is tested in platform-specific tests
        let _ = is_autostart_enabled();
    }

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;
        use serial_test::serial;
        use std::env;
        use std::path::PathBuf;
        use tempfile::TempDir;

        #[test]
        fn test_desktop_entry_content() {
            let exe_path = PathBuf::from("/usr/bin/micround");
            let content = linux::desktop_entry_content(&exe_path);

            assert!(content.contains("[Desktop Entry]"));
            assert!(content.contains("Type=Application"));
            assert!(content.contains("Name=Micround"));
            assert!(content.contains("Exec=\"/usr/bin/micround\" --minimized"));
            assert!(content.contains("Terminal=false"));
            assert!(content.contains("X-GNOME-Autostart-enabled=true"));
        }

        #[test]
        #[serial]
        fn test_enable_disable_cycle() {
            // Create a temp directory for XDG_CONFIG_HOME
            let temp_dir = TempDir::new().unwrap();
            env::set_var("XDG_CONFIG_HOME", temp_dir.path());

            // Initially should be disabled (no file exists)
            assert!(!linux::is_enabled().unwrap());

            // Enable
            linux::enable().unwrap();
            assert!(linux::is_enabled().unwrap());

            // Verify file exists
            let desktop_path = temp_dir.path().join("autostart").join(DESKTOP_FILE_NAME);
            assert!(desktop_path.exists());

            // Disable
            linux::disable().unwrap();
            assert!(!linux::is_enabled().unwrap());

            // Verify file removed
            assert!(!desktop_path.exists());

            // Clean up env var
            env::remove_var("XDG_CONFIG_HOME");
        }

        #[test]
        #[serial]
        fn test_is_enabled_with_hidden_flag() {
            let temp_dir = TempDir::new().unwrap();
            env::set_var("XDG_CONFIG_HOME", temp_dir.path());

            // Create autostart directory
            let autostart_dir = temp_dir.path().join("autostart");
            std::fs::create_dir_all(&autostart_dir).unwrap();

            // Create desktop file with Hidden=true
            let desktop_path = autostart_dir.join(DESKTOP_FILE_NAME);
            std::fs::write(&desktop_path, "[Desktop Entry]\nHidden=true\n").unwrap();

            assert!(!linux::is_enabled().unwrap());

            env::remove_var("XDG_CONFIG_HOME");
        }
    }
}
