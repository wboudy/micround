//! System tray integration
//!
//! Provides cross-platform system tray icon and menu functionality.
//! Works on Windows, macOS, and Linux.
//!
//! # Platform Notes
//!
//! - **Windows/Linux**: Event loop must run on the thread where the tray is created
//! - **macOS**: Event loop must run on the main thread, tray created after loop starts
//! - **Linux**: Requires GTK3, libxdo, and libappindicator/libayatana-appindicator
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐      ┌──────────────────┐      ┌───────────────┐
//! │   TrayController│ ──▶  │  Command Channel │ ──▶  │  App Engine   │
//! │   (menu clicks) │      │  (async)         │      │  (processing) │
//! └─────────────────┘      └──────────────────┘      └───────────────┘
//!         ▲
//!         │
//! ┌───────┴─────────┐
//! │   TrayState     │ (updated by App Engine via events)
//! └─────────────────┘
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use muda::{accelerator::Accelerator, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tracing::{debug, error, info, warn};
use tray_icon::{menu::MenuId, TrayIcon as TrayIconInner, TrayIconBuilder, TrayIconEvent};

use crate::core::events::{AppHandle, AppState, Command};

// ============================================================================
// Menu Item IDs
// ============================================================================

/// Identifiers for menu items, used to route click events
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayMenuId {
    /// Start/Resume the feed
    StartFeed,
    /// Stop the feed
    StopFeed,
    /// Pause display (freeze frame)
    Pause,
    /// Resume from pause
    Resume,
    /// Take a snapshot
    Snapshot,
    /// Open settings window
    Settings,
    /// Quit the application
    Quit,
}

impl TrayMenuId {
    /// Convert to string ID for muda
    fn as_str(&self) -> &'static str {
        match self {
            Self::StartFeed => "start_feed",
            Self::StopFeed => "stop_feed",
            Self::Pause => "pause",
            Self::Resume => "resume",
            Self::Snapshot => "snapshot",
            Self::Settings => "settings",
            Self::Quit => "quit",
        }
    }

    /// Parse from string ID
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "start_feed" => Some(Self::StartFeed),
            "stop_feed" => Some(Self::StopFeed),
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            "snapshot" => Some(Self::Snapshot),
            "settings" => Some(Self::Settings),
            "quit" => Some(Self::Quit),
            _ => None,
        }
    }
}

// ============================================================================
// Tray State
// ============================================================================

/// Current state displayed in the tray
#[derive(Debug, Clone)]
pub struct TrayState {
    /// Current application state
    pub app_state: AppState,
    /// Current resolution (if capturing)
    pub resolution: Option<(u32, u32)>,
    /// Current FPS (if capturing)
    pub fps: Option<f32>,
    /// Camera name (if connected)
    pub camera_name: Option<String>,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            app_state: AppState::Idle,
            resolution: None,
            fps: None,
            camera_name: None,
        }
    }
}

impl TrayState {
    /// Create state for running capture
    pub fn running(resolution: (u32, u32), fps: f32, camera_name: impl Into<String>) -> Self {
        Self {
            app_state: AppState::Running,
            resolution: Some(resolution),
            fps: Some(fps),
            camera_name: Some(camera_name.into()),
        }
    }

    /// Create state for paused display
    pub fn paused(resolution: (u32, u32), fps: f32, camera_name: impl Into<String>) -> Self {
        Self {
            app_state: AppState::Paused,
            resolution: Some(resolution),
            fps: Some(fps),
            camera_name: Some(camera_name.into()),
        }
    }

    /// Generate tooltip text based on current state
    pub fn tooltip(&self) -> String {
        match self.app_state {
            AppState::Idle => "Micround - Idle".to_string(),
            AppState::Starting => "Micround - Starting...".to_string(),
            AppState::Running => {
                if let (Some((w, h)), Some(fps)) = (self.resolution, self.fps) {
                    format!("Micround - Live {}x{} @ {:.0}fps", w, h, fps)
                } else {
                    "Micround - Running".to_string()
                }
            }
            AppState::Paused => {
                if let (Some((w, h)), Some(fps)) = (self.resolution, self.fps) {
                    format!("Micround - Paused ({}x{} @ {:.0}fps)", w, h, fps)
                } else {
                    "Micround - Paused".to_string()
                }
            }
            AppState::Reconnecting => "Micround - Reconnecting...".to_string(),
            AppState::Error => "Micround - Error".to_string(),
            AppState::ShuttingDown => "Micround - Shutting down...".to_string(),
        }
    }

    /// Generate status text for menu header
    pub fn status_text(&self) -> String {
        match self.app_state {
            AppState::Idle => "Status: Stopped".to_string(),
            AppState::Starting => "Status: Starting...".to_string(),
            AppState::Running => {
                if let (Some((w, h)), Some(fps)) = (self.resolution, self.fps) {
                    format!("Status: Live {}x{} @ {:.0}fps", w, h, fps)
                } else {
                    "Status: Running".to_string()
                }
            }
            AppState::Paused => "Status: Paused".to_string(),
            AppState::Reconnecting => "Status: Reconnecting...".to_string(),
            AppState::Error => "Status: Error".to_string(),
            AppState::ShuttingDown => "Status: Shutting down...".to_string(),
        }
    }

    /// Check if feed can be started
    pub fn can_start(&self) -> bool {
        matches!(self.app_state, AppState::Idle | AppState::Error)
    }

    /// Check if feed can be stopped
    pub fn can_stop(&self) -> bool {
        matches!(
            self.app_state,
            AppState::Running | AppState::Paused | AppState::Reconnecting
        )
    }

    /// Check if display can be paused
    pub fn can_pause(&self) -> bool {
        self.app_state == AppState::Running
    }

    /// Check if display can be resumed
    pub fn can_resume(&self) -> bool {
        self.app_state == AppState::Paused
    }

    /// Check if snapshot can be taken
    pub fn can_snapshot(&self) -> bool {
        matches!(self.app_state, AppState::Running | AppState::Paused)
    }
}

// ============================================================================
// Tray Controller
// ============================================================================

/// Manages the system tray icon and menu
pub struct TrayController {
    /// The underlying tray icon
    #[allow(dead_code)]
    tray_icon: TrayIconInner,
    /// Current tray state
    state: TrayState,
    /// Application handle for sending commands
    app_handle: AppHandle,
    /// Flag to signal shutdown
    shutdown: Arc<AtomicBool>,
    /// Menu items that need dynamic updates
    menu_items: TrayMenuItems,
}

/// References to menu items for dynamic updates
struct TrayMenuItems {
    status_item: MenuItem,
    start_item: MenuItem,
    stop_item: MenuItem,
    pause_item: MenuItem,
    resume_item: MenuItem,
    snapshot_item: MenuItem,
}

impl TrayController {
    /// Create a new tray controller
    ///
    /// # Arguments
    /// * `app_handle` - Handle for sending commands to the application
    /// * `initial_state` - Initial tray state
    ///
    /// # Returns
    /// * `Ok(TrayController)` - The created controller
    /// * `Err(TrayError)` - If tray creation failed
    pub fn new(app_handle: AppHandle, initial_state: TrayState) -> Result<Self, TrayError> {
        // Build the menu
        let (menu, menu_items) = build_tray_menu(&initial_state)?;

        // Load the icon
        let icon = load_tray_icon()?;

        // Build the tray icon
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(initial_state.tooltip())
            .with_icon(icon)
            .build()
            .map_err(|e| TrayError::Creation(e.to_string()))?;

        info!("System tray icon created");

        Ok(Self {
            tray_icon,
            state: initial_state,
            app_handle,
            shutdown: Arc::new(AtomicBool::new(false)),
            menu_items,
        })
    }

    /// Update the tray state and refresh the UI
    pub fn update_state(&mut self, new_state: TrayState) {
        let old_icon_state = IconState::from(self.state.app_state);
        let new_icon_state = IconState::from(new_state.app_state);

        self.state = new_state;
        self.refresh_menu();

        // Update icon if state category changed
        if old_icon_state != new_icon_state {
            if let Err(e) = self.update_icon(new_icon_state) {
                warn!(error = %e, "Failed to update tray icon");
            }
        }

        // Update tooltip
        self.tray_icon
            .set_tooltip(Some(&self.state.tooltip()))
            .unwrap_or_else(|e| warn!(error = %e, "Failed to update tooltip"));
    }

    /// Update the tray icon for a given state
    fn update_icon(&mut self, icon_state: IconState) -> Result<(), TrayError> {
        let icon = load_icon_for_state(icon_state)?;
        self.tray_icon
            .set_icon(Some(icon))
            .map_err(|e| TrayError::Icon(e.to_string()))?;
        debug!(?icon_state, "Tray icon updated");
        Ok(())
    }

    /// Refresh menu items based on current state
    fn refresh_menu(&self) {
        // Update status text
        self.menu_items
            .status_item
            .set_text(self.state.status_text());

        // Update enabled state of menu items
        self.menu_items
            .start_item
            .set_enabled(self.state.can_start());
        self.menu_items.stop_item.set_enabled(self.state.can_stop());
        self.menu_items
            .pause_item
            .set_enabled(self.state.can_pause());
        self.menu_items
            .resume_item
            .set_enabled(self.state.can_resume());
        self.menu_items
            .snapshot_item
            .set_enabled(self.state.can_snapshot());

        // Update visibility based on state
        // Show Start when stopped, Stop when running
        let _is_running = matches!(
            self.state.app_state,
            AppState::Running | AppState::Paused | AppState::Reconnecting | AppState::Starting
        );
        // Note: muda doesn't have hide/show, so we use enabled state

        debug!(state = ?self.state.app_state, "Tray menu refreshed");
    }

    /// Handle a menu event
    pub fn handle_menu_event(&self, event: &MenuEvent) {
        let id_str = event.id().0.as_str();

        if let Some(menu_id) = TrayMenuId::from_str(id_str) {
            debug!(?menu_id, "Tray menu item clicked");

            match menu_id {
                TrayMenuId::StartFeed => {
                    // TODO: Need device selection - for now, use last device or show picker
                    info!("Start feed requested from tray");
                    // This would need to either:
                    // 1. Start with last used device
                    // 2. Open camera picker dialog
                    // For now, we'll emit a placeholder command
                }
                TrayMenuId::StopFeed => {
                    if let Err(e) = self.app_handle.try_send_command(Command::StopCapture) {
                        error!(error = %e, "Failed to send StopCapture command");
                    }
                }
                TrayMenuId::Pause => {
                    if let Err(e) = self.app_handle.try_send_command(Command::PauseDisplay) {
                        error!(error = %e, "Failed to send PauseDisplay command");
                    }
                }
                TrayMenuId::Resume => {
                    if let Err(e) = self.app_handle.try_send_command(Command::ResumeDisplay) {
                        error!(error = %e, "Failed to send ResumeDisplay command");
                    }
                }
                TrayMenuId::Snapshot => {
                    // Default to clipboard
                    if let Err(e) = self
                        .app_handle
                        .try_send_command(Command::TakeSnapshot { to_clipboard: true })
                    {
                        error!(error = %e, "Failed to send TakeSnapshot command");
                    }
                }
                TrayMenuId::Settings => {
                    info!("Settings requested from tray");
                    // TODO: Open settings window
                }
                TrayMenuId::Quit => {
                    info!("Quit requested from tray");
                    self.shutdown.store(true, Ordering::Release);
                    if let Err(e) = self.app_handle.try_send_command(Command::Quit) {
                        error!(error = %e, "Failed to send Quit command");
                    }
                }
            }
        } else {
            warn!(id = %id_str, "Unknown menu item clicked");
        }
    }

    /// Handle a tray icon event (click, double-click, etc.)
    pub fn handle_tray_event(&self, event: &TrayIconEvent) {
        match event {
            TrayIconEvent::Click {
                button,
                button_state,
                ..
            } => {
                debug!(?button, ?button_state, "Tray icon clicked");
                // Left click could toggle feed or open settings
                // For now, just log
            }
            TrayIconEvent::DoubleClick { button, .. } => {
                debug!(?button, "Tray icon double-clicked");
                // Double-click could open settings
            }
            _ => {}
        }
    }

    /// Check if shutdown was requested
    pub fn shutdown_requested(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    /// Get current state
    pub fn state(&self) -> &TrayState {
        &self.state
    }
}

// ============================================================================
// Menu Building
// ============================================================================

/// Build the tray context menu
fn build_tray_menu(state: &TrayState) -> Result<(Menu, TrayMenuItems), TrayError> {
    let menu = Menu::new();

    // Status item (disabled, info only)
    let status_item = MenuItem::with_id(
        MenuId::new("status"),
        state.status_text(),
        false, // disabled
        None::<Accelerator>,
    );
    menu.append(&status_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Separator
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Start Feed
    let start_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::StartFeed.as_str()),
        "Start Feed",
        state.can_start(),
        None::<Accelerator>,
    );
    menu.append(&start_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Stop Feed
    let stop_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::StopFeed.as_str()),
        "Stop Feed",
        state.can_stop(),
        None::<Accelerator>,
    );
    menu.append(&stop_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Pause
    let pause_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::Pause.as_str()),
        "Pause",
        state.can_pause(),
        None::<Accelerator>,
    );
    menu.append(&pause_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Resume
    let resume_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::Resume.as_str()),
        "Resume",
        state.can_resume(),
        None::<Accelerator>,
    );
    menu.append(&resume_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Snapshot
    let snapshot_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::Snapshot.as_str()),
        "Take Snapshot",
        state.can_snapshot(),
        None::<Accelerator>,
    );
    menu.append(&snapshot_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Separator
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Settings
    let settings_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::Settings.as_str()),
        "Settings...",
        true,
        None::<Accelerator>,
    );
    menu.append(&settings_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Separator
    menu.append(&PredefinedMenuItem::separator())
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    // Quit
    let quit_item = MenuItem::with_id(
        MenuId::new(TrayMenuId::Quit.as_str()),
        "Quit",
        true,
        None::<Accelerator>,
    );
    menu.append(&quit_item)
        .map_err(|e| TrayError::Menu(e.to_string()))?;

    let menu_items = TrayMenuItems {
        status_item,
        start_item,
        stop_item,
        pause_item,
        resume_item,
        snapshot_item,
    };

    Ok((menu, menu_items))
}

// ============================================================================
// Icon Loading
// ============================================================================

/// Icon state for visual differentiation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    /// Default/idle state (gray)
    Idle,
    /// Running/active state (green)
    Running,
    /// Paused state (yellow/amber)
    Paused,
    /// Error state (red)
    Error,
    /// Reconnecting state (blue, animated in future)
    Reconnecting,
}

impl From<AppState> for IconState {
    fn from(state: AppState) -> Self {
        match state {
            AppState::Idle | AppState::ShuttingDown => IconState::Idle,
            AppState::Starting | AppState::Running => IconState::Running,
            AppState::Paused => IconState::Paused,
            AppState::Error => IconState::Error,
            AppState::Reconnecting => IconState::Reconnecting,
        }
    }
}

/// Load the tray icon for a given state
///
/// Currently uses programmatically generated icons. In the future, this should:
/// - Load platform-specific icons from resources
/// - Handle dark/light theme detection
fn load_tray_icon() -> Result<tray_icon::Icon, TrayError> {
    load_icon_for_state(IconState::Idle)
}

/// Load an icon for a specific state
pub fn load_icon_for_state(state: IconState) -> Result<tray_icon::Icon, TrayError> {
    let icon_rgba = generate_state_icon(state);
    tray_icon::Icon::from_rgba(icon_rgba, 32, 32).map_err(|e| TrayError::Icon(e.to_string()))
}

/// Generate an icon for a specific state (32x32 RGBA)
///
/// Creates a microscope lens icon with color indicating state:
/// - Idle: Gray
/// - Running: Green with pulse animation (future)
/// - Paused: Amber/Yellow
/// - Error: Red
/// - Reconnecting: Blue
fn generate_state_icon(state: IconState) -> Vec<u8> {
    let size = 32;
    let mut data = Vec::with_capacity(size * size * 4);

    // State-based colors
    let (inner_color, border_color): ([u8; 4], [u8; 4]) = match state {
        IconState::Idle => {
            ([140, 140, 140, 255], [100, 100, 100, 255]) // Gray
        }
        IconState::Running => {
            ([50, 205, 50, 255], [34, 139, 34, 255]) // Green
        }
        IconState::Paused => {
            ([255, 191, 0, 255], [218, 165, 32, 255]) // Amber/Gold
        }
        IconState::Error => {
            ([220, 53, 69, 255], [185, 28, 44, 255]) // Red
        }
        IconState::Reconnecting => {
            ([0, 123, 255, 255], [0, 86, 179, 255]) // Blue
        }
    };

    // Create a microscope lens icon
    let center = size as f32 / 2.0;
    let outer_radius = size as f32 / 3.0;
    let inner_radius = outer_radius - 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= inner_radius {
                // Inside the circle - state color
                data.extend_from_slice(&inner_color);
            } else if dist <= outer_radius {
                // Circle border - darker state color
                data.extend_from_slice(&border_color);
            } else if dist <= outer_radius + 2.0 {
                // Outer glow (subtle)
                let alpha = ((outer_radius + 2.0 - dist) / 2.0 * 100.0) as u8;
                data.extend_from_slice(&[border_color[0], border_color[1], border_color[2], alpha]);
            } else {
                // Transparent
                data.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    // Add center dot for "lens" effect
    let dot_radius = 3.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= dot_radius {
                // Overwrite with lighter center
                let idx = (y * size + x) * 4;
                let highlight = [
                    (inner_color[0] as u16 + 50).min(255) as u8,
                    (inner_color[1] as u16 + 50).min(255) as u8,
                    (inner_color[2] as u16 + 50).min(255) as u8,
                    255,
                ];
                data[idx..idx + 4].copy_from_slice(&highlight);
            }
        }
    }

    data
}

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur with tray operations
#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("Failed to create tray icon: {0}")]
    Creation(String),

    #[error("Failed to create menu: {0}")]
    Menu(String),

    #[error("Failed to load icon: {0}")]
    Icon(String),

    #[error("Platform not supported: {0}")]
    Unsupported(String),
}

// ============================================================================
// Event Loop Integration
// ============================================================================

/// Run the tray event processing loop
///
/// This should be called from the main event loop to process
/// tray icon and menu events. Returns true if shutdown was requested.
///
/// # Example
/// ```ignore
/// loop {
///     // Process other events...
///
///     if tray::process_events(&mut tray_controller) {
///         break; // Shutdown requested
///     }
/// }
/// ```
pub fn process_events(controller: &TrayController) -> bool {
    // Process menu events
    if let Ok(event) = MenuEvent::receiver().try_recv() {
        controller.handle_menu_event(&event);
    }

    // Process tray icon events
    if let Ok(event) = TrayIconEvent::receiver().try_recv() {
        controller.handle_tray_event(&event);
    }

    controller.shutdown_requested()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tray_state_default() {
        let state = TrayState::default();
        assert_eq!(state.app_state, AppState::Idle);
        assert!(state.resolution.is_none());
        assert!(state.fps.is_none());
    }

    #[test]
    fn test_tray_state_running() {
        let state = TrayState::running((1920, 1080), 30.0, "USB Camera");
        assert_eq!(state.app_state, AppState::Running);
        assert_eq!(state.resolution, Some((1920, 1080)));
        assert_eq!(state.fps, Some(30.0));
        assert_eq!(state.camera_name, Some("USB Camera".to_string()));
    }

    #[test]
    fn test_tooltip_generation() {
        let idle = TrayState::default();
        assert!(idle.tooltip().contains("Idle"));

        let running = TrayState::running((1920, 1080), 30.0, "Camera");
        assert!(running.tooltip().contains("Live"));
        assert!(running.tooltip().contains("1920x1080"));

        let paused = TrayState::paused((1920, 1080), 30.0, "Camera");
        assert!(paused.tooltip().contains("Paused"));
    }

    #[test]
    fn test_state_transitions() {
        let idle = TrayState::default();
        assert!(idle.can_start());
        assert!(!idle.can_stop());
        assert!(!idle.can_pause());
        assert!(!idle.can_resume());
        assert!(!idle.can_snapshot());

        let running = TrayState::running((1920, 1080), 30.0, "Camera");
        assert!(!running.can_start());
        assert!(running.can_stop());
        assert!(running.can_pause());
        assert!(!running.can_resume());
        assert!(running.can_snapshot());

        let paused = TrayState::paused((1920, 1080), 30.0, "Camera");
        assert!(!paused.can_start());
        assert!(paused.can_stop());
        assert!(!paused.can_pause());
        assert!(paused.can_resume());
        assert!(paused.can_snapshot());
    }

    #[test]
    fn test_menu_id_roundtrip() {
        for id in [
            TrayMenuId::StartFeed,
            TrayMenuId::StopFeed,
            TrayMenuId::Pause,
            TrayMenuId::Resume,
            TrayMenuId::Snapshot,
            TrayMenuId::Settings,
            TrayMenuId::Quit,
        ] {
            let s = id.as_str();
            let parsed = TrayMenuId::from_str(s);
            assert_eq!(parsed, Some(id), "Failed roundtrip for {:?}", id);
        }
    }

    #[test]
    fn test_idle_icon_generation() {
        let icon = generate_state_icon(IconState::Idle);
        assert_eq!(icon.len(), 32 * 32 * 4); // 32x32 RGBA
    }

    #[test]
    fn test_icon_state_from_app_state() {
        assert_eq!(IconState::from(AppState::Idle), IconState::Idle);
        assert_eq!(IconState::from(AppState::ShuttingDown), IconState::Idle);
        assert_eq!(IconState::from(AppState::Starting), IconState::Running);
        assert_eq!(IconState::from(AppState::Running), IconState::Running);
        assert_eq!(IconState::from(AppState::Paused), IconState::Paused);
        assert_eq!(IconState::from(AppState::Error), IconState::Error);
        assert_eq!(
            IconState::from(AppState::Reconnecting),
            IconState::Reconnecting
        );
    }

    #[test]
    fn test_state_icons_generation() {
        // Test all icon states generate correctly sized icons
        for state in [
            IconState::Idle,
            IconState::Running,
            IconState::Paused,
            IconState::Error,
            IconState::Reconnecting,
        ] {
            let icon = generate_state_icon(state);
            assert_eq!(
                icon.len(),
                32 * 32 * 4,
                "Icon for {:?} has wrong size",
                state
            );

            // Verify RGBA format (check first pixel has 4 bytes)
            assert!(icon.len() >= 4);
        }
    }

    #[test]
    fn test_state_icons_have_different_colors() {
        // Icons for different states should have different colors
        let idle = generate_state_icon(IconState::Idle);
        let running = generate_state_icon(IconState::Running);
        let error = generate_state_icon(IconState::Error);

        // Find a non-transparent pixel in each (center should be colored)
        let center_idx = (16 * 32 + 16) * 4;

        // Idle is gray, Running is green, Error is red - should differ
        assert_ne!(
            idle[center_idx..center_idx + 3],
            running[center_idx..center_idx + 3]
        );
        assert_ne!(
            running[center_idx..center_idx + 3],
            error[center_idx..center_idx + 3]
        );
        assert_ne!(
            idle[center_idx..center_idx + 3],
            error[center_idx..center_idx + 3]
        );
    }
}
