use crate::ai_fallback::provider::ping_ollama_quick;
use crate::modules::health as module_health;
use crate::modules::registry as module_registry;
use crate::modules::ModuleSettings;
use crate::state::{
    self, AppState, RuntimeDiagnostics, RuntimeMetricsSnapshot, Settings, StartupStatus,
};
use crate::{
    apply_hidden_creation_flags, capability_enabled, refresh_runtime_diagnostics,
    refresh_startup_status, RuntimeCapability,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct DependencyPreflightItem {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) required: bool,
    pub(crate) message: String,
    pub(crate) hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct DependencyPreflightReport {
    pub(crate) generated_at_ms: u64,
    pub(crate) overall_status: String,
    pub(crate) blocking_count: usize,
    pub(crate) warning_count: usize,
    pub(crate) items: Vec<DependencyPreflightItem>,
}

#[tauri::command]
pub(crate) async fn get_settings(app: AppHandle) -> Settings {
    // Keep settings lock acquisition off the Tauri command executor thread,
    // preventing bootstrap contention with status/diagnostics requests.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        settings
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn get_startup_status(app: AppHandle) -> StartupStatus {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        refresh_startup_status(&app, state.inner())
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn get_runtime_diagnostics(app: AppHandle) -> RuntimeDiagnostics {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        refresh_runtime_diagnostics(&app, state.inner())
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub(crate) fn get_runtime_metrics_snapshot(state: State<'_, AppState>) -> RuntimeMetricsSnapshot {
    state::get_runtime_metrics_snapshot(state.inner())
}

#[tauri::command]
pub(crate) fn record_runtime_metric(
    state: State<'_, AppState>,
    metric: String,
) -> Result<(), String> {
    match metric.trim() {
        "refinement_timeout" | "refinement_fallback_timed_out" => {
            state::record_refinement_timeout(state.inner());
            state::record_refinement_fallback_timed_out(state.inner());
            Ok(())
        }
        other => Err(format!("Unknown runtime metric '{}'", other)),
    }
}

#[cfg(target_os = "windows")]
fn check_powershell_available() -> bool {
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-Command", "$PSVersionTable.PSVersion.Major"]);
    apply_hidden_creation_flags(&mut cmd);
    cmd.output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn build_module_dependency_preflight_items(
    module_settings: &ModuleSettings,
) -> Vec<DependencyPreflightItem> {
    let installed_package_ids = std::collections::HashSet::new();
    let descriptors = module_registry::modules_as_descriptors(module_settings);
    let health =
        module_health::get_health_with_packages(module_settings, None, &installed_package_ids);

    health
        .into_iter()
        .filter(|entry| entry.state != "ok")
        .filter_map(|entry| {
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.id == entry.module_id)?;
            let (status, required) = match entry.state.as_str() {
                "release_blocker" | "error" => ("error", true),
                "needs_setup" | "fallback_active" | "local_warning" | "degraded" => {
                    ("warning", false)
                }
                _ => return None,
            };

            Some(DependencyPreflightItem {
                id: format!("module_{}", entry.module_id),
                status: status.to_string(),
                required,
                message: format!("Module '{}' health: {}", descriptor.name, entry.detail),
                hint: Some("Open Modules tab and run Health / dependency checks.".to_string()),
            })
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn check_powershell_available() -> bool {
    false
}

pub(crate) fn build_dependency_preflight_report(state: &AppState) -> DependencyPreflightReport {
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let mut items: Vec<DependencyPreflightItem> = Vec::new();

    let whisper_cli = crate::paths::resolve_whisper_cli_path_for_backend(Some(
        settings_snapshot.local_backend_preference.as_str(),
    ));
    if let Some(path) = whisper_cli {
        if let Some(issue) = crate::transcription::whisper_runtime_preflight_issue(path.as_path()) {
            let selected_backend =
                crate::transcription::whisper_backend_from_cli_path(path.as_path());
            let vulkan_ready = crate::paths::resolve_whisper_cli_path_for_backend(Some("vulkan"))
                .filter(|candidate| {
                    crate::transcription::whisper_backend_from_cli_path(candidate.as_path())
                        == "vulkan"
                })
                .and_then(|candidate| {
                    if crate::transcription::whisper_runtime_preflight_issue(candidate.as_path())
                        .is_none()
                    {
                        Some(candidate)
                    } else {
                        None
                    }
                });
            let has_working_fallback = selected_backend == "cuda" && vulkan_ready.is_some();
            items.push(DependencyPreflightItem {
                id: "whisper_runtime".to_string(),
                status: if has_working_fallback {
                    "warning".to_string()
                } else {
                    "error".to_string()
                },
                required: true,
                message: issue,
                hint: Some(if has_working_fallback {
                    "CUDA runtime is incomplete; app will fall back to Vulkan. Reinstall/update Trispr Flow CUDA runtime to restore CUDA path.".to_string()
                } else {
                    "Reinstall Trispr Flow and ensure complete CUDA/VULKAN runtime files are bundled (including CUDA runtime DLLs).".to_string()
                }),
            });
        } else {
            items.push(DependencyPreflightItem {
                id: "whisper_runtime".to_string(),
                status: "ok".to_string(),
                required: true,
                message: format!("Whisper runtime found: {}", path.display()),
                hint: None,
            });
        }
    } else {
        items.push(DependencyPreflightItem {
            id: "whisper_runtime".to_string(),
            status: "error".to_string(),
            required: true,
            message: "Whisper runtime executable is missing.".to_string(),
            hint: Some(
                "Reinstall Trispr Flow and ensure the selected CUDA/VULKAN runtime is present."
                    .to_string(),
            ),
        });
    }

    let powershell_ok = check_powershell_available();
    let tts_enabled = capability_enabled(&settings_snapshot, RuntimeCapability::VoiceOutputTts);
    if powershell_ok {
        items.push(DependencyPreflightItem {
            id: "powershell_tts".to_string(),
            status: "ok".to_string(),
            required: tts_enabled,
            message: "PowerShell runtime is available for Windows TTS.".to_string(),
            hint: None,
        });
    } else {
        items.push(DependencyPreflightItem {
            id: "powershell_tts".to_string(),
            status: if tts_enabled {
                "error".to_string()
            } else {
                "warning".to_string()
            },
            required: tts_enabled,
            message: "PowerShell runtime is not available.".to_string(),
            hint: Some(
                "Windows-native TTS requires powershell.exe and System.Speech support.".to_string(),
            ),
        });
    }

    if tts_enabled && settings_snapshot.voice_output_settings.default_provider == "local_custom" {
        items.push(DependencyPreflightItem {
            id: "tts_local_custom".to_string(),
            status: "warning".to_string(),
            required: false,
            message: "Local custom TTS provider is still a placeholder.".to_string(),
            hint: Some(
                "Current fallback uses Windows native TTS until the custom runtime is integrated."
                    .to_string(),
            ),
        });
    }

    if capability_enabled(&settings_snapshot, RuntimeCapability::AiRefinement)
        && settings_snapshot.ai_fallback.provider == "ollama"
    {
        let endpoint = settings_snapshot.providers.ollama.endpoint.clone();
        let local_mode = settings_snapshot.ai_fallback.strict_local_mode;
        let reachable = ping_ollama_quick(&endpoint).is_ok();
        items.push(DependencyPreflightItem {
            id: "ollama_runtime".to_string(),
            status: if reachable {
                "ok".to_string()
            } else {
                "warning".to_string()
            },
            required: false,
            message: if reachable {
                format!("Ollama endpoint reachable: {}", endpoint)
            } else {
                format!("Ollama endpoint not reachable: {}", endpoint)
            },
            hint: if reachable {
                None
            } else {
                Some(if local_mode {
                    "Start/install local Ollama runtime in AI Refinement > Runtime.".to_string()
                } else {
                    "Ensure configured Ollama endpoint is running.".to_string()
                })
            },
        });
    }

    items.extend(build_module_dependency_preflight_items(
        &settings_snapshot.module_settings,
    ));

    let blocking_count = items.iter().filter(|item| item.status == "error").count();
    let warning_count = items.iter().filter(|item| item.status == "warning").count();
    let overall_status = if blocking_count > 0 {
        "error"
    } else if warning_count > 0 {
        "warning"
    } else {
        "ok"
    };

    DependencyPreflightReport {
        generated_at_ms: crate::util::now_ms(),
        overall_status: overall_status.to_string(),
        blocking_count,
        warning_count,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::{registry, ASSISTANT_CORE_MODULE_ID};

    #[test]
    fn dependency_preflight_ignores_stale_disabled_optional_module_error() {
        let mut module_settings = ModuleSettings::default();
        registry::set_last_error(
            &mut module_settings,
            ASSISTANT_CORE_MODULE_ID,
            "Module is in error state.",
        );

        let items = build_module_dependency_preflight_items(&module_settings);

        assert!(items.iter().all(|item| item.id != "module_assistant_core"));
    }

    #[test]
    fn dependency_preflight_warns_for_enabled_module_setup_gap() {
        let mut module_settings = ModuleSettings::default();
        module_settings
            .enabled_modules
            .insert(ASSISTANT_CORE_MODULE_ID.to_string());

        let items = build_module_dependency_preflight_items(&module_settings);
        let assistant_item = items
            .iter()
            .find(|item| item.id == "module_assistant_core")
            .expect("enabled Assistant Core setup gap should be reported");

        assert_eq!(assistant_item.status, "warning");
        assert!(!assistant_item.required);
        assert!(assistant_item.message.contains("Assistant Core"));
    }
}

pub(crate) fn run_dependency_preflight(app: &AppHandle) {
    let state = app.state::<AppState>();
    let report = build_dependency_preflight_report(state.inner());
    if report.overall_status != "ok" {
        for item in report.items.iter().filter(|item| item.status != "ok") {
            warn!(
                "Dependency preflight [{}] {}: {}",
                item.status, item.id, item.message
            );
        }
    } else {
        info!("Dependency preflight passed with no warnings.");
    }
    let _ = app.emit("dependency:preflight", &report);
}

#[tauri::command]
pub(crate) async fn get_dependency_preflight_status(app: AppHandle) -> DependencyPreflightReport {
    // Wrapped in spawn_blocking: check_powershell_available() spawns powershell.exe
    // which blocks for 1-5s; running that on a Tokio worker thread would starve IPC.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        build_dependency_preflight_report(state.inner())
    })
    .await
    .unwrap_or_else(|_| DependencyPreflightReport {
        generated_at_ms: 0,
        overall_status: "error".to_string(),
        blocking_count: 0,
        warning_count: 0,
        items: vec![],
    })
}
