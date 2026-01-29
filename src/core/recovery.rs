//! Crash detection and recovery
//!
//! Detects unclean shutdown (crash) and recovers gracefully on next startup.
//! The user should never be left with a stuck frame as their wallpaper.
//!
//! # Detection Mechanism
//!
//! On startup:
//! 1. Check for 'last_clean_shutdown = false' in config
//! 2. If found, a crash occurred - trigger recovery
//!
//! On normal operation:
//! 1. Set 'last_clean_shutdown = false' when starting capture
//! 2. Set 'last_clean_shutdown = true' on clean shutdown
//!
//! # Recovery Actions
//!
//! On detected crash:
//! 1. Log crash occurrence
//! 2. Restore original wallpaper immediately
//! 3. Clear running marker (set last_clean_shutdown = true)
//! 4. Return recovery status for optional user notification

use crate::config::{load_config, save_config, AppConfig};
use crate::core::{ConfigError, messages};
use crate::platform::wallpaper::restore_wallpaper_from_path;

/// Result of crash detection check
#[derive(Debug, Clone)]
pub enum StartupState {
    /// Clean startup (previous session ended normally)
    Clean,
    /// Crash detected and recovered
    RecoveredFromCrash {
        /// Whether wallpaper was successfully restored
        wallpaper_restored: bool,
        /// Path of restored wallpaper (if any)
        restored_wallpaper_path: Option<String>,
    },
    /// First run (no previous session)
    FirstRun,
}

impl StartupState {
    /// Returns true if a crash was detected
    pub fn was_crash(&self) -> bool {
        matches!(self, Self::RecoveredFromCrash { .. })
    }

    /// Get user-facing message for this state
    pub fn user_message(&self) -> Option<messages::UserMessage> {
        match self {
            Self::Clean | Self::FirstRun => None,
            Self::RecoveredFromCrash { wallpaper_restored, .. } => {
                if *wallpaper_restored {
                    Some(messages::recovery::recovered_from_crash())
                } else {
                    // Wallpaper was NOT restored - show a different message
                    Some(messages::UserMessage::new(
                        "Micround recovered from unexpected shutdown. Unable to restore your original wallpaper.",
                    )
                    .with_error_code("MIC-REC-002")
                    .with_action(messages::RecoveryAction::primary("Dismiss", messages::RecoveryActionId::Dismiss)))
                }
            }
        }
    }
}

/// Crash detection and recovery manager
pub struct RecoveryManager {
    /// Current configuration
    config: AppConfig,
}

impl RecoveryManager {
    /// Create a new recovery manager with the given config
    pub fn new(config: AppConfig) -> Self {
        Self { config }
    }

    /// Check for crash on startup and perform recovery if needed
    ///
    /// This should be called early in the application startup sequence,
    /// before any feed/wallpaper operations begin.
    pub fn check_and_recover(&mut self) -> Result<StartupState, ConfigError> {
        // Check if previous session ended cleanly
        if self.config.internal.last_clean_shutdown {
            // Check if this is first run (no original wallpaper path stored)
            if self.config.internal.original_wallpaper_path.is_none() {
                tracing::info!("First run detected");
                return Ok(StartupState::FirstRun);
            }
            tracing::debug!("Previous session ended cleanly");
            return Ok(StartupState::Clean);
        }

        // Crash detected!
        tracing::warn!(
            "Crash detected: previous session did not shut down cleanly"
        );

        // Attempt to restore wallpaper
        let wallpaper_restored = self.try_restore_wallpaper();
        let restored_path = self.config.internal.original_wallpaper_path.clone();

        // Mark as recovered
        self.config.internal.last_clean_shutdown = true;
        if let Err(e) = save_config(&self.config) {
            tracing::warn!(error = %e, "Failed to save recovery state");
        }

        Ok(StartupState::RecoveredFromCrash {
            wallpaper_restored,
            restored_wallpaper_path: if wallpaper_restored { restored_path } else { None },
        })
    }

    /// Mark session as started (set running marker)
    ///
    /// Call this when the feed starts capturing to mark the session as "in progress".
    /// If the app crashes after this point, recovery will be triggered on next startup.
    pub fn mark_session_started(&mut self) -> Result<(), ConfigError> {
        self.config.internal.last_clean_shutdown = false;
        save_config(&self.config)?;
        tracing::debug!("Session marked as started (running marker set)");
        Ok(())
    }

    /// Mark session as ended cleanly
    ///
    /// Call this during graceful shutdown to mark the session as cleanly ended.
    pub fn mark_session_ended(&mut self) -> Result<(), ConfigError> {
        self.config.internal.last_clean_shutdown = true;
        save_config(&self.config)?;
        tracing::debug!("Session marked as ended cleanly");
        Ok(())
    }

    /// Store the original wallpaper path for later restoration
    pub fn store_original_wallpaper(&mut self, path: &str) -> Result<(), ConfigError> {
        self.config.internal.original_wallpaper_path = Some(path.to_string());
        save_config(&self.config)?;
        tracing::debug!(path = %path, "Original wallpaper path stored");
        Ok(())
    }

    /// Get the stored original wallpaper path
    pub fn original_wallpaper_path(&self) -> Option<&str> {
        self.config.internal.original_wallpaper_path.as_deref()
    }

    /// Clear the original wallpaper path (after successful restore)
    pub fn clear_original_wallpaper(&mut self) -> Result<(), ConfigError> {
        self.config.internal.original_wallpaper_path = None;
        save_config(&self.config)?;
        tracing::debug!("Original wallpaper path cleared");
        Ok(())
    }

    /// Get the current config
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Get mutable access to the current config
    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    /// Try to restore the original wallpaper
    fn try_restore_wallpaper(&self) -> bool {
        if let Some(ref path) = self.config.internal.original_wallpaper_path {
            tracing::info!(path = %path, "Attempting to restore original wallpaper");
            match restore_wallpaper_from_path(path) {
                Ok(()) => {
                    tracing::info!("Original wallpaper restored successfully");
                    true
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to restore original wallpaper");
                    false
                }
            }
        } else {
            tracing::warn!("No original wallpaper path stored, cannot restore");
            false
        }
    }
}

/// Initialize recovery manager with loaded config
///
/// Convenience function that loads config and creates recovery manager.
pub fn init_recovery() -> Result<RecoveryManager, ConfigError> {
    let config = load_config()?;
    Ok(RecoveryManager::new(config))
}

// ============================================================================
// Signal Handlers (Platform-Specific)
// ============================================================================

/// Install signal handlers for graceful shutdown
///
/// On Unix-like systems (Linux, macOS), this installs handlers for:
/// - SIGTERM: Graceful termination
/// - SIGINT: Ctrl+C
///
/// Returns a tokio channel receiver that will receive signals.
#[cfg(unix)]
pub async fn install_signal_handlers() -> tokio::sync::mpsc::Receiver<SignalKind> {
    use tokio::signal::unix::{signal, SignalKind as TokioSignalKind};

    let (tx, rx) = tokio::sync::mpsc::channel(1);

    // SIGTERM handler
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut sigterm = signal(TokioSignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            sigterm.recv().await;
            let _ = tx.send(SignalKind::Terminate).await;
        });
    }

    // SIGINT handler
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut sigint = signal(TokioSignalKind::interrupt())
                .expect("Failed to install SIGINT handler");
            sigint.recv().await;
            let _ = tx.send(SignalKind::Interrupt).await;
        });
    }

    // SIGHUP handler
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut sighup = signal(TokioSignalKind::hangup())
                .expect("Failed to install SIGHUP handler");
            sighup.recv().await;
            let _ = tx.send(SignalKind::Hangup).await;
        });
    }

    tracing::debug!("Signal handlers installed (Unix)");
    rx
}

/// Install signal handlers for graceful shutdown on Windows
#[cfg(windows)]
pub async fn install_signal_handlers() -> tokio::sync::mpsc::Receiver<SignalKind> {
    let (tx, rx) = tokio::sync::mpsc::channel(1);

    // Ctrl+C handler
    {
        let tx = tx.clone();
        tokio::spawn(async move {
            tokio::signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
            let _ = tx.send(SignalKind::Terminate).await;
        });
    }

    tracing::debug!("Signal handlers installed (Windows)");
    rx
}

/// Fallback for other platforms
#[cfg(not(any(unix, windows)))]
pub async fn install_signal_handlers() -> tokio::sync::mpsc::Receiver<SignalKind> {
    let (_, rx) = tokio::sync::mpsc::channel(1);
    tracing::warn!("Signal handlers not supported on this platform");
    rx
}

/// Type of signal received
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// Termination signal (SIGTERM, Ctrl+C)
    Terminate,
    /// Hangup signal (SIGHUP) - may trigger restart
    Hangup,
    /// Interrupt signal (SIGINT)
    Interrupt,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_state_clean() {
        let state = StartupState::Clean;
        assert!(!state.was_crash());
        assert!(state.user_message().is_none());
    }

    #[test]
    fn test_startup_state_crash() {
        let state = StartupState::RecoveredFromCrash {
            wallpaper_restored: true,
            restored_wallpaper_path: Some("/path/to/wallpaper.jpg".to_string()),
        };
        assert!(state.was_crash());
        assert!(state.user_message().is_some());
    }

    #[test]
    fn test_startup_state_first_run() {
        let state = StartupState::FirstRun;
        assert!(!state.was_crash());
        assert!(state.user_message().is_none());
    }

    #[test]
    fn test_recovery_manager_creation() {
        let config = AppConfig::default();
        let manager = RecoveryManager::new(config);
        assert!(manager.config().internal.last_clean_shutdown);
    }

    #[test]
    fn test_recovery_manager_clean_shutdown_detection() {
        let config = AppConfig::default();
        let mut manager = RecoveryManager::new(config);

        // With default config (last_clean_shutdown = true, no wallpaper path), should be FirstRun
        let state = manager.check_and_recover().unwrap();
        assert!(matches!(state, StartupState::FirstRun));
    }

    #[test]
    fn test_recovery_manager_with_previous_session() {
        let mut config = AppConfig::default();
        config.internal.original_wallpaper_path = Some("/some/path.jpg".to_string());
        config.internal.last_clean_shutdown = true;

        let mut manager = RecoveryManager::new(config);
        let state = manager.check_and_recover().unwrap();
        assert!(matches!(state, StartupState::Clean));
    }

    #[test]
    fn test_signal_kind() {
        assert_eq!(SignalKind::Terminate, SignalKind::Terminate);
        assert_ne!(SignalKind::Terminate, SignalKind::Hangup);
    }
}
