//! Micround - Live microscope camera feed as your desktop wallpaper
//!
//! This is the main entry point for the application.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn, Level};

mod capture;
mod config;
mod core;
mod engine;
mod platform;
mod process;
mod render;
mod ui;

use core::events::{AppContext, AppState, Command, Event};
use engine::DisplayEngine;

fn main() -> Result<()> {
    // Initialize logging with file output and rotation
    core::logging::init(Some(Level::INFO))?;

    info!("Micround starting...");
    info!(
        version = env!("CARGO_PKG_VERSION"),
        log_dir = %core::logging::log_directory().display(),
        "Application initialized"
    );

    // 1. Load or create configuration
    let config = config::load_config()?;
    tracing::debug!(?config, "Configuration loaded");

    // 2. Check for crash recovery
    #[cfg(feature = "recovery")]
    {
        let mut recovery = core::recovery::RecoveryManager::new(config.clone());
        let startup_state = recovery.check_and_recover()?;
        if startup_state.was_crash() {
            tracing::warn!("Recovered from crash - wallpaper restored");
        }
    }

    // 3. Create the tokio runtime for async operations
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;

    // 4. Run the main application loop
    runtime.block_on(run_application(config))
}

/// Main application loop
async fn run_application(config: config::AppConfig) -> Result<()> {
    // Create event bus and command channel
    let (ctx, mut cmd_rx) = AppContext::new();
    let app_handle = ctx.handle();

    // Create display engine
    let engine = Arc::new(DisplayEngine::new(ctx.events.clone()));

    // Windows system event monitoring (sleep/wake, session lock)
    #[cfg(target_os = "windows")]
    let mut _system_monitor = {
        use crate::platform::{
            SessionEvent, SleepEvent, SystemEvent, SystemEventHandler, WindowsSystemMonitor,
        };

        struct SystemEventBridge {
            app_handle: core::events::AppHandle,
        }

        impl SystemEventHandler for SystemEventBridge {
            fn on_system_event(&mut self, event: SystemEvent) {
                let command = match event {
                    SystemEvent::Sleep(SleepEvent::WillSleep)
                    | SystemEvent::Session(SessionEvent::ScreenLocking) => {
                        Some(Command::PauseDisplay)
                    }
                    SystemEvent::Sleep(SleepEvent::DidWake)
                    | SystemEvent::Session(SessionEvent::ScreenUnlocked) => {
                        Some(Command::ResumeDisplay)
                    }
                    SystemEvent::Session(SessionEvent::LoggingOut) => Some(Command::StopCapture),
                    _ => None,
                };

                if let Some(cmd) = command {
                    if let Err(err) = self.app_handle.try_send_command(cmd) {
                        tracing::warn!(error = %err, "Failed to enqueue system command");
                    }
                }
            }
        }

        let handler = Box::new(SystemEventBridge {
            app_handle: app_handle.clone(),
        });
        let mut monitor = WindowsSystemMonitor::new(handler);
        match monitor.start() {
            Ok(()) => {
                info!("Windows system monitor started");
                Some(monitor)
            }
            Err(e) => {
                warn!(error = %e, "Failed to start Windows system monitor");
                None
            }
        }
    };

    // Initialize capture backend
    let _capture_backend = capture::create_backend();

    // Initialize render backend
    let mut renderer = render::create_renderer()?;

    // Subscribe to events for state tracking
    let mut event_subscriber = app_handle.subscribe_events();

    // Track application state (used by event handlers)
    #[allow(unused_assignments)]
    let mut current_state = AppState::Idle;

    // Initialize system tray (if feature enabled)
    #[cfg(feature = "tray")]
    let mut _tray = {
        let initial_state = ui::TrayState::default();
        match ui::TrayController::new(app_handle.clone(), initial_state) {
            Ok(tray) => {
                info!("System tray initialized");
                Some(tray)
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialize system tray, continuing without it");
                None
            }
        }
    };

    // Initialize hotkeys (if feature enabled)
    #[cfg(feature = "hotkeys")]
    let _hotkeys = {
        match core::hotkeys::HotkeyManager::new(app_handle.clone()) {
            Ok(manager) => {
                if let Err(e) = manager.register_all() {
                    warn!(error = %e, "Failed to register hotkeys");
                }
                info!("Hotkey manager initialized");
                Some(manager)
            }
            Err(e) => {
                warn!(error = %e, "Failed to initialize hotkey manager");
                None
            }
        }
    };

    // Initialize settings controller for settings window (bd-2k5)
    let settings_controller = std::sync::Arc::new(std::sync::RwLock::new(
        ui::SettingsController::new(app_handle.clone(), &config),
    ));

    // Preview frame channel for camera preview (bd-37z)
    // The sender is held here to pass to capture when preview starts
    let mut _preview_sender: Option<ui::PreviewFrameSender> = None;

    info!("Micround ready - entering event loop");

    // Main event loop
    loop {
        tokio::select! {
            // Handle commands from UI (tray, hotkeys)
            Some(cmd) = cmd_rx.recv() => {
                tracing::debug!(?cmd, "Received command");

                match &cmd {
                    Command::Quit => {
                        info!("Quit command received, shutting down...");
                        break;
                    }
                    Command::PauseDisplay => {
                        engine.pause();
                    }
                    Command::ResumeDisplay => {
                        engine.resume();
                    }
                    Command::StartCapture { device_id } => {
                        info!(device = %device_id.0, "Start capture requested");
                        engine.start();
                        // State update handled by Event::StateChanged handler below
                        app_handle.publish_event(Event::StateChanged {
                            old_state: AppState::Idle,
                            new_state: AppState::Running,
                        });
                    }
                    Command::StopCapture => {
                        info!("Stop capture requested");
                        engine.stop();
                        // State update handled by Event::StateChanged handler below
                        app_handle.publish_event(Event::StateChanged {
                            old_state: AppState::Running,
                            new_state: AppState::Idle,
                        });
                    }
                    Command::TakeSnapshot { to_clipboard } => {
                        info!(to_clipboard = *to_clipboard, "Snapshot requested");
                        // Snapshot handling would go here
                        app_handle.publish_event(Event::SnapshotTaken {
                            to_clipboard: *to_clipboard
                        });
                    }
                    Command::ShowSettings => {
                        info!("Show settings requested");
                        // TODO: Spawn egui settings window
                        // For now, just log the request
                    }
                    Command::RefreshCameras => {
                        info!("Refresh cameras requested");
                        // TODO: Re-enumerate cameras and update settings window
                    }
                    Command::StartPreview { width, height } => {
                        info!(width, height, "Start preview requested");

                        // Create preview channel and wire to settings controller
                        let (sender, receiver) = ui::create_preview_channel();
                        _preview_sender = Some(sender);
                        settings_controller.write().unwrap().set_preview_receiver(receiver);

                        // TODO: Pass _preview_sender to capture subsystem to deliver frames
                        // For now, the channel is set up but no frames will be delivered
                        // until capture integration is complete
                        info!("Preview channel created - awaiting capture integration");
                    }
                    Command::StopPreview => {
                        info!("Stop preview requested");

                        // Drop the sender to close the channel
                        // The controller will detect disconnection on next poll
                        _preview_sender = None;
                    }
                    _ => {
                        // Other commands handled by specific subsystems
                        engine.handle_command(&cmd);
                    }
                }
            }

            // Handle events from subsystems
            Some(event) = event_subscriber.recv() => {
                tracing::trace!(?event, "Received event");

                match &event {
                    Event::StateChanged { new_state, .. } => {
                        current_state = *new_state;
                        info!(state = %current_state, "Application state changed");

                        // Update tray state when state changes
                        #[cfg(feature = "tray")]
                        if let Some(ref mut tray) = _tray {
                            let tray_state = ui::TrayState {
                                app_state: *new_state,
                                resolution: None,
                                fps: None,
                                camera_name: None,
                            };
                            tray.update_state(tray_state);
                        }
                    }
                    Event::DisplayPaused => {
                        info!("Display paused");
                    }
                    Event::DisplayResumed => {
                        info!("Display resumed");
                    }
                    Event::Error { error } => {
                        warn!(error = %error, "Error occurred");
                    }
                    _ => {}
                }
            }

            // Periodic tick for polling tray/hotkey events (every 50ms)
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // Process tray events
                #[cfg(feature = "tray")]
                if let Some(ref tray) = _tray {
                    ui::process_events(tray);
                }

                // Process hotkey events
                #[cfg(feature = "hotkeys")]
                if let Some(ref hotkeys) = _hotkeys {
                    hotkeys.process_events();
                }

                // Poll for preview frames (bd-37z)
                // This allows the settings UI to receive frames from the capture subsystem
                settings_controller.write().unwrap().poll_preview_frames();
            }
        }
    }

    // Cleanup
    info!("Shutting down...");

    // Restore original wallpaper
    if let Err(e) = renderer.restore(&config) {
        warn!(error = %e, "Failed to restore wallpaper during shutdown");
    }
    renderer.shutdown();

    info!("Micround shutdown complete");
    Ok(())
}
