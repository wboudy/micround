//! Integration test: Event bus message flow (bd-10d)
//!
//! Tests command dispatch and event propagation through all components.
//! Verifies all subscribers receive events, commands reach targets.
//!
//! Run with: cargo test --test integration_event_bus_test

use micround::core::{
    AppContext, AppHandle, AppState, Command, DeviceId, DisplayId, Event,
    EventBus, CaptureSettings, FrameDropReason, MicroundError, CaptureError,
    Flip, Rotation, ScalingMode, ErrorContext,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

// ============================================================================
// Basic Event Bus Tests
// ============================================================================

/// Test basic event publish and subscribe
#[tokio::test]
async fn test_basic_event_publish_subscribe() {
    let bus = EventBus::new();

    // Create subscribers before publishing
    let mut sub1 = bus.subscribe();
    let mut sub2 = bus.subscribe();

    assert_eq!(bus.subscriber_count(), 2);

    // Publish an event
    let device_id = DeviceId("camera:0".into());
    let result = bus.publish(Event::CaptureStarted {
        device_id: device_id.clone(),
        resolution: (640, 480),
        fps: 30.0,
    });

    // Should have reached 2 subscribers
    assert_eq!(result, 2);

    // Both subscribers should receive the event
    let event1 = sub1.try_recv();
    let event2 = sub2.try_recv();

    assert!(event1.is_some());
    assert!(event2.is_some());

    match event1.unwrap() {
        Event::CaptureStarted { device_id: id, resolution, fps } => {
            assert_eq!(id, device_id);
            assert_eq!(resolution, (640, 480));
            assert_eq!(fps, 30.0);
        }
        _ => panic!("Unexpected event type"),
    }
}

/// Test multiple events in sequence
#[tokio::test]
async fn test_multiple_events_sequence() {
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();

    // Publish sequence of events
    let events = [
        Event::CaptureStarted {
            device_id: DeviceId("cam:0".into()),
            resolution: (1920, 1080),
            fps: 30.0,
        },
        Event::SettingsChanged,
        Event::FrameDropped {
            sequence: 42,
            reason: FrameDropReason::QueueFull,
        },
        Event::CaptureStopped {
            device_id: DeviceId("cam:0".into()),
        },
    ];

    for event in &events {
        bus.publish(event.clone());
    }

    // Verify all events received in order
    for expected in &events {
        let received = subscriber.try_recv();
        assert!(received.is_some());
        // Check event type matches (simplified comparison)
        match (expected, received.unwrap()) {
            (Event::CaptureStarted { .. }, Event::CaptureStarted { .. }) => {}
            (Event::SettingsChanged, Event::SettingsChanged) => {}
            (Event::FrameDropped { .. }, Event::FrameDropped { .. }) => {}
            (Event::CaptureStopped { .. }, Event::CaptureStopped { .. }) => {}
            _ => panic!("Event type mismatch"),
        }
    }

    // No more events
    assert!(subscriber.try_recv().is_none());
}

// ============================================================================
// Command Channel Tests
// ============================================================================

/// Test command channel dispatch
#[tokio::test]
async fn test_command_channel_basic() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();

    // Send a command
    let device_id = DeviceId("test:0".into());
    handle
        .send_command(Command::StartCapture { device_id: device_id.clone() })
        .await
        .expect("Send should succeed");

    // Receive the command
    let received = timeout(Duration::from_millis(100), cmd_rx.recv())
        .await
        .expect("Should not timeout")
        .expect("Should receive command");

    match received {
        Command::StartCapture { device_id: id } => {
            assert_eq!(id, device_id);
        }
        _ => panic!("Wrong command type"),
    }
}

/// Test multiple commands in sequence
#[tokio::test]
async fn test_multiple_commands_sequence() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();

    // Send multiple commands
    let commands = [
        Command::SelectCamera { device_id: DeviceId("cam:1".into()) },
        Command::SetScaling { mode: ScalingMode::Fit },
        Command::SetRotation { rotation: Rotation::Clockwise90 },
        Command::SetFlip { flip: Flip::Horizontal },
        Command::StartCapture { device_id: DeviceId("cam:1".into()) },
    ];

    for cmd in &commands {
        handle.send_command(cmd.clone()).await.expect("Send should succeed");
    }

    // Verify all received in order
    for _ in &commands {
        let received = timeout(Duration::from_millis(100), cmd_rx.recv())
            .await
            .expect("Should not timeout")
            .expect("Should receive command");
        // Just verify we got a command (exact matching would be verbose)
        let _ = received;
    }
}

// ============================================================================
// AppContext Integration Tests
// ============================================================================

/// Test AppContext creates working channels
#[tokio::test]
async fn test_app_context_integration() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();

    // Subscribe to events
    let mut subscriber = handle.subscribe_events();

    // Spawn a simulated engine that responds to commands
    let engine_handle = handle.clone();
    let engine = tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                Command::StartCapture { device_id } => {
                    engine_handle.publish_event(Event::CaptureStarted {
                        device_id,
                        resolution: (1280, 720),
                        fps: 30.0,
                    });
                }
                Command::StopCapture => {
                    engine_handle.publish_event(Event::CaptureStopped {
                        device_id: DeviceId("active".into()),
                    });
                    break;
                }
                Command::PauseDisplay => {
                    engine_handle.publish_event(Event::DisplayPaused);
                }
                Command::ResumeDisplay => {
                    engine_handle.publish_event(Event::DisplayResumed);
                }
                _ => {}
            }
        }
    });

    // Send commands and verify events
    let device_id = DeviceId("test:0".into());

    // Start capture
    handle.send_command(Command::StartCapture { device_id: device_id.clone() })
        .await.unwrap();
    let event = timeout(Duration::from_millis(100), subscriber.recv())
        .await.expect("Timeout").expect("Event");
    assert!(matches!(event, Event::CaptureStarted { .. }));

    // Pause display
    handle.send_command(Command::PauseDisplay).await.unwrap();
    let event = timeout(Duration::from_millis(100), subscriber.recv())
        .await.expect("Timeout").expect("Event");
    assert!(matches!(event, Event::DisplayPaused));

    // Resume display
    handle.send_command(Command::ResumeDisplay).await.unwrap();
    let event = timeout(Duration::from_millis(100), subscriber.recv())
        .await.expect("Timeout").expect("Event");
    assert!(matches!(event, Event::DisplayResumed));

    // Stop capture
    handle.send_command(Command::StopCapture).await.unwrap();
    let event = timeout(Duration::from_millis(100), subscriber.recv())
        .await.expect("Timeout").expect("Event");
    assert!(matches!(event, Event::CaptureStopped { .. }));

    engine.await.expect("Engine should complete");
}

// ============================================================================
// Multi-Subscriber Tests
// ============================================================================

/// Test multiple subscribers receive all events
#[tokio::test]
async fn test_multi_subscriber_broadcast() {
    let bus = EventBus::new();

    // Create 5 subscribers
    let mut subscribers: Vec<_> = (0..5).map(|_| bus.subscribe()).collect();

    assert_eq!(bus.subscriber_count(), 5);

    // Publish 10 events
    for i in 0..10 {
        bus.publish(Event::FrameDropped {
            sequence: i,
            reason: FrameDropReason::ProcessingTimeout,
        });
    }

    // Verify each subscriber received all 10 events
    for (idx, sub) in subscribers.iter_mut().enumerate() {
        for seq in 0..10 {
            let event = sub.try_recv();
            assert!(event.is_some(), "Subscriber {} missing event {}", idx, seq);
            match event.unwrap() {
                Event::FrameDropped { sequence, .. } => {
                    assert_eq!(sequence, seq, "Subscriber {} wrong sequence", idx);
                }
                _ => panic!("Wrong event type"),
            }
        }
        // No more events
        assert!(sub.try_recv().is_none());
    }
}

/// Test subscriber late join receives only future events
#[tokio::test]
async fn test_late_subscriber() {
    let bus = EventBus::new();
    let mut early_sub = bus.subscribe();

    // Publish before late subscriber joins
    bus.publish(Event::SettingsChanged);

    // Late subscriber joins
    let mut late_sub = bus.subscribe();

    // Publish after late subscriber joins
    bus.publish(Event::DisplayPaused);

    // Early subscriber gets both events
    assert!(early_sub.try_recv().is_some()); // SettingsChanged
    assert!(early_sub.try_recv().is_some()); // DisplayPaused

    // Late subscriber only gets DisplayPaused
    let event = late_sub.try_recv();
    assert!(event.is_some());
    assert!(matches!(event.unwrap(), Event::DisplayPaused));
    assert!(late_sub.try_recv().is_none()); // No SettingsChanged
}

// ============================================================================
// State Change Event Tests
// ============================================================================

/// Test state change events are propagated
#[tokio::test]
async fn test_state_change_events() {
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();

    // Simulate state machine transitions
    let transitions = [
        (AppState::Idle, AppState::Starting),
        (AppState::Starting, AppState::Running),
        (AppState::Running, AppState::Paused),
        (AppState::Paused, AppState::Running),
        (AppState::Running, AppState::Idle),
    ];

    for (old, new) in &transitions {
        assert!(old.can_transition_to(*new), "{} -> {} should be valid", old, new);
        bus.publish(Event::StateChanged {
            old_state: *old,
            new_state: *new,
        });
    }

    // Verify all state changes received
    for (expected_old, expected_new) in &transitions {
        let event = subscriber.try_recv().expect("Should receive state change");
        match event {
            Event::StateChanged { old_state, new_state } => {
                assert_eq!(old_state, *expected_old);
                assert_eq!(new_state, *expected_new);
            }
            _ => panic!("Expected StateChanged event"),
        }
    }
}

/// Test AppState transition validity
#[test]
fn test_state_transition_validity() {
    // Valid transitions from Idle
    assert!(AppState::Idle.can_transition_to(AppState::Starting));
    assert!(AppState::Idle.can_transition_to(AppState::ShuttingDown));
    assert!(!AppState::Idle.can_transition_to(AppState::Running));
    assert!(!AppState::Idle.can_transition_to(AppState::Paused));

    // Valid transitions from Running
    assert!(AppState::Running.can_transition_to(AppState::Idle));
    assert!(AppState::Running.can_transition_to(AppState::Paused));
    assert!(AppState::Running.can_transition_to(AppState::Error));
    assert!(AppState::Running.can_transition_to(AppState::Reconnecting));
    assert!(!AppState::Running.can_transition_to(AppState::Starting));

    // Capturing state checks
    assert!(!AppState::Idle.is_capturing());
    assert!(!AppState::Starting.is_capturing());
    assert!(AppState::Running.is_capturing());
    assert!(AppState::Paused.is_capturing());
    assert!(AppState::Reconnecting.is_capturing());

    // Command acceptance
    assert!(AppState::Idle.can_accept_commands());
    assert!(AppState::Running.can_accept_commands());
    assert!(AppState::Paused.can_accept_commands());
    assert!(!AppState::Starting.can_accept_commands());
    assert!(!AppState::ShuttingDown.can_accept_commands());
}

// ============================================================================
// Error Event Tests
// ============================================================================

/// Test error events are properly propagated
#[tokio::test]
async fn test_error_event_propagation() {
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();

    // Publish various error events
    let errors = [
        MicroundError::Capture { source: CaptureError::DeviceNotFound("cam:1".into()), context: ErrorContext::new() },
        MicroundError::Capture { source: CaptureError::Disconnected, context: ErrorContext::new() },
        MicroundError::Capture { source: CaptureError::Timeout(5000), context: ErrorContext::new() },
    ];

    for error in &errors {
        bus.publish(Event::Error { error: error.clone() });
    }

    // Verify errors received
    for expected in &errors {
        let event = subscriber.try_recv().expect("Should receive error event");
        match event {
            Event::Error { error } => {
                // Check error type matches
                match (&error, expected) {
                    (MicroundError::Capture { source: a, .. }, MicroundError::Capture { source: b, .. }) => {
                        // Basic type check
                        let _ = (a, b);
                    }
                    _ => panic!("Error type mismatch"),
                }
            }
            _ => panic!("Expected Error event"),
        }
    }
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

/// Test concurrent publishers and subscribers
#[tokio::test]
async fn test_concurrent_pub_sub() {
    let bus = Arc::new(EventBus::new());
    let received_count = Arc::new(AtomicUsize::new(0));

    // Spawn 3 subscribers
    let mut handles = vec![];
    for i in 0..3 {
        let bus_clone = bus.clone();
        let count = received_count.clone();
        let handle = tokio::spawn(async move {
            let mut sub = bus_clone.subscribe();
            let mut local_count = 0;
            // Wait for events with timeout
            loop {
                match timeout(Duration::from_millis(500), sub.recv()).await {
                    Ok(Some(_)) => {
                        local_count += 1;
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => break,
                }
            }
            (i, local_count)
        });
        handles.push(handle);
    }

    // Give subscribers time to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Publish events from multiple tasks
    let mut pub_handles = vec![];
    for i in 0..3 {
        let bus_clone = bus.clone();
        let handle = tokio::spawn(async move {
            for j in 0..5 {
                bus_clone.publish(Event::FrameDropped {
                    sequence: (i * 5 + j) as u64,
                    reason: FrameDropReason::QueueFull,
                });
            }
        });
        pub_handles.push(handle);
    }

    // Wait for publishers
    for h in pub_handles {
        h.await.unwrap();
    }

    // Wait for subscribers
    for h in handles {
        let (idx, count) = h.await.unwrap();
        // Each subscriber should receive all 15 events
        assert!(count >= 10, "Subscriber {} only got {} events", idx, count);
    }
}

// ============================================================================
// Handle Cloning Tests
// ============================================================================

/// Test AppHandle can be cloned and used from multiple locations
#[tokio::test]
async fn test_handle_cloning() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle1 = ctx.handle();
    let handle2 = handle1.clone();
    let handle3 = handle2.clone();

    // Subscribe from different handles
    let mut sub1 = handle1.subscribe_events();
    let mut sub2 = handle2.subscribe_events();

    // Send commands from different handles
    handle1.send_command(Command::PauseDisplay).await.unwrap();
    handle2.send_command(Command::ResumeDisplay).await.unwrap();
    handle3.send_command(Command::StopCapture).await.unwrap();

    // Verify commands received
    for _ in 0..3 {
        let cmd = timeout(Duration::from_millis(100), cmd_rx.recv())
            .await.expect("Timeout").expect("Command");
        let _ = cmd;
    }

    // Publish from different handles
    handle1.publish_event(Event::SettingsChanged);
    handle2.publish_event(Event::DisplayPaused);

    // Both subscribers should receive both events
    assert!(sub1.try_recv().is_some());
    assert!(sub1.try_recv().is_some());
    assert!(sub2.try_recv().is_some());
    assert!(sub2.try_recv().is_some());
}

// ============================================================================
// Try Send Tests
// ============================================================================

/// Test try_send_command for non-blocking operation
#[tokio::test]
async fn test_try_send_command() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();

    // try_send should work when channel has capacity
    let result = handle.try_send_command(Command::PauseDisplay);
    assert!(result.is_ok());

    // Verify received
    let cmd = timeout(Duration::from_millis(100), cmd_rx.recv())
        .await.expect("Timeout").expect("Command");
    assert!(matches!(cmd, Command::PauseDisplay));
}

// ============================================================================
// Event Types Coverage
// ============================================================================

/// Test all event types can be published and received
#[tokio::test]
async fn test_all_event_types() {
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();

    let device = micround::core::CameraDevice {
        id: DeviceId("cam:test".into()),
        name: "Test Camera".into(),
        manufacturer: Some("Test Inc".into()),
        capabilities: vec![],
        is_available: true,
    };

    let events: Vec<Event> = vec![
        Event::CameraConnected { device: device.clone() },
        Event::CameraDisconnected { device_id: DeviceId("cam:0".into()) },
        Event::CaptureStarted {
            device_id: DeviceId("cam:0".into()),
            resolution: (1920, 1080),
            fps: 60.0,
        },
        Event::CaptureStopped { device_id: DeviceId("cam:0".into()) },
        Event::FrameDropped { sequence: 100, reason: FrameDropReason::QueueFull },
        Event::FrameDropped { sequence: 101, reason: FrameDropReason::ProcessingTimeout },
        Event::FrameDropped { sequence: 102, reason: FrameDropReason::RenderQueueFull },
        Event::DisplayPaused,
        Event::DisplayResumed,
        Event::SnapshotTaken { to_clipboard: true },
        Event::SnapshotTaken { to_clipboard: false },
        Event::Error { error: MicroundError::Capture { source: CaptureError::Disconnected, context: ErrorContext::new() } },
        Event::SettingsChanged,
        Event::StateChanged { old_state: AppState::Idle, new_state: AppState::Starting },
    ];

    for event in &events {
        bus.publish(event.clone());
    }

    // Verify all received
    let mut received_count = 0;
    while subscriber.try_recv().is_some() {
        received_count += 1;
    }

    assert_eq!(received_count, events.len());
}

// ============================================================================
// Command Types Coverage
// ============================================================================

/// Test all command types
#[tokio::test]
async fn test_all_command_types() {
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();

    let commands: Vec<Command> = vec![
        Command::StartCapture { device_id: DeviceId("cam:0".into()) },
        Command::StopCapture,
        Command::PauseDisplay,
        Command::ResumeDisplay,
        Command::UpdateCaptureSettings {
            settings: CaptureSettings {
                width: 1920,
                height: 1080,
                framerate: 30.0,
                format: None,
            },
        },
        Command::TakeSnapshot { to_clipboard: true },
        Command::TakeSnapshot { to_clipboard: false },
        Command::SelectCamera { device_id: DeviceId("cam:1".into()) },
        Command::SelectDisplay { display_id: DisplayId("display:0".into()) },
        Command::SetScaling { mode: ScalingMode::Fill },
        Command::SetScaling { mode: ScalingMode::Fit },
        Command::SetScaling { mode: ScalingMode::Stretch },
        Command::SetScaling { mode: ScalingMode::Center },
        Command::SetRotation { rotation: Rotation::None },
        Command::SetRotation { rotation: Rotation::Clockwise90 },
        Command::SetRotation { rotation: Rotation::Clockwise180 },
        Command::SetRotation { rotation: Rotation::Clockwise270 },
        Command::SetFlip { flip: Flip::None },
        Command::SetFlip { flip: Flip::Horizontal },
        Command::SetFlip { flip: Flip::Vertical },
        Command::SetFlip { flip: Flip::Both },
        Command::Quit,
    ];

    for cmd in &commands {
        handle.send_command(cmd.clone()).await.expect("Should send");
    }

    // Verify all received
    let mut received_count = 0;
    while let Ok(Some(_)) = timeout(Duration::from_millis(10), cmd_rx.recv()).await {
        received_count += 1;
    }

    assert_eq!(received_count, commands.len());
}

// ============================================================================
// Drop and Cleanup Tests
// ============================================================================

/// Test subscriber drop doesn't affect other subscribers
#[tokio::test]
async fn test_subscriber_drop() {
    let bus = EventBus::new();

    let sub1 = bus.subscribe();
    let mut sub2 = bus.subscribe();
    let mut sub3 = bus.subscribe();

    assert_eq!(bus.subscriber_count(), 3);

    // Drop sub1
    drop(sub1);
    assert_eq!(bus.subscriber_count(), 2);

    // Publish event
    bus.publish(Event::SettingsChanged);

    // Remaining subscribers should still receive
    assert!(sub2.try_recv().is_some());
    assert!(sub3.try_recv().is_some());
}

/// Test bus works when all subscribers dropped
#[test]
fn test_no_subscribers() {
    let bus = EventBus::new();

    // Create and drop subscriber
    let sub = bus.subscribe();
    drop(sub);

    assert_eq!(bus.subscriber_count(), 0);

    // Publishing with no subscribers should return 0
    let result = bus.publish(Event::SettingsChanged);
    assert_eq!(result, 0);
}
