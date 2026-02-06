//! Wallpaper backup and restore functionality
//!
//! This module provides platform-specific functions to capture the user's
//! current wallpaper settings and restore them later. This is critical for
//! maintaining user trust - we must never "lose" their wallpaper.
//!
//! # Platform Support
//!
//! - **Linux**: Uses gsettings for GNOME, with fallbacks for other DEs
//! - **Windows**: Uses registry and SystemParametersInfo
//! - **macOS**: Uses NSWorkspace desktop image APIs
//!
//! # Usage
//!
//! ```ignore
//! // Capture current wallpaper before starting feed
//! let backup = capture_wallpaper()?;
//! config.internal.original_wallpaper_path = backup.path.clone();
//! save_config(&config)?;
//!
//! // ... run live feed ...
//!
//! // Restore on exit
//! restore_wallpaper(&backup)?;
//! ```

use crate::core::PlatformError;
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::process::Command;

/// Information about a wallpaper configuration
#[derive(Debug, Clone, Default)]
pub struct WallpaperInfo {
    /// Path to the wallpaper image file (None if solid color)
    pub path: Option<String>,
    /// Wallpaper style/mode (platform-specific string)
    pub style: Option<String>,
    /// Background color for solid color or letterboxing (hex RGB)
    pub color: Option<String>,
    /// Desktop environment or shell (Linux only)
    pub desktop_env: Option<String>,
    /// Per-monitor settings (display ID -> path)
    pub per_monitor: Vec<(String, String)>,
}

impl WallpaperInfo {
    /// Create a new WallpaperInfo with just a path
    pub fn with_path(path: String) -> Self {
        Self {
            path: Some(path),
            ..Default::default()
        }
    }

    /// Check if this represents a valid wallpaper to restore
    pub fn is_valid(&self) -> bool {
        // Valid if we have a path (and file exists) or a solid color
        if let Some(ref path) = self.path {
            PathBuf::from(path).exists()
        } else {
            self.color.is_some()
        }
    }
}

/// Capture the current wallpaper settings
///
/// This function queries the system for the current wallpaper configuration.
/// It's platform-specific and may not capture all information on all systems.
///
/// # Returns
///
/// * `Ok(WallpaperInfo)` - Current wallpaper settings
/// * `Err(PlatformError)` - If wallpaper info cannot be retrieved
///
/// # Platform Notes
///
/// - **Linux/GNOME**: Captures gsettings values
/// - **Linux/KDE**: Not yet supported (returns error with suggestion)
/// - **Linux/Other**: Best effort, may return incomplete info
pub fn capture_wallpaper() -> Result<WallpaperInfo, PlatformError> {
    #[cfg(target_os = "linux")]
    {
        capture_wallpaper_linux()
    }
    #[cfg(target_os = "windows")]
    {
        capture_wallpaper_windows()
    }
    #[cfg(target_os = "macos")]
    {
        capture_wallpaper_macos()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(PlatformError::Unsupported(
            "Wallpaper backup not supported on this platform".into(),
        ))
    }
}

/// Restore wallpaper from saved settings
///
/// This function restores the wallpaper to the state captured by `capture_wallpaper`.
///
/// # Arguments
///
/// * `info` - Wallpaper settings to restore
///
/// # Returns
///
/// * `Ok(())` - Wallpaper restored successfully
/// * `Err(PlatformError)` - If restoration fails
///
/// # Edge Cases
///
/// - If the original file was deleted, returns an error
/// - If the settings are incomplete, does best-effort restoration
pub fn restore_wallpaper(info: &WallpaperInfo) -> Result<(), PlatformError> {
    // Validate that we can restore
    if !info.is_valid() {
        return Err(PlatformError::InvalidState(
            "Cannot restore wallpaper: original file may have been deleted".into(),
        ));
    }

    #[cfg(target_os = "linux")]
    {
        restore_wallpaper_linux(info)
    }
    #[cfg(target_os = "windows")]
    {
        restore_wallpaper_windows(info)
    }
    #[cfg(target_os = "macos")]
    {
        restore_wallpaper_macos(info)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(PlatformError::Unsupported(
            "Wallpaper restore not supported on this platform".into(),
        ))
    }
}

/// Restore wallpaper from config path (convenience function)
///
/// This is a simpler version that just restores from a path string,
/// useful when loading from config file.
pub fn restore_wallpaper_from_path(path: &str) -> Result<(), PlatformError> {
    let info = WallpaperInfo::with_path(path.to_string());
    restore_wallpaper(&info)
}

// ============================================================================
// Linux Implementation
// ============================================================================

#[cfg(target_os = "linux")]
fn detect_desktop_environment() -> Option<String> {
    // Check XDG_CURRENT_DESKTOP first
    if let Ok(de) = std::env::var("XDG_CURRENT_DESKTOP") {
        return Some(de.to_uppercase());
    }
    // Check DESKTOP_SESSION as fallback
    if let Ok(de) = std::env::var("DESKTOP_SESSION") {
        return Some(de.to_uppercase());
    }
    None
}

#[cfg(target_os = "linux")]
fn capture_wallpaper_linux() -> Result<WallpaperInfo, PlatformError> {
    let de = detect_desktop_environment();
    let de_str = de.as_deref().unwrap_or("UNKNOWN");

    tracing::debug!(desktop_env = %de_str, "Detecting wallpaper on Linux");

    // GNOME and derivatives (Ubuntu, Pop!_OS, etc.)
    if de_str.contains("GNOME") || de_str.contains("UNITY") || de_str.contains("UBUNTU") {
        return capture_wallpaper_gnome();
    }

    // KDE Plasma
    if de_str.contains("KDE") || de_str.contains("PLASMA") {
        return capture_wallpaper_kde();
    }

    // XFCE
    if de_str.contains("XFCE") {
        return capture_wallpaper_xfce();
    }

    // MATE
    if de_str.contains("MATE") {
        return capture_wallpaper_mate();
    }

    // Cinnamon
    if de_str.contains("CINNAMON") || de_str.contains("X-CINNAMON") {
        return capture_wallpaper_cinnamon();
    }

    // Unknown DE - try GNOME as fallback (many DEs use gsettings)
    tracing::warn!(
        desktop_env = %de_str,
        "Unknown desktop environment, attempting gsettings fallback"
    );
    capture_wallpaper_gnome().or_else(|_| {
        Err(PlatformError::Unsupported(format!(
            "Wallpaper capture not supported for desktop environment: {}. \
             Supported: GNOME, KDE, XFCE, MATE, Cinnamon",
            de_str
        )))
    })
}

#[cfg(target_os = "linux")]
fn capture_wallpaper_gnome() -> Result<WallpaperInfo, PlatformError> {
    // GNOME uses gsettings
    let output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "picture-uri"])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("gsettings: {}", e)))?;

    if !output.status.success() {
        return Err(PlatformError::CommandFailed(
            "gsettings failed to get wallpaper URI".into(),
        ));
    }

    let uri = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // gsettings returns 'file:///path/to/file' with quotes
    let path = uri
        .trim_matches('\'')
        .trim_matches('"')
        .strip_prefix("file://")
        .unwrap_or(&uri)
        .to_string();

    // Get picture options (style)
    let style_output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "picture-options"])
        .output()
        .ok();

    let style = style_output.filter(|o| o.status.success()).map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .trim_matches('\'')
            .to_string()
    });

    // Get primary color
    let color_output = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.background", "primary-color"])
        .output()
        .ok();

    let color = color_output.filter(|o| o.status.success()).map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .trim_matches('\'')
            .to_string()
    });

    Ok(WallpaperInfo {
        path: if path.is_empty() { None } else { Some(path) },
        style,
        color,
        desktop_env: Some("GNOME".into()),
        per_monitor: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
fn capture_wallpaper_kde() -> Result<WallpaperInfo, PlatformError> {
    // KDE Plasma stores wallpaper config in plasma config files
    // This is more complex and would require reading config files
    Err(PlatformError::Unsupported(
        "KDE Plasma wallpaper capture not yet implemented. \
         Please manually note your wallpaper path before using Micround."
            .into(),
    ))
}

#[cfg(target_os = "linux")]
fn capture_wallpaper_xfce() -> Result<WallpaperInfo, PlatformError> {
    // XFCE uses xfconf
    let output = Command::new("xfconf-query")
        .args([
            "-c",
            "xfce4-desktop",
            "-p",
            "/backdrop/screen0/monitor0/workspace0/last-image",
        ])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("xfconf-query: {}", e)))?;

    if !output.status.success() {
        return Err(PlatformError::CommandFailed(
            "xfconf-query failed to get wallpaper".into(),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    Ok(WallpaperInfo {
        path: Some(path),
        style: None,
        color: None,
        desktop_env: Some("XFCE".into()),
        per_monitor: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
fn capture_wallpaper_mate() -> Result<WallpaperInfo, PlatformError> {
    // MATE uses gsettings with different schema
    let output = Command::new("gsettings")
        .args(["get", "org.mate.background", "picture-filename"])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("gsettings: {}", e)))?;

    if !output.status.success() {
        return Err(PlatformError::CommandFailed(
            "gsettings failed to get MATE wallpaper".into(),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_matches('\'')
        .to_string();

    Ok(WallpaperInfo {
        path: Some(path),
        style: None,
        color: None,
        desktop_env: Some("MATE".into()),
        per_monitor: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
fn capture_wallpaper_cinnamon() -> Result<WallpaperInfo, PlatformError> {
    // Cinnamon uses gsettings with cinnamon schema
    let output = Command::new("gsettings")
        .args(["get", "org.cinnamon.desktop.background", "picture-uri"])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("gsettings: {}", e)))?;

    if !output.status.success() {
        return Err(PlatformError::CommandFailed(
            "gsettings failed to get Cinnamon wallpaper".into(),
        ));
    }

    let uri = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = uri
        .trim_matches('\'')
        .strip_prefix("file://")
        .unwrap_or(&uri)
        .to_string();

    Ok(WallpaperInfo {
        path: Some(path),
        style: None,
        color: None,
        desktop_env: Some("CINNAMON".into()),
        per_monitor: Vec::new(),
    })
}

#[cfg(target_os = "linux")]
fn restore_wallpaper_linux(info: &WallpaperInfo) -> Result<(), PlatformError> {
    let de = info
        .desktop_env
        .clone()
        .or_else(|| detect_desktop_environment())
        .unwrap_or_else(|| "GNOME".into());
    let de_upper = de.to_uppercase();

    tracing::info!(
        desktop_env = %de_upper,
        path = ?info.path,
        "Restoring wallpaper on Linux"
    );

    if de_upper.contains("GNOME") || de_upper.contains("UNITY") || de_upper.contains("UBUNTU") {
        return restore_wallpaper_gnome(info);
    }

    if de_upper.contains("XFCE") {
        return restore_wallpaper_xfce(info);
    }

    if de_upper.contains("MATE") {
        return restore_wallpaper_mate(info);
    }

    if de_upper.contains("CINNAMON") || de_upper.contains("X-CINNAMON") {
        return restore_wallpaper_cinnamon(info);
    }

    // Fallback: try common tools
    if let Some(ref path) = info.path {
        // Try feh (common lightweight tool)
        if let Ok(status) = Command::new("feh").args(["--bg-fill", path]).status() {
            if status.success() {
                tracing::info!("Restored wallpaper using feh");
                return Ok(());
            }
        }

        // Try nitrogen
        if let Ok(status) = Command::new("nitrogen")
            .args(["--set-zoom-fill", path])
            .status()
        {
            if status.success() {
                tracing::info!("Restored wallpaper using nitrogen");
                return Ok(());
            }
        }
    }

    Err(PlatformError::Unsupported(format!(
        "Cannot restore wallpaper for desktop environment: {}",
        de
    )))
}

#[cfg(target_os = "linux")]
fn restore_wallpaper_gnome(info: &WallpaperInfo) -> Result<(), PlatformError> {
    let path = info
        .path
        .as_ref()
        .ok_or_else(|| PlatformError::InvalidState("No wallpaper path to restore".into()))?;

    let uri = format!("file://{}", path);

    // Set picture URI
    let status = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.background", "picture-uri", &uri])
        .status()
        .map_err(|e| PlatformError::CommandFailed(format!("gsettings: {}", e)))?;

    if !status.success() {
        return Err(PlatformError::CommandFailed(
            "Failed to set GNOME wallpaper".into(),
        ));
    }

    // Also set picture-uri-dark for dark mode support
    let _ = Command::new("gsettings")
        .args([
            "set",
            "org.gnome.desktop.background",
            "picture-uri-dark",
            &uri,
        ])
        .status();

    // Restore picture options if we have them
    if let Some(ref style) = info.style {
        let _ = Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.background",
                "picture-options",
                style,
            ])
            .status();
    }

    // Restore primary color if we have it
    if let Some(ref color) = info.color {
        let _ = Command::new("gsettings")
            .args([
                "set",
                "org.gnome.desktop.background",
                "primary-color",
                color,
            ])
            .status();
    }

    tracing::info!(path = %path, "Restored GNOME wallpaper");
    Ok(())
}

#[cfg(target_os = "linux")]
fn restore_wallpaper_xfce(info: &WallpaperInfo) -> Result<(), PlatformError> {
    let path = info
        .path
        .as_ref()
        .ok_or_else(|| PlatformError::InvalidState("No wallpaper path to restore".into()))?;

    let status = Command::new("xfconf-query")
        .args([
            "-c",
            "xfce4-desktop",
            "-p",
            "/backdrop/screen0/monitor0/workspace0/last-image",
            "-s",
            path,
        ])
        .status()
        .map_err(|e| PlatformError::CommandFailed(format!("xfconf-query: {}", e)))?;

    if !status.success() {
        return Err(PlatformError::CommandFailed(
            "Failed to set XFCE wallpaper".into(),
        ));
    }

    tracing::info!(path = %path, "Restored XFCE wallpaper");
    Ok(())
}

#[cfg(target_os = "linux")]
fn restore_wallpaper_mate(info: &WallpaperInfo) -> Result<(), PlatformError> {
    let path = info
        .path
        .as_ref()
        .ok_or_else(|| PlatformError::InvalidState("No wallpaper path to restore".into()))?;

    let status = Command::new("gsettings")
        .args(["set", "org.mate.background", "picture-filename", path])
        .status()
        .map_err(|e| PlatformError::CommandFailed(format!("gsettings: {}", e)))?;

    if !status.success() {
        return Err(PlatformError::CommandFailed(
            "Failed to set MATE wallpaper".into(),
        ));
    }

    tracing::info!(path = %path, "Restored MATE wallpaper");
    Ok(())
}

#[cfg(target_os = "linux")]
fn restore_wallpaper_cinnamon(info: &WallpaperInfo) -> Result<(), PlatformError> {
    let path = info
        .path
        .as_ref()
        .ok_or_else(|| PlatformError::InvalidState("No wallpaper path to restore".into()))?;

    let uri = format!("file://{}", path);

    let status = Command::new("gsettings")
        .args([
            "set",
            "org.cinnamon.desktop.background",
            "picture-uri",
            &uri,
        ])
        .status()
        .map_err(|e| PlatformError::CommandFailed(format!("gsettings: {}", e)))?;

    if !status.success() {
        return Err(PlatformError::CommandFailed(
            "Failed to set Cinnamon wallpaper".into(),
        ));
    }

    tracing::info!(path = %path, "Restored Cinnamon wallpaper");
    Ok(())
}

// ============================================================================
// Windows Implementation (Stub)
// ============================================================================

#[cfg(target_os = "windows")]
fn capture_wallpaper_windows() -> Result<WallpaperInfo, PlatformError> {
    // TODO: Implement Windows wallpaper capture (bd-3ht)
    // Use registry: HKCU\Control Panel\Desktop\Wallpaper
    Err(PlatformError::Unsupported(
        "Windows wallpaper capture not yet implemented".into(),
    ))
}

#[cfg(target_os = "windows")]
fn restore_wallpaper_windows(_info: &WallpaperInfo) -> Result<(), PlatformError> {
    // TODO: Implement Windows wallpaper restore (bd-3ht)
    // Use SystemParametersInfo with SPI_SETDESKWALLPAPER
    Err(PlatformError::Unsupported(
        "Windows wallpaper restore not yet implemented".into(),
    ))
}

// ============================================================================
// macOS Implementation (AppleScript via osascript)
// ============================================================================

#[cfg(target_os = "macos")]
fn capture_wallpaper_macos() -> Result<WallpaperInfo, PlatformError> {
    use std::process::Command;

    // Use AppleScript to get the current desktop picture
    // This works across macOS versions
    let script = r#"
        tell application "System Events"
            tell every desktop
                get picture
            end tell
        end tell
    "#;

    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PlatformError::CommandFailed(format!(
            "osascript failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let path_str = stdout.trim();

    // Parse the output - may be a single path or comma-separated for multi-monitor
    // Format is typically: "file:///path/to/image.jpg" or "/path/to/image.jpg"
    let paths: Vec<&str> = path_str.split(", ").collect();

    if paths.is_empty() || paths[0].is_empty() {
        // No wallpaper set or solid color
        return Ok(WallpaperInfo {
            path: None,
            color: Some("#000000".into()), // Default to black
            desktop_env: Some("macOS".into()),
            ..Default::default()
        });
    }

    // Clean up the path (remove file:// prefix if present)
    let primary_path = paths[0]
        .strip_prefix("file://")
        .unwrap_or(paths[0])
        .to_string();

    // URL decode the path (spaces become %20, etc.)
    let decoded_path = urlencoding_decode(&primary_path);

    // Build per-monitor list if multiple monitors
    let per_monitor: Vec<(String, String)> = paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let path = p.strip_prefix("file://").unwrap_or(p);
            let decoded = urlencoding_decode(path);
            (format!("desktop_{}", i + 1), decoded)
        })
        .collect();

    tracing::debug!(
        path = %decoded_path,
        monitors = paths.len(),
        "Captured macOS wallpaper"
    );

    Ok(WallpaperInfo {
        path: Some(decoded_path),
        style: None,
        color: None,
        desktop_env: Some("macOS".into()),
        per_monitor,
    })
}

#[cfg(target_os = "macos")]
fn restore_wallpaper_macos(info: &WallpaperInfo) -> Result<(), PlatformError> {
    use std::process::Command;

    let path = info.path.as_ref().ok_or_else(|| {
        PlatformError::CommandFailed("No wallpaper path to restore".into())
    })?;

    // Verify the file exists
    if !std::path::Path::new(path).exists() {
        return Err(PlatformError::CommandFailed(format!(
            "Wallpaper file not found: {}",
            path
        )));
    }

    // Use AppleScript to set the desktop picture
    // POSIX file path is used for compatibility
    let script = format!(
        r#"
        tell application "System Events"
            tell every desktop
                set picture to POSIX file "{}"
            end tell
        end tell
        "#,
        path.replace('\\', "\\\\").replace('"', "\\\"")
    );

    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("Failed to run osascript: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PlatformError::CommandFailed(format!(
            "Failed to restore wallpaper: {}",
            stderr.trim()
        )));
    }

    tracing::info!(path = %path, "Restored macOS wallpaper");
    Ok(())
}

/// Simple URL decoding for file paths (handles %20 -> space, etc.)
#[cfg(target_os = "macos")]
fn urlencoding_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            // Try to read two hex digits
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            // Couldn't decode, keep the % and continue
            result.push('%');
            result.push_str(&hex);
        } else {
            result.push(c);
        }
    }

    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallpaper_info_default() {
        let info = WallpaperInfo::default();
        assert!(info.path.is_none());
        assert!(info.style.is_none());
        assert!(info.color.is_none());
    }

    #[test]
    fn test_wallpaper_info_with_path() {
        let info = WallpaperInfo::with_path("/path/to/wallpaper.jpg".into());
        assert_eq!(info.path, Some("/path/to/wallpaper.jpg".into()));
    }

    #[test]
    fn test_wallpaper_info_is_valid_no_file() {
        let info = WallpaperInfo::with_path("/nonexistent/path.jpg".into());
        assert!(!info.is_valid());
    }

    #[test]
    fn test_wallpaper_info_is_valid_with_color() {
        let info = WallpaperInfo {
            path: None,
            color: Some("#000000".into()),
            ..Default::default()
        };
        assert!(info.is_valid());
    }

    #[test]
    fn test_wallpaper_info_empty_is_invalid() {
        let info = WallpaperInfo::default();
        assert!(!info.is_valid());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_desktop_environment() {
        // This test will return whatever DE is running
        // Just verify it doesn't panic
        let de = detect_desktop_environment();
        println!("Detected desktop environment: {:?}", de);
    }

    #[test]
    fn test_restore_invalid_wallpaper_fails() {
        let info = WallpaperInfo::default();
        let result = restore_wallpaper(&info);
        assert!(result.is_err());
    }

    #[cfg(target_os = "macos")]
    mod macos_tests {
        use super::*;

        #[test]
        fn test_urlencoding_decode_simple() {
            assert_eq!(urlencoding_decode("hello"), "hello");
            assert_eq!(urlencoding_decode("hello%20world"), "hello world");
            assert_eq!(urlencoding_decode("path%2Fto%2Ffile"), "path/to/file");
        }

        #[test]
        fn test_urlencoding_decode_edge_cases() {
            assert_eq!(urlencoding_decode(""), "");
            assert_eq!(urlencoding_decode("%"), "%");
            assert_eq!(urlencoding_decode("%2"), "%2");
            // %%20 is not valid URL encoding - first % is followed by invalid hex "%2"
            // so the entire sequence is kept literally
            assert_eq!(urlencoding_decode("%%20"), "%%20");
            // %25 is the proper URL encoding of %, %20 is space
            assert_eq!(urlencoding_decode("%25%20"), "% ");
        }

        #[test]
        fn test_capture_wallpaper_macos_runs() {
            // Just test that it doesn't panic
            // The actual result depends on system state
            let result = capture_wallpaper_macos();
            // It might fail if osascript isn't available, that's OK
            println!("Capture result: {:?}", result);
        }

        #[test]
        fn test_restore_nonexistent_file_fails() {
            let info = WallpaperInfo::with_path("/nonexistent/wallpaper.jpg".into());
            let result = restore_wallpaper_macos(&info);
            assert!(result.is_err());
        }
    }
}
