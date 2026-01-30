//! Settings window implementation using egui
//!
//! Provides the main configuration interface for users to adjust camera,
//! display, overlay, and startup settings.

use std::sync::{Arc, RwLock};

use crate::config::{AppConfig, CameraConfig, DisplayConfig, StartupConfig};
use crate::core::events::{AppHandle, Command};
use crate::core::{CameraDevice, DisplayId, Flip, Rotation, ScalingMode};

// ============================================================================
// Settings State
// ============================================================================

/// Current state of the settings window
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsWindowState {
    /// Window is hidden
    Hidden,
    /// Window is visible
    Visible,
    /// Window is visible and has unsaved changes
    Modified,
}

/// Settings window controller
///
/// Manages the settings UI state and coordinates with the application
/// through the command/event system.
pub struct SettingsController {
    /// Application handle for sending commands
    app_handle: AppHandle,
    /// Current window state
    state: SettingsWindowState,
    /// Working copy of camera config (edited but not yet applied)
    camera_config: CameraConfig,
    /// Working copy of display config
    display_config: DisplayConfig,
    /// Working copy of startup config
    startup_config: StartupConfig,
    /// List of available cameras
    available_cameras: Vec<CameraDevice>,
    /// List of available displays
    available_displays: Vec<DisplayInfo>,
    /// Currently selected camera index
    selected_camera_idx: usize,
    /// Currently selected display index
    selected_display_idx: usize,
    /// Currently selected resolution index
    selected_resolution_idx: usize,
    /// Error message to display (if any)
    error_message: Option<String>,
    /// Success message to display (if any)
    success_message: Option<String>,
    /// Whether the overlay section is expanded
    overlay_expanded: bool,
}

/// Simple display info for the UI
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub id: DisplayId,
    pub name: String,
    pub resolution: (u32, u32),
    pub is_primary: bool,
}

impl SettingsController {
    /// Create a new settings controller
    pub fn new(app_handle: AppHandle, config: &AppConfig) -> Self {
        Self {
            app_handle,
            state: SettingsWindowState::Hidden,
            camera_config: config.camera.clone(),
            display_config: config.display.clone(),
            startup_config: config.startup.clone(),
            available_cameras: Vec::new(),
            available_displays: vec![DisplayInfo {
                id: DisplayId("primary".to_string()),
                name: "Primary Display".to_string(),
                resolution: (1920, 1080),
                is_primary: true,
            }],
            selected_camera_idx: 0,
            selected_display_idx: 0,
            selected_resolution_idx: 0,
            error_message: None,
            success_message: None,
            overlay_expanded: false,
        }
    }

    /// Show the settings window
    pub fn show(&mut self) {
        self.state = SettingsWindowState::Visible;
        self.error_message = None;
        self.success_message = None;
    }

    /// Hide the settings window
    pub fn hide(&mut self) {
        self.state = SettingsWindowState::Hidden;
    }

    /// Check if the window is visible
    pub fn is_visible(&self) -> bool {
        matches!(self.state, SettingsWindowState::Visible | SettingsWindowState::Modified)
    }

    /// Check if there are unsaved changes
    pub fn has_changes(&self) -> bool {
        matches!(self.state, SettingsWindowState::Modified)
    }

    /// Update the list of available cameras
    pub fn set_available_cameras(&mut self, cameras: Vec<CameraDevice>) {
        self.available_cameras = cameras;
        // Update selected index based on current config
        if let Some(ref device_id) = self.camera_config.device_id {
            self.selected_camera_idx = self.available_cameras
                .iter()
                .position(|c| &c.id == device_id)
                .unwrap_or(0);
        }
    }

    /// Update the list of available displays
    pub fn set_available_displays(&mut self, displays: Vec<DisplayInfo>) {
        self.available_displays = displays;
        // Ensure we have at least the primary display
        if self.available_displays.is_empty() {
            self.available_displays.push(DisplayInfo {
                id: DisplayId("primary".to_string()),
                name: "Primary Display".to_string(),
                resolution: (1920, 1080),
                is_primary: true,
            });
        }
    }

    /// Apply current settings
    pub fn apply(&mut self) -> Result<(), SettingsError> {
        // Validate settings
        self.validate()?;

        // Send command to update settings (ignore channel full errors for now)
        let _ = self.app_handle.try_send_command(Command::UpdateCaptureSettings {
            settings: crate::core::CaptureSettings {
                width: self.camera_config.width,
                height: self.camera_config.height,
                framerate: self.camera_config.framerate,
                format: None,
            },
        });

        // Update scaling mode
        let _ = self.app_handle.try_send_command(Command::SetScaling {
            mode: self.display_config.scaling_mode,
        });

        // Update rotation
        let _ = self.app_handle.try_send_command(Command::SetRotation {
            rotation: match self.display_config.rotation {
                0 => Rotation::None,
                90 => Rotation::Clockwise90,
                180 => Rotation::Clockwise180,
                270 => Rotation::Clockwise270,
                _ => Rotation::None,
            },
        });

        // Update flip
        let _ = self.app_handle.try_send_command(Command::SetFlip {
            flip: match (self.display_config.flip_horizontal, self.display_config.flip_vertical) {
                (true, true) => Flip::Both,
                (true, false) => Flip::Horizontal,
                (false, true) => Flip::Vertical,
                (false, false) => Flip::None,
            },
        });

        self.state = SettingsWindowState::Visible;
        self.success_message = Some("Settings applied successfully".to_string());
        Ok(())
    }

    /// Validate current settings
    fn validate(&self) -> Result<(), SettingsError> {
        if self.camera_config.width == 0 || self.camera_config.height == 0 {
            return Err(SettingsError::InvalidValue("Resolution must be non-zero".to_string()));
        }
        if self.camera_config.framerate <= 0.0 || self.camera_config.framerate > 240.0 {
            return Err(SettingsError::InvalidValue("Framerate must be between 0 and 240".to_string()));
        }
        if !matches!(self.display_config.rotation, 0 | 90 | 180 | 270) {
            return Err(SettingsError::InvalidValue("Rotation must be 0, 90, 180, or 270".to_string()));
        }
        Ok(())
    }

    /// Reset to default values
    pub fn reset_to_defaults(&mut self) {
        self.camera_config = CameraConfig::default();
        self.display_config = DisplayConfig::default();
        self.startup_config = StartupConfig::default();
        self.state = SettingsWindowState::Modified;
    }

    /// Mark as modified
    fn mark_modified(&mut self) {
        if self.state == SettingsWindowState::Visible {
            self.state = SettingsWindowState::Modified;
        }
    }

    /// Get the current config for saving
    pub fn get_config(&self) -> (CameraConfig, DisplayConfig, StartupConfig) {
        (
            self.camera_config.clone(),
            self.display_config.clone(),
            self.startup_config.clone(),
        )
    }

    /// Reload config from AppConfig
    pub fn reload_from_config(&mut self, config: &AppConfig) {
        self.camera_config = config.camera.clone();
        self.display_config = config.display.clone();
        self.startup_config = config.startup.clone();
        self.state = SettingsWindowState::Visible;
    }
}

// ============================================================================
// UI Rendering
// ============================================================================

/// Settings UI state for egui rendering
pub struct SettingsUI {
    controller: Arc<RwLock<SettingsController>>,
}

impl SettingsUI {
    /// Create new settings UI
    pub fn new(controller: Arc<RwLock<SettingsController>>) -> Self {
        Self { controller }
    }

    /// Render the settings window
    ///
    /// Returns true if the window should remain open, false if it should close.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        let mut should_apply = false;
        let mut should_reset = false;
        let mut should_refresh_cameras = false;
        let mut should_close = false;

        egui::Window::new("Micround Settings")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                let mut controller = self.controller.write().unwrap();

                // Error/success messages
                if let Some(ref msg) = controller.error_message {
                    ui.colored_label(egui::Color32::RED, msg);
                    ui.add_space(8.0);
                }
                if let Some(ref msg) = controller.success_message {
                    ui.colored_label(egui::Color32::GREEN, msg);
                    ui.add_space(8.0);
                }

                // Camera section
                ui.heading("Camera");
                ui.separator();
                self.render_camera_section(ui, &mut controller, &mut should_refresh_cameras);
                ui.add_space(16.0);

                // Display section
                ui.heading("Display");
                ui.separator();
                self.render_display_section(ui, &mut controller);
                ui.add_space(16.0);

                // Overlay section (collapsible)
                egui::CollapsingHeader::new("Overlay")
                    .default_open(controller.overlay_expanded)
                    .show(ui, |ui| {
                        self.render_overlay_section(ui, &mut controller);
                    });
                ui.add_space(16.0);

                // Startup section
                ui.heading("Startup");
                ui.separator();
                self.render_startup_section(ui, &mut controller);
                ui.add_space(24.0);

                // Action buttons
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        should_apply = true;
                    }
                    if ui.button("Reset to Defaults").clicked() {
                        should_reset = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            should_close = true;
                        }
                    });
                });
            });

        // Apply close action after window rendering (to avoid double mutable borrow)
        if should_close {
            open = false;
        }

        // Handle actions after releasing the write lock
        if should_apply {
            let mut controller = self.controller.write().unwrap();
            if let Err(e) = controller.apply() {
                controller.error_message = Some(e.to_string());
                controller.success_message = None;
            }
        }
        if should_reset {
            self.controller.write().unwrap().reset_to_defaults();
        }
        if should_refresh_cameras {
            // Signal to refresh camera list
            let controller = self.controller.read().unwrap();
            let _ = controller.app_handle.try_send_command(Command::RefreshCameras);
        }

        if !open {
            self.controller.write().unwrap().hide();
        }

        open
    }

    fn render_camera_section(
        &self,
        ui: &mut egui::Ui,
        controller: &mut SettingsController,
        should_refresh: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.label("Camera:");

            let camera_names: Vec<String> = controller.available_cameras
                .iter()
                .map(|c| c.name.clone())
                .collect();

            if camera_names.is_empty() {
                ui.label("No cameras detected");
            } else {
                let selected_name = camera_names.get(controller.selected_camera_idx)
                    .cloned()
                    .unwrap_or_else(|| "Select camera".to_string());

                egui::ComboBox::from_id_source("camera_select")
                    .selected_text(selected_name)
                    .show_ui(ui, |ui| {
                        for (idx, name) in camera_names.iter().enumerate() {
                            if ui.selectable_value(
                                &mut controller.selected_camera_idx,
                                idx,
                                name
                            ).changed() {
                                if let Some(camera) = controller.available_cameras.get(idx) {
                                    controller.camera_config.device_id = Some(camera.id.clone());
                                    controller.mark_modified();
                                }
                            }
                        }
                    });
            }

            if ui.button("🔄 Refresh").clicked() {
                *should_refresh = true;
            }
        });

        ui.horizontal(|ui| {
            ui.label("Resolution:");

            // Get resolutions from selected camera's capabilities
            let resolutions: Vec<String> = if let Some(camera) =
                controller.available_cameras.get(controller.selected_camera_idx)
            {
                camera.capabilities
                    .iter()
                    .map(|c| format!("{}x{} @ {}fps", c.width, c.height, c.framerate as u32))
                    .collect()
            } else {
                vec![
                    "1920x1080 @ 30fps".to_string(),
                    "1280x720 @ 60fps".to_string(),
                    "1280x720 @ 30fps".to_string(),
                    "640x480 @ 30fps".to_string(),
                ]
            };

            let current_res = format!(
                "{}x{} @ {}fps",
                controller.camera_config.width,
                controller.camera_config.height,
                controller.camera_config.framerate as u32
            );

            egui::ComboBox::from_id_source("resolution_select")
                .selected_text(&current_res)
                .show_ui(ui, |ui| {
                    for (idx, res) in resolutions.iter().enumerate() {
                        if ui.selectable_label(
                            controller.selected_resolution_idx == idx,
                            res
                        ).clicked() {
                            controller.selected_resolution_idx = idx;
                            // Parse resolution from string
                            if let Some(camera) =
                                controller.available_cameras.get(controller.selected_camera_idx)
                            {
                                if let Some(cap) = camera.capabilities.get(idx) {
                                    controller.camera_config.width = cap.width;
                                    controller.camera_config.height = cap.height;
                                    controller.camera_config.framerate = cap.framerate;
                                    controller.mark_modified();
                                }
                            }
                        }
                    }
                });
        });
    }

    fn render_display_section(&self, ui: &mut egui::Ui, controller: &mut SettingsController) {
        ui.horizontal(|ui| {
            ui.label("Target Display:");

            let display_names: Vec<String> = controller.available_displays
                .iter()
                .map(|d| {
                    if d.is_primary {
                        format!("{} (Primary)", d.name)
                    } else {
                        d.name.clone()
                    }
                })
                .collect();

            let selected_name = display_names.get(controller.selected_display_idx)
                .cloned()
                .unwrap_or_else(|| "Primary Display".to_string());

            egui::ComboBox::from_id_source("display_select")
                .selected_text(selected_name)
                .show_ui(ui, |ui| {
                    for (idx, name) in display_names.iter().enumerate() {
                        if ui.selectable_value(
                            &mut controller.selected_display_idx,
                            idx,
                            name
                        ).changed() {
                            if let Some(display) = controller.available_displays.get(idx) {
                                controller.display_config.display_id = Some(display.id.clone());
                                controller.mark_modified();
                            }
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Scaling Mode:");

            let scaling_modes = [
                (ScalingMode::Fill, "Fill (crop to fit)"),
                (ScalingMode::Fit, "Fit (letterbox)"),
                (ScalingMode::Stretch, "Stretch"),
                (ScalingMode::Center, "Center (no scaling)"),
            ];

            let current_label = scaling_modes.iter()
                .find(|(m, _)| *m == controller.display_config.scaling_mode)
                .map(|(_, l)| *l)
                .unwrap_or("Fill");

            egui::ComboBox::from_id_source("scaling_select")
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    for (mode, label) in scaling_modes {
                        if ui.selectable_value(
                            &mut controller.display_config.scaling_mode,
                            mode,
                            label
                        ).changed() {
                            controller.mark_modified();
                        }
                    }
                });
        });

        ui.horizontal(|ui| {
            ui.label("Rotation:");

            let rotations = [
                (0, "0°"),
                (90, "90°"),
                (180, "180°"),
                (270, "270°"),
            ];

            for (angle, label) in rotations {
                if ui.selectable_value(
                    &mut controller.display_config.rotation,
                    angle,
                    label
                ).changed() {
                    controller.mark_modified();
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Flip:");

            if ui.checkbox(&mut controller.display_config.flip_horizontal, "Horizontal").changed() {
                controller.mark_modified();
            }
            if ui.checkbox(&mut controller.display_config.flip_vertical, "Vertical").changed() {
                controller.mark_modified();
            }
        });
    }

    fn render_overlay_section(&self, ui: &mut egui::Ui, _controller: &mut SettingsController) {
        // Overlay settings placeholder
        ui.label("Overlay options coming soon...");
        ui.checkbox(&mut false, "Show timestamp");
        ui.horizontal(|ui| {
            ui.label("Custom text:");
            ui.text_edit_singleline(&mut String::new());
        });
    }

    fn render_startup_section(&self, ui: &mut egui::Ui, controller: &mut SettingsController) {
        if ui.checkbox(&mut controller.startup_config.launch_at_login, "Launch at login").changed() {
            controller.mark_modified();
        }

        if ui.checkbox(&mut controller.startup_config.auto_start_feed, "Start feed automatically").changed() {
            controller.mark_modified();
        }

        if ui.checkbox(&mut controller.startup_config.minimize_on_start, "Minimize to tray on startup").changed() {
            controller.mark_modified();
        }
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Settings-related errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum SettingsError {
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    #[error("Camera not found: {0}")]
    CameraNotFound(String),
    #[error("Display not found: {0}")]
    DisplayNotFound(String),
    #[error("Apply failed: {0}")]
    ApplyFailed(String),
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::AppContext;
    use crate::core::DeviceId;

    fn create_test_controller() -> SettingsController {
        let (ctx, _rx) = AppContext::new();
        let config = AppConfig::default();
        SettingsController::new(ctx.handle(), &config)
    }

    #[test]
    fn test_settings_controller_creation() {
        let controller = create_test_controller();
        assert_eq!(controller.state, SettingsWindowState::Hidden);
        assert!(!controller.is_visible());
    }

    #[test]
    fn test_show_hide() {
        let mut controller = create_test_controller();

        controller.show();
        assert!(controller.is_visible());
        assert_eq!(controller.state, SettingsWindowState::Visible);

        controller.hide();
        assert!(!controller.is_visible());
        assert_eq!(controller.state, SettingsWindowState::Hidden);
    }

    #[test]
    fn test_mark_modified() {
        let mut controller = create_test_controller();

        // Hidden state should not become modified
        controller.mark_modified();
        assert_eq!(controller.state, SettingsWindowState::Hidden);

        // Visible state should become modified
        controller.show();
        controller.mark_modified();
        assert_eq!(controller.state, SettingsWindowState::Modified);
        assert!(controller.has_changes());
    }

    #[test]
    fn test_reset_to_defaults() {
        let mut controller = create_test_controller();
        controller.show();

        // Modify settings
        controller.camera_config.width = 1280;
        controller.display_config.rotation = 90;
        controller.startup_config.launch_at_login = true;

        // Reset
        controller.reset_to_defaults();

        assert_eq!(controller.camera_config.width, 1920);
        assert_eq!(controller.display_config.rotation, 0);
        assert!(!controller.startup_config.launch_at_login);
        assert!(controller.has_changes());
    }

    #[test]
    fn test_validate_success() {
        let controller = create_test_controller();
        assert!(controller.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_resolution() {
        let mut controller = create_test_controller();
        controller.camera_config.width = 0;

        let result = controller.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Resolution"));
    }

    #[test]
    fn test_validate_invalid_framerate() {
        let mut controller = create_test_controller();
        controller.camera_config.framerate = -5.0;

        let result = controller.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Framerate"));
    }

    #[test]
    fn test_validate_invalid_rotation() {
        let mut controller = create_test_controller();
        controller.display_config.rotation = 45;

        let result = controller.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Rotation"));
    }

    #[test]
    fn test_get_config() {
        let mut controller = create_test_controller();
        controller.camera_config.width = 1280;
        controller.display_config.scaling_mode = ScalingMode::Fit;
        controller.startup_config.auto_start_feed = false;

        let (camera, display, startup) = controller.get_config();

        assert_eq!(camera.width, 1280);
        assert_eq!(display.scaling_mode, ScalingMode::Fit);
        assert!(!startup.auto_start_feed);
    }

    #[test]
    fn test_set_available_cameras() {
        let mut controller = create_test_controller();

        let cameras = vec![
            CameraDevice {
                id: DeviceId("camera1".to_string()),
                name: "USB Camera".to_string(),
                manufacturer: None,
                capabilities: vec![],
                is_available: true,
            },
            CameraDevice {
                id: DeviceId("camera2".to_string()),
                name: "Microscope".to_string(),
                manufacturer: Some("Acme".to_string()),
                capabilities: vec![],
                is_available: true,
            },
        ];

        controller.set_available_cameras(cameras);

        assert_eq!(controller.available_cameras.len(), 2);
        assert_eq!(controller.available_cameras[0].name, "USB Camera");
    }

    #[test]
    fn test_set_available_displays() {
        let mut controller = create_test_controller();

        let displays = vec![
            DisplayInfo {
                id: DisplayId("display1".to_string()),
                name: "Monitor 1".to_string(),
                resolution: (1920, 1080),
                is_primary: true,
            },
            DisplayInfo {
                id: DisplayId("display2".to_string()),
                name: "Monitor 2".to_string(),
                resolution: (2560, 1440),
                is_primary: false,
            },
        ];

        controller.set_available_displays(displays);

        assert_eq!(controller.available_displays.len(), 2);
        assert!(controller.available_displays[0].is_primary);
    }

    #[test]
    fn test_reload_from_config() {
        let mut controller = create_test_controller();
        controller.hide();

        let mut new_config = AppConfig::default();
        new_config.camera.width = 1280;
        new_config.display.rotation = 180;

        controller.reload_from_config(&new_config);

        assert_eq!(controller.camera_config.width, 1280);
        assert_eq!(controller.display_config.rotation, 180);
        assert!(controller.is_visible());
    }
}
