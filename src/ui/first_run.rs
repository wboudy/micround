//! First-run user experience
//!
//! Provides a guided setup wizard for first-time users. The first impression
//! determines whether users continue or abandon the app.
//!
//! # Scenarios Handled
//!
//! - **Happy Path**: Camera detected, quick setup to wallpaper
//! - **No Camera**: Friendly guidance to connect a camera
//! - **Permission Denied**: Explains why access is needed (macOS)
//! - **Multiple Cameras**: Helps user select the right one
//!
//! # Design Principles
//!
//! - Never show an empty or confusing state
//! - Always tell user what to do next
//! - Preview camera before committing to wallpaper
//! - Build trust: explain privacy (nothing recorded/sent)
//! - Make the happy path one click after camera selection

use egui::{Align2, Color32, Context, RichText, Ui, Vec2, Window};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::core::DeviceId;

#[cfg(target_os = "macos")]
use crate::platform::{
    create_permission_handler, CameraPermission, PermissionHandler,
};

// ============================================================================
// First-Run State
// ============================================================================

/// Current step in the first-run wizard
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstRunStep {
    /// Welcome screen with app introduction
    Welcome,
    /// Checking for cameras and permissions
    Checking,
    /// No camera detected - guide to connect
    NoCamera,
    /// Permission denied - guide to settings
    PermissionDenied,
    /// Select camera from multiple options
    SelectCamera,
    /// Camera ready - show preview and setup
    CameraReady,
    /// Setup complete
    Complete,
}

/// Camera info for first-run display
#[derive(Debug, Clone)]
pub struct FirstRunCameraInfo {
    /// Device ID
    pub id: DeviceId,
    /// Human-readable name
    pub name: String,
    /// Whether this looks like a microscope (by name heuristics)
    pub is_likely_microscope: bool,
}

impl FirstRunCameraInfo {
    /// Create camera info with microscope detection
    pub fn new(id: DeviceId, name: String) -> Self {
        let name_lower = name.to_lowercase();
        let is_likely_microscope = name_lower.contains("microscope")
            || name_lower.contains("usb")
            || name_lower.contains("endoscope")
            || name_lower.contains("borescope")
            || !name_lower.contains("facetime")
            && !name_lower.contains("isight")
            && !name_lower.contains("built-in");

        Self {
            id,
            name,
            is_likely_microscope,
        }
    }
}

/// State for the first-run experience
pub struct FirstRunWizard {
    /// Current step
    step: FirstRunStep,
    /// Whether the wizard is active
    active: bool,
    /// Detected cameras
    cameras: Vec<FirstRunCameraInfo>,
    /// Selected camera index
    selected_camera_idx: usize,
    /// Error message (if any)
    error_message: Option<String>,
    /// Whether we've checked for first-run status
    first_run_checked: bool,
    /// Path to the first-run marker file
    marker_path: PathBuf,
    /// Permission handler for macOS
    #[cfg(target_os = "macos")]
    permission_handler: std::sync::Arc<dyn PermissionHandler>,
}

impl Default for FirstRunWizard {
    fn default() -> Self {
        Self::new()
    }
}

impl FirstRunWizard {
    /// Create a new first-run wizard
    pub fn new() -> Self {
        let marker_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("micround")
            .join(".first_run_complete");

        Self {
            step: FirstRunStep::Welcome,
            active: false,
            cameras: Vec::new(),
            selected_camera_idx: 0,
            error_message: None,
            first_run_checked: false,
            marker_path,
            #[cfg(target_os = "macos")]
            permission_handler: create_permission_handler(),
        }
    }

    /// Check if this is the first run
    pub fn is_first_run(&mut self) -> bool {
        if self.first_run_checked {
            return self.active;
        }

        self.first_run_checked = true;

        // Check if marker file exists
        if self.marker_path.exists() {
            info!("First-run marker found, skipping wizard");
            self.active = false;
            return false;
        }

        info!("No first-run marker found, starting setup wizard");
        self.active = true;
        true
    }

    /// Start the wizard (can be called manually from settings)
    pub fn start(&mut self) {
        self.active = true;
        self.step = FirstRunStep::Welcome;
        self.error_message = None;
        info!("First-run wizard started");
    }

    /// Mark first-run as complete
    pub fn mark_complete(&mut self) {
        self.active = false;
        self.step = FirstRunStep::Complete;

        // Create marker file
        if let Some(parent) = self.marker_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&self.marker_path, "complete") {
            warn!("Failed to write first-run marker: {}", e);
        }

        info!("First-run wizard completed");
    }

    /// Check if wizard is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Skip the wizard (user dismissed it)
    pub fn skip(&mut self) {
        self.mark_complete();
    }

    /// Get current step
    pub fn current_step(&self) -> FirstRunStep {
        self.step
    }

    /// Get selected camera (if any)
    pub fn selected_camera(&self) -> Option<&FirstRunCameraInfo> {
        self.cameras.get(self.selected_camera_idx)
    }

    /// Update camera list
    pub fn update_cameras(&mut self, cameras: Vec<FirstRunCameraInfo>) {
        self.cameras = cameras;

        // Auto-select likely microscope
        if let Some(idx) = self.cameras.iter().position(|c| c.is_likely_microscope) {
            self.selected_camera_idx = idx;
        }
    }

    /// Advance to next step based on current state
    pub fn advance(&mut self) {
        match self.step {
            FirstRunStep::Welcome => {
                self.step = FirstRunStep::Checking;
            }
            FirstRunStep::Checking => {
                // This will be called after camera detection
            }
            FirstRunStep::NoCamera => {
                // Retry detection
                self.step = FirstRunStep::Checking;
            }
            FirstRunStep::PermissionDenied => {
                // Retry permission check
                self.step = FirstRunStep::Checking;
            }
            FirstRunStep::SelectCamera => {
                self.step = FirstRunStep::CameraReady;
            }
            FirstRunStep::CameraReady => {
                self.mark_complete();
            }
            FirstRunStep::Complete => {}
        }
    }

    /// Handle camera detection result
    pub fn on_cameras_detected(&mut self, cameras: Vec<FirstRunCameraInfo>) {
        self.cameras = cameras;

        if self.cameras.is_empty() {
            self.step = FirstRunStep::NoCamera;
        } else if self.cameras.len() == 1 {
            self.selected_camera_idx = 0;
            self.step = FirstRunStep::CameraReady;
        } else {
            // Auto-select likely microscope
            if let Some(idx) = self.cameras.iter().position(|c| c.is_likely_microscope) {
                self.selected_camera_idx = idx;
            }
            self.step = FirstRunStep::SelectCamera;
        }
    }

    /// Handle permission status (macOS)
    #[cfg(target_os = "macos")]
    pub fn on_permission_status(&mut self, status: CameraPermission) {
        match status {
            CameraPermission::Authorized | CameraPermission::NotRequired => {
                // Continue with camera detection
            }
            CameraPermission::Denied | CameraPermission::Restricted => {
                self.step = FirstRunStep::PermissionDenied;
            }
            CameraPermission::NotDetermined => {
                // Request permission
            }
        }
    }

    /// Open system settings for camera permission
    #[cfg(target_os = "macos")]
    pub fn open_camera_settings(&self) {
        let _ = self.permission_handler.open_camera_settings();
    }

    /// Render the wizard UI
    pub fn render(&mut self, ctx: &Context) -> Option<AppConfig> {
        if !self.active {
            return None;
        }

        let mut result = None;

        Window::new("Welcome to Micround")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(500.0, 400.0))
            .show(ctx, |ui| {
                result = self.render_content(ui);
            });

        result
    }

    /// Render wizard content based on current step
    fn render_content(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        match self.step {
            FirstRunStep::Welcome => self.render_welcome(ui),
            FirstRunStep::Checking => self.render_checking(ui),
            FirstRunStep::NoCamera => self.render_no_camera(ui),
            FirstRunStep::PermissionDenied => self.render_permission_denied(ui),
            FirstRunStep::SelectCamera => self.render_select_camera(ui),
            FirstRunStep::CameraReady => self.render_camera_ready(ui),
            FirstRunStep::Complete => None,
        }
    }

    /// Render welcome step
    fn render_welcome(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.heading(RichText::new("Welcome to Micround").size(24.0));

            ui.add_space(20.0);

            ui.label(
                RichText::new("Display your USB microscope feed as your desktop wallpaper.")
                    .size(16.0),
            );

            ui.add_space(30.0);

            // Privacy assurance
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Your privacy is protected:").strong());
                });
                ui.label("  \u{2022} Camera feed stays on your computer");
                ui.label("  \u{2022} Nothing is recorded or sent anywhere");
                ui.label("  \u{2022} No internet connection required");
            });

            ui.add_space(30.0);

            // Get started button
            if ui
                .button(RichText::new("Get Started").size(18.0))
                .clicked()
            {
                self.advance();
            }

            ui.add_space(10.0);

            // Skip option
            if ui.small_button("Skip setup").clicked() {
                self.skip();
            }
        });

        None
    }

    /// Render checking step
    fn render_checking(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        ui.vertical_centered(|ui| {
            ui.add_space(50.0);

            ui.heading("Checking for cameras...");

            ui.add_space(20.0);

            ui.spinner();

            ui.add_space(20.0);

            ui.label("Looking for connected USB microscopes");
        });

        // NOTE: Camera detection is async - the caller should call on_cameras_detected()
        // when detection completes

        None
    }

    /// Render no camera step
    fn render_no_camera(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);

            ui.heading(
                RichText::new("No Camera Detected")
                    .size(22.0)
                    .color(Color32::from_rgb(200, 100, 0)),
            );

            ui.add_space(20.0);

            ui.label(RichText::new("Please connect your USB microscope.").size(16.0));

            ui.add_space(30.0);

            // Instructions
            ui.group(|ui| {
                ui.label(RichText::new("What to try:").strong());
                ui.label("  1. Connect your USB microscope to any USB port");
                ui.label("  2. Wait a few seconds for it to be recognized");
                ui.label("  3. Click 'Check Again' below");
            });

            ui.add_space(30.0);

            // Refresh button
            if ui
                .button(RichText::new("Check Again").size(16.0))
                .clicked()
            {
                self.advance();
            }

            ui.add_space(10.0);

            // Skip option
            if ui.small_button("Continue without camera").clicked() {
                self.skip();
            }
        });

        None
    }

    /// Render permission denied step (macOS)
    fn render_permission_denied(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        ui.vertical_centered(|ui| {
            ui.add_space(30.0);

            ui.heading(
                RichText::new("Camera Permission Needed")
                    .size(22.0)
                    .color(Color32::from_rgb(200, 100, 0)),
            );

            ui.add_space(20.0);

            ui.label(
                RichText::new("Micround needs camera access to display your microscope feed.")
                    .size(16.0),
            );

            ui.add_space(30.0);

            // Instructions
            ui.group(|ui| {
                ui.label(RichText::new("How to enable:").strong());
                ui.label("  1. Click 'Open Settings' below");
                ui.label("  2. Find 'Micround' in the list");
                ui.label("  3. Enable camera access");
                ui.label("  4. Return here and click 'Check Again'");
            });

            ui.add_space(30.0);

            ui.horizontal(|ui| {
                #[cfg(target_os = "macos")]
                if ui
                    .button(RichText::new("Open Settings").size(16.0))
                    .clicked()
                {
                    self.open_camera_settings();
                }

                #[cfg(not(target_os = "macos"))]
                ui.label("(Settings not applicable on this platform)");
            });

            ui.add_space(10.0);

            if ui
                .button(RichText::new("Check Again").size(14.0))
                .clicked()
            {
                self.advance();
            }

            ui.add_space(10.0);

            if ui.small_button("Skip").clicked() {
                self.skip();
            }
        });

        None
    }

    /// Render camera selection step
    fn render_select_camera(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.heading("Select Your Camera");

            ui.add_space(10.0);

            ui.label("Multiple cameras detected. Please select your microscope:");

            ui.add_space(20.0);
        });

        // Camera list
        for (idx, camera) in self.cameras.iter().enumerate() {
            let is_selected = idx == self.selected_camera_idx;
            let text = if camera.is_likely_microscope {
                format!("{} (recommended)", camera.name)
            } else {
                camera.name.clone()
            };

            if ui
                .selectable_label(is_selected, RichText::new(&text).size(14.0))
                .clicked()
            {
                self.selected_camera_idx = idx;
            }
        }

        ui.add_space(20.0);

        ui.vertical_centered(|ui| {
            if ui
                .button(RichText::new("Continue").size(16.0))
                .clicked()
            {
                self.advance();
            }
        });

        None
    }

    /// Render camera ready step
    fn render_camera_ready(&mut self, ui: &mut Ui) -> Option<AppConfig> {
        let mut result = None;

        ui.vertical_centered(|ui| {
            ui.add_space(20.0);

            ui.heading(
                RichText::new("Camera Ready!")
                    .size(22.0)
                    .color(Color32::from_rgb(50, 150, 50)),
            );

            ui.add_space(10.0);

            if let Some(camera) = self.selected_camera() {
                ui.label(format!("Selected: {}", camera.name));
            }

            ui.add_space(20.0);

            // Preview placeholder
            ui.group(|ui| {
                ui.set_min_size(Vec2::new(320.0, 180.0));
                ui.vertical_centered(|ui| {
                    ui.add_space(70.0);
                    ui.label(RichText::new("Camera Preview").italics());
                    ui.label("(Preview will appear when feed starts)");
                });
            });

            ui.add_space(20.0);

            // Set as wallpaper button
            if ui
                .button(RichText::new("Set as Wallpaper").size(18.0))
                .clicked()
            {
                // Create config with selected camera
                if let Some(camera) = self.selected_camera() {
                    let mut config = AppConfig::default();
                    config.camera.device_id = Some(camera.id.clone());
                    config.startup.auto_start_feed = true;
                    result = Some(config);
                }
                self.advance();
            }

            ui.add_space(10.0);

            // Open settings option
            if ui.small_button("Open Settings Instead").clicked() {
                self.skip();
            }
        });

        result
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_creation() {
        let wizard = FirstRunWizard::new();
        assert!(!wizard.is_active());
        assert_eq!(wizard.current_step(), FirstRunStep::Welcome);
    }

    #[test]
    fn test_wizard_start() {
        let mut wizard = FirstRunWizard::new();
        wizard.start();
        assert!(wizard.is_active());
        assert_eq!(wizard.current_step(), FirstRunStep::Welcome);
    }

    #[test]
    fn test_wizard_skip() {
        let mut wizard = FirstRunWizard::new();
        wizard.start();
        wizard.skip();
        assert!(!wizard.is_active());
        assert_eq!(wizard.current_step(), FirstRunStep::Complete);
    }

    #[test]
    fn test_camera_detection_single() {
        let mut wizard = FirstRunWizard::new();
        wizard.start();

        let cameras = vec![FirstRunCameraInfo::new(
            DeviceId("camera1".into()),
            "USB Microscope".into(),
        )];
        wizard.on_cameras_detected(cameras);

        assert_eq!(wizard.current_step(), FirstRunStep::CameraReady);
        assert!(wizard.selected_camera().is_some());
    }

    #[test]
    fn test_camera_detection_multiple() {
        let mut wizard = FirstRunWizard::new();
        wizard.start();

        let cameras = vec![
            FirstRunCameraInfo::new(DeviceId("cam1".into()), "FaceTime HD".into()),
            FirstRunCameraInfo::new(DeviceId("cam2".into()), "USB Microscope".into()),
        ];
        wizard.on_cameras_detected(cameras);

        assert_eq!(wizard.current_step(), FirstRunStep::SelectCamera);
        // Should auto-select microscope
        assert_eq!(wizard.selected_camera_idx, 1);
    }

    #[test]
    fn test_camera_detection_none() {
        let mut wizard = FirstRunWizard::new();
        wizard.start();
        wizard.on_cameras_detected(vec![]);

        assert_eq!(wizard.current_step(), FirstRunStep::NoCamera);
    }

    #[test]
    fn test_microscope_detection() {
        let microscope = FirstRunCameraInfo::new(DeviceId("1".into()), "USB Microscope".into());
        assert!(microscope.is_likely_microscope);

        let facetime = FirstRunCameraInfo::new(DeviceId("2".into()), "FaceTime HD Camera".into());
        assert!(!facetime.is_likely_microscope);

        let generic = FirstRunCameraInfo::new(DeviceId("3".into()), "USB Camera".into());
        assert!(generic.is_likely_microscope);
    }
}
