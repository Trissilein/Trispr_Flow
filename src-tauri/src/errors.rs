use serde::{Deserialize, Serialize};
use std::fmt;

/// Application-wide error types with categories for better error handling
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "message")]
pub enum AppError {
    /// Audio device-related errors (device not found, stream failed, etc.)
    AudioDevice(String),

    /// Transcription errors (ASR backend failed, model not found, etc.)
    Transcription(String),

    /// Hotkey registration/validation errors
    Hotkey(String),

    /// Settings/history storage errors
    Storage(String),

    /// Network errors (model download, cloud fallback, etc.)
    Network(String),

    /// Overlay/window management errors
    Window(String),

    /// Generic errors that don't fit other categories
    Other(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AppError::AudioDevice(msg) => write!(f, "Audio Device Error: {}", msg),
            AppError::Transcription(msg) => write!(f, "Transcription Error: {}", msg),
            AppError::Hotkey(msg) => write!(f, "Hotkey Error: {}", msg),
            AppError::Storage(msg) => write!(f, "Storage Error: {}", msg),
            AppError::Network(msg) => write!(f, "Network Error: {}", msg),
            AppError::Window(msg) => write!(f, "Window Error: {}", msg),
            AppError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl AppError {
    /// Returns a user-friendly title for the error
    pub fn title(&self) -> &str {
        match self {
            AppError::AudioDevice(_) => "Audio Device Issue",
            AppError::Transcription(_) => "Transcription Failed",
            AppError::Hotkey(_) => "Hotkey Problem",
            AppError::Storage(_) => "Storage Error",
            AppError::Network(_) => "Network Problem",
            AppError::Window(_) => "Window Error",
            AppError::Other(_) => "Error",
        }
    }

    /// Returns the error message
    pub fn message(&self) -> &str {
        match self {
            AppError::AudioDevice(msg)
            | AppError::Transcription(msg)
            | AppError::Hotkey(msg)
            | AppError::Storage(msg)
            | AppError::Network(msg)
            | AppError::Window(msg)
            | AppError::Other(msg) => msg,
        }
    }

    /// Returns whether this error is recoverable (can be retried)
    #[allow(dead_code)]
    pub fn is_recoverable(&self) -> bool {
        match self {
            AppError::AudioDevice(_) => true,  // Device might reconnect
            AppError::Transcription(_) => true, // Can retry transcription
            AppError::Hotkey(_) => false,       // Hotkey conflicts need manual fix
            AppError::Storage(_) => true,       // Might be transient disk issue
            AppError::Network(_) => true,       // Network might recover
            AppError::Window(_) => true,        // Window issues might resolve
            AppError::Other(_) => false,        // Unknown errors, don't retry
        }
    }

    /// Returns a suggested action for the user
    #[allow(dead_code)]
    pub fn suggested_action(&self) -> Option<&str> {
        match self {
            AppError::AudioDevice(_) => Some("Check your microphone connection and try again"),
            AppError::Transcription(_) => Some("Try recording again or check your model installation"),
            AppError::Hotkey(_) => Some("Choose a different hotkey combination"),
            AppError::Storage(_) => Some("Check disk space and permissions"),
            AppError::Network(_) => Some("Check your internet connection"),
            AppError::Window(_) => Some("Try restarting the application"),
            AppError::Other(_) => None,
        }
    }
}

/// Convert from String to AppError::Other
impl From<String> for AppError {
    fn from(error: String) -> Self {
        AppError::Other(error)
    }
}

/// Convert from &str to AppError::Other
impl From<&str> for AppError {
    fn from(error: &str) -> Self {
        AppError::Other(error.to_string())
    }
}

/// Error event payload sent to the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEvent {
    pub error: AppError,
    pub timestamp: u64,
    pub context: Option<String>,
}

impl ErrorEvent {
    pub fn new(error: AppError) -> Self {
        Self {
            error,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            context: None,
        }
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::AudioDevice("Device not found".to_string());
        assert_eq!(err.to_string(), "Audio Device Error: Device not found");
    }

    #[test]
    fn test_error_title() {
        let err = AppError::Transcription("Model failed".to_string());
        assert_eq!(err.title(), "Transcription Failed");
    }

    #[test]
    fn test_recoverable() {
        assert!(AppError::AudioDevice("test".to_string()).is_recoverable());
        assert!(!AppError::Hotkey("test".to_string()).is_recoverable());
    }

    #[test]
    fn test_from_string() {
        let err: AppError = "test error".into();
        assert!(matches!(err, AppError::Other(_)));
    }

    #[test]
    fn test_error_event() {
        let event = ErrorEvent::new(AppError::Network("Connection failed".to_string()))
            .with_context("Downloading model");

        assert!(event.context.is_some());
        assert_eq!(event.context.unwrap(), "Downloading model");
    }
}
