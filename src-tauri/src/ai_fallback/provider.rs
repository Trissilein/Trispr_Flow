use super::error::AIError;
use super::models::{RefinementOptions, RefinementResult, TokenUsage};
use std::time::{Duration, Instant};

// Prompt templates optimized for local models (Ollama: qwen, mistral)
pub const OLLAMA_PROMPT_EN: &str = "Fix this transcribed text: correct punctuation, capitalization, and obvious errors. Keep the meaning unchanged. Return only the corrected text.";

pub const OLLAMA_PROMPT_DE: &str = "Korrigiere diesen transkribierten Text: verbessere Zeichensetzung, Großschreibung und offensichtliche Fehler. Behalte die Bedeutung bei. Gib nur den korrigierten Text zurück.";

pub trait AIProvider: Send + Sync {
    fn id(&self) -> &'static str;
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
            "ollama" => Ok(Box::new(OllamaProvider::new())),
            other => Err(AIError::UnknownProvider(other.to_string())),
        }
    }

    pub fn create_ollama(endpoint: String) -> Box<dyn AIProvider> {
        Box::new(OllamaProvider::with_endpoint(endpoint))
    }
}

/// Fetch model list from a running Ollama instance via GET /api/tags.
/// Returns an empty Vec on any error (Ollama not running, network issue, etc.).
pub fn list_ollama_models(endpoint: &str) -> Vec<String> {
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(5))
        .build();
    let resp = match agent.get(&url).call() {
        Ok(r) => r,
        Err(_) => return vec![],
    };
    let json: serde_json::Value = match resp.into_json() {
        Ok(j) => j,
        Err(_) => return vec![],
    };
    json["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Test whether Ollama is reachable at the given endpoint.
/// Returns Ok(()) if the server responds, Err(AIError::OllamaNotRunning) otherwise.
pub fn ping_ollama(endpoint: &str) -> Result<(), AIError> {
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(5))
        .build();
    match agent.get(&url).call() {
        Ok(_) => Ok(()),
        Err(ureq::Error::Transport(_)) => Err(AIError::OllamaNotRunning),
        Err(ureq::Error::Status(_, _)) => Err(AIError::OllamaNotRunning),
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
        "ollama" => vec![], // Models discovered dynamically via /api/tags
        _ => vec![],
    }
}

pub fn default_prompt_for_language(language: &str) -> &'static str {
    match language.trim().to_lowercase().as_str() {
        "de" | "german" => OLLAMA_PROMPT_DE,
        _ => OLLAMA_PROMPT_EN,
    }
}

fn normalize_provider(provider: &str) -> &str {
    match provider.trim().to_lowercase().as_str() {
        "claude" => "claude",
        "openai" => "openai",
        "gemini" => "gemini",
        "ollama" => "ollama",
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

#[derive(Clone)]
struct OllamaProvider {
    endpoint: String,
}

impl OllamaProvider {
    fn new() -> Self {
        Self {
            endpoint: "http://localhost:11434".to_string(),
        }
    }

    fn with_endpoint(endpoint: String) -> Self {
        Self { endpoint }
    }
}

impl AIProvider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
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

impl AIProvider for OllamaProvider {
    fn id(&self) -> &'static str {
        "ollama"
    }

    fn validate_api_key(&self, _api_key: &str) -> Result<(), AIError> {
        // Ollama doesn't use API keys
        Ok(())
    }

    fn estimate_cost_usd(&self, _model: &str, _input_tokens: usize, _output_tokens: usize) -> f64 {
        // Local Ollama has no cost
        0.0
    }

    fn refine_transcript(
        &self,
        text: &str,
        model: &str,
        options: &RefinementOptions,
        _api_key: &str,
    ) -> Result<RefinementResult, AIError> {
        let start = Instant::now();
        let url = format!("{}/api/chat", self.endpoint.trim_end_matches('/'));

        let system_prompt = options
            .custom_prompt
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                let lang = options.language.as_deref().unwrap_or("en");
                default_prompt_for_language(lang)
            });

        let body = serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": text}
            ],
            "stream": false,
            "options": {
                "temperature": options.temperature,
                "num_predict": options.max_tokens
            }
        });

        let agent = ureq::builder()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(30))
            .build();

        let resp = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|e| match e {
                ureq::Error::Status(404, _) => {
                    AIError::NetworkError(format!("Model '{}' not found in Ollama. Pull it first with: ollama pull {}", model, model))
                }
                ureq::Error::Status(code, _) => {
                    AIError::NetworkError(format!("Ollama returned HTTP {}", code))
                }
                ureq::Error::Transport(t) => {
                    let msg = t.to_string();
                    if msg.contains("timed out") || msg.contains("timeout") {
                        AIError::Timeout
                    } else {
                        AIError::OllamaNotRunning
                    }
                }
            })?;

        let json: serde_json::Value = resp
            .into_json()
            .map_err(|e| AIError::NetworkError(format!("Failed to parse Ollama response: {}", e)))?;

        let refined_text = json["message"]["content"]
            .as_str()
            .ok_or_else(|| AIError::NetworkError("Unexpected Ollama response format: missing message.content".to_string()))?
            .trim()
            .to_string();

        let input_tokens = json["prompt_eval_count"].as_u64().unwrap_or(0) as usize;
        let output_tokens = json["eval_count"].as_u64().unwrap_or(0) as usize;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        Ok(RefinementResult {
            text: refined_text,
            usage: TokenUsage {
                input_tokens,
                output_tokens,
                total_cost_usd: 0.0,
            },
            provider: self.id().to_string(),
            model: model.to_string(),
            execution_time_ms: elapsed_ms,
        })
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
        assert!(ProviderFactory::create("ollama").is_ok());
    }

    #[test]
    fn factory_rejects_unknown_provider() {
        let err = ProviderFactory::create("other").err();
        assert!(matches!(err, Some(AIError::UnknownProvider(_))));
    }

    // --- Ollama: unreachable endpoint returns empty model list (no panic) ---
    #[test]
    fn list_ollama_models_returns_empty_on_bad_endpoint() {
        let models = list_ollama_models("http://127.0.0.1:19999");
        assert!(models.is_empty(), "expected empty list for unreachable endpoint");
    }

    // --- Ollama: unreachable endpoint returns OllamaNotRunning (no panic) ---
    #[test]
    fn ping_ollama_returns_error_on_bad_endpoint() {
        let result = ping_ollama("http://127.0.0.1:19999");
        assert!(
            matches!(result, Err(AIError::OllamaNotRunning)),
            "expected OllamaNotRunning for unreachable endpoint, got: {:?}",
            result
        );
    }

    // --- Prompt selection by language ---
    #[test]
    fn default_prompt_uses_english_for_unknown_language() {
        assert_eq!(default_prompt_for_language("en"), OLLAMA_PROMPT_EN);
        assert_eq!(default_prompt_for_language("fr"), OLLAMA_PROMPT_EN);
        assert_eq!(default_prompt_for_language(""), OLLAMA_PROMPT_EN);
    }

    #[test]
    fn default_prompt_uses_german_for_de() {
        assert_eq!(default_prompt_for_language("de"), OLLAMA_PROMPT_DE);
        assert_eq!(default_prompt_for_language("german"), OLLAMA_PROMPT_DE);
        assert_eq!(default_prompt_for_language("DE"), OLLAMA_PROMPT_DE);
    }

    // --- OllamaProvider: validate_api_key is always Ok (no key needed) ---
    #[test]
    fn ollama_provider_validates_any_key() {
        let p = OllamaProvider::new();
        assert!(p.validate_api_key("").is_ok());
        assert!(p.validate_api_key("some-random-key").is_ok());
    }

    // --- OllamaProvider: cost is always zero ---
    #[test]
    fn ollama_provider_has_zero_cost() {
        let p = OllamaProvider::new();
        let cost = p.estimate_cost_usd("llama3.2:3b", 10_000, 10_000);
        assert_eq!(cost, 0.0, "Ollama should never charge");
    }

    // --- default_models_for_provider returns empty for ollama (dynamic) ---
    #[test]
    fn ollama_default_models_are_empty() {
        let models = default_models_for_provider("ollama");
        assert!(models.is_empty(), "Ollama models are discovered at runtime");
    }

    // --- cloud providers have non-empty default model lists ---
    #[test]
    fn cloud_providers_have_default_models() {
        assert!(!default_models_for_provider("claude").is_empty());
        assert!(!default_models_for_provider("openai").is_empty());
        assert!(!default_models_for_provider("gemini").is_empty());
    }

    // --- ProviderFactory::create_ollama passes custom endpoint ---
    #[test]
    fn create_ollama_with_custom_endpoint() {
        let provider = ProviderFactory::create_ollama("http://my-server:11434".to_string());
        assert_eq!(provider.id(), "ollama");
    }

    // --- rough_token_estimate is sensible ---
    #[test]
    fn token_estimate_is_positive_for_non_empty_text() {
        let estimate = rough_token_estimate("hello world foo bar baz");
        assert!(estimate >= 5, "should be at least as many as words");
    }

    #[test]
    fn token_estimate_is_at_least_one_for_empty_text() {
        assert!(rough_token_estimate("") >= 1);
    }
}
