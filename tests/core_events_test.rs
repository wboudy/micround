//! Unit tests for core/events.rs
//!
//! Tests Command enum, Event enum, EventBus pub/sub,
//! AppState transitions, and AppHandle command sending.
//!
//! Run with: cargo test --test core_events_test

use micround::core::{
    AppContext, AppState, CaptureSettings, Command, DeviceId, DisplayId, Event,
    EventBus, Flip, FrameDropReason, Rotation, ScalingMode,
};

// ============================================================================
// Command Enum Tests
// ============================================================================

#[test]
fn test_command_start_capture() {
    let cmd = Command::StartCapture {
        device_id: DeviceId("camera-0".to_string()),
    };
    assert!(matches!(cmd, Command::StartCapture { .. }));
}

#[test]
fn test_command_stop_capture() {
    let cmd = Command::StopCapture;
    assert!(matches!(cmd, Command::StopCapture));
}

#[test]
fn test_command_pause_display() {
    let cmd = Command::PauseDisplay;
    assert!(matches!(cmd, Command::PauseDisplay));
}

#[test]
fn test_command_resume_display() {
    let cmd = Command::ResumeDisplay;
    assert!(matches!(cmd, Command::ResumeDisplay));
}

#[test]
fn test_command_update_capture_settings() {
    let settings = CaptureSettings {
        width: 1280,
        height: 720,
        framerate: 60.0,
        format: None,
    };
    let cmd = Command::UpdateCaptureSettings { settings };
    assert!(matches!(cmd, Command::UpdateCaptureSettings { .. }));
}

#[test]
fn test_command_take_snapshot_to_clipboard() {
    let cmd = Command::TakeSnapshot { to_clipboard: true };
    if let Command::TakeSnapshot { to_clipboard } = cmd {
        assert!(to_clipboard);
    } else {
        panic!("Expected TakeSnapshot command");
    }
}

#[test]
fn test_command_take_snapshot_to_file() {
    let cmd = Command::TakeSnapshot { to_clipboard: false };
    if let Command::TakeSnapshot { to_clipboard } = cmd {
        assert!(!to_clipboard);
    } else {
        panic!("Expected TakeSnapshot command");
    }
}

#[test]
fn test_command_select_camera() {
    let cmd = Command::SelectCamera {
        device_id: DeviceId("usb-camera".to_string()),
    };
    if let Command::SelectCamera { device_id } = cmd {
        assert_eq!(device_id.0, "usb-camera");
    } else {
        panic!("Expected SelectCamera command");
    }
}

#[test]
fn test_command_select_display() {
    let cmd = Command::SelectDisplay {
        display_id: DisplayId("HDMI-1".to_string()),
    };
    if let Command::SelectDisplay { display_id } = cmd {
        assert_eq!(display_id.0, "HDMI-1");
    } else {
        panic!("Expected SelectDisplay command");
    }
}

#[test]
fn test_command_set_scaling() {
    let cmd = Command::SetScaling { mode: ScalingMode::Fill };
    if let Command::SetScaling { mode } = cmd {
        assert_eq!(mode, ScalingMode::Fill);
    } else {
        panic!("Expected SetScaling command");
    }
}

#[test]
fn test_command_set_rotation() {
    let cmd = Command::SetRotation { rotation: Rotation::Clockwise90 };
    if let Command::SetRotation { rotation } = cmd {
        assert_eq!(rotation, Rotation::Clockwise90);
    } else {
        panic!("Expected SetRotation command");
    }
}

#[test]
fn test_command_set_flip() {
    let cmd = Command::SetFlip { flip: Flip::Horizontal };
    if let Command::SetFlip { flip } = cmd {
        assert_eq!(flip, Flip::Horizontal);
    } else {
        panic!("Expected SetFlip command");
    }
}

#[test]
fn test_command_quit() {
    let cmd = Command::Quit;
    assert!(matches!(cmd, Command::Quit));
}

#[test]
fn test_command_clone() {
    let cmd1 = Command::StartCapture {
        device_id: DeviceId("cam".to_string()),
    };
    let cmd2 = cmd1.clone();
    assert!(matches!(cmd2, Command::StartCapture { .. }));
}

#[test]
fn test_command_debug() {
    let cmd = Command::StopCapture;
    let debug_str = format!("{:?}", cmd);
    assert_eq!(debug_str, "StopCapture");
}

// ============================================================================
// Event Enum Tests
// ============================================================================

#[test]
fn test_event_camera_connected() {
    use micround::core::CameraDevice;

    let device = CameraDevice {
        id: DeviceId("cam-1".to_string()),
        name: "Test Camera".to_string(),
        manufacturer: Some("Test Corp".to_string()),
        capabilities: vec![],
        is_available: true,
    };
    let event = Event::CameraConnected { device };
    assert!(matches!(event, Event::CameraConnected { .. }));
}

#[test]
fn test_event_camera_disconnected() {
    let event = Event::CameraDisconnected {
        device_id: DeviceId("cam-1".to_string()),
    };
    if let Event::CameraDisconnected { device_id } = event {
        assert_eq!(device_id.0, "cam-1");
    } else {
        panic!("Expected CameraDisconnected event");
    }
}

#[test]
fn test_event_capture_started() {
    let event = Event::CaptureStarted {
        device_id: DeviceId("cam-0".to_string()),
        resolution: (1920, 1080),
        fps: 30.0,
    };
    if let Event::CaptureStarted { device_id, resolution, fps } = event {
        assert_eq!(device_id.0, "cam-0");
        assert_eq!(resolution, (1920, 1080));
        assert_eq!(fps, 30.0);
    } else {
        panic!("Expected CaptureStarted event");
    }
}

#[test]
fn test_event_capture_stopped() {
    let event = Event::CaptureStopped {
        device_id: DeviceId("cam-0".to_string()),
    };
    assert!(matches!(event, Event::CaptureStopped { .. }));
}

#[test]
fn test_event_frame_dropped() {
    let event = Event::FrameDropped {
        sequence: 42,
        reason: FrameDropReason::QueueFull,
    };
    if let Event::FrameDropped { sequence, reason } = event {
        assert_eq!(sequence, 42);
        assert!(matches!(reason, FrameDropReason::QueueFull));
    } else {
        panic!("Expected FrameDropped event");
    }
}

#[test]
fn test_event_display_paused() {
    let event = Event::DisplayPaused;
    assert!(matches!(event, Event::DisplayPaused));
}

#[test]
fn test_event_display_resumed() {
    let event = Event::DisplayResumed;
    assert!(matches!(event, Event::DisplayResumed));
}

#[test]
fn test_event_snapshot_taken() {
    let event = Event::SnapshotTaken { to_clipboard: true };
    if let Event::SnapshotTaken { to_clipboard } = event {
        assert!(to_clipboard);
    } else {
        panic!("Expected SnapshotTaken event");
    }
}

#[test]
fn test_event_error() {
    use micround::core::{CaptureError, ErrorContext, MicroundError};

    let source = CaptureError::Timeout(1000);
    let error = MicroundError::Capture {
        source,
        context: ErrorContext::new(),
    };
    let event = Event::Error { error };
    assert!(matches!(event, Event::Error { .. }));
}

#[test]
fn test_event_settings_changed() {
    let event = Event::SettingsChanged;
    assert!(matches!(event, Event::SettingsChanged));
}

#[test]
fn test_event_state_changed() {
    let event = Event::StateChanged {
        old_state: AppState::Idle,
        new_state: AppState::Running,
    };
    if let Event::StateChanged { old_state, new_state } = event {
        assert_eq!(old_state, AppState::Idle);
        assert_eq!(new_state, AppState::Running);
    } else {
        panic!("Expected StateChanged event");
    }
}

#[test]
fn test_event_clone() {
    let event1 = Event::DisplayPaused;
    let event2 = event1.clone();
    assert!(matches!(event2, Event::DisplayPaused));
}

#[test]
fn test_event_debug() {
    let event = Event::DisplayResumed;
    let debug_str = format!("{:?}", event);
    assert_eq!(debug_str, "DisplayResumed");
}

// ============================================================================
// FrameDropReason Tests
// ============================================================================

#[test]
fn test_frame_drop_reason_queue_full() {
    let reason = FrameDropReason::QueueFull;
    assert!(matches!(reason, FrameDropReason::QueueFull));
}

#[test]
fn test_frame_drop_reason_processing_timeout() {
    let reason = FrameDropReason::ProcessingTimeout;
    assert!(matches!(reason, FrameDropReason::ProcessingTimeout));
}

#[test]
fn test_frame_drop_reason_render_queue_full() {
    let reason = FrameDropReason::RenderQueueFull;
    assert!(matches!(reason, FrameDropReason::RenderQueueFull));
}

#[test]
fn test_frame_drop_reason_copy() {
    let reason1 = FrameDropReason::QueueFull;
    let reason2 = reason1; // Copy
    assert!(matches!(reason2, FrameDropReason::QueueFull));
}

#[test]
fn test_frame_drop_reason_clone() {
    let reason1 = FrameDropReason::ProcessingTimeout;
    let reason2 = reason1.clone();
    assert!(matches!(reason2, FrameDropReason::ProcessingTimeout));
}

#[test]
fn test_frame_drop_reason_debug() {
    assert_eq!(format!("{:?}", FrameDropReason::QueueFull), "QueueFull");
    assert_eq!(format!("{:?}", FrameDropReason::ProcessingTimeout), "ProcessingTimeout");
    assert_eq!(format!("{:?}", FrameDropReason::RenderQueueFull), "RenderQueueFull");
}

// ============================================================================
// AppState Tests
// ============================================================================

#[test]
fn test_app_state_variants_exist() {
    let states = [
        AppState::Idle,
        AppState::Starting,
        AppState::Running,
        AppState::Paused,
        AppState::Reconnecting,
        AppState::Error,
        AppState::ShuttingDown,
    ];

    assert_eq!(states.len(), 7);
}

#[test]
fn test_app_state_equality() {
    assert_eq!(AppState::Idle, AppState::Idle);
    assert_eq!(AppState::Running, AppState::Running);
    assert_ne!(AppState::Idle, AppState::Running);
}

#[test]
fn test_app_state_copy() {
    let state1 = AppState::Running;
    let state2 = state1; // Copy
    assert_eq!(state1, state2);
}

#[test]
fn test_app_state_clone() {
    let state1 = AppState::Paused;
    let state2 = state1.clone();
    assert_eq!(state1, state2);
}

#[test]
fn test_app_state_debug() {
    assert_eq!(format!("{:?}", AppState::Idle), "Idle");
    assert_eq!(format!("{:?}", AppState::Starting), "Starting");
    assert_eq!(format!("{:?}", AppState::Running), "Running");
    assert_eq!(format!("{:?}", AppState::Paused), "Paused");
    assert_eq!(format!("{:?}", AppState::Reconnecting), "Reconnecting");
    assert_eq!(format!("{:?}", AppState::Error), "Error");
    assert_eq!(format!("{:?}", AppState::ShuttingDown), "ShuttingDown");
}

#[test]
fn test_app_state_display() {
    assert_eq!(format!("{}", AppState::Idle), "Idle");
    assert_eq!(format!("{}", AppState::Starting), "Starting");
    assert_eq!(format!("{}", AppState::Running), "Running");
    assert_eq!(format!("{}", AppState::Paused), "Paused");
    assert_eq!(format!("{}", AppState::Reconnecting), "Reconnecting");
    assert_eq!(format!("{}", AppState::Error), "Error");
    assert_eq!(format!("{}", AppState::ShuttingDown), "ShuttingDown");
}

// ============================================================================
// AppState Transition Tests
// ============================================================================

#[test]
fn test_app_state_transitions_from_idle() {
    let idle = AppState::Idle;

    assert!(idle.can_transition_to(AppState::Starting));
    assert!(idle.can_transition_to(AppState::ShuttingDown));

    assert!(!idle.can_transition_to(AppState::Idle));
    assert!(!idle.can_transition_to(AppState::Running));
    assert!(!idle.can_transition_to(AppState::Paused));
    assert!(!idle.can_transition_to(AppState::Reconnecting));
    assert!(!idle.can_transition_to(AppState::Error));
}

#[test]
fn test_app_state_transitions_from_starting() {
    let starting = AppState::Starting;

    assert!(starting.can_transition_to(AppState::Running));
    assert!(starting.can_transition_to(AppState::Idle));
    assert!(starting.can_transition_to(AppState::Error));
    assert!(starting.can_transition_to(AppState::ShuttingDown));

    assert!(!starting.can_transition_to(AppState::Starting));
    assert!(!starting.can_transition_to(AppState::Paused));
    assert!(!starting.can_transition_to(AppState::Reconnecting));
}

#[test]
fn test_app_state_transitions_from_running() {
    let running = AppState::Running;

    assert!(running.can_transition_to(AppState::Idle));
    assert!(running.can_transition_to(AppState::Paused));
    assert!(running.can_transition_to(AppState::Error));
    assert!(running.can_transition_to(AppState::Reconnecting));
    assert!(running.can_transition_to(AppState::ShuttingDown));

    assert!(!running.can_transition_to(AppState::Running));
    assert!(!running.can_transition_to(AppState::Starting));
}

#[test]
fn test_app_state_transitions_from_paused() {
    let paused = AppState::Paused;

    assert!(paused.can_transition_to(AppState::Running));
    assert!(paused.can_transition_to(AppState::Idle));
    assert!(paused.can_transition_to(AppState::Error));
    assert!(paused.can_transition_to(AppState::ShuttingDown));

    assert!(!paused.can_transition_to(AppState::Paused));
    assert!(!paused.can_transition_to(AppState::Starting));
    assert!(!paused.can_transition_to(AppState::Reconnecting));
}

#[test]
fn test_app_state_transitions_from_reconnecting() {
    let reconnecting = AppState::Reconnecting;

    assert!(reconnecting.can_transition_to(AppState::Running));
    assert!(reconnecting.can_transition_to(AppState::Idle));
    assert!(reconnecting.can_transition_to(AppState::Error));
    assert!(reconnecting.can_transition_to(AppState::ShuttingDown));

    assert!(!reconnecting.can_transition_to(AppState::Reconnecting));
    assert!(!reconnecting.can_transition_to(AppState::Starting));
    assert!(!reconnecting.can_transition_to(AppState::Paused));
}

#[test]
fn test_app_state_transitions_from_error() {
    let error = AppState::Error;

    assert!(error.can_transition_to(AppState::Idle));
    assert!(error.can_transition_to(AppState::Starting));
    assert!(error.can_transition_to(AppState::ShuttingDown));

    assert!(!error.can_transition_to(AppState::Error));
    assert!(!error.can_transition_to(AppState::Running));
    assert!(!error.can_transition_to(AppState::Paused));
    assert!(!error.can_transition_to(AppState::Reconnecting));
}

#[test]
fn test_app_state_transitions_from_shutting_down() {
    let shutting_down = AppState::ShuttingDown;

    // ShuttingDown is terminal - no valid transitions
    assert!(!shutting_down.can_transition_to(AppState::Idle));
    assert!(!shutting_down.can_transition_to(AppState::Starting));
    assert!(!shutting_down.can_transition_to(AppState::Running));
    assert!(!shutting_down.can_transition_to(AppState::Paused));
    assert!(!shutting_down.can_transition_to(AppState::Reconnecting));
    assert!(!shutting_down.can_transition_to(AppState::Error));
    assert!(!shutting_down.can_transition_to(AppState::ShuttingDown));
}

// ============================================================================
// AppState Helper Method Tests
// ============================================================================

#[test]
fn test_app_state_is_capturing() {
    assert!(!AppState::Idle.is_capturing());
    assert!(!AppState::Starting.is_capturing());
    assert!(AppState::Running.is_capturing());
    assert!(AppState::Paused.is_capturing());
    assert!(AppState::Reconnecting.is_capturing());
    assert!(!AppState::Error.is_capturing());
    assert!(!AppState::ShuttingDown.is_capturing());
}

#[test]
fn test_app_state_can_accept_commands() {
    assert!(AppState::Idle.can_accept_commands());
    assert!(!AppState::Starting.can_accept_commands());
    assert!(AppState::Running.can_accept_commands());
    assert!(AppState::Paused.can_accept_commands());
    assert!(!AppState::Reconnecting.can_accept_commands());
    assert!(AppState::Error.can_accept_commands());
    assert!(!AppState::ShuttingDown.can_accept_commands());
}

// ============================================================================
// EventBus Tests
// ============================================================================

#[test]
fn test_event_bus_new() {
    let bus = EventBus::new();
    assert_eq!(bus.subscriber_count(), 0);
}

#[test]
fn test_event_bus_default() {
    let bus: EventBus = Default::default();
    assert_eq!(bus.subscriber_count(), 0);
}

#[test]
fn test_event_bus_subscribe_increases_count() {
    let bus = EventBus::new();
    assert_eq!(bus.subscriber_count(), 0);

    let _sub1 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 1);

    let _sub2 = bus.subscribe();
    assert_eq!(bus.subscriber_count(), 2);
}

#[test]
fn test_event_bus_subscriber_drop_decreases_count() {
    let bus = EventBus::new();

    {
        let _sub1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
    }
    // sub1 dropped

    assert_eq!(bus.subscriber_count(), 0);
}

#[test]
fn test_event_bus_publish_no_subscribers() {
    let bus = EventBus::new();
    let count = bus.publish(Event::DisplayPaused);
    assert_eq!(count, 0);
}

#[test]
fn test_event_bus_publish_with_subscribers() {
    let bus = EventBus::new();
    let _sub1 = bus.subscribe();
    let _sub2 = bus.subscribe();
    let _sub3 = bus.subscribe();

    let count = bus.publish(Event::DisplayResumed);
    assert_eq!(count, 3);
}

#[test]
fn test_event_bus_clone() {
    let bus1 = EventBus::new();
    let _sub1 = bus1.subscribe();

    let bus2 = bus1.clone();

    // Clone shares the same underlying broadcast channel
    assert_eq!(bus2.subscriber_count(), 1);

    // Publishing on either reaches subscribers
    let _sub2 = bus2.subscribe();
    assert_eq!(bus1.subscriber_count(), 2);
    assert_eq!(bus2.subscriber_count(), 2);
}

#[tokio::test]
async fn test_event_subscriber_recv() {
    let bus = EventBus::new();
    let mut sub = bus.subscribe();

    bus.publish(Event::SettingsChanged);

    let event = sub.recv().await;
    assert!(event.is_some());
    assert!(matches!(event.unwrap(), Event::SettingsChanged));
}

#[test]
fn test_event_subscriber_try_recv_empty() {
    let bus = EventBus::new();
    let mut sub = bus.subscribe();

    let result = sub.try_recv();
    assert!(result.is_none());
}

#[test]
fn test_event_subscriber_try_recv_with_event() {
    let bus = EventBus::new();
    let mut sub = bus.subscribe();

    bus.publish(Event::DisplayPaused);

    let event = sub.try_recv();
    assert!(event.is_some());
    assert!(matches!(event.unwrap(), Event::DisplayPaused));
}

#[tokio::test]
async fn test_event_bus_multiple_events() {
    let bus = EventBus::new();
    let mut sub = bus.subscribe();

    bus.publish(Event::DisplayPaused);
    bus.publish(Event::DisplayResumed);
    bus.publish(Event::SettingsChanged);

    let e1 = sub.recv().await.unwrap();
    let e2 = sub.recv().await.unwrap();
    let e3 = sub.recv().await.unwrap();

    assert!(matches!(e1, Event::DisplayPaused));
    assert!(matches!(e2, Event::DisplayResumed));
    assert!(matches!(e3, Event::SettingsChanged));
}

// ============================================================================
// AppContext Tests
// ============================================================================

#[test]
fn test_app_context_new() {
    let (ctx, _rx) = AppContext::new();
    assert_eq!(ctx.events.subscriber_count(), 0);
}

#[test]
fn test_app_context_handle() {
    let (ctx, _rx) = AppContext::new();
    let _handle = ctx.handle();
    // Handle was created successfully
}

#[test]
fn test_app_context_multiple_handles() {
    let (ctx, _rx) = AppContext::new();
    let handle1 = ctx.handle();
    let handle2 = ctx.handle();

    // Both handles share the same event bus
    let _sub = handle1.subscribe_events();
    assert!(handle2.subscribe_events().try_recv().is_none());
}

// ============================================================================
// AppHandle Tests
// ============================================================================

#[tokio::test]
async fn test_app_handle_send_command() {
    let (ctx, mut rx) = AppContext::new();
    let handle = ctx.handle();

    handle.send_command(Command::Quit).await.unwrap();

    let cmd = rx.recv().await.unwrap();
    assert!(matches!(cmd, Command::Quit));
}

#[test]
fn test_app_handle_try_send_command() {
    let (ctx, _rx) = AppContext::new();
    let handle = ctx.handle();

    let result = handle.try_send_command(Command::StopCapture);
    assert!(result.is_ok());
}

#[test]
fn test_app_handle_try_send_command_receiver_dropped() {
    let (ctx, rx) = AppContext::new();
    let handle = ctx.handle();

    drop(rx);

    // Should fail since receiver is gone
    let result = handle.try_send_command(Command::StopCapture);
    assert!(result.is_err());
}

#[test]
fn test_app_handle_publish_event() {
    let (ctx, _rx) = AppContext::new();
    let handle = ctx.handle();

    let mut sub = handle.subscribe_events();

    let count = handle.publish_event(Event::DisplayPaused);
    assert_eq!(count, 1);

    let event = sub.try_recv().unwrap();
    assert!(matches!(event, Event::DisplayPaused));
}

#[test]
fn test_app_handle_subscribe_events() {
    let (ctx, _rx) = AppContext::new();
    let handle = ctx.handle();

    let _sub1 = handle.subscribe_events();
    let _sub2 = handle.subscribe_events();

    // Both subscriptions work
    let count = handle.publish_event(Event::SettingsChanged);
    assert_eq!(count, 2);
}

#[test]
fn test_app_handle_clone() {
    let (ctx, _rx) = AppContext::new();
    let handle1 = ctx.handle();
    let handle2 = handle1.clone();

    let mut sub = handle1.subscribe_events();

    // Publishing on cloned handle reaches subscribers
    handle2.publish_event(Event::DisplayResumed);

    let event = sub.try_recv().unwrap();
    assert!(matches!(event, Event::DisplayResumed));
}

// ============================================================================
// Command Channel Tests (via AppContext)
// ============================================================================

#[tokio::test]
async fn test_command_channel_send_receive_via_context() {
    let (ctx, mut rx) = AppContext::new();
    let handle = ctx.handle();

    handle.send_command(Command::PauseDisplay).await.unwrap();
    handle.send_command(Command::ResumeDisplay).await.unwrap();

    let cmd1 = rx.recv().await.unwrap();
    let cmd2 = rx.recv().await.unwrap();

    assert!(matches!(cmd1, Command::PauseDisplay));
    assert!(matches!(cmd2, Command::ResumeDisplay));
}

#[tokio::test]
async fn test_command_channel_receiver_dropped_via_context() {
    let (ctx, rx) = AppContext::new();
    let handle = ctx.handle();

    drop(rx);

    let result = handle.send_command(Command::Quit).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_command_channel_sender_dropped_via_context() {
    let (ctx, mut rx) = AppContext::new();
    drop(ctx);

    let result = rx.recv().await;
    assert!(result.is_none());
}

// ============================================================================
// Integration Tests
// ============================================================================

#[tokio::test]
async fn test_full_command_event_flow() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();

    // Subscribe to events
    let mut event_sub = handle.subscribe_events();

    // Send command
    handle.send_command(Command::StartCapture {
        device_id: DeviceId("cam-0".to_string()),
    }).await.unwrap();

    // Receive command
    let cmd = cmd_rx.recv().await.unwrap();
    assert!(matches!(cmd, Command::StartCapture { .. }));

    // Publish event in response
    handle.publish_event(Event::CaptureStarted {
        device_id: DeviceId("cam-0".to_string()),
        resolution: (1920, 1080),
        fps: 30.0,
    });

    // Receive event
    let event = event_sub.recv().await.unwrap();
    assert!(matches!(event, Event::CaptureStarted { .. }));
}

#[tokio::test]
async fn test_multiple_subscribers_receive_same_event() {
    let bus = EventBus::new();

    let mut sub1 = bus.subscribe();
    let mut sub2 = bus.subscribe();
    let mut sub3 = bus.subscribe();

    bus.publish(Event::StateChanged {
        old_state: AppState::Idle,
        new_state: AppState::Running,
    });

    let e1 = sub1.recv().await.unwrap();
    let e2 = sub2.recv().await.unwrap();
    let e3 = sub3.recv().await.unwrap();

    // All subscribers receive the same event
    assert!(matches!(e1, Event::StateChanged { .. }));
    assert!(matches!(e2, Event::StateChanged { .. }));
    assert!(matches!(e3, Event::StateChanged { .. }));
}

#[test]
fn test_event_with_complex_payload() {
    use micround::core::{CameraCapability, CameraDevice, PixelFormat};

    let device = CameraDevice {
        id: DeviceId("camera-123".to_string()),
        name: "HD Pro Webcam".to_string(),
        manufacturer: Some("Logitech".to_string()),
        capabilities: vec![
            CameraCapability {
                width: 1920,
                height: 1080,
                framerate: 30.0,
                format: PixelFormat::Mjpeg,
            },
            CameraCapability {
                width: 1280,
                height: 720,
                framerate: 60.0,
                format: PixelFormat::Yuyv,
            },
        ],
        is_available: true,
    };

    let event = Event::CameraConnected { device };

    if let Event::CameraConnected { device } = event {
        assert_eq!(device.id.0, "camera-123");
        assert_eq!(device.name, "HD Pro Webcam");
        assert_eq!(device.capabilities.len(), 2);
    } else {
        panic!("Expected CameraConnected event");
    }
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_state_self_transition_not_allowed() {
    // Self-transitions should not be allowed for any state
    let states = [
        AppState::Idle,
        AppState::Starting,
        AppState::Running,
        AppState::Paused,
        AppState::Reconnecting,
        AppState::Error,
        AppState::ShuttingDown,
    ];

    for state in states {
        assert!(
            !state.can_transition_to(state),
            "{:?} should not be able to transition to itself",
            state
        );
    }
}

#[test]
fn test_command_with_empty_device_id() {
    let cmd = Command::SelectCamera {
        device_id: DeviceId(String::new()),
    };
    if let Command::SelectCamera { device_id } = cmd {
        assert!(device_id.0.is_empty());
    }
}

#[test]
fn test_event_frame_dropped_max_sequence() {
    let event = Event::FrameDropped {
        sequence: u64::MAX,
        reason: FrameDropReason::QueueFull,
    };
    if let Event::FrameDropped { sequence, .. } = event {
        assert_eq!(sequence, u64::MAX);
    }
}

#[test]
fn test_capture_started_zero_resolution() {
    let event = Event::CaptureStarted {
        device_id: DeviceId("test".to_string()),
        resolution: (0, 0),
        fps: 0.0,
    };
    if let Event::CaptureStarted { resolution, fps, .. } = event {
        assert_eq!(resolution, (0, 0));
        assert_eq!(fps, 0.0);
    }
}
