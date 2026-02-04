//! Application engine - orchestrates capture, processing, and rendering
//!
//! The engine is the central coordinator that connects all subsystems:
//! - Receives frames from the capture subsystem
//! - Processes frames through the pipeline
//! - Manages pause/freeze state with a frozen frame buffer
//! - Delivers frames to the renderer
//!
//! # Pause/Freeze Functionality
//!
//! When paused:
//! - Capture continues running (camera stays open)
//! - The last processed frame is stored in a freeze buffer
//! - The freeze buffer is used for rendering until resumed
//! - New frames from capture are discarded while paused
//!
//! When resumed:
//! - The next captured frame is immediately processed and rendered
//! - No frame gap on resume

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::core::events::{Command, Event, EventBus};
use crate::core::{Frame, RenderError};
use crate::process::{process_frame, ProcessError, ProcessedFrame, ProcessorConfig};
use crate::render::WallpaperRenderer;

// ============================================================================
// Freeze Buffer
// ============================================================================

/// Buffer to store the frozen frame when paused
///
/// The freeze buffer holds the last processed frame and provides thread-safe
/// access for rendering when the display is paused.
pub struct FreezeBuffer {
    /// The frozen frame (if paused and a frame exists)
    frame: RwLock<Option<Arc<ProcessedFrame>>>,
}

impl FreezeBuffer {
    /// Create a new empty freeze buffer
    pub fn new() -> Self {
        Self {
            frame: RwLock::new(None),
        }
    }

    /// Store a frame in the freeze buffer
    ///
    /// This is called when transitioning to the paused state,
    /// capturing the current frame for display.
    pub fn freeze(&self, frame: ProcessedFrame) {
        *self.frame.write().unwrap() = Some(Arc::new(frame));
    }

    /// Store an already-Arc'd frame in the freeze buffer (avoids clone)
    ///
    /// More efficient than `freeze` when you already have an Arc<ProcessedFrame>.
    /// This avoids cloning the frame data (~8MB for HD frames).
    pub fn freeze_arc(&self, frame: Arc<ProcessedFrame>) {
        *self.frame.write().unwrap() = Some(frame);
    }

    /// Get a reference to the frozen frame
    ///
    /// Returns None if no frame has been frozen.
    pub fn get(&self) -> Option<Arc<ProcessedFrame>> {
        self.frame.read().unwrap().clone()
    }

    /// Clear the freeze buffer
    ///
    /// Called when resuming from pause or when the frozen frame is no longer needed.
    pub fn clear(&self) {
        *self.frame.write().unwrap() = None;
    }

    /// Check if a frozen frame exists
    pub fn has_frame(&self) -> bool {
        self.frame.read().unwrap().is_some()
    }
}

impl Default for FreezeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Engine State
// ============================================================================

/// Current state of the display engine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayState {
    /// Not rendering (idle or stopped)
    Stopped,
    /// Actively rendering live frames
    Running,
    /// Display is paused (showing frozen frame)
    Paused,
}

impl DisplayState {
    /// Check if the display is currently showing content
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

// ============================================================================
// Engine Configuration
// ============================================================================

/// Configuration for the display engine
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Processing configuration for frames
    pub processor: ProcessorConfig,
    /// Whether to continue capturing while paused
    pub capture_while_paused: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            processor: ProcessorConfig::default(),
            capture_while_paused: true,
        }
    }
}

// ============================================================================
// Engine Metrics
// ============================================================================

/// Metrics tracked by the engine
#[derive(Debug, Clone, Default)]
pub struct EngineMetrics {
    /// Frames processed
    pub frames_processed: u64,
    /// Frames rendered
    pub frames_rendered: u64,
    /// Frames skipped (while paused)
    pub frames_skipped: u64,
    /// Processing errors
    pub processing_errors: u64,
    /// Render errors
    pub render_errors: u64,
    /// Time spent paused (total)
    pub time_paused_ms: u64,
}

// ============================================================================
// Display Engine
// ============================================================================

/// Central engine that orchestrates the display pipeline
///
/// Connects capture, processing, and rendering while managing
/// pause/freeze state.
pub struct DisplayEngine {
    /// Current display state
    state: RwLock<DisplayState>,
    /// Freeze buffer for paused display
    freeze_buffer: FreezeBuffer,
    /// Configuration
    config: RwLock<EngineConfig>,
    /// Event bus for publishing state changes
    event_bus: EventBus,
    /// Flag indicating engine should stop
    stop_signal: AtomicBool,
    /// Metrics
    metrics: RwLock<EngineMetrics>,
    /// Timestamp when pause started (for metrics)
    pause_start: RwLock<Option<Instant>>,
    /// The most recent processed frame (for snapshot during pause)
    last_frame: RwLock<Option<Arc<ProcessedFrame>>>,
}

impl DisplayEngine {
    /// Create a new display engine
    pub fn new(event_bus: EventBus) -> Self {
        Self {
            state: RwLock::new(DisplayState::Stopped),
            freeze_buffer: FreezeBuffer::new(),
            config: RwLock::new(EngineConfig::default()),
            event_bus,
            stop_signal: AtomicBool::new(false),
            metrics: RwLock::new(EngineMetrics::default()),
            pause_start: RwLock::new(None),
            last_frame: RwLock::new(None),
        }
    }

    /// Create with custom configuration
    pub fn with_config(event_bus: EventBus, config: EngineConfig) -> Self {
        Self {
            state: RwLock::new(DisplayState::Stopped),
            freeze_buffer: FreezeBuffer::new(),
            config: RwLock::new(config),
            event_bus,
            stop_signal: AtomicBool::new(false),
            metrics: RwLock::new(EngineMetrics::default()),
            pause_start: RwLock::new(None),
            last_frame: RwLock::new(None),
        }
    }

    // ========================================================================
    // State Management
    // ========================================================================

    /// Get the current display state
    pub fn state(&self) -> DisplayState {
        *self.state.read().unwrap()
    }

    /// Check if the display is paused
    pub fn is_paused(&self) -> bool {
        self.state() == DisplayState::Paused
    }

    /// Check if the display is running
    pub fn is_running(&self) -> bool {
        self.state() == DisplayState::Running
    }

    /// Start the display engine
    pub fn start(&self) {
        let mut state = self.state.write().unwrap();
        if *state == DisplayState::Stopped {
            *state = DisplayState::Running;
            tracing::info!("Display engine started");
        }
    }

    /// Stop the display engine
    pub fn stop(&self) {
        let mut state = self.state.write().unwrap();
        if *state != DisplayState::Stopped {
            // If we were paused, update pause metrics
            if *state == DisplayState::Paused {
                self.update_pause_metrics();
            }
            *state = DisplayState::Stopped;
            self.freeze_buffer.clear();
            self.stop_signal.store(true, Ordering::Release);
            tracing::info!("Display engine stopped");
        }
    }

    /// Pause the display (freeze current frame)
    ///
    /// The last processed frame is captured to the freeze buffer
    /// and will be used for rendering until resumed.
    pub fn pause(&self) {
        let mut state = self.state.write().unwrap();
        if *state == DisplayState::Running {
            // Capture the last frame to freeze buffer
            // Use Arc clone instead of data clone - this is O(1) vs O(n) for HD frames
            if let Some(frame) = self.last_frame.read().unwrap().clone() {
                self.freeze_buffer.freeze_arc(frame);
            }

            *state = DisplayState::Paused;
            *self.pause_start.write().unwrap() = Some(Instant::now());

            // Publish event
            self.event_bus.publish(Event::DisplayPaused);
            tracing::info!("Display paused");
        }
    }

    /// Resume the display from pause
    ///
    /// Clears the freeze buffer and resumes live frame rendering.
    pub fn resume(&self) {
        let mut state = self.state.write().unwrap();
        if *state == DisplayState::Paused {
            *state = DisplayState::Running;

            // Update pause time metrics
            self.update_pause_metrics();

            // Clear the freeze buffer to free memory (~8MB for HD frames)
            // The last_frame is still available for snapshots when running
            self.freeze_buffer.clear();

            // Publish event
            self.event_bus.publish(Event::DisplayResumed);
            tracing::info!("Display resumed");
        }
    }

    /// Toggle pause state
    pub fn toggle_pause(&self) {
        if self.is_paused() {
            self.resume();
        } else if self.is_running() {
            self.pause();
        }
    }

    fn update_pause_metrics(&self) {
        if let Some(start) = self.pause_start.write().unwrap().take() {
            let elapsed = start.elapsed().as_millis() as u64;
            self.metrics.write().unwrap().time_paused_ms += elapsed;
        }
    }

    // ========================================================================
    // Frame Processing
    // ========================================================================

    /// Process a raw frame from capture
    ///
    /// If the display is paused, the frame is discarded (but counted).
    /// If running, the frame is processed and stored as the last frame.
    pub fn process_frame(
        &self,
        raw_frame: &Frame,
    ) -> Result<Option<Arc<ProcessedFrame>>, ProcessError> {
        let state = self.state();

        // Skip processing if stopped
        if state == DisplayState::Stopped {
            return Ok(None);
        }

        // If paused, skip processing but track the skip
        if state == DisplayState::Paused {
            self.metrics.write().unwrap().frames_skipped += 1;
            tracing::trace!("Frame skipped (paused)");
            return Ok(None);
        }

        // Process the frame
        let config = self.config.read().unwrap();
        let processed = process_frame(raw_frame, &config.processor)?;
        let processed_arc = Arc::new(processed);

        // Store as last frame (for pause snapshot)
        *self.last_frame.write().unwrap() = Some(processed_arc.clone());

        // Update metrics
        self.metrics.write().unwrap().frames_processed += 1;

        Ok(Some(processed_arc))
    }

    /// Get the frame that should be rendered
    ///
    /// Returns the frozen frame if paused, or the last processed frame if running.
    pub fn get_render_frame(&self) -> Option<Arc<ProcessedFrame>> {
        match self.state() {
            DisplayState::Paused => self.freeze_buffer.get(),
            DisplayState::Running => self.last_frame.read().unwrap().clone(),
            DisplayState::Stopped => None,
        }
    }

    /// Get the frozen frame (for snapshot while paused)
    pub fn get_frozen_frame(&self) -> Option<Arc<ProcessedFrame>> {
        self.freeze_buffer.get()
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    /// Render a frame using the provided renderer
    ///
    /// If paused, renders the frozen frame. If running, renders the provided frame.
    pub fn render_frame(
        &self,
        renderer: &mut dyn WallpaperRenderer,
        frame: &ProcessedFrame,
    ) -> Result<(), RenderError> {
        let result = renderer.render(frame);

        match &result {
            Ok(()) => {
                self.metrics.write().unwrap().frames_rendered += 1;
            }
            Err(e) => {
                self.metrics.write().unwrap().render_errors += 1;
                tracing::warn!(error = %e, "Render error");
            }
        }

        result
    }

    // ========================================================================
    // Configuration
    // ========================================================================

    /// Update the processor configuration
    pub fn set_processor_config(&self, config: ProcessorConfig) {
        self.config.write().unwrap().processor = config;
    }

    /// Get the current processor configuration
    pub fn processor_config(&self) -> ProcessorConfig {
        self.config.read().unwrap().processor.clone()
    }

    // ========================================================================
    // Metrics
    // ========================================================================

    /// Get a snapshot of current metrics
    pub fn metrics(&self) -> EngineMetrics {
        self.metrics.read().unwrap().clone()
    }

    /// Reset metrics
    pub fn reset_metrics(&self) {
        *self.metrics.write().unwrap() = EngineMetrics::default();
    }

    // ========================================================================
    // Command Handling
    // ========================================================================

    /// Handle a command from the UI/event system
    pub fn handle_command(&self, command: &Command) {
        match command {
            Command::PauseDisplay => self.pause(),
            Command::ResumeDisplay => self.resume(),
            Command::StopCapture => self.stop(),
            Command::Quit => self.stop(),
            // Other commands are handled elsewhere
            _ => {}
        }
    }
}

// Note: DisplayEngine is automatically Send + Sync because:
// - RwLock<T> is Send + Sync when T: Send + Sync
// - AtomicBool is Send + Sync
// - EventBus is Send + Sync (uses Arc internally)
// - FreezeBuffer contains RwLock<Option<Arc<ProcessedFrame>>> which is Send + Sync
// No unsafe impl needed.

// ============================================================================
// Engine Handle for Async Context
// ============================================================================

/// Handle to control the display engine from async code
#[derive(Clone)]
pub struct EngineHandle {
    engine: Arc<DisplayEngine>,
}

impl EngineHandle {
    /// Create a new engine handle
    pub fn new(engine: Arc<DisplayEngine>) -> Self {
        Self { engine }
    }

    /// Pause the display
    pub fn pause(&self) {
        self.engine.pause();
    }

    /// Resume the display
    pub fn resume(&self) {
        self.engine.resume();
    }

    /// Toggle pause
    pub fn toggle_pause(&self) {
        self.engine.toggle_pause();
    }

    /// Check if paused
    pub fn is_paused(&self) -> bool {
        self.engine.is_paused()
    }

    /// Get the engine state
    pub fn state(&self) -> DisplayState {
        self.engine.state()
    }

    /// Get the frozen frame
    pub fn get_frozen_frame(&self) -> Option<Arc<ProcessedFrame>> {
        self.engine.get_frozen_frame()
    }

    /// Get metrics
    pub fn metrics(&self) -> EngineMetrics {
        self.engine.metrics()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_event_bus() -> EventBus {
        EventBus::new()
    }

    #[test]
    fn test_freeze_buffer_operations() {
        let buffer = FreezeBuffer::new();

        // Initially empty
        assert!(!buffer.has_frame());
        assert!(buffer.get().is_none());

        // Freeze a frame
        let frame = ProcessedFrame::new(vec![0u8; 100], 10, 10);
        buffer.freeze(frame);

        assert!(buffer.has_frame());
        assert!(buffer.get().is_some());

        // Clear
        buffer.clear();
        assert!(!buffer.has_frame());
    }

    #[test]
    fn test_engine_state_transitions() {
        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        // Initially stopped
        assert_eq!(engine.state(), DisplayState::Stopped);

        // Start
        engine.start();
        assert_eq!(engine.state(), DisplayState::Running);

        // Pause
        engine.pause();
        assert_eq!(engine.state(), DisplayState::Paused);

        // Resume
        engine.resume();
        assert_eq!(engine.state(), DisplayState::Running);

        // Stop
        engine.stop();
        assert_eq!(engine.state(), DisplayState::Stopped);
    }

    #[test]
    fn test_pause_from_stopped_is_noop() {
        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        // Try to pause from stopped
        engine.pause();
        assert_eq!(engine.state(), DisplayState::Stopped);
    }

    #[test]
    fn test_toggle_pause() {
        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        engine.start();
        assert!(engine.is_running());

        engine.toggle_pause();
        assert!(engine.is_paused());

        engine.toggle_pause();
        assert!(engine.is_running());
    }

    #[test]
    fn test_frames_skipped_when_paused() {
        use crate::core::PixelFormat;

        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        engine.start();
        engine.pause();

        let raw_frame = Frame {
            data: vec![0u8; 100 * 100 * 4],
            format: PixelFormat::Rgba32,
            width: 100,
            height: 100,
            timestamp_ns: 0,
            sequence: 0,
        };

        // Process should return None when paused
        let result = engine.process_frame(&raw_frame);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Check metrics
        let metrics = engine.metrics();
        assert_eq!(metrics.frames_skipped, 1);
        assert_eq!(metrics.frames_processed, 0);
    }

    #[test]
    fn test_frame_stored_on_pause() {
        use crate::core::PixelFormat;

        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        engine.start();

        // Process a frame while running
        let raw_frame = Frame {
            data: vec![128u8; 100 * 100 * 4],
            format: PixelFormat::Rgba32,
            width: 100,
            height: 100,
            timestamp_ns: 0,
            sequence: 0,
        };

        let result = engine.process_frame(&raw_frame);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Now pause - should capture last frame
        engine.pause();

        // Frozen frame should exist
        assert!(engine.get_frozen_frame().is_some());
    }

    #[test]
    fn test_handle_command() {
        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        engine.start();
        assert!(engine.is_running());

        engine.handle_command(&Command::PauseDisplay);
        assert!(engine.is_paused());

        engine.handle_command(&Command::ResumeDisplay);
        assert!(engine.is_running());

        engine.handle_command(&Command::Quit);
        assert_eq!(engine.state(), DisplayState::Stopped);
    }

    #[test]
    fn test_pause_metrics_tracking() {
        let bus = make_test_event_bus();
        let engine = DisplayEngine::new(bus);

        engine.start();
        engine.pause();

        // Wait a bit
        std::thread::sleep(std::time::Duration::from_millis(50));

        engine.resume();

        let metrics = engine.metrics();
        assert!(metrics.time_paused_ms >= 40); // Allow some margin
    }

    #[test]
    fn test_engine_handle() {
        let bus = make_test_event_bus();
        let engine = Arc::new(DisplayEngine::new(bus));
        let handle = EngineHandle::new(engine.clone());

        engine.start();

        handle.pause();
        assert!(handle.is_paused());

        handle.resume();
        assert!(!handle.is_paused());
    }
}
