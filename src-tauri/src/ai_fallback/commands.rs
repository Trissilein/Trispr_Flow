use super::prepare_refinement;
use super::provider::{is_ssrf_target, ollama_endpoint_candidates, ping_ollama_quick};
use crate::state::AppState;
use crate::{terminate_managed_child_slot, update_runtime_diagnostics, update_startup_status};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, State};

pub(crate) use crate::ollama_runtime::{
    detect_ollama_runtime, download_ollama_runtime, fetch_ollama_online_versions,
    import_ollama_model_from_file, install_ollama_runtime, list_ollama_runtime_versions,
    start_ollama_runtime, verify_ollama_runtime,
};

#[tauri::command]
pub(crate) async fn fetch_ollama_models_with_size(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings.providers.ollama.endpoint.clone()
    };
    tauri::async_runtime::spawn_blocking(move || fetch_ollama_models_with_size_impl(endpoint))
        .await
        .map_err(|e| format!("Fetch Ollama models task failed: {}", e))?
}

fn fetch_ollama_models_with_size_impl(endpoint: String) -> Result<Vec<serde_json::Value>, String> {
    let models = super::provider::list_ollama_models_detailed(&endpoint);
    if models.is_empty() {
        ping_ollama_quick(&endpoint).map_err(|e| e.to_string())?;
    }
    Ok(models)
}

/// Compares installed models with what the public registry currently serves for the
/// same tag. Makes small manifest requests to registry.ollama.ai / huggingface.co only.
#[tauri::command]
pub(crate) async fn check_ollama_model_updates(
    state: State<'_, AppState>,
    models: Vec<super::model_registry::ModelUpdateCheckInput>,
) -> Result<Vec<super::model_registry::ModelUpdateStatus>, String> {
    let endpoint = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings.providers.ollama.endpoint.clone()
    };
    tauri::async_runtime::spawn_blocking(move || {
        // The digest a runtime lists for a model depends on the runtime version; the weights
        // layer does not. Collect the local blob digests so "current" is decided on weights.
        let enriched = models
            .into_iter()
            .map(|mut model| {
                model.local_layers = fetch_local_layer_digests(&endpoint, &model.name);
                model
            })
            .collect();
        super::model_registry::check_model_updates(enriched)
    })
    .await
    .map_err(|e| format!("Model update check failed: {}", e))
}

/// Blob digests of a local model (from `/api/show`). Empty when the lookup fails; the
/// caller then falls back to the manifest digest.
fn fetch_local_layer_digests(endpoint: &str, model: &str) -> Vec<String> {
    let agent = ureq::builder()
        .timeout_connect(std::time::Duration::from_secs(2))
        .timeout_read(std::time::Duration::from_secs(5))
        .build();
    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/show", candidate);
        let Ok(response) = agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(serde_json::json!({ "model": model }))
        else {
            continue;
        };
        let Ok(json) = response.into_json::<serde_json::Value>() else {
            continue;
        };
        return super::model_registry::parse_modelfile_blob_digests(
            json["modelfile"].as_str().unwrap_or(""),
        );
    }
    Vec::new()
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

    let setup = match prepare_refinement(&app, &settings_snapshot) {
        Ok(setup) => setup,
        Err(_) => return Ok(false),
    };

    let ping_text = ".";

    let result = tauri::async_runtime::spawn_blocking(move || {
        setup
            .provider
            .refine_transcript(ping_text, &setup.model, &setup.options)
    })
    .await
    .map_err(|e| format!("ping_refinement_model task failed: {}", e))?;

    Ok(result.is_ok())
}

/// Takes `model` out of the set of running pulls. A lock that a panicking thread poisoned is
/// recovered (like everywhere else): skipping it would leave the entry in place and the model
/// "already in progress" until the app restarts.
pub(super) fn release_pull(
    pulls: &std::sync::Mutex<std::collections::HashSet<String>>,
    model: &str,
) {
    pulls
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(model);
}

#[tauri::command]
pub(crate) async fn pull_ollama_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    use super::model_registry::precheck_model_ref;
    use super::provider::{pull_ollama_model_inner, validate_ollama_model_name};

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
        settings.providers.ollama.endpoint.clone()
    };

    // Registry/runtime lookups are network calls: keep them off the command thread. Besides the
    // existence precheck, refuse before any download when the running runtime is provably too
    // old for the model (the registry would answer the pull with HTTP 412 only afterwards).
    let precheck = {
        let model = model.clone();
        let endpoint = endpoint.clone();
        tauri::async_runtime::spawn_blocking(move || {
            precheck_model_ref(&model)?;
            super::model_verify::ensure_runtime_supports_model(&endpoint, &model)
        })
        .await
        .map_err(|e| format!("Model precheck task failed: {}", e))
        .and_then(|result| result)
    };
    if let Err(error) = precheck {
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
            release_pull(&self.app.state::<AppState>().ollama_pulls, &self.model);
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
        settings.providers.ollama.endpoint.clone()
    };

    tauri::async_runtime::spawn_blocking(move || delete_ollama_model_impl(endpoint, model))
        .await
        .map_err(|e| format!("Delete Ollama model task failed: {}", e))?
}

pub(super) fn delete_ollama_model_impl(endpoint: String, model: String) -> Result<(), String> {
    use super::provider::ollama_endpoint_candidates;

    // Unload first: on Windows the runner loads without mmap, so /api/delete succeeds on a
    // loaded model but leaves a runner that can no longer be unloaded (404) until the
    // server restarts. Best effort; a model that is not loaded is simply a no-op.
    let _ = unload_ollama_model_impl(&endpoint, &model);

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

    let mut last_error: Option<String> = None;
    for candidate in ollama_endpoint_candidates(endpoint) {
        let url = format!("{}/api/generate", candidate);
        match agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(&unload_body)
        {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    Err(format!(
        "Failed to unload Ollama model: {}",
        last_error.unwrap_or_else(|| "unable to reach Ollama endpoint".to_string())
    ))
}

/// Loads `model` with the same runner options refinement uses, so the first dictation after
/// a download or switch does not pay the one-time load (measured: 7-28 s on a fresh pull).
#[tauri::command]
pub(crate) async fn warmup_ollama_model(
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    super::provider::validate_ollama_model_name(&model)?;
    let (endpoint, num_ctx) = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            settings.providers.ollama.endpoint.clone(),
            super::provider::num_ctx_for_tier(settings.ai_fallback.adaptive_num_ctx_tier),
        )
    };
    tauri::async_runtime::spawn_blocking(move || {
        warmup_ollama_model_impl(&endpoint, &model, num_ctx)
    })
    .await
    .map_err(|e| format!("Model warmup failed: {}", e))?
}

pub(crate) fn warmup_ollama_model_impl(
    endpoint: &str,
    model: &str,
    num_ctx: usize,
) -> Result<(), String> {
    let model = model.trim();
    if model.is_empty() {
        return Ok(());
    }
    if is_ssrf_target(endpoint) {
        return Err("Ollama endpoint is not allowed for warmup.".to_string());
    }

    let mut warmup_options = super::provider::ollama_runner_defining_options(num_ctx);
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
pub(crate) fn purge_gpu_memory(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
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
    // The model is gone now, whatever the cached flag said: grey light, next PTT press warms it.
    crate::audio::mark_ollama_model_cold(&app);

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
        // Right after the kill: the cached quick ping would still say "running" for up to 3 s.
        super::provider::reset_quick_ping_cache();
        let runtime_reachable = super::provider::ping_ollama_quick_fresh(&endpoint).is_ok();
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

#[cfg(test)]
mod unload_tests {
    use super::{release_pull, unload_ollama_model_impl};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_finished_pull_is_released_even_when_the_pull_set_lock_is_poisoned() {
        let pulls = Arc::new(Mutex::new(HashSet::from(["gemma4:e4b".to_string()])));
        let poisoner = pulls.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().unwrap();
            panic!("a thread died while holding the pull set");
        })
        .join();
        assert!(pulls.is_poisoned());

        release_pull(&pulls, "gemma4:e4b");

        assert!(
            pulls.lock().unwrap_or_else(|p| p.into_inner()).is_empty(),
            "the model must not stay 'already in progress' for the rest of the session"
        );
        // releasing something that is not there is a no-op
        release_pull(&pulls, "other:1");
    }

    #[test]
    fn unload_reports_when_no_ollama_endpoint_accepts_the_release_request() {
        let error = unload_ollama_model_impl("http://127.0.0.1:1", "test-model")
            .expect_err("an unreachable endpoint must not be reported as unloaded");
        assert!(error.contains("Failed to unload Ollama model"));
    }
}
