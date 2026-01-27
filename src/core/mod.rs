//! Core platform-independent logic
//!
//! This module contains the shared types, traits, and utilities used across
//! all platform-specific implementations.

pub mod types;
pub mod error;
pub mod logging;
pub mod events;

pub use types::*;
pub use error::*;
pub use logging::LoggingError;
pub use events::{
    AppContext, AppHandle, AppState, Command, Event, EventBus, EventSubscriber,
    CaptureSettings as EventCaptureSettings,
};
