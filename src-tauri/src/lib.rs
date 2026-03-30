// Trispr Flow - core app runtime
#![allow(clippy::needless_return)]

mod ai_fallback;
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
mod session_manager;
mod state;
mod transcription;
mod util;
mod whisper_server;
mod workflow_agent;

use arboard::{Clipboard, ImageData};
use enigo::{Enigo, Key, KeyboardControllable};
use errors::{AppError, ErrorEvent};
use overlay::emit_capture_idle_overlay;
use state::{
    AppState, AssistantOrchestratorState, HistoryEntry, RuntimeDiagnostics, Settings, StartupStatus,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{
    AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};
use std::sync::Mutex;

// Exponential backoff state for Ollama diagnostics pings.
// Prevents flooding failed network calls during startup when Ollama is slow to come up.
// Backoff schedule: 1st fail→immediate, 2nd→2 s, 3rd→4 s, 4th→8 s, 5+→30 s.
static OLLAMA_DIAG_FAIL_COUNT: AtomicU32 = AtomicU32::new(0);
static OLLAMA_DIAG_NEXT_MS: AtomicU64 = AtomicU64::new(0);
static ASSISTANT_CONFIRM_TOKEN_SEQ: AtomicU64 = AtomicU64::new(1_000);
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::Wry;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{error, info, warn};

/// Wrap a Tauri command body in `catch_unwind` so that a panic inside module
/// code returns a clean `Err(String)` instead of crashing the app.
/// Works for any command that returns `Result<T, String>`.
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

use crate::ai_fallback::error::AIError;
use crate::ai_fallback::keyring as ai_fallback_keyring;
use crate::ai_fallback::models::RefinementOptions;
use crate::ai_fallback::provider::{
    default_models_for_provider, is_local_ollama_endpoint, is_ssrf_target, list_ollama_models,
    list_ollama_models_with_size, ping_ollama, ping_ollama_quick, prompt_for_profile,
    resolve_effective_local_model, AIProvider, ProviderFactory,
};
use crate::audio::{list_audio_devices, list_output_devices, start_recording, stop_recording};
use crate::history_partition::PartitionedHistory;
use crate::models::{
    check_model_available, clear_hidden_external_models, download_model, get_models_dir,
    hide_external_model, list_models, pick_model_dir, quantize_model, remove_model,
};
use crate::modules::{
    health as module_health, lifecycle as module_lifecycle, normalize_confluence_settings,
    normalize_gdd_module_settings, normalize_module_settings, normalize_vision_input_settings,
    normalize_voice_output_settings, normalize_workflow_agent_settings,
    registry as module_registry,
};
use crate::ollama_runtime::{
    detect_ollama_runtime, download_ollama_runtime, fetch_ollama_online_versions,
    import_ollama_model_from_file, install_ollama_runtime, list_ollama_runtime_versions,
    set_strict_local_mode, start_ollama_runtime, verify_ollama_runtime,
};
use crate::state::{
    get_runtime_metrics_snapshot as runtime_metrics_snapshot, load_settings,
    normalize_ai_fallback_fields, normalize_ai_refinement_module_binding,
    normalize_continuous_dump_fields, normalize_history_alias_fields, normalize_product_mode_field,
    push_history_entry_inner, push_transcribe_entry_inner, record_refinement_fallback_timed_out,
    record_refinement_timeout, save_settings_file, sync_model_dir_env, AI_REFINEMENT_MODULE_ID,
};
use crate::transcription::{
    expand_transcribe_backlog as expand_transcribe_backlog_inner, last_transcription_accelerator,
    start_transcribe_monitor, stop_transcribe_monitor, toggle_transcribe_state, transcribe_audio,
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

#[derive(Debug, Clone)]
struct PendingAssistantConfirmation {
    plan: crate::workflow_agent::AgentExecutionPlan,
    confirm_token: String,
    expires_at_ms: u64,
}

static ASSISTANT_PENDING_CONFIRMATION: Mutex<Option<PendingAssistantConfirmation>> =
    Mutex::new(None);

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
}

/// Guard that rejects requests when strict-local-mode is active and the
/// configured Ollama endpoint is not a local address.
/// Lock settings, apply a mutation, persist to disk, and emit a change event.
///
/// The closure receives `&mut Settings` and may return `Err` to abort.  On
/// success the updated settings are saved and broadcast.
fn update_and_persist_settings<F>(app: &AppHandle, state: &AppState, f: F) -> Result<(), String>
where
    F: FnOnce(&mut Settings) -> Result<(), String>,
{
    let snapshot = {
        let mut settings = state.settings.write().map_err(|e| e.to_string())?;
        f(&mut settings)?;
        settings.clone()
    };
    save_settings_file(app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot);
    Ok(())
}

pub(crate) fn check_strict_local_mode(settings: &Settings) -> Result<(), String> {
    if settings.ai_fallback.strict_local_mode
        && !is_local_ollama_endpoint(&settings.providers.ollama.endpoint)
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
    pub provider: Box<dyn AIProvider>,
    pub api_key: String,
    pub model: String,
    pub repaired: bool,
    pub options: RefinementOptions,
}

/// Shared refinement-context preparation used by the Tauri command, the
/// benchmark helper, and the auto-refinement worker.  Validates settings,
/// creates the provider client, resolves the model, and builds options.
///
/// Does **not** persist model-repair changes — callers decide if/how to do that.
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

    let provider: Box<dyn crate::ai_fallback::provider::AIProvider> = if is_ollama {
        check_strict_local_mode(settings)?;
        ProviderFactory::create_ollama(settings.providers.ollama.endpoint.clone())
    } else if is_lm_studio {
        // Quick pre-flight: verify LM Studio is reachable and has a model loaded
        // before attempting refinement.  Avoids a slow timeout + confusing error.
        if let Err(e) = crate::ai_fallback::provider::ping_lm_studio_quick(
            &settings.providers.lm_studio.endpoint,
        ) {
            return Err(format!("LM Studio not ready: {}", e));
        }
        ProviderFactory::create_lm_studio(
            settings.providers.lm_studio.endpoint.clone(),
            settings.providers.lm_studio.api_key.clone(),
        )
    } else if is_oobabooga {
        ProviderFactory::create_oobabooga(
            settings.providers.oobabooga.endpoint.clone(),
            settings.providers.oobabooga.api_key.clone(),
        )
    } else {
        ProviderFactory::create(&ai.provider).map_err(|e| e.to_string())?
    };

    let api_key = if is_ollama || is_local_compat {
        String::new()
    } else {
        ai_fallback_keyring::read_api_key(app, &ai.provider)?
            .ok_or_else(|| format!("No API key stored for provider '{}'.", ai.provider))?
    };

    if !is_ollama && !is_local_compat {
        provider
            .validate_api_key(&api_key)
            .map_err(|e| e.to_string())?;
    }

    let mut model = ai.model.trim().to_string();
    let mut repaired = false;
    if is_ollama {
        let endpoint = settings.providers.ollama.endpoint.clone();
        let preferred = settings.providers.ollama.preferred_model.clone();
        // Fast reachability check (300ms) before the slow model-list call (up to 5s).
        // This prevents the AI refinement thread from blocking paste for seconds
        // when Ollama is not running.
        crate::ai_fallback::provider::ping_ollama_quick(&endpoint).map_err(|e| e.to_string())?;
        let resolved = resolve_effective_local_model(&model, &preferred, &endpoint)
            .map_err(|e| e.to_string())?;
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
    let enforce_language_guard = ai.preserve_source_language
        && ai.prompt_profile != "custom"
        && ai.prompt_profile != "llm_prompt";

    let options = RefinementOptions {
        temperature: ai.temperature,
        max_tokens: ai.max_tokens,
        low_latency_mode: ai.low_latency_mode,
        language: Some(effective_language.clone()),
        custom_prompt: prompt_for_profile(
            &ai.prompt_profile,
            &effective_language,
            Some(ai.custom_prompt.as_str()),
            ai.preserve_source_language,
        ),
        enforce_language_guard,
    };

    Ok(RefinementSetup {
        provider,
        api_key,
        model,
        repaired,
        options,
    })
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

    let register_ptt = || -> Result<(), String> {
        let ptt = settings.hotkey_ptt.trim();
        if ptt.is_empty() {
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
        info!("Registering Transcribe hotkey (toggle): {}", hotkey);
        match manager.on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let app = app.clone();
                let enabled = app
                    .state::<AppState>()
                    .settings
                    .read()
                    .map(|settings| settings.transcribe_enabled)
                    .unwrap_or(false);
                let target_enabled = !enabled;
                if let Err(err) = set_transcribe_enabled(&app, target_enabled) {
                    emit_error(&app, AppError::AudioDevice(err), Some("System Audio"));
                    return;
                }
                let cue = if target_enabled { "start" } else { "stop" };
                let _ = app.emit("audio:cue", cue);
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
                error!("Failed to register Product Mode hotkey '{}': {}", hotkey, e);
                emit_error(
                    app,
                    AppError::Hotkey(format!(
                        "Could not register Product Mode hotkey '{}': {}",
                        hotkey, e
                    )),
                    Some("Hotkey Registration"),
                );
                Err(e.to_string())
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

    // Register Toggle Activation Words hotkey
    let hotkey = settings.hotkey_toggle_activation_words.trim();
    if !hotkey.is_empty() {
        match manager.on_shortcut(hotkey, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                toggle_activation_words_async(app.clone());
            }
        }) {
            Ok(_) => {
                info!("Toggle Activation Words hotkey registered successfully");
            }
            Err(e) => {
                error!(
                    "Failed to register Toggle Activation Words hotkey '{}': {}",
                    hotkey, e
                );
                errors.push(format!("Toggle Activation Words: {}", e));
                emit_error(
                    app,
                    AppError::Hotkey(format!(
                        "Could not register Toggle Activation Words hotkey '{}': {}",
                        hotkey, e
                    )),
                    Some("Hotkey Registration"),
                );
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
async fn fetch_available_models(
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
            let models =
                crate::ai_fallback::provider::list_openai_compat_models(&endpoint, &api_key);
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

#[tauri::command]
fn open_log_directory() -> Result<(), String> {
    let log_dir = std::env::var("LOCALAPPDATA")
        .map(|d| std::path::PathBuf::from(d).join("Trispr Flow").join("logs"))
        .map_err(|e| e.to_string())?;

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("explorer.exe")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| format!("Failed to open log directory: {}", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On macOS and Linux, use the `open` command
        use std::process::Command;
        Command::new("open")
            .arg(&log_dir)
            .spawn()
            .map_err(|e| format!("Failed to open log directory: {}", e))?;
        Ok(())
    }
}

fn fetch_available_models_ollama_impl(endpoint: String) -> Result<Vec<String>, String> {
    let models = list_ollama_models(&endpoint);
    if models.is_empty() {
        // Distinguish "Ollama not reachable" from "reachable but no models installed".
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
async fn fetch_ollama_models_with_size(
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
        // Distinguish "Ollama not reachable" from "reachable but no models installed".
        // Use quick ping (300ms) to avoid blocking the command thread for seconds.
        ping_ollama_quick(&endpoint).map_err(|e| e.to_string())?;
    }
    Ok(models
        .into_iter()
        .map(|(name, size_bytes)| serde_json::json!({ "name": name, "size_bytes": size_bytes }))
        .collect())
}

#[tauri::command]
async fn test_provider_connection(
    state: State<'_, AppState>,
    provider: String,
    api_key: String,
) -> Result<serde_json::Value, String> {
    let provider_id = provider.trim().to_lowercase();

    // Ollama: perform a real HTTP ping instead of API key validation
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

    // OpenAI-compat backends (LM Studio, Oobabooga)
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
            let models = crate::ai_fallback::provider::list_openai_compat_models(&endpoint, &effective_key);
            if models.is_empty() {
                Err(format!("{} not reachable at {}. Is the server running?", label, endpoint))
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
fn save_provider_api_key(
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

    update_and_persist_settings(&app, state.inner(), |s| {
        s.providers.set_api_key_stored(&provider_id, true)?;
        normalize_ai_fallback_fields(s);
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
fn clear_provider_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
) -> Result<serde_json::Value, String> {
    let provider_id = provider.trim().to_lowercase();
    ai_fallback_keyring::clear_api_key(&app, &provider_id)?;

    update_and_persist_settings(&app, state.inner(), |s| {
        s.providers.set_api_key_stored(&provider_id, false)?;
        normalize_ai_fallback_fields(s);
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
fn verify_provider_auth(
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
        update_and_persist_settings(&app, state.inner(), |s| {
            s.providers.lock_auth(&provider_id)?;
            normalize_ai_fallback_fields(s);
            Ok(())
        })?;
        return Err(
            "OAuth verification is not supported yet. Use API key verification.".to_string(),
        );
    }

    let stored_key = ai_fallback_keyring::read_api_key(&app, &provider_id)?;
    let Some(api_key) = stored_key else {
        update_and_persist_settings(&app, state.inner(), |s| {
            s.providers.lock_auth(&provider_id)?;
            normalize_ai_fallback_fields(s);
            Ok(())
        })?;
        return Err(format!(
            "No stored API key found for provider '{}'.",
            provider_id
        ));
    };

    let provider_client = ProviderFactory::create(&provider_id).map_err(|e| e.to_string())?;
    if let Err(error) = provider_client.validate_api_key(api_key.trim()) {
        update_and_persist_settings(&app, state.inner(), |s| {
            s.providers.lock_auth(&provider_id)?;
            normalize_ai_fallback_fields(s);
            Ok(())
        })?;
        return Err(error.to_string());
    }

    let verified_at = now_iso();
    update_and_persist_settings(&app, state.inner(), |s| {
        s.providers.set_auth_verified(
            &provider_id,
            "verified_api_key",
            Some(verified_at.clone()),
        )?;
        normalize_ai_fallback_fields(s);
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
fn save_ollama_endpoint(
    app: AppHandle,
    state: State<'_, AppState>,
    endpoint: String,
) -> Result<serde_json::Value, String> {
    let trimmed = endpoint.trim().to_string();
    if trimmed.is_empty() {
        return Err("Endpoint cannot be empty.".to_string());
    }
    // Block SSRF-sensitive targets (cloud metadata, link-local)
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
    update_and_persist_settings(&app, state.inner(), |s| {
        s.providers.ollama.endpoint = trimmed.clone();
        Ok(())
    })?;
    Ok(serde_json::json!({
        "status": "success",
        "endpoint": trimmed,
    }))
}

#[tauri::command]
async fn refine_transcript(
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
        update_and_persist_settings(&app, state.inner(), |s| {
            s.ai_fallback.model = model.clone();
            s.providers.ollama.preferred_model = model.clone();
            s.postproc_llm_model = model;
            normalize_ai_fallback_fields(s);
            Ok(())
        })?;
    }

    // The HTTP call to the AI provider can block for many seconds (local LLM
    // inference, slow network, etc.).  Running it on a blocking worker thread
    // prevents it from stalling the Tauri event loop and triggering tao's
    // "NewEvents without RedrawEventsCleared" warning that leads to a UI freeze.
    let app_clone = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        setup
            .provider
            .refine_transcript(&transcript, &setup.model, &setup.options, &setup.api_key)
    })
    .await
    .map_err(|e| format!("refine_transcript task failed: {}", e))?;

    // Emit health-degraded event on transport failures so the frontend can
    // re-check Ollama state without requiring a full app restart.
    if let Err(AIError::Timeout | AIError::OllamaNotRunning) = &result {
        let _ = app_clone.emit("ai_fallback:health_degraded", ());
    }

    let result = result.map_err(|e| e.to_string())?;
    serde_json::to_value(&result).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
struct LatencyBenchmarkRequest {
    fixture_paths: Vec<String>,
    warmup_runs: u32,
    measure_runs: u32,
    include_refinement: bool,
    refinement_model: Option<String>,
}

impl Default for LatencyBenchmarkRequest {
    fn default() -> Self {
        Self {
            fixture_paths: Vec::new(),
            warmup_runs: 3,
            measure_runs: 30,
            include_refinement: true,
            refinement_model: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct LatencyBenchmarkSample {
    fixture: String,
    whisper_ms: u64,
    refine_ms: u64,
    total_ms: u64,
    mode: String,
    accelerator: String,
    refinement_model: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct LatencyBenchmarkResult {
    warmup_runs: u32,
    measure_runs: u32,
    p50_ms: u64,
    p95_ms: u64,
    slo_p50_ms: u64,
    slo_p95_ms: u64,
    slo_pass: bool,
    samples: Vec<LatencyBenchmarkSample>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct TtsBenchmarkScenario {
    id: String,
    text: String,
    length_bucket: String, // "short" | "long"
    language: String,      // "de" | "en"
    thermal: String,       // "cold" | "warm"
}

impl Default for TtsBenchmarkScenario {
    fn default() -> Self {
        Self {
            id: String::new(),
            text: String::new(),
            length_bucket: String::new(),
            language: String::new(),
            thermal: String::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
struct TtsBenchmarkRequest {
    providers: Vec<String>,
    scenarios: Vec<TtsBenchmarkScenario>,
    warmup_runs: u32,
    measure_runs: u32,
    rate: f32,
    volume: f32,
    piper_binary_path: Option<String>,
    piper_model_path: Option<String>,
    qwen3_tts_endpoint: Option<String>,
    qwen3_tts_model: Option<String>,
    qwen3_tts_voice: Option<String>,
    qwen3_tts_api_key: Option<String>,
    qwen3_tts_timeout_sec: Option<u64>,
    lock_matrix: bool,
    run_runtime_smoke: bool,
}

impl Default for TtsBenchmarkRequest {
    fn default() -> Self {
        Self {
            providers: vec![
                "windows_native".to_string(),
                "local_custom".to_string(),
                "qwen3_tts".to_string(),
            ],
            scenarios: Vec::new(),
            warmup_runs: 1,
            measure_runs: 3,
            rate: 1.0,
            volume: 1.0,
            piper_binary_path: None,
            piper_model_path: None,
            qwen3_tts_endpoint: None,
            qwen3_tts_model: None,
            qwen3_tts_voice: None,
            qwen3_tts_api_key: None,
            qwen3_tts_timeout_sec: None,
            lock_matrix: true,
            run_runtime_smoke: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkSample {
    provider: String,
    scenario: String,
    run: u32,
    elapsed_ms: u64,
    success: bool,
    error: Option<String>,
    failure_category: Option<String>, // missing_binary | missing_model | endpoint_unreachable | auth_missing | runtime_error
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkProviderSummary {
    provider: String,
    attempts: u32,
    success_count: u32,
    failure_count: u32,
    success_rate: f32,
    p50_ms: Option<u64>,
    p95_ms: Option<u64>,
    avg_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkGateConfig {
    reliability_min_success_rate: f32,
    latency_target_p50_ms: u64,
    latency_target_p95_ms: u64,
    min_success_per_scenario: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsProviderProfile {
    provider: String,
    surface: String, // "runtime_stable" | "benchmark_experimental"
    experimental_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsPreflightCheck {
    provider: String,
    check: String,
    passed: bool,
    category: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsRuntimeSmokeCheck {
    provider: String,
    passed: bool,
    category: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsProviderGateEvaluation {
    provider: String,
    evaluated_for_release: bool,
    passes_release_gate: bool,
    preflight_ok: bool,
    runtime_smoke_ok: bool,
    reliability_ok: bool,
    latency_ok: bool,
    scenario_success_ok: bool,
    success_rate: f32,
    p50_ms: Option<u64>,
    p95_ms: Option<u64>,
    min_success_in_any_scenario: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkResult {
    artifact_version: String,
    generated_at: String,
    warmup_runs: u32,
    measure_runs: u32,
    providers: Vec<String>,
    scenarios: Vec<String>,
    scenario_matrix_locked: bool,
    gates: TtsBenchmarkGateConfig,
    provider_profiles: Vec<TtsProviderProfile>,
    preflight_checks: Vec<TtsPreflightCheck>,
    runtime_smoke_checks: Vec<TtsRuntimeSmokeCheck>,
    samples: Vec<TtsBenchmarkSample>,
    provider_summaries: Vec<TtsBenchmarkProviderSummary>,
    provider_gate_evaluations: Vec<TtsProviderGateEvaluation>,
    provider_consistency_ok: bool,
    provider_consistency_detail: String,
    fallback_order: Vec<String>,
    release_gate_pass: bool,
    release_gate_reason: String,
    recommended_default_provider: Option<String>,
    recommendation_reason: String,
    uncategorized_failure_count: u32,
    warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct Qwen3TtsBenchmarkConfig {
    endpoint: String,
    model: String,
    voice: String,
    api_key: Option<String>,
    timeout_sec: u64,
}

const TTS_PROVIDER_SURFACE_RUNTIME_STABLE: &str = "runtime_stable";
const TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL: &str = "benchmark_experimental";
const TTS_FAILURE_MISSING_BINARY: &str = "missing_binary";
const TTS_FAILURE_MISSING_MODEL: &str = "missing_model";
const TTS_FAILURE_ENDPOINT_UNREACHABLE: &str = "endpoint_unreachable";
const TTS_FAILURE_AUTH_MISSING: &str = "auth_missing";
const TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED: &str = "stream_config_unsupported";
const TTS_FAILURE_RUNTIME_ERROR: &str = "runtime_error";

fn default_tts_benchmark_gates() -> TtsBenchmarkGateConfig {
    TtsBenchmarkGateConfig {
        reliability_min_success_rate: 0.95,
        latency_target_p50_ms: 700,
        latency_target_p95_ms: 1500,
        min_success_per_scenario: 2,
    }
}

fn tts_provider_profile(provider: &str) -> TtsProviderProfile {
    match provider {
        "qwen3_tts" => TtsProviderProfile {
            provider: provider.to_string(),
            surface: TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL.to_string(),
            experimental_reason: Some(
                "Endpoint-backed runtime provider treated as experimental for release-gating."
                    .to_string(),
            ),
        },
        _ => TtsProviderProfile {
            provider: provider.to_string(),
            surface: TTS_PROVIDER_SURFACE_RUNTIME_STABLE.to_string(),
            experimental_reason: None,
        },
    }
}

fn is_runtime_stable_provider(provider: &str) -> bool {
    tts_provider_profile(provider).surface == TTS_PROVIDER_SURFACE_RUNTIME_STABLE
}

fn classify_tts_failure(error: &str) -> String {
    let normalized = error.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return TTS_FAILURE_RUNTIME_ERROR.to_string();
    }

    if normalized.contains("binary not found")
        || normalized.contains("npm.cmd not found")
        || normalized.contains("failed to start piper")
        || normalized.contains("no such file")
    {
        return TTS_FAILURE_MISSING_BINARY.to_string();
    }
    if normalized.contains("model not found")
        || normalized.contains("no piper voice model found")
        || normalized.contains("set piper_model_path")
        || normalized.contains("onnx")
    {
        return TTS_FAILURE_MISSING_MODEL.to_string();
    }
    if normalized.contains("http 401")
        || normalized.contains("http 403")
        || normalized.contains("unauthorized")
        || normalized.contains("forbidden")
        || normalized.contains("api key")
        || normalized.contains("authorization")
    {
        return TTS_FAILURE_AUTH_MISSING.to_string();
    }
    if normalized.contains("[tts_output_stream_config_unsupported]")
        || normalized.contains("stream configuration is not supported")
        || normalized.contains("streamconfignotsupported")
    {
        return TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED.to_string();
    }
    if normalized.contains("endpoint")
        || normalized.contains("timed out")
        || normalized.contains("connection")
        || normalized.contains("refused")
        || normalized.contains("dns")
        || normalized.contains("transport")
        || normalized.contains("failed to connect")
    {
        return TTS_FAILURE_ENDPOINT_UNREACHABLE.to_string();
    }
    TTS_FAILURE_RUNTIME_ERROR.to_string()
}

fn default_tts_benchmark_scenarios() -> Vec<TtsBenchmarkScenario> {
    vec![
        TtsBenchmarkScenario {
            id: "short_de_cold".to_string(),
            text: "Kurzer Benchmark-Check.".to_string(),
            length_bucket: "short".to_string(),
            language: "de".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "short_de_warm".to_string(),
            text: "Kurzer Benchmark-Check.".to_string(),
            length_bucket: "short".to_string(),
            language: "de".to_string(),
            thermal: "warm".to_string(),
        },
        TtsBenchmarkScenario {
            id: "short_en_cold".to_string(),
            text: "Short benchmark check.".to_string(),
            length_bucket: "short".to_string(),
            language: "en".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "short_en_warm".to_string(),
            text: "Short benchmark check.".to_string(),
            length_bucket: "short".to_string(),
            language: "en".to_string(),
            thermal: "warm".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_de_cold".to_string(),
            text: "Dies ist ein längerer deutscher Benchmark-Satz, der Antworttempo und Stabilität unter praxisnahen Bedingungen vergleicht."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "de".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_de_warm".to_string(),
            text: "Dies ist ein längerer deutscher Benchmark-Satz, der Antworttempo und Stabilität unter praxisnahen Bedingungen vergleicht."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "de".to_string(),
            thermal: "warm".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_en_cold".to_string(),
            text: "This is a longer benchmark sentence to compare synthesis latency and stability under realistic assistant output conditions."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "en".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_en_warm".to_string(),
            text: "This is a longer benchmark sentence to compare synthesis latency and stability under realistic assistant output conditions."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "en".to_string(),
            thermal: "warm".to_string(),
        },
    ]
}

fn normalize_tts_benchmark_providers(requested: &[String]) -> Vec<String> {
    let mut providers = Vec::<String>::new();
    for value in requested {
        let normalized = value.trim().to_lowercase();
        if normalized != "windows_native"
            && normalized != "windows_natural"
            && normalized != "local_custom"
            && normalized != "qwen3_tts"
        {
            continue;
        }
        if !providers.contains(&normalized) {
            providers.push(normalized);
        }
    }
    if providers.is_empty() {
        vec![
            "windows_native".to_string(),
            "local_custom".to_string(),
            "qwen3_tts".to_string(),
        ]
    } else {
        providers
    }
}

fn resolve_qwen3_tts_benchmark_config(request: &TtsBenchmarkRequest) -> Qwen3TtsBenchmarkConfig {
    let endpoint = request
        .qwen3_tts_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http://127.0.0.1:8000/v1/audio/speech")
        .to_string();
    let model = request
        .qwen3_tts_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice")
        .to_string();
    let voice = request
        .qwen3_tts_voice
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("vivian")
        .to_string();
    let api_key = request
        .qwen3_tts_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let timeout_sec = request.qwen3_tts_timeout_sec.unwrap_or(45).clamp(3, 180);

    Qwen3TtsBenchmarkConfig {
        endpoint,
        model,
        voice,
        api_key,
        timeout_sec,
    }
}

fn resolve_qwen3_tts_runtime_config(
    settings: &crate::modules::VoiceOutputSettings,
) -> Qwen3TtsBenchmarkConfig {
    let endpoint = settings.qwen3_tts_endpoint.trim();
    let model = settings.qwen3_tts_model.trim();
    let voice = settings.qwen3_tts_voice.trim();
    let api_key = settings.qwen3_tts_api_key.trim();
    Qwen3TtsBenchmarkConfig {
        endpoint: if endpoint.is_empty() {
            "http://127.0.0.1:8000/v1/audio/speech".to_string()
        } else {
            endpoint.to_string()
        },
        model: if model.is_empty() {
            "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice".to_string()
        } else {
            model.to_string()
        },
        voice: if voice.is_empty() {
            "vivian".to_string()
        } else {
            voice.to_string()
        },
        api_key: if api_key.is_empty() {
            None
        } else {
            Some(api_key.to_string())
        },
        timeout_sec: settings.qwen3_tts_timeout_sec.clamp(3, 180),
    }
}

fn format_ureq_status_error(context: &str, code: u16, response: ureq::Response) -> String {
    let mut body = response.into_string().unwrap_or_default();
    body = body.replace('\n', " ").replace('\r', " ");
    let body = body.trim();
    if body.is_empty() {
        format!("{} failed with HTTP {}", context, code)
    } else {
        format!("{} failed with HTTP {}: {}", context, code, body)
    }
}

fn request_qwen3_tts_audio_bytes(
    text: &str,
    rate: f32,
    config: &Qwen3TtsBenchmarkConfig,
) -> Result<(Vec<u8>, String), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("TTS text is empty.".to_string());
    }

    let agent = ureq::builder()
        .timeout(Duration::from_secs(config.timeout_sec))
        .build();
    let mut request = agent
        .post(&config.endpoint)
        .set("Content-Type", "application/json")
        .set(
            "Accept",
            "audio/wav, audio/mpeg, application/octet-stream, application/json",
        );
    if let Some(api_key) = config.api_key.as_ref() {
        request = request.set("Authorization", &format!("Bearer {}", api_key));
    }

    let body = serde_json::json!({
        "model": config.model,
        "input": text,
        "voice": config.voice,
        "response_format": "wav",
        "stream": false,
        "speed": rate.clamp(0.5, 2.0),
    });

    let response = match request.send_json(body) {
        Ok(response) => response,
        Err(ureq::Error::Status(code, response)) => {
            return Err(format_ureq_status_error(
                "Qwen3-TTS benchmark request",
                code,
                response,
            ));
        }
        Err(ureq::Error::Transport(transport)) => {
            return Err(format!(
                "Qwen3-TTS benchmark request failed: {} (endpoint={})",
                transport, config.endpoint
            ));
        }
    };

    let content_type = response
        .header("Content-Type")
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|err| format!("Qwen3-TTS response read failed: {}", err))?;

    Ok((bytes, content_type))
}

fn benchmark_qwen3_tts_synthesis(
    text: &str,
    rate: f32,
    config: &Qwen3TtsBenchmarkConfig,
) -> Result<(), String> {
    let (bytes, content_type) = request_qwen3_tts_audio_bytes(text, rate, config)?;

    if bytes.is_empty() {
        return Err("Qwen3-TTS returned an empty response body.".to_string());
    }
    if content_type.contains("application/json") {
        let text = String::from_utf8_lossy(&bytes).trim().to_string();
        return Err(format!(
            "Qwen3-TTS returned JSON instead of audio: {}",
            text
        ));
    }

    Ok(())
}

fn speak_qwen3_tts(
    text: &str,
    rate: f32,
    volume: f32,
    output_device_id: &str,
    config: &Qwen3TtsBenchmarkConfig,
) -> Result<(), String> {
    let (bytes, content_type) = request_qwen3_tts_audio_bytes(text, rate, config)?;
    if bytes.is_empty() {
        return Err("Qwen3-TTS returned an empty response body.".to_string());
    }
    if content_type.contains("application/json") {
        let body = String::from_utf8_lossy(&bytes).trim().to_string();
        return Err(format!(
            "Qwen3-TTS returned JSON instead of audio: {}",
            body
        ));
    }
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!(
            "Qwen3-TTS response is not WAV audio (content-type='{}').",
            content_type
        ));
    }
    crate::multimodal_io::play_wav_bytes(&bytes, volume, output_device_id)
}

fn normalize_tts_benchmark_scenarios(
    requested: &[TtsBenchmarkScenario],
    lock_matrix: bool,
) -> Vec<TtsBenchmarkScenario> {
    if lock_matrix {
        return default_tts_benchmark_scenarios();
    }

    let mut scenarios = Vec::<TtsBenchmarkScenario>::new();
    for (idx, scenario) in requested.iter().enumerate() {
        let text = scenario.text.trim();
        if text.is_empty() {
            continue;
        }
        let id = if scenario.id.trim().is_empty() {
            format!("scenario_{}", idx + 1)
        } else {
            scenario.id.trim().to_lowercase().replace(' ', "_")
        };
        let length_bucket = match scenario.length_bucket.trim().to_ascii_lowercase().as_str() {
            "short" => "short".to_string(),
            "long" => "long".to_string(),
            _ => "short".to_string(),
        };
        let language = match scenario.language.trim().to_ascii_lowercase().as_str() {
            "de" => "de".to_string(),
            "en" => "en".to_string(),
            _ => "en".to_string(),
        };
        let thermal = match scenario.thermal.trim().to_ascii_lowercase().as_str() {
            "cold" => "cold".to_string(),
            "warm" => "warm".to_string(),
            _ => "warm".to_string(),
        };
        scenarios.push(TtsBenchmarkScenario {
            id,
            text: text.to_string(),
            length_bucket,
            language,
            thermal,
        });
    }
    if scenarios.is_empty() {
        default_tts_benchmark_scenarios()
    } else {
        scenarios
    }
}

fn run_tts_provider_once(
    provider: &str,
    text: &str,
    rate: f32,
    volume: f32,
    windows_voice_id: &str,
    piper_binary_path: &str,
    piper_model_path: &str,
    qwen3_config: &Qwen3TtsBenchmarkConfig,
) -> Result<(), String> {
    let selected_windows_voice = windows_voice_id.trim();
    let selected_windows_voice = if selected_windows_voice.is_empty() {
        None
    } else {
        Some(selected_windows_voice)
    };
    match provider {
        "windows_native" => crate::multimodal_io::benchmark_windows_native_synthesis(
            text,
            rate,
            volume,
            selected_windows_voice,
        ),
        "windows_natural" => crate::multimodal_io::benchmark_windows_natural_synthesis(
            text,
            rate,
            volume,
            selected_windows_voice,
        ),
        "local_custom" => crate::multimodal_io::benchmark_piper_synthesis(
            text,
            piper_binary_path,
            piper_model_path,
            rate,
        ),
        "qwen3_tts" => benchmark_qwen3_tts_synthesis(text, rate, qwen3_config),
        _ => Err(format!(
            "Unsupported TTS benchmark provider '{}'.",
            provider
        )),
    }
}

fn run_tts_runtime_smoke_once(
    state: &AppState,
    provider: &str,
    rate: f32,
    windows_voice_id: &str,
    piper_binary_path: &str,
    piper_model_path: &str,
    output_device_id: &str,
) -> Result<(), String> {
    let smoke_text = "Trispr Flow runtime smoke test.";
    let selected_windows_voice = windows_voice_id.trim();
    let selected_windows_voice = if selected_windows_voice.is_empty() {
        None
    } else {
        Some(selected_windows_voice)
    };
    match provider {
        "windows_native" => crate::multimodal_io::speak_windows_native(
            smoke_text,
            rate,
            0.0,
            output_device_id,
            selected_windows_voice,
        ),
        "windows_natural" => crate::multimodal_io::speak_windows_natural(
            smoke_text,
            rate,
            0.0,
            output_device_id,
            selected_windows_voice,
        ),
        "local_custom" => crate::multimodal_io::speak_piper(
            &state.piper_daemon,
            smoke_text,
            piper_binary_path,
            piper_model_path,
            rate,
            0.0,
            output_device_id,
        ),
        _ => Err(format!(
            "Runtime smoke is unsupported for benchmark-only provider '{}'.",
            provider
        )),
    }
}

fn summarize_tts_provider(
    provider: &str,
    samples: &[TtsBenchmarkSample],
) -> TtsBenchmarkProviderSummary {
    let mut latencies: Vec<u64> = samples
        .iter()
        .filter(|sample| sample.success)
        .map(|sample| sample.elapsed_ms)
        .collect();
    latencies.sort_unstable();
    let attempts = samples.len() as u32;
    let success_count = latencies.len() as u32;
    let failure_count = attempts.saturating_sub(success_count);
    let success_rate = if attempts == 0 {
        0.0
    } else {
        success_count as f32 / attempts as f32
    };
    let avg_ms = if latencies.is_empty() {
        None
    } else {
        Some(latencies.iter().sum::<u64>() / latencies.len() as u64)
    };

    TtsBenchmarkProviderSummary {
        provider: provider.to_string(),
        attempts,
        success_count,
        failure_count,
        success_rate,
        p50_ms: if latencies.is_empty() {
            None
        } else {
            Some(percentile(&latencies, 0.50))
        },
        p95_ms: if latencies.is_empty() {
            None
        } else {
            Some(percentile(&latencies, 0.95))
        },
        avg_ms,
    }
}

fn build_tts_fallback_order(
    summaries: &[TtsBenchmarkProviderSummary],
    reliability_gate: f32,
) -> Vec<String> {
    let mut eligible: Vec<&TtsBenchmarkProviderSummary> = summaries
        .iter()
        .filter(|summary| summary.success_rate >= reliability_gate && summary.p95_ms.is_some())
        .collect();
    eligible.sort_by(|a, b| {
        a.p95_ms
            .unwrap_or(u64::MAX)
            .cmp(&b.p95_ms.unwrap_or(u64::MAX))
            .then_with(|| {
                a.p50_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.p50_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| a.provider.cmp(&b.provider))
    });

    let mut fallback_sorted: Vec<&TtsBenchmarkProviderSummary> = summaries.iter().collect();
    fallback_sorted.sort_by(|a, b| {
        b.success_rate
            .partial_cmp(&a.success_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.failure_count.cmp(&b.failure_count))
            .then_with(|| {
                a.p95_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.p95_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| {
                a.p50_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.p50_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| a.provider.cmp(&b.provider))
    });

    let mut order: Vec<String> = Vec::new();
    for item in eligible.into_iter().chain(fallback_sorted.into_iter()) {
        if !order.iter().any(|provider| provider == &item.provider) {
            order.push(item.provider.clone());
        }
    }
    order
}

fn scenario_success_counts_for_provider(
    provider: &str,
    scenarios: &[TtsBenchmarkScenario],
    samples: &[TtsBenchmarkSample],
) -> HashMap<String, u32> {
    let mut out = HashMap::<String, u32>::new();
    for scenario in scenarios {
        let success = samples
            .iter()
            .filter(|sample| {
                sample.provider == provider && sample.scenario == scenario.id && sample.success
            })
            .count() as u32;
        out.insert(scenario.id.clone(), success);
    }
    out
}

fn provider_consistency_from_runtime_surface(
    providers: &[String],
    qwen3_tts_enabled: bool,
) -> (bool, String) {
    let runtime_surface = crate::multimodal_io::list_tts_providers(qwen3_tts_enabled)
        .into_iter()
        .map(|info| (info.id, info.surface))
        .collect::<HashMap<_, _>>();

    let mut mismatches: Vec<String> = Vec::new();
    for provider in providers {
        if let Some(surface) = runtime_surface.get(provider) {
            if provider == "qwen3_tts" && surface != TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL {
                mismatches.push(format!(
                    "{} should be '{}' in runtime surface, got '{}'",
                    provider, TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL, surface
                ));
            }
            if provider != "qwen3_tts" && surface != TTS_PROVIDER_SURFACE_RUNTIME_STABLE {
                mismatches.push(format!(
                    "{} should be '{}' in runtime surface, got '{}'",
                    provider, TTS_PROVIDER_SURFACE_RUNTIME_STABLE, surface
                ));
            }
        } else {
            mismatches.push(format!(
                "{} missing from runtime provider exposure list",
                provider
            ));
        }
    }

    if mismatches.is_empty() {
        (
            true,
            "Benchmark scope and runtime provider surface are consistent.".to_string(),
        )
    } else {
        (false, mismatches.join(" | "))
    }
}

#[cfg(test)]
mod tts_benchmark_tests {
    use super::{
        build_tts_fallback_order, classify_tts_failure, normalize_tts_benchmark_providers,
        TtsBenchmarkProviderSummary, TTS_FAILURE_AUTH_MISSING, TTS_FAILURE_ENDPOINT_UNREACHABLE,
        TTS_FAILURE_MISSING_BINARY, TTS_FAILURE_MISSING_MODEL, TTS_FAILURE_RUNTIME_ERROR,
        TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED,
    };

    #[test]
    fn fallback_order_prefers_reliability_gate_then_latency() {
        let summaries = vec![
            TtsBenchmarkProviderSummary {
                provider: "windows_native".to_string(),
                attempts: 9,
                success_count: 9,
                failure_count: 0,
                success_rate: 1.0,
                p50_ms: Some(190),
                p95_ms: Some(290),
                avg_ms: Some(210),
            },
            TtsBenchmarkProviderSummary {
                provider: "local_custom".to_string(),
                attempts: 9,
                success_count: 9,
                failure_count: 0,
                success_rate: 1.0,
                p50_ms: Some(170),
                p95_ms: Some(240),
                avg_ms: Some(185),
            },
        ];

        let order = build_tts_fallback_order(&summaries, 0.95);
        assert_eq!(
            order,
            vec!["local_custom".to_string(), "windows_native".to_string()]
        );
    }

    #[test]
    fn fallback_order_still_returns_best_available_when_gate_not_met() {
        let summaries = vec![
            TtsBenchmarkProviderSummary {
                provider: "windows_native".to_string(),
                attempts: 9,
                success_count: 7,
                failure_count: 2,
                success_rate: 7.0 / 9.0,
                p50_ms: Some(210),
                p95_ms: Some(330),
                avg_ms: Some(230),
            },
            TtsBenchmarkProviderSummary {
                provider: "local_custom".to_string(),
                attempts: 9,
                success_count: 5,
                failure_count: 4,
                success_rate: 5.0 / 9.0,
                p50_ms: Some(260),
                p95_ms: Some(390),
                avg_ms: Some(280),
            },
        ];

        let order = build_tts_fallback_order(&summaries, 0.95);
        assert_eq!(order.first().map(String::as_str), Some("windows_native"));
    }

    #[test]
    fn classifies_tts_failures_into_fixed_categories() {
        assert_eq!(
            classify_tts_failure("Piper TTS binary not found."),
            TTS_FAILURE_MISSING_BINARY.to_string()
        );
        assert_eq!(
            classify_tts_failure("Piper model not found: D:\\voices\\de.onnx"),
            TTS_FAILURE_MISSING_MODEL.to_string()
        );
        assert_eq!(
            classify_tts_failure("Qwen3-TTS benchmark request failed: connection refused"),
            TTS_FAILURE_ENDPOINT_UNREACHABLE.to_string()
        );
        assert_eq!(
            classify_tts_failure("Qwen3-TTS benchmark request failed with HTTP 401"),
            TTS_FAILURE_AUTH_MISSING.to_string()
        );
        assert_eq!(
            classify_tts_failure(
                "[tts_output_stream_config_unsupported] device='wasapi:xyz' wav=22050Hz/1ch/int16 -> target=48000Hz/2ch/f32 reason=The requested stream configuration is not supported by the device."
            ),
            TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED.to_string()
        );
        assert_eq!(
            classify_tts_failure("unexpected panic in voice backend"),
            TTS_FAILURE_RUNTIME_ERROR.to_string()
        );
    }

    #[test]
    fn provider_normalization_accepts_windows_natural_qwen3_and_deduplicates() {
        let input = vec![
            " windows_native ".to_string(),
            "windows_natural".to_string(),
            "qwen3_tts".to_string(),
            "local_custom".to_string(),
            "QWEN3_TTS".to_string(),
            "unsupported".to_string(),
        ];
        let providers = normalize_tts_benchmark_providers(&input);
        assert_eq!(
            providers,
            vec![
                "windows_native".to_string(),
                "windows_natural".to_string(),
                "qwen3_tts".to_string(),
                "local_custom".to_string(),
            ]
        );
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

fn run_latency_benchmark_inner(
    app: &AppHandle,
    state: &AppState,
    request: &LatencyBenchmarkRequest,
) -> Result<LatencyBenchmarkResult, String> {
    let warmup_runs = request.warmup_runs.min(10);
    let measure_runs = request.measure_runs.clamp(1, 200);
    let include_refinement = request.include_refinement;

    let fixture_paths: Vec<PathBuf> = if request.fixture_paths.is_empty() {
        default_latency_fixture_paths()
    } else {
        // Validate user-provided paths against app data directory
        let allowed_root = crate::paths::resolve_base_dir(&app);
        let mut validated = Vec::new();
        for path_str in &request.fixture_paths {
            validated.push(validate_path_within(path_str, &allowed_root)?);
        }
        validated
    };

    if fixture_paths.is_empty() {
        return Err(
            "No benchmark fixtures found. Add WAV files under bench/fixtures/short/.".to_string(),
        );
    }

    let mut fixtures: Vec<(String, Vec<i16>)> = Vec::new();
    for path in fixture_paths {
        let samples = read_wav_for_latency_benchmark(&path)?;
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.display().to_string());
        fixtures.push((label, samples));
    }

    let mut settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if include_refinement {
        if let Some(model) = request
            .refinement_model
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            let model = model.to_string();
            settings_snapshot.ai_fallback.enabled = true;
            settings_snapshot.ai_fallback.provider = "ollama".to_string();
            settings_snapshot.ai_fallback.execution_mode = "local_primary".to_string();
            settings_snapshot.ai_fallback.model = model.clone();
            settings_snapshot.postproc_llm_model = model.clone();
            settings_snapshot.providers.ollama.preferred_model = model;
        }
    }
    let active_refinement_model = if include_refinement && settings_snapshot.ai_fallback.enabled {
        Some(settings_snapshot.ai_fallback.model.clone())
    } else {
        None
    };
    let mut samples: Vec<LatencyBenchmarkSample> = Vec::with_capacity(measure_runs as usize);
    let mut warnings: Vec<String> = Vec::new();
    let total_runs = warmup_runs + measure_runs;

    for run_idx in 0..total_runs {
        let fixture_idx = run_idx as usize % fixtures.len();
        let (fixture_name, fixture_samples) = (&fixtures[fixture_idx].0, &fixtures[fixture_idx].1);

        let whisper_started = Instant::now();
        let (raw_text, _source) = transcribe_audio(app, &settings_snapshot, fixture_samples)?;
        let whisper_ms = whisper_started.elapsed().as_millis() as u64;

        let mut refine_ms = 0u64;
        let mut mode = "raw".to_string();
        let mut refinement_model_used = active_refinement_model.clone();
        if include_refinement && settings_snapshot.ai_fallback.enabled {
            let refine_started = Instant::now();
            match refine_transcript_for_benchmark(app, &settings_snapshot, &raw_text) {
                Ok(result) => {
                    refine_ms = refine_started.elapsed().as_millis() as u64;
                    mode = "refined".to_string();
                    refinement_model_used = Some(result.model);
                }
                Err(error) => {
                    refine_ms = refine_started.elapsed().as_millis() as u64;
                    mode = if error.to_lowercase().contains("timed out") {
                        "fallback_timeout".to_string()
                    } else {
                        "fallback_error".to_string()
                    };
                    warnings.push(format!("{}: {}", fixture_name, error));
                }
            }
        }

        if run_idx < warmup_runs {
            continue;
        }

        let total_ms = whisper_ms.saturating_add(refine_ms);
        samples.push(LatencyBenchmarkSample {
            fixture: fixture_name.clone(),
            whisper_ms,
            refine_ms,
            total_ms,
            mode,
            accelerator: last_transcription_accelerator().to_string(),
            refinement_model: refinement_model_used,
        });
    }

    let mut totals: Vec<u64> = samples.iter().map(|sample| sample.total_ms).collect();
    totals.sort_unstable();
    let p50_ms = percentile(&totals, 0.50);
    let p95_ms = percentile(&totals, 0.95);
    let slo_p50_ms = 2_500;
    let slo_p95_ms = 4_000;
    let slo_pass = p50_ms <= slo_p50_ms && p95_ms <= slo_p95_ms;

    Ok(LatencyBenchmarkResult {
        warmup_runs,
        measure_runs,
        p50_ms,
        p95_ms,
        slo_p50_ms,
        slo_p95_ms,
        slo_pass,
        samples,
        warnings,
    })
}

fn write_latency_benchmark_report(result: &LatencyBenchmarkResult) -> Result<PathBuf, String> {
    let root = resolve_benchmark_root_dir();
    let out_dir = root.join("bench").join("results");
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        format!(
            "Failed creating benchmark output dir '{}': {}",
            out_dir.display(),
            e
        )
    })?;
    let out_path = out_dir.join("latest.json");
    let serialized = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, serialized).map_err(|e| {
        format!(
            "Failed writing benchmark report '{}': {}",
            out_path.display(),
            e
        )
    })?;
    Ok(out_path)
}

fn run_tts_benchmark_inner(
    state: &AppState,
    request: &TtsBenchmarkRequest,
) -> Result<TtsBenchmarkResult, String> {
    let warmup_runs = request.warmup_runs.min(5);
    let measure_runs = request.measure_runs.clamp(3, 100);
    let gates = default_tts_benchmark_gates();
    let providers = normalize_tts_benchmark_providers(&request.providers);
    let scenarios = normalize_tts_benchmark_scenarios(&request.scenarios, request.lock_matrix);
    let qwen3_config = resolve_qwen3_tts_benchmark_config(request);
    let rate = if request.rate.is_finite() {
        request.rate.clamp(0.5, 2.0)
    } else {
        1.0
    };
    let volume = if request.volume.is_finite() {
        request.volume.clamp(0.0, 1.0)
    } else {
        1.0
    };

    let voice_settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .voice_output_settings
        .clone();
    let piper_binary_path = request
        .piper_binary_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| voice_settings.piper_binary_path.clone());
    let piper_model_path = request
        .piper_model_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| voice_settings.piper_model_path.clone());
    let piper_model_dir = voice_settings.piper_model_dir.clone();

    let mut samples: Vec<TtsBenchmarkSample> = Vec::new();
    let mut preflight_checks: Vec<TtsPreflightCheck> = Vec::new();
    let mut runtime_smoke_checks: Vec<TtsRuntimeSmokeCheck> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let provider_profiles = providers
        .iter()
        .map(|provider| tts_provider_profile(provider))
        .collect::<Vec<_>>();

    for provider in &providers {
        let mut provider_preflight: Vec<TtsPreflightCheck> = Vec::new();
        if is_runtime_stable_provider(provider) {
            let module_enabled = voice_settings.enabled;
            provider_preflight.push(TtsPreflightCheck {
                provider: provider.clone(),
                check: "module_enabled".to_string(),
                passed: module_enabled,
                category: if module_enabled {
                    None
                } else {
                    Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                },
                detail: if module_enabled {
                    "Voice output module is enabled.".to_string()
                } else {
                    "Voice output module is disabled. Enable module 'output_voice_tts' before release benchmarking."
                        .to_string()
                },
            });
        }
        match provider.as_str() {
            "windows_native" => {
                let passed = cfg!(target_os = "windows");
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "platform".to_string(),
                    passed,
                    category: if passed {
                        None
                    } else {
                        Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                    },
                    detail: if passed {
                        "Windows runtime detected for windows_native provider.".to_string()
                    } else {
                        "windows_native provider requires Windows runtime.".to_string()
                    },
                });
            }
            "windows_natural" => {
                let platform_ok = cfg!(target_os = "windows");
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "platform".to_string(),
                    passed: platform_ok,
                    category: if platform_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                    },
                    detail: if platform_ok {
                        "Windows runtime detected for windows_natural provider.".to_string()
                    } else {
                        "windows_natural provider requires Windows runtime.".to_string()
                    },
                });

                let natural_voices_ok = crate::multimodal_io::windows_natural_voice_available();
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "natural_voice".to_string(),
                    passed: natural_voices_ok,
                    category: if natural_voices_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                    },
                    detail: if natural_voices_ok {
                        "Detected Windows Natural voice(s) via SAPI.".to_string()
                    } else {
                        "No Windows Natural voice detected. Install NaturalVoiceSAPIAdapter and a Natural voice pack."
                            .to_string()
                    },
                });
            }
            "local_custom" => {
                let binary_preflight =
                    crate::multimodal_io::piper_binary_preflight(&piper_binary_path);
                let binary_ok = binary_preflight.is_ok();
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "binary".to_string(),
                    passed: binary_ok,
                    category: if binary_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_MISSING_BINARY.to_string())
                    },
                    detail: binary_preflight
                        .map(|_| "Piper runtime resolved.".to_string())
                        .unwrap_or_else(|error| error),
                });

                let model_ok = crate::multimodal_io::piper_model_available(
                    &piper_model_path,
                    &piper_model_dir,
                );
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "model".to_string(),
                    passed: model_ok,
                    category: if model_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_MISSING_MODEL.to_string())
                    },
                    detail: if model_ok {
                        "Piper model resolved.".to_string()
                    } else {
                        "Piper model not found. Configure piper_model_path or provide a voices directory."
                            .to_string()
                    },
                });
            }
            "qwen3_tts" => {
                let endpoint_ok = qwen3_config.endpoint.starts_with("http://")
                    || qwen3_config.endpoint.starts_with("https://");
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "endpoint_format".to_string(),
                    passed: endpoint_ok,
                    category: if endpoint_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_ENDPOINT_UNREACHABLE.to_string())
                    },
                    detail: if endpoint_ok {
                        "Qwen3 endpoint format accepted.".to_string()
                    } else {
                        format!(
                            "Qwen3 endpoint '{}' is invalid. Expected http:// or https:// URL.",
                            qwen3_config.endpoint
                        )
                    },
                });

                if endpoint_ok {
                    let probe =
                        benchmark_qwen3_tts_synthesis("Preflight ping.", 1.0, &qwen3_config);
                    provider_preflight.push(TtsPreflightCheck {
                        provider: provider.clone(),
                        check: "endpoint_auth_probe".to_string(),
                        passed: probe.is_ok(),
                        category: probe
                            .as_ref()
                            .err()
                            .map(|error| classify_tts_failure(error)),
                        detail: match probe {
                            Ok(()) => "Qwen3 endpoint/auth probe succeeded.".to_string(),
                            Err(error) => format!("Qwen3 probe failed: {}", error),
                        },
                    });
                }
            }
            _ => {
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "provider".to_string(),
                    passed: false,
                    category: Some(TTS_FAILURE_RUNTIME_ERROR.to_string()),
                    detail: format!("Unsupported benchmark provider '{}'.", provider),
                });
            }
        }

        let preflight_ok = provider_preflight.iter().all(|check| check.passed);
        preflight_checks.extend(provider_preflight.clone());

        if is_runtime_stable_provider(provider) {
            if request.run_runtime_smoke {
                if preflight_ok {
                    match run_tts_runtime_smoke_once(
                        state,
                        provider,
                        rate,
                        &voice_settings.voice_id_windows,
                        &piper_binary_path,
                        &piper_model_path,
                        &voice_settings.output_device,
                    ) {
                        Ok(()) => runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                            provider: provider.clone(),
                            passed: true,
                            category: None,
                            detail: "Runtime smoke speak path succeeded.".to_string(),
                        }),
                        Err(error) => runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                            provider: provider.clone(),
                            passed: false,
                            category: Some(classify_tts_failure(&error)),
                            detail: format!("Runtime smoke speak path failed: {}", error),
                        }),
                    }
                } else {
                    runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                        provider: provider.clone(),
                        passed: false,
                        category: Some(TTS_FAILURE_RUNTIME_ERROR.to_string()),
                        detail: "Runtime smoke skipped due to preflight failure.".to_string(),
                    });
                }
            } else {
                runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                    provider: provider.clone(),
                    passed: true,
                    category: None,
                    detail: "Runtime smoke disabled by request.".to_string(),
                });
            }
        }

        if !preflight_ok {
            let first_failed = provider_preflight
                .iter()
                .find(|check| !check.passed)
                .cloned()
                .unwrap_or(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "unknown".to_string(),
                    passed: false,
                    category: Some(TTS_FAILURE_RUNTIME_ERROR.to_string()),
                    detail: "Unknown preflight failure.".to_string(),
                });
            warnings.push(format!(
                "provider={} preflight failed check={} category={} detail={}",
                provider,
                first_failed.check,
                first_failed
                    .category
                    .clone()
                    .unwrap_or_else(|| TTS_FAILURE_RUNTIME_ERROR.to_string()),
                first_failed.detail
            ));
            for scenario in &scenarios {
                for run in 1..=measure_runs {
                    samples.push(TtsBenchmarkSample {
                        provider: provider.clone(),
                        scenario: scenario.id.clone(),
                        run,
                        elapsed_ms: 0,
                        success: false,
                        error: Some(format!(
                            "Preflight failed ({}): {}",
                            first_failed.check, first_failed.detail
                        )),
                        failure_category: Some(
                            first_failed
                                .category
                                .clone()
                                .unwrap_or_else(|| TTS_FAILURE_RUNTIME_ERROR.to_string()),
                        ),
                    });
                }
            }
            continue;
        }

        for scenario in &scenarios {
            let scenario_warmup = if scenario.thermal == "warm" {
                warmup_runs
            } else {
                0
            };
            for run_idx in 0..(scenario_warmup + measure_runs) {
                let started = Instant::now();
                let outcome = run_tts_provider_once(
                    provider,
                    &scenario.text,
                    rate,
                    volume,
                    &voice_settings.voice_id_windows,
                    &piper_binary_path,
                    &piper_model_path,
                    &qwen3_config,
                );
                let elapsed_ms = started.elapsed().as_millis() as u64;

                if run_idx < scenario_warmup {
                    continue;
                }

                let run = run_idx - scenario_warmup + 1;
                match outcome {
                    Ok(()) => samples.push(TtsBenchmarkSample {
                        provider: provider.clone(),
                        scenario: scenario.id.clone(),
                        run,
                        elapsed_ms,
                        success: true,
                        error: None,
                        failure_category: None,
                    }),
                    Err(error) => {
                        let category = classify_tts_failure(&error);
                        warnings.push(format!(
                            "provider={} scenario={} run={} category={} error={}",
                            provider, scenario.id, run, category, error
                        ));
                        samples.push(TtsBenchmarkSample {
                            provider: provider.clone(),
                            scenario: scenario.id.clone(),
                            run,
                            elapsed_ms,
                            success: false,
                            error: Some(error),
                            failure_category: Some(category),
                        });
                    }
                }
            }
        }
    }

    let mut grouped: HashMap<String, Vec<TtsBenchmarkSample>> = HashMap::new();
    for sample in &samples {
        grouped
            .entry(sample.provider.clone())
            .or_default()
            .push(sample.clone());
    }

    if providers.iter().any(|provider| provider == "qwen3_tts") {
        warnings.push(format!(
            "provider=qwen3_tts config endpoint={} model={} voice={} timeout_sec={}",
            qwen3_config.endpoint, qwen3_config.model, qwen3_config.voice, qwen3_config.timeout_sec
        ));
    }
    if providers.iter().any(|provider| provider == "local_custom") {
        warnings.push(format!(
            "provider=local_custom config piper_binary_path={} piper_model_path={}",
            if piper_binary_path.trim().is_empty() {
                "<auto-resolve>"
            } else {
                piper_binary_path.as_str()
            },
            if piper_model_path.trim().is_empty() {
                "<auto-resolve>"
            } else {
                piper_model_path.as_str()
            }
        ));
    }

    let provider_summaries = providers
        .iter()
        .map(|provider| {
            let provider_samples = grouped.get(provider).cloned().unwrap_or_default();
            summarize_tts_provider(provider, &provider_samples)
        })
        .collect::<Vec<_>>();

    let preflight_ok_by_provider = providers
        .iter()
        .map(|provider| {
            let ok = preflight_checks
                .iter()
                .filter(|check| check.provider == *provider)
                .all(|check| check.passed);
            (provider.clone(), ok)
        })
        .collect::<HashMap<_, _>>();
    let smoke_ok_by_provider = runtime_smoke_checks
        .iter()
        .map(|check| (check.provider.clone(), check.passed))
        .collect::<HashMap<_, _>>();

    let provider_gate_evaluations = providers
        .iter()
        .map(|provider| {
            let summary = provider_summaries
                .iter()
                .find(|summary| summary.provider == *provider)
                .cloned()
                .unwrap_or(TtsBenchmarkProviderSummary {
                    provider: provider.clone(),
                    attempts: 0,
                    success_count: 0,
                    failure_count: 0,
                    success_rate: 0.0,
                    p50_ms: None,
                    p95_ms: None,
                    avg_ms: None,
                });
            let scenario_success =
                scenario_success_counts_for_provider(provider, &scenarios, &samples);
            let min_success_in_any_scenario = scenario_success.values().copied().min().unwrap_or(0);
            let evaluated_for_release = is_runtime_stable_provider(provider);
            let preflight_ok = *preflight_ok_by_provider.get(provider).unwrap_or(&false);
            let runtime_smoke_ok = if evaluated_for_release {
                *smoke_ok_by_provider.get(provider).unwrap_or(&false)
            } else {
                true
            };
            let reliability_ok = summary.success_rate >= gates.reliability_min_success_rate;
            let latency_ok = summary
                .p50_ms
                .map(|value| value <= gates.latency_target_p50_ms)
                .unwrap_or(false)
                && summary
                    .p95_ms
                    .map(|value| value <= gates.latency_target_p95_ms)
                    .unwrap_or(false);
            let scenario_success_ok = min_success_in_any_scenario >= gates.min_success_per_scenario;
            let passes_release_gate = evaluated_for_release
                && preflight_ok
                && runtime_smoke_ok
                && reliability_ok
                && latency_ok
                && scenario_success_ok;
            TtsProviderGateEvaluation {
                provider: provider.clone(),
                evaluated_for_release,
                passes_release_gate,
                preflight_ok,
                runtime_smoke_ok,
                reliability_ok,
                latency_ok,
                scenario_success_ok,
                success_rate: summary.success_rate,
                p50_ms: summary.p50_ms,
                p95_ms: summary.p95_ms,
                min_success_in_any_scenario,
            }
        })
        .collect::<Vec<_>>();

    let (provider_consistency_ok, provider_consistency_detail) =
        provider_consistency_from_runtime_surface(&providers, true); // TODO: pass qwen3_tts_enabled from settings if this function gets State access

    let release_evaluations = provider_gate_evaluations
        .iter()
        .filter(|evaluation| evaluation.evaluated_for_release)
        .collect::<Vec<_>>();
    let release_gate_pass = !release_evaluations.is_empty()
        && release_evaluations
            .iter()
            .all(|evaluation| evaluation.passes_release_gate);
    let release_gate_reason = if release_gate_pass {
        "All runtime-stable providers passed release gates.".to_string()
    } else if release_evaluations.is_empty() {
        "No runtime-stable providers available for release gate evaluation.".to_string()
    } else {
        let failed = release_evaluations
            .iter()
            .filter(|evaluation| !evaluation.passes_release_gate)
            .map(|evaluation| evaluation.provider.clone())
            .collect::<Vec<_>>();
        format!("Release gate failed for providers: {}", failed.join(", "))
    };

    let runtime_summaries = provider_summaries
        .iter()
        .filter(|summary| is_runtime_stable_provider(&summary.provider))
        .cloned()
        .collect::<Vec<_>>();
    let fallback_order =
        build_tts_fallback_order(&runtime_summaries, gates.reliability_min_success_rate);

    let recommended_default_provider = if release_gate_pass {
        fallback_order.first().cloned()
    } else {
        fallback_order
            .iter()
            .find(|provider| {
                provider_gate_evaluations
                    .iter()
                    .find(|evaluation| &evaluation.provider == *provider)
                    .map(|evaluation| {
                        evaluation.preflight_ok
                            && evaluation.runtime_smoke_ok
                            && evaluation.success_rate > 0.0
                    })
                    .unwrap_or(false)
            })
            .cloned()
    };
    let recommendation_reason = if let Some(provider) = recommended_default_provider.as_ref() {
        if release_gate_pass {
            format!(
                "Selected '{}' as default (release gate pass; deterministic fallback order applied).",
                provider
            )
        } else {
            format!(
                "Selected '{}' as best available runtime fallback while release gate is failing.",
                provider
            )
        }
    } else {
        "No runtime provider recommendation available. Resolve preflight/smoke failures first."
            .to_string()
    };

    let uncategorized_failure_count = samples
        .iter()
        .filter(|sample| !sample.success && sample.failure_category.is_none())
        .count() as u32;

    Ok(TtsBenchmarkResult {
        artifact_version: "tts-benchmark-v2".to_string(),
        generated_at: now_iso(),
        warmup_runs,
        measure_runs,
        providers,
        scenarios: scenarios
            .iter()
            .map(|scenario| scenario.id.clone())
            .collect(),
        scenario_matrix_locked: request.lock_matrix,
        gates,
        provider_profiles,
        preflight_checks,
        runtime_smoke_checks,
        samples,
        provider_summaries,
        provider_gate_evaluations,
        provider_consistency_ok,
        provider_consistency_detail,
        fallback_order,
        release_gate_pass,
        release_gate_reason,
        recommended_default_provider,
        recommendation_reason,
        uncategorized_failure_count,
        warnings,
    })
}

fn write_tts_benchmark_report(result: &TtsBenchmarkResult) -> Result<PathBuf, String> {
    let root = resolve_benchmark_root_dir();
    let out_dir = root.join("bench").join("results");
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        format!(
            "Failed creating benchmark output dir '{}': {}",
            out_dir.display(),
            e
        )
    })?;
    let out_path = out_dir.join("tts.latest.json");
    let serialized = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, serialized).map_err(|e| {
        format!(
            "Failed writing TTS benchmark report '{}': {}",
            out_path.display(),
            e
        )
    })?;
    Ok(out_path)
}

fn default_latency_fixture_paths() -> Vec<PathBuf> {
    let root = resolve_benchmark_root_dir();
    let fixture_dir = root.join("bench").join("fixtures").join("short");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&fixture_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_wav = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("wav"))
                .unwrap_or(false);
            if is_wav {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn resolve_benchmark_root_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("bench").is_dir() {
        return cwd;
    }

    let mut candidate = cwd.clone();
    for _ in 0..4 {
        if let Some(parent) = candidate.parent() {
            if parent.join("bench").is_dir() {
                return parent.to_path_buf();
            }
            candidate = parent.to_path_buf();
        } else {
            break;
        }
    }

    cwd
}

fn read_wav_for_latency_benchmark(path: &Path) -> Result<Vec<i16>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV fixture '{}': {}", path.display(), e))?;
    let spec = reader.spec();
    if spec.sample_rate != crate::constants::TARGET_SAMPLE_RATE {
        return Err(format!(
            "Fixture '{}' uses unsupported sample rate {} (expected {}).",
            path.display(),
            spec.sample_rate,
            crate::constants::TARGET_SAMPLE_RATE
        ));
    }

    let channels = spec.channels.max(1) as usize;
    let mut mono = Vec::<i16>::new();

    match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample != 16 {
                return Err(format!(
                    "Fixture '{}' must be 16-bit PCM for benchmark (got {} bits).",
                    path.display(),
                    spec.bits_per_sample
                ));
            }
            let samples: Vec<i16> = reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed reading fixture '{}': {}", path.display(), e))?;
            for frame in samples.chunks(channels) {
                if let Some(first) = frame.first() {
                    mono.push(*first);
                }
            }
        }
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed reading float fixture '{}': {}", path.display(), e))?;
            for frame in samples.chunks(channels) {
                if let Some(first) = frame.first() {
                    let clamped = first.clamp(-1.0, 1.0);
                    mono.push((clamped * i16::MAX as f32) as i16);
                }
            }
        }
    }

    if mono.is_empty() {
        return Err(format!(
            "Fixture '{}' has no audio samples.",
            path.display()
        ));
    }
    Ok(mono)
}

fn percentile(sorted_values: &[u64], quantile: f64) -> u64 {
    if sorted_values.is_empty() {
        return 0;
    }
    let q = quantile.clamp(0.0, 1.0);
    let idx = ((sorted_values.len() - 1) as f64 * q).round() as usize;
    sorted_values[idx]
}

fn refine_transcript_for_benchmark(
    app: &AppHandle,
    settings_snapshot: &Settings,
    transcript: &str,
) -> Result<crate::ai_fallback::models::RefinementResult, String> {
    let setup = prepare_refinement(app, settings_snapshot)?;

    setup
        .provider
        .refine_transcript(transcript, &setup.model, &setup.options, &setup.api_key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn run_latency_benchmark(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<LatencyBenchmarkRequest>,
) -> Result<LatencyBenchmarkResult, String> {
    let request = request.unwrap_or_default();
    run_latency_benchmark_inner(&app, state.inner(), &request)
}

#[tauri::command]
fn run_tts_benchmark(
    state: State<'_, AppState>,
    request: Option<TtsBenchmarkRequest>,
) -> Result<TtsBenchmarkResult, String> {
    let request = request.unwrap_or_default();
    run_tts_benchmark_inner(state.inner(), &request)
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

#[tauri::command]
fn frontend_heartbeat(state: State<'_, AppState>) {
    state
        .frontend_last_heartbeat_ms
        .store(crate::util::now_ms(), Ordering::Relaxed);
}

#[tauri::command]
fn log_frontend_event(level: String, context: String, message: String) -> Result<(), String> {
    let normalized_context = context.trim();
    let normalized_message = message.trim();
    if normalized_message.is_empty() {
        return Ok(());
    }
    match level.trim().to_ascii_lowercase().as_str() {
        "error" => error!(
            "[frontend:{}] {}",
            if normalized_context.is_empty() {
                "unknown"
            } else {
                normalized_context
            },
            normalized_message
        ),
        "warn" => warn!(
            "[frontend:{}] {}",
            if normalized_context.is_empty() {
                "unknown"
            } else {
                normalized_context
            },
            normalized_message
        ),
        _ => info!(
            "[frontend:{}] {}",
            if normalized_context.is_empty() {
                "unknown"
            } else {
                normalized_context
            },
            normalized_message
        ),
    }
    Ok(())
}

fn set_transcribe_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = {
        let mut current = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if current.transcribe_enabled == enabled {
            return Ok(());
        }
        current.transcribe_enabled = enabled;
        current.clone()
    };

    if enabled {
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
        stop_transcribe_monitor(app, &state);
    }

    let _ = app.emit("settings-changed", settings.clone());
    let _ = app.emit("menu:update-transcribe", enabled);
    Ok(())
}

#[tauri::command]
fn pull_ollama_model(
    app: AppHandle,
    state: State<'_, AppState>,
    model: String,
) -> Result<(), String> {
    use crate::ai_fallback::provider::{
        precheck_ollama_registry_model_tag, pull_ollama_model_inner, validate_ollama_model_name,
    };

    validate_ollama_model_name(&model)?;

    // Prevent duplicate pulls for the same model
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
        if let Err(e) = check_strict_local_mode(&settings) {
            let mut pulls = state
                .ollama_pulls
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            pulls.remove(&model);
            return Err(e);
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

    // Drop-Guard ensures the model is removed from ollama_pulls even if the
    // thread panics (e.g. due to a bug in pull_ollama_model_inner).
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
async fn delete_ollama_model(state: State<'_, AppState>, model: String) -> Result<(), String> {
    use crate::ai_fallback::provider::validate_ollama_model_name;

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
    use crate::ai_fallback::provider::ollama_endpoint_candidates;

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
            Err(ureq::Error::Transport(t)) => {
                last_transport_error = Some(t.to_string());
                continue;
            }
            Err(e) => return Err(format!("Failed to delete model: {}", e)),
        }
    }

    Err(format!(
        "Failed to delete model: {}",
        last_transport_error.unwrap_or_else(|| "unable to reach Ollama endpoint".to_string())
    ))
}

#[tauri::command]
async fn get_ollama_model_info(
    state: State<'_, AppState>,
    model: String,
) -> Result<serde_json::Value, String> {
    use crate::ai_fallback::provider::validate_ollama_model_name;

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
    use crate::ai_fallback::provider::ollama_endpoint_candidates;

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
            Ok(r) => r,
            Err(ureq::Error::Transport(t)) => {
                last_transport_error = Some(t.to_string());
                continue;
            }
            Err(e) => return Err(format!("Failed to get model info: {}", e)),
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

/// Synchronous core of save_settings — used by both the async Tauri command
/// and internal callers (e.g. tray menu handlers) that cannot await.
fn save_settings_inner(app: &AppHandle, settings: &mut Settings) -> Result<(), String> {
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
    normalize_product_mode_field(settings);
    normalize_module_settings(&mut settings.module_settings);
    normalize_ai_refinement_module_binding(settings);
    normalize_gdd_module_settings(&mut settings.gdd_module_settings);
    normalize_confluence_settings(&mut settings.confluence_settings);
    normalize_workflow_agent_settings(&mut settings.workflow_agent);
    normalize_vision_input_settings(&mut settings.vision_input_settings);
    normalize_voice_output_settings(&mut settings.voice_output_settings);

    info!("[DIAG] save_settings_inner: acquiring settings lock (write)");
    {
        let mut current = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = settings.clone();
    }
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
            stop_transcribe_monitor(app, &state);
        } else {
            let _ = start_transcribe_monitor(app, &state, settings);
        }
    } else if transcribe_device_changed && settings.transcribe_enabled {
        stop_transcribe_monitor(app, &state);
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
    let _ = emit_assistant_baseline_state(app, state.inner(), settings, "save_settings");
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
fn list_modules(state: State<'_, AppState>) -> Vec<crate::modules::ModuleDescriptor> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    module_registry::modules_as_descriptors(&settings.module_settings)
}

fn should_autostart_ai_refinement_runtime(settings: &Settings) -> bool {
    capability_enabled(settings, RuntimeCapability::AiRefinement)
        && settings.ai_fallback.provider == "ollama"
        && settings.ai_fallback.execution_mode == "local_primary"
}

fn warmup_ai_refinement_model_once(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let setup = prepare_refinement(app, settings)?;
    let warmup_text = "Warmup: local AI refinement runtime.";
    setup
        .provider
        .refine_transcript(warmup_text, &setup.model, &setup.options, &setup.api_key)
        .map(|_| ())
        .map_err(|err| err.to_string())
}

fn schedule_ai_refinement_reenable_bootstrap(app: AppHandle) {
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
        }

        let state = app.state::<AppState>();
        let startup = startup_status_snapshot(state.inner());
        if !startup.ollama_ready {
            warn!("AI refinement runtime warmup skipped: Ollama still not ready after autostart");
            return;
        }

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

fn schedule_piper_daemon_reconcile(
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
        let module_id = module_id.trim().to_string();
        if module_id.is_empty() {
            return Err("Module id cannot be empty.".to_string());
        }

        let grants = grant_permissions.unwrap_or_default();
        let (result, snapshot, descriptors) = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let result =
                module_lifecycle::enable_module(&mut settings.module_settings, &module_id, &grants);
            if result.is_ok() {
                if module_id == "workflow_agent" {
                    settings.workflow_agent.enabled = true;
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
            }
            normalize_module_settings(&mut settings.module_settings);
            normalize_ai_refinement_module_binding(&mut settings);
            normalize_gdd_module_settings(&mut settings.gdd_module_settings);
            normalize_confluence_settings(&mut settings.confluence_settings);
            normalize_workflow_agent_settings(&mut settings.workflow_agent);
            normalize_vision_input_settings(&mut settings.vision_input_settings);
            normalize_voice_output_settings(&mut settings.voice_output_settings);
            let descriptors = module_registry::modules_as_descriptors(&settings.module_settings);
            (result, settings.clone(), descriptors)
        };

        save_settings_file(&app, &snapshot)?;
        if result.is_ok() && module_id == "output_voice_tts" {
            schedule_piper_daemon_reconcile(
                app.clone(),
                snapshot.voice_output_settings.clone(),
                "enable_module",
            );
        }
        let _ = app.emit("settings-changed", snapshot);
        let _ = app.emit("module:state-changed", descriptors);
        let _ = emit_assistant_runtime_state_from_current_settings(
            &app,
            state.inner(),
            "enable_module",
        );
        if result.is_ok() && module_id == AI_REFINEMENT_MODULE_ID {
            schedule_ai_refinement_reenable_bootstrap(app.clone());
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
    })
}

#[tauri::command]
fn disable_module(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: String,
) -> Result<serde_json::Value, String> {
    guarded_command!("disable_module", {
        let module_id = module_id.trim().to_string();
        if module_id.is_empty() {
            return Err("Module id cannot be empty.".to_string());
        }

        let (result, snapshot, descriptors) = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let result =
                module_lifecycle::disable_module(&mut settings.module_settings, &module_id);
            if result.is_ok() {
                if module_id == "workflow_agent" {
                    settings.workflow_agent.enabled = false;
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
            }
            normalize_module_settings(&mut settings.module_settings);
            normalize_ai_refinement_module_binding(&mut settings);
            normalize_gdd_module_settings(&mut settings.gdd_module_settings);
            normalize_confluence_settings(&mut settings.confluence_settings);
            normalize_workflow_agent_settings(&mut settings.workflow_agent);
            normalize_vision_input_settings(&mut settings.vision_input_settings);
            normalize_voice_output_settings(&mut settings.voice_output_settings);
            let descriptors = module_registry::modules_as_descriptors(&settings.module_settings);
            (result, settings.clone(), descriptors)
        };

        if result.is_ok() {
            match module_id.as_str() {
                "input_vision" => {
                    let _ = stop_vision_stream_internal(&app, state.inner());
                }
                "output_voice_tts" => {
                    let _ = stop_tts_internal(&app, state.inner());
                    crate::multimodal_io::shutdown_piper_daemon(state.inner());
                }
                "ai_refinement" => {
                    crate::audio::force_reset_refinement_activity(&app, "forced_reset");

                    let provider = snapshot.ai_fallback.provider.clone();
                    if provider == "ollama" {
                        let app_clone = app.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = stop_ollama_runtime(app_clone).await;
                        });
                    } else if provider == "lm_studio" {
                        crate::util::spawn_guarded("lms_daemon_stop_module_disable", || {
                            lms_daemon_command("stop");
                        });
                    }
                }
                _ => {}
            }
        }

        save_settings_file(&app, &snapshot)?;
        let _ = app.emit("settings-changed", snapshot);
        let _ = app.emit("module:state-changed", descriptors);
        let _ = emit_assistant_runtime_state_from_current_settings(
            &app,
            state.inner(),
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
    })
}

#[tauri::command]
fn get_module_health(
    state: State<'_, AppState>,
    module_id: Option<String>,
) -> Vec<crate::modules::ModuleHealthStatus> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    module_health::get_health(&settings.module_settings, module_id.as_deref())
}

#[tauri::command]
fn check_module_updates(
    app: AppHandle,
    state: State<'_, AppState>,
    module_id: Option<String>,
) -> Vec<crate::modules::ModuleUpdateInfo> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let updates = module_health::check_updates(&settings.module_settings, module_id.as_deref());
    for update in &updates {
        let _ = app.emit("module:update-available", update);
    }
    updates
}

fn collect_partitioned_entries(history: &PartitionedHistory) -> Vec<HistoryEntry> {
    let mut out = Vec::new();
    for partition in history.list_partitions() {
        if let Ok(key) = crate::history_partition::PartitionKey::parse(&partition.key) {
            out.extend(history.load_partition(&key));
        }
    }
    out
}

fn collect_all_transcript_entries(state: &AppState) -> Vec<HistoryEntry> {
    let mut entries = Vec::new();
    {
        let history = state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.extend(collect_partitioned_entries(&history));
    }
    {
        let history = state
            .history_transcribe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        entries.extend(collect_partitioned_entries(&history));
    }
    entries.sort_by_key(|entry| entry.timestamp_ms);
    entries
}

fn module_enabled(settings: &Settings, module_id: &str) -> bool {
    settings.module_settings.enabled_modules.contains(module_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeCapability {
    AiRefinement,
    WorkflowAgent,
    VisionInput,
    VoiceOutputTts,
}

impl RuntimeCapability {
    fn module_id(self) -> &'static str {
        match self {
            Self::AiRefinement => AI_REFINEMENT_MODULE_ID,
            Self::WorkflowAgent => "workflow_agent",
            Self::VisionInput => "input_vision",
            Self::VoiceOutputTts => "output_voice_tts",
        }
    }

    fn setting_enabled(self, settings: &Settings) -> bool {
        match self {
            Self::AiRefinement => settings.ai_fallback.enabled,
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
            Self::WorkflowAgent => {
                "Workflow Agent module is disabled. Enable module 'workflow_agent' first."
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
            Self::WorkflowAgent => "Workflow Agent is disabled in settings.",
            Self::VisionInput => "Vision input is disabled in settings.",
            Self::VoiceOutputTts => "Voice output is disabled in settings.",
        }
    }
}

fn capability_enabled(settings: &Settings, capability: RuntimeCapability) -> bool {
    module_enabled(settings, capability.module_id()) && capability.setting_enabled(settings)
}

fn require_capability_enabled(
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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantCapabilitySnapshot {
    product_mode: String,
    assistant_mode: bool,
    workflow_agent_available: bool,
    tts_available: bool,
    vision_available: bool,
    degraded: bool,
    hard_blocked: bool,
    missing_capabilities: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantStateChangedEvent {
    state: AssistantOrchestratorState,
    previous_state: AssistantOrchestratorState,
    reason: String,
    transition_id: u64,
    changed_at_ms: u64,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantPlanReadyEvent {
    state: AssistantOrchestratorState,
    reason: String,
    plan: crate::workflow_agent::AgentExecutionPlan,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantIntentDetectedEvent {
    state: AssistantOrchestratorState,
    reason: String,
    parse: crate::workflow_agent::AgentCommandParseResult,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantAwaitingConfirmationEvent {
    state: AssistantOrchestratorState,
    reason: String,
    plan: crate::workflow_agent::AgentExecutionPlan,
    confirm_token: String,
    confirm_timeout_sec: u16,
    expires_at_ms: u64,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantConfirmationExpiredEvent {
    state: AssistantOrchestratorState,
    reason: String,
    expired_at_ms: u64,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AssistantActionResultEvent {
    state: AssistantOrchestratorState,
    reason: String,
    result: crate::workflow_agent::AgentExecutionResult,
    capability: AssistantCapabilitySnapshot,
}

#[derive(Debug)]
enum PendingConfirmationError {
    Missing,
    Expired { expired_at_ms: u64 },
    TokenMismatch,
    PlanMismatch,
}

fn next_confirmation_token() -> String {
    let seq = ASSISTANT_CONFIRM_TOKEN_SEQ.fetch_add(1, Ordering::Relaxed);
    let code = (seq % 9_000) + 1_000;
    format!("{code:04}")
}

fn normalize_confirmation_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn plans_match_for_confirmation(
    left: &crate::workflow_agent::AgentExecutionPlan,
    right: &crate::workflow_agent::AgentExecutionPlan,
) -> bool {
    left.intent == right.intent
        && left.session_id == right.session_id
        && left.target_language == right.target_language
        && left.publish == right.publish
}

fn register_pending_confirmation(
    plan: &crate::workflow_agent::AgentExecutionPlan,
    confirm_timeout_sec: u16,
) -> (String, u64) {
    let token = next_confirmation_token();
    let expires_at_ms = crate::util::now_ms().saturating_add(confirm_timeout_sec as u64 * 1_000);
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *pending = Some(PendingAssistantConfirmation {
        plan: plan.clone(),
        confirm_token: token.clone(),
        expires_at_ms,
    });
    (token, expires_at_ms)
}

fn clear_pending_confirmation() {
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *pending = None;
}

fn clear_pending_confirmation_for_plan(plan: &crate::workflow_agent::AgentExecutionPlan) {
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if pending
        .as_ref()
        .map(|entry| plans_match_for_confirmation(&entry.plan, plan))
        .unwrap_or(false)
    {
        *pending = None;
    }
}

fn consume_pending_confirmation(
    plan: &crate::workflow_agent::AgentExecutionPlan,
    token: &str,
) -> Result<(), PendingConfirmationError> {
    let now_ms = crate::util::now_ms();
    let mut pending = ASSISTANT_PENDING_CONFIRMATION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(current) = pending.as_ref() else {
        return Err(PendingConfirmationError::Missing);
    };
    if now_ms > current.expires_at_ms {
        let expired_at_ms = current.expires_at_ms;
        *pending = None;
        return Err(PendingConfirmationError::Expired { expired_at_ms });
    }
    if !plans_match_for_confirmation(&current.plan, plan) {
        return Err(PendingConfirmationError::PlanMismatch);
    }
    if normalize_confirmation_token(token)
        != normalize_confirmation_token(current.confirm_token.as_str())
    {
        return Err(PendingConfirmationError::TokenMismatch);
    }
    *pending = None;
    Ok(())
}

fn assistant_product_mode(settings: &Settings) -> &'static str {
    if settings
        .product_mode
        .trim()
        .eq_ignore_ascii_case("assistant")
    {
        "assistant"
    } else {
        "transcribe"
    }
}

fn assistant_capability_snapshot(settings: &Settings) -> AssistantCapabilitySnapshot {
    let assistant_mode = assistant_product_mode(settings) == "assistant";
    let workflow_agent_available = capability_enabled(settings, RuntimeCapability::WorkflowAgent);
    let tts_available = capability_enabled(settings, RuntimeCapability::VoiceOutputTts);
    let vision_available = capability_enabled(settings, RuntimeCapability::VisionInput);
    let mut missing_capabilities: Vec<String> = Vec::new();
    if !assistant_mode {
        missing_capabilities.push("product_mode_assistant".to_string());
    }
    if !workflow_agent_available {
        missing_capabilities.push("workflow_agent".to_string());
    }
    if !tts_available {
        missing_capabilities.push("output_voice_tts".to_string());
    }
    if !vision_available {
        missing_capabilities.push("input_vision".to_string());
    }
    AssistantCapabilitySnapshot {
        product_mode: assistant_product_mode(settings).to_string(),
        assistant_mode,
        workflow_agent_available,
        tts_available,
        vision_available,
        degraded: assistant_mode
            && workflow_agent_available
            && (!tts_available || !vision_available),
        hard_blocked: !assistant_mode || !workflow_agent_available,
        missing_capabilities,
    }
}

fn assistant_baseline_state(
    capability: &AssistantCapabilitySnapshot,
) -> (AssistantOrchestratorState, &'static str) {
    if !capability.assistant_mode {
        return (AssistantOrchestratorState::Idle, "product_mode_transcribe");
    }
    if !capability.workflow_agent_available {
        return (
            AssistantOrchestratorState::Idle,
            "workflow_agent_unavailable",
        );
    }
    if capability.degraded {
        return (
            AssistantOrchestratorState::Listening,
            "assistant_degraded_capability",
        );
    }
    (AssistantOrchestratorState::Listening, "assistant_ready")
}

fn transition_assistant_state_with_settings(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    next_state: AssistantOrchestratorState,
    reason: impl Into<String>,
) -> AssistantStateChangedEvent {
    let reason = reason.into();
    let capability = assistant_capability_snapshot(settings);
    let now_ms = crate::util::now_ms();
    let (previous_state, changed_at_ms, transition_id) = {
        let mut tracker = state
            .assistant_orchestrator
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_state = tracker.state;
        let has_changed = tracker.state != next_state || tracker.last_reason != reason;
        if has_changed {
            tracker.transition_id = tracker.transition_id.saturating_add(1);
            tracker.changed_at_ms = now_ms;
            tracker.state = next_state;
            tracker.last_reason = reason.clone();
        }
        (
            previous_state,
            if has_changed {
                tracker.changed_at_ms
            } else if tracker.changed_at_ms == 0 {
                now_ms
            } else {
                tracker.changed_at_ms
            },
            tracker.transition_id,
        )
    };

    let payload = AssistantStateChangedEvent {
        state: next_state,
        previous_state,
        reason,
        transition_id,
        changed_at_ms,
        capability,
    };
    let _ = app.emit("assistant:state-changed", &payload);
    payload
}

fn emit_assistant_baseline_state(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    trigger: &str,
) -> AssistantStateChangedEvent {
    let capability = assistant_capability_snapshot(settings);
    let (baseline_state, baseline_reason) = assistant_baseline_state(&capability);
    clear_pending_confirmation();
    let reason = if trigger.trim().is_empty() {
        baseline_reason.to_string()
    } else {
        format!("{}:{}", trigger.trim(), baseline_reason)
    };
    transition_assistant_state_with_settings(app, state, settings, baseline_state, reason)
}

fn emit_assistant_runtime_state_from_current_settings(
    app: &AppHandle,
    state: &AppState,
    trigger: &str,
) -> AssistantStateChangedEvent {
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    emit_assistant_baseline_state(app, state, &settings_snapshot, trigger)
}

fn emit_assistant_plan_ready(
    app: &AppHandle,
    settings: &Settings,
    plan: &crate::workflow_agent::AgentExecutionPlan,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantPlanReadyEvent {
        state: AssistantOrchestratorState::AwaitingConfirm,
        reason: reason.to_string(),
        plan: plan.clone(),
        capability,
    };
    let _ = app.emit("assistant:plan-ready", &payload);
}

fn emit_assistant_intent_detected(
    app: &AppHandle,
    settings: &Settings,
    parse: &crate::workflow_agent::AgentCommandParseResult,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantIntentDetectedEvent {
        state: AssistantOrchestratorState::Parsing,
        reason: reason.to_string(),
        parse: parse.clone(),
        capability,
    };
    let _ = app.emit("assistant:intent-detected", &payload);
}

fn emit_assistant_awaiting_confirmation(
    app: &AppHandle,
    settings: &Settings,
    plan: &crate::workflow_agent::AgentExecutionPlan,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let confirm_timeout_sec = settings.workflow_agent.confirm_timeout_sec.clamp(10, 300);
    let (confirm_token, expires_at_ms) = register_pending_confirmation(plan, confirm_timeout_sec);
    let payload = AssistantAwaitingConfirmationEvent {
        state: AssistantOrchestratorState::AwaitingConfirm,
        reason: reason.to_string(),
        plan: plan.clone(),
        confirm_token,
        confirm_timeout_sec,
        expires_at_ms,
        capability,
    };
    let _ = app.emit("assistant:awaiting-confirmation", &payload);
}

fn emit_assistant_confirmation_expired(
    app: &AppHandle,
    settings: &Settings,
    reason: &str,
    expired_at_ms: u64,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantConfirmationExpiredEvent {
        state: AssistantOrchestratorState::Recovering,
        reason: reason.to_string(),
        expired_at_ms,
        capability,
    };
    let _ = app.emit("assistant:confirmation-expired", &payload);
}

fn expire_pending_confirmation_if_needed(
    app: &AppHandle,
    state: &AppState,
    settings: &Settings,
    trigger: &str,
) -> bool {
    let now_ms = crate::util::now_ms();
    let expired_at_ms = {
        let mut pending = ASSISTANT_PENDING_CONFIRMATION
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match pending.as_ref() {
            Some(entry) if now_ms > entry.expires_at_ms => {
                let expired_at_ms = entry.expires_at_ms;
                *pending = None;
                Some(expired_at_ms)
            }
            _ => None,
        }
    };
    if let Some(expired_at_ms) = expired_at_ms {
        emit_assistant_confirmation_expired(
            app,
            settings,
            &format!("{}:timeout", trigger),
            expired_at_ms,
        );
        let _ = emit_assistant_baseline_state(app, state, settings, trigger);
        return true;
    }
    false
}

fn emit_assistant_action_result(
    app: &AppHandle,
    settings: &Settings,
    state: AssistantOrchestratorState,
    result: &crate::workflow_agent::AgentExecutionResult,
    reason: &str,
) {
    let capability = assistant_capability_snapshot(settings);
    if !capability.assistant_mode {
        return;
    }
    let payload = AssistantActionResultEvent {
        state,
        reason: reason.to_string(),
        result: result.clone(),
        capability,
    };
    let _ = app.emit("assistant:action-result", &payload);
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

#[cfg(test)]
mod assistant_orchestrator_tests {
    use super::{assistant_baseline_state, assistant_capability_snapshot, RuntimeCapability};
    use crate::state::{AssistantOrchestratorState, Settings};

    fn settings_for_assistant_mode(
        product_mode: &str,
        workflow_enabled: bool,
        tts_enabled: bool,
        vision_enabled: bool,
    ) -> Settings {
        let mut settings = Settings::default();
        settings.product_mode = product_mode.to_string();

        if workflow_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(RuntimeCapability::WorkflowAgent.module_id().to_string());
            settings.workflow_agent.enabled = true;
        }
        if tts_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(RuntimeCapability::VoiceOutputTts.module_id().to_string());
            settings.voice_output_settings.enabled = true;
        }
        if vision_enabled {
            settings
                .module_settings
                .enabled_modules
                .insert(RuntimeCapability::VisionInput.module_id().to_string());
            settings.vision_input_settings.enabled = true;
        }

        settings
    }

    #[test]
    fn assistant_baseline_is_idle_when_product_mode_is_transcribe() {
        let settings = settings_for_assistant_mode("transcribe", true, true, true);
        let capability = assistant_capability_snapshot(&settings);
        assert!(!capability.assistant_mode);
        assert!(capability.hard_blocked);
        let (state, reason) = assistant_baseline_state(&capability);
        assert_eq!(state, AssistantOrchestratorState::Idle);
        assert_eq!(reason, "product_mode_transcribe");
    }

    #[test]
    fn assistant_baseline_is_listening_with_degraded_capabilities() {
        let settings = settings_for_assistant_mode("assistant", true, false, true);
        let capability = assistant_capability_snapshot(&settings);
        assert!(capability.assistant_mode);
        assert!(capability.workflow_agent_available);
        assert!(capability.degraded);
        assert!(!capability.hard_blocked);
        assert!(capability
            .missing_capabilities
            .iter()
            .any(|id| id == "output_voice_tts"));
        let (state, reason) = assistant_baseline_state(&capability);
        assert_eq!(state, AssistantOrchestratorState::Listening);
        assert_eq!(reason, "assistant_degraded_capability");
    }

    #[test]
    fn assistant_baseline_is_idle_when_workflow_agent_is_unavailable() {
        let settings = settings_for_assistant_mode("assistant", false, true, true);
        let capability = assistant_capability_snapshot(&settings);
        assert!(capability.assistant_mode);
        assert!(!capability.workflow_agent_available);
        assert!(capability.hard_blocked);
        let (state, reason) = assistant_baseline_state(&capability);
        assert_eq!(state, AssistantOrchestratorState::Idle);
        assert_eq!(reason, "workflow_agent_unavailable");
    }
}

#[tauri::command]
fn agent_list_supported_actions() -> Vec<String> {
    vec![
        "gdd_generate_publish".to_string(),
        "session_recap".to_string(),
        "plan_status".to_string(),
        "confirm_or_cancel".to_string(),
    ]
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct AgentComposeUnknownReplyRequest {
    command_text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
struct AgentComposeReplyResult {
    text: String,
    source: String,
    reason_code: String,
}

fn merged_workflow_wakewords(settings: &crate::modules::WorkflowAgentSettings) -> Vec<String> {
    let mut merged = settings.wakewords.clone();
    merged.extend(settings.wakeword_aliases.clone());
    merged
}

fn unknown_rule_reply(command_text: &str, online_enabled: bool) -> String {
    let normalized = command_text.to_lowercase();
    let english_hint = normalized.contains("please")
        || normalized.contains("what")
        || normalized.contains("session")
        || normalized.contains("status")
        || normalized.contains("weather");
    let weather_like = normalized.contains("weather")
        || normalized.contains("wetter")
        || normalized.contains("forecast")
        || normalized.contains("temperatur");
    if weather_like {
        if online_enabled {
            if english_hint {
                return "Online mode is enabled, but live web lookup tools are not configured yet. I can still provide a best-effort answer from model knowledge.".to_string();
            }
            return "Online-Modus ist aktiv, aber Live-Web-Lookup ist noch nicht konfiguriert. Ich kann dir trotzdem eine Best-Effort-Antwort aus Modellwissen geben.".to_string();
        }
        if english_hint {
            return "I do not have live weather access in local mode. I can still help with plan status, session recaps, or GDD drafts from your transcripts.".to_string();
        }
        return "Ich habe lokal keinen Live-Wetterzugriff. Ich kann dir aber einen Plan, Recap oder GDD-Draft aus deinen Transkripten erstellen.".to_string();
    }
    if english_hint {
        return "I can currently handle GDD drafts, session recaps, and plan status from your local transcripts. Please rephrase your request within that scope.".to_string();
    }
    "Ich kann aktuell GDD-Drafts, Session-Recaps und Plan-Status aus deinen lokalen Transkripten verarbeiten. Formuliere die Anfrage bitte in diesem Scope.".to_string()
}

fn ai_provider_is_local(provider: &str) -> bool {
    matches!(provider, "ollama" | "lm_studio" | "oobabooga")
}

#[tauri::command]
fn agent_compose_unknown_reply(
    app: AppHandle,
    state: State<'_, AppState>,
    request: AgentComposeUnknownReplyRequest,
) -> Result<AgentComposeReplyResult, String> {
    guarded_command!("agent_compose_unknown_reply", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };

        let command_text = request.command_text.trim();
        if command_text.is_empty() {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply("", false),
                source: "rule".to_string(),
                reason_code: "empty_command".to_string(),
            });
        }

        let workflow_cfg = &settings_snapshot.workflow_agent;
        let online_enabled = workflow_cfg.online_enabled;
        if workflow_cfg.reply_mode != "hybrid_local_llm" {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: "rule_only_mode".to_string(),
            });
        }

        if !settings_snapshot.ai_fallback.enabled {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: "ai_refinement_disabled".to_string(),
            });
        }

        if !online_enabled && !ai_provider_is_local(&settings_snapshot.ai_fallback.provider) {
            return Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: "non_local_provider_blocked".to_string(),
            });
        }

        let setup = match prepare_refinement(&app, &settings_snapshot) {
            Ok(value) => value,
            Err(error) => {
                return Ok(AgentComposeReplyResult {
                    text: unknown_rule_reply(command_text, online_enabled),
                    source: "rule".to_string(),
                    reason_code: format!("local_runtime_unavailable:{error}"),
                });
            }
        };

        let mut options = setup.options.clone();
        options.max_tokens = options.max_tokens.clamp(128, 512);
        options.custom_prompt = Some(if online_enabled {
            "You are Trispr. Reply in the same language as the user. Keep answers concise and practical. Online models are allowed, but live web lookup tools may be unavailable. Never fabricate citations or claim verified live data unless explicit tool results are provided. Output only the reply.".to_string()
        } else {
            "You are Trispr, a local assistant. Reply in the same language as the user. Keep answers concise and practical. Never claim live internet access. If the request needs real-time external data, say this capability is unavailable locally and suggest a supported local action. Output only the reply."
                .to_string()
        });
        options.enforce_language_guard = false;

        match setup
            .provider
            .refine_transcript(command_text, &setup.model, &options, &setup.api_key)
        {
            Ok(reply) => {
                let text = reply.text.trim();
                if text.is_empty() {
                    return Ok(AgentComposeReplyResult {
                        text: unknown_rule_reply(command_text, online_enabled),
                        source: "rule".to_string(),
                        reason_code: "local_llm_empty".to_string(),
                    });
                }
                Ok(AgentComposeReplyResult {
                    text: text.to_string(),
                    source: "local_llm".to_string(),
                    reason_code: "local_llm_success".to_string(),
                })
            }
            Err(error) => Ok(AgentComposeReplyResult {
                text: unknown_rule_reply(command_text, online_enabled),
                source: "rule".to_string(),
                reason_code: format!("local_llm_error:{error}"),
            }),
        }
    })
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct AgentCancelPendingConfirmationRequest {
    reason: Option<String>,
}

#[tauri::command]
fn agent_cancel_pending_confirmation(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<AgentCancelPendingConfirmationRequest>,
) -> Result<crate::workflow_agent::AgentExecutionResult, String> {
    guarded_command!("agent_cancel_pending_confirmation", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        let reason = request
            .and_then(|value| value.reason)
            .unwrap_or_else(|| "cancelled_by_user".to_string());
        let reason_trimmed = reason.trim().to_string();
        let pending = {
            let mut guard = ASSISTANT_PENDING_CONFIRMATION
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            guard.take()
        };

        if pending.is_none() {
            return Ok(crate::workflow_agent::AgentExecutionResult {
                status: "cancelled".to_string(),
                message: "No pending confirmation to cancel.".to_string(),
                draft: None,
                publish_result: None,
                queued_job: None,
                error: None,
            });
        }

        let pending = pending.expect("checked is_some");
        let result = crate::workflow_agent::AgentExecutionResult {
            status: "cancelled".to_string(),
            message: "Pending confirmation cancelled.".to_string(),
            draft: None,
            publish_result: None,
            queued_job: None,
            error: None,
        };

        if assistant_mode {
            if reason_trimmed.eq_ignore_ascii_case("timeout") {
                emit_assistant_confirmation_expired(
                    &app,
                    &settings_snapshot,
                    "agent_cancel_pending_confirmation:timeout",
                    pending.expires_at_ms,
                );
            } else {
                emit_assistant_action_result(
                    &app,
                    &settings_snapshot,
                    AssistantOrchestratorState::Recovering,
                    &result,
                    "agent_cancel_pending_confirmation",
                );
            }
            let _ = emit_assistant_baseline_state(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_cancel_pending_confirmation",
            );
        }

        Ok(result)
    })
}

#[tauri::command]
fn agent_parse_command(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::workflow_agent::AgentParseCommandRequest,
) -> Result<crate::workflow_agent::AgentCommandParseResult, String> {
    guarded_command!("agent_parse_command", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = expire_pending_confirmation_if_needed(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_parse_command",
            );
        }
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Parsing,
                "agent_parse_command:start",
            );
        }
        let workflow_settings = settings_snapshot.workflow_agent.clone();
        let intent_keywords = workflow_settings
            .intent_keywords
            .get("gdd_generate_publish")
            .cloned()
            .unwrap_or_default();
        let wakewords = merged_workflow_wakewords(&workflow_settings);
        let parsed = crate::workflow_agent::parse_command(
            &request,
            &wakewords,
            &intent_keywords,
        );
        if parsed.detected || parsed.wakeword_matched {
            let _ = app.emit("agent:command-detected", &parsed);
            if assistant_mode {
                emit_assistant_intent_detected(
                    &app,
                    &settings_snapshot,
                    &parsed,
                    if parsed.detected {
                        "agent_parse_command:detected"
                    } else {
                        "agent_parse_command:wakeword_unknown"
                    },
                );
            }
        }
        if assistant_mode {
            let trigger = if parsed.detected {
                "agent_parse_command:detected"
            } else if parsed.wakeword_matched {
                "agent_parse_command:wakeword_unknown"
            } else {
                "agent_parse_command:ignored"
            };
            let _ = emit_assistant_baseline_state(&app, state.inner(), &settings_snapshot, trigger);
        }
        Ok(parsed)
    })
}

#[tauri::command]
fn search_transcript_sessions(
    state: State<'_, AppState>,
    mut request: crate::workflow_agent::SearchTranscriptSessionsRequest,
) -> Result<Vec<crate::workflow_agent::TranscriptSessionCandidate>, String> {
    guarded_command!("search_transcript_sessions", {
        let defaults = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            (
                settings.workflow_agent.session_gap_minutes,
                settings.workflow_agent.max_candidates,
            )
        };
        if request.session_gap_minutes.unwrap_or(0) == 0 {
            request.session_gap_minutes = Some(defaults.0);
        }
        if request.max_candidates.unwrap_or(0) == 0 {
            request.max_candidates = Some(defaults.1);
        }

        let entries = collect_all_transcript_entries(&state);
        let sessions = crate::workflow_agent::build_sessions(
            &entries,
            request.session_gap_minutes.unwrap_or(defaults.0),
        );
        Ok(crate::workflow_agent::score_sessions(&sessions, &request))
    })
}

#[tauri::command]
fn agent_build_execution_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::workflow_agent::AgentBuildExecutionPlanRequest,
) -> Result<crate::workflow_agent::AgentExecutionPlan, String> {
    guarded_command!("agent_build_execution_plan", {
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = expire_pending_confirmation_if_needed(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_build_execution_plan",
            );
        }
        if request.intent.trim().is_empty() {
            return Err("Intent is required.".to_string());
        }
        if request.session_id.trim().is_empty() {
            return Err("Session id is required.".to_string());
        }
        const ALLOWED_LANGUAGES: &[&str] = &[
            "source", "en", "de", "fr", "es", "it", "pt", "nl", "pl", "ru", "ja", "ko", "zh", "ar",
            "tr", "hi",
        ];
        let lang = request.target_language.trim();
        if !ALLOWED_LANGUAGES.contains(&lang) {
            return Err(format!(
                "Invalid target language '{}'. Allowed: {}",
                lang,
                ALLOWED_LANGUAGES.join(", ")
            ));
        }
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Planning,
                "agent_build_execution_plan:start",
            );
        }
        let plan = crate::workflow_agent::default_execution_plan(&request);
        let _ = app.emit("agent:plan-ready", &plan);
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::AwaitingConfirm,
                "agent_build_execution_plan:ready",
            );
            emit_assistant_plan_ready(
                &app,
                &settings_snapshot,
                &plan,
                "agent_build_execution_plan:ready",
            );
            emit_assistant_awaiting_confirmation(
                &app,
                &settings_snapshot,
                &plan,
                "agent_build_execution_plan:ready",
            );
        }
        Ok(plan)
    })
}

#[tauri::command]
fn agent_execute_gdd_plan(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::workflow_agent::AgentExecuteGddPlanRequest,
) -> Result<crate::workflow_agent::AgentExecutionResult, String> {
    guarded_command!("agent_execute_gdd_plan", {
        let plan = request.plan.clone();
        let settings_snapshot = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::WorkflowAgent)?;
            settings.clone()
        };
        let assistant_mode = assistant_product_mode(&settings_snapshot) == "assistant";
        if assistant_mode {
            let _ = expire_pending_confirmation_if_needed(
                &app,
                state.inner(),
                &settings_snapshot,
                "agent_execute_gdd_plan",
            );
        }
        if assistant_mode {
            let _ = transition_assistant_state_with_settings(
                &app,
                state.inner(),
                &settings_snapshot,
                AssistantOrchestratorState::Executing,
                "agent_execute_gdd_plan:start",
            );
        }
        let confirmation_token = request
            .confirmation_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string);

        if let Some(token) = confirmation_token.as_deref() {
            match consume_pending_confirmation(&plan, token) {
                Ok(()) => {}
                Err(PendingConfirmationError::Missing) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "No pending confirmation for this execution.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_missing".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_missing",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_missing",
                        );
                    }
                    return Ok(result);
                }
                Err(PendingConfirmationError::Expired { expired_at_ms }) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Confirmation token expired. Build a new plan and confirm again."
                            .to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_expired".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_confirmation_expired(
                            &app,
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_expired",
                            expired_at_ms,
                        );
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_expired",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_expired",
                        );
                    }
                    return Ok(result);
                }
                Err(PendingConfirmationError::TokenMismatch) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Invalid confirmation token.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_token_mismatch".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_token_mismatch",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_token_mismatch",
                        );
                    }
                    return Ok(result);
                }
                Err(PendingConfirmationError::PlanMismatch) => {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Confirmation token does not match the active plan.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("confirmation_plan_mismatch".to_string()),
                    };
                    if assistant_mode {
                        emit_assistant_action_result(
                            &app,
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            &result,
                            "agent_execute_gdd_plan:confirmation_plan_mismatch",
                        );
                        let _ = emit_assistant_baseline_state(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            "agent_execute_gdd_plan:confirmation_plan_mismatch",
                        );
                    }
                    return Ok(result);
                }
            }
        } else {
            clear_pending_confirmation_for_plan(&plan);
        }

        let execution_result =
            (|| -> Result<crate::workflow_agent::AgentExecutionResult, String> {
                if plan.intent != "gdd_generate_publish" {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Unsupported agent intent.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some(format!("Unsupported intent '{}'.", plan.intent)),
                    };
                    let _ = app.emit("agent:execution-failed", &result);
                    return Ok(result);
                }

                let _ = app.emit(
                    "agent:execution-progress",
                    serde_json::json!({
                        "session_id": plan.session_id,
                        "stage": "load_session",
                    }),
                );

                let workflow_gap_minutes = settings_snapshot.workflow_agent.session_gap_minutes;
                let preset_clones = settings_snapshot.gdd_module_settings.preset_clones.clone();
                let confluence_settings = settings_snapshot.confluence_settings.clone();
                let one_click_threshold = settings_snapshot
                    .gdd_module_settings
                    .one_click_confidence_threshold;
                let vision_bridge_enabled =
                    capability_enabled(&settings_snapshot, RuntimeCapability::VisionInput);
                let tts_bridge_enabled =
                    capability_enabled(&settings_snapshot, RuntimeCapability::VoiceOutputTts)
                        && settings_snapshot.workflow_agent.voice_feedback_enabled;
                let maybe_agent_speak = |context: &str, message: &str| {
                    if !tts_bridge_enabled || message.trim().is_empty() {
                        return;
                    }
                    let request = crate::multimodal_io::TtsSpeakRequest {
                        provider: String::new(),
                        text: message.trim().to_string(),
                        rate: None,
                        volume: None,
                        context: Some(context.to_string()),
                    };
                    if let Err(tts_error) = speak_tts_internal(&app, state.inner(), request) {
                        info!("workflow_agent tts skipped: {}", tts_error);
                    }
                };
                let entries = collect_all_transcript_entries(&state);
                let sessions =
                    crate::workflow_agent::build_sessions(&entries, workflow_gap_minutes);
                let session = match sessions
                    .iter()
                    .find(|candidate| candidate.id == plan.session_id)
                    .cloned()
                {
                    Some(session) => session,
                    None => {
                        let result = crate::workflow_agent::AgentExecutionResult {
                            status: "failed".to_string(),
                            message: format!("Session '{}' not found.", plan.session_id),
                            draft: None,
                            publish_result: None,
                            queued_job: None,
                            error: Some(format!("Session '{}' not found.", plan.session_id)),
                        };
                        let _ = app.emit("agent:execution-failed", &result);
                        return Ok(result);
                    }
                };

                let transcript = session
                    .entries
                    .iter()
                    .map(|entry| entry.text.trim())
                    .filter(|text| !text.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                if transcript.trim().is_empty() {
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Session has no transcript content.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some("Session content was empty.".to_string()),
                    };
                    let _ = app.emit("agent:execution-failed", &result);
                    return Ok(result);
                }

                let title = request
                    .title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("GDD Session {}", session.start_ms));
                let target_language = plan.target_language.trim().to_string();
                let mut template_hints: Vec<String> = Vec::new();
                if !target_language.is_empty() {
                    template_hints.push(format!(
                    "Target output language preference: {}. Keep source facts unchanged and avoid invention.",
                    target_language
                ));
                }
                if vision_bridge_enabled {
                    let _ = app.emit(
                        "agent:execution-progress",
                        serde_json::json!({
                            "session_id": plan.session_id,
                            "stage": "vision_context",
                        }),
                    );
                    match capture_vision_snapshot_internal(&app, state.inner()) {
                        Ok(snapshot) => {
                            let dimensions = match (snapshot.width, snapshot.height) {
                                (Some(w), Some(h)) => format!("{}x{}", w, h),
                                _ => "unknown".to_string(),
                            };
                            let scope = snapshot
                                .source_scope
                                .as_deref()
                                .unwrap_or("unknown")
                                .to_string();
                            template_hints.push(format!(
                            "Vision context available (scope={}, sources={}, frame={}, timestamp_ms={}). Treat this as supporting context only.",
                            scope,
                            snapshot.source_count,
                            dimensions,
                            snapshot.timestamp_ms
                        ));
                            let _ = app.emit(
                                "agent:execution-progress",
                                serde_json::json!({
                                    "session_id": plan.session_id,
                                    "stage": "vision_context_ready",
                                    "source_scope": scope,
                                    "source_count": snapshot.source_count,
                                    "timestamp_ms": snapshot.timestamp_ms,
                                }),
                            );
                        }
                        Err(error) => {
                            warn!("workflow_agent vision context unavailable: {}", error);
                            let _ = app.emit(
                                "agent:execution-progress",
                                serde_json::json!({
                                    "session_id": plan.session_id,
                                    "stage": "vision_context_unavailable",
                                    "error": error,
                                }),
                            );
                        }
                    }
                }
                let template_hint = if template_hints.is_empty() {
                    None
                } else {
                    Some(template_hints.join(" "))
                };

                let _ = app.emit(
                    "agent:execution-progress",
                    serde_json::json!({
                        "session_id": plan.session_id,
                        "stage": "generate_draft",
                        "target_language": target_language,
                    }),
                );

                let draft_request = crate::gdd::GenerateGddDraftRequest {
                    transcript,
                    preset_id: request.preset_id.clone(),
                    title: Some(title.clone()),
                    max_chunk_chars: request.max_chunk_chars,
                    template_hint,
                    template_label: Some("workflow_agent".to_string()),
                };
                let draft = crate::gdd::generate_draft(&draft_request, &preset_clones);

                let publish_after_draft = crate::workflow_agent::should_publish_after_draft(&plan);
                if !publish_after_draft {
                    let skipped_reason = if plan.publish {
                        "Draft generated. Publish skipped because the execution lane had no publish step."
                    } else {
                        "Draft generated. Publish skipped by plan."
                    };
                    let result = crate::workflow_agent::AgentExecutionResult {
                        status: "completed".to_string(),
                        message: skipped_reason.to_string(),
                        draft: Some(draft.clone()),
                        publish_result: None,
                        queued_job: None,
                        error: None,
                    };
                    let _ = app.emit("agent:execution-finished", &result);
                    maybe_agent_speak(
                        "agent_reply",
                        "Workflow Agent: Draft generated. Publish was skipped.",
                    );
                    return Ok(result);
                }

                let space_key = request
                    .space_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        let fallback = confluence_settings.default_space_key.trim();
                        if fallback.is_empty() {
                            None
                        } else {
                            Some(fallback.to_string())
                        }
                    })
                    .ok_or_else(|| "No Confluence space key provided for publish.".to_string())?;
                let parent_page_id = request
                    .parent_page_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
                    .or_else(|| {
                        let fallback = confluence_settings.default_parent_page_id.trim();
                        if fallback.is_empty() {
                            None
                        } else {
                            Some(fallback.to_string())
                        }
                    });
                let target_page_id = request
                    .target_page_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);

                let _ = app.emit(
                    "agent:execution-progress",
                    serde_json::json!({
                        "session_id": plan.session_id,
                        "stage": "publish_or_queue",
                        "space_key": space_key,
                    }),
                );

                let storage_body = crate::gdd::render_storage::render_confluence_storage(&draft);
                let publish_request = crate::gdd::confluence::ConfluencePublishRequest {
                    title,
                    storage_body,
                    space_key: space_key.clone(),
                    parent_page_id,
                    target_page_id,
                };

                let publish_result =
                    crate::gdd::confluence::publish(&app, &confluence_settings, &publish_request);
                match publish_result {
                    Ok(publish) => {
                        {
                            let mut settings = state
                                .settings
                                .write()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            let route_key = crate::gdd::confluence::routing_key_for(
                                &space_key,
                                &publish_request.title,
                            );
                            settings
                                .confluence_settings
                                .routing_memory
                                .insert(route_key, publish.page_id.clone());
                            normalize_confluence_settings(&mut settings.confluence_settings);
                            let _ = save_settings_file(&app, &settings);
                            let _ = app.emit("settings-changed", settings.clone());
                        }
                        let result = crate::workflow_agent::AgentExecutionResult {
                            status: "completed".to_string(),
                            message: "Draft generated and published to Confluence.".to_string(),
                            draft: Some(draft),
                            publish_result: Some(publish),
                            queued_job: None,
                            error: None,
                        };
                        let _ = app.emit("agent:execution-finished", &result);
                        maybe_agent_speak(
                            "agent_reply",
                            "Workflow Agent: Draft generated and published to Confluence.",
                        );
                        Ok(result)
                    }
                    Err(error) => {
                        if crate::gdd::publish_queue::is_queueable_publish_error(&error) {
                            let queue_request =
                                crate::gdd::publish_queue::GddPublishOrQueueRequest {
                                    draft: draft.clone(),
                                    publish_request,
                                    routing_confidence: Some(one_click_threshold),
                                    routing_reasoning: Some("workflow_agent execution".to_string()),
                                };
                            let queued_job = crate::gdd::publish_queue::queue_publish_request(
                                &app,
                                &queue_request,
                                &error,
                            )?;
                            let result = crate::workflow_agent::AgentExecutionResult {
                                status: "queued".to_string(),
                                message: "Confluence unavailable. Publish request queued locally."
                                    .to_string(),
                                draft: Some(draft),
                                publish_result: None,
                                queued_job: Some(queued_job),
                                error: Some(error),
                            };
                            let _ = app.emit("agent:execution-finished", &result);
                            maybe_agent_speak(
                            "agent_event",
                            "Workflow Agent: Confluence unavailable. Publish request was queued locally.",
                        );
                            Ok(result)
                        } else {
                            let result = crate::workflow_agent::AgentExecutionResult {
                                status: "failed".to_string(),
                                message: "Publish failed with non-queueable error.".to_string(),
                                draft: Some(draft),
                                publish_result: None,
                                queued_job: None,
                                error: Some(error.clone()),
                            };
                            let _ = app.emit("agent:execution-failed", &result);
                            maybe_agent_speak(
                                "agent_event",
                                "Workflow Agent: Publish failed with a non-queueable error.",
                            );
                            Ok(result)
                        }
                    }
                }
            })();

        match execution_result {
            Ok(result) => {
                if assistant_mode {
                    let assistant_state = if result.status == "failed" {
                        let _ = transition_assistant_state_with_settings(
                            &app,
                            state.inner(),
                            &settings_snapshot,
                            AssistantOrchestratorState::Recovering,
                            "agent_execute_gdd_plan:result_failed",
                        );
                        AssistantOrchestratorState::Recovering
                    } else {
                        AssistantOrchestratorState::Executing
                    };
                    emit_assistant_action_result(
                        &app,
                        &settings_snapshot,
                        assistant_state,
                        &result,
                        "agent_execute_gdd_plan:result",
                    );
                    let _ = emit_assistant_baseline_state(
                        &app,
                        state.inner(),
                        &settings_snapshot,
                        "agent_execute_gdd_plan:complete",
                    );
                }
                Ok(result)
            }
            Err(error) => {
                if assistant_mode {
                    let _ = transition_assistant_state_with_settings(
                        &app,
                        state.inner(),
                        &settings_snapshot,
                        AssistantOrchestratorState::Recovering,
                        "agent_execute_gdd_plan:error",
                    );
                    let synthetic_result = crate::workflow_agent::AgentExecutionResult {
                        status: "failed".to_string(),
                        message: "Workflow execution failed before completion.".to_string(),
                        draft: None,
                        publish_result: None,
                        queued_job: None,
                        error: Some(error.clone()),
                    };
                    emit_assistant_action_result(
                        &app,
                        &settings_snapshot,
                        AssistantOrchestratorState::Recovering,
                        &synthetic_result,
                        "agent_execute_gdd_plan:error",
                    );
                    let _ = emit_assistant_baseline_state(
                        &app,
                        state.inner(),
                        &settings_snapshot,
                        "agent_execute_gdd_plan:error",
                    );
                }
                Err(error)
            }
        }
    })
}

#[tauri::command]
fn list_screen_sources(
    app: AppHandle,
) -> Result<Vec<crate::multimodal_io::VisionSourceInfo>, String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found.".to_string())?;
    let monitors = window
        .available_monitors()
        .map_err(|error| format!("Failed to list monitors: {}", error))?;

    let mut sources = Vec::new();
    for (index, monitor) in monitors.iter().enumerate() {
        let size = monitor.size();
        let label = monitor
            .name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| format!("Monitor {}", index + 1));
        sources.push(crate::multimodal_io::VisionSourceInfo {
            id: format!("monitor_{}", index + 1),
            label,
            width: size.width,
            height: size.height,
        });
    }
    if sources.is_empty() {
        if let Some(current) = window.current_monitor().map_err(|e| e.to_string())? {
            let size = current.size();
            sources.push(crate::multimodal_io::VisionSourceInfo {
                id: "monitor_1".to_string(),
                label: current
                    .name()
                    .map(|name| name.to_string())
                    .unwrap_or_else(|| "Primary monitor".to_string()),
                width: size.width,
                height: size.height,
            });
        }
    }
    Ok(sources)
}

#[tauri::command]
fn start_vision_stream(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::multimodal_io::VisionStreamHealth, String> {
    guarded_command!("start_vision_stream", {
        let (fps, source_scope, max_width, jpeg_quality, ram_buffer_seconds) = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::VisionInput)?;
            (
                settings.vision_input_settings.fps,
                settings.vision_input_settings.source_scope.clone(),
                settings.vision_input_settings.max_width,
                settings.vision_input_settings.jpeg_quality,
                settings.vision_input_settings.ram_buffer_seconds,
            )
        };

        let already_running = state.vision_stream_running.swap(true, Ordering::AcqRel);
        if !already_running {
            state.vision_stream_frame_seq.store(0, Ordering::Release);
            state
                .vision_stream_started_ms
                .store(crate::util::now_ms(), Ordering::Release);
            state
                .vision_frame_buffer
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clear();
            let source_scope_for_thread = source_scope.clone();
            let buffer_frame_cap =
                (usize::from(fps.max(1)) * usize::from(ram_buffer_seconds.max(1))).max(1);
            let app_c = app.clone();
            crate::util::spawn_guarded("vision_frame_capture", move || loop {
                let state = app_c.state::<AppState>();
                if !state.vision_stream_running.load(Ordering::Acquire) {
                    break;
                }
                match crate::multimodal_io::capture_vision_frame(
                    &app_c,
                    &source_scope_for_thread,
                    max_width,
                    jpeg_quality,
                ) {
                    Ok(mut frame) => {
                        frame.seq =
                            state.vision_stream_frame_seq.fetch_add(1, Ordering::AcqRel) + 1;
                        let meta = state
                            .vision_frame_buffer
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .push(frame, buffer_frame_cap);
                        let _ = app_c.emit("vision:frame-meta", &meta);
                    }
                    Err(error) => {
                        let _ = app_c.emit(
                            "vision:stream-error",
                            serde_json::json!({
                                "timestamp_ms": crate::util::now_ms(),
                                "source_scope": source_scope_for_thread,
                                "error": error,
                            }),
                        );
                    }
                }
                let frame_sleep_ms = (1000u64 / (fps.max(1) as u64)).clamp(50, 1000);
                std::thread::sleep(Duration::from_millis(frame_sleep_ms));
            });
            let _ = app.emit(
                "vision:stream-started",
                serde_json::json!({
                    "timestamp_ms": crate::util::now_ms(),
                    "fps": fps,
                }),
            );
        }

        let buffer_stats = state
            .vision_frame_buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .stats();
        Ok(crate::multimodal_io::VisionStreamHealth {
            running: true,
            fps,
            source_scope,
            started_at_ms: Some(state.vision_stream_started_ms.load(Ordering::Acquire)),
            frame_seq: state.vision_stream_frame_seq.load(Ordering::Acquire),
            buffered_frames: buffer_stats.buffered_frames,
            buffered_bytes: buffer_stats.buffered_bytes,
            last_frame_timestamp_ms: buffer_stats.last_frame_timestamp_ms,
            last_frame_width: buffer_stats.last_frame_width,
            last_frame_height: buffer_stats.last_frame_height,
        })
    })
}

#[tauri::command]
fn stop_vision_stream(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::multimodal_io::VisionStreamHealth, String> {
    guarded_command!("stop_vision_stream", {
        Ok(stop_vision_stream_internal(&app, state.inner()))
    })
}

fn stop_vision_stream_internal(
    app: &AppHandle,
    state: &AppState,
) -> crate::multimodal_io::VisionStreamHealth {
    state.vision_stream_running.store(false, Ordering::Release);
    let (fps, source_scope) = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            settings.vision_input_settings.fps,
            settings.vision_input_settings.source_scope.clone(),
        )
    };
    let buffer_stats = state
        .vision_frame_buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .stats();
    let health = crate::multimodal_io::VisionStreamHealth {
        running: false,
        fps,
        source_scope,
        started_at_ms: Some(state.vision_stream_started_ms.load(Ordering::Acquire)),
        frame_seq: state.vision_stream_frame_seq.load(Ordering::Acquire),
        buffered_frames: buffer_stats.buffered_frames,
        buffered_bytes: buffer_stats.buffered_bytes,
        last_frame_timestamp_ms: buffer_stats.last_frame_timestamp_ms,
        last_frame_width: buffer_stats.last_frame_width,
        last_frame_height: buffer_stats.last_frame_height,
    };
    state
        .vision_frame_buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
    let _ = app.emit("vision:stream-stopped", &health);
    health
}

#[tauri::command]
fn get_vision_stream_health(
    state: State<'_, AppState>,
) -> crate::multimodal_io::VisionStreamHealth {
    let (fps, source_scope) = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            settings.vision_input_settings.fps,
            settings.vision_input_settings.source_scope.clone(),
        )
    };
    let buffer_stats = state
        .vision_frame_buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .stats();
    crate::multimodal_io::VisionStreamHealth {
        running: state.vision_stream_running.load(Ordering::Acquire),
        fps,
        source_scope,
        started_at_ms: Some(state.vision_stream_started_ms.load(Ordering::Acquire)),
        frame_seq: state.vision_stream_frame_seq.load(Ordering::Acquire),
        buffered_frames: buffer_stats.buffered_frames,
        buffered_bytes: buffer_stats.buffered_bytes,
        last_frame_timestamp_ms: buffer_stats.last_frame_timestamp_ms,
        last_frame_width: buffer_stats.last_frame_width,
        last_frame_height: buffer_stats.last_frame_height,
    }
}

fn capture_vision_snapshot_internal(
    app: &AppHandle,
    state: &AppState,
) -> Result<crate::multimodal_io::VisionSnapshotResult, String> {
    let vision_settings = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        require_capability_enabled(&settings, RuntimeCapability::VisionInput)?;
        settings.vision_input_settings.clone()
    };
    let source_scope = vision_settings.source_scope.clone();
    if let Some(frame) = state
        .vision_frame_buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .latest()
        .cloned()
    {
        return Ok(crate::multimodal_io::vision_snapshot_from_frame(
            &frame,
            "Snapshot returned from in-memory vision buffer.".to_string(),
        ));
    }

    let mut frame = crate::multimodal_io::capture_vision_frame(
        app,
        &source_scope,
        vision_settings.max_width,
        vision_settings.jpeg_quality,
    )?;
    frame.seq = state.vision_stream_frame_seq.load(Ordering::Acquire);
    Ok(crate::multimodal_io::vision_snapshot_from_frame(
        &frame,
        if source_scope == "active_window" {
            "Snapshot captured from active monitor fallback for active_window scope.".to_string()
        } else {
            "Snapshot captured from in-memory vision path without disk persistence.".to_string()
        },
    ))
}

#[tauri::command]
fn capture_vision_snapshot(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::multimodal_io::VisionSnapshotResult, String> {
    capture_vision_snapshot_internal(&app, state.inner())
}

#[tauri::command]
fn list_tts_providers(state: State<'_, AppState>) -> Vec<crate::multimodal_io::TtsProviderInfo> {
    let settings = state.settings.read().unwrap_or_else(|p| p.into_inner());
    crate::multimodal_io::list_tts_providers(settings.voice_output_settings.qwen3_tts_enabled)
}

#[tauri::command]
fn list_tts_voices(
    state: State<'_, AppState>,
    provider: Option<String>,
) -> Vec<crate::multimodal_io::TtsVoiceInfo> {
    let provider = provider.as_deref().unwrap_or("windows_native");
    if provider == "local_custom" {
        let model_dir = state
            .settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .voice_output_settings
            .piper_model_dir
            .clone();
        crate::multimodal_io::list_piper_voices(&model_dir)
    } else {
        crate::multimodal_io::list_tts_voices(provider)
    }
}

#[tauri::command]
fn list_piper_voice_catalog(
    state: State<'_, AppState>,
) -> Result<Vec<crate::multimodal_io::PiperVoiceCatalogEntry>, String> {
    let model_dir = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        require_capability_enabled(&settings, RuntimeCapability::VoiceOutputTts)?;
        settings.voice_output_settings.piper_model_dir.clone()
    };
    Ok(crate::multimodal_io::list_piper_voice_catalog(&model_dir))
}

#[tauri::command]
fn download_piper_voice_key(
    app: AppHandle,
    state: State<'_, AppState>,
    voice_key: String,
) -> Result<String, String> {
    {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        require_capability_enabled(&settings, RuntimeCapability::VoiceOutputTts)?;
    }
    let path =
        crate::multimodal_io::download_piper_voice_with_progress(voice_key.trim(), |progress| {
            let _ = app.emit("piper:voice-download-progress", progress);
        })?;
    Ok(path.to_string_lossy().to_string())
}

fn pinned_tts_language_hint(language_mode: &str, language_pinned: bool) -> Option<String> {
    if !language_pinned {
        return None;
    }
    let normalized = language_mode.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "auto" {
        return None;
    }
    if normalized.starts_with("de") {
        return Some("de".to_string());
    }
    if normalized.starts_with("en") {
        return Some("en".to_string());
    }
    Some(normalized.chars().take(2).collect())
}

fn infer_tts_language_hint(
    text: &str,
    language_mode: &str,
    language_pinned: bool,
) -> Option<String> {
    if let Some(pinned) = pinned_tts_language_hint(language_mode, language_pinned) {
        return Some(pinned);
    }

    let lowered = text.trim().to_lowercase();
    if lowered.is_empty() {
        return None;
    }
    if lowered.contains('ä')
        || lowered.contains('ö')
        || lowered.contains('ü')
        || lowered.contains('ß')
    {
        return Some("de".to_string());
    }

    let mut de_score = 0_u32;
    let mut en_score = 0_u32;
    for token in lowered.split(|ch: char| !ch.is_alphabetic()) {
        if token.is_empty() {
            continue;
        }
        match token {
            "der" | "die" | "das" | "und" | "nicht" | "ich" | "ist" | "mit" | "für" | "den"
            | "dem" | "ein" | "eine" => de_score += 1,
            "the" | "and" | "not" | "you" | "is" | "are" | "with" | "for" | "this" | "that"
            | "a" | "an" | "to" | "of" => en_score += 1,
            _ => {}
        }
    }

    if de_score > en_score && de_score >= 2 {
        return Some("de".to_string());
    }
    if en_score > de_score && en_score >= 2 {
        return Some("en".to_string());
    }
    None
}

fn resolve_windows_voice_for_provider(
    provider: &str,
    manual_voice_id: &str,
    auto_by_language_enabled: bool,
    language_hint: Option<&str>,
) -> Option<String> {
    if provider != "windows_native" && provider != "windows_natural" {
        return None;
    }

    if auto_by_language_enabled {
        if let Some(language) = language_hint {
            if let Some(auto_voice) =
                crate::multimodal_io::select_windows_voice_for_language(provider, language)
            {
                return Some(auto_voice);
            }
        }
    }

    let manual = manual_voice_id.trim();
    if manual.is_empty() {
        None
    } else {
        Some(manual.to_string())
    }
}

fn resolve_manual_windows_voice_id_for_lane(
    lane_provider: &str,
    default_provider: &str,
    fallback_provider: &str,
    default_voice_id: &str,
    fallback_voice_id: &str,
) -> String {
    if lane_provider == fallback_provider && lane_provider != default_provider {
        let fallback = fallback_voice_id.trim();
        if !fallback.is_empty() {
            return fallback.to_string();
        }
    }
    default_voice_id.trim().to_string()
}

fn piper_effective_volume(global_volume: f32, piper_gain_db: f32) -> f32 {
    let gain = 10_f32.powf(piper_gain_db.clamp(-24.0, 6.0) / 20.0);
    (global_volume.clamp(0.0, 1.0) * gain).clamp(0.0, 1.0)
}

fn is_tts_output_device_unavailable_error(error: &str) -> bool {
    error
        .to_ascii_lowercase()
        .contains("[tts_output_device_unavailable]")
}

fn speak_tts_internal(
    app: &AppHandle,
    state: &AppState,
    request: crate::multimodal_io::TtsSpeakRequest,
) -> Result<crate::multimodal_io::TtsSpeakResult, String> {
    let text = request.text.trim().to_string();
    if text.is_empty() {
        return Err("TTS text cannot be empty.".to_string());
    }

    let (voice_settings, language_mode, language_pinned) = {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        require_capability_enabled(&settings, RuntimeCapability::VoiceOutputTts)?;
        (
            settings.voice_output_settings.clone(),
            settings.language_mode.clone(),
            settings.language_pinned,
        )
    };

    let context = request
        .context
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("manual_user")
        .to_lowercase();
    if !crate::multimodal_io::is_tts_policy_allowed(&voice_settings.output_policy, &context) {
        return Err(format!(
            "Voice output policy '{}' blocks context '{}'.",
            voice_settings.output_policy, context
        ));
    }

    let preferred_provider = if request.provider.trim().is_empty() {
        voice_settings.default_provider.clone()
    } else {
        request.provider.trim().to_string()
    };
    let fallback_provider = voice_settings.fallback_provider.clone();
    let rate = request.rate.unwrap_or(voice_settings.rate).clamp(0.5, 2.0);
    let volume = request
        .volume
        .unwrap_or(voice_settings.volume)
        .clamp(0.0, 1.0);
    let piper_gain_db = voice_settings.piper_gain_db.clamp(-24.0, 6.0);

    state.tts_speaking.store(true, Ordering::Release);
    let _ = app.emit(
        "tts:speech-started",
        serde_json::json!({
            "provider": preferred_provider.clone(),
            "text_len": text.len(),
            "context": context.clone(),
        }),
    );

    let piper_binary_path = voice_settings.piper_binary_path.clone();
    let piper_model_path = voice_settings.piper_model_path.clone();
    let qwen3_runtime_config = resolve_qwen3_tts_runtime_config(&voice_settings);
    let output_device_id = voice_settings.output_device.clone();
    let default_provider_config = voice_settings.default_provider.clone();
    let fallback_provider_config = voice_settings.fallback_provider.clone();
    let windows_voice_id_default = voice_settings.voice_id_windows.clone();
    let windows_voice_id_fallback = voice_settings.voice_id_windows_fallback.clone();
    let auto_voice_by_language_enabled = voice_settings.auto_voice_by_detected_language;
    let auto_voice_language_hint = if auto_voice_by_language_enabled {
        infer_tts_language_hint(&text, &language_mode, language_pinned)
    } else {
        None
    };

    let preferred_provider_for_thread = preferred_provider.clone();
    let fallback_provider_for_thread = fallback_provider.clone();
    let context_for_thread = context.clone();
    let app_c = app.clone();
    crate::util::spawn_guarded("tts_playback", move || {
        let run_chain = |selected_output_device: &str| {
            crate::multimodal_io::execute_tts_with_fallback(
                &preferred_provider_for_thread,
                &fallback_provider_for_thread,
                |provider| match provider {
                    "windows_native" => {
                        let manual_voice_id = resolve_manual_windows_voice_id_for_lane(
                            "windows_native",
                            &default_provider_config,
                            &fallback_provider_config,
                            &windows_voice_id_default,
                            &windows_voice_id_fallback,
                        );
                        let resolved_voice = resolve_windows_voice_for_provider(
                            "windows_native",
                            &manual_voice_id,
                            auto_voice_by_language_enabled,
                            auto_voice_language_hint.as_deref(),
                        );
                        crate::multimodal_io::speak_windows_native(
                            &text,
                            rate,
                            volume,
                            selected_output_device,
                            resolved_voice.as_deref(),
                        )
                    }
                    "windows_natural" => {
                        let manual_voice_id = resolve_manual_windows_voice_id_for_lane(
                            "windows_natural",
                            &default_provider_config,
                            &fallback_provider_config,
                            &windows_voice_id_default,
                            &windows_voice_id_fallback,
                        );
                        let resolved_voice = resolve_windows_voice_for_provider(
                            "windows_natural",
                            &manual_voice_id,
                            auto_voice_by_language_enabled,
                            auto_voice_language_hint.as_deref(),
                        );
                        crate::multimodal_io::speak_windows_natural(
                            &text,
                            rate,
                            volume,
                            selected_output_device,
                            resolved_voice.as_deref(),
                        )
                    }
                    "local_custom" => crate::multimodal_io::speak_piper(
                        &app_c.state::<AppState>().piper_daemon,
                        &text,
                        &piper_binary_path,
                        &piper_model_path,
                        rate,
                        piper_effective_volume(volume, piper_gain_db),
                        selected_output_device,
                    ),
                    "qwen3_tts" => speak_qwen3_tts(
                        &text,
                        rate,
                        volume,
                        selected_output_device,
                        &qwen3_runtime_config,
                    ),
                    _ => Err(format!("Unknown TTS provider '{}'.", provider)),
                },
            )
        };

        let mut recovered_output_device = false;
        let result = match run_chain(&output_device_id) {
            Ok(outcome) => Ok(outcome),
            Err(error)
                if output_device_id != "default"
                    && is_tts_output_device_unavailable_error(&error) =>
            {
                tracing::warn!(
                    "tts playback retrying with default output device after unavailable device '{}': {}",
                    output_device_id,
                    error
                );
                match run_chain("default") {
                    Ok(outcome) => {
                        recovered_output_device = true;
                        Ok(outcome)
                    }
                    Err(retry_error) => Err(format!(
                        "{error} Retry with default device failed: {retry_error}"
                    )),
                }
            }
            Err(error) => Err(error),
        };

        let state = app_c.state::<AppState>();
        state.tts_speaking.store(false, Ordering::Release);
        match result {
            Ok(outcome) => {
                if recovered_output_device {
                    let mut snapshot = {
                        let mut settings = state
                            .settings
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        settings.voice_output_settings.output_device = "default".to_string();
                        settings.clone()
                    };
                    crate::modules::normalize_voice_output_settings(
                        &mut snapshot.voice_output_settings,
                    );
                    let _ = save_settings_file(&app_c, &snapshot);
                    let _ = app_c.emit("settings-changed", snapshot);
                }
                let _ = app_c.emit(
                    "tts:speech-finished",
                    serde_json::json!({
                        "provider_used": outcome.provider_used,
                        "preferred_provider": preferred_provider_for_thread,
                        "fallback_provider": fallback_provider_for_thread,
                        "used_fallback": outcome.used_fallback,
                        "primary_error": outcome.primary_error,
                        "output_device_recovered": recovered_output_device,
                        "context": context_for_thread,
                        "timestamp_ms": crate::util::now_ms(),
                    }),
                );
            }
            Err(error) => {
                let _ = app_c.emit(
                    "tts:speech-error",
                    serde_json::json!({
                        "provider": preferred_provider_for_thread.clone(),
                        "preferred_provider": preferred_provider_for_thread.clone(),
                        "fallback_provider": fallback_provider_for_thread.clone(),
                        "context": context_for_thread.clone(),
                        "error": error,
                        "timestamp_ms": crate::util::now_ms(),
                    }),
                );
            }
        }
    });

    let accepted_message = if preferred_provider == fallback_provider {
        format!("TTS request accepted (provider: {}).", preferred_provider)
    } else {
        format!(
            "TTS request accepted (fallback chain: {} -> {}).",
            preferred_provider, fallback_provider
        )
    };

    Ok(crate::multimodal_io::TtsSpeakResult {
        provider_used: preferred_provider.clone(),
        accepted: true,
        message: accepted_message,
        used_fallback: None,
        preferred_provider: Some(preferred_provider),
        fallback_provider: Some(fallback_provider),
        primary_error: None,
    })
}

#[tauri::command]
fn speak_tts(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::multimodal_io::TtsSpeakRequest,
) -> Result<crate::multimodal_io::TtsSpeakResult, String> {
    guarded_command!("speak_tts", {
        speak_tts_internal(&app, state.inner(), request)
    })
}

#[tauri::command]
fn stop_tts(app: AppHandle, state: State<'_, AppState>) -> Result<bool, String> {
    Ok(stop_tts_internal(&app, state.inner()))
}

fn stop_tts_internal(app: &AppHandle, state: &AppState) -> bool {
    let was_speaking = state.tts_speaking.swap(false, Ordering::AcqRel);
    let _ = app.emit(
        "tts:speech-finished",
        serde_json::json!({
            "stopped": was_speaking,
            "timestamp_ms": crate::util::now_ms(),
        }),
    );
    was_speaking
}

#[tauri::command]
fn test_tts_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: Option<String>,
) -> Result<crate::multimodal_io::TtsSpeakResult, String> {
    guarded_command!("test_tts_provider", {
        let preferred_provider = provider
            .unwrap_or_else(|| "windows_native".to_string())
            .trim()
            .to_string();
        let (voice_settings, language_mode, language_pinned) = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            require_capability_enabled(&settings, RuntimeCapability::VoiceOutputTts)?;
            (
                settings.voice_output_settings.clone(),
                settings.language_mode.clone(),
                settings.language_pinned,
            )
        };

        let fallback_provider = voice_settings.fallback_provider.clone();
        let rate = voice_settings.rate.clamp(0.5, 2.0);
        let volume = voice_settings.volume.clamp(0.0, 1.0);
        let piper_gain_db = voice_settings.piper_gain_db.clamp(-24.0, 6.0);
        let piper_binary_path = voice_settings.piper_binary_path.clone();
        let piper_model_path = voice_settings.piper_model_path.clone();
        let qwen3_runtime_config = resolve_qwen3_tts_runtime_config(&voice_settings);
        let output_device_id = voice_settings.output_device.clone();
        let default_provider_config = voice_settings.default_provider.clone();
        let fallback_provider_config = voice_settings.fallback_provider.clone();
        let windows_voice_id_default = voice_settings.voice_id_windows.clone();
        let windows_voice_id_fallback = voice_settings.voice_id_windows_fallback.clone();
        let auto_voice_by_language_enabled = voice_settings.auto_voice_by_detected_language;
        let sample_text = "Trisper Flow voice output test.";
        let auto_voice_language_hint = if auto_voice_by_language_enabled {
            infer_tts_language_hint(sample_text, &language_mode, language_pinned)
        } else {
            None
        };

        let run_chain = |selected_output_device: &str| {
            crate::multimodal_io::execute_tts_with_fallback(
                &preferred_provider,
                &fallback_provider,
                |lane| match lane {
                    "windows_native" => {
                        let manual_voice_id = resolve_manual_windows_voice_id_for_lane(
                            "windows_native",
                            &default_provider_config,
                            &fallback_provider_config,
                            &windows_voice_id_default,
                            &windows_voice_id_fallback,
                        );
                        let resolved_voice = resolve_windows_voice_for_provider(
                            "windows_native",
                            &manual_voice_id,
                            auto_voice_by_language_enabled,
                            auto_voice_language_hint.as_deref(),
                        );
                        crate::multimodal_io::speak_windows_native(
                            sample_text,
                            rate,
                            volume,
                            selected_output_device,
                            resolved_voice.as_deref(),
                        )
                    }
                    "windows_natural" => {
                        let manual_voice_id = resolve_manual_windows_voice_id_for_lane(
                            "windows_natural",
                            &default_provider_config,
                            &fallback_provider_config,
                            &windows_voice_id_default,
                            &windows_voice_id_fallback,
                        );
                        let resolved_voice = resolve_windows_voice_for_provider(
                            "windows_natural",
                            &manual_voice_id,
                            auto_voice_by_language_enabled,
                            auto_voice_language_hint.as_deref(),
                        );
                        crate::multimodal_io::speak_windows_natural(
                            sample_text,
                            rate,
                            volume,
                            selected_output_device,
                            resolved_voice.as_deref(),
                        )
                    }
                    "local_custom" => crate::multimodal_io::speak_piper(
                        &state.piper_daemon,
                        sample_text,
                        &piper_binary_path,
                        &piper_model_path,
                        rate,
                        piper_effective_volume(volume, piper_gain_db),
                        selected_output_device,
                    ),
                    "qwen3_tts" => speak_qwen3_tts(
                        sample_text,
                        rate,
                        volume,
                        selected_output_device,
                        &qwen3_runtime_config,
                    ),
                    _ => Err(format!("Unknown TTS provider '{}'.", lane)),
                },
            )
        };

        let mut recovered_output_device = false;
        let outcome = match run_chain(&output_device_id) {
            Ok(outcome) => outcome,
            Err(error)
                if output_device_id != "default"
                    && is_tts_output_device_unavailable_error(&error) =>
            {
                tracing::warn!(
                    "test_tts_provider retrying with default output device after unavailable '{}': {}",
                    output_device_id,
                    error
                );
                match run_chain("default") {
                    Ok(outcome) => {
                        recovered_output_device = true;
                        {
                            let mut settings_guard = state
                                .settings
                                .write()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            settings_guard.voice_output_settings.output_device =
                                "default".to_string();
                        }
                        let snapshot = state
                            .settings
                            .read()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .clone();
                        let _ = save_settings_file(&app, &snapshot);
                        let _ = app.emit("settings-changed", snapshot);
                        outcome
                    }
                    Err(retry_error) => {
                        let merged =
                            format!("{error} Retry with default device failed: {retry_error}");
                        tracing::error!(
                            "test_tts_provider failed preferred='{}' fallback='{}' device='{}': {}",
                            preferred_provider,
                            fallback_provider,
                            output_device_id,
                            merged
                        );
                        let _ = app.emit(
                            "tts:speech-error",
                            serde_json::json!({
                                "provider": preferred_provider.clone(),
                                "preferred_provider": preferred_provider.clone(),
                                "fallback_provider": fallback_provider.clone(),
                                "context": "manual_test",
                                "error": merged.clone(),
                                "timestamp_ms": crate::util::now_ms(),
                            }),
                        );
                        return Err(merged);
                    }
                }
            }
            Err(error) => {
                tracing::error!(
                    "test_tts_provider failed preferred='{}' fallback='{}' device='{}': {}",
                    preferred_provider,
                    fallback_provider,
                    output_device_id,
                    error
                );
                let _ = app.emit(
                    "tts:speech-error",
                    serde_json::json!({
                        "provider": preferred_provider.clone(),
                        "preferred_provider": preferred_provider.clone(),
                        "fallback_provider": fallback_provider.clone(),
                        "context": "manual_test",
                        "error": error.clone(),
                        "timestamp_ms": crate::util::now_ms(),
                    }),
                );
                return Err(error);
            }
        };

        let provider_used = outcome.provider_used.clone();
        let primary_error = outcome.primary_error.clone();
        let used_fallback = outcome.used_fallback;

        if used_fallback {
            let _ = app.emit(
                "tts:speech-finished",
                serde_json::json!({
                    "provider_used": provider_used.clone(),
                    "preferred_provider": preferred_provider.clone(),
                    "fallback_provider": fallback_provider.clone(),
                    "used_fallback": true,
                    "primary_error": primary_error.clone(),
                    "output_device_recovered": recovered_output_device,
                    "context": "manual_test",
                    "timestamp_ms": crate::util::now_ms(),
                }),
            );
        }

        Ok(crate::multimodal_io::TtsSpeakResult {
            provider_used: provider_used.clone(),
            accepted: true,
            message: if used_fallback {
                let fallback_msg = format!(
                    "Fallback used: {} -> {}.",
                    preferred_provider, provider_used
                );
                if recovered_output_device {
                    format!("{fallback_msg} Gerät ungültig, auf Default zurückgesetzt.")
                } else {
                    fallback_msg
                }
            } else {
                let base = format!("Provider '{}' responded.", provider_used);
                if recovered_output_device {
                    format!("{base} Gerät ungültig, auf Default zurückgesetzt.")
                } else {
                    base
                }
            },
            used_fallback: Some(used_fallback),
            preferred_provider: Some(preferred_provider),
            fallback_provider: Some(fallback_provider),
            primary_error,
        })
    })
}

#[tauri::command]
fn list_gdd_presets(state: State<'_, AppState>) -> Vec<crate::gdd::GddPreset> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::gdd::list_presets(&settings.gdd_module_settings.preset_clones)
}

#[tauri::command]
fn save_gdd_preset_clone(
    app: AppHandle,
    state: State<'_, AppState>,
    mut preset: crate::gdd::GddPresetClone,
) -> Result<Vec<crate::gdd::GddPreset>, String> {
    guarded_command!("save_gdd_preset_clone", {
        preset.id = preset.id.trim().to_lowercase();
        if preset.id.is_empty() {
            return Err("Preset clone id cannot be empty.".to_string());
        }
        if preset.section_order.is_empty() {
            return Err("Preset clone requires at least one section.".to_string());
        }
        if preset.name.trim().is_empty() {
            return Err("Preset clone name cannot be empty.".to_string());
        }

        let snapshot = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Some(existing) = settings
                .gdd_module_settings
                .preset_clones
                .iter_mut()
                .find(|candidate| candidate.id == preset.id)
            {
                *existing = preset;
            } else {
                settings.gdd_module_settings.preset_clones.push(preset);
            }
            normalize_gdd_module_settings(&mut settings.gdd_module_settings);
            settings.clone()
        };

        save_settings_file(&app, &snapshot)?;
        let _ = app.emit("settings-changed", snapshot.clone());

        Ok(crate::gdd::list_presets(
            &snapshot.gdd_module_settings.preset_clones,
        ))
    })
}

#[tauri::command]
fn detect_gdd_preset(
    state: State<'_, AppState>,
    request: crate::gdd::DetectGddPresetRequest,
) -> crate::gdd::GddRecognitionResult {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let presets = crate::gdd::list_presets(&settings.gdd_module_settings.preset_clones);
    crate::gdd::detect_preset(&request.transcript, &presets)
}

#[tauri::command]
fn generate_gdd_draft(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::gdd::GenerateGddDraftRequest,
) -> Result<crate::gdd::GddDraft, String> {
    guarded_command!("generate_gdd_draft", {
        let _ = app.emit(
            "gdd:generation-started",
            serde_json::json!({ "preset": request.preset_id }),
        );
        let draft = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            crate::gdd::generate_draft(&request, &settings.gdd_module_settings.preset_clones)
        };
        let markdown_preview = crate::gdd::render_storage::render_markdown(&draft);
        let confluence_storage = crate::gdd::render_storage::render_confluence_storage(&draft);
        let _ = app.emit(
            "gdd:generation-progress",
            serde_json::json!({
                "stage": "synthesized",
                "chunk_count": draft.chunk_count,
                "markdown_chars": markdown_preview.len(),
                "storage_chars": confluence_storage.len(),
            }),
        );
        let _ = app.emit("gdd:generation-finished", &draft);
        Ok(draft)
    })
}

#[tauri::command]
fn validate_gdd_draft(draft: crate::gdd::GddDraft) -> crate::gdd::ValidateGddDraftResult {
    crate::gdd::validate_draft(&draft)
}

#[tauri::command]
fn render_gdd_for_confluence(draft: crate::gdd::GddDraft) -> String {
    crate::gdd::render_storage::render_confluence_storage(&draft)
}

#[tauri::command]
fn render_gdd_markdown(draft: crate::gdd::GddDraft) -> String {
    crate::gdd::render_storage::render_markdown(&draft)
}

#[tauri::command]
fn test_confluence_connection(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<crate::gdd::confluence::ConfluenceConnectionResult, String> {
    guarded_command!("test_confluence_connection", {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::gdd::confluence::test_connection(&app, &settings.confluence_settings)
    })
}

#[tauri::command]
fn confluence_oauth_start() -> Result<crate::gdd::confluence::ConfluenceOauthStartResult, String> {
    guarded_command!("confluence_oauth_start", {
        crate::gdd::confluence::oauth_start()
    })
}

#[tauri::command]
fn confluence_oauth_exchange(
    app: AppHandle,
    state: State<'_, AppState>,
    code: String,
) -> Result<serde_json::Value, String> {
    guarded_command!("confluence_oauth_exchange", {
        let exchange_result = {
            let settings = state
                .settings
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            crate::gdd::confluence::oauth_exchange(&app, &settings.confluence_settings, &code)?
        };

        let snapshot = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settings.confluence_settings.enabled = true;
            settings.confluence_settings.auth_mode = "oauth".to_string();
            settings.confluence_settings.site_base_url = exchange_result.selected_site_url.clone();
            settings.confluence_settings.oauth_cloud_id = exchange_result.selected_cloud_id.clone();
            normalize_confluence_settings(&mut settings.confluence_settings);
            settings.clone()
        };

        save_settings_file(&app, &snapshot)?;
        let _ = app.emit("settings-changed", snapshot);

        serde_json::to_value(exchange_result).map_err(|error| error.to_string())
    })
}

#[tauri::command]
fn confluence_list_spaces(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<crate::gdd::confluence::ConfluenceSpace>, String> {
    guarded_command!("confluence_list_spaces", {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::gdd::confluence::list_spaces(&app, &settings.confluence_settings)
    })
}

#[tauri::command]
fn load_gdd_template_from_file(
    file_path: String,
) -> Result<crate::gdd::GddTemplateSourceResult, String> {
    guarded_command!("load_gdd_template_from_file", {
        crate::gdd::load_template_from_file(&file_path)
    })
}

#[tauri::command]
fn load_gdd_template_from_confluence(
    app: AppHandle,
    state: State<'_, AppState>,
    source_url: String,
) -> Result<crate::gdd::GddTemplateSourceResult, String> {
    guarded_command!("load_gdd_template_from_confluence", {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let page = crate::gdd::confluence::load_page_template_from_url(
            &app,
            &settings.confluence_settings,
            &source_url,
        )?;
        Ok(crate::gdd::template_sources::from_confluence_page(
            page.source_url,
            page.page_title,
            page.text,
        ))
    })
}

#[tauri::command]
fn suggest_confluence_target(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::gdd::confluence::ConfluenceTargetSuggestionRequest,
) -> Result<crate::gdd::confluence::ConfluenceTargetSuggestion, String> {
    guarded_command!("suggest_confluence_target", {
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        crate::gdd::confluence::suggest_target(&app, &settings.confluence_settings, &request)
    })
}

#[tauri::command]
fn publish_gdd_to_confluence(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::gdd::confluence::ConfluencePublishRequest,
) -> Result<crate::gdd::confluence::ConfluencePublishResult, String> {
    guarded_command!("publish_gdd_to_confluence", {
        let _ = app.emit(
            "gdd:publish-started",
            serde_json::json!({ "title": request.title }),
        );

        let settings_snapshot = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let result =
            crate::gdd::confluence::publish(&app, &settings_snapshot.confluence_settings, &request);

        match result {
            Ok(publish) => {
                {
                    let mut settings = state
                        .settings
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let route_key =
                        crate::gdd::confluence::routing_key_for(&request.space_key, &request.title);
                    settings
                        .confluence_settings
                        .routing_memory
                        .insert(route_key, publish.page_id.clone());
                    normalize_confluence_settings(&mut settings.confluence_settings);
                    let _ = save_settings_file(&app, &settings);
                    let _ = app.emit("settings-changed", settings.clone());
                }
                let _ = app.emit("gdd:publish-finished", &publish);
                Ok(publish)
            }
            Err(error) => {
                let _ = app.emit(
                    "gdd:publish-failed",
                    serde_json::json!({ "title": request.title, "error": error }),
                );
                Err(error)
            }
        }
    })
}

#[tauri::command]
fn publish_or_queue_gdd_to_confluence(
    app: AppHandle,
    state: State<'_, AppState>,
    request: crate::gdd::publish_queue::GddPublishOrQueueRequest,
) -> Result<crate::gdd::publish_queue::GddPublishAttemptResult, String> {
    guarded_command!("publish_or_queue_gdd_to_confluence", {
        let _ = app.emit(
            "gdd:publish-started",
            serde_json::json!({ "title": request.publish_request.title }),
        );

        let settings_snapshot = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let publish_result = crate::gdd::confluence::publish(
            &app,
            &settings_snapshot.confluence_settings,
            &request.publish_request,
        );

        match publish_result {
            Ok(publish) => {
                {
                    let mut settings = state
                        .settings
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let route_key = crate::gdd::confluence::routing_key_for(
                        &request.publish_request.space_key,
                        &request.publish_request.title,
                    );
                    settings
                        .confluence_settings
                        .routing_memory
                        .insert(route_key, publish.page_id.clone());
                    normalize_confluence_settings(&mut settings.confluence_settings);
                    let _ = save_settings_file(&app, &settings);
                    let _ = app.emit("settings-changed", settings.clone());
                }

                let _ = app.emit("gdd:publish-finished", &publish);
                Ok(crate::gdd::publish_queue::GddPublishAttemptResult {
                    status: "published".to_string(),
                    publish_result: Some(publish),
                    queued_job: None,
                    error: None,
                })
            }
            Err(error) => {
                if crate::gdd::publish_queue::is_queueable_publish_error(&error) {
                    let queued_job =
                        crate::gdd::publish_queue::queue_publish_request(&app, &request, &error)?;
                    let _ = app.emit(
                        "gdd:publish-queued",
                        serde_json::json!({
                            "job_id": queued_job.job_id,
                            "title": queued_job.title,
                            "error": error,
                        }),
                    );
                    return Ok(crate::gdd::publish_queue::GddPublishAttemptResult {
                        status: "queued".to_string(),
                        publish_result: None,
                        queued_job: Some(queued_job),
                        error: Some(error),
                    });
                }

                let _ = app.emit(
                    "gdd:publish-failed",
                    serde_json::json!({
                        "title": request.publish_request.title,
                        "error": error,
                    }),
                );
                Ok(crate::gdd::publish_queue::GddPublishAttemptResult {
                    status: "failed".to_string(),
                    publish_result: None,
                    queued_job: None,
                    error: Some(error),
                })
            }
        }
    })
}

#[tauri::command]
fn list_pending_gdd_publishes(
    app: AppHandle,
) -> Result<Vec<crate::gdd::publish_queue::GddPendingPublishJob>, String> {
    guarded_command!("list_pending_gdd_publishes", {
        crate::gdd::publish_queue::list_pending_jobs(&app)
    })
}

#[tauri::command]
fn retry_pending_gdd_publish(
    app: AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> Result<crate::gdd::publish_queue::GddPublishAttemptResult, String> {
    guarded_command!("retry_pending_gdd_publish", {
        let job_id = job_id.trim().to_string();
        if job_id.is_empty() {
            return Err("job_id is required.".to_string());
        }
        let mut job = crate::gdd::publish_queue::load_pending_job(&app, &job_id)?
            .ok_or_else(|| format!("Pending publish job '{}' not found.", job_id))?;
        let publish_request = crate::gdd::publish_queue::load_publish_request_for_job(&job)?;

        let _ = app.emit(
            "gdd:publish-started",
            serde_json::json!({ "title": publish_request.title, "job_id": job.job_id }),
        );

        let settings_snapshot = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let publish_result = crate::gdd::confluence::publish(
            &app,
            &settings_snapshot.confluence_settings,
            &publish_request,
        );

        match publish_result {
            Ok(publish) => {
                let _ = crate::gdd::publish_queue::consume_pending_job(&app, &job.job_id)?;
                {
                    let mut settings = state
                        .settings
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let route_key = crate::gdd::confluence::routing_key_for(
                        &publish_request.space_key,
                        &publish_request.title,
                    );
                    settings
                        .confluence_settings
                        .routing_memory
                        .insert(route_key, publish.page_id.clone());
                    normalize_confluence_settings(&mut settings.confluence_settings);
                    let _ = save_settings_file(&app, &settings);
                    let _ = app.emit("settings-changed", settings.clone());
                }
                let _ = app.emit("gdd:publish-finished", &publish);
                Ok(crate::gdd::publish_queue::GddPublishAttemptResult {
                    status: "published".to_string(),
                    publish_result: Some(publish),
                    queued_job: None,
                    error: None,
                })
            }
            Err(error) => {
                crate::gdd::publish_queue::mark_retry_failure(&mut job, &error);
                crate::gdd::publish_queue::persist_pending_job(&app, &job)?;
                let _ = app.emit(
                    "gdd:publish-failed",
                    serde_json::json!({
                        "title": publish_request.title,
                        "error": error,
                        "job_id": job.job_id,
                    }),
                );
                Ok(crate::gdd::publish_queue::GddPublishAttemptResult {
                    status: if crate::gdd::publish_queue::is_queueable_publish_error(&error) {
                        "queued".to_string()
                    } else {
                        "failed".to_string()
                    },
                    publish_result: None,
                    queued_job: Some(job),
                    error: Some(error),
                })
            }
        }
    })
}

#[tauri::command]
fn delete_pending_gdd_publish(app: AppHandle, job_id: String) -> Result<bool, String> {
    guarded_command!("delete_pending_gdd_publish", {
        let job_id = job_id.trim();
        if job_id.is_empty() {
            return Err("job_id is required.".to_string());
        }
        crate::gdd::publish_queue::delete_pending_job(&app, job_id)
    })
}

#[tauri::command]
fn save_confluence_secret(
    app: AppHandle,
    state: State<'_, AppState>,
    secret_id: String,
    secret_value: String,
) -> Result<serde_json::Value, String> {
    guarded_command!("save_confluence_secret", {
        let secret_id = secret_id.trim().to_lowercase();
        confluence::keyring::store_secret(&app, &secret_id, &secret_value)?;

        let snapshot = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settings.confluence_settings.enabled = true;
            settings.clone()
        };
        save_settings_file(&app, &snapshot)?;
        let _ = app.emit("settings-changed", snapshot);

        Ok(serde_json::json!({
            "status": "success",
            "secret_id": secret_id
        }))
    })
}

#[tauri::command]
fn clear_confluence_secret(app: AppHandle, secret_id: String) -> Result<serde_json::Value, String> {
    guarded_command!("clear_confluence_secret", {
        let secret_id = secret_id.trim().to_lowercase();
        confluence::keyring::clear_secret(&app, &secret_id)?;
        Ok(serde_json::json!({
            "status": "success",
            "secret_id": secret_id
        }))
    })
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
fn get_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state
        .history
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .active
        .iter()
        .cloned()
        .collect()
}

#[tauri::command]
fn get_transcribe_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state
        .history_transcribe
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .active
        .iter()
        .cloned()
        .collect()
}

#[tauri::command]
fn clear_active_transcript_history(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    let mic_deleted = {
        let mut history = state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deleted = history.active.len() as u64;
        history.active.clear();
        history.flush_to_disk()?;
        let updated: Vec<_> = history.active.iter().cloned().collect();
        drop(history);
        let _ = app.emit("history:updated", updated);
        deleted
    };

    let system_deleted = {
        let mut history = state
            .history_transcribe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let deleted = history.active.len() as u64;
        history.active.clear();
        history.flush_to_disk()?;
        let updated: Vec<_> = history.active.iter().cloned().collect();
        drop(history);
        let _ = app.emit("transcribe:history-updated", updated);
        deleted
    };

    Ok(mic_deleted + system_deleted)
}

#[tauri::command]
fn delete_active_transcript_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    entry_id: String,
) -> Result<u64, String> {
    let entry_id = entry_id.trim();
    if entry_id.is_empty() {
        return Ok(0);
    }

    let mic_deleted = {
        let mut history = state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = history.active.len();
        history.active.retain(|entry| entry.id != entry_id);
        let deleted = before.saturating_sub(history.active.len()) as u64;
        if deleted > 0 {
            history.flush_to_disk()?;
            let updated: Vec<_> = history.active.iter().cloned().collect();
            drop(history);
            let _ = app.emit("history:updated", updated);
        }
        deleted
    };

    let system_deleted = {
        let mut history = state
            .history_transcribe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = history.active.len();
        history.active.retain(|entry| entry.id != entry_id);
        let deleted = before.saturating_sub(history.active.len()) as u64;
        if deleted > 0 {
            history.flush_to_disk()?;
            let updated: Vec<_> = history.active.iter().cloned().collect();
            drop(history);
            let _ = app.emit("transcribe:history-updated", updated);
        }
        deleted
    };

    Ok(mic_deleted + system_deleted)
}

#[tauri::command]
fn list_history_partitions(
    app: AppHandle,
    kind: String,
) -> Result<Vec<crate::history_partition::PartitionInfo>, String> {
    let state = app.state::<AppState>();
    match kind.as_str() {
        "mic" => Ok(state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .list_partitions()),
        "system" => Ok(state
            .history_transcribe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .list_partitions()),
        _ => Err(format!("Unknown history kind: {}", kind)),
    }
}

#[tauri::command]
fn load_history_partition(
    app: AppHandle,
    kind: String,
    key: String,
) -> Result<Vec<HistoryEntry>, String> {
    let state = app.state::<AppState>();
    let pk = crate::history_partition::PartitionKey::parse(&key)?;
    match kind.as_str() {
        "mic" => Ok(state
            .history
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .load_partition(&pk)),
        "system" => Ok(state
            .history_transcribe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .load_partition(&pk)),
        _ => Err(format!("Unknown history kind: {}", kind)),
    }
}

#[tauri::command]
fn add_history_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
    source: Option<String>,
) -> Result<Vec<HistoryEntry>, String> {
    let source = source.unwrap_or_else(|| "local".to_string());
    push_history_entry_inner(&app, &state.history, text, source)
}

#[tauri::command]
fn add_transcribe_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    text: String,
) -> Result<Vec<HistoryEntry>, String> {
    push_transcribe_entry_inner(&app, &state.history_transcribe, text)
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
fn paste_transcript_text(text: String) -> Result<(), String> {
    paste_text(&text)
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
            stop_transcribe_monitor(&app, &state);
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

#[tauri::command]
async fn unload_ollama_model(model: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || unload_ollama_model_impl(model))
        .await
        .map_err(|e| format!("Unload Ollama model task failed: {}", e))?
}

fn unload_ollama_model_impl(model: String) -> Result<(), String> {
    // Send a request to Ollama to unload the model from VRAM.
    // This uses a minimal POST to /api/generate with keep_alive: "0m" to signal
    // that the model should be unloaded immediately.

    let ollama_endpoint = "http://127.0.0.1:11434";
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

    let url = format!("{}/api/generate", ollama_endpoint);

    // Fire and forget — we don't care about the response, just sending the unload signal
    let _ = agent
        .post(&url)
        .set("Content-Type", "application/json")
        .send_json(&unload_body);

    Ok(())
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

#[tauri::command]
fn purge_gpu_memory(state: State<'_, AppState>) -> Result<(), String> {
    // Purge GPU memory by unloading all loaded models from both Ollama and Whisper

    // Unload current Ollama model if set
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let current_ollama_model = settings.ai_fallback.model.clone();
    drop(settings);

    if !current_ollama_model.is_empty() {
        let _ = unload_ollama_model_impl(current_ollama_model);
    }

    // Kill and restart Whisper server to clear old model
    // This is the most reliable way to free VRAM from Whisper
    let _ = crate::whisper_server::kill_whisper_server(&state);
    state
        .whisper_server_warmup_started
        .store(false, Ordering::Relaxed);

    Ok(())
}

#[tauri::command]
async fn stop_ollama_runtime(app: AppHandle) -> Result<(), String> {
    // Stop the managed Ollama runtime process
    // This is called when user disables AI refinement or exits the app
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
fn install_lm_studio() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Opens a new PowerShell console window running the official LM Studio install script.
        // Uses `cmd /C start` to ensure a visible window is spawned even from a windowless
        // Tauri process. CREATE_NO_WINDOW on the cmd.exe wrapper avoids a brief flash.
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

#[tauri::command]
fn validate_hotkey(key: String) -> hotkeys::ValidationResult {
    hotkeys::validate_hotkey_format(&key)
}

#[tauri::command]
fn test_hotkey(app: AppHandle, key: String) -> Result<(), String> {
    hotkeys::test_hotkey_registration(&app, &key)
}

#[tauri::command]
fn get_hotkey_conflicts(state: State<'_, AppState>) -> Vec<hotkeys::ConflictInfo> {
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let hotkeys = vec![
        settings.hotkey_ptt.clone(),
        settings.hotkey_toggle.clone(),
        settings.transcribe_hotkey.clone(),
        settings.hotkey_product_mode_toggle.clone(),
    ];
    hotkeys::detect_conflicts(hotkeys)
}

#[tauri::command]
fn save_transcript(filename: String, content: String, format: String) -> Result<String, String> {
    // Determine file extension based on format
    let extension = match format.as_str() {
        "txt" => "txt",
        "md" => "md",
        "json" => "json",
        _ => "txt", // fallback
    };

    // Show save file dialog
    let file_path = rfd::FileDialog::new()
        .set_file_name(&filename)
        .add_filter(&format.to_uppercase(), &[extension])
        .save_file()
        .ok_or("File save cancelled")?;

    // Write content to file
    std::fs::write(&file_path, content).map_err(|e| format!("Failed to write file: {}", e))?;

    // Return the saved file path
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
fn save_crash_recovery(app: AppHandle, content: String) -> Result<(), String> {
    // Write to app data dir (user-private) instead of world-readable %TEMP%
    let data_dir = crate::paths::resolve_base_dir(&app);
    let _ = std::fs::create_dir_all(&data_dir);

    let crash_file = data_dir.join(".crash_recovery.json");
    std::fs::write(&crash_file, content)
        .map_err(|e| format!("Failed to save crash recovery: {}", e))?;

    Ok(())
}

#[tauri::command]
fn clear_crash_recovery(app: AppHandle) -> Result<(), String> {
    let data_dir = crate::paths::resolve_base_dir(&app);

    let crash_file = data_dir.join(".crash_recovery.json");
    if crash_file.exists() {
        std::fs::remove_file(&crash_file)
            .map_err(|e| format!("Failed to clear crash recovery: {}", e))?;
    }

    // Also clean up legacy TEMP file if it exists
    let legacy_temp = if cfg!(windows) {
        std::env::var("TEMP").ok()
    } else {
        std::env::var("TMPDIR").ok().or(Some("/tmp".to_string()))
    };
    if let Some(temp_dir) = legacy_temp {
        let legacy_file = PathBuf::from(&temp_dir).join("trispr_crash_recovery.json");
        let _ = std::fs::remove_file(&legacy_file); // best-effort cleanup
    }

    Ok(())
}

/// Validate that a path resolves within the allowed root directory.
/// For existing files: canonicalize the full path.
/// For new files (output): canonicalize the parent directory.
fn validate_path_within(
    path_str: &str,
    allowed_root: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    // Reject UNC paths (\\server\share) before canonicalize — they trigger an SMB
    // round-trip which can leak NTLM credentials even if starts_with() later rejects them.
    if path_str.starts_with("\\\\") || path_str.starts_with("//") {
        return Err(format!("UNC paths are not allowed: '{}'", path_str));
    }
    let path = std::path::PathBuf::from(path_str);

    // For existing files, canonicalize directly
    if path.exists() {
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Cannot resolve path '{}': {}", path_str, e))?;
        if !canonical.starts_with(allowed_root) {
            return Err(format!("Path '{}' is outside allowed directory", path_str));
        }
        return Ok(canonical);
    }

    // For non-existing files (e.g. output), canonicalize the parent
    let parent = path
        .parent()
        .ok_or_else(|| format!("Path '{}' has no parent directory", path_str))?;
    if !parent.exists() {
        return Err(format!("Parent directory of '{}' does not exist", path_str));
    }
    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| format!("Cannot resolve parent of '{}': {}", path_str, e))?;
    if !canonical_parent.starts_with(allowed_root) {
        return Err(format!("Path '{}' is outside allowed directory", path_str));
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("Path '{}' has no file name", path_str))?;
    Ok(canonical_parent.join(file_name))
}

#[tauri::command]
fn encode_to_opus(
    app: tauri::AppHandle,
    input_path: String,
    output_path: String,
    bitrate_kbps: Option<u32>,
) -> Result<opus::OpusEncodeResult, String> {
    let allowed_root = crate::paths::resolve_base_dir(&app);

    let input = validate_path_within(&input_path, &allowed_root)?;
    let output = validate_path_within(&output_path, &allowed_root)?;

    if let Some(bitrate) = bitrate_kbps {
        let mut config = opus::OpusEncoderConfig::default();
        config.bitrate_kbps = bitrate;
        opus::encode_wav_to_opus(&input, &output, &config)
    } else {
        opus::encode_wav_to_opus_default(&input, &output)
    }
}

#[tauri::command]
fn check_ffmpeg() -> Result<bool, String> {
    Ok(opus::check_ffmpeg_available())
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

    let model_ready = check_model_available(app.clone(), settings_snapshot.model.clone());
    if model_ready {
        items.push(DependencyPreflightItem {
            id: "whisper_model".to_string(),
            status: "ok".to_string(),
            required: true,
            message: format!("Speech model '{}' is available.", settings_snapshot.model),
            hint: None,
        });
    } else {
        items.push(DependencyPreflightItem {
            id: "whisper_model".to_string(),
            status: "error".to_string(),
            required: true,
            message: format!(
                "Speech model '{}' is not installed.",
                settings_snapshot.model
            ),
            hint: Some("Download a model in Whisper Model Manager.".to_string()),
        });
    }

    let quantize = paths::resolve_quantize_path(app);
    if let Some(path) = quantize {
        items.push(DependencyPreflightItem {
            id: "quantize_binary".to_string(),
            status: "ok".to_string(),
            required: false,
            message: format!("Quantize binary found: {}", path.display()),
            hint: None,
        });
    } else {
        items.push(DependencyPreflightItem {
            id: "quantize_binary".to_string(),
            status: "warning".to_string(),
            required: false,
            message: "Model optimization binary (quantize.exe) not found.".to_string(),
            hint: Some(
                "Optimize in Whisper Model Manager will be unavailable until quantize.exe is bundled."
                    .to_string(),
            ),
        });
    }

    match opus::find_ffmpeg_for_opus() {
        Ok(path) => items.push(DependencyPreflightItem {
            id: "ffmpeg".to_string(),
            status: "ok".to_string(),
            required: false,
            message: format!("FFmpeg with libopus found: {}", path.display()),
            hint: None,
        }),
        Err(error) => items.push(DependencyPreflightItem {
            id: "ffmpeg".to_string(),
            status: "warning".to_string(),
            required: false,
            message: "FFmpeg with libopus encoder is not available.".to_string(),
            hint: Some(format!(
                "{} OPUS encode/merge features may not work until FFmpeg is available.",
                error
            )),
        }),
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

#[tauri::command]
fn get_ffmpeg_version_info() -> Result<String, String> {
    opus::get_ffmpeg_version()
}

#[tauri::command]
fn get_last_recording_path(
    source: String, // "mic" or "output"
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let path = if source == "output" || source == "system" {
        state
            .last_system_recording_path
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    } else {
        state
            .last_mic_recording_path
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    };
    Ok(path)
}

#[tauri::command]
fn get_recordings_directory(app: AppHandle) -> Result<String, String> {
    let data_dir = crate::paths::resolve_base_dir(&app);
    let recordings_dir = data_dir.join("recordings");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|e| format!("Failed to create recordings dir: {}", e))?;

    Ok(recordings_dir.to_string_lossy().to_string())
}

#[tauri::command]
fn open_recordings_directory(app: AppHandle) -> Result<(), String> {
    let recordings_dir = get_recordings_directory(app.clone())?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&recordings_dir)
            .spawn()
            .map_err(|e| format!("Failed to open directory: {}", e))?;
    }

    Ok(())
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
fn lms_daemon_command(action: &str) {
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
        kitt_min_width: settings.overlay_kitt_min_width as f64,
        kitt_max_width: settings.overlay_kitt_max_width as f64,
        kitt_height: settings.overlay_kitt_height as f64,
    }
}

fn init_logging() {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Write logs to %LOCALAPPDATA%\Trispr Flow\logs\trispr-flow.YYYY-MM-DD.txt (daily rotation)
    let log_dir = std::env::var("LOCALAPPDATA")
        .map(|d| std::path::PathBuf::from(d).join("Trispr Flow").join("logs"))
        .unwrap_or_else(|_| std::path::PathBuf::from("logs"));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("trispr-flow")
        .filename_suffix("txt")
        .build(&log_dir)
        .expect("failed to initialize log appender");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Keep the guard alive for the process lifetime
    std::mem::forget(_guard);

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_target(false)
                .with_thread_ids(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(non_blocking)
                .with_ansi(false),
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

fn latency_benchmark_request_from_env() -> LatencyBenchmarkRequest {
    let mut request = LatencyBenchmarkRequest::default();

    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_WARMUP_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.warmup_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_MEASURE_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.measure_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_INCLUDE_REFINEMENT") {
        request.include_refinement = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_FIXTURES") {
        let fixtures = value
            .split(';')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        if !fixtures.is_empty() {
            request.fixture_paths = fixtures;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_REFINE_MODEL") {
        let model = value.trim();
        if !model.is_empty() {
            request.refinement_model = Some(model.to_string());
        }
    }

    request
}

fn tts_benchmark_request_from_env() -> TtsBenchmarkRequest {
    let mut request = TtsBenchmarkRequest::default();

    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_WARMUP_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.warmup_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_MEASURE_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.measure_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_RATE") {
        if let Ok(parsed) = value.trim().parse::<f32>() {
            request.rate = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_VOLUME") {
        if let Ok(parsed) = value.trim().parse::<f32>() {
            request.volume = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_PROVIDERS") {
        let providers = value
            .split(';')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        if !providers.is_empty() {
            request.providers = providers;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_PIPER_BINARY_PATH") {
        let path = value.trim();
        if !path.is_empty() {
            request.piper_binary_path = Some(path.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_PIPER_MODEL_PATH") {
        let path = value.trim();
        if !path.is_empty() {
            request.piper_model_path = Some(path.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_ENDPOINT") {
        let endpoint = value.trim();
        if !endpoint.is_empty() {
            request.qwen3_tts_endpoint = Some(endpoint.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_MODEL") {
        let model = value.trim();
        if !model.is_empty() {
            request.qwen3_tts_model = Some(model.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_VOICE") {
        let voice = value.trim();
        if !voice.is_empty() {
            request.qwen3_tts_voice = Some(voice.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_API_KEY") {
        let key = value.trim();
        if !key.is_empty() {
            request.qwen3_tts_api_key = Some(key.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_TIMEOUT_SEC") {
        if let Ok(parsed) = value.trim().parse::<u64>() {
            request.qwen3_tts_timeout_sec = Some(parsed);
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_LOCK_MATRIX") {
        request.lock_matrix = !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        );
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_RUNTIME_SMOKE") {
        request.run_runtime_smoke = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }

    request
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

pub(crate) fn paste_text(text: &str) -> Result<(), String> {
    let snapshot = capture_clipboard_snapshot_with_retry();
    set_clipboard_text_with_retry(text)?;

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

fn show_main_window(app: &AppHandle) {
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
        let (next_mode, snapshot) = {
            let mut settings = state
                .settings
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settings.product_mode = if settings.product_mode.trim().eq_ignore_ascii_case("assistant") {
                "transcribe".to_string()
            } else {
                "assistant".to_string()
            };
            normalize_product_mode_field(&mut settings);
            let next_mode = settings.product_mode.clone();
            let snapshot = settings.clone();
            let _ = save_settings_file(&app, &settings);
            (next_mode, snapshot)
        };

        let _ = app.emit("settings-changed", snapshot.clone());
        let _ = emit_assistant_baseline_state(&app, state.inner(), &snapshot, "hotkey_toggle_product_mode");
        let cue = if next_mode == "assistant" { "start" } else { "stop" };
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
        error!("PANIC at {}: {}", location, payload);
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

            let settings = load_settings(app.handle());

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
                piper_daemon: crate::multimodal_io::PiperDaemonState::default(),
                #[cfg(target_os = "windows")]
                system_cluster_buffer: Mutex::new(state::SystemClusterBuffer::default()),
                #[cfg(target_os = "windows")]
                managed_process_job: create_managed_process_job(),
            });

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

            // Pre-warm whisper capability probe in background so the first PTT transcription
            // doesn't pay the 2-3s CUDA init cost for the -ngl support check.
            {
                let handle = app.handle().clone();
                crate::util::spawn_guarded("prewarm_whisper", move || {
                    let state = handle.state::<AppState>();
                    let settings = state.settings.read().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
                    if let Some(cli_path) = crate::paths::resolve_whisper_cli_path_for_backend(
                        Some(settings.local_backend_preference.as_str()),
                    ) {
                        crate::transcription::prewarm_whisper_capability_cache(&cli_path);
                    }
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
                    let request = latency_benchmark_request_from_env();
                    let result = {
                        let state = app_handle.state::<AppState>();
                        run_latency_benchmark_inner(&app_handle, state.inner(), &request)
                    };

                    match result {
                        Ok(report) => match write_latency_benchmark_report(&report) {
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
                    let request = tts_benchmark_request_from_env();
                    let result = {
                        let state = app_handle.state::<AppState>();
                        run_tts_benchmark_inner(state.inner(), &request)
                    };

                    match result {
                        Ok(report) => match write_tts_benchmark_report(&report) {
                            Ok(path) => {
                                info!(
                                    "TTS benchmark complete: recommended_default={:?} release_gate_pass={} (report: {})",
                                    report.recommended_default_provider,
                                    report.release_gate_pass,
                                    path.display()
                                );
                                if let Some(provider) = report.recommended_default_provider.as_ref()
                                {
                                    info!(
                                        "TTS benchmark recommendation: provider='{}' reason='{}'",
                                        provider, report.recommendation_reason
                                    );
                                } else {
                                    warn!(
                                        "TTS benchmark produced no recommendation: {}",
                                        report.recommendation_reason
                                    );
                                }
                                if !report.release_gate_pass {
                                    warn!(
                                        "TTS release gate failed: {}",
                                        report.release_gate_reason
                                    );
                                }
                                if report.uncategorized_failure_count > 0 {
                                    warn!(
                                        "TTS benchmark uncategorized failures: {}",
                                        report.uncategorized_failure_count
                                    );
                                }
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
                eprintln!("⚠ Failed to register hotkeys: {}", err);
            }
            info!("[DIAG] setup: hotkeys done");

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
                        eprintln!("⚠ Failed to start VAD monitor: {}", err);
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
            get_startup_status,
            get_runtime_diagnostics,
            save_settings,
            save_window_state,
            save_window_visibility_state,
            list_modules,
            enable_module,
            disable_module,
            get_module_health,
            check_module_updates,
            agent_list_supported_actions,
            agent_parse_command,
            agent_compose_unknown_reply,
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
            list_gdd_presets,
            save_gdd_preset_clone,
            detect_gdd_preset,
            generate_gdd_draft,
            validate_gdd_draft,
            render_gdd_for_confluence,
            render_gdd_markdown,
            test_confluence_connection,
            confluence_oauth_start,
            confluence_oauth_exchange,
            confluence_list_spaces,
            load_gdd_template_from_file,
            load_gdd_template_from_confluence,
            suggest_confluence_target,
            publish_gdd_to_confluence,
            publish_or_queue_gdd_to_confluence,
            list_pending_gdd_publishes,
            retry_pending_gdd_publish,
            delete_pending_gdd_publish,
            save_confluence_secret,
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
