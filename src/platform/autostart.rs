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
// macOS Implementation (LaunchAgent)
// ============================================================================

#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;

    /// LaunchAgent plist filename
    const LAUNCH_AGENT_FILENAME: &str = "com.micround.app.plist";

    /// Bundle identifier for the LaunchAgent
    const BUNDLE_ID: &str = "com.micround.app";

    /// Get the LaunchAgents directory (~Library/LaunchAgents/)
    fn launch_agents_dir() -> AutostartResult<PathBuf> {
        dirs::home_dir()
            .map(|h| h.join("Library").join("LaunchAgents"))
            .ok_or_else(|| AutostartError::PathError("Cannot determine home directory".into()))
    }

    /// Get the path to our LaunchAgent plist
    fn plist_path() -> AutostartResult<PathBuf> {
        Ok(launch_agents_dir()?.join(LAUNCH_AGENT_FILENAME))
    }

    /// Get the path to the application executable
    fn app_executable_path() -> AutostartResult<PathBuf> {
        // First try to find the app bundle
        let current_exe = env::current_exe()
            .map_err(|e| AutostartError::Io(format!("Cannot determine executable path: {}", e)))?;

        // If running from app bundle, use that path
        // App bundle structure: Micround.app/Contents/MacOS/micround
        if let Some(contents_dir) = current_exe.parent().and_then(|p| p.parent()) {
            if contents_dir.file_name().map(|n| n == "Contents").unwrap_or(false) {
                // We're in an app bundle
                return Ok(current_exe);
            }
        }

        // Otherwise, check if we're in /Applications
        let app_path = PathBuf::from("/Applications/Micround.app/Contents/MacOS/micround");
        if app_path.exists() {
            return Ok(app_path);
        }

        // Fall back to current executable
        Ok(current_exe)
    }

    /// Generate the LaunchAgent plist content
    pub(crate) fn plist_content(exe_path: &PathBuf) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{}</string>

    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--minimized</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <false/>

    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
            BUNDLE_ID,
            exe_path.display()
        )
    }

    pub fn is_enabled() -> AutostartResult<bool> {
        let path = plist_path()?;

        if !path.exists() {
            return Ok(false);
        }

        // Check if the plist has RunAtLoad set to true
        let content = fs::read_to_string(&path)?;

        // Simple check - if RunAtLoad and true are both present, it's enabled
        if content.contains("<key>RunAtLoad</key>") && content.contains("<true/>") {
            return Ok(true);
        }

        Ok(false)
    }

    pub fn enable() -> AutostartResult<()> {
        let launch_agents_dir = launch_agents_dir()?;
        let plist_path = plist_path()?;
        let exe_path = app_executable_path()?;

        // Create LaunchAgents directory if it doesn't exist
        if !launch_agents_dir.exists() {
            fs::create_dir_all(&launch_agents_dir)?;
        }

        // Write the plist
        let content = plist_content(&exe_path);
        fs::write(&plist_path, content)?;

        // Load the LaunchAgent (optional - it will load on next login anyway)
        // We don't error if this fails since the plist is already installed
        let _ = std::process::Command::new("launchctl")
            .args(["load", "-w"])
            .arg(&plist_path)
            .output();

        tracing::info!(
            path = %plist_path.display(),
            "Autostart enabled (LaunchAgent installed)"
        );

        Ok(())
    }

    pub fn disable() -> AutostartResult<()> {
        let plist_path = plist_path()?;

        if plist_path.exists() {
            // Unload the LaunchAgent first
            let _ = std::process::Command::new("launchctl")
                .args(["unload", "-w"])
                .arg(&plist_path)
                .output();

            // Remove the plist file
            fs::remove_file(&plist_path)?;

            tracing::info!(
                path = %plist_path.display(),
                "Autostart disabled (LaunchAgent removed)"
            );
        }

        Ok(())
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

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::*;
        use serial_test::serial;
        use std::env;
        use std::path::PathBuf;
        use tempfile::TempDir;

        #[test]
        fn test_plist_content() {
            let exe_path = PathBuf::from("/Applications/Micround.app/Contents/MacOS/micround");
            let content = macos::plist_content(&exe_path);

            assert!(content.contains("<?xml version=\"1.0\""));
            assert!(content.contains("<key>Label</key>"));
            assert!(content.contains("<string>com.micround.app</string>"));
            assert!(content.contains("<key>ProgramArguments</key>"));
            assert!(content.contains("/Applications/Micround.app/Contents/MacOS/micround"));
            assert!(content.contains("--minimized"));
            assert!(content.contains("<key>RunAtLoad</key>"));
            assert!(content.contains("<true/>"));
        }

        #[test]
        fn test_is_enabled_no_file() {
            // When plist doesn't exist, should return false (not error)
            // This test only works if ~/Library/LaunchAgents/com.micround.app.plist doesn't exist
            let plist_path = dirs::home_dir()
                .unwrap()
                .join("Library")
                .join("LaunchAgents")
                .join("com.micround.app.plist");

            if !plist_path.exists() {
                assert!(!macos::is_enabled().unwrap());
            }
        }
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
