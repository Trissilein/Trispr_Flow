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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{error, info};

use crate::audio::{list_audio_devices, list_output_devices, start_recording, stop_recording};
use crate::models::{download_model, get_models_dir, list_models, pick_model_dir, remove_model};
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
  start_transcribe_monitor,
  stop_transcribe_monitor,
  toggle_transcribe_state,
};

const TRAY_CLICK_DEBOUNCE_MS: u64 = 250;

static LAST_TRAY_CLICK_MS: AtomicU64 = AtomicU64::new(0);

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
fn open_conversation_window(app: AppHandle) -> Result<(), String> {
  if app.get_webview_window("conversation").is_some() {
    if let Some(window) = app.get_webview_window("conversation") {
      let _ = window.show();
      let _ = window.set_focus();
    }
    return Ok(());
  }

  let window = tauri::WebviewWindowBuilder::new(
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
  .build()
  .map_err(|e| e.to_string())?;

  let _ = window.eval(
    "window.__TRISPR_VIEW__='conversation'; window.dispatchEvent(new CustomEvent('trispr:view', { detail: 'conversation' }));",
  );

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

fn show_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window("main") {
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
      let mut settings = load_settings(app.handle());
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
        if let Err(err) = crate::audio::start_vad_monitor(app.handle(), &app.state::<AppState>(), &settings) {
          eprintln!("⚠ Failed to start VAD monitor: {}", err);
        }
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
      if let Some((pos_x, pos_y)) =
        overlay::resolve_overlay_position_for_settings(&app.handle(), &overlay_settings)
      {
        if overlay_settings.style == "kitt" {
          settings.overlay_kitt_pos_x = pos_x;
          settings.overlay_kitt_pos_y = pos_y;
        } else {
          settings.overlay_pos_x = pos_x;
          settings.overlay_pos_y = pos_y;
        }
        if let Ok(mut current) = app.state::<AppState>().settings.lock() {
          *current = settings.clone();
        }
        let _ = save_settings_file(app.handle(), &settings);
      }
      let _ = overlay::apply_overlay_settings(&app.handle(), &build_overlay_settings(&settings));
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

      let _tray_icon = tauri::tray::TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Trispr Flow")
        .on_tray_icon_event(|tray, event| {
          use tauri::tray::{MouseButton, TrayIconEvent};
          match event {
            TrayIconEvent::Click { button: MouseButton::Left, .. } => {
              if should_handle_tray_click() {
                toggle_main_window(tray.app_handle());
              }
            }
            _ => {}
          }
        })
        .on_menu_event(|app, event| {
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
              &tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
            ],
          )?
        })
        .show_menu_on_left_click(false)
        .build(app);

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
      list_audio_devices,
      list_output_devices,
      list_models,
      download_model,
      remove_model,
      pick_model_dir,
      get_models_dir,
      get_history,
      get_transcribe_history,
      add_history_entry,
      add_transcribe_entry,
      start_recording,
      stop_recording,
      toggle_transcribe,
      open_conversation_window,
      validate_hotkey,
      test_hotkey,
      get_hotkey_conflicts,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
