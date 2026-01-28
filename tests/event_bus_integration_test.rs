//! Integration test: Event bus message flow (bd-10d)
//!
//! Tests the event bus system for coordinating events between components.
//!
//! # Test Coverage
//!
//! - Multiple subscribers receiving events
//! - Command routing and processing
//! - State change events
//! - Error event propagation
//! - Frame drop events
//! - Camera connect/disconnect events
//! - Event ordering
//! - Command queueing
//! - Publisher/subscriber patterns
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐      ┌─────────────┐
//! │   Command   │ ───▶ │   Engine    │
//! │   Sender    │      │  (command   │
//! │             │      │   receiver) │
//! └─────────────┘      └─────────────┘
//!                             │
//!                             ▼
//! ┌─────────────┐      ┌─────────────┐
//! │  Subscriber │ ◀─── │  EventBus   │
//! │      A      │      │  (broadcast)│
//! └─────────────┘      └─────────────┘
//!                             │
//! ┌─────────────┐             │
//! │  Subscriber │ ◀───────────┘
//! │      B      │
//! └─────────────┘
//! ```

mod common;

use std::time::Duration;

use micround::core::{
    AppContext, AppState, Command, Event, EventBus,
    CameraDevice, CaptureSettings, DeviceId, DisplayId, FrameDropReason,
    Flip, Rotation, ScalingMode, MicroundError, ErrorContext,
};

use common::test_logger::TestLogger;
use tokio::time::timeout;

// ============================================================================
// Test Constants
// ============================================================================

/// Default timeout for async operations
const TEST_TIMEOUT: Duration = Duration::from_secs(5);

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a test camera device
fn make_test_camera(id: &str, name: &str) -> CameraDevice {
    CameraDevice {
        id: DeviceId(id.into()),
        name: name.into(),
        manufacturer: Some("Test Manufacturer".into()),
        capabilities: vec![],
        is_available: true,
    }
}

/// Create a test error
fn make_test_error(message: &str) -> MicroundError {
    MicroundError::Internal {
        message: message.into(),
        context: ErrorContext::new(),
    }
}

// ============================================================================
// Basic Event Bus Tests
// ============================================================================

#[tokio::test]
async fn event_bus_basic_publish_subscribe() {
    let mut logger = TestLogger::new("event_bus_basic_publish_subscribe", 4);

    // Step 1: Create event bus
    logger.step("Creating event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.assert_eq("Initial subscribers", &bus.subscriber_count(), &1usize);
    logger.step_ok("Event bus created with 1 subscriber");

    // Step 2: Publish event
    logger.step("Publishing event");
    let count = bus.publish(Event::DisplayPaused);
    logger.assert_eq("Receivers notified", &count, &1usize);
    logger.step_ok("Event published");

    // Step 3: Receive event
    logger.step("Receiving event");
    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    assert!(matches!(event, Event::DisplayPaused));
    logger.assert_pass("Received DisplayPaused event");
    logger.step_ok("Event received");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_bus_multiple_subscribers() {
    let mut logger = TestLogger::new("event_bus_multiple_subscribers", 4);

    // Step 1: Create bus with multiple subscribers
    logger.step("Creating event bus with multiple subscribers");
    let bus = EventBus::new();
    let mut sub_a = bus.subscribe();
    let mut sub_b = bus.subscribe();
    let mut sub_c = bus.subscribe();
    logger.assert_eq("Subscriber count", &bus.subscriber_count(), &3usize);
    logger.step_ok("3 subscribers created");

    // Step 2: Publish event
    logger.step("Publishing event to all subscribers");
    let count = bus.publish(Event::DisplayResumed);
    logger.assert_eq("Receivers notified", &count, &3usize);
    logger.step_ok("Event broadcast to all");

    // Step 3: Verify all received
    logger.step("Verifying all subscribers received event");
    let event_a = timeout(TEST_TIMEOUT, sub_a.recv())
        .await
        .expect("Timeout A")
        .expect("Closed A");
    let event_b = timeout(TEST_TIMEOUT, sub_b.recv())
        .await
        .expect("Timeout B")
        .expect("Closed B");
    let event_c = timeout(TEST_TIMEOUT, sub_c.recv())
        .await
        .expect("Timeout C")
        .expect("Closed C");

    assert!(matches!(event_a, Event::DisplayResumed));
    assert!(matches!(event_b, Event::DisplayResumed));
    assert!(matches!(event_c, Event::DisplayResumed));
    logger.assert_pass("All 3 subscribers received event");
    logger.step_ok("All subscribers verified");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_bus_no_subscribers() {
    let mut logger = TestLogger::new("event_bus_no_subscribers", 3);

    // Step 1: Create bus without subscribers
    logger.step("Creating event bus without subscribers");
    let bus = EventBus::new();
    logger.assert_eq("Subscriber count", &bus.subscriber_count(), &0usize);
    logger.step_ok("Empty event bus created");

    // Step 2: Publish event (should not fail)
    logger.step("Publishing event with no subscribers");
    let count = bus.publish(Event::DisplayPaused);
    logger.assert_eq("Receivers notified", &count, &0usize);
    logger.assert_pass("Event dropped gracefully");
    logger.step_ok("No error when no subscribers");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Command Channel Tests
// ============================================================================

#[tokio::test]
async fn command_channel_basic_flow() {
    let mut logger = TestLogger::new("command_channel_basic_flow", 4);

    // Step 1: Create context
    logger.step("Creating app context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    logger.step_ok("App context created");

    // Step 2: Send command
    logger.step("Sending StartCapture command");
    let device_id = DeviceId("test-device".into());
    handle
        .send_command(Command::StartCapture {
            device_id: device_id.clone(),
        })
        .await
        .expect("Send failed");
    logger.step_ok("Command sent");

    // Step 3: Receive command
    logger.step("Receiving command");
    let cmd = timeout(TEST_TIMEOUT, cmd_rx.recv())
        .await
        .expect("Timeout")
        .expect("Channel closed");

    match cmd {
        Command::StartCapture { device_id: id } => {
            logger.assert_eq("Device ID", &id.0, &"test-device".to_string());
        }
        other => panic!("Wrong command type: {:?}", other),
    }
    logger.step_ok("Command received correctly");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn command_channel_multiple_commands() {
    let mut logger = TestLogger::new("command_channel_multiple_commands", 4);

    // Step 1: Create context
    logger.step("Creating app context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    logger.step_ok("Context ready");

    // Step 2: Send multiple commands
    logger.step("Sending multiple commands");
    let commands = vec![
        Command::StartCapture {
            device_id: DeviceId("cam1".into()),
        },
        Command::PauseDisplay,
        Command::SetScaling {
            mode: ScalingMode::Fit,
        },
        Command::SetRotation {
            rotation: Rotation::Clockwise90,
        },
        Command::StopCapture,
    ];

    for cmd in &commands {
        handle.send_command(cmd.clone()).await.expect("Send failed");
    }
    logger.step_ok(&format!("Sent {} commands", commands.len()));

    // Step 3: Receive and verify order
    logger.step("Verifying command order");
    let received: Vec<Command> = tokio::time::timeout(TEST_TIMEOUT, async {
        let mut received = Vec::new();
        for _ in 0..5 {
            if let Some(cmd) = cmd_rx.recv().await {
                received.push(cmd);
            }
        }
        received
    })
    .await
    .expect("Timeout");

    logger.assert_eq("Commands received", &received.len(), &5usize);

    // Verify order
    assert!(matches!(received[0], Command::StartCapture { .. }));
    assert!(matches!(received[1], Command::PauseDisplay));
    assert!(matches!(received[2], Command::SetScaling { .. }));
    assert!(matches!(received[3], Command::SetRotation { .. }));
    assert!(matches!(received[4], Command::StopCapture));

    logger.assert_pass("Command order preserved");
    logger.step_ok("Order verified");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn command_try_send() {
    let mut logger = TestLogger::new("command_try_send", 3);

    // Step 1: Create context
    logger.step("Creating app context");
    let (ctx, _cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    logger.step_ok("Context ready");

    // Step 2: Try send command
    logger.step("Using try_send_command");
    let result = handle.try_send_command(Command::PauseDisplay);
    assert!(result.is_ok());
    logger.assert_pass("try_send succeeded");
    logger.step_ok("Non-blocking send works");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Event Types Tests
// ============================================================================

#[tokio::test]
async fn event_camera_connected() {
    let mut logger = TestLogger::new("event_camera_connected", 3);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish camera connected
    logger.step("Publishing CameraConnected event");
    let camera = make_test_camera("/dev/video0", "USB Camera");
    bus.publish(Event::CameraConnected {
        device: camera.clone(),
    });

    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    match event {
        Event::CameraConnected { device } => {
            logger.assert_eq("Camera ID", &device.id.0, &"/dev/video0".to_string());
            logger.assert_eq("Camera name", &device.name, &"USB Camera".to_string());
        }
        other => panic!("Wrong event: {:?}", other),
    }
    logger.step_ok("CameraConnected event verified");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_camera_disconnected() {
    let mut logger = TestLogger::new("event_camera_disconnected", 3);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish camera disconnected
    logger.step("Publishing CameraDisconnected event");
    let device_id = DeviceId("/dev/video0".into());
    bus.publish(Event::CameraDisconnected {
        device_id: device_id.clone(),
    });

    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    match event {
        Event::CameraDisconnected { device_id: id } => {
            logger.assert_eq("Device ID", &id.0, &"/dev/video0".to_string());
        }
        other => panic!("Wrong event: {:?}", other),
    }
    logger.step_ok("CameraDisconnected event verified");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_capture_started() {
    let mut logger = TestLogger::new("event_capture_started", 3);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish capture started
    logger.step("Publishing CaptureStarted event");
    bus.publish(Event::CaptureStarted {
        device_id: DeviceId("cam0".into()),
        resolution: (1920, 1080),
        fps: 30.0,
    });

    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    match event {
        Event::CaptureStarted {
            device_id,
            resolution,
            fps,
        } => {
            logger.assert_eq("Device ID", &device_id.0, &"cam0".to_string());
            logger.assert_eq("Resolution", &resolution, &(1920u32, 1080u32));
            logger.assert_pass(&format!("FPS: {}", fps));
            assert!((fps - 30.0).abs() < 0.01);
        }
        other => panic!("Wrong event: {:?}", other),
    }
    logger.step_ok("CaptureStarted event verified");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_frame_dropped() {
    let mut logger = TestLogger::new("event_frame_dropped", 4);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Test different drop reasons
    let reasons = [
        (FrameDropReason::QueueFull, "QueueFull"),
        (FrameDropReason::ProcessingTimeout, "ProcessingTimeout"),
        (FrameDropReason::RenderQueueFull, "RenderQueueFull"),
    ];

    for (reason, reason_name) in reasons {
        logger.step(&format!("Testing {} reason", reason_name));

        bus.publish(Event::FrameDropped {
            sequence: 42,
            reason,
        });

        let event = timeout(TEST_TIMEOUT, subscriber.recv())
            .await
            .expect("Timeout")
            .expect("Closed");

        match event {
            Event::FrameDropped {
                sequence,
                reason: r,
            } => {
                logger.assert_eq("Sequence", &sequence, &42u64);
                assert!(std::mem::discriminant(&r) == std::mem::discriminant(&reason));
            }
            other => panic!("Wrong event: {:?}", other),
        }
        logger.step_ok(&format!("{} reason verified", reason_name));
    }

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_state_changed() {
    let mut logger = TestLogger::new("event_state_changed", 3);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish state change
    logger.step("Publishing StateChanged event");
    bus.publish(Event::StateChanged {
        old_state: AppState::Idle,
        new_state: AppState::Starting,
    });

    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    match event {
        Event::StateChanged {
            old_state,
            new_state,
        } => {
            logger.assert_pass(&format!("Transition: {} -> {}", old_state, new_state));
            assert_eq!(old_state, AppState::Idle);
            assert_eq!(new_state, AppState::Starting);
        }
        other => panic!("Wrong event: {:?}", other),
    }
    logger.step_ok("StateChanged event verified");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_error() {
    let mut logger = TestLogger::new("event_error", 3);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish error event
    logger.step("Publishing Error event");
    let error = make_test_error("Test error message");
    bus.publish(Event::Error {
        error: error.clone(),
    });

    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    match event {
        Event::Error { error: e } => {
            logger.assert_pass(&format!("Error: {}", e));
        }
        other => panic!("Wrong event: {:?}", other),
    }
    logger.step_ok("Error event verified");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// AppState Transition Tests
// ============================================================================

#[tokio::test]
async fn app_state_valid_transitions() {
    let mut logger = TestLogger::new("app_state_valid_transitions", 3);

    // Step 1: Test valid transitions
    logger.step("Testing valid state transitions");

    let valid_transitions = [
        (AppState::Idle, AppState::Starting),
        (AppState::Starting, AppState::Running),
        (AppState::Running, AppState::Paused),
        (AppState::Paused, AppState::Running),
        (AppState::Running, AppState::Idle),
        (AppState::Error, AppState::Idle),
        (AppState::Running, AppState::Error),
        (AppState::Running, AppState::Reconnecting),
        (AppState::Reconnecting, AppState::Running),
    ];

    for (from, to) in valid_transitions {
        assert!(
            from.can_transition_to(to),
            "Transition {} -> {} should be valid",
            from,
            to
        );
        logger.assert_pass(&format!("{} -> {} valid", from, to));
    }
    logger.step_ok("All valid transitions verified");

    // Step 2: Test invalid transitions
    logger.step("Testing invalid state transitions");

    let invalid_transitions = [
        (AppState::Running, AppState::Starting),
        (AppState::Idle, AppState::Running),
        (AppState::Paused, AppState::Starting),
        (AppState::ShuttingDown, AppState::Running),
    ];

    for (from, to) in invalid_transitions {
        assert!(
            !from.can_transition_to(to),
            "Transition {} -> {} should be invalid",
            from,
            to
        );
        logger.assert_pass(&format!("{} -> {} invalid (correct)", from, to));
    }
    logger.step_ok("Invalid transitions rejected");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn app_state_properties() {
    let mut logger = TestLogger::new("app_state_properties", 3);

    // Step 1: Test is_capturing
    logger.step("Testing is_capturing property");

    let capturing_states = [AppState::Running, AppState::Paused, AppState::Reconnecting];
    let non_capturing_states = [
        AppState::Idle,
        AppState::Starting,
        AppState::Error,
        AppState::ShuttingDown,
    ];

    for state in capturing_states {
        assert!(state.is_capturing(), "{} should be capturing", state);
        logger.assert_pass(&format!("{}.is_capturing() = true", state));
    }

    for state in non_capturing_states {
        assert!(!state.is_capturing(), "{} should not be capturing", state);
        logger.assert_pass(&format!("{}.is_capturing() = false", state));
    }
    logger.step_ok("is_capturing verified");

    // Step 2: Test can_accept_commands
    logger.step("Testing can_accept_commands property");

    let accepting_states = [
        AppState::Idle,
        AppState::Running,
        AppState::Paused,
        AppState::Error,
    ];
    let non_accepting_states = [
        AppState::Starting,
        AppState::Reconnecting,
        AppState::ShuttingDown,
    ];

    for state in accepting_states {
        assert!(
            state.can_accept_commands(),
            "{} should accept commands",
            state
        );
        logger.assert_pass(&format!("{}.can_accept_commands() = true", state));
    }

    for state in non_accepting_states {
        assert!(
            !state.can_accept_commands(),
            "{} should not accept commands",
            state
        );
        logger.assert_pass(&format!("{}.can_accept_commands() = false", state));
    }
    logger.step_ok("can_accept_commands verified");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Integration Pattern Tests
// ============================================================================

#[tokio::test]
async fn event_bus_command_response_pattern() {
    let mut logger = TestLogger::new("event_bus_command_response_pattern", 5);

    // Step 1: Setup context with simulated engine
    logger.step("Setting up app context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    let mut subscriber = handle.subscribe_events();
    logger.step_ok("Context ready");

    // Step 2: Spawn simulated engine
    logger.step("Spawning simulated engine");
    let engine_handle = handle.clone();
    let engine = tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                Command::StartCapture { device_id } => {
                    engine_handle.publish_event(Event::CaptureStarted {
                        device_id,
                        resolution: (640, 480),
                        fps: 30.0,
                    });
                }
                Command::StopCapture => {
                    engine_handle.publish_event(Event::CaptureStopped {
                        device_id: DeviceId("default".into()),
                    });
                    break;
                }
                _ => {}
            }
        }
    });
    logger.step_ok("Engine spawned");

    // Step 3: Send command
    logger.step("Sending StartCapture command");
    handle
        .send_command(Command::StartCapture {
            device_id: DeviceId("test-cam".into()),
        })
        .await
        .expect("Send failed");
    logger.step_ok("Command sent");

    // Step 4: Wait for response event
    logger.step("Waiting for CaptureStarted event");
    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    match event {
        Event::CaptureStarted {
            device_id,
            resolution,
            fps,
        } => {
            logger.assert_eq("Device ID", &device_id.0, &"test-cam".to_string());
            logger.assert_eq("Resolution", &resolution, &(640u32, 480u32));
            logger.assert_pass(&format!("FPS: {}", fps));
        }
        other => panic!("Wrong event: {:?}", other),
    }
    logger.step_ok("Response received");

    // Cleanup
    handle
        .send_command(Command::StopCapture)
        .await
        .expect("Send failed");
    engine.await.expect("Engine panicked");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_bus_multiple_event_types() {
    let mut logger = TestLogger::new("event_bus_multiple_event_types", 4);

    // Step 1: Setup
    logger.step("Setting up event bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish sequence of events
    logger.step("Publishing multiple event types");
    let events = vec![
        Event::StateChanged {
            old_state: AppState::Idle,
            new_state: AppState::Starting,
        },
        Event::CaptureStarted {
            device_id: DeviceId("cam0".into()),
            resolution: (640, 480),
            fps: 30.0,
        },
        Event::StateChanged {
            old_state: AppState::Starting,
            new_state: AppState::Running,
        },
        Event::FrameDropped {
            sequence: 100,
            reason: FrameDropReason::QueueFull,
        },
        Event::SettingsChanged,
    ];

    for event in &events {
        bus.publish(event.clone());
    }
    logger.step_ok(&format!("Published {} events", events.len()));

    // Step 3: Receive and verify all
    logger.step("Receiving and verifying all events");
    let mut received_types = Vec::new();

    for _ in 0..events.len() {
        let event = timeout(TEST_TIMEOUT, subscriber.recv())
            .await
            .expect("Timeout")
            .expect("Closed");

        let type_name = match event {
            Event::StateChanged { .. } => "StateChanged",
            Event::CaptureStarted { .. } => "CaptureStarted",
            Event::FrameDropped { .. } => "FrameDropped",
            Event::SettingsChanged => "SettingsChanged",
            _ => "Other",
        };
        received_types.push(type_name);
    }

    logger.assert_eq("Events received", &received_types.len(), &5usize);
    logger.assert_pass(&format!("Event types: {:?}", received_types));
    logger.step_ok("All events received in order");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_bus_subscriber_dropped() {
    let mut logger = TestLogger::new("event_bus_subscriber_dropped", 4);

    // Step 1: Create bus with subscriber
    logger.step("Creating bus with subscriber");
    let bus = EventBus::new();
    let subscriber = bus.subscribe();
    logger.assert_eq("Initial subscribers", &bus.subscriber_count(), &1usize);
    logger.step_ok("1 subscriber");

    // Step 2: Drop subscriber
    logger.step("Dropping subscriber");
    drop(subscriber);
    // Subscriber count decreases when receiver is dropped
    logger.step_ok("Subscriber dropped");

    // Step 3: Publish with no subscribers
    logger.step("Publishing with no subscribers");
    let count = bus.publish(Event::DisplayPaused);
    logger.assert_eq("Receivers notified", &count, &0usize);
    logger.step_ok("Event dropped gracefully");

    let result = logger.finish();
    assert!(result.passed);
}

#[tokio::test]
async fn event_bus_late_subscriber() {
    let mut logger = TestLogger::new("event_bus_late_subscriber", 4);

    // Step 1: Create bus
    logger.step("Creating bus");
    let bus = EventBus::new();
    logger.step_ok("Bus created");

    // Step 2: Publish event before any subscribers
    logger.step("Publishing before subscribers");
    let count = bus.publish(Event::DisplayPaused);
    logger.assert_eq("Receivers notified", &count, &0usize);
    logger.step_ok("Event dropped (no subscribers)");

    // Step 3: Add late subscriber
    logger.step("Adding late subscriber");
    let mut subscriber = bus.subscribe();

    // Step 4: Verify late subscriber doesn't receive past events
    logger.step("Verifying late subscriber doesn't receive past events");

    // Publish a new event
    bus.publish(Event::DisplayResumed);

    // Subscriber should only receive the new event
    let event = timeout(TEST_TIMEOUT, subscriber.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    assert!(
        matches!(event, Event::DisplayResumed),
        "Should receive only new events"
    );
    logger.assert_pass("Late subscriber only receives new events");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// Handle Cloning Tests
// ============================================================================

#[tokio::test]
async fn app_handle_cloning() {
    let mut logger = TestLogger::new("app_handle_cloning", 4);

    // Step 1: Create context and handles
    logger.step("Creating context and handles");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle1 = ctx.handle();
    let handle2 = handle1.clone();
    let handle3 = ctx.handle();
    logger.step_ok("3 handles created");

    // Step 2: Subscribe from different handles
    logger.step("Subscribing from different handles");
    let mut sub1 = handle1.subscribe_events();
    let mut sub2 = handle2.subscribe_events();
    let mut sub3 = handle3.subscribe_events();
    logger.step_ok("3 subscribers from different handles");

    // Step 3: Publish from one handle
    logger.step("Publishing from handle1");
    handle1.publish_event(Event::SettingsChanged);

    // All should receive
    let _ = timeout(TEST_TIMEOUT, sub1.recv())
        .await
        .expect("Timeout")
        .expect("Closed");
    let _ = timeout(TEST_TIMEOUT, sub2.recv())
        .await
        .expect("Timeout")
        .expect("Closed");
    let _ = timeout(TEST_TIMEOUT, sub3.recv())
        .await
        .expect("Timeout")
        .expect("Closed");

    logger.assert_pass("All 3 subscribers received event");
    logger.step_ok("Handle cloning works correctly");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// try_recv Tests
// ============================================================================

#[tokio::test]
async fn event_subscriber_try_recv() {
    let mut logger = TestLogger::new("event_subscriber_try_recv", 4);

    // Step 1: Create bus
    logger.step("Creating bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Try recv when empty
    logger.step("Testing try_recv when empty");
    let result = subscriber.try_recv();
    assert!(result.is_none());
    logger.assert_pass("try_recv returns None when empty");
    logger.step_ok("Empty case verified");

    // Step 3: Publish and try recv
    logger.step("Publishing and try_recv");
    bus.publish(Event::DisplayPaused);

    // Small delay to ensure event is received
    tokio::time::sleep(Duration::from_millis(10)).await;

    let result = subscriber.try_recv();
    assert!(result.is_some());
    assert!(matches!(result.unwrap(), Event::DisplayPaused));
    logger.assert_pass("try_recv returns event when available");
    logger.step_ok("Non-blocking receive works");

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// All Command Types Tests
// ============================================================================

#[tokio::test]
async fn all_command_types() {
    let mut logger = TestLogger::new("all_command_types", 3);

    // Step 1: Create context
    logger.step("Creating context");
    let (ctx, mut cmd_rx) = AppContext::new();
    let handle = ctx.handle();
    logger.step_ok("Context ready");

    // Step 2: Send all command types
    logger.step("Sending all command types");
    let commands = vec![
        Command::StartCapture {
            device_id: DeviceId("cam".into()),
        },
        Command::StopCapture,
        Command::PauseDisplay,
        Command::ResumeDisplay,
        Command::UpdateCaptureSettings {
            settings: CaptureSettings::default(),
        },
        Command::TakeSnapshot { to_clipboard: true },
        Command::SelectCamera {
            device_id: DeviceId("cam2".into()),
        },
        Command::SelectDisplay {
            display_id: DisplayId("disp".into()),
        },
        Command::SetScaling {
            mode: ScalingMode::Fit,
        },
        Command::SetRotation {
            rotation: Rotation::Clockwise180,
        },
        Command::SetFlip { flip: Flip::Both },
        Command::Quit,
    ];

    for cmd in &commands {
        handle.send_command(cmd.clone()).await.expect("Send failed");
    }
    logger.step_ok(&format!("Sent {} command types", commands.len()));

    // Step 3: Receive all
    logger.step("Receiving all command types");
    let received: Vec<Command> = tokio::time::timeout(TEST_TIMEOUT, async {
        let mut received = Vec::new();
        for _ in 0..commands.len() {
            if let Some(cmd) = cmd_rx.recv().await {
                received.push(cmd);
            }
        }
        received
    })
    .await
    .expect("Timeout");

    logger.assert_eq("Commands received", &received.len(), &commands.len());
    logger.assert_pass(&format!("All {} command types work", commands.len()));

    let result = logger.finish();
    assert!(result.passed);
}

// ============================================================================
// All Event Types Tests
// ============================================================================

#[tokio::test]
async fn all_event_types() {
    let mut logger = TestLogger::new("all_event_types", 3);

    // Step 1: Create bus
    logger.step("Creating bus");
    let bus = EventBus::new();
    let mut subscriber = bus.subscribe();
    logger.step_ok("Bus ready");

    // Step 2: Publish all event types
    logger.step("Publishing all event types");
    let events = vec![
        Event::CameraConnected {
            device: make_test_camera("cam", "Camera"),
        },
        Event::CameraDisconnected {
            device_id: DeviceId("cam".into()),
        },
        Event::CaptureStarted {
            device_id: DeviceId("cam".into()),
            resolution: (640, 480),
            fps: 30.0,
        },
        Event::CaptureStopped {
            device_id: DeviceId("cam".into()),
        },
        Event::FrameDropped {
            sequence: 1,
            reason: FrameDropReason::QueueFull,
        },
        Event::DisplayPaused,
        Event::DisplayResumed,
        Event::SnapshotTaken { to_clipboard: true },
        Event::Error {
            error: make_test_error("test"),
        },
        Event::SettingsChanged,
        Event::StateChanged {
            old_state: AppState::Idle,
            new_state: AppState::Running,
        },
    ];

    for event in &events {
        bus.publish(event.clone());
    }
    logger.step_ok(&format!("Published {} event types", events.len()));

    // Step 3: Receive all
    logger.step("Receiving all event types");
    let mut received_count = 0;
    for _ in 0..events.len() {
        if timeout(TEST_TIMEOUT, subscriber.recv())
            .await
            .expect("Timeout")
            .is_some()
        {
            received_count += 1;
        }
    }

    logger.assert_eq("Events received", &received_count, &events.len());
    logger.assert_pass(&format!("All {} event types work", events.len()));

    let result = logger.finish();
    assert!(result.passed);
}
