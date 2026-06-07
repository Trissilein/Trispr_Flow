use crate::modules::{
    canonicalize_module_id, lifecycle as module_lifecycle, normalize_confluence_settings,
    normalize_gdd_module_settings, normalize_module_settings, normalize_vision_input_settings,
    normalize_voice_output_settings, normalize_workflow_agent_settings,
    registry as module_registry, ASSISTANT_CORE_MODULE_ID, ASSISTANT_PRESENCE_MODULE_ID,
    TASK_CAPTURE_MODULE_ID,
};
use crate::state::{
    normalize_ai_refinement_module_binding, normalize_assistant_core_binding,
    normalize_assistant_presence_binding, normalize_product_mode_field, save_settings_file,
    AppState, AI_REFINEMENT_MODULE_ID,
};
use crate::transcription::{start_transcribe_monitor, stop_transcribe_monitor};
use tauri::{AppHandle, Emitter};

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

    // ==========================================
    // PHASE A: Settle (In-Memory State Settle)
    // ==========================================
    let (result, snapshot, descriptors, prev_transcribe_enabled) = {
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prev_transcribe_enabled = settings.transcribe_enabled;

        let result =
            module_lifecycle::enable_module(&mut settings.module_settings, &module_id, &grants);

        if result.is_ok() {
            if module_id == ASSISTANT_CORE_MODULE_ID {
                settings.workflow_agent.enabled = true;
                settings.assistant_presence_enabled = true;
                settings
                    .module_settings
                    .enabled_modules
                    .insert(ASSISTANT_PRESENCE_MODULE_ID.to_string());
            }
            if module_id == ASSISTANT_PRESENCE_MODULE_ID {
                settings.assistant_presence_enabled = true;
            }
            if module_id == "input_vision" {
                settings.vision_input_settings.enabled = true;
            }
            if module_id == "output_voice_tts" {
                settings.voice_output_settings.enabled = true;
            }
            if module_id == AI_REFINEMENT_MODULE_ID {
                settings.ai_fallback.enabled = true;
                settings.postproc_llm_enabled = true;
            }
            if module_id == TASK_CAPTURE_MODULE_ID {}
        }

        normalize_module_settings(&mut settings.module_settings);
        normalize_assistant_core_binding(&mut settings);
        normalize_product_mode_field(&mut settings);
        normalize_ai_refinement_module_binding(&mut settings);
        normalize_gdd_module_settings(&mut settings.gdd_module_settings);
        normalize_confluence_settings(&mut settings.confluence_settings);
        normalize_workflow_agent_settings(&mut settings.workflow_agent);
        normalize_assistant_presence_binding(&mut settings);
        normalize_vision_input_settings(&mut settings.vision_input_settings);
        normalize_voice_output_settings(&mut settings.voice_output_settings);
        crate::reconcile_assistant_transcribe_flag(&mut settings);

        let descriptors = module_registry::modules_as_descriptors(&settings.module_settings);
        (
            result,
            settings.clone(),
            descriptors,
            prev_transcribe_enabled,
        )
    };

    // ==========================================
    // PHASE B: Persist (Durable Persistence)
    // ==========================================
    save_settings_file(app, &snapshot)?;

    // ==========================================
    // PHASE C: Reconcile (Reconcile Side-Effects)
    // ==========================================
    if result.is_ok() && module_id == "output_voice_tts" {
        crate::schedule_piper_daemon_reconcile(
            app.clone(),
            snapshot.voice_output_settings.clone(),
            "enable_module",
        );
    }

    if result.is_ok() {
        if snapshot.transcribe_enabled && !prev_transcribe_enabled {
            let _ = start_transcribe_monitor(app, state, &snapshot);
        } else if !snapshot.transcribe_enabled && prev_transcribe_enabled {
            stop_transcribe_monitor(app, state);
        }
    }

    let _ = app.emit("settings-changed", snapshot.clone());
    let _ = app.emit("menu:update-transcribe", snapshot.transcribe_enabled);
    let _ = app.emit("module:state-changed", descriptors);
    crate::assistant_presence::reconcile_assistant_presence_window(app, &snapshot);
    let _ = crate::workflow_agent::emit_assistant_runtime_state_from_current_settings(
        app,
        state,
        "enable_module",
    );

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

    // ==========================================
    // PHASE A: Settle (In-Memory State Settle)
    // ==========================================
    let (result, snapshot, descriptors, prev_transcribe_enabled) = {
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let prev_transcribe_enabled = settings.transcribe_enabled;

        let result = module_lifecycle::disable_module(&mut settings.module_settings, &module_id);

        if result.is_ok() {
            if module_id == ASSISTANT_CORE_MODULE_ID {
                settings.workflow_agent.enabled = false;
            }
            if module_id == ASSISTANT_PRESENCE_MODULE_ID {
                settings.assistant_presence_enabled = false;
            }
            if module_id == "input_vision" {
                settings.vision_input_settings.enabled = false;
            }
            if module_id == "output_voice_tts" {
                settings.voice_output_settings.enabled = false;
            }
            if module_id == AI_REFINEMENT_MODULE_ID {
                settings.ai_fallback.enabled = false;
                settings.postproc_llm_enabled = false;
            }
            if module_id == TASK_CAPTURE_MODULE_ID {}
        }

        normalize_module_settings(&mut settings.module_settings);
        normalize_assistant_core_binding(&mut settings);
        normalize_product_mode_field(&mut settings);
        normalize_ai_refinement_module_binding(&mut settings);
        normalize_gdd_module_settings(&mut settings.gdd_module_settings);
        normalize_confluence_settings(&mut settings.confluence_settings);
        normalize_workflow_agent_settings(&mut settings.workflow_agent);
        normalize_assistant_presence_binding(&mut settings);
        normalize_vision_input_settings(&mut settings.vision_input_settings);
        normalize_voice_output_settings(&mut settings.voice_output_settings);
        crate::reconcile_assistant_transcribe_flag(&mut settings);

        let descriptors = module_registry::modules_as_descriptors(&settings.module_settings);
        (
            result,
            settings.clone(),
            descriptors,
            prev_transcribe_enabled,
        )
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
            "input_vision" => {
                let _ = crate::multimodal_io::stop_vision_stream_internal(app, state);
            }
            "output_voice_tts" => {
                let _ = crate::multimodal_io::stop_tts_internal(app, state);
                crate::multimodal_io::shutdown_piper_daemon(state);
            }
            "ai_refinement" => {
                crate::audio::force_reset_refinement_activity(app, "forced_reset");

                let provider = snapshot.ai_fallback.provider.clone();
                if provider == "ollama" {
                    let app_clone = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::ai_fallback::commands::stop_ollama_runtime(app_clone).await;
                    });
                } else if provider == "lm_studio" {
                    crate::util::spawn_guarded("lms_daemon_stop_module_disable", || {
                        crate::lms_daemon_command("stop");
                    });
                }
            }
            _ => {}
        }

        if snapshot.transcribe_enabled && !prev_transcribe_enabled {
            let _ = start_transcribe_monitor(app, state, &snapshot);
        } else if !snapshot.transcribe_enabled && prev_transcribe_enabled {
            stop_transcribe_monitor(app, state);
        }
    }

    let _ = app.emit("settings-changed", snapshot.clone());
    let _ = app.emit("menu:update-transcribe", snapshot.transcribe_enabled);
    let _ = app.emit("module:state-changed", descriptors);
    crate::assistant_presence::reconcile_assistant_presence_window(app, &snapshot);
    let _ = crate::workflow_agent::emit_assistant_runtime_state_from_current_settings(
        app,
        state,
        "disable_module",
    );

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
