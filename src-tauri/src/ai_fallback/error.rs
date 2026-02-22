use std::fmt;

#[derive(Debug, Clone)]
pub enum AIError {
    UnknownProvider(String),
    MissingApiKey(String),
    InvalidApiKey(String),
    OllamaNotRunning,
    NetworkError(String),
    Timeout,
}

impl fmt::Display for AIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AIError::UnknownProvider(provider) => {
                write!(f, "Unknown AI provider: {}", provider)
            }
            AIError::MissingApiKey(provider) => {
                write!(f, "No API key configured for provider '{}'", provider)
            }
            AIError::InvalidApiKey(message) => write!(f, "Invalid API key: {}", message),
            AIError::OllamaNotRunning => {
                write!(f, "Ollama is not running. Please start Ollama before using local AI refinement.")
            }
            AIError::NetworkError(message) => {
                write!(f, "Network error: {}", message)
            }
            AIError::Timeout => {
                write!(f, "Request timed out. The AI provider may be overloaded.")
            }
        }
    }
}

impl std::error::Error for AIError {}
