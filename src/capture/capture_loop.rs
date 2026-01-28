//! Frame capture loop
//!
//! Manages continuous frame acquisition from a camera backend,
//! delivering frames to consumers via a bounded channel.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐      ┌──────────────────┐      ┌───────────────┐
//! │ CaptureBackend  │ ──▶  │  Capture Thread  │ ──▶  │ Frame Channel │
//! │ (next_frame())  │      │  (CaptureLoop)   │      │ (bounded)     │
//! └─────────────────┘      └──────────────────┘      └───────────────┘
//!                                  │
//!                                  ▼
//!                          ┌──────────────────┐
//!                          │ CaptureMetrics   │
//!                          └──────────────────┘
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::core::{CaptureError, CaptureSettings, DeviceId, Frame, NegotiatedFormat};
use crate::capture::CaptureBackend;

/// Capacity of the frame delivery channel
/// Small to minimize latency, but enough to handle brief processing delays
const FRAME_CHANNEL_CAPACITY: usize = 2;

/// Maximum consecutive errors before entering error state
const MAX_CONSECUTIVE_ERRORS: u32 = 10;

/// Frame timeout in milliseconds
const FRAME_TIMEOUT_MS: u64 = 1000;

/// Statistics tracked by the capture loop
#[derive(Debug, Default)]
pub struct CaptureMetrics {
    /// Total frames captured successfully
    pub frames_captured: AtomicU64,
    /// Frames dropped due to full channel
    pub frames_dropped: AtomicU64,
    /// Consecutive capture errors
    pub consecutive_errors: AtomicU64,
    /// Total capture errors encountered
    pub total_errors: AtomicU64,
}

impl CaptureMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current snapshot of metrics
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            frames_captured: self.frames_captured.load(Ordering::Relaxed),
            frames_dropped: self.frames_dropped.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
        }
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.frames_captured.store(0, Ordering::Relaxed);
        self.frames_dropped.store(0, Ordering::Relaxed);
        self.consecutive_errors.store(0, Ordering::Relaxed);
        self.total_errors.store(0, Ordering::Relaxed);
    }

    fn record_frame_captured(&self) {
        self.frames_captured.fetch_add(1, Ordering::Relaxed);
        self.consecutive_errors.store(0, Ordering::Relaxed);
    }

    fn record_frame_dropped(&self) {
        self.frames_dropped.fetch_add(1, Ordering::Relaxed);
    }

    fn record_error(&self) {
        self.consecutive_errors.fetch_add(1, Ordering::Relaxed);
        self.total_errors.fetch_add(1, Ordering::Relaxed);
    }

    fn consecutive_errors(&self) -> u64 {
        self.consecutive_errors.load(Ordering::Relaxed)
    }
}

/// Snapshot of capture metrics at a point in time
#[derive(Debug, Clone, Copy)]
pub struct MetricsSnapshot {
    pub frames_captured: u64,
    pub frames_dropped: u64,
    pub total_errors: u64,
}

/// State of the capture loop
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureState {
    /// Not started
    Idle,
    /// Capture thread is running
    Running,
    /// Stopped normally
    Stopped,
    /// Stopped due to error
    Error,
}

/// Handle to control a running capture loop
pub struct CaptureLoopHandle {
    /// Signal to stop the capture thread
    stop_signal: Arc<AtomicBool>,
    /// Thread handle for joining
    thread_handle: Option<JoinHandle<Result<(), CaptureError>>>,
    /// Shared metrics
    metrics: Arc<CaptureMetrics>,
    /// Negotiated format
    format: NegotiatedFormat,
}

impl CaptureLoopHandle {
    /// Request the capture loop to stop
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::Release);
    }

    /// Wait for the capture thread to finish
    ///
    /// Returns the result of the capture loop
    pub fn join(mut self) -> Result<(), CaptureError> {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            handle.join().map_err(|_| {
                CaptureError::Platform("Capture thread panicked".into())
            })?
        } else {
            Ok(())
        }
    }

    /// Check if the capture loop is still running
    pub fn is_running(&self) -> bool {
        self.thread_handle
            .as_ref()
            .map(|h| !h.is_finished())
            .unwrap_or(false)
    }

    /// Get current metrics
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Get the negotiated format
    pub fn format(&self) -> &NegotiatedFormat {
        &self.format
    }
}

impl Drop for CaptureLoopHandle {
    fn drop(&mut self) {
        // Signal stop when handle is dropped
        self.stop_signal.store(true, Ordering::Release);
    }
}

/// Receiver for frames from the capture loop
pub type FrameReceiver = mpsc::Receiver<Frame>;

/// Error from starting capture loop, may include the backend for recovery
pub struct CaptureLoopError {
    /// The error that occurred
    pub error: CaptureError,
    /// The backend, returned so caller can recover or retry.
    /// None if the backend was irrecoverably lost (e.g., thread spawn failure).
    pub backend: Option<Box<dyn CaptureBackend>>,
}

impl std::fmt::Debug for CaptureLoopError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureLoopError")
            .field("error", &self.error)
            .field("backend_recovered", &self.backend.is_some())
            .finish()
    }
}

/// Start a capture loop on a dedicated thread
///
/// Opens the camera, starts capture, and delivers frames to the returned channel.
/// The loop continues until stopped via the handle or an unrecoverable error occurs.
///
/// # Arguments
/// * `backend` - The capture backend to use (takes ownership)
/// * `device_id` - ID of the camera to capture from
/// * `settings` - Requested capture settings
///
/// # Returns
/// * `Ok((handle, receiver))` - Handle to control the loop and receiver for frames
/// * `Err(CaptureLoopError)` - If the camera couldn't be opened (includes backend for recovery)
pub fn start_capture_loop(
    mut backend: Box<dyn CaptureBackend>,
    device_id: DeviceId,
    settings: CaptureSettings,
) -> Result<(CaptureLoopHandle, FrameReceiver), CaptureLoopError> {
    // Open the camera and get negotiated format
    let format = match backend.open(&device_id, settings) {
        Ok(f) => f,
        Err(e) => return Err(CaptureLoopError { error: e, backend: Some(backend) }),
    };

    // Start capture
    if let Err(e) = backend.start() {
        backend.close();
        return Err(CaptureLoopError { error: e, backend: Some(backend) });
    }

    // Create frame channel
    let (tx, rx) = mpsc::channel(FRAME_CHANNEL_CAPACITY);

    // Create shared state
    let stop_signal = Arc::new(AtomicBool::new(false));
    let metrics = Arc::new(CaptureMetrics::new());

    // Clone for the thread
    let stop_signal_clone = stop_signal.clone();
    let metrics_clone = metrics.clone();
    let format_clone = format.clone();

    // Spawn capture thread
    // Note: If spawn fails, the backend is lost (moved into the closure).
    // This is acceptable since spawn failures only occur on extreme resource
    // exhaustion (thread limit exceeded), which is unrecoverable anyway.
    let thread_handle = match thread::Builder::new()
        .name("micround-capture".into())
        .spawn(move || {
            capture_thread_main(backend, tx, stop_signal_clone, metrics_clone)
        }) {
        Ok(handle) => handle,
        Err(e) => {
            // Backend is lost here, but this is extremely rare (thread limit exceeded)
            return Err(CaptureLoopError {
                error: CaptureError::Platform(format!(
                    "Failed to spawn capture thread: {}. Backend was lost; recreate it.",
                    e
                )),
                backend: None, // Cannot recover - backend was moved into the closure
            });
        }
    };

    let handle = CaptureLoopHandle {
        stop_signal,
        thread_handle: Some(thread_handle),
        metrics,
        format: format_clone,
    };

    Ok((handle, rx))
}

/// Main function for the capture thread
fn capture_thread_main(
    mut backend: Box<dyn CaptureBackend>,
    tx: mpsc::Sender<Frame>,
    stop_signal: Arc<AtomicBool>,
    metrics: Arc<CaptureMetrics>,
) -> Result<(), CaptureError> {
    tracing::info!("Capture thread started");

    let mut last_frame_time = Instant::now();

    loop {
        // Check for stop signal
        if stop_signal.load(Ordering::Acquire) {
            tracing::info!("Capture thread received stop signal");
            break;
        }

        // Try to capture a frame
        match backend.next_frame() {
            Ok(frame) => {
                let now = Instant::now();
                let inter_frame_ms = now.duration_since(last_frame_time).as_millis();
                last_frame_time = now;

                // Try to send the frame
                match tx.try_send(frame) {
                    Ok(()) => {
                        metrics.record_frame_captured();
                        tracing::trace!(
                            sequence = metrics.frames_captured.load(Ordering::Relaxed),
                            inter_frame_ms = inter_frame_ms,
                            "Frame captured"
                        );
                    }
                    Err(mpsc::error::TrySendError::Full(_frame)) => {
                        // Channel full, drop the frame
                        metrics.record_frame_dropped();
                        tracing::warn!("Frame dropped: channel full");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Receiver dropped, stop capture
                        tracing::info!("Frame receiver closed, stopping capture");
                        break;
                    }
                }
            }
            Err(e) => {
                metrics.record_error();
                let consecutive = metrics.consecutive_errors();

                match e {
                    CaptureError::Timeout(_) => {
                        tracing::debug!(consecutive_errors = consecutive, "Frame timeout");
                    }
                    CaptureError::Disconnected => {
                        tracing::warn!("Camera disconnected");
                        return Err(e);
                    }
                    _ => {
                        tracing::warn!(error = %e, consecutive_errors = consecutive, "Capture error");
                    }
                }

                // Check for too many consecutive errors
                if consecutive >= MAX_CONSECUTIVE_ERRORS as u64 {
                    tracing::error!(
                        consecutive_errors = consecutive,
                        "Too many consecutive capture errors, stopping"
                    );
                    return Err(CaptureError::Platform(format!(
                        "Too many consecutive errors: {}",
                        consecutive
                    )));
                }

                // Brief pause before retrying
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    // Clean shutdown
    tracing::info!("Capture thread stopping");
    let _ = backend.stop();
    backend.close();

    let final_metrics = metrics.snapshot();
    tracing::info!(
        frames_captured = final_metrics.frames_captured,
        frames_dropped = final_metrics.frames_dropped,
        total_errors = final_metrics.total_errors,
        "Capture thread finished"
    );

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_recording() {
        let metrics = CaptureMetrics::new();

        // Record some frames
        metrics.record_frame_captured();
        metrics.record_frame_captured();
        metrics.record_frame_dropped();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.frames_captured, 2);
        assert_eq!(snapshot.frames_dropped, 1);
        assert_eq!(snapshot.total_errors, 0);
    }

    #[test]
    fn test_metrics_error_tracking() {
        let metrics = CaptureMetrics::new();

        // Record errors
        metrics.record_error();
        metrics.record_error();
        assert_eq!(metrics.consecutive_errors(), 2);

        // Frame capture resets consecutive errors
        metrics.record_frame_captured();
        assert_eq!(metrics.consecutive_errors(), 0);

        // But total errors remain
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.total_errors, 2);
    }

    #[test]
    fn test_metrics_reset() {
        let metrics = CaptureMetrics::new();

        metrics.record_frame_captured();
        metrics.record_error();

        metrics.reset();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.frames_captured, 0);
        assert_eq!(snapshot.total_errors, 0);
    }
}
