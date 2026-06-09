use super::error::AIError;
use super::keyring as ai_fallback_keyring;
use super::provider::{
    default_models_for_provider, is_local_ollama_endpoint, is_ssrf_target, list_ollama_models,
    list_ollama_models_with_size, ollama_endpoint_candidates, ping_ollama, ping_ollama_quick,
    ProviderFactory,
};
use super::{check_strict_local_mode, prepare_refinement, update_and_persist_settings};
use crate::state::{normalize_ai_fallback_fields, AppState};
use crate::{
    now_iso, terminate_managed_child_slot, update_runtime_diagnostics, update_startup_status,
};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

pub(crate) use crate::ollama_runtime::{
    detect_ollama_runtime, download_ollama_runtime, fetch_ollama_online_versions,
    import_ollama_model_from_file, install_ollama_runtime, list_ollama_runtime_versions,
    set_strict_local_mode, start_ollama_runtime, verify_ollama_runtime,
};

#[tauri::command]
pub(crate) async fn fetch_available_models(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<String>, String> {
    let provider_id = provider.trim().to_lowercase();
    if provider_id == "ollama" {
        let endpoint = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            check_strict_local_mode(&settings)?;
            settings.providers.ollama.endpoint.clone()
        };
        return tauri::async_runtime::spawn_blocking(move || {
            fetch_available_models_ollama_impl(endpoint)
        })
        .await
        .map_err(|e| format!("Fetch available models task failed: {}", e))?;
    }

    if provider_id == "lm_studio" || provider_id == "oobabooga" {
        let (endpoint, api_key) = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if provider_id == "lm_studio" {
                (
                    settings.providers.lm_studio.endpoint.clone(),
                    settings.providers.lm_studio.api_key.clone(),
                )
            } else {
                (
                    settings.providers.oobabooga.endpoint.clone(),
                    settings.providers.oobabooga.api_key.clone(),
                )
            }
        };
        return tauri::async_runtime::spawn_blocking(move || {
            let models = super::provider::list_openai_compat_models(&endpoint, &api_key);
            if models.is_empty() {
                Err(format!(
                    "No models found at {}. Is the server running?",
                    endpoint
                ))
            } else {
                Ok(models)
            }
        })
        .await
        .map_err(|e| format!("Fetch available models task failed: {}", e))?;
    }

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || fetch_available_models_impl(&app_handle, provider))
        .await
        .map_err(|e| format!("Fetch available models task failed: {}", e))?
}

fn fetch_available_models_ollama_impl(endpoint: String) -> Result<Vec<String>, String> {
    let models = list_ollama_models(&endpoint);
    if models.is_empty() {
        ping_ollama_quick(&endpoint).map_err(|e| e.to_string())?;
    }
    Ok(models)
}

fn fetch_available_models_impl(app: &AppHandle, provider: String) -> Result<Vec<String>, String> {
    let provider_id = provider.trim().to_lowercase();

    let from_settings = {
        let state = app.state::<AppState>();
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings
            .providers
            .get(&provider_id)
            .map(|cfg| cfg.available_models.clone())
            .unwrap_or_default()
    };

    if !from_settings.is_empty() {
        return Ok(from_settings);
    }

    let defaults = default_models_for_provider(&provider_id);
    if defaults.is_empty() {
        return Err(format!("Unknown AI provider: {}", provider));
    }
    Ok(defaults)
}

#[tauri::command]
pub(crate) async fn fetch_ollama_models_with_size(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        check_strict_local_mode(&settings)?;
        settings.providers.ollama.endpoint.clone()
    };
    tauri::async_runtime::spawn_blocking(move || fetch_ollama_models_with_size_impl(endpoint))
        .await
        .map_err(|e| format!("Fetch Ollama models task failed: {}", e))?
}

fn fetch_ollama_models_with_size_impl(endpoint: String) -> Result<Vec<serde_json::Value>, String> {
    let models = list_ollama_models_with_size(&endpoint);
    if models.is_empty() {
        ping_ollama_quick(&endpoint).map_err(|e| e.to_string())?;
    }
    Ok(models
        .into_iter()
        .map(|(name, size_bytes)| serde_json::json!({ "name": name, "size_bytes": size_bytes }))
        .collect())
}

#[tauri::command]
pub(crate) async fn test_provider_connection(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<serde_json::Value, String> {
    let provider_id = provider.trim().to_lowercase();

    if provider_id == "ollama" {
        let endpoint = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            check_strict_local_mode(&settings)?;
            settings.providers.ollama.endpoint.clone()
        };
        return tauri::async_runtime::spawn_blocking(move || {
            test_provider_connection_ollama_impl(endpoint)
        })
        .await
        .map_err(|e| format!("Test provider connection task failed: {}", e))?;
    }

    if provider_id == "lm_studio" || provider_id == "oobabooga" {
        let (endpoint, stored_key, label) = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if provider_id == "lm_studio" {
                (
                    settings.providers.lm_studio.endpoint.clone(),
                    settings.providers.lm_studio.api_key.clone(),
                    "LM Studio".to_string(),
                )
            } else {
                (
                    settings.providers.oobabooga.endpoint.clone(),
                    settings.providers.oobabooga.api_key.clone(),
                    "Oobabooga".to_string(),
                )
            }
        };
        let effective_key = if api_key.trim().is_empty() {
            stored_key
        } else {
            api_key
        };
        return tauri::async_runtime::spawn_blocking(move || {
            let models = super::provider::list_openai_compat_models(&endpoint, &effective_key);
            if models.is_empty() {
                Err(format!(
                    "{} not reachable at {}. Is the server running?",
                    label, endpoint
                ))
            } else {
                Ok(serde_json::json!({
                    "ok": true,
                    "provider": provider_id,
                    "message": format!("{} is running. {} model(s) available.", label, models.len()),
                    "models": models,
                }))
            }
        })
        .await
        .map_err(|e| format!("Test provider connection task failed: {}", e))?;
    }

    tauri::async_runtime::spawn_blocking(move || {
        test_provider_connection_impl(provider_id, api_key)
    })
    .await
    .map_err(|e| format!("Test provider connection task failed: {}", e))?
}

fn test_provider_connection_ollama_impl(endpoint: String) -> Result<serde_json::Value, String> {
    ping_ollama(&endpoint).map_err(|e| e.to_string())?;
    let models = list_ollama_models(&endpoint);
    Ok(serde_json::json!({
        "ok": true,
        "provider": "ollama",
        "message": format!("Ollama is running. {} model(s) available.", models.len()),
        "models": models,
    }))
}

fn test_provider_connection_impl(
    provider_id: String,
    api_key: String,
) -> Result<serde_json::Value, String> {
    let provider_client = ProviderFactory::create(&provider_id).map_err(|e| e.to_string())?;
    provider_client
        .validate_api_key(api_key.trim())
        .map_err(|e| e.to_string())?;

    Ok(serde_json::json!({
      "ok": true,
      "provider": provider_id,
      "message": "API key format looks valid. Live provider connection checks are activated with provider integrations.",
    }))
}

#[tauri::command]
pub(crate) fn save_provider_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<serde_json::Value, String> {
    let provider_id = provider.trim().to_lowercase();
    let provider_client = ProviderFactory::create(&provider_id).map_err(|e| e.to_string())?;
    provider_client
        .validate_api_key(api_key.trim())
        .map_err(|e| e.to_string())?;
    ai_fallback_keyring::store_api_key(&app, &provider_id, api_key.trim())?;

    update_and_persist_settings(&app, state.inner(), |settings| {
        settings.providers.set_api_key_stored(&provider_id, true)?;
        normalize_ai_fallback_fields(settings);
        Ok(())
    })?;

    Ok(serde_json::json!({
      "status": "success",
      "provider": provider_id,
      "stored": true,
      "auth_status": "locked",
    }))
}

#[tauri::command]
pub(crate) fn clear_provider_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<serde_json::Value, String> {
    let provider_id = provider.trim().to_lowercase();
    ai_fallback_keyring::clear_api_key(&app, &provider_id)?;

    update_and_persist_settings(&app, state.inner(), |settings| {
        settings.providers.set_api_key_stored(&provider_id, false)?;
        normalize_ai_fallback_fields(settings);
        Ok(())
    })?;

    Ok(serde_json::json!({
      "status": "success",
      "provider": provider_id,
      "stored": false,
      "auth_status": "locked",
    }))
}

#[tauri::command]
pub(crate) fn verify_provider_auth(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    method: Option<String>,
) -> Result<serde_json::Value, String> {
    let provider_id = provider.trim().to_lowercase();
    let method_id = method.as_deref().unwrap_or("api_key").trim().to_lowercase();

    if provider_id == "ollama" {
        return Err("Ollama does not require cloud credential verification.".to_string());
    }
    if !matches!(provider_id.as_str(), "claude" | "openai" | "gemini") {
        return Err(format!("Unknown AI provider: {}", provider));
    }
    if method_id != "api_key" && method_id != "oauth" {
        return Err(format!(
            "Unsupported auth verification method '{}'.",
            method_id
        ));
    }

    if method_id == "oauth" {
        update_and_persist_settings(&app, state.inner(), |settings| {
            settings.providers.lock_auth(&provider_id)?;
            normalize_ai_fallback_fields(settings);
            Ok(())
        })?;
        return Err(
            "OAuth verification is not supported yet. Use API key verification.".to_string(),
        );
    }

    let stored_key = ai_fallback_keyring::read_api_key(&app, &provider_id)?;
    let Some(api_key) = stored_key else {
        update_and_persist_settings(&app, state.inner(), |settings| {
            settings.providers.lock_auth(&provider_id)?;
            normalize_ai_fallback_fields(settings);
            Ok(())
        })?;
        return Err(format!(
            "No stored API key found for provider '{}'.",
            provider_id
        ));
    };

    let provider_client = ProviderFactory::create(&provider_id).map_err(|e| e.to_string())?;
    if let Err(error) = provider_client.validate_api_key(api_key.trim()) {
        update_and_persist_settings(&app, state.inner(), |settings| {
            settings.providers.lock_auth(&provider_id)?;
            normalize_ai_fallback_fields(settings);
            Ok(())
        })?;
        return Err(error.to_string());
    }

    let verified_at = now_iso();
    update_and_persist_settings(&app, state.inner(), |settings| {
        settings.providers.set_auth_verified(
            &provider_id,
            "verified_api_key",
            Some(verified_at.clone()),
        )?;
        normalize_ai_fallback_fields(settings);
        Ok(())
    })?;

    Ok(serde_json::json!({
      "ok": true,
      "provider": provider_id,
      "method": "verified_api_key",
      "verified_at": verified_at,
      "message": "Provider credentials verified successfully.",
    }))
}

#[tauri::command]
pub(crate) fn save_ollama_endpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
) -> Result<serde_json::Value, String> {
    let trimmed = endpoint.trim().to_string();
    if trimmed.is_empty() {
        return Err("Endpoint cannot be empty.".to_string());
    }
    if is_ssrf_target(&trimmed) {
        return Err("This endpoint address is not allowed (SSRF protection).".to_string());
    }
    {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if settings.ai_fallback.strict_local_mode && !is_local_ollama_endpoint(&trimmed) {
            return Err(
                "Strict local mode is enabled. Only localhost/127.0.0.1:11434 is allowed."
                    .to_string(),
            );
        }
    }
    update_and_persist_settings(&app, state.inner(), |settings| {
        settings.providers.ollama.endpoint = trimmed.clone();
        Ok(())
    })?;
    Ok(serde_json::json!({
        "status": "success",
        "endpoint": trimmed,
    }))
}

#[tauri::command]
pub(crate) async fn refine_transcript(
    app: AppHandle,
    state: State<'_, AppState>,
    transcript: String,
) -> Result<serde_json::Value, String> {
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    let setup = prepare_refinement(&app, &settings_snapshot)?;

    if setup.repaired {
        let model = setup.model.clone();
        update_and_persist_settings(&app, state.inner(), |settings| {
            settings.ai_fallback.model = model.clone();
            settings.providers.ollama.preferred_model = model.clone();
            settings.postproc_llm_model = model;
            normalize_ai_fallback_fields(settings);
            Ok(())
        })?;
    }

    let app_clone = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        setup
            .provider
            .refine_transcript(&transcript, &setup.model, &setup.options, &setup.api_key)
    })
    .await
    .map_err(|e| format!("refine_transcript task failed: {}", e))?;

    if let Err(AIError::Timeout | AIError::OllamaNotRunning) = &result {
        let _ = app_clone.emit("ai_fallback:health_degraded", ());
    }

    let result = result.map_err(|e| e.to_string())?;
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn ping_refinement_model(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    if !settings_snapshot.ai_fallback.enabled {
        return Ok(false);
    }

    let provider = &settings_snapshot.ai_fallback.provider;
    if provider != "ollama" && provider != "lm_studio" && provider != "oobabooga" {
        return Ok(false);
    }

    let setup = match prepare_refinement(&app, &settings_snapshot) {
        Ok(setup) => setup,
        Err(_) => return Ok(false),
    };

    let ping_text = ".";

    let result = tauri::async_runtime::spawn_blocking(move || {
        setup
            .provider
            .refine_transcript(ping_text, &setup.model, &setup.options, &setup.api_key)
    })
    .await
    .map_err(|e| format!("ping_refinement_model task failed: {}", e))?;

    Ok(result.is_ok())
}

#[tauri::command]
pub(crate) fn pull_ollama_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    use super::provider::{
        precheck_ollama_registry_model_tag, pull_ollama_model_inner, validate_ollama_model_name,
    };

    validate_ollama_model_name(&model)?;

    {
        let mut pulls = state
            .ollama_pulls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pulls.contains(&model) {
            return Err(format!("Pull already in progress for '{}'", model));
        }
        pulls.insert(model.clone());
    }

    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Err(error) = check_strict_local_mode(&settings) {
            let mut pulls = state
                .ollama_pulls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pulls.remove(&model);
            return Err(error);
        }
        settings.providers.ollama.endpoint.clone()
    };

    if let Err(error) = precheck_ollama_registry_model_tag(&model) {
        let mut pulls = state
            .ollama_pulls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pulls.remove(&model);
        return Err(error);
    }

    struct PullGuard {
        app: AppHandle,
        model: String,
    }
    impl Drop for PullGuard {
        fn drop(&mut self) {
            if let Ok(mut pulls) = self.app.state::<AppState>().ollama_pulls.lock() {
                pulls.remove(&self.model);
            }
        }
    }

    let app_handle = app.clone();
    let model_clone = model.clone();
    crate::util::spawn_guarded("ollama_model_pull", move || {
        let _guard = PullGuard {
            app: app_handle.clone(),
            model: model_clone.clone(),
        };
        pull_ollama_model_inner(app_handle, model_clone, endpoint);
    });

    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_ollama_model(
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    use super::provider::validate_ollama_model_name;

    validate_ollama_model_name(&model)?;
    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        check_strict_local_mode(&settings)?;
        settings.providers.ollama.endpoint.clone()
    };

    tauri::async_runtime::spawn_blocking(move || delete_ollama_model_impl(endpoint, model))
        .await
        .map_err(|e| format!("Delete Ollama model task failed: {}", e))?
}

fn delete_ollama_model_impl(endpoint: String, model: String) -> Result<(), String> {
    use super::provider::ollama_endpoint_candidates;

    let body = serde_json::json!({ "model": model.clone() });

    let agent = ureq::builder()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();

    let mut last_transport_error: Option<String> = None;
    for candidate in ollama_endpoint_candidates(&endpoint) {
        let url = format!("{}/api/delete", candidate);
        let request = agent
            .request("DELETE", &url)
            .set("Content-Type", "application/json");

        match request.send_json(body.clone()) {
            Ok(_) => return Ok(()),
            Err(ureq::Error::Status(404, _)) => {
                return Err(format!("Model '{}' not found in Ollama", model));
            }
            Err(ureq::Error::Transport(transport)) => {
                last_transport_error = Some(transport.to_string());
                continue;
            }
            Err(error) => return Err(format!("Failed to delete model: {}", error)),
        }
    }

    Err(format!(
        "Failed to delete model: {}",
        last_transport_error.unwrap_or_else(|| "unable to reach Ollama endpoint".to_string())
    ))
}

#[tauri::command]
pub(crate) async fn get_ollama_model_info(
    state: State<'_, AppState>,
    model: String,
) -> Result<serde_json::Value, String> {
    use super::provider::validate_ollama_model_name;

    validate_ollama_model_name(&model)?;
    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        check_strict_local_mode(&settings)?;
        settings.providers.ollama.endpoint.clone()
    };

    tauri::async_runtime::spawn_blocking(move || get_ollama_model_info_impl(endpoint, model))
        .await
        .map_err(|e| format!("Get Ollama model info task failed: {}", e))?
}

fn get_ollama_model_info_impl(
    endpoint: String,
    model: String,
) -> Result<serde_json::Value, String> {
    use super::provider::ollama_endpoint_candidates;

    let body = serde_json::json!({ "model": model });

    let agent = ureq::builder()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(10))
        .build();

    let mut last_transport_error: Option<String> = None;
    for candidate in ollama_endpoint_candidates(&endpoint) {
        let url = format!("{}/api/show", candidate);
        let response = match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body.clone())
        {
            Ok(response) => response,
            Err(ureq::Error::Transport(transport)) => {
                last_transport_error = Some(transport.to_string());
                continue;
            }
            Err(error) => return Err(format!("Failed to get model info: {}", error)),
        };

        return response
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("Failed to parse response: {}", e));
    }

    Err(format!(
        "Failed to get model info: {}",
        last_transport_error.unwrap_or_else(|| "unable to reach Ollama endpoint".to_string())
    ))
}

#[tauri::command]
pub(crate) async fn unload_ollama_model(app: AppHandle, model: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unload_configured_ollama_model(&app, &model))
        .await
        .map_err(|e| format!("Unload Ollama model task failed: {}", e))?
}

fn unload_configured_ollama_model(app: &AppHandle, model: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    check_strict_local_mode(&settings)?;
    unload_ollama_model_impl(&settings.providers.ollama.endpoint, model)
}

pub(crate) fn unload_ollama_model_impl(endpoint: &str, model: &str) -> Result<(), String> {
    let model = model.trim();
    if model.is_empty() {
        return Ok(());
    }
    if is_ssrf_target(endpoint) {
        return Err("Ollama endpoint is not allowed for unload.".to_string());
    }
    let unload_body = serde_json::json!({
        "model": model,
        "prompt": "",
        "keep_alive": "0m",
        "stream": false
    });

    let agent = ureq::builder()
        .timeout_connect(std::time::Duration::from_secs(2))
        .timeout_read(std::time::Duration::from_secs(5))
        .build();

    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/generate", candidate);
        if agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(&unload_body)
            .is_ok()
        {
            return Ok(());
        }
    }

    Ok(())
}

pub(crate) fn warmup_ollama_model_impl(endpoint: &str, model: &str) -> Result<(), String> {
    let model = model.trim();
    if model.is_empty() {
        return Ok(());
    }
    if is_ssrf_target(endpoint) {
        return Err("Ollama endpoint is not allowed for warmup.".to_string());
    }

    let mut warmup_options = super::provider::ollama_runner_defining_options();
    warmup_options.insert("num_predict".to_string(), serde_json::json!(1));
    let warmup_body = serde_json::json!({
        "model": model,
        "prompt": ".",
        "stream": false,
        "keep_alive": -1,
        "options": serde_json::Value::Object(warmup_options),
    });

    let agent = ureq::builder()
        .timeout_connect(std::time::Duration::from_secs(2))
        .timeout_read(std::time::Duration::from_secs(90))
        .build();

    let mut last_error: Option<String> = None;
    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/generate", candidate);
        match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(warmup_body.clone())
        {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    Err(format!(
        "Failed to warm Ollama model: {}",
        last_error.unwrap_or_else(|| "unable to reach Ollama endpoint".to_string())
    ))
}

#[tauri::command]
pub(crate) fn purge_gpu_memory(state: State<'_, AppState>) -> Result<(), String> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current_ollama_model = settings.ai_fallback.model.clone();
    drop(settings);

    if !current_ollama_model.is_empty() {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ =
            unload_ollama_model_impl(&settings.providers.ollama.endpoint, &current_ollama_model);
    }

    let _ = crate::whisper_server::kill_whisper_server(&state);
    state
        .whisper_server_warmup_started
        .store(false, Ordering::Relaxed);

    Ok(())
}

#[tauri::command]
pub(crate) async fn stop_ollama_runtime(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        terminate_managed_child_slot("managed Ollama runtime", &state.managed_ollama_child);
        crate::ollama_runtime::clear_ollama_pid_lockfile(&app);

        let endpoint = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            settings.providers.ollama.endpoint
        };
        let runtime_reachable = ping_ollama_quick(&endpoint).is_ok();
        update_startup_status(&app, state.inner(), |status| {
            status.ollama_ready = runtime_reachable;
            status.ollama_starting = false;
        });
        update_runtime_diagnostics(&app, state.inner(), |diagnostics| {
            diagnostics.ollama.managed_pid = None;
            diagnostics.ollama.reachable = runtime_reachable;
            diagnostics.ollama.spawn_stage = if runtime_reachable {
                "running_externally".to_string()
            } else {
                "stopped".to_string()
            };
            if !runtime_reachable {
                diagnostics.ollama.last_error.clear();
            }
        });

        Ok(())
    })
    .await
    .unwrap_or_else(|_| Ok(()))
}

#[tauri::command]
pub(crate) fn install_lm_studio() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        std::process::Command::new("cmd")
            .args([
                "/C",
                "start",
                "powershell",
                "-ExecutionPolicy",
                "Bypass",
                "-NoExit",
                "-Command",
                "Write-Host 'Trispr Flow: Installing LM Studio...' -ForegroundColor Cyan; irm 'https://lmstudio.ai/install.ps1' | iex",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to launch LM Studio installer: {e}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("LM Studio installer helper is only supported on Windows.".to_string())
    }
}
