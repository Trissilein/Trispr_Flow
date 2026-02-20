use std::fmt;

#[derive(Debug, Clone)]
pub enum AIError {
    UnknownProvider(String),
    MissingApiKey(String),
    InvalidApiKey(String),
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
        }
    }
}

impl std::error::Error for AIError {}
