use std::fmt;

#[derive(Debug, Clone)]
pub enum AIError {
    UnknownProvider(String),
    MissingApiKey(String),
    InvalidApiKey(String),
    NotImplemented(String),
    Network(String),
    Config(String),
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
            AIError::NotImplemented(message) => write!(f, "Not implemented: {}", message),
            AIError::Network(message) => write!(f, "Network error: {}", message),
            AIError::Config(message) => write!(f, "Configuration error: {}", message),
        }
    }
}

impl std::error::Error for AIError {}
