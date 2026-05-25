pub mod commands;
pub mod error;
pub mod keyring;
pub mod models;
pub mod provider;

use crate::state::{save_settings_file, AppState, Settings};
use crate::{require_capability_enabled, startup_status_snapshot, RuntimeCapability};
use models::RefinementOptions;
use tauri::{AppHandle, Emitter, Manager};

/// Lock settings, apply a mutation, persist to disk, and emit a change event.
///
/// The closure receives `&mut Settings` and may return `Err` to abort. On
/// success the updated settings are saved and broadcast.
pub(crate) fn update_and_persist_settings<F>(
    app: &AppHandle,
    state: &AppState,
    f: F,
) -> Result<(), String>
where
    F: FnOnce(&mut Settings) -> Result<(), String>,
{
    let snapshot = {
        let mut settings = state.settings.write().map_err(|error| error.to_string())?;
        f(&mut settings)?;
        settings.clone()
    };
    save_settings_file(app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot);
    Ok(())
}

/// Guard that rejects requests when strict-local-mode is active and the
/// configured Ollama endpoint is not a local address.
pub(crate) fn check_strict_local_mode(settings: &Settings) -> Result<(), String> {
    if settings.ai_fallback.strict_local_mode
        && !provider::is_local_ollama_endpoint(&settings.providers.ollama.endpoint)
    {
        return Err(
            "Strict local mode is enabled. Only localhost/127.0.0.1 endpoints are allowed."
                .to_string(),
        );
    }
    Ok(())
}

/// Common result of preparing a refinement call: provider client, API key,
/// resolved model name, whether the model resolution repaired a stale setting,
/// and the `RefinementOptions` to pass to the provider.
pub(crate) struct RefinementSetup {
    pub provider: Box<dyn provider::AIProvider>,
    pub api_key: String,
    pub model: String,
    pub repaired: bool,
    pub options: RefinementOptions,
}

/// Append a "preserve these terms verbatim" clause to the refinement system
/// prompt. This prevents the LLM from mangling user-specific proper nouns
/// and acronyms during refinement (e.g. "MemPalace" -> "Mem Palace",
/// "XPBar" -> "xp bar"). Returns None if neither the base prompt nor the
/// terms list produce usable content.
fn augment_prompt_with_vocab_terms(base: Option<String>, terms: &[String]) -> Option<String> {
    let mut cleaned: Vec<&str> = terms
        .iter()
        .map(|term| term.trim())
        .filter(|term| !term.is_empty())
        .collect();
    if cleaned.is_empty() {
        return base;
    }

    let mut seen = std::collections::HashSet::new();
    cleaned.retain(|term| seen.insert(term.to_lowercase()));

    const MAX_TERMS_CHARS: usize = 600;
    let mut joined = String::new();
    for term in cleaned {
        let delim = if joined.is_empty() { "" } else { ", " };
        if joined.len() + delim.len() + term.len() > MAX_TERMS_CHARS {
            break;
        }
        joined.push_str(delim);
        joined.push_str(term);
    }
    if joined.is_empty() {
        return base;
    }

    let suffix = format!(
        "\n\nKnown terms (proper nouns, acronyms, product names) — preserve these exactly as written, do not translate or normalize them: {}",
        joined
    );
    match base {
        Some(base_prompt) if !base_prompt.trim().is_empty() => {
            Some(format!("{}{}", base_prompt, suffix))
        }
        _ => Some(suffix.trim_start().to_string()),
    }
}

/// Shared refinement-context preparation used by the Tauri command, the
/// benchmark helper, and the auto-refinement worker. Validates settings,
/// creates the provider client, resolves the model, and builds options.
///
/// Does **not** persist model-repair changes; callers decide if/how to do that.
pub(crate) fn prepare_refinement(
    app: &AppHandle,
    settings: &Settings,
) -> Result<RefinementSetup, String> {
    require_capability_enabled(settings, RuntimeCapability::AiRefinement)?;

    let ai = &settings.ai_fallback;

    let is_ollama = ai.provider == "ollama";
    let is_lm_studio = ai.provider == "lm_studio";
    let is_oobabooga = ai.provider == "oobabooga";
    let is_local_compat = is_lm_studio || is_oobabooga;

    if is_ollama {
        let state = app.state::<AppState>();
        let startup_status = startup_status_snapshot(state.inner());
        if !startup_status.ollama_ready {
            return Err(
                "Ollama refinement is not ready yet. Raw or rule-based fallback remains active."
                    .to_string(),
            );
        }
    }

    let provider: Box<dyn provider::AIProvider> = if is_ollama {
        check_strict_local_mode(settings)?;
        provider::ProviderFactory::create_ollama(settings.providers.ollama.endpoint.clone())
    } else if is_lm_studio {
        if let Err(error) = provider::ping_lm_studio_quick(&settings.providers.lm_studio.endpoint) {
            return Err(format!("LM Studio not ready: {}", error));
        }
        provider::ProviderFactory::create_lm_studio(
            settings.providers.lm_studio.endpoint.clone(),
            settings.providers.lm_studio.api_key.clone(),
        )
    } else if is_oobabooga {
        provider::ProviderFactory::create_oobabooga(
            settings.providers.oobabooga.endpoint.clone(),
            settings.providers.oobabooga.api_key.clone(),
        )
    } else {
        provider::ProviderFactory::create(&ai.provider).map_err(|error| error.to_string())?
    };

    let api_key = if is_ollama || is_local_compat {
        String::new()
    } else {
        keyring::read_api_key(app, &ai.provider)?
            .ok_or_else(|| format!("No API key stored for provider '{}'.", ai.provider))?
    };

    if !is_ollama && !is_local_compat {
        provider
            .validate_api_key(&api_key)
            .map_err(|error| error.to_string())?;
    }

    let mut model = ai.model.trim().to_string();
    let mut repaired = false;
    if is_ollama {
        let endpoint = settings.providers.ollama.endpoint.clone();
        let preferred = settings.providers.ollama.preferred_model.clone();
        provider::ping_ollama_quick(&endpoint).map_err(|error| error.to_string())?;
        let resolved = provider::resolve_effective_local_model(&model, &preferred, &endpoint)
            .map_err(|error| error.to_string())?;
        repaired = resolved.repaired
            || settings.ai_fallback.model.trim() != resolved.model
            || settings.providers.ollama.preferred_model.trim() != resolved.model
            || settings.postproc_llm_model.trim() != resolved.model;
        model = resolved.model;
    } else if is_lm_studio && model.is_empty() {
        model = settings
            .providers
            .lm_studio
            .preferred_model
            .trim()
            .to_string();
    } else if is_oobabooga && model.is_empty() {
        model = settings
            .providers
            .oobabooga
            .preferred_model
            .trim()
            .to_string();
    } else if model.is_empty() {
        model = match ai.provider.as_str() {
            "claude" => settings.providers.claude.preferred_model.trim().to_string(),
            "openai" => settings.providers.openai.preferred_model.trim().to_string(),
            "gemini" => settings.providers.gemini.preferred_model.trim().to_string(),
            _ => String::new(),
        };
    }

    if model.is_empty() {
        return Err(if is_ollama {
            "No local Ollama model configured. Download a model and set it active first."
                .to_string()
        } else if is_lm_studio {
            "No model selected for LM Studio. Load a model in LM Studio and set it active."
                .to_string()
        } else if is_oobabooga {
            "No model selected for Oobabooga. Load a model and set it active in settings."
                .to_string()
        } else {
            "No cloud model configured for the selected provider.".to_string()
        });
    }

    let effective_language = if settings.language_pinned {
        settings.language_mode.clone()
    } else {
        "auto".to_string()
    };
    let enforce_language_guard = ai.preserve_source_language && ai.prompt_profile != "llm_prompt";

    let options = RefinementOptions {
        temperature: ai.temperature,
        max_tokens: ai.max_tokens,
        low_latency_mode: ai.low_latency_mode,
        language: Some(effective_language.clone()),
        custom_prompt: augment_prompt_with_vocab_terms(
            provider::prompt_for_profile(
                &ai.prompt_profile,
                &effective_language,
                Some(ai.custom_prompt.as_str()),
                ai.preserve_source_language,
            ),
            &settings.vocab_terms,
        ),
        enforce_language_guard,
        prompt_profile: ai.prompt_profile.clone(),
    };

    Ok(RefinementSetup {
        provider,
        api_key,
        model,
        repaired,
        options,
    })
}
