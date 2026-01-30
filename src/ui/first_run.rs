//! First-run user experience
//!
//! Manages the welcome flow when the user launches Micround for the first time.
//! Designed to be UI-framework agnostic - provides state and messages that can
//! be rendered by egui, a web UI, or even a CLI.
//!
//! # Design Principles
//!
//! 1. Never show an empty or confusing state
//! 2. Always tell user what to do next
//! 3. Preview camera before committing to wallpaper
//! 4. Build trust by explaining privacy guarantees
//!
//! # State Machine
//!
//! ```text
//! ┌─────────────┐
//! │   Welcome   │──────────────────────────────┐
//! └──────┬──────┘                              │
//!        │ check cameras                       │
//!        ▼                                     │
//! ┌─────────────────┐                          │
//! │ Detecting       │                          │
//! │ Cameras...      │                          │
//! └────────┬────────┘                          │
//!          │                                   │
//!    ┌─────┴─────┬─────────────┐               │
//!    │           │             │               │
//!    ▼           ▼             ▼               │
//! ┌──────┐  ┌─────────┐  ┌───────────┐        │
//! │ No   │  │ Single  │  │ Multiple  │        │
//! │Camera│  │ Camera  │  │ Cameras   │        │
//! └──┬───┘  └────┬────┘  └─────┬─────┘        │
//!    │           │             │               │
//!    │           ▼             ▼               │
//!    │      ┌─────────┐  ┌───────────┐        │
//!    │      │ Preview │  │  Select   │        │
//!    │      │         │  │  Camera   │        │
//!    │      └────┬────┘  └─────┬─────┘        │
//!    │           │             │               │
//!    │           └──────┬──────┘               │
//!    │                  ▼                      │
//!    │           ┌─────────────┐               │
//!    │           │  Confirm    │               │
//!    │           │  Setup      │               │
//!    │           └──────┬──────┘               │
//!    │                  │                      │
//!    │                  ▼                      │
//!    │           ┌─────────────┐               │
//!    │           │   Success   │               │
//!    │           └─────────────┘               │
//!    │                                         │
//!    └─────────────────────────────────────────┘
//!                  (user connects camera)
//! ```

use std::time::Instant;
use crate::core::{CameraDevice, DeviceId};

/// Current state of the first-run experience
#[derive(Debug, Clone)]
pub enum FirstRunState {
    /// Initial welcome screen explaining what Micround does
    Welcome,

    /// Detecting available cameras
    DetectingCameras,

    /// No camera found - guide user to connect one
    NoCamera {
        /// Time when we started showing this state (for polling)
        since: Instant,
    },

    /// Single camera found - show preview
    SingleCamera {
        device: CameraDevice,
    },

    /// Multiple cameras found - let user choose
    MultipleCamera {
        devices: Vec<CameraDevice>,
        selected_index: Option<usize>,
    },

    /// Showing camera preview before committing
    Preview {
        device: CameraDevice,
    },

    /// Permission denied (macOS)
    PermissionDenied {
        platform: String,
    },

    /// Confirming setup before applying
    ConfirmSetup {
        device: CameraDevice,
    },

    /// Setup complete!
    Success {
        device: CameraDevice,
    },

    /// User cancelled the wizard
    Cancelled,
}

/// Actions the user can take in each state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirstRunAction {
    /// Start the setup process
    StartSetup,
    /// Refresh camera list
    RefreshCameras,
    /// Select a camera by index
    SelectCamera(usize),
    /// Confirm the current camera selection
    ConfirmCamera,
    /// Start the camera preview
    StartPreview,
    /// Apply the selected camera as wallpaper
    ApplyWallpaper,
    /// Go back to previous step
    GoBack,
    /// Open system settings (for permissions)
    OpenSettings,
    /// Cancel and exit the wizard
    Cancel,
    /// Skip wizard (use defaults or defer)
    Skip,
}

/// Content to display in each state
#[derive(Debug, Clone)]
pub struct StateContent {
    /// Main heading
    pub title: &'static str,
    /// Descriptive text
    pub description: &'static str,
    /// Primary action button label (if any)
    pub primary_action: Option<(&'static str, FirstRunAction)>,
    /// Secondary action button label (if any)
    pub secondary_action: Option<(&'static str, FirstRunAction)>,
    /// Whether to show camera preview
    pub show_preview: bool,
    /// Whether to show camera list
    pub show_camera_list: bool,
    /// Trust message to display (if any)
    pub trust_message: Option<&'static str>,
}

impl FirstRunState {
    /// Get the content to display for this state
    pub fn content(&self) -> StateContent {
        match self {
            Self::Welcome => StateContent {
                title: "Welcome to Micround",
                description: "Turn your USB microscope into a living desktop wallpaper. \
                    Your camera feed is displayed directly on your desktop background.",
                primary_action: Some(("Get Started", FirstRunAction::StartSetup)),
                secondary_action: Some(("Skip", FirstRunAction::Skip)),
                show_preview: false,
                show_camera_list: false,
                trust_message: Some(
                    "Privacy first: Your camera feed stays on your computer. \
                    Nothing is recorded or sent anywhere."
                ),
            },

            Self::DetectingCameras => StateContent {
                title: "Looking for cameras...",
                description: "Scanning for connected USB cameras and microscopes.",
                primary_action: None,
                secondary_action: Some(("Cancel", FirstRunAction::Cancel)),
                show_preview: false,
                show_camera_list: false,
                trust_message: None,
            },

            Self::NoCamera { .. } => StateContent {
                title: "No Camera Detected",
                description: "Connect your USB microscope and click Refresh. \
                    Make sure the camera is plugged in and powered on.",
                primary_action: Some(("Refresh", FirstRunAction::RefreshCameras)),
                secondary_action: Some(("Cancel", FirstRunAction::Cancel)),
                show_preview: false,
                show_camera_list: false,
                trust_message: None,
            },

            Self::SingleCamera { device: _ } => StateContent {
                title: "Camera Found!",
                description: "We found your camera. Let's make sure it's working.",
                primary_action: Some(("Preview Camera", FirstRunAction::StartPreview)),
                secondary_action: Some(("Back", FirstRunAction::GoBack)),
                show_preview: false,
                show_camera_list: true,
                trust_message: None,
            },

            Self::MultipleCamera { .. } => StateContent {
                title: "Select Your Camera",
                description: "Multiple cameras detected. Choose the one you want to use \
                    as your wallpaper source.",
                primary_action: Some(("Preview Selected", FirstRunAction::StartPreview)),
                secondary_action: Some(("Back", FirstRunAction::GoBack)),
                show_preview: false,
                show_camera_list: true,
                trust_message: None,
            },

            Self::Preview { .. } => StateContent {
                title: "Camera Preview",
                description: "Make sure this is the correct camera and the image looks good.",
                primary_action: Some(("Use This Camera", FirstRunAction::ConfirmCamera)),
                secondary_action: Some(("Choose Different", FirstRunAction::GoBack)),
                show_preview: true,
                show_camera_list: false,
                trust_message: None,
            },

            Self::PermissionDenied { platform: _ } => StateContent {
                title: "Camera Access Needed",
                description: "Micround needs camera access to display your microscope feed. \
                    Please grant permission in System Settings.",
                primary_action: Some(("Open Settings", FirstRunAction::OpenSettings)),
                secondary_action: Some(("Cancel", FirstRunAction::Cancel)),
                show_preview: false,
                show_camera_list: false,
                trust_message: Some(
                    "Your camera feed is only used for the wallpaper display. \
                    We don't record, store, or transmit any video."
                ),
            },

            Self::ConfirmSetup { .. } => StateContent {
                title: "Ready to Set Up",
                description: "Your camera feed will replace your desktop wallpaper. \
                    You can always stop it from the system tray icon.",
                primary_action: Some(("Set as Wallpaper", FirstRunAction::ApplyWallpaper)),
                secondary_action: Some(("Back", FirstRunAction::GoBack)),
                show_preview: true,
                show_camera_list: false,
                trust_message: None,
            },

            Self::Success { .. } => StateContent {
                title: "All Set!",
                description: "Your microscope feed is now your desktop wallpaper. \
                    Use the system tray icon to pause, stop, or change settings.",
                primary_action: Some(("Done", FirstRunAction::Skip)),
                secondary_action: None,
                show_preview: false,
                show_camera_list: false,
                trust_message: None,
            },

            Self::Cancelled => StateContent {
                title: "Setup Cancelled",
                description: "You can run the setup wizard anytime from the settings menu.",
                primary_action: Some(("Exit", FirstRunAction::Cancel)),
                secondary_action: None,
                show_preview: false,
                show_camera_list: false,
                trust_message: None,
            },
        }
    }

    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Success { .. } | Self::Cancelled)
    }
}

/// Controller for the first-run experience
pub struct FirstRunController {
    state: FirstRunState,
    /// Cameras found during detection
    cameras: Vec<CameraDevice>,
    /// Whether this is the first run (vs. re-running wizard)
    is_first_run: bool,
}

impl FirstRunController {
    /// Create a new first-run controller
    pub fn new(is_first_run: bool) -> Self {
        Self {
            state: FirstRunState::Welcome,
            cameras: Vec::new(),
            is_first_run,
        }
    }

    /// Get current state
    pub fn state(&self) -> &FirstRunState {
        &self.state
    }

    /// Get available cameras
    pub fn cameras(&self) -> &[CameraDevice] {
        &self.cameras
    }

    /// Process an action from the UI
    pub fn handle_action(&mut self, action: FirstRunAction) -> Option<FirstRunEvent> {
        match action {
            FirstRunAction::StartSetup => {
                self.state = FirstRunState::DetectingCameras;
                Some(FirstRunEvent::StartCameraDetection)
            }

            FirstRunAction::RefreshCameras => {
                self.state = FirstRunState::DetectingCameras;
                Some(FirstRunEvent::StartCameraDetection)
            }

            FirstRunAction::SelectCamera(index) => {
                if let FirstRunState::MultipleCamera { ref mut selected_index, .. } = self.state {
                    *selected_index = Some(index);
                }
                None
            }

            FirstRunAction::StartPreview => {
                if let Some(device) = self.get_selected_camera().cloned() {
                    let device_id = device.id.clone();
                    self.state = FirstRunState::Preview { device };
                    Some(FirstRunEvent::StartPreview(device_id))
                } else {
                    None
                }
            }

            FirstRunAction::ConfirmCamera => {
                if let FirstRunState::Preview { ref device } = self.state {
                    let device = device.clone();
                    self.state = FirstRunState::ConfirmSetup { device };
                }
                None
            }

            FirstRunAction::ApplyWallpaper => {
                if let FirstRunState::ConfirmSetup { ref device } = self.state {
                    let device = device.clone();
                    self.state = FirstRunState::Success { device: device.clone() };
                    Some(FirstRunEvent::ApplyWallpaper(device.id.clone()))
                } else {
                    None
                }
            }

            FirstRunAction::GoBack => {
                self.go_back();
                None
            }

            FirstRunAction::OpenSettings => {
                Some(FirstRunEvent::OpenSystemSettings)
            }

            FirstRunAction::Cancel => {
                self.state = FirstRunState::Cancelled;
                Some(FirstRunEvent::Cancelled)
            }

            FirstRunAction::Skip => {
                if self.state.is_terminal() {
                    Some(FirstRunEvent::Completed)
                } else {
                    self.state = FirstRunState::Cancelled;
                    Some(FirstRunEvent::Skipped)
                }
            }
        }
    }

    /// Called when camera detection completes
    pub fn on_cameras_detected(&mut self, cameras: Vec<CameraDevice>) {
        self.cameras = cameras.clone();

        self.state = match cameras.len() {
            0 => FirstRunState::NoCamera { since: Instant::now() },
            1 => FirstRunState::SingleCamera { device: cameras.into_iter().next().unwrap() },
            _ => FirstRunState::MultipleCamera {
                devices: cameras,
                selected_index: None,
            },
        };
    }

    /// Called when camera permission is denied
    pub fn on_permission_denied(&mut self) {
        self.state = FirstRunState::PermissionDenied {
            platform: std::env::consts::OS.to_string(),
        };
    }

    /// Go back to the previous logical state
    fn go_back(&mut self) {
        self.state = match &self.state {
            FirstRunState::NoCamera { .. } => FirstRunState::Welcome,
            FirstRunState::SingleCamera { .. } => FirstRunState::Welcome,
            FirstRunState::MultipleCamera { .. } => FirstRunState::Welcome,
            FirstRunState::Preview { .. } => {
                // Go back to camera selection
                match self.cameras.len() {
                    0 => FirstRunState::DetectingCameras,
                    1 => FirstRunState::SingleCamera {
                        device: self.cameras[0].clone()
                    },
                    _ => FirstRunState::MultipleCamera {
                        devices: self.cameras.clone(),
                        selected_index: None,
                    },
                }
            }
            FirstRunState::ConfirmSetup { device } => {
                FirstRunState::Preview { device: device.clone() }
            }
            _ => FirstRunState::Welcome,
        };
    }

    /// Get the currently selected camera (if any)
    fn get_selected_camera(&self) -> Option<&CameraDevice> {
        match &self.state {
            FirstRunState::SingleCamera { device } => Some(device),
            FirstRunState::MultipleCamera { devices, selected_index } => {
                selected_index.and_then(|i| devices.get(i))
            }
            FirstRunState::Preview { device } => Some(device),
            FirstRunState::ConfirmSetup { device } => Some(device),
            _ => None,
        }
    }
}

/// Events emitted by the first-run controller
#[derive(Debug, Clone)]
pub enum FirstRunEvent {
    /// Start camera detection
    StartCameraDetection,
    /// Start preview for a camera
    StartPreview(DeviceId),
    /// Apply the selected camera as wallpaper
    ApplyWallpaper(DeviceId),
    /// Open system settings for permissions
    OpenSystemSettings,
    /// User completed the wizard
    Completed,
    /// User skipped the wizard
    Skipped,
    /// User cancelled the wizard
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_device(name: &str) -> CameraDevice {
        CameraDevice {
            id: DeviceId(name.to_string()),
            name: name.to_string(),
            manufacturer: None,
            capabilities: vec![],
            is_available: true,
        }
    }

    #[test]
    fn test_welcome_state_content() {
        let state = FirstRunState::Welcome;
        let content = state.content();

        assert_eq!(content.title, "Welcome to Micround");
        assert!(content.primary_action.is_some());
        assert!(content.trust_message.is_some());
    }

    #[test]
    fn test_start_setup_transitions_to_detecting() {
        let mut controller = FirstRunController::new(true);
        assert!(matches!(controller.state(), FirstRunState::Welcome));

        let event = controller.handle_action(FirstRunAction::StartSetup);

        assert!(matches!(controller.state(), FirstRunState::DetectingCameras));
        assert!(matches!(event, Some(FirstRunEvent::StartCameraDetection)));
    }

    #[test]
    fn test_no_cameras_detected() {
        let mut controller = FirstRunController::new(true);
        controller.handle_action(FirstRunAction::StartSetup);

        controller.on_cameras_detected(vec![]);

        assert!(matches!(controller.state(), FirstRunState::NoCamera { .. }));
    }

    #[test]
    fn test_single_camera_detected() {
        let mut controller = FirstRunController::new(true);
        controller.handle_action(FirstRunAction::StartSetup);

        let device = make_test_device("USB Microscope");
        controller.on_cameras_detected(vec![device.clone()]);

        assert!(matches!(controller.state(), FirstRunState::SingleCamera { .. }));
    }

    #[test]
    fn test_multiple_cameras_detected() {
        let mut controller = FirstRunController::new(true);
        controller.handle_action(FirstRunAction::StartSetup);

        let devices = vec![
            make_test_device("USB Microscope"),
            make_test_device("Built-in Camera"),
        ];
        controller.on_cameras_detected(devices);

        assert!(matches!(controller.state(), FirstRunState::MultipleCamera { .. }));
    }

    #[test]
    fn test_full_happy_path() {
        let mut controller = FirstRunController::new(true);

        // Start
        controller.handle_action(FirstRunAction::StartSetup);

        // Camera found
        let device = make_test_device("Microscope");
        controller.on_cameras_detected(vec![device]);

        // Start preview
        controller.handle_action(FirstRunAction::StartPreview);
        assert!(matches!(controller.state(), FirstRunState::Preview { .. }));

        // Confirm camera
        controller.handle_action(FirstRunAction::ConfirmCamera);
        assert!(matches!(controller.state(), FirstRunState::ConfirmSetup { .. }));

        // Apply wallpaper
        let event = controller.handle_action(FirstRunAction::ApplyWallpaper);
        assert!(matches!(controller.state(), FirstRunState::Success { .. }));
        assert!(matches!(event, Some(FirstRunEvent::ApplyWallpaper(_))));
    }

    #[test]
    fn test_go_back_from_preview() {
        let mut controller = FirstRunController::new(true);
        controller.handle_action(FirstRunAction::StartSetup);

        let device = make_test_device("Microscope");
        controller.on_cameras_detected(vec![device]);
        controller.handle_action(FirstRunAction::StartPreview);

        controller.handle_action(FirstRunAction::GoBack);

        assert!(matches!(controller.state(), FirstRunState::SingleCamera { .. }));
    }

    #[test]
    fn test_skip_from_welcome() {
        let mut controller = FirstRunController::new(true);

        let event = controller.handle_action(FirstRunAction::Skip);

        assert!(matches!(event, Some(FirstRunEvent::Skipped)));
    }

    #[test]
    fn test_terminal_states() {
        assert!(FirstRunState::Success {
            device: make_test_device("test")
        }.is_terminal());
        assert!(FirstRunState::Cancelled.is_terminal());
        assert!(!FirstRunState::Welcome.is_terminal());
    }
}
