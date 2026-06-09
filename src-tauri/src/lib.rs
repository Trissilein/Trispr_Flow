// Trispr Flow - core app runtime
#![allow(clippy::needless_return)]

mod ai_fallback;
mod assistant_presence;
mod audio;
mod confluence;
mod constants;
mod continuous_dump;
mod data_migration;
mod errors;
mod gdd;
mod history_partition;
mod hotkeys;
mod models;
mod modules;
mod multimodal_io;
mod ollama_runtime;
mod opus;
mod overlay;
mod paths;
mod postprocessing;
mod refinement_adaptation;
mod session_manager;
mod state;
mod transcription;
mod tts_benchmark;
mod uiautomation_capture;
mod util;
mod video_generation;
mod video_ingest;
mod weather;
mod whisper_server;
mod workflow_agent;

use arboard::{Clipboard, ImageData};
use enigo::{Enigo, Key, KeyboardControllable};
use errors::{AppError, ErrorEvent};
use overlay::emit_capture_idle_overlay;
use state::{AppState, RuntimeDiagnostics, Settings, StartupStatus};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{
    AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::Mutex;

// Exponential backoff state for Ollama diagnostics pings.
// Prevents flooding failed network calls during startup when Ollama is slow to come up.
// Backoff schedule: 1st fail→immediate, 2nd→2 s, 3rd→4 s, 4th→8 s, 5+→30 s.
static OLLAMA_DIAG_FAIL_COUNT: AtomicU32 = AtomicU32::new(0);
static OLLAMA_DIAG_NEXT_MS: AtomicU64 = AtomicU64::new(0);
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::Wry;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{error, info, warn};

pub(crate) use audio::{
    get_last_recording_path, get_recordings_directory, open_recordings_directory,
};
#[cfg(feature = "module-confluence")]
pub(crate) use gdd::confluence::{
    clear_confluence_secret, confluence_list_spaces, confluence_oauth_exchange,
    confluence_oauth_start, delete_pending_gdd_publish, list_pending_gdd_publishes,
    load_gdd_template_from_confluence, load_gdd_template_from_file, publish_gdd_to_confluence,
    publish_or_queue_gdd_to_confluence, retry_pending_gdd_publish, save_confluence_secret,
    suggest_confluence_target, test_confluence_connection,
};
#[cfg(feature = "module-gdd")]
pub(crate) use gdd::{
    detect_gdd_preset, generate_gdd_draft, list_gdd_presets, render_gdd_for_confluence,
    render_gdd_markdown, save_gdd_preset_clone, validate_gdd_draft,
};
pub(crate) use history_partition::{
    add_history_entry, add_transcribe_entry, clear_active_transcript_history,
    delete_active_transcript_entry, get_history, get_transcribe_history, list_history_partitions,
    load_history_partition, save_transcript,
};
pub(crate) use hotkeys::{get_hotkey_conflicts, test_hotkey, validate_hotkey};
pub(crate) use modules::task_capture::{
    get_task_capture_settings, save_task_capture_settings, test_task_capture_endpoint,
};
pub(crate) use multimodal_io::{
    capture_vision_snapshot, download_piper_voice_key, get_vision_stream_health,
    list_piper_voice_catalog, list_screen_sources, list_tts_providers, list_tts_voices, speak_tts,
    start_vision_stream, stop_tts, stop_vision_stream, test_tts_provider,
};
pub(crate) use opus::{check_ffmpeg, encode_to_opus, get_ffmpeg_version_info};
pub(crate) use paths::open_log_directory;
pub(crate) use session_manager::{clear_crash_recovery, save_crash_recovery};
pub(crate) use tts_benchmark::{run_latency_benchmark, run_tts_benchmark};
pub(crate) use util::{frontend_heartbeat, log_frontend_event};
pub(crate) use video_generation::{video_generate, video_get_output_dir, video_open_output_dir};
pub(crate) use video_ingest::{video_ingest_history_entry, video_ingest_sources};
pub(crate) use workflow_agent::{
    agent_build_execution_plan, agent_cancel_pending_confirmation, agent_compose_unknown_reply,
    agent_execute_gdd_plan, agent_list_supported_actions, agent_parse_command,
    assistant_execute_direct_action, search_transcript_sessions,
};

/// Wrap a Tauri command body in `catch_unwind` so that a panic inside module
/// code returns a clean `Err(String)` instead of crashing the app.
/// Works for any command that returns `Result<T, String>`.
#[macro_export]
macro_rules! guarded_command {
    ($label:expr, $body:expr) => {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
            Ok(result) => result,
            Err(payload) => {
                let msg = crate::format_panic_payload(&*payload);
                tracing::error!("Command '{}' panicked: {}", $label, msg);
                Err(format!("Internal error in {}: {}", $label, msg))
            }
        }
    };
}

use crate::ai_fallback::provider::ping_ollama_quick;
use crate::audio::{list_audio_devices, list_output_devices, start_recording, stop_recording};
use crate::history_partition::PartitionedHistory;
use crate::models::{
    check_model_available, clear_hidden_external_models, download_model, get_models_dir,
    hide_external_model, list_models, pick_model_dir, quantize_model, remove_model,
};
use crate::modules::{
    canonicalize_module_id, health as module_health, normalize_confluence_settings,
    normalize_gdd_module_settings, normalize_module_settings, normalize_task_capture_settings,
    normalize_vision_input_settings, normalize_voice_output_settings,
    normalize_workflow_agent_settings, package as module_package, registry as module_registry,
    ASSISTANT_CORE_MODULE_ID, TASK_CAPTURE_MODULE_ID,
};
use crate::state::{
    get_runtime_metrics_snapshot as runtime_metrics_snapshot, load_settings,
    normalize_ai_fallback_fields, normalize_ai_refinement_module_binding,
    normalize_assistant_core_binding, normalize_assistant_presence_binding,
    normalize_continuous_dump_fields, normalize_history_alias_fields, normalize_product_mode_field,
    record_refinement_fallback_timed_out, record_refinement_timeout, save_settings_file,
    sync_model_dir_env, AI_REFINEMENT_MODULE_ID,
};
use crate::transcription::{
    expand_transcribe_backlog as expand_transcribe_backlog_inner, start_transcribe_monitor,
    stop_transcribe_monitor_and_release_whisper, toggle_transcribe_state,
};
pub(crate) use ai_fallback::commands::{
    clear_provider_api_key, delete_ollama_model, detect_ollama_runtime, download_ollama_runtime,
    fetch_available_models, fetch_ollama_models_with_size, fetch_ollama_online_versions,
    get_ollama_model_info, import_ollama_model_from_file, install_lm_studio,
    install_ollama_runtime, list_ollama_runtime_versions, ping_refinement_model, pull_ollama_model,
    purge_gpu_memory, refine_transcript, save_ollama_endpoint, save_provider_api_key,
    set_strict_local_mode, start_ollama_runtime, stop_ollama_runtime, test_provider_connection,
    unload_ollama_model, unload_ollama_model_impl, verify_ollama_runtime, verify_provider_auth,
    warmup_ollama_model_impl,
};
const TRAY_CLICK_DEBOUNCE_MS: u64 = 250;
const TRAY_ICON_ID: &str = "main-tray";
const TRAY_PULSE_FRAMES: usize = 6;
const TRAY_PULSE_CYCLE_MS: u64 = 1600;
const BACKLOG_AUTOEXPAND_TIMEOUT_MS: u64 = 5_000;
const FRONTEND_HEARTBEAT_STALE_MS: u64 = 15_000;
const FRONTEND_WATCHDOG_CHECK_MS: u64 = 5_000;
const FRONTEND_WATCHDOG_COOLDOWN_MS: u64 = 90_000;
const FRONTEND_WATCHDOG_STARTUP_GRACE_MS: u64 = 15_000;
const FRONTEND_WATCHDOG_RECOVERY_WINDOW_MS: u64 = 10 * 60_000;
const FRONTEND_WATCHDOG_RECOVERY_RESTART_THRESHOLD: usize = 3;
const FRONTEND_WATCHDOG_RESTART_WINDOW_MS: u64 = 60 * 60_000;
const FRONTEND_WATCHDOG_RESTART_MAX_PER_WINDOW: usize = 2;
const FRONTEND_WATCHDOG_RESTART_LEDGER_FILE: &str = "frontend_watchdog_restarts.json";
const CLIPBOARD_RETRY_INTERVAL_MS: u64 = 50;
const CLIPBOARD_CAPTURE_TIMEOUT_MS: u64 = 1_000;
const CLIPBOARD_RESTORE_DELAY_MS: u64 = 350;
const CLIPBOARD_RESTORE_TIMEOUT_MS: u64 = 3_000;

static LAST_TRAY_CLICK_MS: AtomicU64 = AtomicU64::new(0);
static TRAY_CAPTURE_STATE: AtomicU8 = AtomicU8::new(0);
static TRAY_TRANSCRIBE_STATE: AtomicU8 = AtomicU8::new(0);
static TRAY_PULSE_STARTED: AtomicBool = AtomicBool::new(false);
static BACKLOG_PROMPT_ACTIVE: AtomicBool = AtomicBool::new(false);
static BACKLOG_PROMPT_CANCELLED: AtomicBool = AtomicBool::new(false);
static MAIN_WINDOW_RESTORED: AtomicBool = AtomicBool::new(false);
static CLIPBOARD_PASTE_GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_GEOMETRY_SAVE_MS: AtomicU64 = AtomicU64::new(0);
static PTT_KEY_HELD: AtomicBool = AtomicBool::new(false);
static PTT_PRESS_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct FrontendRestartLedger {
    timestamps_ms: Vec<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct StabilityDegradedEvent {
    reason: String,
    recoveries_in_window: u64,
    restarts_in_window: u64,
    restart_blocked: bool,
}

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(target_os = "windows")]
fn apply_hidden_creation_flags(cmd: &mut std::process::Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn apply_hidden_creation_flags(_cmd: &mut std::process::Command) {}

fn load_frontend_restart_ledger(app: &AppHandle) -> FrontendRestartLedger {
    let path = crate::paths::resolve_base_dir(app).join(FRONTEND_WATCHDOG_RESTART_LEDGER_FILE);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return FrontendRestartLedger::default();
    };
    serde_json::from_str::<FrontendRestartLedger>(&raw).unwrap_or_default()
}

fn save_frontend_restart_ledger(app: &AppHandle, ledger: &FrontendRestartLedger) {
    let path = crate::paths::resolve_base_dir(app).join(FRONTEND_WATCHDOG_RESTART_LEDGER_FILE);
    let Ok(json) = serde_json::to_string(ledger) else {
        return;
    };
    let _ = std::fs::write(path, json);
}

fn prune_timestamps_window(timestamps: &mut Vec<u64>, now_ms: u64, window_ms: u64) {
    timestamps.retain(|ts| now_ms.saturating_sub(*ts) <= window_ms);
}

fn request_controlled_self_restart(app: &AppHandle, reason: &str) -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|err| format!("current_exe failed: {}", err))?;
    let mut cmd = std::process::Command::new(&current_exe);
    for arg in std::env::args_os().skip(1) {
        cmd.arg(arg);
    }
    apply_hidden_creation_flags(&mut cmd);
    cmd.spawn()
        .map_err(|err| format!("Failed to spawn replacement process: {}", err))?;
    warn!(
        "Frontend watchdog requested controlled self-restart (reason={})",
        reason
    );
    app.exit(0);
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_local_dump_registry_value(
    key_path: &str,
    value_name: &str,
    value_type: &str,
    value_data: &str,
) -> Result<(), String> {
    let mut cmd = std::process::Command::new("reg");
    cmd.args([
        "add", key_path, "/v", value_name, "/t", value_type, "/d", value_data, "/f",
    ]);
    apply_hidden_creation_flags(&mut cmd);
    let status = cmd
        .status()
        .map_err(|err| format!("reg add failed: {}", err))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "reg add exited with code {:?} for {}\\{}",
            status.code(),
            key_path,
            value_name
        ))
    }
}

#[cfg(target_os = "windows")]
fn configure_windows_local_dumps(app: &AppHandle) {
    let dump_dir = crate::paths::resolve_base_dir(app).join("crashdumps");
    if let Err(err) = std::fs::create_dir_all(&dump_dir) {
        warn!(
            "Failed to create crash dump directory '{}': {}",
            dump_dir.display(),
            err
        );
        return;
    }
    let dump_dir_value = dump_dir.to_string_lossy().to_string();
    for exe_name in [
        "trispr-flow.exe",
        "Trispr Flow.exe",
        "com.trispr.flow.exe",
        "msedgewebview2.exe",
    ] {
        let key = format!(
            r"HKCU\Software\Microsoft\Windows\Windows Error Reporting\LocalDumps\{}",
            exe_name
        );
        let result = (|| -> Result<(), String> {
            apply_local_dump_registry_value(&key, "DumpType", "REG_DWORD", "2")?;
            apply_local_dump_registry_value(&key, "DumpCount", "REG_DWORD", "10")?;
            apply_local_dump_registry_value(&key, "DumpFolder", "REG_EXPAND_SZ", &dump_dir_value)?;
            Ok(())
        })();
        match result {
            Ok(()) => info!(
                "Crash dump capture configured for {} (folder: {})",
                exe_name,
                dump_dir.display()
            ),
            Err(err) => warn!(
                "Failed to configure crash dump capture for {}: {}",
                exe_name, err
            ),
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn configure_windows_local_dumps(_app: &AppHandle) {}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn build_startup_degraded_reasons(
    settings: &Settings,
    whisper_cli_ready: bool,
    model_ready: bool,
    status: &StartupStatus,
) -> Vec<String> {
    let mut degraded_reasons = Vec::new();

    if !whisper_cli_ready {
        degraded_reasons.push("Local transcription runtime unavailable.".to_string());
    }
    if !model_ready {
        degraded_reasons.push(format!(
            "Selected transcription model '{}' is not available yet.",
            settings.model
        ));
    }
    if capability_enabled(settings, RuntimeCapability::AiRefinement)
        && settings.ai_fallback.provider == "ollama"
    {
        if status.ollama_starting {
            degraded_reasons.push("Ollama is starting in background.".to_string());
        } else if !status.ollama_ready {
            degraded_reasons.push(
                "Ollama refinement unavailable; raw or rule-based output remains active."
                    .to_string(),
            );
        }
    }

    degraded_reasons
}

pub(crate) fn startup_status_snapshot(state: &AppState) -> StartupStatus {
    state
        .startup_status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

pub(crate) fn update_startup_status<F>(app: &AppHandle, state: &AppState, f: F) -> StartupStatus
where
    F: FnOnce(&mut StartupStatus),
{
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let whisper_cli_ready = paths::resolve_whisper_cli_path_for_backend(Some(
        settings.local_backend_preference.as_str(),
    ))
    .is_some();
    let model_ready = check_model_available(app.clone(), settings.model.clone());
    let snapshot = {
        let mut status = state
            .startup_status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut status);
        status.transcription_ready = whisper_cli_ready && model_ready;
        status.rules_ready = true;
        status.degraded_reasons =
            build_startup_degraded_reasons(&settings, whisper_cli_ready, model_ready, &status);
        status.clone()
    };
    let _ = app.emit("startup:status", &snapshot);
    snapshot
}

pub(crate) fn refresh_startup_status(app: &AppHandle, state: &AppState) -> StartupStatus {
    update_startup_status(app, state, |_| {})
}

pub(crate) fn update_runtime_diagnostics<F>(
    app: &AppHandle,
    state: &AppState,
    f: F,
) -> RuntimeDiagnostics
where
    F: FnOnce(&mut RuntimeDiagnostics),
{
    let snapshot = {
        let mut diagnostics = state
            .runtime_diagnostics
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&mut diagnostics);
        diagnostics.clone()
    };
    let _ = app.emit("runtime:diagnostics", &snapshot);
    snapshot
}

pub(crate) fn managed_child_slot_status(
    slot: &Mutex<Option<std::process::Child>>,
) -> (Option<u32>, bool) {
    let Ok(mut guard) = slot.lock() else {
        return (None, false);
    };

    let Some(child) = guard.as_mut() else {
        return (None, false);
    };

    let pid = Some(child.id());
    match child.try_wait() {
        Ok(Some(_)) => {
            *guard = None;
            (pid, false)
        }
        Ok(None) => (pid, true),
        Err(_) => (pid, false),
    }
}

pub(crate) fn refresh_runtime_diagnostics(app: &AppHandle, state: &AppState) -> RuntimeDiagnostics {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let strict_backend = match settings
        .local_backend_preference
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "cuda" => Some("cuda"),
        "vulkan" => Some("vulkan"),
        _ => None,
    };
    let whisper_cli = strict_backend
        .and_then(|backend| {
            crate::paths::resolve_whisper_cli_path_for_backend(Some(backend)).and_then(|path| {
                if crate::transcription::whisper_backend_from_cli_path(path.as_path()) == backend {
                    Some(path)
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            crate::paths::resolve_whisper_cli_path_for_backend(Some(
                settings.local_backend_preference.as_str(),
            ))
        });
    let whisper_server = strict_backend
        .and_then(|backend| {
            crate::paths::resolve_whisper_server_path_for_backend(Some(backend)).and_then(|path| {
                if crate::transcription::whisper_backend_from_cli_path(path.as_path()) == backend {
                    Some(path)
                } else {
                    None
                }
            })
        })
        .or_else(|| {
            crate::paths::resolve_whisper_server_path_for_backend(Some(
                settings.local_backend_preference.as_str(),
            ))
        });
    let whisper_backend = whisper_cli
        .as_deref()
        .map(crate::transcription::whisper_backend_from_cli_path)
        .or(strict_backend)
        .unwrap_or("unknown")
        .to_string();
    let (managed_pid, _) = managed_child_slot_status(&state.managed_ollama_child);
    let endpoint = settings.providers.ollama.endpoint.clone();
    // Only ping Ollama when it is the active provider — avoids a 2–3 s Windows
    // localhost-DNS stall on every save_settings call when the user is on LM Studio
    // or Oobabooga.  The reachability field stays false until Ollama is re-selected
    // and the frontend explicitly calls refreshOllamaRuntimeState.
    let ollama_is_active_provider = capability_enabled(&settings, RuntimeCapability::AiRefinement)
        && settings.ai_fallback.provider == "ollama";
    let reachable = if ollama_is_active_provider {
        let now = crate::util::now_ms();
        let next_ms = OLLAMA_DIAG_NEXT_MS.load(Ordering::Relaxed);
        if now < next_ms {
            // Still within backoff window — skip the network call, report not ready.
            false
        } else if ping_ollama_quick(&endpoint).is_ok() {
            OLLAMA_DIAG_FAIL_COUNT.store(0, Ordering::Relaxed);
            OLLAMA_DIAG_NEXT_MS.store(0, Ordering::Relaxed);
            true
        } else {
            let failures = OLLAMA_DIAG_FAIL_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
            let delay_ms: u64 = match failures {
                0 | 1 => 0,
                2 => 2_000,
                3 => 4_000,
                4 => 8_000,
                _ => 30_000,
            };
            if delay_ms > 0 {
                OLLAMA_DIAG_NEXT_MS.store(now + delay_ms, Ordering::Relaxed);
            }
            false
        }
    } else {
        false
    };

    if reachable {
        update_startup_status(app, state, |status| {
            status.ollama_ready = true;
            status.ollama_starting = false;
        });
    }

    update_runtime_diagnostics(app, state, |diagnostics| {
        let watchdog_snapshot = state
            .frontend_watchdog_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        diagnostics.ollama.configured_path = settings.providers.ollama.runtime_path.clone();
        diagnostics.ollama.detected = !settings.providers.ollama.runtime_path.trim().is_empty();
        diagnostics.ollama.managed_pid = managed_pid;
        diagnostics.ollama.endpoint = endpoint.clone();
        diagnostics.ollama.reachable = reachable;
        if reachable {
            diagnostics.ollama.spawn_stage = "ready".to_string();
            diagnostics.ollama.last_error.clear();
        }

        diagnostics.whisper.cli_path = whisper_cli
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        diagnostics.whisper.server_path = whisper_server
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        diagnostics.whisper.backend_selected = whisper_backend.clone();
        if diagnostics.whisper.mode == "idle" && settings.capture_enabled {
            diagnostics.whisper.mode = "cli".to_string();
        }
        diagnostics.frontend_watchdog.recovery_count = watchdog_snapshot.recovery_count;
        diagnostics.frontend_watchdog.restart_count = watchdog_snapshot.restart_count;
        diagnostics.frontend_watchdog.last_recovery_reason =
            watchdog_snapshot.last_recovery_reason.clone();
        diagnostics.frontend_watchdog.last_degraded_reason =
            watchdog_snapshot.last_degraded_reason.clone();
    })
}

#[cfg(target_os = "windows")]
fn create_managed_process_job() -> Option<state::ManagedProcessJob> {
    use std::ffi::c_void;
    use windows_sys::Win32::System::JobObjects::{
        JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe extern "system" {
        fn CreateJobObjectW(
            lp_job_attributes: *const c_void,
            lp_name: *const u16,
        ) -> windows_sys::Win32::Foundation::HANDLE;
    }

    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        warn!("Failed to create managed-process job object.");
        return None;
    }

    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &mut limits as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        warn!("Failed to configure managed-process job object.");
        unsafe {
            let _ = windows_sys::Win32::Foundation::CloseHandle(handle);
        }
        return None;
    }

    Some(state::ManagedProcessJob {
        handle: handle as isize,
    })
}

pub(crate) struct ManagedChildSpawnResult {
    pub(crate) pid: u32,
    pub(crate) job_assigned: bool,
}

#[cfg(target_os = "windows")]
pub(crate) fn assign_child_to_managed_process_job(
    state: &AppState,
    label: &str,
    child: &std::process::Child,
) -> Result<(), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;

    let Some(job) = state.managed_process_job.as_ref() else {
        return Err("no managed-process job object available".to_string());
    };
    let process_handle = child.as_raw_handle();
    if process_handle.is_null() {
        return Err(format!(
            "failed to assign {label} to managed-process job object: null handle"
        ));
    }
    let ok = unsafe { AssignProcessToJobObject(job.handle as _, process_handle as *mut _) };
    if ok == 0 {
        return Err(format!(
            "failed to assign {label} (pid {}) to managed-process job object",
            child.id()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn assign_child_to_managed_process_job(
    _state: &AppState,
    _label: &str,
    _child: &std::process::Child,
) -> Result<(), String> {
    Ok(())
}

pub(crate) fn spawn_managed_child(
    state: &AppState,
    label: &str,
    slot: &Mutex<Option<std::process::Child>>,
    cmd: &mut std::process::Command,
) -> Result<ManagedChildSpawnResult, String> {
    let child = cmd
        .spawn()
        .map_err(|err| format!("spawn_failed: {}", err))?;
    let pid = child.id();
    let job_assigned = match assign_child_to_managed_process_job(state, label, &child) {
        Ok(()) => true,
        Err(err) => {
            warn!("{err}");
            false
        }
    };

    match slot.lock() {
        Ok(mut guard) => {
            *guard = Some(child);
        }
        Err(err) => {
            let mut child = child;
            terminate_child_process(label, &mut child);
            return Err(format!(
                "spawn_failed: failed to store managed child handle for {label}: {}",
                err
            ));
        }
    }

    Ok(ManagedChildSpawnResult { pid, job_assigned })
}

fn terminate_child_process(label: &str, child: &mut std::process::Child) {
    let pid = child.id();
    info!("Stopping {label} (pid {pid})");

    #[cfg(target_os = "windows")]
    {
        let pid_string = pid.to_string();
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/PID", pid_string.as_str(), "/T", "/F"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        apply_hidden_creation_flags(&mut cmd);
        let forced = cmd.status();
        match forced {
            Ok(status) if !status.success() => {
                warn!("Forced taskkill returned non-zero exit for {label} (pid {pid}): {status}");
            }
            Err(err) => {
                warn!("Failed to force taskkill for {label} (pid {pid}): {err}");
            }
            Ok(_) => {}
        }
    }

    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    if let Err(err) = child.wait() {
        warn!("Failed to wait for {label} (pid {pid}): {err}");
    }
}

pub(crate) fn terminate_managed_child_slot(label: &str, slot: &Mutex<Option<std::process::Child>>) {
    let child = match slot.lock() {
        Ok(mut guard) => guard.take(),
        Err(err) => {
            warn!("Failed to lock managed process slot for {label}: {err}");
            None
        }
    };
    if let Some(mut child) = child {
        terminate_child_process(label, &mut child);
    }
}

pub(crate) fn cleanup_managed_processes(app: &AppHandle, state: &AppState) {
    crate::multimodal_io::shutdown_piper_daemon(state);
    terminate_managed_child_slot("managed Ollama runtime", &state.managed_ollama_child);
    crate::ollama_runtime::clear_ollama_pid_lockfile(app);
    terminate_managed_child_slot(
        "managed Whisper-Server runtime",
        &state.managed_whisper_server_child,
    );
    crate::uiautomation_capture::shutdown(&state.enter_capture);
}

fn cancel_backlog_auto_expand(_app: &AppHandle) {
    BACKLOG_PROMPT_CANCELLED.store(true, Ordering::Release);
}

fn schedule_backlog_auto_expand(app: AppHandle, cancel_item: MenuItem<Wry>) {
    if BACKLOG_PROMPT_ACTIVE.swap(true, Ordering::AcqRel) {
        return;
    }
    BACKLOG_PROMPT_CANCELLED.store(false, Ordering::Release);
    let _ = cancel_item.set_enabled(true);
    let _ = cancel_item.set_text(format!(
        "Cancel Auto-Expand ({}s)",
        BACKLOG_AUTOEXPAND_TIMEOUT_MS / 1000
    ));

    crate::util::spawn_guarded("backlog_autoexpand", move || {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_millis(BACKLOG_AUTOEXPAND_TIMEOUT_MS);
        while std::time::Instant::now() < deadline {
            if BACKLOG_PROMPT_CANCELLED.load(Ordering::Acquire) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if !BACKLOG_PROMPT_CANCELLED.load(Ordering::Acquire) {
            let _ = expand_transcribe_backlog_inner(&app);
        }

        let _ = cancel_item.set_enabled(false);
        let _ = cancel_item.set_text("Cancel Auto-Expand");
        BACKLOG_PROMPT_ACTIVE.store(false, Ordering::Release);
    });
}

/// Returns a human-readable reason if the hotkey is reserved by the OS and
/// therefore cannot be captured via a global shortcut — regardless of what
/// other applications are doing. These keys always fail to register and would
/// produce a scary "already registered" error on every registration pass.
fn os_reserved_hotkey_reason(key: &str) -> Option<&'static str> {
    let normalized = key
        .to_ascii_lowercase()
        .replace("commandorcontrol", "ctrl")
        .replace("command", "ctrl")
        .replace("super", "win");
    let flat = normalized.replace(' ', "");
    // Ctrl+Shift+Esc opens Windows Task Manager; Win cannot yield it.
    if flat.contains("ctrl+shift+escape") || flat.contains("ctrl+shift+esc") {
        return Some("Ctrl+Shift+Esc is reserved by Windows for Task Manager");
    }
    // Ctrl+Alt+Del is the Secure Attention Sequence and cannot be intercepted.
    if flat.contains("ctrl+alt+delete") || flat.contains("ctrl+alt+del") {
        return Some("Ctrl+Alt+Del is reserved by Windows as the Secure Attention Sequence");
    }
    // Win+L locks the workstation and is reserved.
    if flat.contains("win+l") {
        return Some("Win+L is reserved by Windows for lock workstation");
    }
    None
}

/// Pattern-matches the error returned by `GlobalShortcutManager` when the key
/// is already held by another application in the current session. We use this
/// to downgrade the user-facing modal to a quieter inline warning, since the
/// app keeps working — the shortcut just won't fire.
fn is_already_registered_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("already registered") || lower.contains("hotkey already")
}

fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let manager = app.global_shortcut();

    // Unregister all existing hotkeys to prevent conflicts
    if let Err(e) = manager.unregister_all() {
        warn!(
            "Failed to unregister all hotkeys (may be OK if none registered): {}",
            e
        );
    } else {
        info!("Successfully unregistered all hotkeys");
    }

    // Collect registration errors instead of failing early
    let mut errors = Vec::new();

    // Track which keys are claimed during THIS registration pass. Prevents
    // the scary "HotKey already registered" modal when two Trispr slots map
    // to the same combination — the first one wins, later ones log and skip.
    let claimed: std::cell::RefCell<std::collections::HashSet<String>> =
        std::cell::RefCell::new(std::collections::HashSet::new());

    // Returns true if the caller should proceed with registration. Returns
    // false (and logs at INFO) for internal duplicates or OS-reserved keys.
    let try_claim = |key: &str, slot: &'static str| -> bool {
        if key.is_empty() {
            return false;
        }
        if let Some(reason) = os_reserved_hotkey_reason(key) {
            info!("Skipping {} hotkey '{}': {}.", slot, key, reason);
            return false;
        }
        let mut set = claimed.borrow_mut();
        if set.contains(key) {
            info!(
                "Skipping {} hotkey '{}': already claimed by another Trispr slot in this registration pass.",
                slot, key
            );
            return false;
        }
        set.insert(key.to_string());
        true
    };

    let register_ptt = || -> Result<(), String> {
        let ptt = settings.hotkey_ptt.trim();
        if ptt.is_empty() {
            return Ok(());
        }
        if !try_claim(ptt, "PTT") {
            return Ok(());
        }
        info!("Registering PTT hotkey (hold): {}", ptt);
        match manager.on_shortcut(ptt, |app, _shortcut, event| {
            let app = app.clone();
            if event.state == ShortcutState::Pressed {
                PTT_KEY_HELD.store(true, Ordering::Release);
                info!("PTT hotkey pressed");
                if PTT_PRESS_IN_FLIGHT
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    crate::util::spawn_guarded("ptt_hotkey_press", move || {
                        struct InFlightReset;
                        impl Drop for InFlightReset {
                            fn drop(&mut self) {
                                PTT_PRESS_IN_FLIGHT.store(false, Ordering::Release);
                            }
                        }
                        let _in_flight_reset = InFlightReset;

                        if let Err(err) = crate::audio::handle_ptt_press(&app) {
                            error!("PTT hotkey press handler failed: {}", err);
                            emit_error(
                                &app,
                                AppError::AudioDevice(format!(
                                    "PTT startup failed: {}",
                                    err.trim()
                                )),
                                Some("PTT"),
                            );
                            return;
                        }

                        // Release can arrive while press-handling work is still in flight.
                        // If so, complete the pending stop after press initialization.
                        if !PTT_KEY_HELD.load(Ordering::Acquire) {
                            crate::audio::handle_ptt_release_async(app.clone());
                        }
                    });
                } else {
                    warn!("PTT press ignored while previous press handling is still active");
                }
            } else {
                PTT_KEY_HELD.store(false, Ordering::Release);
                info!("PTT hotkey released");
                crate::audio::handle_ptt_release_async(app);
            }
        }) {
            Ok(_) => {
                info!("PTT hotkey registered successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to register PTT hotkey '{}': {}", ptt, e);
                // Warn user but don't block - they might want to use it anyway
                emit_error(
          app,
          AppError::Hotkey(format!(
            "Warning: PTT hotkey '{}' may conflict with another application ({}). It might still work.",
            ptt, e
          )),
          Some("Hotkey Registration"),
        );
                // Return Ok to allow app to continue
                warn!("Continuing despite PTT hotkey registration failure");
                Ok(())
            }
        }
    };

    let register_toggle = || -> Result<(), String> {
        let toggle = settings.hotkey_toggle.trim();
        if toggle.is_empty() {
            return Ok(());
        }
        if !try_claim(toggle, "Toggle") {
            return Ok(());
        }
        info!("Registering Toggle hotkey (click): {}", toggle);
        match manager.on_shortcut(toggle, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                info!("Toggle hotkey pressed");
                let app = app.clone();
                crate::audio::handle_toggle_async(app);
            }
        }) {
            Ok(_) => {
                info!("Toggle hotkey registered successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to register Toggle hotkey '{}': {}", toggle, e);
                // Warn user but don't block
                emit_error(
          app,
          AppError::Hotkey(format!(
            "Warning: Toggle hotkey '{}' may conflict with another application ({}). It might still work.",
            toggle, e
          )),
          Some("Hotkey Registration"),
        );
                warn!("Continuing despite Toggle hotkey registration failure");
                Ok(())
            }
        }
    };

    let register_transcribe = || -> Result<(), String> {
        let hotkey = settings.transcribe_hotkey.trim();
        if hotkey.is_empty() {
            return Ok(());
        }
        if !try_claim(hotkey, "Transcribe") {
            return Ok(());
        }
        info!("Registering Transcribe hotkey (toggle): {}", hotkey);
        match manager.on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let app = app.clone();
                let was_enabled = app
                    .state::<AppState>()
                    .settings
                    .read()
                    .map(|settings| settings.transcribe_enabled)
                    .unwrap_or(false);
                let target_enabled = !was_enabled;
                let effective_enabled = match set_transcribe_enabled(&app, target_enabled) {
                    Ok(enabled) => enabled,
                    Err(err) => {
                        emit_error(&app, AppError::AudioDevice(err), Some("System Audio"));
                        return;
                    }
                };
                if effective_enabled != was_enabled {
                    let cue = if effective_enabled { "start" } else { "stop" };
                    let _ = app.emit("audio:cue", cue);
                }
            }
        }) {
            Ok(_) => {
                info!("Transcribe hotkey registered successfully");
                Ok(())
            }
            Err(e) => {
                error!("Failed to register Transcribe hotkey '{}': {}", hotkey, e);
                // Warn user but don't block
                emit_error(
          app,
          AppError::Hotkey(format!(
            "Warning: Transcribe hotkey '{}' may conflict with another application ({}). It might still work.",
            hotkey, e
          )),
          Some("Hotkey Registration"),
        );
                warn!("Continuing despite Transcribe hotkey registration failure");
                Ok(())
            }
        }
    };

    let register_product_mode_toggle = || -> Result<(), String> {
        let hotkey = settings.hotkey_product_mode_toggle.trim();
        if hotkey.is_empty() {
            return Ok(());
        }
        if !try_claim(hotkey, "Product Mode") {
            return Ok(());
        }
        info!("Registering Product Mode hotkey (toggle): {}", hotkey);
        match manager.on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_product_mode_async(app.clone());
            }
        }) {
            Ok(_) => {
                info!("Product Mode hotkey registered successfully");
                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                if is_already_registered_error(&err_str) {
                    warn!(
                        "Product Mode hotkey '{}' is already held by another application — shortcut will not fire.",
                        hotkey
                    );
                    Ok(())
                } else {
                    error!(
                        "Failed to register Product Mode hotkey '{}': {}",
                        hotkey, err_str
                    );
                    emit_error(
                        app,
                        AppError::Hotkey(format!(
                            "Could not register Product Mode hotkey '{}': {}",
                            hotkey, err_str
                        )),
                        Some("Hotkey Registration"),
                    );
                    Err(err_str)
                }
            }
        }
    };

    let register_tts_stop = || -> Result<(), String> {
        let hotkey = settings.hotkey_tts_stop.trim();
        if hotkey.is_empty() {
            return Ok(());
        }
        if !try_claim(hotkey, "TTS Stop") {
            return Ok(());
        }
        info!("Registering TTS Stop hotkey: {}", hotkey);
        match manager.on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let app = app.clone();
                let _ =
                    crate::multimodal_io::stop_tts_internal(&app, app.state::<AppState>().inner());
            }
        }) {
            Ok(_) => {
                info!("TTS Stop hotkey registered successfully");
                Ok(())
            }
            Err(e) => {
                let err_str = e.to_string();
                if is_already_registered_error(&err_str) {
                    warn!(
                        "TTS Stop hotkey '{}' is already held by another application — shortcut will not fire.",
                        hotkey
                    );
                    Ok(())
                } else {
                    error!(
                        "Failed to register TTS Stop hotkey '{}': {}",
                        hotkey, err_str
                    );
                    emit_error(
                        app,
                        AppError::Hotkey(format!(
                            "Could not register TTS Stop hotkey '{}': {}",
                            hotkey, err_str
                        )),
                        Some("Hotkey Registration"),
                    );
                    Err(err_str)
                }
            }
        }
    };

    match settings.mode.as_str() {
        "ptt" => {
            if let Err(e) = register_ptt() {
                errors.push(format!("PTT: {}", e));
            }
            if let Err(e) = register_toggle() {
                errors.push(format!("Toggle: {}", e));
            }
        }
        "vad" => {}
        _ => {
            if let Err(e) = register_ptt() {
                errors.push(format!("PTT: {}", e));
            }
            if let Err(e) = register_toggle() {
                errors.push(format!("Toggle: {}", e));
            }
        }
    }

    if let Err(e) = register_transcribe() {
        errors.push(format!("Transcribe: {}", e));
    }
    if let Err(e) = register_product_mode_toggle() {
        errors.push(format!("Product Mode: {}", e));
    }
    if let Err(e) = register_tts_stop() {
        errors.push(format!("TTS Stop: {}", e));
    }

    // Register Toggle Activation Words hotkey
    let hotkey = settings.hotkey_toggle_activation_words.trim();
    if !hotkey.is_empty() && try_claim(hotkey, "Toggle Activation Words") {
        match manager.on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_activation_words_async(app.clone());
            }
        }) {
            Ok(_) => {
                info!("Toggle Activation Words hotkey registered successfully");
            }
            Err(e) => {
                let err_str = e.to_string();
                if is_already_registered_error(&err_str) {
                    warn!(
                        "Toggle Activation Words hotkey '{}' is already held by another application — shortcut will not fire.",
                        hotkey
                    );
                } else {
                    error!(
                        "Failed to register Toggle Activation Words hotkey '{}': {}",
                        hotkey, err_str
                    );
                    errors.push(format!("Toggle Activation Words: {}", err_str));
                    emit_error(
                        app,
                        AppError::Hotkey(format!(
                            "Could not register Toggle Activation Words hotkey '{}': {}",
                            hotkey, err_str
                        )),
                        Some("Hotkey Registration"),
                    );
                }
            }
        }
    }

    // Emit registration status to frontend so UI can show conflict badges
    {
        let status = serde_json::json!({
            "ptt": {
                "key": settings.hotkey_ptt.trim(),
                "registered": !errors.iter().any(|e| e.starts_with("PTT")),
                "error": errors.iter().find(|e| e.starts_with("PTT")).cloned(),
            },
            "toggle": {
                "key": settings.hotkey_toggle.trim(),
                "registered": !errors.iter().any(|e| e.starts_with("Toggle:")),
                "error": errors.iter().find(|e| e.starts_with("Toggle:")).cloned(),
            },
            "transcribe": {
                "key": settings.transcribe_hotkey.trim(),
                "registered": !errors.iter().any(|e| e.starts_with("Transcribe")),
                "error": errors.iter().find(|e| e.starts_with("Transcribe")).cloned(),
            },
            "activation_words": {
                "key": settings.hotkey_toggle_activation_words.trim(),
                "registered": !errors.iter().any(|e| e.starts_with("Toggle Activation")),
                "error": errors.iter().find(|e| e.starts_with("Toggle Activation")).cloned(),
            },
            "product_mode": {
                "key": settings.hotkey_product_mode_toggle.trim(),
                "registered": !errors.iter().any(|e| e.starts_with("Product Mode")),
                "error": errors.iter().find(|e| e.starts_with("Product Mode")).cloned(),
            },
            "tts_stop": {
                "key": settings.hotkey_tts_stop.trim(),
                "registered": !errors.iter().any(|e| e.starts_with("TTS Stop")),
                "error": errors.iter().find(|e| e.starts_with("TTS Stop")).cloned(),
            },
        });
        let _ = app.emit("hotkey:registration-status", &status);
    }

    // Report all errors if any occurred, but don't fail completely
    if !errors.is_empty() {
        let error_msg = format!("Some hotkeys failed to register: {}", errors.join(", "));
        warn!("{}", error_msg);
        Ok(())
    } else {
        info!("All hotkeys registered successfully");
        Ok(())
    }
}

#[tauri::command]
async fn get_settings(app: AppHandle) -> Settings {
    // spawn_blocking keeps the settings.lock() acquisition off the Tauri command
    // executor thread, preventing contention with get_startup_status / get_runtime_diagnostics
    // which also need the same lock during the bootstrap Promise.all.
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
async fn get_startup_status(app: AppHandle) -> StartupStatus {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        refresh_startup_status(&app, state.inner())
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn get_runtime_diagnostics(app: AppHandle) -> RuntimeDiagnostics {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        refresh_runtime_diagnostics(&app, state.inner())
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
fn get_runtime_metrics_snapshot(
    state: State<'_, AppState>,
) -> crate::state::RuntimeMetricsSnapshot {
    runtime_metrics_snapshot(state.inner())
}

#[tauri::command]
fn record_runtime_metric(state: State<'_, AppState>, metric: String) -> Result<(), String> {
    match metric.trim() {
        "refinement_timeout" | "refinement_fallback_timed_out" => {
            record_refinement_timeout(state.inner());
            record_refinement_fallback_timed_out(state.inner());
            Ok(())
        }
        other => Err(format!("Unknown runtime metric '{}'", other)),
    }
}

pub(crate) fn format_ureq_status_error(
    context: &str,
    code: u16,
    response: ureq::Response,
) -> String {
    let mut body = response.into_string().unwrap_or_default();
    body = body.replace('\n', " ").replace('\r', " ");
    let body = body.trim();
    if body.is_empty() {
        format!("{} failed with HTTP {}", context, code)
    } else {
        format!("{} failed with HTTP {}: {}", context, code, body)
    }
}

#[cfg(test)]
mod piper_daemon_lifecycle_tests {
    use super::{piper_daemon_lifecycle_action, PiperDaemonLifecycleAction};
    use crate::modules::VoiceOutputSettings;

    #[test]
    fn lifecycle_action_prewarms_when_voice_output_enabled_and_piper_primary() {
        let mut voice = VoiceOutputSettings::default();
        voice.enabled = true;
        voice.default_provider = "local_custom".to_string();
        voice.fallback_provider = "windows_native".to_string();

        assert_eq!(
            piper_daemon_lifecycle_action(&voice),
            PiperDaemonLifecycleAction::PrewarmPrimary
        );
    }

    #[test]
    fn lifecycle_action_stops_when_voice_output_disabled() {
        let mut voice = VoiceOutputSettings::default();
        voice.enabled = false;
        voice.default_provider = "local_custom".to_string();

        assert_eq!(
            piper_daemon_lifecycle_action(&voice),
            PiperDaemonLifecycleAction::Shutdown
        );
    }

    #[test]
    fn lifecycle_action_stops_when_piper_is_only_fallback() {
        let mut voice = VoiceOutputSettings::default();
        voice.enabled = true;
        voice.default_provider = "windows_native".to_string();
        voice.fallback_provider = "local_custom".to_string();

        assert_eq!(
            piper_daemon_lifecycle_action(&voice),
            PiperDaemonLifecycleAction::Shutdown
        );
    }
}
fn assistant_requires_transcribe(settings: &Settings) -> bool {
    settings
        .product_mode
        .trim()
        .eq_ignore_ascii_case("assistant")
        && capability_enabled(settings, RuntimeCapability::WorkflowAgent)
        && settings.workflow_agent.hands_free_enabled
}

pub(crate) fn reconcile_assistant_transcribe_flag(settings: &mut Settings) -> bool {
    if assistant_requires_transcribe(settings) && !settings.transcribe_enabled {
        settings.transcribe_enabled = true;
        return true;
    }
    false
}

fn set_transcribe_enabled(app: &AppHandle, enabled: bool) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let (settings, effective_enabled) = {
        let mut current = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let requested = if assistant_requires_transcribe(&current) {
            true
        } else {
            enabled
        };
        if current.transcribe_enabled == requested {
            return Ok(current.transcribe_enabled);
        }
        current.transcribe_enabled = requested;
        (current.clone(), requested)
    };

    if effective_enabled {
        if let Err(err) = start_transcribe_monitor(app, &state, &settings) {
            let reverted = {
                let mut current = state
                    .settings
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                current.transcribe_enabled = false;
                current.clone()
            };
            let _ = app.emit("settings-changed", reverted.clone());
            let _ = app.emit("menu:update-transcribe", false);
            return Err(err);
        }
    } else {
        stop_transcribe_monitor_and_release_whisper(app, &state);
    }

    let _ = app.emit("settings-changed", settings.clone());
    let _ = app.emit("menu:update-transcribe", effective_enabled);
    Ok(effective_enabled)
}

/// Synchronous core of save_settings — used by both the async Tauri command
/// and internal callers (e.g. tray menu handlers) that cannot await.
pub(crate) fn save_settings_inner(app: &AppHandle, settings: &mut Settings) -> Result<(), String> {
    info!(
        "[DIAG] save_settings_inner: enter (thread {:?})",
        std::thread::current().id()
    );
    let state = app.state::<AppState>();
    info!("[DIAG] save_settings_inner: acquiring settings lock (read)");
    let (
        prev_mode,
        prev_device,
        prev_capture_enabled,
        prev_transcribe_enabled,
        prev_transcribe_output_device,
        prev_local_backend_preference,
        prev_ai_refinement_enabled,
        prev_provider,
    ) = {
        let current = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            current.mode.clone(),
            current.input_device.clone(),
            current.capture_enabled,
            current.transcribe_enabled,
            current.transcribe_output_device.clone(),
            current.local_backend_preference.clone(),
            current.ai_fallback.enabled,
            current.ai_fallback.provider.clone(),
        )
    };
    info!("[DIAG] save_settings_inner: normalizing");
    normalize_ai_fallback_fields(settings);
    normalize_continuous_dump_fields(settings);
    normalize_history_alias_fields(settings);
    normalize_module_settings(&mut settings.module_settings);
    normalize_assistant_core_binding(settings);
    normalize_product_mode_field(settings);
    normalize_ai_refinement_module_binding(settings);
    normalize_gdd_module_settings(&mut settings.gdd_module_settings);
    normalize_confluence_settings(&mut settings.confluence_settings);
    normalize_workflow_agent_settings(&mut settings.workflow_agent);
    normalize_assistant_presence_binding(settings);
    normalize_vision_input_settings(&mut settings.vision_input_settings);
    normalize_voice_output_settings(&mut settings.voice_output_settings);
    normalize_task_capture_settings(&mut settings.task_capture_settings);
    reconcile_assistant_transcribe_flag(settings);

    info!("[DIAG] save_settings_inner: acquiring settings lock (write)");
    {
        let mut current = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = settings.clone();
    }
    crate::state::sync_diagnostic_logging_enabled(settings);
    info!("[DIAG] save_settings_inner: saving file");
    sync_model_dir_env(settings);
    save_settings_file(app, settings)?;
    schedule_piper_daemon_reconcile(
        app.clone(),
        settings.voice_output_settings.clone(),
        "save_settings",
    );

    // Register hotkeys on a detached thread: the Windows GlobalShortcut API
    // internally dispatches to the main event-loop thread and waits for
    // acknowledgement. When called from a Tauri command-pool thread (which
    // save_settings runs on), this creates a cross-thread deadlock because
    // the event loop may be waiting for this command to finish.
    {
        let app_clone = app.clone();
        let settings_clone = settings.clone();
        crate::util::spawn_guarded("register_hotkeys", move || {
            if let Err(e) = register_hotkeys(&app_clone, &settings_clone) {
                warn!("Hotkey registration failed: {}", e);
            }
        });
    }

    // LM Studio daemon lifecycle: start when switching TO lm_studio,
    // stop when switching AWAY from lm_studio.
    if prev_provider != settings.ai_fallback.provider {
        if settings.ai_fallback.provider == "lm_studio" {
            let preferred_model = settings
                .providers
                .lm_studio
                .preferred_model
                .trim()
                .to_string();
            crate::util::spawn_guarded("lms_daemon_up", move || {
                lms_daemon_command("up");
                // Give the daemon a few seconds to bind its HTTP port before
                // attempting to load a model.
                if !preferred_model.is_empty() {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    lms_load_model(&preferred_model);
                }
            });
        } else if prev_provider == "lm_studio" {
            crate::util::spawn_guarded("lms_daemon_stop", || {
                lms_daemon_command("stop");
            });
        }
    }

    info!("[DIAG] save_settings_inner: hotkeys spawned, acquiring recorder lock");
    if let Ok(recorder) = state.recorder.lock() {
        recorder.input_gain_db.store(
            (settings.mic_input_gain_db * 1000.0) as i64,
            Ordering::Relaxed,
        );
    }
    info!("[DIAG] save_settings_inner: recorder lock released, checking mode change");

    let mode_changed = prev_mode != settings.mode;
    let device_changed = prev_device != settings.input_device;

    if mode_changed || (device_changed && settings.mode == "vad") {
        if prev_mode == "vad" || (settings.mode == "vad" && device_changed) {
            crate::audio::stop_vad_monitor(app, &state);
        }
        if settings.mode == "vad" && settings.capture_enabled {
            let _ = crate::audio::start_vad_monitor(app, &state, settings);
        }
    } else if settings.mode == "vad" {
        if let Ok(recorder) = state.recorder.lock() {
            recorder.update_vad_settings(
                settings.vad_threshold_start,
                settings.vad_threshold_sustain,
                settings.vad_silence_ms,
            );
        }
    }

    let capture_enabled_changed = prev_capture_enabled != settings.capture_enabled;
    if capture_enabled_changed {
        if !settings.capture_enabled {
            crate::audio::stop_vad_monitor(app, &state);
        } else if settings.mode == "vad" {
            let _ = crate::audio::start_vad_monitor(app, &state, settings);
        }
    }
    crate::audio::sync_ptt_hot_standby(app, &state, settings);

    let transcribe_enabled_changed = prev_transcribe_enabled != settings.transcribe_enabled;
    let transcribe_device_changed =
        prev_transcribe_output_device != settings.transcribe_output_device;
    if transcribe_enabled_changed {
        if !settings.transcribe_enabled {
            stop_transcribe_monitor_and_release_whisper(app, &state);
        } else {
            let _ = start_transcribe_monitor(app, &state, settings);
        }
    } else if transcribe_device_changed && settings.transcribe_enabled {
        stop_transcribe_monitor_and_release_whisper(app, &state);
        let _ = start_transcribe_monitor(app, &state, settings);
    }

    let local_backend_changed = !prev_local_backend_preference
        .trim()
        .eq_ignore_ascii_case(settings.local_backend_preference.trim());
    if local_backend_changed {
        info!(
            "Whisper backend preference changed: '{}' -> '{}'",
            prev_local_backend_preference, settings.local_backend_preference
        );
        if let Some(model_path) = crate::models::resolve_model_path(app, &settings.model) {
            if let Err(err) =
                crate::whisper_server::restart_whisper_server_if_running(app, &state, &model_path)
            {
                warn!(
                    "Failed to restart whisper-server after backend switch: {}",
                    err
                );
            }
            crate::whisper_server::schedule_whisper_server_warmup(
                app,
                state.inner(),
                &model_path,
                settings,
            );
        } else {
            warn!(
                "Skipping immediate backend switch warmup: model '{}' could not be resolved.",
                settings.model
            );
        }
    }

    info!("[DIAG] save_settings_inner: applying overlay settings");
    let overlay_settings = build_overlay_settings(settings);
    let _ = overlay::apply_overlay_settings(app, &overlay_settings);
    info!("[DIAG] save_settings_inner: overlay done");

    if prev_ai_refinement_enabled && !settings.ai_fallback.enabled {
        crate::audio::force_reset_refinement_activity(app, "forced_reset");
    } else if !prev_ai_refinement_enabled && settings.ai_fallback.enabled {
        schedule_ai_refinement_reenable_bootstrap(app.clone());
    }

    info!("[DIAG] save_settings_inner: acquiring recorder lock (2nd)");
    let recorder = state
        .recorder
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !recorder.active {
        let _ = emit_capture_idle_overlay(app, settings);
    }
    drop(recorder);
    info!("[DIAG] save_settings_inner: recorder lock released, refreshing status");

    // Fire startup-status refresh on a detached thread: resolve_model_path()
    // does blocking filesystem I/O (exists() checks, current_dir()) that can
    // hang indefinitely on Windows when the CWD is on a network drive or
    // becomes inaccessible.  Same pattern as runtime_diagnostics below.
    {
        let app_clone = app.clone();
        crate::util::spawn_guarded("startup_status_refresh", move || {
            refresh_startup_status(&app_clone, &app_clone.state::<AppState>());
        });
    }

    // Fire diagnostics (contains blocking ping_ollama_quick) on a detached thread
    // so the IPC/UI thread is never blocked by network latency.
    {
        let app_clone = app.clone();
        crate::util::spawn_guarded("runtime_diagnostics", move || {
            refresh_runtime_diagnostics(&app_clone, &app_clone.state::<AppState>());
        });
    }

    info!("[DIAG] save_settings_inner: emitting settings-changed");
    let _ = app.emit("settings-changed", settings.clone());
    assistant_presence::reconcile_assistant_presence_window(app, settings);
    let _ = workflow_agent::emit_assistant_baseline_state(
        app,
        state.inner(),
        settings,
        "save_settings",
    );
    let _ = app.emit("menu:update-mic", settings.capture_enabled);
    let _ = app.emit("menu:update-transcribe", settings.transcribe_enabled);
    info!("[DIAG] save_settings_inner: done");
    Ok(())
}

#[tauri::command]
async fn save_settings(app: AppHandle, mut settings: Settings) -> Result<(), String> {
    // Run on a blocking worker thread so the Tauri event-loop thread is never
    // stalled by file I/O, lock contention, or Win32 hotkey-registration calls.
    tauri::async_runtime::spawn_blocking(move || save_settings_inner(&app, &mut settings))
        .await
        .map_err(|e| format!("save_settings task failed: {}", e))?
}

#[tauri::command]
fn list_modules(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Vec<crate::modules::ModuleDescriptor> {
    let installed_package_ids = scan_installed_module_ids_lossy(&app);
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    module_registry::modules_as_descriptors_with_packages(
        &settings.module_settings,
        &installed_package_ids,
    )
}

#[tauri::command]
fn scan_module_packages(app: AppHandle) -> Result<module_package::ModulePackageScanReport, String> {
    let modules_dir = crate::paths::resolve_modules_dir(&app);
    module_package::scan_modules_dir(&modules_dir)
}

fn scan_installed_module_ids_lossy(app: &AppHandle) -> std::collections::HashSet<String> {
    let modules_dir = crate::paths::resolve_modules_dir(app);
    match module_package::scan_installed_module_ids(&modules_dir) {
        Ok(module_ids) => module_ids,
        Err(error) => {
            warn!("Failed to scan installed module packages: {}", error);
            std::collections::HashSet::new()
        }
    }
}

fn bundled_module_package_source(app: &AppHandle, module_id: &str) -> Option<PathBuf> {
    let module_id = canonicalize_module_id(module_id);
    let mut candidates = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("module-packages").join(module_id));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("module-packages")
            .join(module_id),
    );

    candidates.into_iter().find(|candidate| candidate.is_dir())
}

#[tauri::command]
fn install_bundled_module_package(
    app: AppHandle,
    module_id: String,
) -> Result<module_package::ModulePackageInstallResult, String> {
    let module_id = canonicalize_module_id(&module_id).to_string();
    let manifest = module_registry::find_manifest(&module_id)
        .ok_or_else(|| format!("Unknown module id '{}'.", module_id))?;
    if !manifest.bundled {
        return Err(format!(
            "Module '{}' is not bundled in this build.",
            module_id
        ));
    }
    let source_dir = bundled_module_package_source(&app, &module_id).ok_or_else(|| {
        format!(
            "Bundled module package '{}' was not found in app resources.",
            module_id
        )
    })?;
    let modules_dir = crate::paths::resolve_modules_dir(&app);
    module_package::install_package_from_dir(&source_dir, &modules_dir)
}

fn should_autostart_ai_refinement_runtime(settings: &Settings) -> bool {
    capability_enabled(settings, RuntimeCapability::AiRefinement)
        && settings.ai_fallback.provider == "ollama"
        && settings.ai_fallback.execution_mode == "local_primary"
}

fn warmup_ai_refinement_model_once(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let setup = crate::ai_fallback::prepare_refinement(app, settings)?;
    let warmup_text = "Warmup: local AI refinement runtime.";
    setup
        .provider
        .refine_transcript(warmup_text, &setup.model, &setup.options, &setup.api_key)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

pub(crate) fn schedule_ai_refinement_reenable_bootstrap(app: AppHandle) {
    crate::util::spawn_guarded("ai_refinement_reenable_bootstrap", move || {
        let initial_settings = {
            let state = app.state::<AppState>();
            let snapshot = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            snapshot
        };
        if !should_autostart_ai_refinement_runtime(&initial_settings) {
            return;
        }

        if let Err(error) = tauri::async_runtime::block_on(start_ollama_runtime(app.clone())) {
            warn!(
                "AI refinement re-enable autostart failed (continuing with raw fallback): {}",
                error
            );
            return;
        }

        if let Err(error) = tauri::async_runtime::block_on(verify_ollama_runtime(app.clone())) {
            warn!(
                "AI refinement runtime verify after re-enable failed: {}",
                error
            );
            return;
        }

        // verify_ollama_runtime success path already sets ollama_ready=true.
        // Do not re-read startup_status here: a concurrent start_ollama_runtime
        // invocation could briefly reset the flag and cause us to skip warmup
        // even though Ollama is reachable.
        let state = app.state::<AppState>();
        let latest_settings = {
            let snapshot = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            snapshot
        };
        if !should_autostart_ai_refinement_runtime(&latest_settings) {
            return;
        }

        match warmup_ai_refinement_model_once(&app, &latest_settings) {
            Ok(()) => info!("AI refinement warmup completed after module re-enable"),
            Err(error) => warn!(
                "AI refinement warmup failed after module re-enable (non-fatal): {}",
                error
            ),
        }
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PiperDaemonLifecycleAction {
    PrewarmPrimary,
    Shutdown,
}

fn piper_daemon_lifecycle_action(
    voice_settings: &crate::modules::VoiceOutputSettings,
) -> PiperDaemonLifecycleAction {
    if voice_settings.enabled && voice_settings.default_provider == "local_custom" {
        PiperDaemonLifecycleAction::PrewarmPrimary
    } else {
        PiperDaemonLifecycleAction::Shutdown
    }
}

pub(crate) fn schedule_piper_daemon_reconcile(
    app: AppHandle,
    voice_settings: crate::modules::VoiceOutputSettings,
    trigger: &'static str,
) {
    crate::util::spawn_guarded("piper_daemon_reconcile", move || {
        let state = app.state::<AppState>();
        match piper_daemon_lifecycle_action(&voice_settings) {
            PiperDaemonLifecycleAction::Shutdown => {
                crate::multimodal_io::shutdown_piper_daemon(state.inner());
            }
            PiperDaemonLifecycleAction::PrewarmPrimary => {
                let rate = voice_settings.rate.clamp(0.5, 2.0);
                match crate::multimodal_io::prewarm_piper_daemon(
                    state.inner(),
                    &voice_settings.piper_binary_path,
                    &voice_settings.piper_model_path,
                    rate,
                ) {
                    Ok(()) => info!(
                        "[piper-daemon] prewarm complete trigger={} rate={:.3}",
                        trigger, rate
                    ),
                    Err(error) => warn!(
                        "[piper-daemon] prewarm failed trigger={} rate={:.3}: {}",
                        trigger, rate, error
                    ),
                }
            }
        }
    });
}

#[tauri::command]
fn enable_module(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
    grant_permissions: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    guarded_command!("enable_module", {
        crate::modules::lifecycle_coordinator::enable_module_actions(
            &app,
            state.inner(),
            module_id,
            grant_permissions,
        )
    })
}

#[tauri::command]
fn disable_module(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
) -> Result<serde_json::Value, String> {
    guarded_command!("disable_module", {
        crate::modules::lifecycle_coordinator::disable_module_actions(
            &app,
            state.inner(),
            module_id,
        )
    })
}

#[tauri::command]
fn get_module_health(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: Option<String>,
) -> Vec<crate::modules::ModuleHealthStatus> {
    let installed_package_ids = scan_installed_module_ids_lossy(&app);
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    module_health::get_health_with_packages(
        &settings.module_settings,
        module_id.as_deref(),
        &installed_package_ids,
    )
}

#[tauri::command]
fn check_module_updates(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: Option<String>,
) -> Vec<crate::modules::ModuleUpdateInfo> {
    let installed_package_ids = scan_installed_module_ids_lossy(&app);
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let updates = module_health::check_updates_with_packages(
        &settings.module_settings,
        module_id.as_deref(),
        &installed_package_ids,
    );
    for update in &updates {
        let _ = app.emit("module:update-available", update);
    }
    updates
}

fn module_enabled(settings: &Settings, module_id: &str) -> bool {
    let module_id = canonicalize_module_id(module_id);
    settings
        .module_settings
        .enabled_modules
        .iter()
        .any(|enabled| canonicalize_module_id(enabled) == module_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeCapability {
    AiRefinement,
    TaskCapture,
    WorkflowAgent,
    VisionInput,
    VoiceOutputTts,
}

impl RuntimeCapability {
    pub(crate) fn module_id(self) -> &'static str {
        match self {
            Self::AiRefinement => AI_REFINEMENT_MODULE_ID,
            Self::TaskCapture => TASK_CAPTURE_MODULE_ID,
            Self::WorkflowAgent => ASSISTANT_CORE_MODULE_ID,
            Self::VisionInput => "input_vision",
            Self::VoiceOutputTts => "output_voice_tts",
        }
    }

    fn setting_enabled(self, settings: &Settings) -> bool {
        match self {
            Self::AiRefinement => settings.ai_fallback.enabled,
            Self::TaskCapture => true,
            Self::WorkflowAgent => settings.workflow_agent.enabled,
            Self::VisionInput => settings.vision_input_settings.enabled,
            Self::VoiceOutputTts => settings.voice_output_settings.enabled,
        }
    }

    fn module_disabled_message(self) -> &'static str {
        match self {
            Self::AiRefinement => {
                "AI Refinement module is disabled. Enable module 'ai_refinement' first."
            }
            Self::TaskCapture => {
                "Task Capture module is disabled. Enable module 'task_capture' first."
            }
            Self::WorkflowAgent => {
                "Assistant Core module is disabled. Enable module 'assistant_core' first."
            }
            Self::VisionInput => {
                "Vision input module is disabled. Enable module 'input_vision' first."
            }
            Self::VoiceOutputTts => {
                "Voice output module is disabled. Enable module 'output_voice_tts' first."
            }
        }
    }

    fn setting_disabled_message(self) -> &'static str {
        match self {
            Self::AiRefinement => "AI refinement is disabled in settings.",
            Self::TaskCapture => "Task Capture is disabled in settings.",
            Self::WorkflowAgent => "Assistant Core is disabled in settings.",
            Self::VisionInput => "Vision input is disabled in settings.",
            Self::VoiceOutputTts => "Voice output is disabled in settings.",
        }
    }
}

pub(crate) fn capability_enabled(settings: &Settings, capability: RuntimeCapability) -> bool {
    module_enabled(settings, capability.module_id()) && capability.setting_enabled(settings)
}

pub(crate) fn require_capability_enabled(
    settings: &Settings,
    capability: RuntimeCapability,
) -> Result<(), String> {
    if !module_enabled(settings, capability.module_id()) {
        return Err(capability.module_disabled_message().to_string());
    }
    if !capability.setting_enabled(settings) {
        return Err(capability.setting_disabled_message().to_string());
    }
    Ok(())
}

#[cfg(test)]
mod runtime_capability_gate_tests {
    use super::{capability_enabled, require_capability_enabled, RuntimeCapability};
    use crate::state::Settings;

    fn settings_for_capability(
        capability: RuntimeCapability,
        module_enabled: bool,
        setting_enabled: bool,
    ) -> Settings {
        let mut settings = Settings::default();
        if module_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(capability.module_id().to_string());
        }
        match capability {
            RuntimeCapability::AiRefinement => settings.ai_fallback.enabled = setting_enabled,
            RuntimeCapability::TaskCapture => {}
            RuntimeCapability::WorkflowAgent => settings.workflow_agent.enabled = setting_enabled,
            RuntimeCapability::VisionInput => {
                settings.vision_input_settings.enabled = setting_enabled
            }
            RuntimeCapability::VoiceOutputTts => {
                settings.voice_output_settings.enabled = setting_enabled
            }
        }
        settings
    }

    #[test]
    fn capability_enabled_requires_module_and_setting_flag() {
        let refinement_enabled =
            settings_for_capability(RuntimeCapability::AiRefinement, true, true);
        assert!(capability_enabled(
            &refinement_enabled,
            RuntimeCapability::AiRefinement
        ));

        let refinement_missing_module =
            settings_for_capability(RuntimeCapability::AiRefinement, false, true);
        assert!(!capability_enabled(
            &refinement_missing_module,
            RuntimeCapability::AiRefinement
        ));

        let refinement_missing_setting =
            settings_for_capability(RuntimeCapability::AiRefinement, true, false);
        assert!(!capability_enabled(
            &refinement_missing_setting,
            RuntimeCapability::AiRefinement
        ));

        let both_enabled = settings_for_capability(RuntimeCapability::VisionInput, true, true);
        assert!(capability_enabled(
            &both_enabled,
            RuntimeCapability::VisionInput
        ));

        let missing_module = settings_for_capability(RuntimeCapability::VisionInput, false, true);
        assert!(!capability_enabled(
            &missing_module,
            RuntimeCapability::VisionInput
        ));

        let missing_setting = settings_for_capability(RuntimeCapability::VisionInput, true, false);
        assert!(!capability_enabled(
            &missing_setting,
            RuntimeCapability::VisionInput
        ));
    }

    #[test]
    fn require_capability_reports_module_and_setting_failures() {
        let ai_module_disabled =
            settings_for_capability(RuntimeCapability::AiRefinement, false, true);
        let ai_module_error =
            require_capability_enabled(&ai_module_disabled, RuntimeCapability::AiRefinement)
                .unwrap_err();
        assert_eq!(
            ai_module_error,
            "AI Refinement module is disabled. Enable module 'ai_refinement' first."
        );

        let ai_setting_disabled =
            settings_for_capability(RuntimeCapability::AiRefinement, true, false);
        let ai_setting_error =
            require_capability_enabled(&ai_setting_disabled, RuntimeCapability::AiRefinement)
                .unwrap_err();
        assert_eq!(ai_setting_error, "AI refinement is disabled in settings.");

        let module_disabled =
            settings_for_capability(RuntimeCapability::VoiceOutputTts, false, true);
        let module_error =
            require_capability_enabled(&module_disabled, RuntimeCapability::VoiceOutputTts)
                .unwrap_err();
        assert_eq!(
            module_error,
            "Voice output module is disabled. Enable module 'output_voice_tts' first."
        );

        let setting_disabled =
            settings_for_capability(RuntimeCapability::VoiceOutputTts, true, false);
        let setting_error =
            require_capability_enabled(&setting_disabled, RuntimeCapability::VoiceOutputTts)
                .unwrap_err();
        assert_eq!(setting_error, "Voice output is disabled in settings.");
    }
}

#[tauri::command]
fn save_window_visibility_state(app: AppHandle, visibility: String) {
    // "normal" or "minimized" from frontend; "tray" is set by hide_main_window
    if ["normal", "minimized"].contains(&visibility.as_str()) {
        save_window_visibility(&app, &visibility);
    }
}

#[tauri::command]
fn save_window_state(
    app: AppHandle,
    state: State<'_, AppState>,
    window_label: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    // Validate window state: reject if window is minimized or has invalid dimensions
    // Windows uses ~-32000 for minimized window positions
    const MINIMIZED_THRESHOLD: i32 = -30000;

    if x < MINIMIZED_THRESHOLD || y < MINIMIZED_THRESHOLD {
        // Window is minimized, don't save this state
        return Ok(());
    }

    // Reject if dimensions are too small (below minimum from tauri.conf.json)
    if width < 980 || height < 640 {
        return Ok(());
    }

    let monitor_name = if let Some(window) = app.get_webview_window(&window_label) {
        window
            .current_monitor()
            .ok()
            .flatten()
            .and_then(|m| m.name().map(|n| n.clone()))
    } else {
        None
    };

    let mut current = state
        .settings
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    match window_label.as_str() {
        "main" => {
            current.main_window_x = Some(x);
            current.main_window_y = Some(y);
            current.main_window_width = Some(width);
            current.main_window_height = Some(height);
            current.main_window_monitor = monitor_name;
        }
        _ => return Err("Unknown window label".to_string()),
    }

    // Debounce: skip disk write if less than 500ms since last geometry save.
    // The in-memory settings are always updated above so the latest geometry is
    // available for other code paths even when the disk write is skipped.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let last = LAST_GEOMETRY_SAVE_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 500 {
        return Ok(());
    }
    LAST_GEOMETRY_SAVE_MS.store(now, Ordering::Relaxed);

    let settings = current.clone();
    drop(current);

    let _ = save_settings_file(&app, &settings);
    Ok(())
}

#[tauri::command]
fn show_assistant_presence_window(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let snapshot = {
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings.assistant_presence_enabled = true;
        normalize_module_settings(&mut settings.module_settings);
        normalize_workflow_agent_settings(&mut settings.workflow_agent);
        normalize_assistant_presence_binding(&mut settings);
        settings.clone()
    };
    save_settings_file(&app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot.clone());
    assistant_presence::reconcile_assistant_presence_window(&app, &snapshot);
    Ok(())
}

#[tauri::command]
fn toggle_transcribe(app: AppHandle) -> Result<(), String> {
    toggle_transcribe_state(&app);
    Ok(())
}

#[tauri::command]
fn expand_transcribe_backlog(
    app: AppHandle,
) -> Result<transcription::TranscribeBacklogStatus, String> {
    cancel_backlog_auto_expand(&app);
    expand_transcribe_backlog_inner(&app)
}

#[tauri::command]
fn paste_transcript_text(app: AppHandle, text: String) -> Result<(), String> {
    paste_text(&app, &text)
}

#[tauri::command]
async fn apply_model(app: AppHandle, model_id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old_model = settings.model.clone();
        settings.model = model_id.clone();
        drop(settings);

        // Save the new model setting
        save_settings_file(
            &app,
            &state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )?;

        // If transcription is active or Whisper server is running, restart with new model
        // to clear old model from VRAM and load new model
        if state.transcribe_active.load(Ordering::Relaxed) {
            stop_transcribe_monitor_and_release_whisper(&app, &state);
            let new_settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if let Err(err) = start_transcribe_monitor(&app, &state, &new_settings) {
                // Restore old model if restart fails
                let mut settings = state
                    .settings
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                settings.model = old_model;
                drop(settings);
                let _ = save_settings_file(
                    &app,
                    &state
                        .settings
                        .read()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()),
                );
                state.transcribe_active.store(false, Ordering::Relaxed);
                return Err(format!("Failed to apply model: {}", err));
            }
        } else {
            // Even if transcription is inactive, restart Whisper server if it's running
            // to clear old model from VRAM and load new model
            if let Some(new_model_path) = crate::models::resolve_model_path(&app, &model_id) {
                let _ = crate::whisper_server::restart_whisper_server_if_running(
                    &app,
                    &state,
                    &new_model_path,
                );
            }
        }

        refresh_startup_status(&app, state.inner());
        refresh_runtime_diagnostics(&app, state.inner());
        let _ = app.emit("model:changed", model_id);
        Ok(())
    })
    .await
    .unwrap_or_else(|e| Err(format!("apply_model panicked: {e}")))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HardwareInfo {
    pub gpu_name: String,
    pub gpu_vram: String,
    pub backend_recommended: String, // "cuda" | "vulkan" | "cpu"
    pub cuda_available: bool,
    pub driver_version: String,
    pub update_url: Option<String>,
}

#[tauri::command]
fn get_hardware_info() -> Result<HardwareInfo, String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

        let mut gpu_name = "Unknown".to_string();
        let mut cuda_available = false;
        let mut driver_version = "Unknown".to_string();
        let mut update_url = None;

        // 1. Detect GPU via DXGI
        if let Ok(factory) = unsafe { CreateDXGIFactory1::<IDXGIFactory1>() } {
            let mut adapter_index = 0;
            while let Ok(adapter) = unsafe { factory.EnumAdapters1(adapter_index) } {
                if let Ok(desc) = unsafe { adapter.GetDesc1() } {
                    let name = String::from_utf16_lossy(&desc.Description);
                    let name = name.trim_matches(char::from(0)).trim().to_string();

                    // Prioritize dedicated GPUs (especially NVIDIA)
                    if name.to_lowercase().contains("nvidia") {
                        gpu_name = name;
                        break;
                    } else if gpu_name == "Unknown"
                        || gpu_name.to_lowercase().contains("intel")
                        || gpu_name.to_lowercase().contains("microsoft")
                    {
                        gpu_name = name;
                    }
                }
                adapter_index += 1;
            }
        }

        // 2. Check for CUDA readiness
        // We look for the NVIDIA compiler/runtime DLL which is a good indicator of driver support.
        let cuda_dlls = ["nvrtc64_120_0.dll", "nvrtc64_112_0.dll", "nvcuda.dll"];
        for dll in cuda_dlls {
            if unsafe {
                windows::Win32::System::LibraryLoader::GetModuleHandleA(windows::core::PCSTR(
                    format!("{}\0", dll).as_ptr(),
                ))
                .is_ok()
            } {
                cuda_available = true;
                break;
            }
            // Also check on disk if not loaded
            if which::which(dll).is_ok() {
                cuda_available = true;
                break;
            }
        }

        // Manual check in System32 for nvcuda.dll
        if !cuda_available {
            let sys32_nvcuda = std::path::PathBuf::from("C:\\Windows\\System32\\nvcuda.dll");
            if sys32_nvcuda.exists() {
                cuda_available = true;
            }
        }

        // 3. Get VRAM and Driver Version via nvidia-smi if available
        let mut gpu_vram = "Unknown".to_string();
        if gpu_name.to_lowercase().contains("nvidia") {
            use std::process::Command;
            let mut cmd = Command::new("nvidia-smi");
            cmd.args(&[
                "--query-gpu=memory.total,driver_version",
                "--format=csv,noheader,nounits",
            ]);

            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NO_WINDOW: u32 = 0x08000000;
                cmd.creation_flags(CREATE_NO_WINDOW);
            }

            if let Ok(output) = cmd.output() {
                if output.status.success() {
                    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let parts: Vec<&str> = result.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let Ok(total_mb) = parts[0].parse::<f64>() {
                            gpu_vram = format!("{:.1} GB", total_mb / 1024.0);
                        }
                        driver_version = parts[1].to_string();
                    }
                }
            }

            if driver_version == "Unknown" || driver_version.is_empty() {
                update_url = Some("https://www.nvidia.com/Download/index.aspx".to_string());
            }
        }

        let backend_recommended = if cuda_available {
            "cuda".to_string()
        } else if gpu_name.to_lowercase().contains("amd")
            || gpu_name.to_lowercase().contains("intel")
        {
            "vulkan".to_string()
        } else {
            "cpu".to_string()
        };

        Ok(HardwareInfo {
            gpu_name,
            gpu_vram,
            backend_recommended,
            cuda_available,
            driver_version,
            update_url,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        Ok(HardwareInfo {
            gpu_name: "Generic".to_string(),
            gpu_vram: "Unknown".to_string(),
            backend_recommended: "cpu".to_string(),
            cuda_available: false,
            driver_version: "N/A".to_string(),
            update_url: None,
        })
    }
}

#[tauri::command]
async fn get_gpu_vram_usage() -> Result<String, String> {
    // Query NVIDIA GPU VRAM usage via nvidia-smi — wrapped in spawn_blocking to
    // avoid blocking the Tokio worker thread during the nvidia-smi process spawn.
    tauri::async_runtime::spawn_blocking(|| {
        use std::process::Command;

        let mut cmd = Command::new("nvidia-smi");
        cmd.args(&[
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ]);

        // On Windows, hide the command window to prevent visual pop-ups
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let output = cmd
            .output()
            .map_err(|_| "nvidia-smi not found".to_string())?;

        if !output.status.success() {
            return Ok(String::new());
        }

        let result = String::from_utf8(output.stdout)
            .unwrap_or_default()
            .trim()
            .to_string();

        let parts: Vec<&str> = result.split(',').map(|s| s.trim()).collect();
        if parts.len() == 2 {
            if let (Ok(used_mb), Ok(total_mb)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>())
            {
                let used_gb = used_mb / 1024.0;
                let total_gb = total_mb / 1024.0;
                return Ok(format!("{:.1} GB / {:.1} GB", used_gb, total_gb));
            }
        }

        Ok(String::new())
    })
    .await
    .unwrap_or_else(|_| Ok(String::new()))
}

#[derive(Debug, Clone, serde::Serialize)]
struct DependencyPreflightItem {
    id: String,
    status: String, // "ok" | "warning" | "error"
    required: bool,
    message: String,
    hint: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct DependencyPreflightReport {
    generated_at_ms: u64,
    overall_status: String, // "ok" | "warning" | "error"
    blocking_count: usize,
    warning_count: usize,
    items: Vec<DependencyPreflightItem>,
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

#[cfg(not(target_os = "windows"))]
fn check_powershell_available() -> bool {
    false
}

fn build_dependency_preflight_report(
    app: &AppHandle,
    state: &AppState,
) -> DependencyPreflightReport {
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let mut items: Vec<DependencyPreflightItem> = Vec::new();

    let whisper_cli = paths::resolve_whisper_cli_path_for_backend(Some(
        settings_snapshot.local_backend_preference.as_str(),
    ));
    if let Some(path) = whisper_cli {
        if let Some(issue) = crate::transcription::whisper_runtime_preflight_issue(path.as_path()) {
            let selected_backend =
                crate::transcription::whisper_backend_from_cli_path(path.as_path());
            let vulkan_ready = paths::resolve_whisper_cli_path_for_backend(Some("vulkan"))
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

    let module_descriptors =
        module_registry::modules_as_descriptors(&settings_snapshot.module_settings);
    for descriptor in module_descriptors
        .iter()
        .filter(|module| module.state == "error")
    {
        let message = descriptor
            .last_error
            .clone()
            .unwrap_or_else(|| "Module is in error state.".to_string());
        items.push(DependencyPreflightItem {
            id: format!("module_{}", descriptor.id),
            status: "warning".to_string(),
            required: false,
            message: format!("Module '{}' has an issue: {}", descriptor.name, message),
            hint: Some("Open Modules tab and run Health / dependency checks.".to_string()),
        });
    }

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

#[tauri::command]
async fn get_dependency_preflight_status(app: AppHandle) -> DependencyPreflightReport {
    // Wrapped in spawn_blocking: check_powershell_available() spawns powershell.exe
    // which blocks for 1-5s; running that on a Tokio worker thread would starve IPC.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        build_dependency_preflight_report(&app, state.inner())
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

pub(crate) fn save_recording_opus(
    app: &AppHandle,
    samples: &[i16],
    source: &str,
    session_name: Option<&str>,
) -> Result<String, String> {
    // Generate human-readable filename
    let now = chrono::Local::now();
    let duration_s = samples.len() as f64 / 16000.0; // 16kHz sample rate
    let duration_label = if duration_s < 60.0 {
        format!("{}s", duration_s.round() as u32)
    } else {
        let mins = (duration_s / 60.0).floor() as u32;
        let secs = (duration_s % 60.0).round() as u32;
        if secs > 0 {
            format!("{}m{}s", mins, secs)
        } else {
            format!("{}m", mins)
        }
    };

    // Build base filename
    let prefix = match source {
        "mixed" => "call",
        "output" => "system",
        _ => "mic",
    };

    let base_filename = if let Some(name) = session_name {
        // User-provided name: call_TeamStandup_20260215_1430_15m
        let sanitized = sanitize_session_name(name);
        let date = now.format("%Y%m%d").to_string();
        let time = now.format("%H%M").to_string();
        format!(
            "{}_{}_{}_{}_{}",
            prefix, sanitized, date, time, duration_label
        )
    } else {
        // Fallback: Compact timestamp ID: call_0215T1430_15m
        let timestamp_id = now.format("%m%dT%H%M").to_string();
        format!("{}_{}_{}", prefix, timestamp_id, duration_label)
    };

    // Save to app data dir: ~/.local/share/trispr-flow/recordings/
    let data_dir = crate::paths::resolve_base_dir(&app);
    let recordings_dir = data_dir.join("recordings");

    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings dir: {}", e))?;

    let wav_filename = format!("{}.wav", base_filename);
    let wav_path = recordings_dir.join(&wav_filename);

    // Write WAV file
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut wav_writer = hound::WavWriter::create(&wav_path, spec)
        .map_err(|e| format!("Failed to create WAV file: {}", e))?;

    for &sample in samples {
        wav_writer
            .write_sample(sample)
            .map_err(|e| format!("Failed to write WAV sample: {}", e))?;
    }

    wav_writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

    // Convert WAV to OPUS
    let opus_filename = format!("{}.opus", base_filename);
    let opus_path = recordings_dir.join(&opus_filename);

    opus::encode_wav_to_opus_default(&wav_path, &opus_path)
        .map_err(|e| format!("Failed to encode OPUS: {}", e))?;

    // Delete WAV file (we only need OPUS)
    let _ = std::fs::remove_file(&wav_path);

    Ok(opus_path.to_string_lossy().to_string())
}

fn sanitize_session_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    let trimmed = sanitized.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "session".to_string()
    } else if trimmed.len() > 40 {
        trimmed[..40].to_string()
    } else {
        trimmed
    }
}

/// Start or stop the LM Studio daemon via its CLI (`lms daemon up|stop`).
/// True fire-and-forget: spawns the process and detaches immediately.
/// The daemon runs independently — we never wait for it.
pub(crate) fn lms_daemon_command(action: &str) {
    use std::process::{Command, Stdio};

    let candidates = [
        "lms".to_string(),
        format!(
            "{}\\LM Studio\\lms.exe",
            std::env::var("LOCALAPPDATA").unwrap_or_default()
        ),
    ];
    for bin in &candidates {
        let mut cmd = Command::new(bin);
        cmd.args(["daemon", action])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        // Suppress console window on Windows
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.spawn() {
            Ok(child) => {
                info!(
                    "lms daemon {} spawned (pid {:?}) — detached",
                    action,
                    child.id()
                );
                // Do NOT wait — the daemon runs independently
                return;
            }
            Err(_) => continue,
        }
    }
    warn!("lms CLI not found — cannot run 'lms daemon {}'", action);
}

/// Load a model into LM Studio via `lms load <identifier>`.
/// This is a one-shot command that blocks until complete — call from a
/// background thread only.
fn lms_load_model(model_identifier: &str) {
    use std::process::{Command, Stdio};

    let candidates = [
        "lms".to_string(),
        format!(
            "{}\\LM Studio\\lms.exe",
            std::env::var("LOCALAPPDATA").unwrap_or_default()
        ),
    ];
    for bin in &candidates {
        let mut cmd = Command::new(bin);
        cmd.args(["load", model_identifier])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.spawn() {
            Ok(mut child) => {
                info!(
                    "lms load '{}' spawned (pid {:?})",
                    model_identifier,
                    child.id()
                );
                match child.wait() {
                    Ok(status) => info!("lms load '{}' exited: {}", model_identifier, status),
                    Err(e) => warn!("lms load '{}' wait failed: {}", model_identifier, e),
                }
                return;
            }
            Err(_) => continue,
        }
    }
    warn!(
        "lms CLI not found — cannot run 'lms load {}'",
        model_identifier
    );
}

fn build_overlay_settings(settings: &Settings) -> overlay::OverlaySettings {
    let use_kitt = settings.overlay_style == "kitt";
    let (color, rise_ms, fall_ms, opacity_inactive, opacity_active, pos_x, pos_y) = if use_kitt {
        (
            settings.overlay_kitt_color.clone(),
            settings.overlay_kitt_rise_ms,
            settings.overlay_kitt_fall_ms,
            settings.overlay_kitt_opacity_inactive,
            settings.overlay_kitt_opacity_active,
            settings.overlay_kitt_pos_x,
            settings.overlay_kitt_pos_y,
        )
    } else {
        (
            settings.overlay_color.clone(),
            settings.overlay_rise_ms,
            settings.overlay_fall_ms,
            settings.overlay_opacity_inactive,
            settings.overlay_opacity_active,
            settings.overlay_pos_x,
            settings.overlay_pos_y,
        )
    };
    overlay::OverlaySettings {
        color,
        min_radius: settings.overlay_min_radius as f64,
        max_radius: settings.overlay_max_radius as f64,
        rise_ms,
        fall_ms,
        opacity_inactive: opacity_inactive as f64,
        opacity_active: opacity_active as f64,
        pos_x,
        pos_y,
        style: settings.overlay_style.clone(),
        refining_indicator_enabled: settings.overlay_refining_indicator_enabled,
        refining_indicator_preset: settings.overlay_refining_indicator_preset.clone(),
        refining_indicator_color: settings.overlay_refining_indicator_color.clone(),
        refining_indicator_speed_ms: settings.overlay_refining_indicator_speed_ms,
        refining_indicator_range: settings.overlay_refining_indicator_range as f64,
        tts_stop_enabled: settings.overlay_tts_stop_enabled,
        tts_stop_shape: settings.overlay_tts_stop_shape.clone(),
        tts_stop_color: settings.overlay_tts_stop_color.clone(),
        kitt_min_width: settings.overlay_kitt_min_width as f64,
        kitt_max_width: settings.overlay_kitt_max_width as f64,
        kitt_height: settings.overlay_kitt_height as f64,
    }
}

fn init_logging() {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{
        filter::LevelFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Logs go to %LOCALAPPDATA%\Trispr Flow\logs\:
    //   - trispr-flow.YYYY-MM-DD.txt         (all levels, daily rotation, 30-day retention)
    //   - trispr-flow-errors.YYYY-MM-DD.txt  (WARN+ERROR only — compact scan surface)
    let log_dir = std::env::var("LOCALAPPDATA")
        .map(|d| std::path::PathBuf::from(d).join("Trispr Flow").join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from("logs"));
    let _ = std::fs::create_dir_all(&log_dir);

    let main_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("trispr-flow")
        .filename_suffix("txt")
        .max_log_files(30)
        .build(&log_dir)
        .expect("failed to initialize main log appender");
    let (main_nb, main_guard) = tracing_appender::non_blocking(main_appender);
    std::mem::forget(main_guard);

    let errors_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("trispr-flow-errors")
        .filename_suffix("txt")
        .max_log_files(30)
        .build(&log_dir)
        .expect("failed to initialize errors log appender");
    let (errors_nb, errors_guard) = tracing_appender::non_blocking(errors_appender);
    std::mem::forget(errors_guard);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(main_nb)
                .with_ansi(false),
        )
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(errors_nb)
                .with_ansi(false)
                .with_filter(LevelFilter::WARN),
        )
        .init();

    info!("Trispr Flow starting up — log: {}", log_dir.display());
}

pub(crate) fn emit_error(app: &AppHandle, error: AppError, context: Option<&str>) {
    let event = if let Some(ctx) = context {
        ErrorEvent::new(error.clone()).with_context(ctx)
    } else {
        ErrorEvent::new(error.clone())
    };

    error!("{}: {}", error.title(), error.message());

    let _ = app.emit("app:error", event);
}

fn load_local_env() {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let parent = cwd.parent().map(|p| p.to_path_buf());
    let grandparent = parent
        .as_ref()
        .and_then(|p| p.parent().map(|gp| gp.to_path_buf()));
    let mut candidates = vec![cwd.join(".env.local"), cwd.join(".env")];
    if let Some(parent) = parent {
        candidates.push(parent.join(".env.local"));
        candidates.push(parent.join(".env"));
    }
    if let Some(grandparent) = grandparent {
        candidates.push(grandparent.join(".env.local"));
        candidates.push(grandparent.join(".env"));
    }

    for path in candidates {
        if !path.exists() {
            continue;
        }
        if let Ok(raw) = fs::read_to_string(&path) {
            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.splitn(2, '=');
                let key = parts.next().unwrap_or("").trim();
                let value = parts.next().unwrap_or("").trim();
                if key.is_empty() || value.is_empty() {
                    continue;
                }
                if std::env::var(key).is_err() {
                    std::env::set_var(key, value);
                }
            }
        }
    }
}

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

/// Snapshot of clipboard content before we overwrite it.
enum ClipboardSnapshot {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
    Empty,
}

fn capture_clipboard_snapshot_with_retry() -> ClipboardSnapshot {
    let deadline = std::time::Instant::now() + Duration::from_millis(CLIPBOARD_CAPTURE_TIMEOUT_MS);

    loop {
        match Clipboard::new() {
            Ok(mut clipboard) => {
                if let Ok(text) = clipboard.get_text() {
                    return ClipboardSnapshot::Text(text);
                }

                if let Ok(image) = clipboard.get_image() {
                    return ClipboardSnapshot::Image {
                        width: image.width,
                        height: image.height,
                        bytes: image.bytes.into_owned(),
                    };
                }

                return ClipboardSnapshot::Empty;
            }
            Err(err) => {
                if std::time::Instant::now() >= deadline {
                    let err = err.to_string();
                    warn!(
                        "Clipboard snapshot capture timed out after {} ms: {}",
                        CLIPBOARD_CAPTURE_TIMEOUT_MS, err
                    );
                    return ClipboardSnapshot::Empty;
                }
            }
        }

        thread::sleep(Duration::from_millis(CLIPBOARD_RETRY_INTERVAL_MS));
    }
}

fn clipboard_text_matches(expected: &str, current: &str) -> bool {
    if expected == current {
        return true;
    }

    // Windows clipboard conversions can normalize newlines to CRLF.
    expected.replace("\r\n", "\n") == current.replace("\r\n", "\n")
}

fn set_clipboard_text_with_retry(text: &str) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_millis(CLIPBOARD_CAPTURE_TIMEOUT_MS);
    let text = text.to_string();

    loop {
        let attempt_error = match Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(text.clone()) {
                Ok(()) => return Ok(()),
                Err(err) => err.to_string(),
            },
            Err(err) => err.to_string(),
        };

        if std::time::Instant::now() >= deadline {
            return Err(format!(
                "Failed to set clipboard text after {} ms: {}",
                CLIPBOARD_CAPTURE_TIMEOUT_MS, attempt_error
            ));
        }

        thread::sleep(Duration::from_millis(CLIPBOARD_RETRY_INTERVAL_MS));
    }
}

fn restore_snapshot_with_retry(snapshot: ClipboardSnapshot) -> Result<(), String> {
    if matches!(snapshot, ClipboardSnapshot::Empty) {
        return Ok(());
    }

    let deadline = std::time::Instant::now() + Duration::from_millis(CLIPBOARD_RESTORE_TIMEOUT_MS);

    loop {
        let attempt_error = match Clipboard::new() {
            Ok(mut clipboard) => {
                let write_result = match &snapshot {
                    ClipboardSnapshot::Text(text) => clipboard.set_text(text.clone()),
                    ClipboardSnapshot::Image {
                        width,
                        height,
                        bytes,
                    } => clipboard.set_image(ImageData {
                        width: *width,
                        height: *height,
                        bytes: std::borrow::Cow::Borrowed(bytes.as_slice()),
                    }),
                    ClipboardSnapshot::Empty => return Ok(()),
                };

                match write_result {
                    Ok(()) => {
                        if let ClipboardSnapshot::Text(expected) = &snapshot {
                            match clipboard.get_text() {
                                Ok(current) if clipboard_text_matches(expected, &current) => {
                                    return Ok(());
                                }
                                Ok(_) => "Clipboard text verification mismatch".to_string(),
                                Err(err) => format!("Clipboard text verification failed: {}", err),
                            }
                        } else {
                            return Ok(());
                        }
                    }
                    Err(err) => err.to_string(),
                }
            }
            Err(err) => err.to_string(),
        };

        if std::time::Instant::now() >= deadline {
            warn!(
                "Clipboard restore timed out after {} ms: {}",
                CLIPBOARD_RESTORE_TIMEOUT_MS, attempt_error
            );
            return Err(format!(
                "Failed to restore clipboard after {} ms: {}",
                CLIPBOARD_RESTORE_TIMEOUT_MS, attempt_error
            ));
        }

        thread::sleep(Duration::from_millis(CLIPBOARD_RETRY_INTERVAL_MS));
    }
}

pub(crate) fn paste_text(app_handle: &AppHandle, text: &str) -> Result<(), String> {
    let snapshot = capture_clipboard_snapshot_with_retry();
    set_clipboard_text_with_retry(text)?;
    {
        let ec_state = app_handle.state::<crate::state::AppState>();
        crate::uiautomation_capture::record_paste(&ec_state.enter_capture, text);
    }

    if let Err(paste_error) = send_paste_keystroke() {
        if let Err(restore_error) = restore_snapshot_with_retry(snapshot) {
            warn!(
                "Clipboard restore failed after paste keystroke error: {}",
                restore_error
            );
            return Err(format!(
                "Failed to send paste keystroke: {}. Clipboard restore also failed: {}",
                paste_error, restore_error
            ));
        }

        return Err(format!("Failed to send paste keystroke: {}", paste_error));
    }

    let operation_generation = CLIPBOARD_PASTE_GENERATION.fetch_add(1, Ordering::AcqRel) + 1;

    crate::util::spawn_guarded("clipboard_restore", move || {
        thread::sleep(Duration::from_millis(CLIPBOARD_RESTORE_DELAY_MS));

        // Newer paste operations supersede older restore attempts.
        if CLIPBOARD_PASTE_GENERATION.load(Ordering::Acquire) != operation_generation {
            return;
        }

        if let Err(err) = restore_snapshot_with_retry(snapshot) {
            warn!("Clipboard restore failed: {}", err);
        }
    });

    Ok(())
}

fn send_paste_keystroke() -> Result<(), String> {
    let mut enigo = Enigo::new();
    if cfg!(target_os = "macos") {
        enigo.key_down(Key::Meta);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Meta);
    } else {
        enigo.key_down(Key::Control);
        enigo.key_click(Key::Layout('v'));
        enigo.key_up(Key::Control);
    }
    Ok(())
}

fn try_load_tray_icon(icon_path: &std::path::Path) -> Option<tauri::image::Image<'static>> {
    use tauri::image::Image;

    match std::fs::read(icon_path) {
        Ok(png_data) => {
            match image::load_from_memory_with_format(&png_data, image::ImageFormat::Png) {
                Ok(img) => {
                    let rgba_img = img.to_rgba8();
                    let (width, height) = rgba_img.dimensions();
                    return Some(Image::new_owned(rgba_img.into_raw(), width, height));
                }
                Err(_) => {}
            }
        }
        Err(_) => {}
    }
    None
}

fn create_fallback_icon() -> tauri::image::Image<'static> {
    use tauri::image::Image;

    let mut pixels = vec![0u8; 64 * 64 * 4];
    for i in (0..pixels.len()).step_by(4) {
        pixels[i] = 40; // R
        pixels[i + 1] = 130; // G
        pixels[i + 2] = 140; // B
        pixels[i + 3] = 255; // A
    }

    Image::new_owned(pixels, 64, 64)
}

fn parse_tray_state_code(payload: &str) -> u8 {
    let value = serde_json::from_str::<String>(payload)
        .ok()
        .unwrap_or_else(|| payload.trim_matches('"').to_string());
    match value.as_str() {
        "recording" => 1,
        "transcribing" => 2,
        _ => 0,
    }
}

fn draw_circle_rgba(
    pixels: &mut [u8],
    size: usize,
    center_x: f32,
    center_y: f32,
    radius: f32,
    color: [u8; 4],
) {
    let radius_sq = radius * radius;
    let min_x = (center_x - radius).floor().max(0.0) as i32;
    let max_x = (center_x + radius).ceil().min((size - 1) as f32) as i32;
    let min_y = (center_y - radius).floor().max(0.0) as i32;
    let max_y = (center_y + radius).ceil().min((size - 1) as f32) as i32;

    let alpha = color[3] as f32 / 255.0;
    let inv_alpha = 1.0 - alpha;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = (x as f32 + 0.5) - center_x;
            let dy = (y as f32 + 0.5) - center_y;
            if dx * dx + dy * dy > radius_sq {
                continue;
            }
            let idx = (y as usize * size + x as usize) * 4;
            pixels[idx] = (pixels[idx] as f32 * inv_alpha + color[0] as f32 * alpha) as u8;
            pixels[idx + 1] = (pixels[idx + 1] as f32 * inv_alpha + color[1] as f32 * alpha) as u8;
            pixels[idx + 2] = (pixels[idx + 2] as f32 * inv_alpha + color[2] as f32 * alpha) as u8;
            let out_alpha = color[3] as f32 + (pixels[idx + 3] as f32 * inv_alpha);
            pixels[idx + 3] = out_alpha.min(255.0) as u8;
        }
    }
}

fn create_tray_pulse_icon(
    frame: usize,
    recording_active: bool,
    transcribe_active: bool,
) -> tauri::image::Image<'static> {
    use tauri::image::Image;

    let size = 32usize;
    let mut pixels = vec![0u8; size * size * 4];
    let frame_mod = frame % TRAY_PULSE_FRAMES;
    let angle = (frame_mod as f32 / TRAY_PULSE_FRAMES as f32) * std::f32::consts::TAU;
    let pulse = 0.5 + 0.5 * angle.sin();
    // Keep the brand-like two-circle silhouette: slight diagonal offset, low overlap.
    let rec_center_x = 10.0f32;
    let rec_center_y = 22.0f32;
    let trans_center_x = 22.0f32;
    let trans_center_y = 10.0f32;

    // +30% compared to the previous 7.6 radius.
    let rec_base = 9.9f32;
    let trans_base = 9.9f32;
    let rec_radius = if recording_active {
        rec_base + (pulse * 0.35)
    } else {
        rec_base
    };
    let trans_radius = if transcribe_active {
        trans_base + (pulse * 0.35)
    } else {
        trans_base
    };

    if recording_active {
        draw_circle_rgba(
            &mut pixels,
            size,
            rec_center_x,
            rec_center_y,
            rec_radius + 0.45,
            [29, 166, 160, 72],
        );
    }
    if transcribe_active {
        draw_circle_rgba(
            &mut pixels,
            size,
            trans_center_x,
            trans_center_y,
            trans_radius + 0.45,
            [245, 179, 66, 72],
        );
    }

    let rec_color = if recording_active {
        [29, 166, 160, 245]
    } else {
        [29, 166, 160, 185]
    };
    let trans_color = if transcribe_active {
        [245, 179, 66, 245]
    } else {
        [245, 179, 66, 185]
    };
    draw_circle_rgba(
        &mut pixels,
        size,
        rec_center_x,
        rec_center_y,
        rec_radius,
        rec_color,
    );
    draw_circle_rgba(
        &mut pixels,
        size,
        trans_center_x,
        trans_center_y,
        trans_radius,
        trans_color,
    );

    Image::new_owned(pixels, size as u32, size as u32)
}

fn refresh_tray_icon(app: &AppHandle, frame: usize) {
    let capture_state = TRAY_CAPTURE_STATE.load(Ordering::Relaxed);
    let transcribe_state = TRAY_TRANSCRIBE_STATE.load(Ordering::Relaxed);
    let recording_active = capture_state == 1;
    let transcribe_active = transcribe_state == 1 || transcribe_state == 2;
    let effective_frame = if recording_active || transcribe_active {
        frame
    } else {
        0
    };

    if let Some(tray) = app.tray_by_id(TRAY_ICON_ID) {
        let icon = create_tray_pulse_icon(effective_frame, recording_active, transcribe_active);
        let _ = tray.set_icon(Some(icon));
    }
}

fn start_tray_pulse_loop(app: AppHandle) {
    if TRAY_PULSE_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    crate::util::spawn_guarded("tray_pulse", move || {
        let frame_ms = (TRAY_PULSE_CYCLE_MS / TRAY_PULSE_FRAMES as u64).max(120);
        let mut frame = 0usize;
        let mut last_signature = (u8::MAX, u8::MAX, usize::MAX);

        loop {
            let capture_state = TRAY_CAPTURE_STATE.load(Ordering::Relaxed);
            let transcribe_state = TRAY_TRANSCRIBE_STATE.load(Ordering::Relaxed);
            let active = capture_state == 1 || transcribe_state == 1 || transcribe_state == 2;
            let effective_frame = if active { frame } else { 0 };
            let signature = (capture_state, transcribe_state, effective_frame);
            if signature != last_signature {
                refresh_tray_icon(&app, effective_frame);
                last_signature = signature;
            }

            thread::sleep(Duration::from_millis(frame_ms));
            if active {
                frame = (frame + 1) % TRAY_PULSE_FRAMES;
            } else {
                frame = 0;
            }
        }
    });
}

/// Validates window state to prevent restoring minimized or invalid window positions
fn is_valid_window_state(x: i32, y: i32, width: u32, height: u32) -> bool {
    const MINIMIZED_THRESHOLD: i32 = -30000;
    const MIN_WIDTH: u32 = 980;
    const MIN_HEIGHT: u32 = 640;

    // Reject minimized window positions (Windows uses ~-32000 for minimized)
    if x < MINIMIZED_THRESHOLD || y < MINIMIZED_THRESHOLD {
        return false;
    }

    // Reject dimensions smaller than minimum
    if width < MIN_WIDTH || height < MIN_HEIGHT {
        return false;
    }

    true
}

/// Restore saved window geometry (position + size), falling back to centering on
/// the primary monitor when the saved state is invalid or the target monitor has
/// been disconnected.
fn restore_window_geometry(window: &tauri::WebviewWindow, settings: &Settings) {
    if let (Some(x), Some(y), Some(w), Some(h)) = (
        settings.main_window_x,
        settings.main_window_y,
        settings.main_window_width,
        settings.main_window_height,
    ) {
        let state_valid = is_valid_window_state(x, y, w, h);

        let monitor_valid = window
            .available_monitors()
            .ok()
            .map(|monitors| {
                if let Some(monitor_name) = &settings.main_window_monitor {
                    monitors.iter().any(|m| {
                        m.name().as_ref().map(|n| n.as_str()) == Some(monitor_name.as_str())
                    })
                } else {
                    true // No specific monitor was saved, so any monitor is valid
                }
            })
            .unwrap_or(false);

        if state_valid && monitor_valid {
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
            let _ = window.set_size(tauri::PhysicalSize::new(w, h));
        } else {
            if let Ok(Some(primary)) = window.primary_monitor() {
                let primary_size = primary.size();
                let window_w = w.max(980);
                let window_h = h.max(640);
                let center_x = (primary_size.width as i32 - window_w as i32) / 2;
                let center_y = (primary_size.height as i32 - window_h as i32) / 2;
                let _ = window.set_position(tauri::PhysicalPosition::new(center_x, center_y));
                let _ = window.set_size(tauri::PhysicalSize::new(window_w, window_h));
            }
        }
    }
}

fn recreate_main_window_from_config(
    app: &AppHandle,
    reason: &str,
    should_show: bool,
) -> Result<(), String> {
    let app_config = app.config();
    let window_config = app_config
        .app
        .windows
        .iter()
        .find(|cfg| cfg.label == "main")
        .or_else(|| app_config.app.windows.first())
        .ok_or_else(|| "Main window configuration missing".to_string())?;

    let window = tauri::WebviewWindowBuilder::from_config(app, window_config)
        .map_err(|err| format!("Main window config build failed: {}", err))?
        .build()
        .map_err(|err| format!("Main window recreation failed: {}", err))?;

    let settings = load_settings(app);
    restore_window_geometry(&window, &settings);
    if should_show {
        let _ = window.show();
        let _ = window.set_skip_taskbar(false);
        let _ = window.set_focus();
    } else {
        let _ = window.hide();
        let _ = window.set_skip_taskbar(true);
    }
    info!(
        "Main webview window recreated after watchdog recovery ({})",
        reason
    );
    Ok(())
}

fn recover_main_window_webview(app: &AppHandle, reason: &str) -> Result<(), String> {
    let mut attempts: Vec<String> = Vec::new();
    let mut was_visible = load_settings(app).main_window_start_state != "tray";

    if let Some(window) = app.get_webview_window("main") {
        was_visible = window.is_visible().unwrap_or(true);
        match window.reload() {
            Ok(_) => {
                if was_visible {
                    let _ = window.set_focus();
                }
                return Ok(());
            }
            Err(err) => {
                attempts.push(format!("reload#1 failed: {}", err));
            }
        }

        std::thread::sleep(Duration::from_millis(250));
        match window.reload() {
            Ok(_) => {
                if was_visible {
                    let _ = window.set_focus();
                }
                return Ok(());
            }
            Err(err) => {
                attempts.push(format!("reload#2 failed: {}", err));
            }
        }

        match window.destroy() {
            Ok(_) => attempts.push("destroyed stale main window".to_string()),
            Err(err) => attempts.push(format!("destroy failed: {}", err)),
        }
        std::thread::sleep(Duration::from_millis(350));
    } else {
        attempts.push("main window handle missing".to_string());
    }

    recreate_main_window_from_config(app, reason, was_visible).map_err(|err| {
        attempts.push(err);
        format!(
            "Main webview recovery failed during {}: {}",
            reason,
            attempts.join("; ")
        )
    })
}

pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        // Restore window geometry on first show
        if !MAIN_WINDOW_RESTORED.swap(true, Ordering::AcqRel) {
            let settings = load_settings(app);
            restore_window_geometry(&window, &settings);
        }

        let _ = window.show();
        let _ = window.set_skip_taskbar(false);
        let _ = window.set_focus();
        save_window_visibility(app, "normal");
    }
}

fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
        let _ = window.set_skip_taskbar(true);
        save_window_visibility(app, "tray");
    }
}

/// Persist the window visibility state ("normal", "minimized", "tray") to settings
fn save_window_visibility(app: &AppHandle, visibility: &str) {
    let state = app.state::<AppState>();
    let mut settings = state
        .settings
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if settings.main_window_start_state != visibility {
        settings.main_window_start_state = visibility.to_string();
        let s = settings.clone();
        drop(settings);
        let _ = save_settings_file(app, &s);
    }
}

fn should_handle_tray_click() -> bool {
    let now = util::now_ms();
    let last = LAST_TRAY_CLICK_MS.load(Ordering::Relaxed);
    if now.saturating_sub(last) <= TRAY_CLICK_DEBOUNCE_MS {
        return false;
    }
    LAST_TRAY_CLICK_MS.store(now, Ordering::Relaxed);
    true
}

pub(crate) fn toggle_activation_words_async(app: AppHandle) {
    crate::util::spawn_guarded("toggle_activation", move || {
        let state = app.state::<AppState>();
        let new_enabled = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settings.activation_words_enabled = !settings.activation_words_enabled;
            let enabled = settings.activation_words_enabled;
            let _ = save_settings_file(&app, &settings);
            enabled
        };

        let cue = if new_enabled { "start" } else { "stop" };
        let _ = app.emit("audio:cue", cue);
        let _ = app.emit("settings:updated", {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            settings
        });
        info!("Activation words toggled to: {}", new_enabled);
    });
}

pub(crate) fn toggle_product_mode_async(app: AppHandle) {
    crate::util::spawn_guarded("toggle_product_mode", move || {
        let state = app.state::<AppState>();
        let toggled = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let prev_transcribe_enabled = settings.transcribe_enabled;
            let assistant_core_available =
                capability_enabled(&settings, RuntimeCapability::WorkflowAgent);
            if !assistant_core_available {
                normalize_product_mode_field(&mut settings);
                let snapshot = settings.clone();
                let _ = save_settings_file(&app, &settings);
                let _ = app.emit("settings-changed", snapshot.clone());
                assistant_presence::reconcile_assistant_presence_window(&app, &snapshot);
                let _ = workflow_agent::emit_assistant_baseline_state(
                    &app,
                    state.inner(),
                    &snapshot,
                    "hotkey_toggle_product_mode_blocked",
                );
                info!("Product mode toggle ignored because Assistant Core is unavailable.");
                return;
            }
            settings.product_mode = if settings
                .product_mode
                .trim()
                .eq_ignore_ascii_case("assistant")
            {
                "transcribe".to_string()
            } else {
                "assistant".to_string()
            };
            normalize_product_mode_field(&mut settings);
            reconcile_assistant_transcribe_flag(&mut settings);
            let next_mode = settings.product_mode.clone();
            let snapshot = settings.clone();
            let _ = save_settings_file(&app, &settings);
            (next_mode, snapshot, prev_transcribe_enabled)
        };
        let (next_mode, snapshot, prev_transcribe_enabled) = toggled;

        if snapshot.transcribe_enabled && !prev_transcribe_enabled {
            let _ = start_transcribe_monitor(&app, &state, &snapshot);
        } else if !snapshot.transcribe_enabled && prev_transcribe_enabled {
            stop_transcribe_monitor_and_release_whisper(&app, &state);
        }

        let _ = app.emit("settings-changed", snapshot.clone());
        let _ = app.emit("menu:update-transcribe", snapshot.transcribe_enabled);
        assistant_presence::reconcile_assistant_presence_window(&app, &snapshot);
        let _ = workflow_agent::emit_assistant_baseline_state(
            &app,
            state.inner(),
            &snapshot,
            "hotkey_toggle_product_mode",
        );
        let cue = if next_mode == "assistant" {
            "start"
        } else {
            "stop"
        };
        let _ = app.emit("audio:cue", cue);
        info!("Product mode toggled to: {}", next_mode);
    });
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let visible = window.is_visible().unwrap_or(true);
        if visible {
            hide_main_window(app);
        } else {
            show_main_window(app);
        }
    }
}

fn with_dialog_plugin(builder: tauri::Builder<Wry>) -> tauri::Builder<Wry> {
    #[cfg(test)]
    {
        builder
    }

    #[cfg(not(test))]
    {
        builder.plugin(tauri_plugin_dialog::init())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Extract a human-readable message from a `catch_unwind` panic payload.
pub(crate) fn format_panic_payload(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

pub fn run() {
    init_logging();
    load_local_env();

    // Global panic hook: log every panic (including from spawned threads) so
    // we have full tracing context for crashes instead of silent thread death.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = info
            .payload()
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| info.payload().downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "non-string panic".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        error!(
            "PANIC at {}: {}\nBacktrace:\n{}",
            location, payload, backtrace
        );
        default_hook(info);
    }));

    info!("Starting Trispr Flow application");
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            warn!("Second instance launch blocked: focusing existing Trispr Flow window.");
            show_main_window(app);
            let _ = app.emit("app:instance-activated", true);
        }));
    with_dialog_plugin(builder)
        .setup(|app| {
            // Cold-start buffer: suppress Ollama pings for the first 10 s so the
            // runtime has time to spawn and become reachable.  The frontend defers
            // its own Ollama init by the same amount (OLLAMA_DEFER_MS in main.ts).
            // Reuses the existing OLLAMA_DIAG_NEXT_MS backoff gate.
            OLLAMA_DIAG_NEXT_MS.store(crate::util::now_ms() + 10_000, Ordering::Relaxed);

            // Migrate data from legacy %APPDATA%\com.trispr.flow\ to
            // %LOCALAPPDATA%\Trispr Flow\ before any state is loaded.
            crate::data_migration::migrate_legacy_data(app.handle());

            // Kill any Ollama process left over from a previous crash or hard-kill.
            // Moved to a background thread: taskkill on Windows can block for 1–3 s,
            // which would delay window creation and allow the frontend to issue IPC
            // calls before setup() completes — a key source of startup deadlocks.
            {
                let handle = app.handle().clone();
                crate::util::spawn_guarded("kill_stale_ollama", move || {
                    crate::ollama_runtime::kill_stale_ollama_pid(&handle);
                });
            }

            let mut settings = load_settings(app.handle());
            reconcile_assistant_transcribe_flag(&mut settings);
            crate::state::sync_diagnostic_logging_enabled(&settings);

            // Compute partition base directories and legacy paths for migration.
            let app_data_dir = crate::paths::resolve_base_dir(app.handle());
            let mic_history_dir = app_data_dir.join("history").join("mic");
            let system_history_dir = app_data_dir.join("history").join("system");
            let legacy_mic_path = app_data_dir.join("history.json");
            let legacy_system_path = app_data_dir.join("history_transcribe.json");

            let (history, history_transcribe) = std::thread::scope(|s| {
                let mic = s.spawn(|| {
                    PartitionedHistory::load_or_migrate(mic_history_dir, Some(&legacy_mic_path))
                });
                let sys = s.spawn(|| {
                    PartitionedHistory::load_or_migrate(
                        system_history_dir,
                        Some(&legacy_system_path),
                    )
                });
                (
                    mic.join().expect("mic history load"),
                    sys.join().expect("system history load"),
                )
            });

            app.manage(AppState {
                settings: std::sync::RwLock::new(settings.clone()),
                history: Mutex::new(history),
                history_transcribe: Mutex::new(history_transcribe),
                recorder: Mutex::new(crate::audio::Recorder::new()),
                transcribe: Mutex::new(crate::transcription::TranscribeRecorder::new()),
                downloads: Mutex::new(HashSet::new()),
                ollama_pulls: Mutex::new(HashSet::new()),
                transcribe_active: AtomicBool::new(false),
                refinement_active_count: AtomicUsize::new(0),
                refinement_watchdog_generation: AtomicU64::new(0),
                refinement_last_change_ms: AtomicU64::new(0),
                refinement_last_success_ms: AtomicU64::new(0),
                refinement_last_success_model: Mutex::new(None),
                ollama_idle_release_generation: AtomicU64::new(0),
                runtime_start_attempts: AtomicU64::new(0),
                runtime_start_failures: AtomicU64::new(0),
                refinement_timeouts: AtomicU64::new(0),
                refinement_fallback_failed: AtomicU64::new(0),
                refinement_fallback_timed_out: AtomicU64::new(0),
                last_mic_recording_path: Mutex::new(None),
                last_system_recording_path: Mutex::new(None),
                managed_ollama_child: Mutex::new(None),
                managed_whisper_server_child: Mutex::new(None),
                whisper_server_port: AtomicU16::new(crate::whisper_server::WHISPER_SERVER_PORT),
                whisper_server_warmup_started: AtomicBool::new(false),
                ollama_model_warm: AtomicBool::new(false),
                ollama_warmup_in_progress: AtomicBool::new(false),
                whisper_server_warm_until_ms: AtomicU64::new(0),
                whisper_server_retire_generation: AtomicU64::new(0),
                vision_stream_running: AtomicBool::new(false),
                vision_stream_started_ms: AtomicU64::new(0),
                vision_stream_frame_seq: AtomicU64::new(0),
                vision_frame_buffer: Mutex::new(crate::multimodal_io::VisionFrameBuffer::default()),
                startup_status: Mutex::new(StartupStatus::default()),
                runtime_diagnostics: Mutex::new(RuntimeDiagnostics::default()),
                overlay_controller: Mutex::new(crate::overlay::OverlayController::default()),
                frontend_last_heartbeat_ms: AtomicU64::new(crate::util::now_ms()),
                frontend_watchdog_last_reload_ms: AtomicU64::new(0),
                frontend_watchdog_reload_count: AtomicU64::new(0),
                frontend_watchdog_state: Mutex::new(state::FrontendWatchdogState::default()),
                assistant_orchestrator: Mutex::new(state::AssistantOrchestratorStatus::default()),
                tts_speaking: AtomicBool::new(false),
                tts_session_counter: AtomicU64::new(0),
                tts_playback_control: Mutex::new(None),
                piper_daemon: crate::multimodal_io::PiperDaemonState::default(),
                enter_capture: crate::state::EnterCaptureState::default(),
                #[cfg(target_os = "windows")]
                system_cluster_buffer: Mutex::new(state::SystemClusterBuffer::default()),
                #[cfg(target_os = "windows")]
                managed_process_job: create_managed_process_job(),
            });

            crate::uiautomation_capture::start_hook_thread(app.handle().clone());

            {
                let state = app.state::<AppState>();
                let now = crate::util::now_ms();
                let mut ledger = load_frontend_restart_ledger(app.handle());
                prune_timestamps_window(
                    &mut ledger.timestamps_ms,
                    now,
                    FRONTEND_WATCHDOG_RESTART_WINDOW_MS,
                );
                save_frontend_restart_ledger(app.handle(), &ledger);
                let mut watchdog_state = state
                    .frontend_watchdog_state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                watchdog_state.restart_count = ledger.timestamps_ms.len() as u64;
                watchdog_state.restart_timestamps_ms =
                    ledger.timestamps_ms.iter().copied().collect();
            }

            configure_windows_local_dumps(app.handle());

            {
                let state = app.state::<AppState>();
                update_startup_status(app.handle(), state.inner(), |status| {
                    status.rules_ready = true;
                });
            }
            {
                let handle = app.handle().clone();
                crate::util::spawn_guarded("startup_diagnostics", move || {
                    let state = handle.state::<AppState>();
                    refresh_runtime_diagnostics(&handle, state.inner());
                });
            }

            // Eagerly start whisper-server in background so the first transcription
            // uses the fast HTTP path instead of the slow CLI cold-start (~50s → <1s).
            {
                let handle = app.handle().clone();
                crate::util::spawn_guarded("eager_whisper_server", move || {
                    let state = handle.state::<AppState>();
                    let model_id = {
                        let s = state.settings.read()
                            .unwrap_or_else(|p| p.into_inner());
                        s.model.clone()
                    };
                    if let Some(model_path) = crate::models::resolve_model_path(&handle, &model_id) {
                        match crate::whisper_server::start_whisper_server(&handle, state.inner(), &model_path) {
                            Ok(()) => info!("Eager whisper-server started successfully"),
                            Err(e) => warn!("Eager whisper-server start failed (CLI fallback available): {}", e),
                        }
                    } else {
                        warn!("Eager whisper-server skipped: model '{}' not found on disk", model_id);
                    }
                });
            }

            {
                let handle = app.handle().clone();
                crate::util::spawn_guarded("dependency_preflight", move || {
                    let state = handle.state::<AppState>();
                    let report = build_dependency_preflight_report(&handle, state.inner());
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
                    let _ = handle.emit("dependency:preflight", &report);
                });
            }

            if env_flag("TRISPR_RUN_LATENCY_BENCHMARK") {
                let app_handle = app.handle().clone();
                crate::util::spawn_guarded("latency_benchmark", move || {
                    let request = crate::tts_benchmark::latency_benchmark_request_from_env();
                    let result = {
                        let state = app_handle.state::<AppState>();
                        crate::tts_benchmark::run_latency_benchmark_inner(
                            &app_handle,
                            state.inner(),
                            &request,
                        )
                    };

                    match result {
                        Ok(report) => match crate::tts_benchmark::write_latency_benchmark_report(&report) {
                            Ok(path) => {
                                info!(
                                    "Latency benchmark complete: p50={}ms p95={}ms (report: {})",
                                    report.p50_ms,
                                    report.p95_ms,
                                    path.display()
                                );
                                if !report.slo_pass {
                                    warn!(
                                        "Latency benchmark SLO warning: p50={}ms (target {}), p95={}ms (target {})",
                                        report.p50_ms,
                                        report.slo_p50_ms,
                                        report.p95_ms,
                                        report.slo_p95_ms
                                    );
                                }
                            }
                            Err(err) => {
                                error!("Failed to write latency benchmark report: {}", err);
                            }
                        },
                        Err(err) => {
                            error!("Latency benchmark failed: {}", err);
                        }
                    }

                    if env_flag("TRISPR_RUN_LATENCY_BENCHMARK_EXIT") {
                        app_handle.exit(0);
                    }
                });
            }

            if env_flag("TRISPR_RUN_TTS_BENCHMARK") {
                let app_handle = app.handle().clone();
                crate::util::spawn_guarded("tts_benchmark", move || {
                    let request = tts_benchmark::tts_benchmark_request_from_env();
                    let result = {
                        let state = app_handle.state::<AppState>();
                        tts_benchmark::run_tts_benchmark_inner(state.inner(), &request)
                    };

                    match result {
                        Ok(report) => match tts_benchmark::write_tts_benchmark_report(&report) {
                            Ok(path) => {
                                report.log_summary(&path);
                            }
                            Err(err) => {
                                error!("Failed to write TTS benchmark report: {}", err);
                            }
                        },
                        Err(err) => {
                            error!("TTS benchmark failed: {}", err);
                        }
                    }

                    if env_flag("TRISPR_RUN_TTS_BENCHMARK_EXIT") {
                        app_handle.exit(0);
                    }
                });
            }

            let _ = app.emit("transcribe:state", "idle");

            // Initialise session manager with the recordings directory
            {
                let recordings_dir = paths::resolve_recordings_dir(app.handle());
                session_manager::init(recordings_dir.clone());

                // Surface any incomplete sessions from a previous crash as a warning
                let incomplete = session_manager::scan_incomplete(&recordings_dir);
                if !incomplete.is_empty() {
                    warn!(
                        "{} incomplete audio session(s) found from a previous run: {:?}",
                        incomplete.len(),
                        incomplete
                    );
                    // Emit so the frontend can show a recovery toast (future work)
                    let _ = app.emit("session:recovery-available", incomplete.len());
                }
            }

            info!("[DIAG] setup: registering hotkeys...");
            if let Err(err) = register_hotkeys(app.handle(), &settings) {
                warn!("Failed to register hotkeys: {}", err);
            }
            info!("[DIAG] setup: hotkeys done");

            if settings.transcribe_enabled {
                let state = app.state::<AppState>();
                if let Err(err) = start_transcribe_monitor(app.handle(), &state, &settings) {
                    warn!("Failed to start transcribe monitor during setup: {}", err);
                    settings.transcribe_enabled = false;
                    {
                        let mut current = state
                            .settings
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        current.transcribe_enabled = false;
                    }
                }
            }

            // Heartbeat watchdog: logs alive status every 30s to detect event-loop freezes
            crate::util::spawn_guarded("heartbeat", || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    info!("[HEARTBEAT] main process alive");
                }
            });

            // Frontend watchdog: if renderer heartbeats stop for too long while the
            // app is running, attempt webview reload/recreation.
            {
                let app_handle = app.handle().clone();
                let started_ms = crate::util::now_ms();
                crate::util::spawn_guarded("frontend_watchdog", move || {
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(
                            FRONTEND_WATCHDOG_CHECK_MS,
                        ));
                        let now = crate::util::now_ms();
                        if now.saturating_sub(started_ms) < FRONTEND_WATCHDOG_STARTUP_GRACE_MS {
                            continue;
                        }

                        let state = app_handle.state::<AppState>();
                        let last_heartbeat = state.frontend_last_heartbeat_ms.load(Ordering::Relaxed);
                        let heartbeat_age_ms = now.saturating_sub(last_heartbeat);
                        if heartbeat_age_ms < FRONTEND_HEARTBEAT_STALE_MS {
                            continue;
                        }

                        let last_reload = state
                            .frontend_watchdog_last_reload_ms
                            .load(Ordering::Relaxed);
                        if now.saturating_sub(last_reload) < FRONTEND_WATCHDOG_COOLDOWN_MS {
                            continue;
                        }

                        let Some(main_window) = app_handle.get_webview_window("main") else {
                            continue;
                        };
                        drop(main_window);

                        warn!(
                            "Frontend heartbeat stale ({} ms). Triggering main webview recovery.",
                            heartbeat_age_ms
                        );
                        state
                            .frontend_watchdog_last_reload_ms
                            .store(now, Ordering::Relaxed);
                        match recover_main_window_webview(&app_handle, "heartbeat_stale") {
                            Ok(()) => {
                                state
                                    .frontend_last_heartbeat_ms
                                    .store(now, Ordering::Relaxed);
                                let recovery_count =
                                    state.frontend_watchdog_reload_count.fetch_add(1, Ordering::Relaxed)
                                        + 1;
                                let (
                                    should_restart,
                                    recoveries_in_window,
                                    restarts_in_window,
                                    degraded_event,
                                ) = {
                                    let mut should_restart_local = false;
                                    let mut restarts_in_window_local = 0usize;
                                    let mut degraded_event_local: Option<StabilityDegradedEvent> = None;
                                    let recoveries_in_window_local: usize;

                                    {
                                        let mut watchdog_state = state
                                            .frontend_watchdog_state
                                            .lock()
                                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                                        watchdog_state.recovery_count = recovery_count;
                                        watchdog_state.last_recovery_reason = format!(
                                            "heartbeat_stale (stale_ms={})",
                                            heartbeat_age_ms
                                        );
                                        watchdog_state.recovery_timestamps_ms.push_back(now);
                                        while let Some(oldest) =
                                            watchdog_state.recovery_timestamps_ms.front().copied()
                                        {
                                            if now.saturating_sub(oldest)
                                                > FRONTEND_WATCHDOG_RECOVERY_WINDOW_MS
                                            {
                                                let _ =
                                                    watchdog_state.recovery_timestamps_ms.pop_front();
                                            } else {
                                                break;
                                            }
                                        }
                                        let recoveries_now =
                                            watchdog_state.recovery_timestamps_ms.len();
                                        recoveries_in_window_local = recoveries_now;

                                        if recoveries_now
                                            >= FRONTEND_WATCHDOG_RECOVERY_RESTART_THRESHOLD
                                        {
                                            let mut ledger =
                                                load_frontend_restart_ledger(&app_handle);
                                            prune_timestamps_window(
                                                &mut ledger.timestamps_ms,
                                                now,
                                                FRONTEND_WATCHDOG_RESTART_WINDOW_MS,
                                            );
                                            restarts_in_window_local = ledger.timestamps_ms.len();

                                            if restarts_in_window_local
                                                < FRONTEND_WATCHDOG_RESTART_MAX_PER_WINDOW
                                            {
                                                ledger.timestamps_ms.push(now);
                                                restarts_in_window_local =
                                                    ledger.timestamps_ms.len();
                                                save_frontend_restart_ledger(&app_handle, &ledger);

                                                watchdog_state.restart_timestamps_ms.push_back(now);
                                                while let Some(oldest) = watchdog_state
                                                    .restart_timestamps_ms
                                                    .front()
                                                    .copied()
                                                {
                                                    if now.saturating_sub(oldest)
                                                        > FRONTEND_WATCHDOG_RESTART_WINDOW_MS
                                                    {
                                                        let _ = watchdog_state
                                                            .restart_timestamps_ms
                                                            .pop_front();
                                                    } else {
                                                        break;
                                                    }
                                                }
                                                watchdog_state.restart_count =
                                                    watchdog_state.restart_count.saturating_add(1);
                                                should_restart_local = true;
                                            } else {
                                                let reason = format!(
                                                    "Frontend stability degraded: {} recoveries in {} min, restart budget exhausted ({}/{}) in the last 60 min.",
                                                    recoveries_now,
                                                    FRONTEND_WATCHDOG_RECOVERY_WINDOW_MS / 60_000,
                                                    restarts_in_window_local,
                                                    FRONTEND_WATCHDOG_RESTART_MAX_PER_WINDOW
                                                );
                                                let changed =
                                                    watchdog_state.last_degraded_reason != reason;
                                                watchdog_state.last_degraded_reason =
                                                    reason.clone();
                                                if changed {
                                                    degraded_event_local =
                                                        Some(StabilityDegradedEvent {
                                                            reason,
                                                            recoveries_in_window:
                                                                recoveries_now as u64,
                                                            restarts_in_window:
                                                                restarts_in_window_local as u64,
                                                            restart_blocked: true,
                                                        });
                                                }
                                            }
                                        }
                                    }

                                    (
                                        should_restart_local,
                                        recoveries_in_window_local,
                                        restarts_in_window_local,
                                        degraded_event_local,
                                    )
                                };

                                warn!(
                                    "Main webview recovery completed (count={}, stale_ms={})",
                                    recovery_count, heartbeat_age_ms
                                );
                                refresh_runtime_diagnostics(&app_handle, state.inner());

                                if let Some(event) = degraded_event {
                                    warn!("{}", event.reason);
                                    let _ = app_handle.emit("app:stability-degraded", &event);
                                    continue;
                                }

                                if should_restart {
                                    let event = StabilityDegradedEvent {
                                        reason: "Repeated frontend recoveries detected; restarting app to restore stability.".to_string(),
                                        recoveries_in_window: recoveries_in_window as u64,
                                        restarts_in_window: restarts_in_window as u64,
                                        restart_blocked: false,
                                    };
                                    let _ = app_handle.emit("app:stability-degraded", &event);
                                    if let Err(err) =
                                        request_controlled_self_restart(&app_handle, "frontend_watchdog")
                                    {
                                        warn!("Automatic self-restart failed: {}", err);
                                    }
                                    return;
                                }
                            }
                            Err(err) => {
                                warn!("Main webview recovery failed: {}", err);
                            }
                        }
                    }
                });
            }

            // LM Studio daemon auto-start: if lm_studio was the active provider when the
            // app was last closed, the provider-switch event never fires at next launch.
            // We ping first — if the daemon is already running (e.g. user keeps it open),
            // we leave it alone. Only start if unreachable.
            if capability_enabled(&settings, RuntimeCapability::AiRefinement)
                && settings.ai_fallback.provider == "lm_studio"
            {
                let endpoint = settings.providers.lm_studio.endpoint.clone();
                let preferred_model = settings.providers.lm_studio.preferred_model.trim().to_string();
                crate::util::spawn_guarded("lms_daemon_startup", move || {
                    use crate::ai_fallback::provider::ping_lm_studio_quick;
                    if ping_lm_studio_quick(&endpoint).is_err() {
                        info!("LM Studio not reachable at startup — starting daemon");
                        lms_daemon_command("up");
                        if !preferred_model.is_empty() {
                            std::thread::sleep(std::time::Duration::from_secs(3));
                            lms_load_model(&preferred_model);
                        }
                    } else {
                        info!("LM Studio already reachable at startup — skipping daemon start");
                    }
                });
            }

            if settings.mode == "vad" && settings.capture_enabled {
                // Delay VAD start by 2 seconds to allow models to load on first startup
                let app_handle = app.handle().clone();
                let settings_clone = settings.clone();
                crate::util::spawn_guarded("vad_monitor_start", move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let state = app_handle.state::<AppState>();
                    if let Err(err) =
                        crate::audio::start_vad_monitor(&app_handle, &state, &settings_clone)
                    {
                        warn!("Failed to start VAD monitor: {}", err);
                    }
                });
            }
            info!("[DIAG] setup: sync_ptt_hot_standby...");
            crate::audio::sync_ptt_hot_standby(app.handle(), &app.state::<AppState>(), &settings);
            info!("[DIAG] setup: ptt done, priming overlay state...");

            let overlay_app = app.handle().clone();
            app.listen("overlay:ready", move |_| {
                info!("[DIAG] overlay:ready event received");
                overlay::mark_overlay_ready(&overlay_app);
                info!("[DIAG] overlay:ready handled");
            });
            let overlay_heartbeat_app = app.handle().clone();
            app.listen("overlay:heartbeat", move |_| {
                overlay::mark_overlay_heartbeat(&overlay_heartbeat_app);
            });
            if env_flag("TRISPR_DISABLE_OVERLAY") {
                warn!("Overlay initialization skipped via TRISPR_DISABLE_OVERLAY=1");
            } else {
                let overlay_settings = build_overlay_settings(&settings);
                overlay::prime_overlay_controller(
                    &app.handle(),
                    Some(overlay_settings),
                    overlay::idle_overlay_state_for_settings(&settings),
                );
                overlay::preload_overlay_window(&app.handle());
                info!("[DIAG] setup: overlay state primed + window pre-warmed, building tray...");
            }
            assistant_presence::reconcile_assistant_presence_window(&app.handle(), &settings);

            let icon = {
                let paths = [
                    std::path::PathBuf::from("icons/icon.png"),
                    std::path::PathBuf::from("src-tauri/icons/icon.png"),
                    std::path::PathBuf::from("../icons/icon.png"),
                    std::path::PathBuf::from("./icons/icon.png"),
                ];

                let mut loaded_icon = None;
                for path in &paths {
                    if let Some(icon) = try_load_tray_icon(path) {
                        loaded_icon = Some(icon);
                        break;
                    }
                }

                loaded_icon.unwrap_or_else(create_fallback_icon)
            };

            let cancel_backlog_item = MenuItem::with_id(
                app,
                "cancel-backlog-expand",
                "Cancel Auto-Expand",
                false,
                None::<&str>,
            )?;
            let cancel_backlog_item_menu = cancel_backlog_item.clone();
            let cancel_backlog_item_event = cancel_backlog_item.clone();

            let _tray_icon = tauri::tray::TrayIconBuilder::with_id(TRAY_ICON_ID)
                .icon(icon)
                .tooltip("Trispr Flow")
                .on_tray_icon_event(|tray, event| {
                    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } => {
                            if BACKLOG_PROMPT_ACTIVE.load(Ordering::Acquire) {
                                return;
                            }
                            if should_handle_tray_click() {
                                toggle_main_window(tray.app_handle());
                            }
                        }
                        _ => {}
                    }
                })
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        show_main_window(app);
                    }
                    "toggle-mic" => {
                        let app_clone = app.clone();
                        crate::util::spawn_guarded("tray_toggle_mic", move || {
                            let mut current = app_clone.state::<AppState>().settings.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
                            current.capture_enabled = !current.capture_enabled;
                            if let Err(err) = save_settings_inner(&app_clone, &mut current) {
                                emit_error(&app_clone, AppError::Storage(err), Some("Tray menu"));
                            }
                        });
                    }
                    "toggle-transcribe" => {
                        let app_clone = app.clone();
                        crate::util::spawn_guarded("tray_toggle_transcribe", move || {
                            let mut current = app_clone.state::<AppState>().settings.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
                            current.transcribe_enabled = !current.transcribe_enabled;
                            if let Err(err) = save_settings_inner(&app_clone, &mut current) {
                                emit_error(&app_clone, AppError::Storage(err), Some("Tray menu"));
                            }
                        });
                    }
                    "cancel-backlog-expand" => {
                        cancel_backlog_auto_expand(app);
                        let _ = cancel_backlog_item_event.set_enabled(false);
                        let _ = cancel_backlog_item_event.set_text("Cancel Auto-Expand");
                    }
                    "quit" => {
                        cleanup_managed_processes(app, app.state::<AppState>().inner());
                        // Use ExitProcess directly to bypass all Rust/C cleanup handlers,
                        // including WebView2 destructors that cause ERROR_CLASS_HAS_WINDOWS (1412)
                        // and a 5-10s hang on Windows. Settings are persisted on every change.
                        info!("Trispr Flow shutting down — user quit (clean exit)");
                        // Brief pause to let the non-blocking log writer flush before ExitProcess
                        // kills the process (std::mem::forget(_guard) skips the normal flush).
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        #[cfg(target_os = "windows")]
                        unsafe {
                            windows_sys::Win32::System::Threading::ExitProcess(0);
                        }
                        #[cfg(not(target_os = "windows"))]
                        std::process::exit(0);
                    }
                    _ => {}
                })
                .menu({
                    let mic_item = CheckMenuItem::with_id(
                        app,
                        "toggle-mic",
                        "Microphone tracking",
                        true,
                        settings.capture_enabled,
                        None::<&str>,
                    )?;

                    let mic_item_clone = mic_item.clone();
                    app.listen("menu:update-mic", move |event| {
                        if let Ok(checked) = serde_json::from_str::<bool>(event.payload()) {
                            let _ = mic_item_clone.set_checked(checked);
                        }
                    });

                    let transcribe_item = CheckMenuItem::with_id(
                        app,
                        "toggle-transcribe",
                        "System audio transcription",
                        true,
                        settings.transcribe_enabled,
                        None::<&str>,
                    )?;

                    let transcribe_item_clone = transcribe_item.clone();
                    app.listen("menu:update-transcribe", move |event| {
                        if let Ok(checked) = serde_json::from_str::<bool>(event.payload()) {
                            let _ = transcribe_item_clone.set_checked(checked);
                        }
                    });

                    &tauri::menu::Menu::with_items(
                        app,
                        &[
                            &tauri::menu::MenuItem::with_id(
                                app,
                                "show",
                                "Open Trispr Flow",
                                true,
                                None::<&str>,
                            )?,
                            &tauri::menu::PredefinedMenuItem::separator(app)?,
                            &mic_item,
                            &transcribe_item,
                            &tauri::menu::PredefinedMenuItem::separator(app)?,
                            &cancel_backlog_item_menu,
                            &tauri::menu::PredefinedMenuItem::separator(app)?,
                            &tauri::menu::MenuItem::with_id(
                                app,
                                "quit",
                                "Quit",
                                true,
                                None::<&str>,
                            )?,
                        ],
                    )?
                })
                .show_menu_on_left_click(false)
                .build(app);

            let tray_capture_handle = app.handle().clone();
            app.listen("capture:state", move |event| {
                let code = parse_tray_state_code(event.payload());
                TRAY_CAPTURE_STATE.store(code, Ordering::Relaxed);
                refresh_tray_icon(&tray_capture_handle, 0);
            });

            let tray_transcribe_handle = app.handle().clone();
            app.listen("transcribe:state", move |event| {
                let code = parse_tray_state_code(event.payload());
                TRAY_TRANSCRIBE_STATE.store(code, Ordering::Relaxed);
                refresh_tray_icon(&tray_transcribe_handle, 0);
            });

            let backlog_prompt_handle = app.handle().clone();
            let cancel_backlog_item_prompt = cancel_backlog_item.clone();
            app.listen("transcribe:backlog-warning", move |_event| {
                schedule_backlog_auto_expand(
                    backlog_prompt_handle.clone(),
                    cancel_backlog_item_prompt.clone(),
                );
            });

            refresh_tray_icon(app.handle(), 0);
            start_tray_pulse_loop(app.handle().clone());

            // Restore main window geometry and visibility state
            if let Some(window) = app.get_webview_window("main") {
                let window_settings = load_settings(app.handle());
                restore_window_geometry(&window, &window_settings);
                MAIN_WINDOW_RESTORED.store(true, Ordering::Release);

                // Restore window visibility state from last session
                match window_settings.main_window_start_state.as_str() {
                    "tray" => {
                        // Start hidden in system tray
                        info!("Restoring window state: hidden in system tray");
                        let _ = window.hide();
                        let _ = window.set_skip_taskbar(true);
                    }
                    "minimized" => {
                        // Start minimized
                        info!("Restoring window state: minimized");
                        let _ = window.show();
                        let _ = window.set_skip_taskbar(false);
                        let _ = window.minimize();
                    }
                    _ => {
                        // "normal" — explicitly show from hidden startup config.
                        let _ = window.show();
                        let _ = window.set_skip_taskbar(false);
                    }
                }
            }

            {
                let state = app.state::<AppState>();
                let snapshot = {
                    let mut status = state
                        .startup_status
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    status.interactive = true;
                    status.clone()
                };
                let _ = app.emit("startup:status", &snapshot);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                hide_main_window(window.app_handle());
            }

            // Re-anchor overlay when the main window moves to a monitor with
            // different DPI (e.g. user drags app to a 4K display, or system
            // display settings change). The overlay window fires its own
            // ScaleFactorChanged too, but the main-window signal catches
            // cases where only the primary display DPI changes and the
            // overlay was parked there.
            //
            // IMPORTANT: on_window_event runs on the Win32 message thread.
            // Calling apply_overlay_settings directly here would invoke
            // window.set_size/set_position/eval synchronously from within
            // WndProc, causing tao re-entrance → freeze. Offload to a
            // background thread via spawn_guarded (same pattern as tray handlers).
            if let tauri::WindowEvent::ScaleFactorChanged { .. } = event {
                let app = window.app_handle().clone();
                crate::util::spawn_guarded("dpi_overlay_reanchor", move || {
                    let desired = app
                        .state::<AppState>()
                        .overlay_controller
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .desired_settings
                        .clone();
                    if let Some(settings) = desired {
                        let _ = crate::overlay::apply_overlay_settings(&app, &settings);
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            get_task_capture_settings,
            save_task_capture_settings,
            test_task_capture_endpoint,
            get_startup_status,
            get_runtime_diagnostics,
            save_settings,
            save_window_state,
            save_window_visibility_state,
            show_assistant_presence_window,
            list_modules,
            scan_module_packages,
            install_bundled_module_package,
            enable_module,
            disable_module,
            get_module_health,
            check_module_updates,
            agent_list_supported_actions,
            agent_parse_command,
            agent_compose_unknown_reply,
            assistant_execute_direct_action,
            search_transcript_sessions,
            agent_build_execution_plan,
            agent_execute_gdd_plan,
            agent_cancel_pending_confirmation,
            list_screen_sources,
            start_vision_stream,
            stop_vision_stream,
            get_vision_stream_health,
            capture_vision_snapshot,
            list_tts_providers,
            list_tts_voices,
            list_piper_voice_catalog,
            download_piper_voice_key,
            speak_tts,
            stop_tts,
            test_tts_provider,
            #[cfg(feature = "module-gdd")]
            list_gdd_presets,
            #[cfg(feature = "module-gdd")]
            save_gdd_preset_clone,
            #[cfg(feature = "module-gdd")]
            detect_gdd_preset,
            #[cfg(feature = "module-gdd")]
            generate_gdd_draft,
            #[cfg(feature = "module-gdd")]
            validate_gdd_draft,
            #[cfg(feature = "module-gdd")]
            render_gdd_for_confluence,
            #[cfg(feature = "module-gdd")]
            render_gdd_markdown,
            #[cfg(feature = "module-confluence")]
            test_confluence_connection,
            #[cfg(feature = "module-confluence")]
            confluence_oauth_start,
            #[cfg(feature = "module-confluence")]
            confluence_oauth_exchange,
            #[cfg(feature = "module-confluence")]
            confluence_list_spaces,
            #[cfg(feature = "module-confluence")]
            load_gdd_template_from_file,
            #[cfg(feature = "module-confluence")]
            load_gdd_template_from_confluence,
            #[cfg(feature = "module-confluence")]
            suggest_confluence_target,
            #[cfg(feature = "module-confluence")]
            publish_gdd_to_confluence,
            #[cfg(feature = "module-confluence")]
            publish_or_queue_gdd_to_confluence,
            #[cfg(feature = "module-confluence")]
            list_pending_gdd_publishes,
            #[cfg(feature = "module-confluence")]
            retry_pending_gdd_publish,
            #[cfg(feature = "module-confluence")]
            delete_pending_gdd_publish,
            #[cfg(feature = "module-confluence")]
            save_confluence_secret,
            #[cfg(feature = "module-confluence")]
            clear_confluence_secret,
            save_transcript,
            list_audio_devices,
            list_output_devices,
            list_models,
            download_model,
            check_model_available,
            remove_model,
            quantize_model,
            hide_external_model,
            clear_hidden_external_models,
            pick_model_dir,
            get_models_dir,
            get_history,
            get_transcribe_history,
            clear_active_transcript_history,
            delete_active_transcript_entry,
            list_history_partitions,
            load_history_partition,
            add_history_entry,
            add_transcribe_entry,
            start_recording,
            stop_recording,
            toggle_transcribe,
            expand_transcribe_backlog,
            paste_transcript_text,
            apply_model,
            validate_hotkey,
            test_hotkey,
            get_hotkey_conflicts,
            save_crash_recovery,
            clear_crash_recovery,
            encode_to_opus,
            check_ffmpeg,
            get_dependency_preflight_status,
            get_ffmpeg_version_info,
            get_last_recording_path,
            get_recordings_directory,
            open_recordings_directory,
            open_log_directory,
            fetch_available_models,
            fetch_ollama_models_with_size,
            test_provider_connection,
            save_provider_api_key,
            clear_provider_api_key,
            verify_provider_auth,
            save_ollama_endpoint,
            detect_ollama_runtime,
            list_ollama_runtime_versions,
            fetch_ollama_online_versions,
            download_ollama_runtime,
            install_ollama_runtime,
            start_ollama_runtime,
            verify_ollama_runtime,
            import_ollama_model_from_file,
            set_strict_local_mode,
            refine_transcript,
            ping_refinement_model,
            run_latency_benchmark,
            run_tts_benchmark,
            get_runtime_metrics_snapshot,
            record_runtime_metric,
            frontend_heartbeat,
            log_frontend_event,
            pull_ollama_model,
            delete_ollama_model,
            get_ollama_model_info,
            unload_ollama_model,
            get_gpu_vram_usage,
            get_hardware_info,
            purge_gpu_memory,
            stop_ollama_runtime,
            install_lm_studio,
            video_ingest_sources,
            video_ingest_history_entry,
            video_generate,
            video_get_output_dir,
            video_open_output_dir,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                info!("Application exiting, cleaning up child processes");
                cleanup_managed_processes(app_handle, app_handle.state::<AppState>().inner());
            }
        });
}
