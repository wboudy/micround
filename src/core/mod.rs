//! Core platform-independent logic
//!
//! This module contains the shared types, traits, and utilities used across
//! all platform-specific implementations.

pub mod types;
pub mod error;
pub mod logging;
pub mod events;
pub mod error_injector;
pub mod latency;
pub mod messages;
pub mod recovery;

pub use types::*;
pub use error::*;
pub use logging::LoggingError;
pub use events::{
    AppContext, AppHandle, AppState, Command, Event, EventBus, EventSubscriber, FrameDropReason,
};
pub use latency::{
    FrameMetrics, LatencyHistogram, LatencyTracker, LatencySummaryReport,
    SharedLatencyTracker, StageBreakdown, shared_latency_tracker,
};
pub use messages::{RecoveryAction, RecoveryActionId, UserMessage};
pub use recovery::{RecoveryManager, SignalKind, StartupState, init_recovery, install_signal_handlers};
