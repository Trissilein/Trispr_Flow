use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use super::provider::{NUM_CTX_TIERS, NUM_PREDICT_TIERS};

const DEFAULT_PROVIDER: &str = "ollama";
const DEFAULT_TEMPERATURE: f32 = 0.3;
const DEFAULT_MAX_TOKENS: u32 = 4000;
const DEFAULT_PROMPT_PROFILE: &str = "wording";
const USER_PROMPT_PRESET_PREFIX: &str = "user:";
const DEFAULT_PRESERVE_SOURCE_LANGUAGE: bool = true;
const DEFAULT_OLLAMA_ENDPOINT: &str = "http://127.0.0.1:11434";
const MANAGED_RUNTIME_SOURCE: &str = "per_user_zip";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UserPromptPreset {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_prompt: Option<String>,
}

impl Default for UserPromptPreset {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            prompt: String::new(),
            previous_prompt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AIFallbackSettings {
    pub enabled: bool,
    pub provider: String, // always "ollama"
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
    /// Per-built-in override strings. Keyed by built-in preset id (e.g. "wording").
    /// When present, replaces the factory default prompt for both EN and DE.
    #[serde(default)]
    pub prompt_preset_overrides: BTreeMap<String, String>,
    /// Index into `provider::NUM_CTX_TIERS`. Grows automatically (never shrinks)
    /// when a transcript threatens to overflow the current context window.
    /// Persisted so the app doesn't relearn the user's habitual transcript
    /// length after every restart.
    #[serde(default)]
    pub adaptive_num_ctx_tier: usize,
    /// Index into `provider::NUM_PREDICT_TIERS`, keyed by normalized prompt
    /// profile id (e.g. "summary"). Each profile grows independently since
    /// "wording" and "summary" have very different natural output lengths.
    #[serde(default)]
    pub adaptive_num_predict_tiers: BTreeMap<String, usize>,
}

impl Default for AIFallbackSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: DEFAULT_PROVIDER.to_string(),
            preserve_source_language: DEFAULT_PRESERVE_SOURCE_LANGUAGE,
            model: "gemma4:e4b".to_string(),
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
            prompt_preset_overrides: BTreeMap::new(),
            adaptive_num_ctx_tier: 0,
            adaptive_num_predict_tiers: BTreeMap::new(),
        }
    }
}

impl AIFallbackSettings {
    pub fn normalize(&mut self) {
        // Refinement is Ollama-only. Older settings may still name a cloud, LM Studio or
        // Oobabooga provider; they all land on Ollama.
        self.provider = DEFAULT_PROVIDER.to_string();

        self.temperature = self.temperature.clamp(0.0, 1.0);
        if self.max_tokens < 128 {
            self.max_tokens = 128;
        }
        if self.max_tokens > 8192 {
            self.max_tokens = 8192;
        }
        let max_ctx_tier_idx = NUM_CTX_TIERS.len() - 1;
        if self.adaptive_num_ctx_tier > max_ctx_tier_idx {
            self.adaptive_num_ctx_tier = max_ctx_tier_idx;
        }
        let max_predict_tier_idx = NUM_PREDICT_TIERS.len() - 1;
        for tier_idx in self.adaptive_num_predict_tiers.values_mut() {
            if *tier_idx > max_predict_tier_idx {
                *tier_idx = max_predict_tier_idx;
            }
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
        self.prompt_preset_overrides =
            normalize_prompt_preset_overrides(std::mem::take(&mut self.prompt_preset_overrides));
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
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaSettings {
    pub endpoint: String,
    pub available_models: Vec<String>,
    pub preferred_model: String,
    pub runtime_source: String, // always "per_user_zip" (managed runtime)
    pub runtime_path: String,
    pub runtime_version: String,
    pub runtime_target_version: String,
    /// The user picked `runtime_target_version` in the version list (Install / Switch to). Only
    /// then may it stay older than the running runtime; an implicit target is lifted instead.
    pub runtime_target_explicit: bool,
    pub last_health_check: Option<String>,
}

impl Default for OllamaSettings {
    fn default() -> Self {
        Self {
            endpoint: DEFAULT_OLLAMA_ENDPOINT.to_string(),
            available_models: Vec::new(),
            preferred_model: String::new(),
            runtime_source: MANAGED_RUNTIME_SOURCE.to_string(),
            runtime_path: String::new(),
            runtime_version: String::new(),
            runtime_target_version: "0.40.13".to_string(),
            runtime_target_explicit: false,
            last_health_check: None,
        }
    }
}

impl OllamaSettings {
    /// A target nobody chose that is older than the runtime we have recorded as running moves up
    /// to that runtime, so the UI never offers a downgrade the user did not ask for. True when
    /// changed.
    pub fn heal_stale_runtime_target(&mut self) -> bool {
        if self.runtime_target_explicit {
            return false;
        }
        let running = self.runtime_version.trim().trim_start_matches('v');
        let stale = matches!(
            (
                crate::ollama_runtime::parse_stable_semver(&self.runtime_target_version),
                crate::ollama_runtime::parse_stable_semver(running),
            ),
            (Some(target), Some(running)) if target < running
        );
        if stale {
            self.runtime_target_version = running.to_string();
        }
        stale
    }

    fn normalize(&mut self) {
        // The managed runtime always serves on this address; a stored remote or custom
        // endpoint (older versions let the user type one) is dropped.
        self.endpoint = DEFAULT_OLLAMA_ENDPOINT.to_string();
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
        if self.runtime_source != MANAGED_RUNTIME_SOURCE {
            // A system or manual runtime was somebody else's binary: forget its path and version.
            self.runtime_source = MANAGED_RUNTIME_SOURCE.to_string();
            self.runtime_path.clear();
            self.runtime_version.clear();
        }
        // Targets that older releases wrote as their default are not a choice the user made.
        const LEGACY_RUNTIME_TARGETS: [&str; 1] = ["0.20.2"];
        let target = self.runtime_target_version.trim();
        if target.is_empty()
            || (!self.runtime_target_explicit && LEGACY_RUNTIME_TARGETS.contains(&target))
        {
            self.runtime_target_version = "0.40.13".to_string();
            self.runtime_target_explicit = false;
        }
        self.heal_stale_runtime_target();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AIProvidersSettings {
    pub ollama: OllamaSettings,
}

impl AIProvidersSettings {
    pub fn normalize(&mut self) {
        self.ollama.normalize();
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
    pub prompt_profile: String,
    /// Persisted `provider::NUM_PREDICT_TIERS` index for this profile, read
    /// from `AIFallbackSettings::adaptive_num_predict_tiers`. Ollama-only.
    #[serde(default)]
    pub num_predict_tier_idx: usize,
    /// Persisted `provider::NUM_CTX_TIERS` index, read from
    /// `AIFallbackSettings::adaptive_num_ctx_tier`. Ollama-only.
    #[serde(default)]
    pub num_ctx_tier_idx: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_cost_usd: f64,
}

/// Reports that an Ollama refinement request needed a bigger tier than what
/// was persisted in settings. The caller (which owns `AppState`/settings I/O)
/// persists this so future requests start with headroom already in place —
/// see `provider::grow_tier`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TierUpdate {
    pub num_predict_tier_idx: usize,
    pub num_ctx_tier_idx: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementResult {
    pub text: String,
    pub usage: TokenUsage,
    pub provider: String,
    pub model: String,
    pub execution_time_ms: u64,
    /// OLLAMA-internal: nanoseconds from response divided by 1_000_000.
    /// None for non-Ollama providers. >0 means model was cold-loaded this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_load_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_prompt_eval_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ollama_eval_ms: Option<u64>,
    /// Set only when this request grew a tier beyond what was passed in via
    /// `RefinementOptions`. `None` for non-Ollama providers and for requests
    /// that fit comfortably within the already-persisted tiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_update: Option<TierUpdate>,
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
        let previous_prompt = preset
            .previous_prompt
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty() && p != &prompt);
        cleaned.push(UserPromptPreset {
            id,
            name,
            prompt,
            previous_prompt,
        });
    }
    cleaned
}

fn normalize_prompt_preset_overrides(
    overrides: BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut cleaned: BTreeMap<String, String> = BTreeMap::new();
    for (key, value) in overrides {
        let id = normalize_prompt_profile_id(&key);
        // Only keep overrides for built-in preset ids; never allow "custom" as an override key.
        if id.is_empty() || id == "custom" {
            continue;
        }
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        cleaned.insert(id.to_string(), trimmed);
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
