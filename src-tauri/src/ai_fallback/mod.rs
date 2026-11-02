pub mod commands;
pub mod error;
pub mod model_info;
pub mod model_registry;
pub mod model_verify;
pub mod models;
pub mod provider;
#[cfg(test)]
pub(crate) mod test_support;

use crate::state::{save_settings_file, AppState, Settings};
use crate::{
    capability_enabled, require_capability_enabled, startup_status_snapshot, RuntimeCapability,
};
use models::{RefinementOptions, TierUpdate};
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
        // A poisoned lock is recovered like at every other settings access; failing here would
        // make every later settings change (model switch, update, rollback) fail for the rest
        // of the session.
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut settings)?;
        settings.clone()
    };
    save_settings_file(app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot);
    Ok(())
}

/// Persists adaptive tier growth reported by a completed Ollama refinement
/// (see `models::TierUpdate`). Called only from the production dictation
/// paths (PTT flow, manual re-refine command) — not from narrow-purpose
/// internal prompts (task capture, workflow agent replies) which deliberately
/// clamp `max_tokens` low and share the "custom" profile key, so growing
/// tiers there would pollute the tier the user's own custom prompt relies on.
pub(crate) fn persist_tier_update(
    app: &AppHandle,
    state: &AppState,
    prompt_profile: &str,
    tier_update: &TierUpdate,
) {
    let profile_key = models::normalize_prompt_profile_id(prompt_profile).to_string();
    let predict_idx = tier_update.num_predict_tier_idx;
    let ctx_idx = tier_update.num_ctx_tier_idx;
    // max(), not insert/assign: two refinements can race on the write lock
    // (e.g. PTT + a manual re-refine), each computed against a snapshot
    // that's slightly stale by the time it writes. Tiers must never appear
    // to shrink just because the smaller of two concurrent updates lands last.
    if let Err(error) = update_and_persist_settings(app, state, |settings| {
        let entry = settings
            .ai_fallback
            .adaptive_num_predict_tiers
            .entry(profile_key.clone())
            .or_insert(0);
        *entry = (*entry).max(predict_idx);
        settings.ai_fallback.adaptive_num_ctx_tier =
            settings.ai_fallback.adaptive_num_ctx_tier.max(ctx_idx);
        Ok(())
    }) {
        tracing::warn!(
            "[refinement] failed to persist adaptive tier growth: {}",
            error
        );
    }
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

/// Common result of preparing a refinement call: provider client,
/// resolved model name, whether the model resolution repaired a stale setting,
/// and the `RefinementOptions` to pass to the provider.
pub(crate) struct RefinementSetup {
    pub provider: provider::OllamaProvider,
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

    let state = app.state::<AppState>();
    let startup_status = startup_status_snapshot(state.inner());
    if !startup_status.ollama_ready {
        return Err(
            "Ollama refinement is not ready yet. Raw or rule-based fallback remains active."
                .to_string(),
        );
    }

    let endpoint = settings.providers.ollama.endpoint.clone();
    let provider = provider::OllamaProvider::new(endpoint.clone());

    provider::ping_ollama_quick(&endpoint).map_err(|error| error.to_string())?;

    let requested_model = ai.model.trim().to_string();
    let installed_models = provider::list_ollama_models(&endpoint);
    let resolution = resolve_ollama_refinement_model(settings, &requested_model, &installed_models)
        .ok_or_else(|| {
            "No installed Ollama models found. Download a model and set it active first."
                .to_string()
        })?;
    if crate::state::diagnostic_logging_enabled() {
        tracing::info!(
            "[refinement:prepare] ollama model resolution requested={} selected={} repaired={} installed={}",
            requested_model, resolution.model, resolution.repaired, installed_models.len()
        );
    }
    let model = resolution.model;
    let repaired = resolution.repaired;

    if model.is_empty() {
        return Err(
            "No local Ollama model configured. Download a model and set it active first."
                .to_string(),
        );
    }

    let effective_language = if settings.language_pinned {
        settings.language_mode.clone()
    } else {
        "auto".to_string()
    };
    let enforce_language_guard =
        should_enforce_language_guard(ai.preserve_source_language, &ai.prompt_profile);

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
        num_predict_tier_idx: ai
            .adaptive_num_predict_tiers
            .get(models::normalize_prompt_profile_id(&ai.prompt_profile))
            .copied()
            .unwrap_or(0),
        num_ctx_tier_idx: ai.adaptive_num_ctx_tier,
    };

    Ok(RefinementSetup {
        provider,
        model,
        repaired,
        options,
    })
}

/// Returns whether refinement output must stay in the source language.
///
/// The LLM Prompt profile deliberately emits English prompts, so it is exempt
/// even when the source-language preference is enabled.
pub(crate) fn should_enforce_language_guard(
    preserve_source_language: bool,
    prompt_profile: &str,
) -> bool {
    preserve_source_language && prompt_profile != "llm_prompt"
}

fn should_autostart_ai_refinement_runtime(settings: &Settings) -> bool {
    capability_enabled(settings, RuntimeCapability::AiRefinement)
        && settings.ai_fallback.provider == "ollama"
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

#[cfg(test)]
mod language_guard_tests {
    use super::should_enforce_language_guard;

    #[test]
    fn language_guard_follows_preserve_flag_for_builtin_and_custom_profiles() {
        assert!(should_enforce_language_guard(true, "wording"));
        assert!(should_enforce_language_guard(true, "custom"));
        assert!(!should_enforce_language_guard(false, "wording"));
        assert!(!should_enforce_language_guard(false, "custom"));
    }

    #[test]
    fn llm_prompt_profile_is_exempt_with_either_preserve_setting() {
        assert!(!should_enforce_language_guard(true, "llm_prompt"));
        assert!(!should_enforce_language_guard(false, "llm_prompt"));
    }
}
