use serde::{Deserialize, Serialize};

use super::provider::default_models_for_provider;

const DEFAULT_PROVIDER: &str = "claude";
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_MAX_TOKENS: u32 = 4000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIFallbackSettings {
    pub enabled: bool,
    pub provider: String, // "claude" | "openai" | "gemini"
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub custom_prompt_enabled: bool,
    pub custom_prompt: String,
    pub use_default_prompt: bool,
}

impl Default for AIFallbackSettings {
    fn default() -> Self {
        let provider = DEFAULT_PROVIDER.to_string();
        let model = default_models_for_provider(&provider)
            .into_iter()
            .next()
            .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
        Self {
            enabled: false,
            provider,
            model,
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
            custom_prompt_enabled: false,
            custom_prompt:
                "Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.".to_string(),
            use_default_prompt: true,
        }
    }
}

impl AIFallbackSettings {
    pub fn normalize(&mut self) {
        let provider = normalize_provider_id(&self.provider);
        self.provider = provider.to_string();

        self.temperature = self.temperature.clamp(0.0, 1.0);
        if self.max_tokens < 128 {
            self.max_tokens = 128;
        }
        if self.max_tokens > 8192 {
            self.max_tokens = 8192;
        }
        if self.custom_prompt.trim().is_empty() {
            self.custom_prompt = AIFallbackSettings::default().custom_prompt;
        }
        if self.model.trim().is_empty() {
            self.model = default_models_for_provider(&self.provider)
                .into_iter()
                .next()
                .unwrap_or_else(|| "claude-3-5-sonnet-20241022".to_string());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIProviderSettings {
    pub api_key_stored: bool,
    pub available_models: Vec<String>,
    pub preferred_model: String,
}

impl AIProviderSettings {
    fn with_provider_defaults(provider: &str) -> Self {
        let mut models = default_models_for_provider(provider);
        if models.is_empty() {
            models.push("unknown".to_string());
        }
        let preferred_model = models[0].clone();
        Self {
            api_key_stored: false,
            available_models: models,
            preferred_model,
        }
    }

    fn normalize_for_provider(&mut self, provider: &str) {
        if self.available_models.is_empty() {
            self.available_models = default_models_for_provider(provider);
        }
        if self.available_models.is_empty() {
            self.available_models.push("unknown".to_string());
        }
        if self.preferred_model.trim().is_empty()
            || !self.available_models.iter().any(|m| m == &self.preferred_model)
        {
            self.preferred_model = self.available_models[0].clone();
        }
    }
}

impl Default for AIProviderSettings {
    fn default() -> Self {
        Self {
            api_key_stored: false,
            available_models: Vec::new(),
            preferred_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIProvidersSettings {
    pub claude: AIProviderSettings,
    pub openai: AIProviderSettings,
    pub gemini: AIProviderSettings,
}

impl Default for AIProvidersSettings {
    fn default() -> Self {
        Self {
            claude: AIProviderSettings::with_provider_defaults("claude"),
            openai: AIProviderSettings::with_provider_defaults("openai"),
            gemini: AIProviderSettings::with_provider_defaults("gemini"),
        }
    }
}

impl AIProvidersSettings {
    pub fn get(&self, provider: &str) -> Option<&AIProviderSettings> {
        match normalize_provider_id(provider) {
            "claude" => Some(&self.claude),
            "openai" => Some(&self.openai),
            "gemini" => Some(&self.gemini),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, provider: &str) -> Option<&mut AIProviderSettings> {
        match normalize_provider_id(provider) {
            "claude" => Some(&mut self.claude),
            "openai" => Some(&mut self.openai),
            "gemini" => Some(&mut self.gemini),
            _ => None,
        }
    }

    pub fn normalize(&mut self) {
        self.claude.normalize_for_provider("claude");
        self.openai.normalize_for_provider("openai");
        self.gemini.normalize_for_provider("gemini");
    }

    pub fn sync_from_ai_fallback(&mut self, fallback: &AIFallbackSettings) {
        self.normalize();
        let provider = normalize_provider_id(&fallback.provider);
        if let Some(config) = self.get_mut(provider) {
            if !config.available_models.iter().any(|m| m == &fallback.model) {
                config.available_models.push(fallback.model.clone());
            }
            config.preferred_model = fallback.model.clone();
        }
    }

    pub fn set_api_key_stored(&mut self, provider: &str, stored: bool) -> Result<(), String> {
        let provider_config = self
            .get_mut(provider)
            .ok_or_else(|| format!("Unknown AI provider: {}", provider))?;
        provider_config.api_key_stored = stored;
        Ok(())
    }

}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementOptions {
    pub temperature: f32,
    pub max_tokens: u32,
    pub language: Option<String>,
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementResult {
    pub text: String,
    pub usage: TokenUsage,
    pub provider: String,
    pub model: String,
    pub execution_time_ms: u64,
}

pub fn normalize_provider_id(provider: &str) -> &str {
    match provider.trim().to_lowercase().as_str() {
        "claude" => "claude",
        "openai" => "openai",
        "gemini" => "gemini",
        _ => DEFAULT_PROVIDER,
    }
}
