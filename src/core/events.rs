//! Internal event/message bus for Micround
//!
//! Provides type-safe, thread-safe communication between components:
#![allow(dead_code)] // Full event system API
//! - Commands: Synchronous dispatch from UI to components
//! - Events: Asynchronous notification from components to UI
//!
//! Design principles:
//! - No unbounded queues (bounded channels with backpressure)
//! - Thread-safe (all types implement Send)
//! - Type-safe (strongly typed messages)
//! - No global mutable state

use tokio::sync::{broadcast, mpsc};

use super::error::MicroundError;
use super::types::{CameraDevice, CaptureSettings, DeviceId, DisplayId, Flip, Rotation, ScalingMode};

// ============================================================================
// Commands (UI → Components)
// ============================================================================

/// Commands sent from UI to the capture/render subsystems
#[derive(Debug, Clone)]
pub enum Command {
    /// Start capturing from the specified camera
    StartCapture { device_id: DeviceId },
    /// Stop capturing
    StopCapture,
    /// Pause display (freeze current frame)
    PauseDisplay,
    /// Resume display from pause
    ResumeDisplay,
    /// Update capture settings
    UpdateCaptureSettings { settings: CaptureSettings },
    /// Take a snapshot
    TakeSnapshot { to_clipboard: bool },
    /// Select a different camera
    SelectCamera { device_id: DeviceId },
    /// Select target display
    SelectDisplay { display_id: DisplayId },
    /// Update scaling mode
    SetScaling { mode: ScalingMode },
    /// Update rotation
    SetRotation { rotation: Rotation },
    /// Update flip
    SetFlip { flip: Flip },
    /// Refresh camera list
    RefreshCameras,
    /// Show settings window
    ShowSettings,
    /// Start camera preview in settings window (bd-37z)
    StartPreview {
        /// Preview width (typically smaller than capture)
        width: u32,
        /// Preview height
        height: u32,
    },
    /// Stop camera preview in settings window
    StopPreview,
    /// Quit the application
    Quit,
}

/// Channel capacity for command queue
const COMMAND_CHANNEL_CAPACITY: usize = 32;

/// Sender half of the command channel
pub type CommandSender = mpsc::Sender<Command>;

/// Receiver half of the command channel
pub type CommandReceiver = mpsc::Receiver<Command>;

/// Create a new command channel
pub fn command_channel() -> (CommandSender, CommandReceiver) {
    mpsc::channel(COMMAND_CHANNEL_CAPACITY)
}

// ============================================================================
// Events (Components → UI)
// ============================================================================

/// Events broadcast from components to subscribers (UI, logging, etc.)
#[derive(Debug, Clone)]
pub enum Event {
    /// A camera was connected
    CameraConnected { device: CameraDevice },
    /// A camera was disconnected
    CameraDisconnected { device_id: DeviceId },
    /// Capture has started
    CaptureStarted {
        device_id: DeviceId,
        resolution: (u32, u32),
        fps: f32,
    },
    /// Capture has stopped
    CaptureStopped { device_id: DeviceId },
    /// A frame was dropped (queue full or timeout)
    FrameDropped {
        sequence: u64,
        reason: FrameDropReason,
    },
    /// Display was paused
    DisplayPaused,
    /// Display was resumed
    DisplayResumed,
    /// Snapshot was taken
    SnapshotTaken { to_clipboard: bool },
    /// An error occurred
    Error { error: MicroundError },
    /// Settings were changed
    SettingsChanged,
    /// Application state changed
    StateChanged { old_state: AppState, new_state: AppState },
    /// Camera was reconnected after disconnection
    CameraReconnected { device_id: DeviceId },
    /// Camera reconnection failed
    CameraReconnectionFailed { device_id: DeviceId },
    /// Camera reconnection timed out
    CameraReconnectionTimedOut { device_id: DeviceId },
}

/// Reasons a frame might be dropped
#[derive(Debug, Clone, Copy)]
pub enum FrameDropReason {
    /// Processing queue was full
    QueueFull,
    /// Processing took too long
    ProcessingTimeout,
    /// Render queue was full
    RenderQueueFull,
}

/// Application state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    /// Initial state, not capturing
    Idle,
    /// Starting capture
    Starting,
    /// Actively capturing and displaying
    Running,
    /// Display paused (still capturing)
    Paused,
    /// Reconnecting after disconnect
    Reconnecting,
    /// Error state
    Error,
    /// Shutting down
    ShuttingDown,
}

impl AppState {
    /// Check if a transition from self to target is valid
    pub fn can_transition_to(&self, target: AppState) -> bool {
        matches!(
            (self, target),
            // From Idle
            (AppState::Idle, AppState::Starting)
                | (AppState::Idle, AppState::ShuttingDown)
                // From Starting
                | (AppState::Starting, AppState::Running)
                | (AppState::Starting, AppState::Idle)
                | (AppState::Starting, AppState::Error)
                | (AppState::Starting, AppState::ShuttingDown)
                // From Running
                | (AppState::Running, AppState::Idle)
                | (AppState::Running, AppState::Paused)
                | (AppState::Running, AppState::Error)
                | (AppState::Running, AppState::Reconnecting)
                | (AppState::Running, AppState::ShuttingDown)
                // From Paused
                | (AppState::Paused, AppState::Running)
                | (AppState::Paused, AppState::Idle)
                | (AppState::Paused, AppState::Error)
                | (AppState::Paused, AppState::ShuttingDown)
                // From Reconnecting
                | (AppState::Reconnecting, AppState::Running)
                | (AppState::Reconnecting, AppState::Idle)
                | (AppState::Reconnecting, AppState::Error)
                | (AppState::Reconnecting, AppState::ShuttingDown)
                // From Error
                | (AppState::Error, AppState::Idle)
                | (AppState::Error, AppState::Starting)
                | (AppState::Error, AppState::ShuttingDown)
        )
    }

    /// Returns true if the application is in an active capture state
    pub fn is_capturing(&self) -> bool {
        matches!(self, AppState::Running | AppState::Paused | AppState::Reconnecting)
    }

    /// Returns true if the application can accept user commands
    pub fn can_accept_commands(&self) -> bool {
        matches!(self, AppState::Idle | AppState::Running | AppState::Paused | AppState::Error)
    }
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Starting => write!(f, "Starting"),
            Self::Running => write!(f, "Running"),
            Self::Paused => write!(f, "Paused"),
            Self::Reconnecting => write!(f, "Reconnecting"),
            Self::Error => write!(f, "Error"),
            Self::ShuttingDown => write!(f, "ShuttingDown"),
        }
    }
}

/// Channel capacity for event broadcast
const EVENT_CHANNEL_CAPACITY: usize = 64;

/// Event bus for broadcasting events to multiple subscribers
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self { sender }
    }

    /// Publish an event to all subscribers
    ///
    /// Returns the number of active subscribers that received the event.
    /// Returns 0 if there are no subscribers (event is dropped).
    pub fn publish(&self, event: Event) -> usize {
        // send() returns Err if there are no receivers, which is fine
        self.sender.send(event).unwrap_or(0)
    }

    /// Subscribe to events
    ///
    /// Returns a receiver that will receive all future events.
    /// Past events are not replayed.
    pub fn subscribe(&self) -> EventSubscriber {
        EventSubscriber {
            receiver: self.sender.subscribe(),
        }
    }

    /// Get the current number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

/// A subscriber to the event bus
pub struct EventSubscriber {
    receiver: broadcast::Receiver<Event>,
}

impl EventSubscriber {
    /// Receive the next event (async)
    ///
    /// Returns `None` if the bus is closed.
    /// May skip events if the subscriber falls behind (lagged).
    pub async fn recv(&mut self) -> Option<Event> {
        let mut consecutive_lags = 0u32;
        loop {
            match self.receiver.recv().await {
                Ok(event) => {
                    return Some(event);
                }
                Err(broadcast::error::RecvError::Lagged(count)) => {
                    consecutive_lags = consecutive_lags.saturating_add(1);
                    // Log that we dropped events
                    tracing::warn!(
                        dropped = count,
                        consecutive_lags,
                        "Event subscriber lagged, dropped events"
                    );
                    // If severely lagging, yield to allow other tasks to run
                    // and prevent spinning
                    if consecutive_lags > 3 {
                        tracing::error!(
                            consecutive_lags,
                            "Event subscriber severely lagged, yielding"
                        );
                        tokio::task::yield_now().await;
                    }
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }

    /// Try to receive an event without blocking
    ///
    /// Returns `None` if no event is available or the bus is closed.
    /// May skip events if the subscriber fell behind (lagged).
    pub fn try_recv(&mut self) -> Option<Event> {
        // Limit iterations to prevent infinite loop if continuously lagging
        const MAX_LAG_ITERATIONS: u32 = 10;
        let mut iterations = 0;

        loop {
            match self.receiver.try_recv() {
                Ok(event) => return Some(event),
                Err(broadcast::error::TryRecvError::Lagged(count)) => {
                    iterations += 1;
                    tracing::warn!(
                        dropped = count,
                        iteration = iterations,
                        "Event subscriber lagged, dropped events"
                    );
                    if iterations >= MAX_LAG_ITERATIONS {
                        tracing::error!("Event subscriber exceeded max lag iterations, giving up");
                        return None;
                    }
                    continue;
                }
                Err(_) => return None,
            }
        }
    }
}

// ============================================================================
// Application Context
// ============================================================================

/// Central application context holding communication channels
///
/// This is passed to all components to enable inter-component communication.
pub struct AppContext {
    /// Send commands to the engine
    pub commands: CommandSender,
    /// Event bus for subscribing to events
    pub events: EventBus,
}

impl AppContext {
    /// Create a new application context
    pub fn new() -> (Self, CommandReceiver) {
        let (cmd_tx, cmd_rx) = command_channel();
        let events = EventBus::new();

        (
            Self {
                commands: cmd_tx,
                events,
            },
            cmd_rx,
        )
    }

    /// Create a handle that can be passed to components
    pub fn handle(&self) -> AppHandle {
        AppHandle {
            commands: self.commands.clone(),
            events: self.events.clone(),
        }
    }
}

/// A clonable handle to the application context
///
/// Components receive this to send commands and subscribe to events.
#[derive(Clone)]
pub struct AppHandle {
    commands: CommandSender,
    events: EventBus,
}

impl AppHandle {
    /// Send a command to the engine
    pub async fn send_command(&self, command: Command) -> Result<(), mpsc::error::SendError<Command>> {
        self.commands.send(command).await
    }

    /// Try to send a command without blocking
    pub fn try_send_command(&self, command: Command) -> Result<(), mpsc::error::TrySendError<Command>> {
        self.commands.try_send(command)
    }

    /// Publish an event
    pub fn publish_event(&self, event: Event) -> usize {
        self.events.publish(event)
    }

    /// Subscribe to events
    pub fn subscribe_events(&self) -> EventSubscriber {
        self.events.subscribe()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_command_channel() {
        let (tx, mut rx) = command_channel();

        tx.send(Command::StartCapture {
            device_id: DeviceId("test".into()),
        })
        .await
        .unwrap();

        let cmd = rx.recv().await.unwrap();
        assert!(matches!(cmd, Command::StartCapture { .. }));
    }

    #[tokio::test]
    async fn test_event_bus() {
        let bus = EventBus::new();
        let mut sub1 = bus.subscribe();
        let mut sub2 = bus.subscribe();

        // Publish an event
        let count = bus.publish(Event::DisplayPaused);
        assert_eq!(count, 2);

        // Both subscribers should receive it
        let ev1 = sub1.recv().await.unwrap();
        let ev2 = sub2.recv().await.unwrap();

        assert!(matches!(ev1, Event::DisplayPaused));
        assert!(matches!(ev2, Event::DisplayPaused));
    }

    #[tokio::test]
    async fn test_app_context() {
        let (ctx, mut cmd_rx) = AppContext::new();
        let handle = ctx.handle();

        // Subscribe before publishing
        let mut sub = handle.subscribe_events();

        // Send command
        handle
            .send_command(Command::PauseDisplay)
            .await
            .unwrap();

        // Publish event
        handle.publish_event(Event::DisplayPaused);

        // Verify command received
        let cmd = cmd_rx.recv().await.unwrap();
        assert!(matches!(cmd, Command::PauseDisplay));

        // Verify event received
        let ev = sub.recv().await.unwrap();
        assert!(matches!(ev, Event::DisplayPaused));
    }
}
