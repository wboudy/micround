//! Core platform-independent logic
//!
//! This module contains the shared types, traits, and utilities used across
//! all platform-specific implementations.
#![allow(dead_code)] // Core API exports
#![allow(unused_imports)] // Library API re-exports

pub mod error;
pub mod error_injector;
pub mod events;
pub mod hotkeys;
pub mod latency;
pub mod logging;
pub mod messages;
pub mod recovery;
pub mod types;

pub use error::*;
pub use events::{
    AppContext, AppHandle, AppState, Command, Event, EventBus, EventSubscriber, FrameDropReason,
};
pub use latency::{
    shared_latency_tracker, FrameMetrics, LatencyHistogram, LatencySummaryReport, LatencyTracker,
    SharedLatencyTracker, StageBreakdown,
};
pub use logging::LoggingError;
pub use messages::{RecoveryAction, RecoveryActionId, UserMessage};
pub use recovery::{
    init_recovery, install_signal_handlers, RecoveryManager, SignalKind, StartupState,
};
pub use types::*;
