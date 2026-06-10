pub mod commands;
pub mod error;
pub mod keyring;
pub mod models;
pub mod provider;

use crate::state::{save_settings_file, AppState, Settings};
use crate::{
    capability_enabled, require_capability_enabled, startup_status_snapshot, RuntimeCapability,
};
use models::RefinementOptions;
use std::thread;
use std::time::{Duration, Instant};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OllamaModelResolution {
    pub model: String,
    pub repaired: bool,
}

/// Resolve which Ollama model to use for refinement.
/// Falls back through: requested → preferred_model → postproc_llm_model → first installed.
/// Returns None only when no model is installed at all.
pub(crate) fn resolve_ollama_refinement_model(
    settings: &crate::state::Settings,
    requested_model: &str,
    installed_models: &[String],
) -> Option<OllamaModelResolution> {
    let find = |candidate: &str| -> Option<&str> {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            return None;
        }
        installed_models
            .iter()
            .map(String::as_str)
            .find(|m| m.trim().eq_ignore_ascii_case(candidate))
    };

    let requested = requested_model.trim();
    let selected = find(requested)
        .or_else(|| find(settings.providers.ollama.preferred_model.trim()))
        .or_else(|| find(settings.postproc_llm_model.trim()))
        .or_else(|| {
            installed_models
                .iter()
                .map(String::as_str)
                .find(|m| !m.trim().is_empty())
        })?
        .to_string();

    let repaired = requested.is_empty() || !selected.eq_ignore_ascii_case(requested);
    Some(OllamaModelResolution {
        model: selected,
        repaired,
    })
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
        provider::ping_ollama_quick(&endpoint).map_err(|error| error.to_string())?;

        let installed_models = provider::list_ollama_models(&endpoint);
        let resolution = resolve_ollama_refinement_model(settings, &model, &installed_models)
            .ok_or_else(|| {
                "No installed Ollama models found. Download a model and set it active first."
                    .to_string()
            })?;
        if crate::state::diagnostic_logging_enabled() {
            tracing::info!(
                "[refinement:prepare] ollama model resolution requested={} selected={} repaired={} installed={}",
                model, resolution.model, resolution.repaired, installed_models.len()
            );
        }
        model = resolution.model;
        repaired = resolution.repaired;
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

    let low_latency_decision =
        crate::refinement_adaptation::resolve_adaptive_low_latency(app, settings, &model);
    if crate::state::diagnostic_logging_enabled() {
        tracing::info!(
            "[refinement:prepare] low_latency={} reason={}",
            low_latency_decision.enabled,
            low_latency_decision.reason
        );
    }

    let options = RefinementOptions {
        temperature: ai.temperature,
        max_tokens: ai.max_tokens,
        low_latency_mode: low_latency_decision.enabled,
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

fn should_autostart_ai_refinement_runtime(settings: &Settings) -> bool {
    capability_enabled(settings, RuntimeCapability::AiRefinement)
        && settings.ai_fallback.provider == "ollama"
        && settings.ai_fallback.execution_mode == "local_primary"
}

pub(crate) fn ensure_ollama_runtime_ready_for_refinement(
    app: &AppHandle,
    settings: &Settings,
) -> Result<(), String> {
    let endpoint = settings.providers.ollama.endpoint.trim().to_string();
    let state = app.state::<AppState>();
    let startup_status = startup_status_snapshot(state.inner());
    let autostart = should_autostart_ai_refinement_runtime(settings);
    if crate::state::diagnostic_logging_enabled() {
        tracing::info!(
            "[ollama.runtime] ensure_ready_for_refinement start endpoint={} autostart={} ready={} starting={}",
            endpoint,
            autostart,
            startup_status.ollama_ready,
            startup_status.ollama_starting
        );
    }
    if !autostart {
        if crate::state::diagnostic_logging_enabled() {
            tracing::info!(
                "[ollama.runtime] ensure_ready_for_refinement skipped (autostart disabled) endpoint={}",
                endpoint
            );
        }
        return Ok(());
    }

    if startup_status.ollama_ready {
        if crate::state::diagnostic_logging_enabled() {
            tracing::info!(
                "[ollama.runtime] ensure_ready_for_refinement already ready endpoint={}",
                endpoint
            );
        }
        return Ok(());
    }

    let start_result = tauri::async_runtime::block_on(commands::start_ollama_runtime(app.clone()))
        .map_err(|error| format!("Failed to start Ollama runtime for refinement: {}", error))?;
    if crate::state::diagnostic_logging_enabled() {
        tracing::info!(
            "[ollama.runtime] ensure_ready_for_refinement start result pending_start={} startup_wait_ms={}",
            start_result.pending_start,
            start_result.startup_wait_ms
        );
    }
    if start_result.pending_start {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline && !startup_status_snapshot(state.inner()).ollama_ready {
            if provider::ping_ollama_quick(&endpoint).is_ok() {
                break;
            }
            thread::sleep(Duration::from_millis(500));
        }
    }
    tauri::async_runtime::block_on(commands::verify_ollama_runtime(app.clone()))
        .map_err(|error| format!("Failed to verify Ollama runtime for refinement: {}", error))?;

    if !startup_status_snapshot(state.inner()).ollama_ready {
        if crate::state::diagnostic_logging_enabled() {
            tracing::info!(
                "[ollama.runtime] ensure_ready_for_refinement still not ready endpoint={}",
                endpoint
            );
        }
        return Err("Ollama runtime is still not ready after on-demand start.".to_string());
    }

    if crate::state::diagnostic_logging_enabled() {
        tracing::info!(
            "[ollama.runtime] ensure_ready_for_refinement ready endpoint={}",
            endpoint
        );
    }
    Ok(())
}

#[cfg(test)]
mod ollama_model_resolution_tests {
    use super::*;
    use crate::state::Settings;

    fn settings_with_models(preferred: &str, postproc: &str) -> Settings {
        let mut s = Settings::default();
        s.providers.ollama.preferred_model = preferred.to_string();
        s.postproc_llm_model = postproc.to_string();
        s
    }

    #[test]
    fn ollama_model_resolution_keeps_installed_model() {
        let s = settings_with_models("qwen2.5-coder:14b", "");
        let installed = vec!["qwen2.5-coder:14b".to_string(), "llama3:8b".to_string()];
        let res = resolve_ollama_refinement_model(&s, "qwen2.5-coder:14b", &installed).unwrap();
        assert_eq!(res.model, "qwen2.5-coder:14b");
        assert!(!res.repaired);
    }

    #[test]
    fn ollama_model_resolution_repairs_stale_config_to_installed_preference() {
        let s = settings_with_models("qwen2.5-coder:14b", "");
        let installed = vec!["qwen2.5-coder:14b".to_string()];
        // requested model is stale (not in installed list)
        let res = resolve_ollama_refinement_model(&s, "old-model:7b", &installed).unwrap();
        assert_eq!(res.model, "qwen2.5-coder:14b");
        assert!(res.repaired);
    }

    #[test]
    fn ollama_model_resolution_empty_config_falls_back_to_first_installed() {
        let s = settings_with_models("", "");
        let installed = vec!["llama3:8b".to_string()];
        let res = resolve_ollama_refinement_model(&s, "", &installed).unwrap();
        assert_eq!(res.model, "llama3:8b");
        assert!(res.repaired);
    }

    #[test]
    fn ollama_model_resolution_returns_none_when_no_models_installed() {
        let s = settings_with_models("", "");
        let res = resolve_ollama_refinement_model(&s, "any", &[]);
        assert!(res.is_none());
    }

    #[test]
    fn ollama_model_resolution_case_insensitive_match() {
        let s = settings_with_models("", "");
        let installed = vec!["Qwen2.5-Coder:14b".to_string()];
        let res = resolve_ollama_refinement_model(&s, "qwen2.5-coder:14b", &installed).unwrap();
        assert_eq!(res.model, "Qwen2.5-Coder:14b");
        assert!(!res.repaired);
    }
}
