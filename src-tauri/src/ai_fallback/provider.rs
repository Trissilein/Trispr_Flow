use super::error::AIError;
use super::models::{RefinementOptions, RefinementResult, TokenUsage};

pub trait AIProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn available_models(&self) -> Vec<String>;
    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError>;
    fn estimate_cost_usd(&self, model: &str, input_tokens: usize, output_tokens: usize) -> f64;
    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError>;
}

pub struct ProviderFactory;

impl ProviderFactory {
    pub fn create(provider: &str) -> Result<Box<dyn AIProvider>, AIError> {
        match normalize_provider(provider) {
            "claude" => Ok(Box::new(ClaudeProvider)),
            "openai" => Ok(Box::new(OpenAIProvider)),
            "gemini" => Ok(Box::new(GeminiProvider)),
            other => Err(AIError::UnknownProvider(other.to_string())),
        }
    }
}

pub fn default_models_for_provider(provider: &str) -> Vec<String> {
    match normalize_provider(provider) {
        "claude" => vec![
            "claude-3-5-sonnet-20241022".to_string(),
            "claude-3-5-haiku-20241022".to_string(),
            "claude-3-opus-20240229".to_string(),
        ],
        "openai" => vec![
            "gpt-4o-mini".to_string(),
            "gpt-4o".to_string(),
            "gpt-4.1-mini".to_string(),
        ],
        "gemini" => vec![
            "gemini-2.0-flash".to_string(),
            "gemini-1.5-pro".to_string(),
            "gemini-1.5-flash".to_string(),
        ],
        _ => vec![],
    }
}

fn normalize_provider(provider: &str) -> &str {
    match provider.trim().to_lowercase().as_str() {
        "claude" => "claude",
        "openai" => "openai",
        "gemini" => "gemini",
        _ => "unknown",
    }
}

fn rough_token_estimate(text: &str) -> usize {
    let words = text.split_whitespace().count();
    (words as f64 * 1.35).ceil().max(1.0) as usize
}

fn validate_key_basic(api_key: &str, provider: &str) -> Result<(), AIError> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return Err(AIError::MissingApiKey(provider.to_string()));
    }
    if trimmed.len() < 12 {
        return Err(AIError::InvalidApiKey(format!(
            "{} key is too short",
            provider
        )));
    }
    Ok(())
}

fn passthrough_refinement(
    provider: &dyn AIProvider,
    text: &str,
    model: &str,
    options: &RefinementOptions,
) -> RefinementResult {
    let input_tokens = rough_token_estimate(text);
    let output_tokens = input_tokens.min(options.max_tokens as usize);
    let cost = provider.estimate_cost_usd(model, input_tokens, output_tokens);
    RefinementResult {
        text: text.to_string(),
        usage: TokenUsage {
            input_tokens,
            output_tokens,
            total_cost_usd: cost,
        },
        provider: provider.id().to_string(),
        model: model.to_string(),
        execution_time_ms: 0,
    }
}

struct ClaudeProvider;
struct OpenAIProvider;
struct GeminiProvider;

impl AIProvider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn available_models(&self) -> Vec<String> {
        default_models_for_provider("claude")
    }

    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError> {
        validate_key_basic(api_key, self.id())?;
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
        // Approximation for planning UI only.
        (input_tokens as f64 * 0.000003) + (output_tokens as f64 * 0.000015)
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        self.validate_api_key(api_key)?;
        Ok(passthrough_refinement(self, text, model, options))
    }
}

impl AIProvider for OpenAIProvider {
    fn id(&self) -> &'static str {
        "openai"
    }

    fn available_models(&self) -> Vec<String> {
        default_models_for_provider("openai")
    }

    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError> {
        validate_key_basic(api_key, self.id())?;
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
        (input_tokens as f64 * 0.000005) + (output_tokens as f64 * 0.000015)
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        self.validate_api_key(api_key)?;
        Ok(passthrough_refinement(self, text, model, options))
    }
}

impl AIProvider for GeminiProvider {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn available_models(&self) -> Vec<String> {
        default_models_for_provider("gemini")
    }

    fn validate_api_key(&self, api_key: &str) -> Result<(), AIError> {
        validate_key_basic(api_key, self.id())?;
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, input_tokens: usize, output_tokens: usize) -> f64 {
        (input_tokens as f64 * 0.0000015) + (output_tokens as f64 * 0.0000035)
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        self.validate_api_key(api_key)?;
        Ok(passthrough_refinement(self, text, model, options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_creates_known_providers() {
        assert!(ProviderFactory::create("claude").is_ok());
        assert!(ProviderFactory::create("openai").is_ok());
        assert!(ProviderFactory::create("gemini").is_ok());
    }

    #[test]
    fn factory_rejects_unknown_provider() {
        let err = ProviderFactory::create("other").err();
        assert!(matches!(err, Some(AIError::UnknownProvider(_))));
    }
}
