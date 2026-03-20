use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::provider::default_models_for_provider;

const DEFAULT_PROVIDER: &str = "ollama";
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_MAX_TOKENS: u32 = 4000;
const DEFAULT_EXECUTION_MODE: &str = "local_primary";
const DEFAULT_PROMPT_PROFILE: &str = "wording";
const USER_PROMPT_PRESET_PREFIX: &str = "user:";
const DEFAULT_PRESERVE_SOURCE_LANGUAGE: bool = true;
const AUTH_STATUS_LOCKED: &str = "locked";
const AUTH_STATUS_VERIFIED_API_KEY: &str = "verified_api_key";
const AUTH_STATUS_VERIFIED_OAUTH: &str = "verified_oauth";
const AUTH_METHOD_API_KEY: &str = "api_key";
const AUTH_METHOD_OAUTH: &str = "oauth";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserPromptPreset {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

impl Default for UserPromptPreset {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            prompt: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIFallbackSettings {
    pub enabled: bool,
    pub provider: String, // "claude" | "openai" | "gemini" | "ollama"
    pub fallback_provider: Option<String>, // "claude" | "openai" | "gemini"
    pub execution_mode: String, // "local_primary" | "online_fallback"
    pub strict_local_mode: bool,
    pub preserve_source_language: bool,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub low_latency_mode: bool,
    pub prompt_profile: String, // "wording" | "summary" | "technical_specs" | "action_items" | "custom"
    pub custom_prompt_enabled: bool,
    pub custom_prompt: String,
    pub use_default_prompt: bool,
    pub prompt_presets: Vec<UserPromptPreset>,
    pub active_prompt_preset_id: String, // "wording" | "summary" | ... | "custom" | "user:<id>"
}

impl Default for AIFallbackSettings {
    fn default() -> Self {
        let provider = DEFAULT_PROVIDER.to_string();
        let model = default_models_for_provider(&provider)
            .into_iter()
            .next()
            .unwrap_or_default();
        Self {
            enabled: false,
            provider,
            fallback_provider: None,
            execution_mode: DEFAULT_EXECUTION_MODE.to_string(),
            strict_local_mode: true,
            preserve_source_language: DEFAULT_PRESERVE_SOURCE_LANGUAGE,
            model,
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
            low_latency_mode: false,
            prompt_profile: DEFAULT_PROMPT_PROFILE.to_string(),
            custom_prompt_enabled: false,
            custom_prompt:
                "Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.".to_string(),
            use_default_prompt: true,
            prompt_presets: Vec::new(),
            active_prompt_preset_id: DEFAULT_PROMPT_PROFILE.to_string(),
        }
    }
}

impl AIFallbackSettings {
    pub fn normalize(&mut self) {
        self.fallback_provider = self
            .fallback_provider
            .as_ref()
            .and_then(|provider| normalize_cloud_provider_id(provider).map(str::to_string));

        if self.execution_mode != "local_primary" && self.execution_mode != "online_fallback" {
            self.execution_mode = DEFAULT_EXECUTION_MODE.to_string();
        }

        let normalized_provider = normalize_provider_id(&self.provider).to_string();
        const LOCAL_BACKENDS: &[&str] = &["ollama", "lm_studio", "oobabooga"];
        self.provider = if self.execution_mode == "online_fallback" {
            self.fallback_provider
                .clone()
                .unwrap_or_else(|| DEFAULT_PROVIDER.to_string())
        } else if LOCAL_BACKENDS.contains(&self.provider.as_str()) {
            // Preserve the currently selected local backend.
            self.provider.clone()
        } else {
            DEFAULT_PROVIDER.to_string()
        };
        if !LOCAL_BACKENDS.contains(&self.provider.as_str()) && self.fallback_provider.is_none() {
            self.fallback_provider =
                normalize_cloud_provider_id(&normalized_provider).map(str::to_string);
        }

        self.temperature = self.temperature.clamp(0.0, 1.0);
        if self.max_tokens < 128 {
            self.max_tokens = 128;
        }
        if self.max_tokens > 8192 {
            self.max_tokens = 8192;
        }
        let normalized_profile = normalize_prompt_profile_id(&self.prompt_profile);
        if normalized_profile.is_empty() {
            if self.custom_prompt_enabled && !self.use_default_prompt {
                self.prompt_profile = "custom".to_string();
            } else {
                self.prompt_profile = DEFAULT_PROMPT_PROFILE.to_string();
            }
        } else {
            self.prompt_profile = normalized_profile.to_string();
        }
        self.prompt_presets =
            normalize_user_prompt_presets(std::mem::take(&mut self.prompt_presets));
        self.active_prompt_preset_id = normalize_active_prompt_preset_id(
            &self.active_prompt_preset_id,
            &self.prompt_profile,
            &self.prompt_presets,
        );
        if let Some(selected_user_preset) =
            user_prompt_preset_from_option_id(&self.active_prompt_preset_id, &self.prompt_presets)
        {
            self.prompt_profile = "custom".to_string();
            self.custom_prompt_enabled = true;
            self.custom_prompt = selected_user_preset.prompt.clone();
        }
        if self.custom_prompt.trim().is_empty() {
            self.custom_prompt = AIFallbackSettings::default().custom_prompt;
        }
        if self.model.trim().is_empty() {
            // For Ollama: models are discovered at runtime, don't default to a cloud model.
            let defaults = default_models_for_provider(&self.provider);
            if let Some(first) = defaults.into_iter().next() {
                self.model = first;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIProviderSettings {
    pub api_key_stored: bool,
    pub auth_method_preference: String, // "api_key" | "oauth"
    pub auth_status: String,            // "locked" | "verified_api_key" | "verified_oauth"
    pub auth_verified_at: Option<String>,
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
            auth_method_preference: AUTH_METHOD_API_KEY.to_string(),
            auth_status: AUTH_STATUS_LOCKED.to_string(),
            auth_verified_at: None,
            available_models: models,
            preferred_model,
        }
    }

    fn normalize_for_provider(&mut self, provider: &str) {
        if !is_valid_auth_method_preference(&self.auth_method_preference) {
            self.auth_method_preference = AUTH_METHOD_API_KEY.to_string();
        }
        if !is_valid_auth_status(&self.auth_status) {
            self.auth_status = AUTH_STATUS_LOCKED.to_string();
            self.auth_verified_at = None;
        }
        if !self.api_key_stored && self.auth_status != AUTH_STATUS_VERIFIED_OAUTH {
            self.auth_status = AUTH_STATUS_LOCKED.to_string();
            self.auth_verified_at = None;
        }
        if self.available_models.is_empty() {
            self.available_models = default_models_for_provider(provider);
        }
        if self.available_models.is_empty() {
            self.available_models.push("unknown".to_string());
        }
        if self.preferred_model.trim().is_empty()
            || !self
                .available_models
                .iter()
                .any(|m| m == &self.preferred_model)
        {
            self.preferred_model = self.available_models[0].clone();
        }
    }
}

impl Default for AIProviderSettings {
    fn default() -> Self {
        Self {
            api_key_stored: false,
            auth_method_preference: AUTH_METHOD_API_KEY.to_string(),
            auth_status: AUTH_STATUS_LOCKED.to_string(),
            auth_verified_at: None,
            available_models: Vec::new(),
            preferred_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaSettings {
    pub endpoint: String,
    /// Additional endpoints tried in order when the primary endpoint is unreachable.
    /// Useful for alternate ports (e.g. 11435), LM Studio (1234), or Oobabooga (5000).
    pub fallback_endpoints: Vec<String>,
    pub available_models: Vec<String>,
    pub preferred_model: String,
    pub runtime_source: String, // "system" | "per_user_zip" | "manual"
    pub runtime_path: String,
    pub runtime_version: String,
    pub runtime_target_version: String,
    pub last_health_check: Option<String>,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:11434".to_string(),
            fallback_endpoints: Vec::new(),
            available_models: Vec::new(),
            preferred_model: String::new(),
            runtime_source: "manual".to_string(),
            runtime_path: String::new(),
            runtime_version: String::new(),
            runtime_target_version: "0.17.7".to_string(),
            last_health_check: None,
        }
    }
}

impl OllamaSettings {
    fn normalize(&mut self) {
        if self.endpoint.trim().is_empty() {
            self.endpoint = "http://127.0.0.1:11434".to_string();
        }
        if self.available_models.is_empty() {
            self.preferred_model = String::new();
        } else if self.preferred_model.trim().is_empty()
            || !self
                .available_models
                .iter()
                .any(|m| m == &self.preferred_model)
        {
            self.preferred_model = self.available_models[0].clone();
        }
        if self.runtime_source != "system"
            && self.runtime_source != "per_user_zip"
            && self.runtime_source != "manual"
        {
            self.runtime_source = "manual".to_string();
        }
        if self.runtime_target_version.trim().is_empty() {
            self.runtime_target_version = "0.17.7".to_string();
        }
    }
}

/// Settings for any OpenAI-compatible local backend (LM Studio, Oobabooga, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAICompatSettings {
    pub endpoint: String,
    /// Optional API key. LM Studio and Oobabooga usually don't require one.
    pub api_key: String,
    pub preferred_model: String,
    pub available_models: Vec<String>,
}

impl OpenAICompatSettings {
    pub fn lm_studio_defaults() -> Self {
        Self {
            endpoint: "http://127.0.0.1:1234".to_string(),
            api_key: String::new(),
            preferred_model: String::new(),
            available_models: Vec::new(),
        }
    }

    pub fn oobabooga_defaults() -> Self {
        Self {
            endpoint: "http://127.0.0.1:5000".to_string(),
            api_key: String::new(),
            preferred_model: String::new(),
            available_models: Vec::new(),
        }
    }
}

impl Default for OpenAICompatSettings {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            api_key: String::new(),
            preferred_model: String::new(),
            available_models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIProvidersSettings {
    pub claude: AIProviderSettings,
    pub openai: AIProviderSettings,
    pub gemini: AIProviderSettings,
    pub ollama: OllamaSettings,
    pub lm_studio: OpenAICompatSettings,
    pub oobabooga: OpenAICompatSettings,
}

impl Default for AIProvidersSettings {
    fn default() -> Self {
        Self {
            claude: AIProviderSettings::with_provider_defaults("claude"),
            openai: AIProviderSettings::with_provider_defaults("openai"),
            gemini: AIProviderSettings::with_provider_defaults("gemini"),
            ollama: OllamaSettings::default(),
            lm_studio: OpenAICompatSettings::lm_studio_defaults(),
            oobabooga: OpenAICompatSettings::oobabooga_defaults(),
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
        self.ollama.normalize();
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
        provider_config.auth_status = AUTH_STATUS_LOCKED.to_string();
        provider_config.auth_verified_at = None;
        Ok(())
    }

    pub fn set_auth_verified(
        &mut self,
        provider: &str,
        method: &str,
        verified_at: Option<String>,
    ) -> Result<(), String> {
        let provider_config = self
            .get_mut(provider)
            .ok_or_else(|| format!("Unknown AI provider: {}", provider))?;
        if method != AUTH_STATUS_VERIFIED_API_KEY && method != AUTH_STATUS_VERIFIED_OAUTH {
            return Err(format!("Unsupported auth verification method: {}", method));
        }
        provider_config.auth_status = method.to_string();
        provider_config.auth_verified_at = verified_at;
        Ok(())
    }

    pub fn lock_auth(&mut self, provider: &str) -> Result<(), String> {
        let provider_config = self
            .get_mut(provider)
            .ok_or_else(|| format!("Unknown AI provider: {}", provider))?;
        provider_config.auth_status = AUTH_STATUS_LOCKED.to_string();
        provider_config.auth_verified_at = None;
        Ok(())
    }

    pub fn is_verified(&self, provider: &str) -> bool {
        self.get(provider)
            .map(|config| config.auth_status != AUTH_STATUS_LOCKED)
            .unwrap_or(false)
    }
}

fn is_valid_auth_status(status: &str) -> bool {
    matches!(
        status.trim(),
        AUTH_STATUS_LOCKED | AUTH_STATUS_VERIFIED_API_KEY | AUTH_STATUS_VERIFIED_OAUTH
    )
}

fn is_valid_auth_method_preference(method: &str) -> bool {
    matches!(method.trim(), AUTH_METHOD_API_KEY | AUTH_METHOD_OAUTH)
}

pub(crate) fn normalize_cloud_provider_id(provider: &str) -> Option<&'static str> {
    match provider.trim().to_lowercase().as_str() {
        "claude" => Some("claude"),
        "openai" => Some("openai"),
        "gemini" => Some("gemini"),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementOptions {
    pub temperature: f32,
    pub max_tokens: u32,
    pub low_latency_mode: bool,
    pub language: Option<String>,
    pub custom_prompt: Option<String>,
    pub enforce_language_guard: bool,
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

pub fn normalize_provider_id(provider: &str) -> &'static str {
    match provider.trim().to_lowercase().as_str() {
        "claude" => "claude",
        "openai" => "openai",
        "gemini" => "gemini",
        "ollama" => "ollama",
        "lm_studio" => "lm_studio",
        "oobabooga" => "oobabooga",
        _ => DEFAULT_PROVIDER,
    }
}

pub fn normalize_prompt_profile_id(profile: &str) -> &'static str {
    match profile.trim().to_lowercase().as_str() {
        "wording" => "wording",
        "summary" => "summary",
        "technical_specs" => "technical_specs",
        "action_items" => "action_items",
        "llm_prompt" => "llm_prompt",
        "custom" => "custom",
        _ => "",
    }
}

fn sanitize_user_prompt_preset_id(id: &str) -> String {
    let lowered = id.trim().to_lowercase();
    let mut normalized = String::with_capacity(lowered.len());
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            normalized.push(ch);
        } else {
            normalized.push('-');
        }
    }
    normalized.trim_matches('-').to_string()
}

fn normalize_user_prompt_presets(presets: Vec<UserPromptPreset>) -> Vec<UserPromptPreset> {
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut cleaned: Vec<UserPromptPreset> = Vec::new();
    for preset in presets {
        let id = sanitize_user_prompt_preset_id(&preset.id);
        let name = preset.name.trim().to_string();
        let prompt = preset.prompt.trim().to_string();
        if id.is_empty() || name.is_empty() || prompt.is_empty() {
            continue;
        }
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        cleaned.push(UserPromptPreset { id, name, prompt });
    }
    cleaned
}

fn user_prompt_preset_from_option_id<'a>(
    option_id: &str,
    presets: &'a [UserPromptPreset],
) -> Option<&'a UserPromptPreset> {
    let normalized = option_id.trim().to_lowercase();
    let raw_id = normalized.strip_prefix(USER_PROMPT_PRESET_PREFIX)?;
    let preset_id = sanitize_user_prompt_preset_id(raw_id);
    if preset_id.is_empty() {
        return None;
    }
    presets.iter().find(|preset| preset.id == preset_id)
}

fn normalize_active_prompt_preset_id(
    active_prompt_preset_id: &str,
    prompt_profile: &str,
    presets: &[UserPromptPreset],
) -> String {
    let normalized_active = active_prompt_preset_id.trim().to_lowercase();
    let normalized_profile = normalize_prompt_profile_id(prompt_profile);
    let normalized_active_profile = normalize_prompt_profile_id(&normalized_active);

    if !normalized_active_profile.is_empty() {
        return normalized_active_profile.to_string();
    }
    if let Some(user_preset) = user_prompt_preset_from_option_id(&normalized_active, presets) {
        return format!("{}{}", USER_PROMPT_PRESET_PREFIX, user_preset.id);
    }
    if !normalized_profile.is_empty() {
        return normalized_profile.to_string();
    }
    DEFAULT_PROMPT_PROFILE.to_string()
}
