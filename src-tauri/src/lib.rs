// Trispr Flow - core app runtime
#![allow(clippy::needless_return)]

mod audio;
mod auto_processing;
mod constants;
mod errors;
mod hotkeys;
mod models;
mod opus;
mod overlay;
mod paths;
mod postprocessing;
mod session_manager;
mod sidecar;
mod sidecar_process;
mod state;
mod transcription;
mod util;

use arboard::{Clipboard, ImageData};
use enigo::{Enigo, Key, KeyboardControllable};
use errors::{AppError, ErrorEvent};
use overlay::{update_overlay_state, OverlayState};
use state::{AppState, HistoryEntry, Settings};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::menu::{CheckMenuItem, MenuItem};
use tauri::Wry;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{error, info, warn};

use crate::audio::{list_audio_devices, list_output_devices, start_recording, stop_recording};
use crate::models::{
    check_model_available, clear_hidden_external_models, download_model, get_models_dir,
    hide_external_model, list_models, pick_model_dir, quantize_model, remove_model,
};
use crate::state::{
    load_history, load_settings, load_transcribe_history, push_history_entry_inner,
    push_transcribe_entry_inner, save_settings_file, sync_model_dir_env,
};
use crate::transcription::{
    expand_transcribe_backlog as expand_transcribe_backlog_inner, start_transcribe_monitor,
    stop_transcribe_monitor, toggle_transcribe_state, transcribe_audio,
};

const TRAY_CLICK_DEBOUNCE_MS: u64 = 250;
const TRAY_ICON_ID: &str = "main-tray";
const TRAY_PULSE_FRAMES: usize = 6;
const TRAY_PULSE_CYCLE_MS: u64 = 1600;
const BACKLOG_AUTOEXPAND_TIMEOUT_MS: u64 = 5_000;

static LAST_TRAY_CLICK_MS: AtomicU64 = AtomicU64::new(0);
static TRAY_CAPTURE_STATE: AtomicU8 = AtomicU8::new(0);
static TRAY_TRANSCRIBE_STATE: AtomicU8 = AtomicU8::new(0);
static TRAY_PULSE_STARTED: AtomicBool = AtomicBool::new(false);
static BACKLOG_PROMPT_ACTIVE: AtomicBool = AtomicBool::new(false);
static BACKLOG_PROMPT_CANCELLED: AtomicBool = AtomicBool::new(false);
static MAIN_WINDOW_RESTORED: AtomicBool = AtomicBool::new(false);

fn cancel_backlog_auto_expand(app: &AppHandle) {
    BACKLOG_PROMPT_CANCELLED.store(true, Ordering::Release);
    if let Some(tray) = app.tray_by_id(TRAY_ICON_ID) {
        let _ = tray.set_show_menu_on_left_click(false);
    }
}

fn schedule_backlog_auto_expand(app: AppHandle, cancel_item: MenuItem<Wry>) {
    if BACKLOG_PROMPT_ACTIVE.swap(true, Ordering::AcqRel) {
        return;
    }
    BACKLOG_PROMPT_CANCELLED.store(false, Ordering::Release);
    if let Some(tray) = app.tray_by_id(TRAY_ICON_ID) {
        let _ = tray.set_show_menu_on_left_click(true);
    }
    let _ = cancel_item.set_enabled(true);
    let _ = cancel_item.set_text(format!(
        "Cancel Auto-Expand ({}s)",
        BACKLOG_AUTOEXPAND_TIMEOUT_MS / 1000
    ));

    std::thread::spawn(move || {
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
        if let Some(tray) = app.tray_by_id(TRAY_ICON_ID) {
            let _ = tray.set_show_menu_on_left_click(false);
        }
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
                info!("PTT hotkey pressed");
                let _ = crate::audio::handle_ptt_press(&app);
            } else {
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
                    .lock()
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

    // Report all errors if any occurred, but don't fail completely
    if !errors.is_empty() {
        let error_msg = format!("Some hotkeys failed to register: {}", errors.join(", "));
        warn!("{}", error_msg);
        // Return Ok to prevent blocking the app, errors already emitted to UI
        Ok(())
    } else {
        info!("All hotkeys registered successfully");
        Ok(())
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

fn set_transcribe_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = {
        let mut current = state.settings.lock().unwrap();
        if current.transcribe_enabled == enabled {
            return Ok(());
        }
        current.transcribe_enabled = enabled;
        current.clone()
    };

    if enabled {
        if let Err(err) = start_transcribe_monitor(app, &state, &settings) {
            let reverted = {
                let mut current = state.settings.lock().unwrap();
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
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), String> {
    let (prev_mode, prev_device, prev_capture_enabled, prev_transcribe_enabled) = {
        let current = state.settings.lock().unwrap();
        (
            current.mode.clone(),
            current.input_device.clone(),
            current.capture_enabled,
            current.transcribe_enabled,
        )
    };
    {
        let mut current = state.settings.lock().unwrap();
        *current = settings.clone();
    }
    sync_model_dir_env(&settings);
    save_settings_file(&app, &settings)?;
    register_hotkeys(&app, &settings)?;

    if let Ok(recorder) = state.recorder.lock() {
        recorder.input_gain_db.store(
            (settings.mic_input_gain_db * 1000.0) as i64,
            Ordering::Relaxed,
        );
    }

    let mode_changed = prev_mode != settings.mode;
    let device_changed = prev_device != settings.input_device;

    if mode_changed || (device_changed && settings.mode == "vad") {
        if prev_mode == "vad" || (settings.mode == "vad" && device_changed) {
            crate::audio::stop_vad_monitor(&app, &state);
        }
        if settings.mode == "vad" && settings.capture_enabled {
            let _ = crate::audio::start_vad_monitor(&app, &state, &settings);
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
            crate::audio::stop_vad_monitor(&app, &state);
        } else if settings.mode == "vad" {
            let _ = crate::audio::start_vad_monitor(&app, &state, &settings);
        }
    }

    let transcribe_enabled_changed = prev_transcribe_enabled != settings.transcribe_enabled;
    if transcribe_enabled_changed {
        if !settings.transcribe_enabled {
            stop_transcribe_monitor(&app, &state);
        } else {
            let _ = start_transcribe_monitor(&app, &state, &settings);
        }
    }

    let overlay_settings = build_overlay_settings(&settings);
    let _ = overlay::apply_overlay_settings(&app, &overlay_settings);

    let recorder = state.recorder.lock().unwrap();
    if !recorder.active {
        let _ = update_overlay_state(&app, OverlayState::Idle);
    }
    drop(recorder);

    let _ = app.emit("settings-changed", settings.clone());
    let _ = app.emit("menu:update-mic", settings.capture_enabled);
    let _ = app.emit("menu:update-transcribe", settings.transcribe_enabled);
    Ok(())
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

    let mut current = state.settings.lock().unwrap();

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

    let settings = current.clone();
    drop(current);

    save_settings_file(&app, &settings)?;
    Ok(())
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state.history.lock().unwrap().clone()
}

#[tauri::command]
fn get_transcribe_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
    state.history_transcribe.lock().unwrap().clone()
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
fn get_chapters(state: State<'_, AppState>) -> Vec<state::Chapter> {
    state.chapters.lock().unwrap().clone()
}

#[tauri::command]
fn add_chapter(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    label: String,
    timestamp_ms: u64,
    entry_count: u32,
) -> Result<Vec<state::Chapter>, String> {
    let new_chapter = state::Chapter {
        id,
        label,
        timestamp_ms,
        entry_count,
    };

    let mut chapters = state.chapters.lock().unwrap();
    chapters.push(new_chapter);
    let updated = chapters.clone();
    drop(chapters);

    state::save_chapters_file(&app, &updated)?;
    Ok(updated)
}

#[tauri::command]
fn update_chapter(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    label: String,
) -> Result<Vec<state::Chapter>, String> {
    let mut chapters = state.chapters.lock().unwrap();

    if let Some(chapter) = chapters.iter_mut().find(|c| c.id == id) {
        chapter.label = label;
    }

    let updated = chapters.clone();
    drop(chapters);

    state::save_chapters_file(&app, &updated)?;
    Ok(updated)
}

#[tauri::command]
fn delete_chapter(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<state::Chapter>, String> {
    let mut chapters = state.chapters.lock().unwrap();
    chapters.retain(|c| c.id != id);
    let updated = chapters.clone();
    drop(chapters);

    state::save_chapters_file(&app, &updated)?;
    Ok(updated)
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
fn apply_model(app: AppHandle, state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    let old_model = settings.model.clone();
    settings.model = model_id.clone();
    drop(settings);

    // Save the new model setting
    save_settings_file(&app, &state.settings.lock().unwrap())?;

    // If transcription is active, restart it with the new model
    if state.transcribe_active.load(Ordering::Relaxed) {
        stop_transcribe_monitor(&app, &state);
        let new_settings = state.settings.lock().unwrap().clone();
        if let Err(err) = start_transcribe_monitor(&app, &state, &new_settings) {
            // Restore old model if restart fails
            let mut settings = state.settings.lock().unwrap();
            settings.model = old_model;
            drop(settings);
            let _ = save_settings_file(&app, &state.settings.lock().unwrap());
            state.transcribe_active.store(false, Ordering::Relaxed);
            return Err(format!("Failed to apply model: {}", err));
        }
    }

    let _ = app.emit("model:changed", model_id);
    Ok(())
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
    let settings = state.settings.lock().unwrap();
    let hotkeys = vec![
        settings.hotkey_ptt.clone(),
        settings.hotkey_toggle.clone(),
        settings.transcribe_hotkey.clone(),
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
fn save_crash_recovery(content: String) -> Result<(), String> {
    use std::env;
    use std::fs;

    // Use %TEMP% or /tmp for crash recovery file
    let temp_dir = if cfg!(windows) {
        env::var("TEMP").unwrap_or_else(|_| "C:\\Users\\AppData\\Local\\Temp".to_string())
    } else {
        env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string())
    };

    let crash_recovery_file = PathBuf::from(&temp_dir).join("trispr_crash_recovery.json");

    // Write JSON content to crash recovery file
    fs::write(&crash_recovery_file, content)
        .map_err(|e| format!("Failed to save crash recovery: {}", e))?;

    Ok(())
}

#[tauri::command]
fn clear_crash_recovery() -> Result<(), String> {
    use std::env;
    use std::fs;

    // Use %TEMP% or /tmp for crash recovery file
    let temp_dir = if cfg!(windows) {
        env::var("TEMP").unwrap_or_else(|_| "C:\\Users\\AppData\\Local\\Temp".to_string())
    } else {
        env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string())
    };

    let crash_recovery_file = PathBuf::from(&temp_dir).join("trispr_crash_recovery.json");

    // Delete crash recovery file if it exists
    if crash_recovery_file.exists() {
        fs::remove_file(&crash_recovery_file)
            .map_err(|e| format!("Failed to clear crash recovery: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
fn encode_to_opus(
    input_path: String,
    output_path: String,
    bitrate_kbps: Option<u32>,
) -> Result<opus::OpusEncodeResult, String> {
    use std::path::Path;

    let input = Path::new(&input_path);
    let output = Path::new(&output_path);

    if let Some(bitrate) = bitrate_kbps {
        let mut config = opus::OpusEncoderConfig::default();
        config.bitrate_kbps = bitrate;
        opus::encode_wav_to_opus(input, output, &config)
    } else {
        opus::encode_wav_to_opus_default(input, output)
    }
}

#[tauri::command]
fn check_ffmpeg() -> Result<bool, String> {
    Ok(opus::check_ffmpeg_available())
}

#[tauri::command]
fn get_ffmpeg_version_info() -> Result<String, String> {
    opus::get_ffmpeg_version()
}

fn resolve_sidecar_dir(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("sidecar")
            .join("vibevoice-asr");
        if dev_path.exists() {
            return dev_path
                .canonicalize()
                .map_err(|e| format!("Failed to resolve dev sidecar path: {}", e));
        }
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|e| format!("Failed to get resource dir: {}", e))?;
    Ok(resource_dir.join("sidecar").join("vibevoice-asr"))
}

#[cfg(target_os = "windows")]
fn tail_output_lines(raw: &str, max_lines: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if max_lines == 0 {
        return String::new();
    }
    let lines: Vec<&str> = trimmed.lines().collect();
    if lines.len() <= max_lines {
        return trimmed.to_string();
    }
    format!("...\n{}", lines[lines.len() - max_lines..].join("\n"))
}

#[tauri::command]
fn start_sidecar(app: AppHandle) -> Result<(), String> {
    let sidecar_dir = resolve_sidecar_dir(&app)?;
    sidecar_process::start_sidecar(None, Some(sidecar_dir))
}

#[tauri::command]
fn install_vibevoice_dependencies(
    app: AppHandle,
    prefetch_model: Option<bool>,
) -> Result<serde_json::Value, String> {
    let sidecar_dir = resolve_sidecar_dir(&app)?;
    let setup_script = sidecar_dir.join("setup-vibevoice.ps1");
    if !setup_script.exists() {
        return Err(format!(
            "Voice Analysis setup script not found: {}",
            setup_script.display()
        ));
    }

    let use_prefetch = prefetch_model.unwrap_or(false);
    let mut cmd_args = vec![
        "-NoProfile".to_string(),
        "-ExecutionPolicy".to_string(),
        "Bypass".to_string(),
        "-File".to_string(),
        setup_script.display().to_string(),
    ];
    if use_prefetch {
        cmd_args.push("-PrefetchModel".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("powershell");
        cmd.args(&cmd_args)
            .current_dir(&sidecar_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run Voice Analysis setup: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout_tail = tail_output_lines(&stdout, 120);
        let stderr_tail = tail_output_lines(&stderr, 120);
        let setup_cmd = format!("powershell {}", cmd_args.join(" "));

        if !output.status.success() {
            let status = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated".to_string());
            let mut detail = String::new();
            if !stderr_tail.is_empty() {
                detail.push_str(&stderr_tail);
            } else if !stdout_tail.is_empty() {
                detail.push_str(&stdout_tail);
            }
            if detail.is_empty() {
                detail.push_str("No installer output captured.");
            }
            return Err(format!(
                "Voice Analysis setup failed (exit code {}).\n{}\n\nRun manually:\n{}",
                status, detail, setup_cmd
            ));
        }

        return Ok(serde_json::json!({
          "status": "success",
          "prefetch_model": use_prefetch,
          "setup_script": setup_script.to_string_lossy().to_string(),
          "stdout": stdout_tail,
          "stderr": stderr_tail
        }));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = use_prefetch;
        let _ = cmd_args;
        Err("Automatic Voice Analysis setup is currently supported on Windows only.".to_string())
    }
}

#[tauri::command]
fn stop_sidecar() -> Result<(), String> {
    sidecar_process::stop_sidecar()
}

#[tauri::command]
fn sidecar_health() -> Result<serde_json::Value, String> {
    let client = sidecar_process::get_sidecar_client()?;
    match client.health_check() {
        Ok(health) => serde_json::to_value(&health).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn sidecar_transcribe(
    audio_path: String,
    precision: Option<String>,
    language: Option<String>,
) -> Result<serde_json::Value, String> {
    let client = sidecar_process::get_sidecar_client()?;
    let path = std::path::Path::new(&audio_path);

    match client.transcribe(path, precision.as_deref(), language.as_deref()) {
        Ok(result) => serde_json::to_value(&result).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
fn parallel_transcribe(
    app: AppHandle,
    audio_path: String,
    precision: Option<String>,
    language: Option<String>,
) -> Result<serde_json::Value, String> {
    use std::thread;

    let audio_path_clone = audio_path.clone();
    let precision_clone = precision.clone();
    let language_clone = language.clone();

    // Run VibeVoice sidecar transcription in a separate thread
    let sidecar_handle = thread::spawn(move || -> Result<serde_json::Value, String> {
        let client = sidecar_process::get_sidecar_client()?;
        let path = std::path::Path::new(&audio_path_clone);
        match client.transcribe(path, precision_clone.as_deref(), language_clone.as_deref()) {
            Ok(result) => serde_json::to_value(&result).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        }
    });

    // Run Whisper transcription on main thread (reads audio from file)
    let whisper_result = {
        let path = std::path::Path::new(&audio_path);
        let samples = read_audio_file_as_i16(path)?;
        let state = app.state::<AppState>();
        let settings = state.settings.lock().unwrap().clone();
        match transcribe_audio(&app, &settings, &samples) {
            Ok((text, source)) => {
                serde_json::json!({ "text": text, "source": source })
            }
            Err(e) => serde_json::json!({ "error": e }),
        }
    };

    // Wait for sidecar result
    let sidecar_result = sidecar_handle
        .join()
        .map_err(|_| "Sidecar thread panicked".to_string())?;

    Ok(serde_json::json!({
      "whisper": whisper_result,
      "vibevoice": sidecar_result.unwrap_or_else(|e| serde_json::json!({ "error": e })),
    }))
}

/// Read an audio file (WAV/OPUS) as i16 samples at 16kHz
fn read_audio_file_as_i16(path: &std::path::Path) -> Result<Vec<i16>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let wav_path = match ext.as_str() {
        "wav" => path.to_path_buf(),
        "opus" => {
            // Decode OPUS to temporary WAV using FFmpeg
            let ffmpeg = crate::opus::find_ffmpeg()
                .map_err(|e| format!("FFmpeg required for OPUS decoding: {}", e))?;

            let temp_wav = std::env::temp_dir().join(format!(
                "opus_decode_{}.wav",
                std::process::id()
            ));

            std::process::Command::new(&ffmpeg)
                .args(&[
                    "-i",
                    &path.to_string_lossy().to_string(),
                    "-acodec",
                    "pcm_s16le",
                    "-ar",
                    "16000",
                    "-f",
                    "wav",
                    "-",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .and_then(|out| {
                    if !out.status.success() {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "FFmpeg decoding failed",
                        ));
                    }
                    std::fs::write(&temp_wav, out.stdout)?;
                    Ok(())
                })
                .map_err(|e| format!("Failed to decode OPUS with FFmpeg: {}", e))?;

            temp_wav
        }
        _ => {
            return Err(format!(
                "Unsupported audio format for parallel mode: .{}",
                ext
            ))
        }
    };

    // Read the WAV file
    let reader = hound::WavReader::open(&wav_path)
        .map_err(|e| format!("Failed to open WAV: {}", e))?;
    let spec = reader.spec();
    let samples: Vec<i16> = if spec.sample_format == hound::SampleFormat::Float {
        reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .map(|s| (s * i16::MAX as f32) as i16)
            .collect()
    } else {
        reader
            .into_samples::<i16>()
            .filter_map(|s| s.ok())
            .collect()
    };

    // Clean up temporary WAV if we created one
    if ext == "opus" {
        let _ = std::fs::remove_file(&wav_path);
    }

    Ok(samples)
}

#[tauri::command]
fn get_last_recording_path(
    source: String, // "mic" or "output"
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let path = if source == "output" || source == "system" {
        state.last_system_recording_path.lock().unwrap().clone()
    } else {
        state.last_mic_recording_path.lock().unwrap().clone()
    };
    Ok(path)
}

#[tauri::command]
fn get_recordings_directory(app: AppHandle) -> Result<String, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
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

fn parse_semver_like(version: &str) -> Option<(u64, u64, u64)> {
    let cleaned = version
        .trim()
        .trim_start_matches(|c: char| c == 'v' || c == 'V');
    let numeric_prefix: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    if numeric_prefix.is_empty() {
        return None;
    }

    let mut parts = numeric_prefix.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch))
}

fn is_newer_version(latest: &str, current: &str) -> bool {
    match (parse_semver_like(latest), parse_semver_like(current)) {
        (Some(latest_semver), Some(current_semver)) => latest_semver > current_semver,
        _ => latest.trim() != current.trim(),
    }
}

#[tauri::command]
fn check_vibevoice_updates() -> Result<serde_json::Value, String> {
    let response = ureq::get("https://api.github.com/repos/microsoft/VibeVoice/releases/latest")
        .set("User-Agent", "Trispr-Flow/0.6.0")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| format!("Failed to query VibeVoice releases: {}", e))?;

    let release: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    let latest_version = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let current_version =
        std::env::var("VIBEVOICE_CURRENT_VERSION").unwrap_or_else(|_| "1.0.0".to_string());

    let release_notes_full = release.get("body").and_then(|v| v.as_str()).unwrap_or("");
    let release_notes = release_notes_full
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .chars()
        .take(240)
        .collect::<String>();

    let download_url = release
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let update_available = if latest_version.is_empty() {
        false
    } else {
        is_newer_version(&latest_version, &current_version)
    };

    Ok(serde_json::json!({
      "update_available": update_available,
      "current_version": current_version,
      "latest_version": latest_version,
      "release_notes": release_notes,
      "download_url": download_url,
    }))
}

/// Sanitize session name for filename
fn sanitize_session_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>()
        .chars()
        .take(30) // Max 30 chars
        .collect()
}

/// Save audio samples as OPUS file for later analysis
/// This is used to enable VibeVoice-ASR speaker diarization on recorded audio
///
/// # Arguments
/// * `app` - App handle
/// * `samples` - Audio samples (i16, 16kHz)
/// * `source` - "mic", "output", or "mixed"
/// * `session_name` - Optional user-provided session name
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
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
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
        kitt_min_width: settings.overlay_kitt_min_width as f64,
        kitt_max_width: settings.overlay_kitt_max_width as f64,
        kitt_height: settings.overlay_kitt_height as f64,
    }
}

fn init_logging() {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("Trispr Flow starting up");
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

/// Snapshot of clipboard content before we overwrite it.
enum ClipboardSnapshot {
    Text(String),
    Image { width: usize, height: usize, bytes: Vec<u8> },
    Empty,
}

pub(crate) fn paste_text(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    // Save whatever is currently in the clipboard (text or image).
    let snapshot = if let Ok(t) = clipboard.get_text() {
        ClipboardSnapshot::Text(t)
    } else if let Ok(img) = clipboard.get_image() {
        ClipboardSnapshot::Image {
            width: img.width,
            height: img.height,
            bytes: img.bytes.into_owned(),
        }
    } else {
        ClipboardSnapshot::Empty
    };

    clipboard.set_text(text.to_string()).map_err(|e| e.to_string())?;

    send_paste_keystroke()?;

    // Restore the clipboard after the target app has had time to read it.
    //
    // We cannot restore immediately: send_paste_keystroke() only queues the
    // Ctrl+V event — the foreground app reads the clipboard asynchronously
    // when it processes WM_KEYDOWN. 300 ms is conservative enough for most
    // apps even under load; the ideal fix would be WaitForInputIdle() but
    // that requires windows-sys and adds complexity.
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(300));
        if let Ok(mut cb) = Clipboard::new() {
            match snapshot {
                ClipboardSnapshot::Text(t) => { let _ = cb.set_text(t); }
                ClipboardSnapshot::Image { width, height, bytes } => {
                    let _ = cb.set_image(ImageData {
                        width,
                        height,
                        bytes: std::borrow::Cow::Owned(bytes),
                    });
                }
                ClipboardSnapshot::Empty => {}
            }
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
    thread::spawn(move || {
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

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        // Restore window geometry on first show
        if !MAIN_WINDOW_RESTORED.swap(true, Ordering::AcqRel) {
            let settings = load_settings(app);

            if let (Some(x), Some(y), Some(w), Some(h)) = (
                settings.main_window_x,
                settings.main_window_y,
                settings.main_window_width,
                settings.main_window_height,
            ) {
                // Validate window state (reject minimized positions and invalid sizes)
                let state_valid = is_valid_window_state(x, y, w, h);

                // Validate monitor still exists
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
                    // Restore saved geometry
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    let _ = window.set_size(tauri::PhysicalSize::new(w, h));
                } else {
                    // Fallback: center on primary monitor
                    if let Ok(Some(primary)) = window.primary_monitor() {
                        let primary_size = primary.size();
                        let window_w = w.max(980);
                        let window_h = h.max(640);
                        let center_x = (primary_size.width as i32 - window_w as i32) / 2;
                        let center_y = (primary_size.height as i32 - window_h as i32) / 2;
                        let _ =
                            window.set_position(tauri::PhysicalPosition::new(center_x, center_y));
                        let _ = window.set_size(tauri::PhysicalSize::new(window_w, window_h));
                    }
                }
            }
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
    let mut settings = state.settings.lock().unwrap();
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
    thread::spawn(move || {
        let state = app.state::<AppState>();
        let new_enabled = {
            let mut settings = state.settings.lock().unwrap();
            settings.activation_words_enabled = !settings.activation_words_enabled;
            let enabled = settings.activation_words_enabled;
            let _ = save_settings_file(&app, &settings);
            enabled
        };

        let cue = if new_enabled { "start" } else { "stop" };
        let _ = app.emit("audio:cue", cue);
        let _ = app.emit("settings:updated", {
            let settings = state.settings.lock().unwrap().clone();
            settings
        });
        info!("Activation words toggled to: {}", new_enabled);
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
pub fn run() {
    init_logging();
    load_local_env();

    info!("Starting Trispr Flow application");
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_global_shortcut::Builder::new().build());
    with_dialog_plugin(builder)
        .setup(|app| {
            let settings = load_settings(app.handle());
            let history = load_history(app.handle());
            let history_transcribe = load_transcribe_history(app.handle());
            let chapters = state::load_chapters(app.handle());

            app.manage(AppState {
                settings: Mutex::new(settings.clone()),
                history: Mutex::new(history),
                history_transcribe: Mutex::new(history_transcribe),
                chapters: Mutex::new(chapters),
                recorder: Mutex::new(crate::audio::Recorder::new()),
                transcribe: Mutex::new(crate::transcription::TranscribeRecorder::new()),
                downloads: Mutex::new(HashSet::new()),
                transcribe_active: AtomicBool::new(false),
                last_mic_recording_path: Mutex::new(None),
                last_system_recording_path: Mutex::new(None),
            });

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

            if let Err(err) = register_hotkeys(app.handle(), &settings) {
                eprintln!("⚠ Failed to register hotkeys: {}", err);
            }

            if settings.mode == "vad" && settings.capture_enabled {
                // Delay VAD start by 2 seconds to allow models to load on first startup
                let app_handle = app.handle().clone();
                let settings_clone = settings.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let state = app_handle.state::<AppState>();
                    if let Err(err) =
                        crate::audio::start_vad_monitor(&app_handle, &state, &settings_clone)
                    {
                        eprintln!("⚠ Failed to start VAD monitor: {}", err);
                    }
                });
            }

            let overlay_app = app.handle().clone();
            app.listen("overlay:ready", move |_| {
                overlay::mark_overlay_ready();
                let settings = overlay_app
                    .state::<AppState>()
                    .settings
                    .lock()
                    .unwrap()
                    .clone();
                let _ = overlay::apply_overlay_settings(
                    &overlay_app,
                    &build_overlay_settings(&settings),
                );
            });
            if let Err(err) = overlay::create_overlay_window(&app.handle()) {
                eprintln!("⚠ Failed to create overlay window: {}", err);
            }
            let overlay_settings = build_overlay_settings(&settings);
            let _ = overlay::apply_overlay_settings(&app.handle(), &overlay_settings);
            let _ = overlay::update_overlay_state(&app.handle(), overlay::OverlayState::Idle);

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
                    use tauri::tray::{MouseButton, TrayIconEvent};
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
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
                        let state = app.state::<AppState>();
                        let mut current = state.settings.lock().unwrap().clone();
                        current.capture_enabled = !current.capture_enabled;
                        if let Err(err) = save_settings(app.clone(), state, current) {
                            emit_error(app, AppError::Storage(err), Some("Tray menu"));
                        }
                    }
                    "toggle-transcribe" => {
                        let state = app.state::<AppState>();
                        let mut current = state.settings.lock().unwrap().clone();
                        current.transcribe_enabled = !current.transcribe_enabled;
                        if let Err(err) = save_settings(app.clone(), state, current) {
                            emit_error(app, AppError::Storage(err), Some("Tray menu"));
                        }
                    }
                    "cancel-backlog-expand" => {
                        cancel_backlog_auto_expand(app);
                        let _ = cancel_backlog_item_event.set_enabled(false);
                        let _ = cancel_backlog_item_event.set_text("Cancel Auto-Expand");
                    }
                    "quit" => {
                        app.exit(0);
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

                if let (Some(x), Some(y), Some(w), Some(h)) = (
                    window_settings.main_window_x,
                    window_settings.main_window_y,
                    window_settings.main_window_width,
                    window_settings.main_window_height,
                ) {
                    // Validate window state (reject minimized positions and invalid sizes)
                    let state_valid = is_valid_window_state(x, y, w, h);

                    // Validate monitor still exists
                    let monitor_valid = window
                        .available_monitors()
                        .ok()
                        .map(|monitors| {
                            if let Some(monitor_name) = &window_settings.main_window_monitor {
                                monitors.iter().any(|m| {
                                    m.name().as_ref().map(|n| n.as_str())
                                        == Some(monitor_name.as_str())
                                })
                            } else {
                                true
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
                            let _ = window
                                .set_position(tauri::PhysicalPosition::new(center_x, center_y));
                            let _ = window.set_size(tauri::PhysicalSize::new(window_w, window_h));
                        }
                    }
                }

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
                        let _ = window.minimize();
                    }
                    _ => {
                        // "normal" — default behavior, window shows normally
                    }
                }
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
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            save_window_state,
            save_window_visibility_state,
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
            add_history_entry,
            add_transcribe_entry,
            get_chapters,
            add_chapter,
            update_chapter,
            delete_chapter,
            start_recording,
            stop_recording,
            toggle_transcribe,
            expand_transcribe_backlog,
            apply_model,
            validate_hotkey,
            test_hotkey,
            get_hotkey_conflicts,
            save_crash_recovery,
            clear_crash_recovery,
            encode_to_opus,
            check_ffmpeg,
            get_ffmpeg_version_info,
            start_sidecar,
            install_vibevoice_dependencies,
            stop_sidecar,
            sidecar_health,
            sidecar_transcribe,
            parallel_transcribe,
            get_last_recording_path,
            get_recordings_directory,
            open_recordings_directory,
            check_vibevoice_updates,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
