use crate::modules::{
    canonicalize_module_id, lifecycle as module_lifecycle, normalize_module_settings,
    registry as module_registry,
};
use crate::state::{
    normalize_ai_refinement_module_binding, save_settings_file, AppState, AI_REFINEMENT_MODULE_ID,
};
use tauri::{AppHandle, Emitter};
use tracing::warn;

fn scan_installed_module_ids(app: &AppHandle) -> Result<std::collections::HashSet<String>, String> {
    let modules_dir = crate::paths::resolve_modules_dir(app);
    crate::modules::package::scan_installed_module_ids(&modules_dir)
}

pub(crate) fn enable_module_actions(
    app: &AppHandle,
    state: &AppState,
    module_id: String,
    grant_permissions: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    let module_id = canonicalize_module_id(module_id.trim()).to_string();
    if module_id.is_empty() {
        return Err("Module id cannot be empty.".to_string());
    }

    let grants = grant_permissions.unwrap_or_default();
    let installed_package_ids = scan_installed_module_ids(app)?;

    // ==========================================
    // PHASE A: Settle (In-Memory State Settle)
    // ==========================================
    let (result, snapshot, descriptors) = {
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let result = module_lifecycle::enable_module_with_packages(
            &mut settings.module_settings,
            &module_id,
            &grants,
            &installed_package_ids,
        );

        if result.is_ok() {
            if module_id == AI_REFINEMENT_MODULE_ID {
                settings.ai_fallback.enabled = true;
                settings.postproc_llm_enabled = true;
            }
        }

        normalize_module_settings(&mut settings.module_settings);
        normalize_ai_refinement_module_binding(&mut settings);

        let descriptors = module_registry::modules_as_descriptors_with_packages(
            &settings.module_settings,
            &installed_package_ids,
        );
        (result, settings.clone(), descriptors)
    };

    // ==========================================
    // PHASE B: Persist (Durable Persistence)
    // ==========================================
    save_settings_file(app, &snapshot)?;

    // ==========================================
    // PHASE C: Reconcile (Reconcile Side-Effects)
    // ==========================================
    let _ = app.emit("settings-changed", snapshot.clone());
    let _ = app.emit("module:state-changed", descriptors);

    if result.is_ok() && module_id == AI_REFINEMENT_MODULE_ID {
        crate::schedule_ai_refinement_reenable_bootstrap(app.clone());
    }

    match result {
        Ok(lifecycle) => Ok(serde_json::json!(lifecycle)),
        Err(error) => {
            let _ = app.emit(
                "module:error",
                serde_json::json!({ "module_id": module_id, "error": error }),
            );
            Err(error)
        }
    }
}

pub(crate) fn disable_module_actions(
    app: &AppHandle,
    state: &AppState,
    module_id: String,
) -> Result<serde_json::Value, String> {
    let module_id = canonicalize_module_id(module_id.trim()).to_string();
    if module_id.is_empty() {
        return Err("Module id cannot be empty.".to_string());
    }
    let installed_package_ids = scan_installed_module_ids(app).unwrap_or_default();

    // ==========================================
    // PHASE A: Settle (In-Memory State Settle)
    // ==========================================
    let (result, snapshot, descriptors) = {
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let result = module_lifecycle::disable_module(&mut settings.module_settings, &module_id);

        if result.is_ok() {
            if module_id == AI_REFINEMENT_MODULE_ID {
                settings.ai_fallback.enabled = false;
                settings.postproc_llm_enabled = false;
            }
        }

        normalize_module_settings(&mut settings.module_settings);
        normalize_ai_refinement_module_binding(&mut settings);

        let descriptors = module_registry::modules_as_descriptors_with_packages(
            &settings.module_settings,
            &installed_package_ids,
        );
        (result, settings.clone(), descriptors)
    };

    // ==========================================
    // PHASE B: Persist (Durable Persistence)
    // ==========================================
    save_settings_file(app, &snapshot)?;

    // ==========================================
    // PHASE C: Reconcile (Reconcile Side-Effects)
    // ==========================================
    if result.is_ok() {
        match module_id.as_str() {
            "ai_refinement" => {
                crate::audio::force_reset_refinement_activity(app, "forced_reset");

                let app_clone = app.clone();
                let endpoint = snapshot.providers.ollama.endpoint.clone();
                let model = snapshot.ai_fallback.model.clone();
                tauri::async_runtime::spawn(async move {
                    match tauri::async_runtime::spawn_blocking(move || {
                        crate::ai_fallback::commands::unload_ollama_model_impl(&endpoint, &model)
                    })
                    .await
                    {
                        Ok(Ok(())) => crate::audio::mark_ollama_model_cold(&app_clone),
                        Ok(Err(error)) => {
                            warn!("[module:disable] Ollama unload failed: {}", error)
                        }
                        Err(error) => {
                            warn!("[module:disable] Ollama unload task failed: {}", error)
                        }
                    }
                    let _ = crate::ai_fallback::commands::stop_ollama_runtime(app_clone).await;
                });
            }
            _ => {}
        }
    }

    let _ = app.emit("settings-changed", snapshot.clone());
    let _ = app.emit("module:state-changed", descriptors);

    match result {
        Ok(lifecycle) => Ok(serde_json::json!(lifecycle)),
        Err(error) => {
            let _ = app.emit(
                "module:error",
                serde_json::json!({ "module_id": module_id, "error": error }),
            );
            Err(error)
        }
    }
}
