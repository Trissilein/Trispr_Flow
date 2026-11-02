use std::fmt;

#[derive(Debug, Clone)]
pub enum AIError {
    OllamaNotRunning,
    NetworkError(String),
    Timeout,
}

impl fmt::Display for AIError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AIError::OllamaNotRunning => {
                write!(
                    f,
                    "Ollama is not running. Please start Ollama before using local AI refinement."
                )
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
