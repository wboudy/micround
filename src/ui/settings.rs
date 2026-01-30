//! Settings window implementation using egui
//!
//! Provides the main configuration interface for users to adjust camera,
//! display, overlay, and startup settings.
//!
//! # Camera Preview
//! The settings window includes a live camera preview (bd-37z) that allows users
//! to verify camera selection and see the effect of transforms before applying.

use std::sync::{Arc, RwLock};

use crate::config::{AppConfig, CameraConfig, DisplayConfig, StartupConfig};
use crate::core::events::{AppHandle, Command};
use crate::core::{CameraDevice, DisplayId, Flip, Rotation, ScalingMode};

// ============================================================================
// Preview State
// ============================================================================

/// Preview panel dimensions (16:9 aspect ratio at small size)
const PREVIEW_WIDTH: u32 = 320;
const PREVIEW_HEIGHT: u32 = 180;

/// State of the camera preview in settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewState {
    /// Preview is not active
    #[default]
    Inactive,
    /// Preview is starting (camera opening)
    Starting,
    /// Preview is running and showing live frames
    Running,
    /// Preview encountered an error
    Error,
    /// No camera is selected
    NoCameraSelected,
}

impl std::fmt::Display for PreviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewState::Inactive => write!(f, "Preview inactive"),
            PreviewState::Starting => write!(f, "Starting preview..."),
            PreviewState::Running => write!(f, "Preview running"),
            PreviewState::Error => write!(f, "Preview error"),
            PreviewState::NoCameraSelected => write!(f, "Select a camera"),
        }
    }
}

/// Holds the current preview frame data for display
#[derive(Default)]
pub struct PreviewFrame {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Frame sequence number (for detecting updates)
    pub sequence: u64,
}

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
    /// Preview state
    preview_state: PreviewState,
    /// Current preview frame (updated by capture thread)
    preview_frame: Option<PreviewFrame>,
    /// Preview error message
    preview_error: Option<String>,
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
            preview_state: PreviewState::Inactive,
            preview_frame: None,
            preview_error: None,
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

    // ========================================================================
    // Preview Methods
    // ========================================================================

    /// Start the camera preview
    pub fn start_preview(&mut self) {
        if self.camera_config.device_id.is_none() {
            self.preview_state = PreviewState::NoCameraSelected;
            self.preview_error = Some("Please select a camera first".to_string());
            return;
        }

        self.preview_state = PreviewState::Starting;
        self.preview_error = None;

        // Send command to start preview capture
        let _ = self.app_handle.try_send_command(Command::StartPreview {
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
        });
    }

    /// Stop the camera preview
    pub fn stop_preview(&mut self) {
        self.preview_state = PreviewState::Inactive;
        self.preview_frame = None;
        self.preview_error = None;

        // Send command to stop preview capture
        let _ = self.app_handle.try_send_command(Command::StopPreview);
    }

    /// Check if preview is active
    pub fn is_preview_active(&self) -> bool {
        matches!(
            self.preview_state,
            PreviewState::Starting | PreviewState::Running
        )
    }

    /// Update the preview frame (called by capture thread)
    pub fn update_preview_frame(&mut self, frame: PreviewFrame) {
        self.preview_state = PreviewState::Running;
        self.preview_frame = Some(frame);
        self.preview_error = None;
    }

    /// Set preview error state
    pub fn set_preview_error(&mut self, error: String) {
        self.preview_state = PreviewState::Error;
        self.preview_error = Some(error);
    }

    /// Get current preview state
    pub fn preview_state(&self) -> PreviewState {
        self.preview_state
    }

    /// Get preview frame reference (if available)
    pub fn preview_frame(&self) -> Option<&PreviewFrame> {
        self.preview_frame.as_ref()
    }
}

// ============================================================================
// UI Rendering
// ============================================================================

/// Settings UI state for egui rendering
pub struct SettingsUI {
    controller: Arc<RwLock<SettingsController>>,
    /// Texture handle for the preview (managed by egui)
    preview_texture: Option<egui::TextureHandle>,
    /// Last sequence number to detect frame updates
    last_preview_sequence: u64,
}

impl SettingsUI {
    /// Create new settings UI
    pub fn new(controller: Arc<RwLock<SettingsController>>) -> Self {
        Self {
            controller,
            preview_texture: None,
            last_preview_sequence: 0,
        }
    }

    /// Render the settings window
    ///
    /// Returns true if the window should remain open, false if it should close.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        // Update preview texture if we have a new frame
        self.update_preview_texture(ctx);

        let mut open = true;
        let mut should_apply = false;
        let mut should_reset = false;
        let mut should_refresh_cameras = false;
        let mut should_close = false;
        let mut should_start_preview = false;
        let mut should_stop_preview = false;

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
                ui.add_space(8.0);

                // Camera preview section (bd-37z)
                self.render_preview_section(
                    ui,
                    &controller,
                    &mut should_start_preview,
                    &mut should_stop_preview,
                );
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
        if should_start_preview {
            self.controller.write().unwrap().start_preview();
        }
        if should_stop_preview {
            self.controller.write().unwrap().stop_preview();
            // Clear the texture when stopping
            self.preview_texture = None;
            self.last_preview_sequence = 0;
        }

        if !open {
            // Stop preview when closing window
            let mut controller = self.controller.write().unwrap();
            if controller.is_preview_active() {
                controller.stop_preview();
            }
            controller.hide();
            self.preview_texture = None;
        }

        open
    }

    /// Update the preview texture from the latest frame
    fn update_preview_texture(&mut self, ctx: &egui::Context) {
        let controller = self.controller.read().unwrap();

        if let Some(ref frame) = controller.preview_frame {
            // Only update if we have a new frame
            if frame.sequence > self.last_preview_sequence && !frame.data.is_empty() {
                // Create ColorImage from RGBA data
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [frame.width as usize, frame.height as usize],
                    &frame.data,
                );

                // Update or create texture
                if let Some(ref mut texture) = self.preview_texture {
                    texture.set(image, egui::TextureOptions::LINEAR);
                } else {
                    self.preview_texture = Some(ctx.load_texture(
                        "camera_preview",
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }

                self.last_preview_sequence = frame.sequence;
            }
        }
    }

    /// Render the camera preview section (bd-37z)
    fn render_preview_section(
        &self,
        ui: &mut egui::Ui,
        controller: &SettingsController,
        should_start: &mut bool,
        should_stop: &mut bool,
    ) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("Preview");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Preview toggle button
                    if controller.is_preview_active() {
                        if ui.button("⏹ Stop").clicked() {
                            *should_stop = true;
                        }
                    } else if ui.button("▶ Start Preview").clicked() {
                        *should_start = true;
                    }
                });
            });

            ui.add_space(4.0);

            // Preview area (fixed size)
            let preview_size = egui::vec2(PREVIEW_WIDTH as f32, PREVIEW_HEIGHT as f32);

            // Draw the preview or placeholder
            match controller.preview_state {
                PreviewState::Running => {
                    if let Some(ref texture) = self.preview_texture {
                        ui.image(egui::load::SizedTexture::new(texture.id(), preview_size));
                    } else {
                        // Texture not yet available, show loading
                        self.render_preview_placeholder(ui, preview_size, "Loading...");
                    }
                }
                PreviewState::Starting => {
                    self.render_preview_placeholder(ui, preview_size, "Starting camera...");
                }
                PreviewState::Error => {
                    let msg = controller.preview_error.as_deref().unwrap_or("Preview error");
                    self.render_preview_placeholder(ui, preview_size, msg);
                }
                PreviewState::NoCameraSelected => {
                    self.render_preview_placeholder(ui, preview_size, "Select a camera above");
                }
                PreviewState::Inactive => {
                    self.render_preview_placeholder(ui, preview_size, "Click 'Start Preview' to test camera");
                }
            }

            // Preview status indicator
            ui.horizontal(|ui| {
                let (color, text) = match controller.preview_state {
                    PreviewState::Running => (egui::Color32::GREEN, "● Live"),
                    PreviewState::Starting => (egui::Color32::YELLOW, "○ Starting"),
                    PreviewState::Error => (egui::Color32::RED, "● Error"),
                    PreviewState::NoCameraSelected | PreviewState::Inactive => {
                        (egui::Color32::GRAY, "○ Inactive")
                    }
                };
                ui.colored_label(color, text);
            });
        });
    }

    /// Render a placeholder for the preview area
    fn render_preview_placeholder(&self, ui: &mut egui::Ui, size: egui::Vec2, text: &str) {
        let (rect, _response) = ui.allocate_exact_size(size, egui::Sense::hover());

        // Dark background
        ui.painter().rect_filled(rect, 4.0, egui::Color32::from_gray(30));

        // Border
        ui.painter().rect_stroke(
            rect,
            4.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
        );

        // Centered text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::FontId::proportional(14.0),
            egui::Color32::GRAY,
        );
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

    // ========================================================================
    // Preview Tests (bd-37z)
    // ========================================================================

    #[test]
    fn test_preview_initial_state() {
        let controller = create_test_controller();
        assert_eq!(controller.preview_state(), PreviewState::Inactive);
        assert!(!controller.is_preview_active());
        assert!(controller.preview_frame().is_none());
    }

    #[test]
    fn test_preview_start_without_camera() {
        let mut controller = create_test_controller();
        controller.camera_config.device_id = None;

        controller.start_preview();

        assert_eq!(controller.preview_state(), PreviewState::NoCameraSelected);
        assert!(controller.preview_error.is_some());
    }

    #[test]
    fn test_preview_start_with_camera() {
        let mut controller = create_test_controller();
        controller.camera_config.device_id = Some(DeviceId("test_cam".to_string()));

        controller.start_preview();

        assert_eq!(controller.preview_state(), PreviewState::Starting);
        assert!(controller.is_preview_active());
        assert!(controller.preview_error.is_none());
    }

    #[test]
    fn test_preview_stop() {
        let mut controller = create_test_controller();
        controller.camera_config.device_id = Some(DeviceId("test_cam".to_string()));

        // Start then stop
        controller.start_preview();
        controller.stop_preview();

        assert_eq!(controller.preview_state(), PreviewState::Inactive);
        assert!(!controller.is_preview_active());
        assert!(controller.preview_frame().is_none());
    }

    #[test]
    fn test_preview_update_frame() {
        let mut controller = create_test_controller();
        controller.camera_config.device_id = Some(DeviceId("test_cam".to_string()));
        controller.start_preview();

        // Simulate receiving a frame
        let frame = PreviewFrame {
            data: vec![0u8; 320 * 180 * 4], // RGBA
            width: 320,
            height: 180,
            sequence: 1,
        };
        controller.update_preview_frame(frame);

        assert_eq!(controller.preview_state(), PreviewState::Running);
        assert!(controller.preview_frame().is_some());
        assert_eq!(controller.preview_frame().unwrap().sequence, 1);
    }

    #[test]
    fn test_preview_error_state() {
        let mut controller = create_test_controller();
        controller.camera_config.device_id = Some(DeviceId("test_cam".to_string()));
        controller.start_preview();

        // Simulate an error
        controller.set_preview_error("Camera disconnected".to_string());

        assert_eq!(controller.preview_state(), PreviewState::Error);
        assert!(controller.preview_error.is_some());
        assert_eq!(controller.preview_error.as_deref(), Some("Camera disconnected"));
    }

    #[test]
    fn test_preview_state_display() {
        // Test Display impl for PreviewState
        assert_eq!(PreviewState::Inactive.to_string(), "Preview inactive");
        assert_eq!(PreviewState::Running.to_string(), "Preview running");
        assert_eq!(PreviewState::Error.to_string(), "Preview error");
        assert_eq!(PreviewState::NoCameraSelected.to_string(), "Select a camera");
    }

    #[test]
    fn test_preview_frame_default() {
        let frame = PreviewFrame::default();
        assert!(frame.data.is_empty());
        assert_eq!(frame.width, 0);
        assert_eq!(frame.height, 0);
        assert_eq!(frame.sequence, 0);
    }
}
