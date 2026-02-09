// Trispr Flow - core app runtime
#![allow(clippy::needless_return)]

mod audio;
mod constants;
mod errors;
mod hotkeys;
mod models;
mod overlay;
mod paths;
mod state;
mod postprocessing;
mod transcription;
mod util;

use arboard::Clipboard;
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
use tracing::{error, info};

use crate::audio::{list_audio_devices, list_output_devices, start_recording, stop_recording};
use crate::models::{
  check_model_available, clear_hidden_external_models, download_model, get_models_dir,
  hide_external_model, list_models, pick_model_dir, quantize_model, remove_model,
};
use crate::state::{
  load_history,
  load_settings,
  load_transcribe_history,
  push_history_entry_inner,
  push_transcribe_entry_inner,
  save_settings_file,
  sync_model_dir_env,
};
use crate::transcription::{
  expand_transcribe_backlog as expand_transcribe_backlog_inner,
  start_transcribe_monitor,
  stop_transcribe_monitor,
  toggle_transcribe_state,
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(BACKLOG_AUTOEXPAND_TIMEOUT_MS);
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
  let _ = manager.unregister_all();

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
        emit_error(
          app,
          AppError::Hotkey(format!(
            "Could not register PTT hotkey '{}': {}. Try a different key.",
            ptt, e
          )),
          Some("Hotkey Registration"),
        );
        Err(e.to_string())
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
        emit_error(
          app,
          AppError::Hotkey(format!(
            "Could not register Toggle hotkey '{}': {}. Try a different key.",
            toggle, e
          )),
          Some("Hotkey Registration"),
        );
        Err(e.to_string())
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
          emit_error(
            &app,
            AppError::AudioDevice(err),
            Some("System Audio"),
          );
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
        emit_error(
          app,
          AppError::Hotkey(format!(
            "Could not register Transcribe hotkey '{}': {}. Try a different key.",
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
      register_ptt()?;
      register_toggle()?;
    }
    "vad" => {}
    _ => {
      register_ptt()?;
      register_toggle()?;
    }
  }

  register_transcribe()?;

  // Register Toggle Activation Words hotkey
  let hotkey = settings.hotkey_toggle_activation_words.trim();
  if !hotkey.is_empty() {
    match manager.on_shortcut(hotkey, |app, _shortcut, event| {
      if event.state == ShortcutState::Pressed {
        toggle_activation_words_async(app.clone());
      }
    }) {
      Ok(_) => {},
      Err(e) => {
        error!("Failed to register Toggle Activation Words hotkey '{}': {}", hotkey, e);
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

  Ok(())
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
fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
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
    recorder
      .input_gain_db
      .store((settings.mic_input_gain_db * 1000.0) as i64, Ordering::Relaxed);
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
fn save_window_state(
    app: AppHandle,
    state: State<'_, AppState>,
    window_label: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let monitor_name = if let Some(window) = app.get_webview_window(&window_label) {
        window.current_monitor()
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
        "conversation" => {
            current.conv_window_x = Some(x);
            current.conv_window_y = Some(y);
            current.conv_window_width = Some(width);
            current.conv_window_height = Some(height);
            current.conv_window_monitor = monitor_name;
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
fn toggle_transcribe(app: AppHandle) -> Result<(), String> {
  toggle_transcribe_state(&app);
  Ok(())
}

#[tauri::command]
fn expand_transcribe_backlog(app: AppHandle) -> Result<transcription::TranscribeBacklogStatus, String> {
  expand_transcribe_backlog_inner(&app)
}

#[tauri::command]
fn open_conversation_window(app: AppHandle) -> Result<(), String> {
  if app.get_webview_window("conversation").is_some() {
    if let Some(window) = app.get_webview_window("conversation") {
      let _ = window.show();
      let _ = window.set_focus();
    }
    return Ok(());
  }

  let settings = load_settings(&app);

  let mut builder = tauri::WebviewWindowBuilder::new(
    &app,
    "conversation",
    tauri::WebviewUrl::App("index.html".into()),
  )
  .title("Trispr Flow · Conversation")
  .inner_size(860.0, 680.0)
  .min_inner_size(640.0, 420.0)
  .resizable(true)
  .decorations(true)
  .transparent(false)
  .visible(true)
  .always_on_top(settings.conv_window_always_on_top);

  // Restore geometry if available
  if let (Some(x), Some(y), Some(w), Some(h)) = (
    settings.conv_window_x,
    settings.conv_window_y,
    settings.conv_window_width,
    settings.conv_window_height,
  ) {
    builder = builder
      .position(x as f64, y as f64)
      .inner_size(w as f64, h as f64);
  }

  let window = builder.build().map_err(|e| e.to_string())?;

  // Validate monitor after creation
  let monitor_valid = window.available_monitors()
    .ok()
    .map(|monitors| {
      if let Some(monitor_name) = &settings.conv_window_monitor {
        monitors.iter().any(|m| {
          m.name().as_ref().map(|n| n.as_str()) == Some(monitor_name.as_str())
        })
      } else {
        true  // No specific monitor was saved, so any monitor is valid
      }
    })
    .unwrap_or(false);

  if !monitor_valid {
    // Fallback: center on primary monitor
    if let Ok(Some(primary)) = window.primary_monitor() {
      let primary_size = primary.size();
      let window_w = settings.conv_window_width.unwrap_or(860).max(640);
      let window_h = settings.conv_window_height.unwrap_or(680).max(420);
      let center_x = (primary_size.width as i32 - window_w as i32) / 2;
      let center_y = (primary_size.height as i32 - window_h as i32) / 2;
      let _ = window.set_position(tauri::PhysicalPosition::new(center_x, center_y));
    }
  }

  let _ = window.eval(
    "window.__TRISPR_VIEW__='conversation'; window.dispatchEvent(new CustomEvent('trispr:view', { detail: 'conversation' }));",
  );

  Ok(())
}

#[tauri::command]
fn set_conversation_window_always_on_top(app: AppHandle, state: State<'_, AppState>, always_on_top: bool) -> Result<(), String> {
  if let Some(window) = app.get_webview_window("conversation") {
    let _ = window.set_always_on_top(always_on_top).map_err(|e| e.to_string())?;
  }
  let mut settings = state.settings.lock().unwrap();
  settings.conv_window_always_on_top = always_on_top;
  drop(settings);
  save_settings_file(&app, &state.settings.lock().unwrap())?;
  Ok(())
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
  let grandparent = parent.as_ref().and_then(|p| p.parent().map(|gp| gp.to_path_buf()));
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

pub(crate) fn paste_text(text: &str) -> Result<(), String> {
  let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
  let previous = clipboard.get_text().ok();
  clipboard
    .set_text(text.to_string())
    .map_err(|e| e.to_string())?;

  send_paste_keystroke()?;

  if let Some(previous) = previous {
    thread::spawn(move || {
      thread::sleep(Duration::from_millis(150));
      if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text(previous);
      }
    });
  }

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

fn create_tray_pulse_icon(frame: usize, recording_active: bool, transcribe_active: bool) -> tauri::image::Image<'static> {
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
    draw_circle_rgba(&mut pixels, size, rec_center_x, rec_center_y, rec_radius + 0.45, [29, 166, 160, 72]);
  }
  if transcribe_active {
    draw_circle_rgba(&mut pixels, size, trans_center_x, trans_center_y, trans_radius + 0.45, [245, 179, 66, 72]);
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
  draw_circle_rgba(&mut pixels, size, rec_center_x, rec_center_y, rec_radius, rec_color);
  draw_circle_rgba(&mut pixels, size, trans_center_x, trans_center_y, trans_radius, trans_color);

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
        // Validate monitor still exists
        let monitor_valid = window.available_monitors()
          .ok()
          .map(|monitors| {
            if let Some(monitor_name) = &settings.main_window_monitor {
              monitors.iter().any(|m| {
                m.name().as_ref().map(|n| n.as_str()) == Some(monitor_name.as_str())
              })
            } else {
              true  // No specific monitor was saved, so any monitor is valid
            }
          })
          .unwrap_or(false);

        if monitor_valid {
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
            let _ = window.set_position(tauri::PhysicalPosition::new(center_x, center_y));
            let _ = window.set_size(tauri::PhysicalSize::new(window_w, window_h));
          }
        }
      }
    }

    let _ = window.show();
    let _ = window.set_skip_taskbar(false);
    let _ = window.set_focus();
  }
}

fn hide_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window("main") {
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  init_logging();
  load_local_env();

  info!("Starting Trispr Flow application");
  tauri::Builder::default()
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
    .setup(|app| {
      let settings = load_settings(app.handle());
      let history = load_history(app.handle());
      let history_transcribe = load_transcribe_history(app.handle());

      app.manage(AppState {
        settings: Mutex::new(settings.clone()),
        history: Mutex::new(history),
        history_transcribe: Mutex::new(history_transcribe),
        recorder: Mutex::new(crate::audio::Recorder::new()),
        transcribe: Mutex::new(crate::transcription::TranscribeRecorder::new()),
        downloads: Mutex::new(HashSet::new()),
        transcribe_active: AtomicBool::new(false),
      });

      let _ = app.emit("transcribe:state", "idle");

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
          if let Err(err) = crate::audio::start_vad_monitor(&app_handle, &state, &settings_clone) {
            eprintln!("⚠ Failed to start VAD monitor: {}", err);
          }
        });
      }

      let overlay_app = app.handle().clone();
      app.listen("overlay:ready", move |_| {
        overlay::mark_overlay_ready();
        let settings = overlay_app.state::<AppState>().settings.lock().unwrap().clone();
        let _ = overlay::apply_overlay_settings(&overlay_app, &build_overlay_settings(&settings));
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
            TrayIconEvent::Click { button: MouseButton::Left, .. } => {
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
        .on_menu_event(move |app, event| {
          match event.id.as_ref() {
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
              BACKLOG_PROMPT_CANCELLED.store(true, Ordering::Release);
              BACKLOG_PROMPT_ACTIVE.store(false, Ordering::Release);
              let _ = cancel_backlog_item_event.set_enabled(false);
              let _ = cancel_backlog_item_event.set_text("Cancel Auto-Expand");
            }
            "quit" => {
              app.exit(0);
            }
            _ => {}
          }
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
              &tauri::menu::MenuItem::with_id(app, "show", "Open Trispr Flow", true, None::<&str>)?,
              &tauri::menu::PredefinedMenuItem::separator(app)?,
              &mic_item,
              &transcribe_item,
              &tauri::menu::PredefinedMenuItem::separator(app)?,
              &cancel_backlog_item_menu,
              &tauri::menu::PredefinedMenuItem::separator(app)?,
              &tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
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
        schedule_backlog_auto_expand(backlog_prompt_handle.clone(), cancel_backlog_item_prompt.clone());
      });

      refresh_tray_icon(app.handle(), 0);
      start_tray_pulse_loop(app.handle().clone());

      // Restore main window geometry
      if let Some(window) = app.get_webview_window("main") {
        let window_settings = load_settings(app.handle());

        if let (Some(x), Some(y), Some(w), Some(h)) = (
          window_settings.main_window_x,
          window_settings.main_window_y,
          window_settings.main_window_width,
          window_settings.main_window_height,
        ) {
          // Validate monitor still exists
          let monitor_valid = window.available_monitors()
            .ok()
            .map(|monitors| {
              if let Some(monitor_name) = &window_settings.main_window_monitor {
                monitors.iter().any(|m| {
                  m.name().as_ref().map(|n| n.as_str()) == Some(monitor_name.as_str())
                })
              } else {
                true
              }
            })
            .unwrap_or(false);

          if monitor_valid {
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
      start_recording,
      stop_recording,
      toggle_transcribe,
      expand_transcribe_backlog,
      open_conversation_window,
      set_conversation_window_always_on_top,
      apply_model,
      validate_hotkey,
      test_hotkey,
      get_hotkey_conflicts,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
