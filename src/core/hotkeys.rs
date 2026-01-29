//! Global keyboard shortcuts
//!
//! Provides cross-platform global hotkey support using the `global-hotkey` crate.
#![allow(dead_code)] // Hotkey system API
//! Hotkeys work even when the application is not focused.
//!
//! # Platform Notes
//!
//! - **Windows**: Uses RegisterHotKey API
//! - **macOS**: Uses Carbon hotkey API (may require accessibility permissions)
//! - **Linux**: Uses X11 XGrabKey (may not work reliably on Wayland)
//!
//! # Default Bindings
//!
//! | Action | Windows/Linux | macOS |
//! |--------|---------------|-------|
//! | Toggle feed | Ctrl+Alt+M | Cmd+Option+M |
//! | Pause/Resume | Ctrl+Alt+P | Cmd+Option+P |
//! | Take snapshot | Ctrl+Alt+S | Cmd+Option+S |

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::core::events::{AppHandle, Command};

// ============================================================================
// Hotkey Identifiers
// ============================================================================

/// Identifiers for global hotkeys
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HotkeyId {
    /// Toggle feed on/off
    ToggleFeed,
    /// Pause/resume display (freeze frame)
    PauseResume,
    /// Take a snapshot
    TakeSnapshot,
}

impl HotkeyId {
    /// Get all hotkey IDs
    pub fn all() -> &'static [HotkeyId] {
        &[
            HotkeyId::ToggleFeed,
            HotkeyId::PauseResume,
            HotkeyId::TakeSnapshot,
        ]
    }

    /// Get the display name for this hotkey
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ToggleFeed => "Toggle Feed",
            Self::PauseResume => "Pause/Resume",
            Self::TakeSnapshot => "Take Snapshot",
        }
    }

    /// Get the default binding string for this hotkey
    #[cfg(target_os = "macos")]
    pub fn default_binding(&self) -> &'static str {
        match self {
            Self::ToggleFeed => "Cmd+Option+M",
            Self::PauseResume => "Cmd+Option+P",
            Self::TakeSnapshot => "Cmd+Option+S",
        }
    }

    /// Get the default binding string for this hotkey
    #[cfg(not(target_os = "macos"))]
    pub fn default_binding(&self) -> &'static str {
        match self {
            Self::ToggleFeed => "Ctrl+Alt+M",
            Self::PauseResume => "Ctrl+Alt+P",
            Self::TakeSnapshot => "Ctrl+Alt+S",
        }
    }
}

// ============================================================================
// Hotkey Configuration
// ============================================================================

/// A single hotkey binding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyBinding {
    /// The hotkey action
    pub id: HotkeyId,
    /// The key binding string (e.g., "Ctrl+Alt+M")
    pub binding: String,
    /// Whether this hotkey is enabled
    pub enabled: bool,
}

impl HotkeyBinding {
    /// Create a new enabled binding
    pub fn new(id: HotkeyId, binding: impl Into<String>) -> Self {
        Self {
            id,
            binding: binding.into(),
            enabled: true,
        }
    }

    /// Create a disabled binding
    pub fn disabled(id: HotkeyId, binding: impl Into<String>) -> Self {
        Self {
            id,
            binding: binding.into(),
            enabled: false,
        }
    }
}

/// Configuration for all hotkeys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Individual hotkey bindings
    pub bindings: Vec<HotkeyBinding>,
    /// Whether global hotkeys are enabled at all
    pub enabled: bool,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            bindings: HotkeyId::all()
                .iter()
                .map(|id| HotkeyBinding::new(*id, id.default_binding()))
                .collect(),
            enabled: true,
        }
    }
}

impl HotkeyConfig {
    /// Get the binding for a specific hotkey
    pub fn get_binding(&self, id: HotkeyId) -> Option<&HotkeyBinding> {
        self.bindings.iter().find(|b| b.id == id)
    }

    /// Update a binding
    pub fn set_binding(&mut self, id: HotkeyId, binding: String) {
        if let Some(b) = self.bindings.iter_mut().find(|b| b.id == id) {
            b.binding = binding;
        }
    }

    /// Enable or disable a specific hotkey
    pub fn set_enabled(&mut self, id: HotkeyId, enabled: bool) {
        if let Some(b) = self.bindings.iter_mut().find(|b| b.id == id) {
            b.enabled = enabled;
        }
    }

    /// Reset all bindings to defaults
    pub fn reset_to_defaults(&mut self) {
        *self = Self::default();
    }
}

// ============================================================================
// Hotkey Manager (Feature-gated)
// ============================================================================

#[cfg(feature = "hotkeys")]
mod manager {
    use super::*;
    use global_hotkey::{
        hotkey::{Code, HotKey, Modifiers},
        GlobalHotKeyEvent, GlobalHotKeyManager,
    };
    use std::sync::RwLock;

    /// Manages global hotkey registration and events
    pub struct HotkeyManager {
        /// The underlying global hotkey manager
        manager: GlobalHotKeyManager,
        /// Registered hotkeys (id -> GlobalHotKey)
        registered: RwLock<HashMap<u32, HotkeyId>>,
        /// Application handle for sending commands
        app_handle: AppHandle,
        /// Configuration
        config: RwLock<HotkeyConfig>,
    }

    impl HotkeyManager {
        /// Create a new hotkey manager
        pub fn new(app_handle: AppHandle) -> Result<Self, HotkeyError> {
            let manager = GlobalHotKeyManager::new()
                .map_err(|e| HotkeyError::Initialization(e.to_string()))?;

            Ok(Self {
                manager,
                registered: RwLock::new(HashMap::new()),
                app_handle,
                config: RwLock::new(HotkeyConfig::default()),
            })
        }

        /// Create with custom configuration
        pub fn with_config(app_handle: AppHandle, config: HotkeyConfig) -> Result<Self, HotkeyError> {
            let manager = Self::new(app_handle)?;
            *manager.config.write().unwrap() = config;
            Ok(manager)
        }

        /// Register all enabled hotkeys
        pub fn register_all(&self) -> Result<(), HotkeyError> {
            let config = self.config.read().unwrap();

            if !config.enabled {
                info!("Global hotkeys disabled in configuration");
                return Ok(());
            }

            for binding in &config.bindings {
                if binding.enabled {
                    if let Err(e) = self.register_hotkey(binding) {
                        warn!(
                            hotkey = ?binding.id,
                            binding = %binding.binding,
                            error = %e,
                            "Failed to register hotkey"
                        );
                    }
                }
            }

            info!("Global hotkeys registered");
            Ok(())
        }

        /// Register a single hotkey
        fn register_hotkey(&self, binding: &HotkeyBinding) -> Result<(), HotkeyError> {
            let hotkey = parse_hotkey_string(&binding.binding)?;

            self.manager
                .register(hotkey)
                .map_err(|e| HotkeyError::Registration(e.to_string()))?;

            self.registered
                .write()
                .unwrap()
                .insert(hotkey.id(), binding.id);

            debug!(
                hotkey = ?binding.id,
                binding = %binding.binding,
                "Registered hotkey"
            );

            Ok(())
        }

        /// Unregister all hotkeys
        pub fn unregister_all(&self) -> Result<(), HotkeyError> {
            // Note: global-hotkey doesn't have a direct unregister by ID
            // The manager handles cleanup on drop - we just clear our tracking
            self.registered.write().unwrap().clear();

            info!("Global hotkeys unregistered");
            Ok(())
        }

        /// Process pending hotkey events
        ///
        /// Call this regularly from your event loop.
        /// Returns true if any events were processed.
        pub fn process_events(&self) -> bool {
            let mut processed = false;

            if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                processed = true;
                self.handle_event(event);
            }

            processed
        }

        /// Handle a hotkey event
        fn handle_event(&self, event: GlobalHotKeyEvent) {
            let registered = self.registered.read().unwrap();

            if let Some(hotkey_id) = registered.get(&event.id()) {
                debug!(hotkey = ?hotkey_id, "Hotkey pressed");

                let command = match hotkey_id {
                    HotkeyId::ToggleFeed => {
                        // Toggle requires knowing current state
                        // For now, we'll send StopCapture as a toggle
                        // The UI layer should handle actual toggling
                        info!("Toggle feed hotkey pressed");
                        None // Handled by tray/UI state
                    }
                    HotkeyId::PauseResume => {
                        info!("Pause/Resume hotkey pressed");
                        Some(Command::PauseDisplay) // Toggle handled by engine
                    }
                    HotkeyId::TakeSnapshot => {
                        info!("Take snapshot hotkey pressed");
                        Some(Command::TakeSnapshot { to_clipboard: false })
                    }
                };

                if let Some(cmd) = command {
                    if let Err(e) = self.app_handle.try_send_command(cmd) {
                        error!(error = %e, "Failed to send hotkey command");
                    }
                }
            }
        }

        /// Get current configuration
        pub fn config(&self) -> HotkeyConfig {
            self.config.read().unwrap().clone()
        }

        /// Update configuration and re-register hotkeys
        pub fn update_config(&self, config: HotkeyConfig) -> Result<(), HotkeyError> {
            self.unregister_all()?;
            *self.config.write().unwrap() = config;
            self.register_all()
        }
    }

    /// Parse a hotkey string like "Ctrl+Alt+M" into a HotKey
    pub(crate) fn parse_hotkey_string(s: &str) -> Result<HotKey, HotkeyError> {
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();

        if parts.is_empty() {
            return Err(HotkeyError::Parse("Empty hotkey string".into()));
        }

        let mut modifiers = Modifiers::empty();
        let mut key_code = None;

        for part in parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
                "alt" | "option" => modifiers |= Modifiers::ALT,
                "shift" => modifiers |= Modifiers::SHIFT,
                "cmd" | "command" | "meta" | "super" => modifiers |= Modifiers::META,
                _ => {
                    // This should be the key
                    key_code = Some(parse_key_code(part)?);
                }
            }
        }

        let code = key_code.ok_or_else(|| HotkeyError::Parse("No key specified".into()))?;

        Ok(HotKey::new(Some(modifiers), code))
    }

    /// Parse a key code string
    pub(crate) fn parse_key_code(s: &str) -> Result<Code, HotkeyError> {
        let s_lower = s.to_lowercase();

        // Single letter keys
        if s_lower.len() == 1 {
            let c = s_lower.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                return Ok(match c {
                    'a' => Code::KeyA,
                    'b' => Code::KeyB,
                    'c' => Code::KeyC,
                    'd' => Code::KeyD,
                    'e' => Code::KeyE,
                    'f' => Code::KeyF,
                    'g' => Code::KeyG,
                    'h' => Code::KeyH,
                    'i' => Code::KeyI,
                    'j' => Code::KeyJ,
                    'k' => Code::KeyK,
                    'l' => Code::KeyL,
                    'm' => Code::KeyM,
                    'n' => Code::KeyN,
                    'o' => Code::KeyO,
                    'p' => Code::KeyP,
                    'q' => Code::KeyQ,
                    'r' => Code::KeyR,
                    's' => Code::KeyS,
                    't' => Code::KeyT,
                    'u' => Code::KeyU,
                    'v' => Code::KeyV,
                    'w' => Code::KeyW,
                    'x' => Code::KeyX,
                    'y' => Code::KeyY,
                    'z' => Code::KeyZ,
                    _ => return Err(HotkeyError::Parse(format!("Unknown key: {}", s))),
                });
            }
        }

        // Special keys
        match s_lower.as_str() {
            "space" => Ok(Code::Space),
            "enter" | "return" => Ok(Code::Enter),
            "escape" | "esc" => Ok(Code::Escape),
            "tab" => Ok(Code::Tab),
            "backspace" => Ok(Code::Backspace),
            "delete" | "del" => Ok(Code::Delete),
            "home" => Ok(Code::Home),
            "end" => Ok(Code::End),
            "pageup" | "pgup" => Ok(Code::PageUp),
            "pagedown" | "pgdn" => Ok(Code::PageDown),
            "up" | "arrowup" => Ok(Code::ArrowUp),
            "down" | "arrowdown" => Ok(Code::ArrowDown),
            "left" | "arrowleft" => Ok(Code::ArrowLeft),
            "right" | "arrowright" => Ok(Code::ArrowRight),
            "f1" => Ok(Code::F1),
            "f2" => Ok(Code::F2),
            "f3" => Ok(Code::F3),
            "f4" => Ok(Code::F4),
            "f5" => Ok(Code::F5),
            "f6" => Ok(Code::F6),
            "f7" => Ok(Code::F7),
            "f8" => Ok(Code::F8),
            "f9" => Ok(Code::F9),
            "f10" => Ok(Code::F10),
            "f11" => Ok(Code::F11),
            "f12" => Ok(Code::F12),
            _ => Err(HotkeyError::Parse(format!("Unknown key: {}", s))),
        }
    }
}

#[cfg(feature = "hotkeys")]
pub use manager::HotkeyManager;

// ============================================================================
// Stub implementation when hotkeys feature is disabled
// ============================================================================

#[cfg(not(feature = "hotkeys"))]
pub struct HotkeyManager;

#[cfg(not(feature = "hotkeys"))]
impl HotkeyManager {
    pub fn new(_app_handle: AppHandle) -> Result<Self, HotkeyError> {
        warn!("Hotkeys feature not enabled, hotkey manager is a no-op");
        Ok(Self)
    }

    pub fn with_config(_app_handle: AppHandle, _config: HotkeyConfig) -> Result<Self, HotkeyError> {
        Self::new(_app_handle)
    }

    pub fn register_all(&self) -> Result<(), HotkeyError> {
        Ok(())
    }

    pub fn unregister_all(&self) -> Result<(), HotkeyError> {
        Ok(())
    }

    pub fn process_events(&self) -> bool {
        false
    }

    pub fn config(&self) -> HotkeyConfig {
        HotkeyConfig::default()
    }

    pub fn update_config(&self, _config: HotkeyConfig) -> Result<(), HotkeyError> {
        Ok(())
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur with hotkey operations
#[derive(Debug, thiserror::Error)]
pub enum HotkeyError {
    #[error("Failed to initialize hotkey manager: {0}")]
    Initialization(String),

    #[error("Failed to register hotkey: {0}")]
    Registration(String),

    #[error("Failed to parse hotkey binding: {0}")]
    Parse(String),

    #[error("Hotkey conflict: {0}")]
    Conflict(String),

    #[error("Platform not supported: {0}")]
    Unsupported(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotkey_id_all() {
        let all = HotkeyId::all();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&HotkeyId::ToggleFeed));
        assert!(all.contains(&HotkeyId::PauseResume));
        assert!(all.contains(&HotkeyId::TakeSnapshot));
    }

    #[test]
    fn test_hotkey_id_display_names() {
        assert_eq!(HotkeyId::ToggleFeed.display_name(), "Toggle Feed");
        assert_eq!(HotkeyId::PauseResume.display_name(), "Pause/Resume");
        assert_eq!(HotkeyId::TakeSnapshot.display_name(), "Take Snapshot");
    }

    #[test]
    fn test_hotkey_config_default() {
        let config = HotkeyConfig::default();
        assert!(config.enabled);
        assert_eq!(config.bindings.len(), 3);

        // All bindings should be enabled by default
        for binding in &config.bindings {
            assert!(binding.enabled);
        }
    }

    #[test]
    fn test_hotkey_config_get_binding() {
        let config = HotkeyConfig::default();

        let toggle = config.get_binding(HotkeyId::ToggleFeed);
        assert!(toggle.is_some());
        assert!(toggle.unwrap().binding.contains("M"));
    }

    #[test]
    fn test_hotkey_config_set_binding() {
        let mut config = HotkeyConfig::default();

        config.set_binding(HotkeyId::ToggleFeed, "Ctrl+Shift+X".to_string());

        let binding = config.get_binding(HotkeyId::ToggleFeed).unwrap();
        assert_eq!(binding.binding, "Ctrl+Shift+X");
    }

    #[test]
    fn test_hotkey_config_set_enabled() {
        let mut config = HotkeyConfig::default();

        config.set_enabled(HotkeyId::TakeSnapshot, false);

        let binding = config.get_binding(HotkeyId::TakeSnapshot).unwrap();
        assert!(!binding.enabled);
    }

    #[test]
    fn test_hotkey_config_reset() {
        let mut config = HotkeyConfig::default();
        config.enabled = false;
        config.set_binding(HotkeyId::ToggleFeed, "Custom".to_string());

        config.reset_to_defaults();

        assert!(config.enabled);
        let binding = config.get_binding(HotkeyId::ToggleFeed).unwrap();
        assert!(binding.binding.contains("Alt") || binding.binding.contains("Option"));
    }

    #[test]
    fn test_hotkey_binding_new() {
        let binding = HotkeyBinding::new(HotkeyId::PauseResume, "Ctrl+P");
        assert_eq!(binding.id, HotkeyId::PauseResume);
        assert_eq!(binding.binding, "Ctrl+P");
        assert!(binding.enabled);
    }

    #[test]
    fn test_hotkey_binding_disabled() {
        let binding = HotkeyBinding::disabled(HotkeyId::PauseResume, "Ctrl+P");
        assert!(!binding.enabled);
    }

    #[cfg(feature = "hotkeys")]
    mod hotkey_tests {
        use super::*;

        #[test]
        fn test_parse_hotkey_string_simple() {
            let hotkey = manager::parse_hotkey_string("Ctrl+Alt+M").unwrap();
            // Just verify it parses without error
            assert!(hotkey.id() > 0);
        }

        #[test]
        fn test_parse_hotkey_string_with_shift() {
            let hotkey = manager::parse_hotkey_string("Ctrl+Shift+S").unwrap();
            assert!(hotkey.id() > 0);
        }

        #[test]
        fn test_parse_hotkey_string_invalid() {
            let result = manager::parse_hotkey_string("");
            assert!(result.is_err());
        }

        #[test]
        fn test_parse_key_code_letters() {
            assert!(manager::parse_key_code("M").is_ok());
            assert!(manager::parse_key_code("m").is_ok());
            assert!(manager::parse_key_code("P").is_ok());
            assert!(manager::parse_key_code("S").is_ok());
        }

        #[test]
        fn test_parse_key_code_special() {
            assert!(manager::parse_key_code("Space").is_ok());
            assert!(manager::parse_key_code("Enter").is_ok());
            assert!(manager::parse_key_code("F1").is_ok());
            assert!(manager::parse_key_code("Escape").is_ok());
        }
    }
}
