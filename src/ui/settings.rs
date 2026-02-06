//! Settings window using egui
//!
//! Provides a cross-platform settings UI for camera, display, and startup configuration.
//!
//! # Architecture
//!
//! The settings window operates independently of the main capture/render loop.
//! Changes are staged in a local copy of AppConfig and only applied when the user
//! clicks "Apply" or "OK".
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │  Settings                                              [X]  │
//! ├──────────────────────────────────────────────────────────────┤
//! │  ┌─ Camera ─────────────────────────────────────────────┐   │
//! │  │  Device:     [USB Microscope ▼]        [Refresh]     │   │
//! │  │  Resolution: [1920x1080 ▼]                           │   │
//! │  │  Framerate:  [30 ▼]                                  │   │
//! │  │                                                      │   │
//! │  │  ┌─────────────────────┐                             │   │
//! │  │  │    Live Preview     │                             │   │
//! │  │  └─────────────────────┘                             │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                              │
//! │  ┌─ Display ────────────────────────────────────────────┐   │
//! │  │  Target:    [Primary Display ▼]                      │   │
//! │  │  Scaling:   ○ Fit  ● Fill  ○ Stretch  ○ Center       │   │
//! │  │  Rotation:  ○ 0°  ○ 90°  ○ 180°  ○ 270°              │   │
//! │  │  Flip:      ☐ Horizontal  ☐ Vertical                 │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                              │
//! │  ┌─ Startup ────────────────────────────────────────────┐   │
//! │  │  ☐ Launch at login                                   │   │
//! │  │  ☐ Auto-start feed                                   │   │
//! │  │  ☐ Minimize to tray on startup                       │   │
//! │  └──────────────────────────────────────────────────────┘   │
//! │                                                              │
//! │                            [Cancel]  [Apply]  [OK]          │
//! └──────────────────────────────────────────────────────────────┘
//! ```

use egui::{Context, Id, TextureHandle, Ui, Window};
use tracing::{debug, info, warn};

use crate::config::{save_config, AppConfig};
use crate::core::{DeviceId, DisplayId, ScalingMode};

// ============================================================================
// Preview State
// ============================================================================

/// Camera preview state
#[derive(Debug, Clone, Default)]
pub enum PreviewState {
    /// No camera is selected
    #[default]
    NoCameraSelected,
    /// Camera selected but waiting for first frame
    WaitingForFrame,
    /// Actively receiving preview frames
    Capturing,
    /// Preview error occurred
    Error(String),
}

/// Preview frame data for updating the settings preview
#[derive(Debug, Clone)]
pub struct PreviewFrame {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
}

/// Preview configuration
pub struct PreviewConfig {
    /// Target preview width (will maintain aspect ratio)
    pub target_width: f32,
    /// Maximum preview height
    pub max_height: f32,
    /// Target framerate for preview (lower than main capture)
    pub target_fps: f32,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            target_width: 320.0,
            max_height: 240.0,
            target_fps: 15.0,
        }
    }
}

// ============================================================================
// Settings State
// ============================================================================

/// Camera device info for display in dropdown
#[derive(Debug, Clone)]
pub struct CameraInfo {
    /// Device identifier
    pub id: DeviceId,
    /// Human-readable name
    pub name: String,
    /// Available resolutions
    pub resolutions: Vec<(u32, u32)>,
    /// Available framerates
    pub framerates: Vec<f32>,
}

/// Display info for display in dropdown
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    /// Display identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Is this the primary display?
    pub is_primary: bool,
}

/// State for the settings window
pub struct SettingsWindow {
    /// Whether the window is visible
    visible: bool,
    /// Working copy of the configuration (staged changes)
    staged_config: AppConfig,
    /// Original config for cancel/revert
    original_config: AppConfig,
    /// Available cameras
    cameras: Vec<CameraInfo>,
    /// Currently selected camera index
    selected_camera_idx: usize,
    /// Available displays
    displays: Vec<DisplayInfo>,
    /// Currently selected display index
    selected_display_idx: usize,
    /// Available resolutions for current camera
    available_resolutions: Vec<(u32, u32)>,
    /// Selected resolution index
    selected_resolution_idx: usize,
    /// Available framerates for current camera
    available_framerates: Vec<f32>,
    /// Selected framerate index
    selected_framerate_idx: usize,
    /// Whether changes have been made
    has_changes: bool,
    /// Error message to display (if any)
    error_message: Option<String>,
    /// Success message to display (if any)
    success_message: Option<String>,
    /// Preview texture handle for displaying camera frames
    preview_texture: Option<TextureHandle>,
    /// Current preview state
    preview_state: PreviewState,
    /// Preview configuration
    preview_config: PreviewConfig,
    /// Last frame dimensions (for aspect ratio calculations)
    last_frame_size: Option<(u32, u32)>,
}

impl Default for SettingsWindow {
    fn default() -> Self {
        Self::new(AppConfig::default())
    }
}

impl SettingsWindow {
    /// Create a new settings window with the given configuration
    pub fn new(config: AppConfig) -> Self {
        Self {
            visible: false,
            staged_config: config.clone(),
            original_config: config,
            cameras: Vec::new(),
            selected_camera_idx: 0,
            displays: vec![DisplayInfo {
                id: "primary".into(),
                name: "Primary Display".into(),
                is_primary: true,
            }],
            selected_display_idx: 0,
            available_resolutions: vec![(1920, 1080), (1280, 720), (640, 480)],
            selected_resolution_idx: 0,
            available_framerates: vec![30.0, 60.0, 15.0],
            selected_framerate_idx: 0,
            has_changes: false,
            error_message: None,
            success_message: None,
            preview_texture: None,
            preview_state: PreviewState::NoCameraSelected,
            preview_config: PreviewConfig::default(),
            last_frame_size: None,
        }
    }

    /// Show the settings window
    pub fn show(&mut self) {
        self.visible = true;
        self.staged_config = self.original_config.clone();
        self.has_changes = false;
        self.error_message = None;
        self.success_message = None;
        // Set preview state based on camera selection
        self.preview_state = if self.staged_config.camera.device_id.is_some() {
            PreviewState::WaitingForFrame
        } else {
            PreviewState::NoCameraSelected
        };
        self.preview_texture = None;
        self.last_frame_size = None;
        debug!("Settings window opened");
    }

    /// Hide the settings window
    pub fn hide(&mut self) {
        self.visible = false;
        // Clear preview resources when window is hidden
        self.preview_texture = None;
        self.preview_state = PreviewState::NoCameraSelected;
        self.last_frame_size = None;
        debug!("Settings window closed");
    }

    /// Check if the window is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Update the configuration (called when config changes externally)
    pub fn update_config(&mut self, config: AppConfig) {
        self.original_config = config.clone();
        if !self.has_changes {
            self.staged_config = config;
        }
    }

    /// Update the list of available cameras
    pub fn update_cameras(&mut self, cameras: Vec<CameraInfo>) {
        // Find index of currently selected camera
        let current_id = self.staged_config.camera.device_id.as_ref();
        self.selected_camera_idx = cameras
            .iter()
            .position(|c| Some(&c.id) == current_id)
            .unwrap_or(0);

        self.cameras = cameras;

        // Update available resolutions/framerates for selected camera
        self.update_camera_capabilities();
    }

    /// Update the list of available displays
    pub fn update_displays(&mut self, displays: Vec<DisplayInfo>) {
        // Find index of currently selected display
        let current_id = self.staged_config.display.display_id.as_ref();
        self.selected_display_idx = displays
            .iter()
            .position(|d| {
                if let Some(id) = current_id {
                    d.id == id.0
                } else {
                    d.is_primary
                }
            })
            .unwrap_or(0);

        self.displays = displays;
    }

    /// Update the preview frame with new camera data
    ///
    /// Call this when a new frame is available from the capture system.
    /// The frame should be RGBA format (4 bytes per pixel).
    pub fn update_preview_frame(&mut self, ctx: &Context, frame: PreviewFrame) {
        if frame.data.len() != (frame.width * frame.height * 4) as usize {
            warn!(
                "Invalid preview frame size: expected {} bytes, got {}",
                frame.width * frame.height * 4,
                frame.data.len()
            );
            return;
        }

        // Create color image from frame data
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [frame.width as usize, frame.height as usize],
            &frame.data,
        );

        // Update or create texture
        match &mut self.preview_texture {
            Some(texture) => {
                texture.set(image, egui::TextureOptions::LINEAR);
            }
            None => {
                self.preview_texture = Some(ctx.load_texture(
                    "camera_preview",
                    image,
                    egui::TextureOptions::LINEAR,
                ));
            }
        }

        self.last_frame_size = Some((frame.width, frame.height));
        self.preview_state = PreviewState::Capturing;
    }

    /// Set the preview state (called when capture state changes)
    pub fn set_preview_state(&mut self, state: PreviewState) {
        self.preview_state = state;
        // Clear texture when not capturing
        if !matches!(self.preview_state, PreviewState::Capturing) {
            self.preview_texture = None;
            self.last_frame_size = None;
        }
    }

    /// Get the preview configuration (for the capture system)
    pub fn preview_config(&self) -> &PreviewConfig {
        &self.preview_config
    }

    /// Check if the preview should be active (window visible and camera selected)
    pub fn should_capture_preview(&self) -> bool {
        self.visible && self.staged_config.camera.device_id.is_some()
    }

    /// Update available resolutions and framerates for the selected camera
    fn update_camera_capabilities(&mut self) {
        if let Some(camera) = self.cameras.get(self.selected_camera_idx) {
            self.available_resolutions = if camera.resolutions.is_empty() {
                vec![(1920, 1080), (1280, 720), (640, 480)]
            } else {
                camera.resolutions.clone()
            };

            self.available_framerates = if camera.framerates.is_empty() {
                vec![30.0, 60.0, 15.0]
            } else {
                camera.framerates.clone()
            };

            // Find current resolution/framerate indices
            let (w, h) = (
                self.staged_config.camera.width,
                self.staged_config.camera.height,
            );
            self.selected_resolution_idx = self
                .available_resolutions
                .iter()
                .position(|&(rw, rh)| rw == w && rh == h)
                .unwrap_or(0);

            let fps = self.staged_config.camera.framerate;
            self.selected_framerate_idx = self
                .available_framerates
                .iter()
                .position(|&f| (f - fps).abs() < 0.1)
                .unwrap_or(0);
        }
    }

    /// Draw the settings window
    ///
    /// Returns `Some(config)` if the user clicked Apply or OK,
    /// indicating the config should be applied.
    pub fn draw(&mut self, ctx: &Context) -> Option<AppConfig> {
        if !self.visible {
            return None;
        }

        let mut result = None;
        let mut close_window = false;

        Window::new("Settings")
            .id(Id::new("micround_settings"))
            .resizable(false)
            .collapsible(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                // Error/success messages
                if let Some(msg) = &self.error_message {
                    ui.colored_label(egui::Color32::RED, msg);
                    ui.add_space(8.0);
                }
                if let Some(msg) = &self.success_message {
                    ui.colored_label(egui::Color32::GREEN, msg);
                    ui.add_space(8.0);
                }

                // Camera section
                self.draw_camera_section(ui);
                ui.add_space(12.0);

                // Display section
                self.draw_display_section(ui);
                ui.add_space(12.0);

                // Startup section
                self.draw_startup_section(ui);
                ui.add_space(16.0);

                // Buttons
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // OK button
                        if ui.button("OK").clicked() {
                            if let Some(config) = self.try_apply() {
                                result = Some(config);
                                close_window = true;
                            }
                        }

                        // Apply button
                        let apply_enabled = self.has_changes;
                        if ui.add_enabled(apply_enabled, egui::Button::new("Apply")).clicked() {
                            if let Some(config) = self.try_apply() {
                                result = Some(config);
                            }
                        }

                        // Cancel button
                        if ui.button("Cancel").clicked() {
                            self.staged_config = self.original_config.clone();
                            self.has_changes = false;
                            close_window = true;
                        }
                    });
                });
            });

        if close_window {
            self.hide();
        }

        result
    }

    /// Draw the camera configuration section
    fn draw_camera_section(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.heading("Camera");
            ui.add_space(4.0);

            // Device dropdown with refresh button
            ui.horizontal(|ui| {
                ui.label("Device:");

                let camera_names: Vec<String> = if self.cameras.is_empty() {
                    vec!["No cameras found".into()]
                } else {
                    self.cameras.iter().map(|c| c.name.clone()).collect()
                };

                let selected = if self.cameras.is_empty() {
                    0
                } else {
                    self.selected_camera_idx.min(camera_names.len() - 1)
                };

                let selected_name = &camera_names[selected];

                egui::ComboBox::from_id_source("camera_device")
                    .selected_text(selected_name)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for (i, name) in camera_names.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_camera_idx, i, name).clicked() {
                                self.on_camera_selected(i);
                            }
                        }
                    });

                if ui.button("Refresh").clicked() {
                    // TODO: Emit event to refresh camera list
                    info!("Camera refresh requested");
                }
            });

            // Resolution dropdown
            ui.horizontal(|ui| {
                ui.label("Resolution:");

                let res_strings: Vec<String> = self
                    .available_resolutions
                    .iter()
                    .map(|(w, h)| format!("{}x{}", w, h))
                    .collect();

                let selected = self.selected_resolution_idx.min(res_strings.len().saturating_sub(1));
                let selected_res = res_strings.get(selected).cloned().unwrap_or_default();

                egui::ComboBox::from_id_source("camera_resolution")
                    .selected_text(&selected_res)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for (i, res) in res_strings.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_resolution_idx, i, res).clicked() {
                                self.on_resolution_selected(i);
                            }
                        }
                    });
            });

            // Framerate dropdown
            ui.horizontal(|ui| {
                ui.label("Framerate:");

                let fps_strings: Vec<String> = self
                    .available_framerates
                    .iter()
                    .map(|f| format!("{:.0} fps", f))
                    .collect();

                let selected = self.selected_framerate_idx.min(fps_strings.len().saturating_sub(1));
                let selected_fps = fps_strings.get(selected).cloned().unwrap_or_default();

                egui::ComboBox::from_id_source("camera_framerate")
                    .selected_text(&selected_fps)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for (i, fps) in fps_strings.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_framerate_idx, i, fps).clicked() {
                                self.on_framerate_selected(i);
                            }
                        }
                    });
            });

            // Live preview
            ui.add_space(8.0);
            self.draw_preview(ui);
        });
    }

    /// Draw the camera preview
    fn draw_preview(&self, ui: &mut Ui) {
        // Calculate preview size maintaining aspect ratio
        let target_width = self.preview_config.target_width;
        let max_height = self.preview_config.max_height;

        let (preview_width, preview_height) = if let Some((frame_w, frame_h)) = self.last_frame_size
        {
            // Calculate size maintaining aspect ratio
            let aspect = frame_w as f32 / frame_h as f32;
            let height = (target_width / aspect).min(max_height);
            let width = height * aspect;
            (width, height)
        } else {
            // Default 16:9 aspect ratio
            (target_width, target_width * 9.0 / 16.0)
        };

        ui.group(|ui| {
            ui.set_min_size(egui::vec2(preview_width, preview_height));

            match &self.preview_state {
                PreviewState::NoCameraSelected => {
                    ui.centered_and_justified(|ui| {
                        ui.label("Select a camera to see preview");
                    });
                }
                PreviewState::WaitingForFrame => {
                    ui.centered_and_justified(|ui| {
                        ui.spinner();
                        ui.label("Starting camera...");
                    });
                }
                PreviewState::Error(msg) => {
                    ui.centered_and_justified(|ui| {
                        ui.colored_label(egui::Color32::RED, format!("Error: {}", msg));
                    });
                }
                PreviewState::Capturing => {
                    if let Some(texture) = &self.preview_texture {
                        // Draw the preview image
                        let size = egui::vec2(preview_width, preview_height);
                        ui.image((texture.id(), size));
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.spinner();
                            ui.label("Loading...");
                        });
                    }
                }
            }
        });

        // Show transform indicators below preview
        self.draw_transform_indicators(ui);
    }

    /// Draw transform indicators showing current settings
    fn draw_transform_indicators(&self, ui: &mut Ui) {
        let display = &self.staged_config.display;

        // Only show if transforms are applied
        let has_transforms = display.rotation != 0
            || display.flip_horizontal
            || display.flip_vertical
            || display.scaling_mode != ScalingMode::Fit;

        if has_transforms {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.small("Transforms:");

                if display.rotation != 0 {
                    ui.small(format!("↻{}°", display.rotation));
                }
                if display.flip_horizontal {
                    ui.small("↔H");
                }
                if display.flip_vertical {
                    ui.small("↕V");
                }
                match display.scaling_mode {
                    ScalingMode::Fit => {}
                    ScalingMode::Fill => { ui.small("[Fill]"); }
                    ScalingMode::Stretch => { ui.small("[Stretch]"); }
                    ScalingMode::Center => { ui.small("[Center]"); }
                }
            });
        }
    }

    /// Draw the display configuration section
    fn draw_display_section(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.heading("Display");
            ui.add_space(4.0);

            // Target display dropdown
            ui.horizontal(|ui| {
                ui.label("Target:");

                let display_names: Vec<String> = self.displays.iter().map(|d| d.name.clone()).collect();
                let selected = self.selected_display_idx.min(display_names.len().saturating_sub(1));
                let selected_name = display_names.get(selected).cloned().unwrap_or_default();

                egui::ComboBox::from_id_source("target_display")
                    .selected_text(&selected_name)
                    .show_ui(ui, |ui: &mut egui::Ui| {
                        for (i, name) in display_names.iter().enumerate() {
                            if ui.selectable_value(&mut self.selected_display_idx, i, name).clicked() {
                                self.on_display_selected(i);
                            }
                        }
                    });
            });

            // Scaling mode radio buttons
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Scaling:");

                let mut mode = self.staged_config.display.scaling_mode;

                if ui.radio_value(&mut mode, ScalingMode::Fit, "Fit").changed()
                    || ui.radio_value(&mut mode, ScalingMode::Fill, "Fill").changed()
                    || ui.radio_value(&mut mode, ScalingMode::Stretch, "Stretch").changed()
                    || ui.radio_value(&mut mode, ScalingMode::Center, "Center").changed()
                {
                    self.staged_config.display.scaling_mode = mode;
                    self.mark_changed();
                }
            });

            // Rotation radio buttons
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Rotation:");

                let mut rotation = self.staged_config.display.rotation;

                if ui.radio_value(&mut rotation, 0, "0°").changed()
                    || ui.radio_value(&mut rotation, 90, "90°").changed()
                    || ui.radio_value(&mut rotation, 180, "180°").changed()
                    || ui.radio_value(&mut rotation, 270, "270°").changed()
                {
                    self.staged_config.display.rotation = rotation;
                    self.mark_changed();
                }
            });

            // Flip checkboxes
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label("Flip:");

                if ui.checkbox(&mut self.staged_config.display.flip_horizontal, "Horizontal").changed() {
                    self.mark_changed();
                }
                if ui.checkbox(&mut self.staged_config.display.flip_vertical, "Vertical").changed() {
                    self.mark_changed();
                }
            });
        });
    }

    /// Draw the startup configuration section
    fn draw_startup_section(&mut self, ui: &mut Ui) {
        ui.group(|ui| {
            ui.heading("Startup");
            ui.add_space(4.0);

            if ui.checkbox(&mut self.staged_config.startup.launch_at_login, "Launch at login").changed() {
                self.mark_changed();
            }

            if ui.checkbox(&mut self.staged_config.startup.auto_start_feed, "Auto-start feed").changed() {
                self.mark_changed();
            }

            if ui.checkbox(&mut self.staged_config.startup.minimize_on_start, "Minimize to tray on startup").changed() {
                self.mark_changed();
            }
        });
    }

    /// Handle camera selection change
    fn on_camera_selected(&mut self, idx: usize) {
        self.selected_camera_idx = idx;
        if let Some(camera) = self.cameras.get(idx) {
            self.staged_config.camera.device_id = Some(camera.id.clone());
            self.update_camera_capabilities();
            self.mark_changed();
            // Reset preview state for new camera
            self.preview_state = PreviewState::WaitingForFrame;
            self.preview_texture = None;
            self.last_frame_size = None;
        }
    }

    /// Handle resolution selection change
    fn on_resolution_selected(&mut self, idx: usize) {
        self.selected_resolution_idx = idx;
        if let Some(&(w, h)) = self.available_resolutions.get(idx) {
            self.staged_config.camera.width = w;
            self.staged_config.camera.height = h;
            self.mark_changed();
        }
    }

    /// Handle framerate selection change
    fn on_framerate_selected(&mut self, idx: usize) {
        self.selected_framerate_idx = idx;
        if let Some(&fps) = self.available_framerates.get(idx) {
            self.staged_config.camera.framerate = fps;
            self.mark_changed();
        }
    }

    /// Handle display selection change
    fn on_display_selected(&mut self, idx: usize) {
        self.selected_display_idx = idx;
        if let Some(display) = self.displays.get(idx) {
            self.staged_config.display.display_id = if display.is_primary {
                None
            } else {
                Some(DisplayId(display.id.clone()))
            };
            self.mark_changed();
        }
    }

    /// Mark config as changed
    fn mark_changed(&mut self) {
        self.has_changes = true;
        self.success_message = None;
    }

    /// Try to apply the staged configuration
    fn try_apply(&mut self) -> Option<AppConfig> {
        // Validate
        let errors = self.staged_config.validate();
        if !errors.is_empty() {
            let error_msgs: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
            self.error_message = Some(format!("Invalid settings: {}", error_msgs.join(", ")));
            return None;
        }

        // Save to disk
        if let Err(e) = save_config(&self.staged_config) {
            self.error_message = Some(format!("Failed to save settings: {}", e));
            return None;
        }

        // Update original config
        self.original_config = self.staged_config.clone();
        self.has_changes = false;
        self.error_message = None;
        self.success_message = Some("Settings saved successfully".into());

        info!("Settings applied and saved");
        Some(self.staged_config.clone())
    }

    /// Get the current staged configuration
    pub fn staged_config(&self) -> &AppConfig {
        &self.staged_config
    }

    /// Check if there are unsaved changes
    pub fn has_unsaved_changes(&self) -> bool {
        self.has_changes
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_window_creation() {
        let window = SettingsWindow::new(AppConfig::default());
        assert!(!window.is_visible());
        assert!(!window.has_unsaved_changes());
    }

    #[test]
    fn test_show_hide() {
        let mut window = SettingsWindow::new(AppConfig::default());

        window.show();
        assert!(window.is_visible());

        window.hide();
        assert!(!window.is_visible());
    }

    #[test]
    fn test_config_update() {
        let mut window = SettingsWindow::new(AppConfig::default());
        let mut new_config = AppConfig::default();
        new_config.camera.width = 1280;

        window.update_config(new_config.clone());
        assert_eq!(window.staged_config().camera.width, 1280);
    }

    #[test]
    fn test_camera_list_update() {
        let mut window = SettingsWindow::new(AppConfig::default());

        let cameras = vec![
            CameraInfo {
                id: DeviceId("camera1".to_string()),
                name: "Camera 1".into(),
                resolutions: vec![(1920, 1080), (1280, 720)],
                framerates: vec![30.0, 60.0],
            },
            CameraInfo {
                id: DeviceId("camera2".to_string()),
                name: "Camera 2".into(),
                resolutions: vec![(640, 480)],
                framerates: vec![15.0],
            },
        ];

        window.update_cameras(cameras);
        assert_eq!(window.cameras.len(), 2);
    }

    #[test]
    fn test_display_list_update() {
        let mut window = SettingsWindow::new(AppConfig::default());

        let displays = vec![
            DisplayInfo {
                id: "display1".into(),
                name: "Primary".into(),
                is_primary: true,
            },
            DisplayInfo {
                id: "display2".into(),
                name: "Secondary".into(),
                is_primary: false,
            },
        ];

        window.update_displays(displays);
        assert_eq!(window.displays.len(), 2);
    }

    #[test]
    fn test_mark_changed() {
        let mut window = SettingsWindow::new(AppConfig::default());
        assert!(!window.has_unsaved_changes());

        window.mark_changed();
        assert!(window.has_unsaved_changes());
    }

    #[test]
    fn test_resolution_selection() {
        let mut window = SettingsWindow::new(AppConfig::default());
        window.available_resolutions = vec![(1920, 1080), (1280, 720)];

        window.on_resolution_selected(1);
        assert_eq!(window.staged_config.camera.width, 1280);
        assert_eq!(window.staged_config.camera.height, 720);
        assert!(window.has_unsaved_changes());
    }

    #[test]
    fn test_framerate_selection() {
        let mut window = SettingsWindow::new(AppConfig::default());
        window.available_framerates = vec![30.0, 60.0];

        window.on_framerate_selected(1);
        assert_eq!(window.staged_config.camera.framerate, 60.0);
        assert!(window.has_unsaved_changes());
    }

    #[test]
    fn test_scaling_mode_preserved() {
        let mut config = AppConfig::default();
        config.display.scaling_mode = ScalingMode::Fit;

        let window = SettingsWindow::new(config);
        assert_eq!(window.staged_config.display.scaling_mode, ScalingMode::Fit);
    }

    #[test]
    fn test_preview_state_default() {
        let window = SettingsWindow::new(AppConfig::default());
        assert!(matches!(window.preview_state, PreviewState::NoCameraSelected));
        assert!(window.preview_texture.is_none());
    }

    #[test]
    fn test_preview_state_on_show() {
        let mut window = SettingsWindow::new(AppConfig::default());
        window.show();
        // No camera selected, should be NoCameraSelected
        assert!(matches!(window.preview_state, PreviewState::NoCameraSelected));

        // With camera selected
        let mut config = AppConfig::default();
        config.camera.device_id = Some(DeviceId("camera1".into()));
        let mut window2 = SettingsWindow::new(config);
        window2.show();
        // Camera selected, should be WaitingForFrame
        assert!(matches!(window2.preview_state, PreviewState::WaitingForFrame));
    }

    #[test]
    fn test_preview_state_on_hide() {
        let mut window = SettingsWindow::new(AppConfig::default());
        window.preview_state = PreviewState::Capturing;
        window.hide();
        // Should reset to NoCameraSelected when hidden
        assert!(matches!(window.preview_state, PreviewState::NoCameraSelected));
        assert!(window.preview_texture.is_none());
    }

    #[test]
    fn test_preview_state_on_camera_selection() {
        let mut window = SettingsWindow::new(AppConfig::default());
        window.cameras = vec![CameraInfo {
            id: DeviceId("camera1".into()),
            name: "Camera 1".into(),
            resolutions: vec![(1920, 1080)],
            framerates: vec![30.0],
        }];
        window.preview_state = PreviewState::Capturing;

        window.on_camera_selected(0);

        // Selecting a camera should reset to WaitingForFrame
        assert!(matches!(window.preview_state, PreviewState::WaitingForFrame));
        assert!(window.preview_texture.is_none());
    }

    #[test]
    fn test_should_capture_preview() {
        let mut window = SettingsWindow::new(AppConfig::default());

        // Not visible, no camera - should not capture
        assert!(!window.should_capture_preview());

        // Visible, no camera - should not capture
        window.visible = true;
        assert!(!window.should_capture_preview());

        // Visible, camera selected - should capture
        window.staged_config.camera.device_id = Some(DeviceId("camera1".into()));
        assert!(window.should_capture_preview());

        // Not visible, camera selected - should not capture
        window.visible = false;
        assert!(!window.should_capture_preview());
    }

    #[test]
    fn test_set_preview_state() {
        let mut window = SettingsWindow::new(AppConfig::default());

        window.set_preview_state(PreviewState::WaitingForFrame);
        assert!(matches!(window.preview_state, PreviewState::WaitingForFrame));

        window.set_preview_state(PreviewState::Error("Test error".into()));
        assert!(matches!(window.preview_state, PreviewState::Error(_)));

        // Texture should be cleared when not capturing
        window.last_frame_size = Some((100, 100));
        window.set_preview_state(PreviewState::NoCameraSelected);
        assert!(window.last_frame_size.is_none());
    }

    #[test]
    fn test_preview_config_defaults() {
        let config = PreviewConfig::default();
        assert_eq!(config.target_width, 320.0);
        assert_eq!(config.max_height, 240.0);
        assert_eq!(config.target_fps, 15.0);
    }

    #[test]
    fn test_preview_frame_creation() {
        let frame = PreviewFrame {
            data: vec![0u8; 640 * 480 * 4],
            width: 640,
            height: 480,
        };
        assert_eq!(frame.data.len(), (frame.width * frame.height * 4) as usize);
    }
}
