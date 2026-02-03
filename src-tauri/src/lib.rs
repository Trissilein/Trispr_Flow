// Trispr Flow - core app runtime
#![allow(clippy::needless_return)]

mod errors;
mod hotkeys;
mod overlay;

use arboard::Clipboard;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use enigo::{Enigo, Key, KeyboardControllable};
use errors::{AppError, ErrorEvent};
use overlay::{OverlayState, update_overlay_state};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const MIN_AUDIO_MS: u64 = 250;
const VAD_THRESHOLD_DEFAULT: f32 = 0.015;
const VAD_SILENCE_MS_DEFAULT: u64 = 700;
const VAD_MIN_VOICE_MS: u64 = 250;
const DEFAULT_MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

struct ModelSpec {
  id: &'static str,
  label: &'static str,
  file_name: &'static str,
  size_mb: u32,
}

const MODEL_SPECS: &[ModelSpec] = &[
  ModelSpec {
    id: "whisper-large-v3",
    label: "Whisper large-v3",
    file_name: "ggml-large-v3.bin",
    size_mb: 2900,
  },
  ModelSpec {
    id: "whisper-large-v3-turbo",
    label: "Whisper large-v3-turbo",
    file_name: "ggml-large-v3-turbo.bin",
    size_mb: 1500,
  },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
  mode: String,
  hotkey_ptt: String,
  hotkey_toggle: String,
  input_device: String,
  language_mode: String,
  model: String,
  cloud_fallback: bool,
  audio_cues: bool,
  audio_cues_volume: f32,
  vad_threshold: f32,
  vad_silence_ms: u64,
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      mode: "ptt".to_string(),
      hotkey_ptt: "CommandOrControl+Shift+Space".to_string(),
      hotkey_toggle: "CommandOrControl+Shift+M".to_string(),
      input_device: "default".to_string(),
      language_mode: "auto".to_string(),
      model: "whisper-large-v3".to_string(),
      cloud_fallback: false,
      audio_cues: true,
      audio_cues_volume: 0.3,
      vad_threshold: VAD_THRESHOLD_DEFAULT,
      vad_silence_ms: VAD_SILENCE_MS_DEFAULT,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
  id: String,
  text: String,
  timestamp_ms: u64,
  source: String,
}

#[derive(Debug, Clone, Serialize)]
struct AudioDevice {
  id: String,
  label: String,
}

#[derive(Debug, Clone, Serialize)]
struct ModelInfo {
  id: String,
  label: String,
  file_name: String,
  size_mb: u32,
  installed: bool,
  downloading: bool,
  path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
  id: String,
  downloaded: u64,
  total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadComplete {
  id: String,
  path: String,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadError {
  id: String,
  error: String,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionResult {
  text: String,
  source: String,
}

#[derive(Default)]
struct CaptureBuffer {
  samples: Vec<i16>,
  resample_pos: f64,
}

impl CaptureBuffer {
  fn reset(&mut self) {
    self.samples.clear();
    self.resample_pos = 0.0;
  }

  fn drain(&mut self) -> Vec<i16> {
    let mut out = Vec::new();
    std::mem::swap(&mut out, &mut self.samples);
    self.resample_pos = 0.0;
    out
  }

  fn push_samples(&mut self, input: &[f32], in_rate: u32) {
    if input.is_empty() {
      return;
    }

    if in_rate == TARGET_SAMPLE_RATE {
      for &sample in input {
        self.samples.push(float_to_i16(sample));
      }
      return;
    }

    let ratio = in_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let mut pos = self.resample_pos;

    while pos + 1.0 < input.len() as f64 {
      let idx = pos.floor() as usize;
      let frac = pos - idx as f64;
      let a = input[idx] as f64;
      let b = input[idx + 1] as f64;
      let sample = (a * (1.0 - frac) + b * frac) as f32;
      self.samples.push(float_to_i16(sample));
      pos += ratio;
    }

    self.resample_pos = pos - input.len() as f64;
  }
}

struct Recorder {
  buffer: Arc<Mutex<CaptureBuffer>>,
  active: bool,
  transcribing: bool,
  stop_tx: Option<std::sync::mpsc::Sender<()>>,
  join_handle: Option<thread::JoinHandle<()>>,
  vad_tx: Option<std::sync::mpsc::Sender<VadEvent>>,
  vad_runtime: Option<Arc<VadRuntime>>,
}

impl Recorder {
  fn new() -> Self {
    Self {
      buffer: Arc::new(Mutex::new(CaptureBuffer::default())),
      active: false,
      transcribing: false,
      stop_tx: None,
      join_handle: None,
      vad_tx: None,
      vad_runtime: None,
    }
  }
}

struct AppState {
  settings: Mutex<Settings>,
  history: Mutex<Vec<HistoryEntry>>,
  recorder: Mutex<Recorder>,
  downloads: Mutex<HashSet<String>>,
}

fn resolve_config_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = app
    .path()
    .app_config_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

fn resolve_data_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = app
    .path()
    .app_data_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

fn load_settings(app: &AppHandle) -> Settings {
  let path = resolve_config_path(app, "settings.json");
  match fs::read_to_string(path) {
    Ok(raw) => {
      let mut settings: Settings = serde_json::from_str(&raw).unwrap_or_default();
      if settings.mode != "ptt" && settings.mode != "vad" {
        settings.mode = "ptt".to_string();
      }
      if !(0.0..=1.0).contains(&settings.vad_threshold) {
        settings.vad_threshold = VAD_THRESHOLD_DEFAULT;
      }
      if settings.vad_silence_ms < 100 {
        settings.vad_silence_ms = VAD_SILENCE_MS_DEFAULT;
      }
      settings
    }
    Err(_) => Settings::default(),
  }
}

fn save_settings_file(app: &AppHandle, settings: &Settings) -> Result<(), String> {
  let path = resolve_config_path(app, "settings.json");
  let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

fn load_history(app: &AppHandle) -> Vec<HistoryEntry> {
  let path = resolve_data_path(app, "history.json");
  match fs::read_to_string(path) {
    Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
    Err(_) => Vec::new(),
  }
}

fn save_history_file(app: &AppHandle, history: &[HistoryEntry]) -> Result<(), String> {
  let path = resolve_data_path(app, "history.json");
  let raw = serde_json::to_string_pretty(history).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

fn model_spec(model_id: &str) -> Option<&'static ModelSpec> {
  MODEL_SPECS.iter().find(|spec| spec.id == model_id)
}

fn resolve_models_dir(app: &AppHandle) -> PathBuf {
  let base = app
    .path()
    .app_data_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let dir = base.join("models");
  let _ = fs::create_dir_all(&dir);
  dir
}

fn model_candidates(spec: &ModelSpec) -> Vec<String> {
  let mut candidates = vec![spec.file_name.to_string()];
  if let Some(stripped) = spec.file_name.strip_suffix(".bin") {
    candidates.push(format!("{}-q5_0.bin", stripped));
  }
  candidates
}

fn find_model_in_dir(dir: &PathBuf, spec: &ModelSpec) -> Option<PathBuf> {
  for file in model_candidates(spec) {
    let candidate = dir.join(&file);
    if candidate.exists() {
      return Some(candidate);
    }
  }
  None
}

fn resolve_model_path(app: &AppHandle, model_id: &str) -> Option<PathBuf> {
  info!("Resolving model path for: {}", model_id);

  if let Ok(path) = std::env::var("TRISPR_WHISPER_MODEL") {
    let candidate = PathBuf::from(&path);
    info!("Checking TRISPR_WHISPER_MODEL env var: {}", path);
    if candidate.exists() {
      info!("Found model at: {}", candidate.display());
      return Some(candidate);
    }
  }

  let spec = model_spec(model_id)?;
  info!("Looking for model file: {}", spec.file_name);

  if let Ok(dir) = std::env::var("TRISPR_WHISPER_MODEL_DIR") {
    let dir = PathBuf::from(&dir);
    info!("Checking TRISPR_WHISPER_MODEL_DIR: {}", dir.display());
    if let Some(path) = find_model_in_dir(&dir, spec) {
      info!("Found model at: {}", path.display());
      return Some(path);
    }
  }

  let models_dir = resolve_models_dir(app);
  info!("Checking app data dir: {}", models_dir.display());
  if let Some(path) = find_model_in_dir(&models_dir, spec) {
    info!("Found model at: {}", path.display());
    return Some(path);
  }

  // Search relative to executable location (works for built .exe)
  if let Ok(exe_path) = std::env::current_exe() {
    if let Some(exe_dir) = exe_path.parent() {
      info!("Executable directory: {}", exe_dir.display());
      let exe_search_dirs = [
        exe_dir.join("models"),              // models/ next to .exe
        exe_dir.join("../models"),           // models/ one level up
        exe_dir.join("../whisper.cpp/models"), // whisper.cpp/models one level up
        exe_dir.join("../../whisper.cpp/models"), // whisper.cpp/models two levels up
      ];
      for dir in &exe_search_dirs {
        info!("Checking: {}", dir.display());
        if let Some(path) = find_model_in_dir(dir, spec) {
          info!("Found model at: {}", path.display());
          return Some(path);
        }
      }
    }
  }

  // Search relative to current working directory (works for dev)
  if let Ok(cwd) = std::env::current_dir() {
    info!("Current working directory: {}", cwd.display());
    let search_dirs = [
      cwd.join("models"),
      cwd.join("../whisper.cpp/models"),
      cwd.join("../../whisper.cpp/models"),
    ];
    for dir in &search_dirs {
      info!("Checking: {}", dir.display());
      if let Some(path) = find_model_in_dir(dir, spec) {
        info!("Found model at: {}", path.display());
        return Some(path);
      }
    }
  }

  warn!("Model not found for: {}", model_id);
  None
}

fn float_to_i16(sample: f32) -> i16 {
  let clamped = sample.clamp(-1.0, 1.0);
  (clamped * i16::MAX as f32) as i16
}

struct OverlayLevelEmitter {
  app: AppHandle,
  start: Instant,
  last_emit_ms: AtomicU64,
}

impl OverlayLevelEmitter {
  fn new(app: AppHandle) -> Self {
    Self {
      app,
      start: Instant::now(),
      last_emit_ms: AtomicU64::new(0),
    }
  }

  fn emit_level(&self, level: f32) {
    let now_ms = self.start.elapsed().as_millis() as u64;
    let last = self.last_emit_ms.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < 50 {
      return;
    }
    self.last_emit_ms.store(now_ms, Ordering::Relaxed);
    let level = level.clamp(0.0, 1.0);
    let _ = self.app.emit("overlay:level", level);
    let _ = self.app.emit("audio:level", level);
  }
}

#[derive(Debug)]
struct VadRuntime {
  recording: std::sync::atomic::AtomicBool,
  pending_flush: std::sync::atomic::AtomicBool,
  processing: std::sync::atomic::AtomicBool,
  last_voice_ms: AtomicU64,
  start_ms: AtomicU64,
  audio_cues: bool,
  threshold_scaled: AtomicU64,
  silence_ms: AtomicU64,
}

impl VadRuntime {
  fn new(audio_cues: bool, threshold: f32, silence_ms: u64) -> Self {
    let scaled = (threshold.clamp(0.001, 0.5) * 1_000_000.0) as u64;
    Self {
      recording: std::sync::atomic::AtomicBool::new(false),
      pending_flush: std::sync::atomic::AtomicBool::new(false),
      processing: std::sync::atomic::AtomicBool::new(false),
      last_voice_ms: AtomicU64::new(0),
      start_ms: AtomicU64::new(0),
      audio_cues,
      threshold_scaled: AtomicU64::new(scaled),
      silence_ms: AtomicU64::new(silence_ms.max(100)),
    }
  }

  fn threshold(&self) -> f32 {
    self.threshold_scaled.load(Ordering::Relaxed) as f32 / 1_000_000.0
  }

  fn update_threshold(&self, threshold: f32) {
    let scaled = (threshold.clamp(0.001, 0.5) * 1_000_000.0) as u64;
    self.threshold_scaled.store(scaled, Ordering::Relaxed);
  }

  fn silence_ms(&self) -> u64 {
    self.silence_ms.load(Ordering::Relaxed)
  }

  fn update_silence_ms(&self, silence_ms: u64) {
    self.silence_ms.store(silence_ms.max(100), Ordering::Relaxed);
  }
}

#[derive(Debug, Clone)]
enum VadEvent {
  Finalize(Vec<i16>),
}

#[derive(Clone)]
struct VadHandle {
  runtime: Arc<VadRuntime>,
  tx: std::sync::mpsc::Sender<VadEvent>,
  app: AppHandle,
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
        let _ = handle_ptt_press(&app);
      } else {
        info!("PTT hotkey released");
        handle_ptt_release_async(app);
      }
    }) {
      Ok(_) => {
        info!("PTT hotkey registered successfully");
        Ok(())
      }
      Err(e) => {
        error!("Failed to register PTT hotkey '{}': {}", ptt, e);
        emit_error(app, AppError::Hotkey(format!("Could not register PTT hotkey '{}': {}. Try a different key.", ptt, e)), Some("Hotkey Registration"));
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
        handle_toggle_async(app);
      }
    }) {
      Ok(_) => {
        info!("Toggle hotkey registered successfully");
        Ok(())
      }
      Err(e) => {
        error!("Failed to register Toggle hotkey '{}': {}", toggle, e);
        emit_error(app, AppError::Hotkey(format!("Could not register Toggle hotkey '{}': {}. Try a different key.", toggle, e)), Some("Hotkey Registration"));
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

  Ok(())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
  state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
  let prev_mode = { state.settings.lock().unwrap().mode.clone() };
  {
    let mut current = state.settings.lock().unwrap();
    *current = settings.clone();
  }
  save_settings_file(&app, &settings)?;
  register_hotkeys(&app, &settings)?;

  if prev_mode != settings.mode {
    if prev_mode == "vad" {
      stop_vad_monitor(&app, &state);
    }
    if settings.mode == "vad" {
      let _ = start_vad_monitor(&app, &state, &settings);
    }
  }
  if settings.mode == "vad" {
    if let Ok(recorder) = state.recorder.lock() {
      if let Some(runtime) = recorder.vad_runtime.as_ref() {
        runtime.update_threshold(settings.vad_threshold);
        runtime.update_silence_ms(settings.vad_silence_ms);
      }
    }
  }

  // Update overlay state based on new mode
  let recorder = state.recorder.lock().unwrap();
  if !recorder.active {
    let _ = overlay::update_overlay_state(&app, overlay::OverlayState::Idle);
  }
  drop(recorder);

  let _ = app.emit("settings-changed", settings.clone());
  let _ = app.emit("menu:update-cloud", settings.cloud_fallback.to_string());
  Ok(())
}

#[tauri::command]
fn list_audio_devices() -> Vec<AudioDevice> {
  let mut devices = vec![AudioDevice {
    id: "default".to_string(),
    label: "Default (System)".to_string(),
  }];

  let host = cpal::default_host();
  if let Ok(inputs) = host.input_devices() {
    for (index, device) in inputs.enumerate() {
      let name = device
        .name()
        .unwrap_or_else(|_| format!("Input {}", index + 1));
      let id = format!("input-{}-{}", index, name);
      devices.push(AudioDevice { id, label: name });
    }
  }

  devices
}

#[tauri::command]
fn list_models(app: AppHandle, state: State<'_, AppState>) -> Vec<ModelInfo> {
  let downloads = state.downloads.lock().unwrap();
  MODEL_SPECS
    .iter()
    .map(|spec| {
      let path = resolve_model_path(&app, spec.id);
      ModelInfo {
        id: spec.id.to_string(),
        label: spec.label.to_string(),
        file_name: spec.file_name.to_string(),
        size_mb: spec.size_mb,
        installed: path.is_some(),
        downloading: downloads.contains(spec.id),
        path: path.map(|p| p.to_string_lossy().to_string()),
      }
    })
    .collect()
}

#[tauri::command]
fn download_model(app: AppHandle, state: State<'_, AppState>, model_id: String) -> Result<(), String> {
  let spec = model_spec(&model_id).ok_or_else(|| "Unknown model".to_string())?;
  {
    let mut downloads = state.downloads.lock().unwrap();
    if downloads.contains(spec.id) {
      return Err("Download already in progress".to_string());
    }
    downloads.insert(spec.id.to_string());
  }

  let app_handle = app.clone();
  thread::spawn(move || {
    let result = download_model_file(&app_handle, &model_id);
    match result {
      Ok(path) => {
        let _ = app_handle.emit(
          "model:download-complete",
          DownloadComplete {
            id: model_id.clone(),
            path: path.to_string_lossy().to_string(),
          },
        );
      }
      Err(error) => {
        let _ = app_handle.emit(
          "model:download-error",
          DownloadError {
            id: model_id.clone(),
            error,
          },
        );
      }
    }

    let state = app_handle.state::<AppState>();
    let mut downloads = state.downloads.lock().unwrap();
    downloads.remove(&model_id);
  });

  Ok(())
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
  state.history.lock().unwrap().clone()
}

#[tauri::command]
fn add_history_entry(
  app: AppHandle,
  state: State<'_, AppState>,
  text: String,
  source: Option<String>,
) -> Result<Vec<HistoryEntry>, String> {
  let source = source.unwrap_or_else(|| "local".to_string());
  push_history_entry(&app, &state, text, source)
}

#[tauri::command]
fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
  let settings = state.settings.lock().unwrap().clone();
  start_recording_with_settings(&app, &state, &settings)
}

#[tauri::command]
fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
  stop_recording_async(app, &state);
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
  ];
  hotkeys::detect_conflicts(hotkeys)
}

fn resolve_input_device(device_id: &str) -> Option<cpal::Device> {
  let host = cpal::default_host();
  if device_id == "default" {
    return host.default_input_device();
  }

  if let Ok(inputs) = host.input_devices() {
    for (index, device) in inputs.enumerate() {
      let name = device
        .name()
        .unwrap_or_else(|_| format!("Input {}", index + 1));
      let id = format!("input-{}-{}", index, name);
      if id == device_id {
        return Some(device);
      }
    }
  }

  host.default_input_device()
}

fn push_mono_samples(
  buffer: &Arc<Mutex<CaptureBuffer>>,
  mono: &[f32],
  sample_rate: u32,
) {
  if let Ok(mut guard) = buffer.lock() {
    guard.push_samples(mono, sample_rate);
  }
}

fn handle_vad_audio(
  vad_handle: &VadHandle,
  buffer: &Arc<Mutex<CaptureBuffer>>,
  mono: Vec<f32>,
  level: f32,
  sample_rate: u32,
) {
  let runtime = &vad_handle.runtime;
  let now = now_ms();
  let threshold = runtime.threshold();
  if level >= threshold {
    runtime.last_voice_ms.store(now, Ordering::Relaxed);
    if !runtime.recording.swap(true, Ordering::Relaxed) {
      runtime.start_ms.store(now, Ordering::Relaxed);
      runtime.pending_flush.store(false, Ordering::Relaxed);
      if let Ok(mut buf) = buffer.lock() {
        buf.reset();
      }
      let _ = vad_handle.app.emit("capture:state", "recording");
      let _ = update_overlay_state(&vad_handle.app, OverlayState::Recording);
      if runtime.audio_cues {
        let _ = vad_handle.app.emit("audio:cue", "start");
      }
    }
  }

  if runtime.recording.load(Ordering::Relaxed) {
    push_mono_samples(buffer, &mono, sample_rate);

    let last = runtime.last_voice_ms.load(Ordering::Relaxed);
    let start = runtime.start_ms.load(Ordering::Relaxed);
    let silence_ms = runtime.silence_ms();
    if now.saturating_sub(last) > silence_ms && now.saturating_sub(start) > VAD_MIN_VOICE_MS {
      if !runtime.pending_flush.swap(true, Ordering::Relaxed) {
        runtime.recording.store(false, Ordering::Relaxed);
        let samples = {
          let mut buf = buffer.lock().unwrap();
          buf.drain()
        };
        let _ = vad_handle.tx.send(VadEvent::Finalize(samples));
      }
    }
  }
}

fn build_input_stream_f32(
  device: &cpal::Device,
  config: &StreamConfig,
  buffer: Arc<Mutex<CaptureBuffer>>,
  overlay: Option<Arc<OverlayLevelEmitter>>,
  vad: Option<VadHandle>,
) -> Result<cpal::Stream, String> {
  let channels = config.channels as usize;
  let sample_rate = config.sample_rate.0;
  let err_fn = |err| eprintln!("audio stream error: {}", err);

  device
    .build_input_stream(
      config,
      move |data: &[f32], _| {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        let mut sum_abs = 0.0f32;
        for frame in data.chunks(channels.max(1)) {
          let mut sum = 0.0f32;
          for &sample in frame {
            sum += sample;
          }
          let sample = sum / channels.max(1) as f32;
          mono.push(sample);
          sum_abs += sample.abs();
        }
        let level = if mono.is_empty() {
          0.0
        } else {
          (sum_abs / mono.len() as f32).min(1.0)
        };
        if let Some(emitter) = overlay.as_ref() {
          emitter.emit_level(level);
        }
        if let Some(vad_handle) = vad.as_ref() {
          handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
        } else {
          push_mono_samples(&buffer, &mono, sample_rate);
        }
      },
      err_fn,
      None,
    )
    .map_err(|e| e.to_string())
}

fn build_input_stream_i16(
  device: &cpal::Device,
  config: &StreamConfig,
  buffer: Arc<Mutex<CaptureBuffer>>,
  overlay: Option<Arc<OverlayLevelEmitter>>,
  vad: Option<VadHandle>,
) -> Result<cpal::Stream, String> {
  let channels = config.channels as usize;
  let sample_rate = config.sample_rate.0;
  let err_fn = |err| eprintln!("audio stream error: {}", err);

  device
    .build_input_stream(
      config,
      move |data: &[i16], _| {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        let mut sum_abs = 0.0f32;
        for frame in data.chunks(channels.max(1)) {
          let mut sum = 0.0f32;
          for &sample in frame {
            sum += sample as f32 / i16::MAX as f32;
          }
          let sample = sum / channels.max(1) as f32;
          mono.push(sample);
          sum_abs += sample.abs();
        }
        let level = if mono.is_empty() {
          0.0
        } else {
          (sum_abs / mono.len() as f32).min(1.0)
        };
        if let Some(emitter) = overlay.as_ref() {
          emitter.emit_level(level);
        }
        if let Some(vad_handle) = vad.as_ref() {
          handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
        } else {
          push_mono_samples(&buffer, &mono, sample_rate);
        }
      },
      err_fn,
      None,
    )
    .map_err(|e| e.to_string())
}

fn build_input_stream_u16(
  device: &cpal::Device,
  config: &StreamConfig,
  buffer: Arc<Mutex<CaptureBuffer>>,
  overlay: Option<Arc<OverlayLevelEmitter>>,
  vad: Option<VadHandle>,
) -> Result<cpal::Stream, String> {
  let channels = config.channels as usize;
  let sample_rate = config.sample_rate.0;
  let err_fn = |err| eprintln!("audio stream error: {}", err);

  device
    .build_input_stream(
      config,
      move |data: &[u16], _| {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        let mut sum_abs = 0.0f32;
        for frame in data.chunks(channels.max(1)) {
          let mut sum = 0.0f32;
          for &sample in frame {
            let centered = sample as f32 - 32768.0;
            sum += centered / 32768.0;
          }
          let sample = sum / channels.max(1) as f32;
          mono.push(sample);
          sum_abs += sample.abs();
        }
        let level = if mono.is_empty() {
          0.0
        } else {
          (sum_abs / mono.len() as f32).min(1.0)
        };
        if let Some(emitter) = overlay.as_ref() {
          emitter.emit_level(level);
        }
        if let Some(vad_handle) = vad.as_ref() {
          handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
        } else {
          push_mono_samples(&buffer, &mono, sample_rate);
        }
      },
      err_fn,
      None,
    )
    .map_err(|e| e.to_string())
}

fn start_recording_with_settings(
  app: &AppHandle,
  state: &State<'_, AppState>,
  settings: &Settings,
) -> Result<(), String> {
  info!("start_recording_with_settings called");
  let mut recorder = state.recorder.lock().unwrap();
  if recorder.active || recorder.transcribing {
    info!("Recording already active or transcribing, skipping");
    return Ok(());
  }

  if let Ok(mut buf) = recorder.buffer.lock() {
    buf.reset();
  }

  let buffer = recorder.buffer.clone();
  let overlay_emitter = Arc::new(OverlayLevelEmitter::new(app.clone()));
  let device_id = settings.input_device.clone();
  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

  let join_handle = thread::spawn(move || {
    let result = (|| -> Result<(), String> {
      let device = resolve_input_device(&device_id)
        .ok_or_else(|| "No input device available".to_string())?;
      let config = device
        .default_input_config()
        .map_err(|e| e.to_string())?;
      let stream_config: StreamConfig = config.clone().into();

      let overlay = Some(overlay_emitter);
      let vad = None;
      let stream = match config.sample_format() {
        SampleFormat::F32 => build_input_stream_f32(&device, &stream_config, buffer, overlay.clone(), vad.clone())?,
        SampleFormat::I16 => build_input_stream_i16(&device, &stream_config, buffer, overlay.clone(), vad.clone())?,
        SampleFormat::U16 => build_input_stream_u16(&device, &stream_config, buffer, overlay.clone(), vad.clone())?,
        _ => return Err("Unsupported sample format".to_string()),
      };

      stream.play().map_err(|e| e.to_string())?;
      let _ = ready_tx.send(Ok(()));

      let _ = stop_rx.recv();
      drop(stream);
      Ok(())
    })();

    if let Err(err) = result {
      let _ = ready_tx.send(Err(err));
    }
  });

  let start_result = match ready_rx.recv_timeout(Duration::from_secs(3)) {
    Ok(Ok(())) => Ok(()),
    Ok(Err(err)) => Err(err),
    Err(_) => Err("Failed to start audio stream".to_string()),
  };

  if let Err(err) = start_result {
    error!("Failed to start recording: {}", err);
    let _ = stop_tx.send(());
    let _ = join_handle.join();
    return Err(err);
  }

  recorder.stop_tx = Some(stop_tx);
  recorder.join_handle = Some(join_handle);
  recorder.active = true;

  info!("Recording started successfully, updating overlay");
  let _ = app.emit("capture:state", "recording");
  let _ = update_overlay_state(app, OverlayState::Recording);

  // Emit audio cue if enabled
  if settings.audio_cues {
    let _ = app.emit("audio:cue", "start");
  }

  Ok(())
}

fn start_vad_monitor(
  app: &AppHandle,
  state: &State<'_, AppState>,
  settings: &Settings,
) -> Result<(), String> {
  info!("start_vad_monitor called");
  let mut recorder = state.recorder.lock().unwrap();
  if recorder.active || recorder.transcribing {
    info!("VAD already active or transcribing, skipping");
    return Ok(());
  }

  if let Ok(mut buf) = recorder.buffer.lock() {
    buf.reset();
  }

  let buffer = recorder.buffer.clone();
  let overlay_emitter = Arc::new(OverlayLevelEmitter::new(app.clone()));
  let device_id = settings.input_device.clone();
  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
  let (vad_tx, vad_rx) = std::sync::mpsc::channel::<VadEvent>();

  let vad_runtime = Arc::new(VadRuntime::new(
    settings.audio_cues,
    settings.vad_threshold,
    settings.vad_silence_ms,
  ));
  let vad_handle = VadHandle {
    runtime: vad_runtime.clone(),
    tx: vad_tx.clone(),
    app: app.clone(),
  };

  let app_handle = app.clone();
  let settings_clone = settings.clone();
  let buffer_clone = buffer.clone();
  let vad_runtime_clone = vad_runtime.clone();
  thread::spawn(move || {
    for event in vad_rx {
      match event {
        VadEvent::Finalize(samples) => {
          process_vad_segment(app_handle.clone(), settings_clone.clone(), samples, vad_runtime_clone.clone());
        }
      }
    }
  });

  let join_handle = thread::spawn(move || {
    let result = (|| -> Result<(), String> {
      let device = resolve_input_device(&device_id)
        .ok_or_else(|| "No input device available".to_string())?;
      let config = device
        .default_input_config()
        .map_err(|e| e.to_string())?;
      let stream_config: StreamConfig = config.clone().into();

      let overlay = Some(overlay_emitter);
      let vad = Some(vad_handle);
      let stream = match config.sample_format() {
        SampleFormat::F32 => build_input_stream_f32(&device, &stream_config, buffer, overlay.clone(), vad.clone())?,
        SampleFormat::I16 => build_input_stream_i16(&device, &stream_config, buffer, overlay.clone(), vad.clone())?,
        SampleFormat::U16 => build_input_stream_u16(&device, &stream_config, buffer, overlay.clone(), vad.clone())?,
        _ => return Err("Unsupported sample format".to_string()),
      };

      stream.play().map_err(|e| e.to_string())?;
      let _ = ready_tx.send(Ok(()));

      let _ = stop_rx.recv();
      drop(stream);
      Ok(())
    })();

    if let Err(err) = result {
      let _ = ready_tx.send(Err(err));
    }
  });

  let start_result = match ready_rx.recv_timeout(Duration::from_secs(3)) {
    Ok(Ok(())) => Ok(()),
    Ok(Err(err)) => Err(err),
    Err(_) => Err("Failed to start audio stream".to_string()),
  };

  if let Err(err) = start_result {
    error!("Failed to start VAD monitor: {}", err);
    let _ = stop_tx.send(());
    let _ = join_handle.join();
    return Err(err);
  }

  recorder.stop_tx = Some(stop_tx);
  recorder.join_handle = Some(join_handle);
  recorder.active = true;
  recorder.vad_tx = Some(vad_tx);
  recorder.vad_runtime = Some(vad_runtime);

  let _ = app.emit("capture:state", "idle");
  let _ = update_overlay_state(app, OverlayState::Idle);
  Ok(())
}

fn stop_vad_monitor(app: &AppHandle, state: &State<'_, AppState>) {
  let (stop_tx, join_handle, vad_tx, vad_runtime) = {
    let mut recorder = state.recorder.lock().unwrap();
    if !recorder.active {
      return;
    }
    recorder.active = false;
    (
      recorder.stop_tx.take(),
      recorder.join_handle.take(),
      recorder.vad_tx.take(),
      recorder.vad_runtime.take(),
    )
  };

  if let Some(runtime) = vad_runtime {
    runtime.recording.store(false, Ordering::Relaxed);
    runtime.processing.store(false, Ordering::Relaxed);
    runtime.pending_flush.store(false, Ordering::Relaxed);
  }

  if let Some(tx) = vad_tx {
    drop(tx);
  }

  if let Some(tx) = stop_tx {
    let _ = tx.send(());
  }
  if let Some(join_handle) = join_handle {
    let _ = join_handle.join();
  }

  let _ = app.emit("capture:state", "idle");
  let _ = update_overlay_state(app, OverlayState::Idle);
}

fn process_vad_segment(
  app_handle: AppHandle,
  settings: Settings,
  samples: Vec<i16>,
  runtime: Arc<VadRuntime>,
) {
  let state = app_handle.state::<AppState>();
  if samples.is_empty() {
    runtime.pending_flush.store(false, Ordering::Relaxed);
    return;
  }
  if let Ok(mut recorder) = state.recorder.lock() {
    recorder.transcribing = true;
  }

  let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
  if samples.len() < min_samples {
    let _ = app_handle.emit("capture:state", "idle");
    let _ = update_overlay_state(&app_handle, OverlayState::Idle);
    let _ = app_handle.emit(
      "transcription:error",
      format!(
        "Audio too short ({} ms). Speak a bit longer.",
        (samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64)
      ),
    );
    runtime.processing.store(false, Ordering::Relaxed);
    runtime.pending_flush.store(false, Ordering::Relaxed);
    if let Ok(mut recorder) = state.recorder.lock() {
      recorder.transcribing = false;
    }
    return;
  }

  runtime.processing.store(true, Ordering::Relaxed);
  let _ = app_handle.emit("capture:state", "transcribing");
  let _ = update_overlay_state(&app_handle, OverlayState::Transcribing);

  let result = transcribe_audio(&app_handle, &settings, &samples);

  if let Ok(mut recorder) = state.recorder.lock() {
    recorder.transcribing = false;
  }

  runtime.processing.store(false, Ordering::Relaxed);
  runtime.pending_flush.store(false, Ordering::Relaxed);

  if runtime.recording.load(Ordering::Relaxed) {
    let _ = app_handle.emit("capture:state", "recording");
    let _ = update_overlay_state(&app_handle, OverlayState::Recording);
  } else {
    let _ = app_handle.emit("capture:state", "idle");
    let _ = update_overlay_state(&app_handle, OverlayState::Idle);
  }

  if settings.audio_cues {
    let _ = app_handle.emit("audio:cue", "stop");
  }

  match result {
    Ok((text, source)) => {
      if !text.trim().is_empty() {
        if let Ok(updated) = push_history_entry_inner(&app_handle, &state.history, text.clone(), source.clone()) {
          let _ = app_handle.emit("history:updated", updated);
        }
        let _ = app_handle.emit(
          "transcription:result",
          TranscriptionResult {
            text: text.clone(),
            source: source.clone(),
          },
        );
        if let Err(err) = paste_text(&text) {
          let _ = app_handle.emit("transcription:error", err);
        }
      }
    }
    Err(err) => {
      let _ = app_handle.emit("transcription:error", err);
    }
  }
}

fn stop_recording_async(app: AppHandle, state: &State<'_, AppState>) {
  let app_handle = app.clone();
  let settings = state.settings.lock().unwrap().clone();

  thread::spawn(move || {
    info!("stop_recording_async called");
    let state = app_handle.state::<AppState>();
    let (buffer, stop_tx, join_handle) = {
      let mut recorder = state.recorder.lock().unwrap();
      if !recorder.active {
        info!("Recording not active, skipping stop");
        return;
      }
      recorder.active = false;
      recorder.transcribing = true;
      let stop_tx = recorder.stop_tx.take();
      let join_handle = recorder.join_handle.take();
      (recorder.buffer.clone(), stop_tx, join_handle)
    };

    if let Some(tx) = stop_tx {
      let _ = tx.send(());
    }
    if let Some(join_handle) = join_handle {
      let _ = join_handle.join();
    }

    let samples = {
      let mut buf = buffer.lock().unwrap();
      buf.drain()
    };

    let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
    if samples.len() < min_samples {
      let _ = app_handle.emit("capture:state", "idle");
      let _ = update_overlay_state(&app_handle, OverlayState::Idle);
      let _ = app_handle.emit(
        "transcription:error",
        format!(
          "Audio too short ({} ms). Hold PTT a bit longer.",
          (samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64)
        ),
      );
      let mut recorder = state.recorder.lock().unwrap();
      recorder.transcribing = false;
      return;
    }

    let _ = app_handle.emit("capture:state", "transcribing");
    let _ = update_overlay_state(&app_handle, OverlayState::Transcribing);

    let result = transcribe_audio(&app_handle, &settings, &samples);

    let mut recorder = state.recorder.lock().unwrap();
    recorder.transcribing = false;
    drop(recorder);

    let _ = app_handle.emit("capture:state", "idle");
    let _ = update_overlay_state(&app_handle, OverlayState::Idle);

    // Emit audio cue if enabled
    if settings.audio_cues {
      let _ = app_handle.emit("audio:cue", "stop");
    }

    match result {
      Ok((text, source)) => {
        if !text.trim().is_empty() {
          if let Ok(updated) = push_history_entry_inner(&app_handle, &state.history, text.clone(), source.clone()) {
            let _ = app_handle.emit("history:updated", updated);
          }
          let _ = app_handle.emit(
            "transcription:result",
            TranscriptionResult {
              text: text.clone(),
              source: source.clone(),
            },
          );
          if let Err(err) = paste_text(&text) {
            let _ = app_handle.emit("transcription:error", err);
          }
        }
      }
      Err(err) => {
        let _ = app_handle.emit("transcription:error", err);
      }
    }
  });
}

fn handle_ptt_press(app: &AppHandle) -> Result<(), String> {
  let state = app.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if settings.mode != "ptt" {
    return Ok(());
  }
  start_recording_with_settings(app, &state, &settings)
}

fn handle_ptt_release_async(app: AppHandle) {
  let app_handle = app.clone();
  let state = app_handle.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if settings.mode != "ptt" {
    return;
  }
  stop_recording_async(app, &state);
}

fn handle_toggle_async(app: AppHandle) {
  let app_handle = app.clone();
  let state = app_handle.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if settings.mode != "ptt" {
    return;
  }

  let active = state.recorder.lock().unwrap().active;
  if active {
    stop_recording_async(app, &state);
  } else {
    let _ = start_recording_with_settings(&app, &state, &settings);
  }
}

fn push_history_entry(
  app: &AppHandle,
  state: &State<'_, AppState>,
  text: String,
  source: String,
) -> Result<Vec<HistoryEntry>, String> {
  push_history_entry_inner(app, &state.history, text, source)
}

fn push_history_entry_inner(
  app: &AppHandle,
  history: &Mutex<Vec<HistoryEntry>>,
  text: String,
  source: String,
) -> Result<Vec<HistoryEntry>, String> {
  let mut history = history.lock().unwrap();
  let entry = HistoryEntry {
    id: format!("h_{}", now_ms()),
    text,
    timestamp_ms: now_ms(),
    source,
  };
  history.insert(0, entry);
  save_history_file(app, &history)?;
  Ok(history.clone())
}

fn encode_wav_i16(samples: &[i16], sample_rate: u32) -> Vec<u8> {
  let data_len = (samples.len() * 2) as u32;
  let mut wav = Vec::with_capacity(44 + samples.len() * 2);

  wav.extend_from_slice(b"RIFF");
  wav.extend_from_slice(&(36 + data_len).to_le_bytes());
  wav.extend_from_slice(b"WAVE");
  wav.extend_from_slice(b"fmt ");
  wav.extend_from_slice(&16u32.to_le_bytes());
  wav.extend_from_slice(&1u16.to_le_bytes());
  wav.extend_from_slice(&1u16.to_le_bytes());
  wav.extend_from_slice(&sample_rate.to_le_bytes());
  wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
  wav.extend_from_slice(&2u16.to_le_bytes());
  wav.extend_from_slice(&16u16.to_le_bytes());
  wav.extend_from_slice(b"data");
  wav.extend_from_slice(&data_len.to_le_bytes());

  for sample in samples {
    wav.extend_from_slice(&sample.to_le_bytes());
  }

  wav
}

fn download_model_file(app: &AppHandle, model_id: &str) -> Result<PathBuf, String> {
  let spec = model_spec(model_id).ok_or_else(|| "Unknown model".to_string())?;
  let models_dir = resolve_models_dir(app);
  let dest_path = models_dir.join(spec.file_name);
  if dest_path.exists() {
    return Ok(dest_path);
  }

  let base_url = std::env::var("TRISPR_WHISPER_MODEL_BASE_URL").unwrap_or_else(|_| DEFAULT_MODEL_BASE_URL.to_string());
  let url = format!("{}/{}", base_url.trim_end_matches('/'), spec.file_name);

  let tmp_path = dest_path.with_extension("bin.part");
  let result = (|| -> Result<PathBuf, String> {
    let response = ureq::get(&url).call().map_err(|e| e.to_string())?;
    let total = response
      .header("Content-Length")
      .and_then(|value| value.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&tmp_path).map_err(|e| e.to_string())?;

    let mut downloaded = 0u64;
    let mut last_emit = Instant::now();
    let mut buffer = [0u8; 64 * 1024];

    loop {
      let read_bytes = reader.read(&mut buffer).map_err(|e| e.to_string())?;
      if read_bytes == 0 {
        break;
      }
      file
        .write_all(&buffer[..read_bytes])
        .map_err(|e| e.to_string())?;
      downloaded += read_bytes as u64;

      if last_emit.elapsed() >= Duration::from_millis(250) {
        let _ = app.emit(
          "model:download-progress",
          DownloadProgress {
            id: model_id.to_string(),
            downloaded,
            total,
          },
        );
        last_emit = Instant::now();
      }
    }

    file.flush().map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &dest_path).map_err(|e| e.to_string())?;

    let _ = app.emit(
      "model:download-progress",
      DownloadProgress {
        id: model_id.to_string(),
        downloaded,
        total,
      },
    );

    Ok(dest_path)
  })();

  if result.is_err() {
    let _ = fs::remove_file(&tmp_path);
  }

  result
}

fn transcribe_audio(
  app: &AppHandle,
  settings: &Settings,
  samples: &[i16],
) -> Result<(String, String), String> {
  let wav_bytes = encode_wav_i16(samples, TARGET_SAMPLE_RATE);

  if settings.cloud_fallback {
    let text = transcribe_cloud(&wav_bytes)?;
    return Ok((text, "cloud".to_string()));
  }

  let text = transcribe_local(app, settings, &wav_bytes)?;
  Ok((text, "local".to_string()))
}

fn transcribe_local(app: &AppHandle, settings: &Settings, wav_bytes: &[u8]) -> Result<String, String> {
  let temp_dir = std::env::temp_dir();
  let stamp = now_ms();
  let base = temp_dir.join(format!("trispr_{}", stamp));
  let wav_path = base.with_extension("wav");
  let output_base = base.clone();

  fs::write(&wav_path, wav_bytes).map_err(|e| e.to_string())?;

  let model_path = resolve_model_path(app, &settings.model)
    .ok_or_else(|| "Model file not found. Set TRISPR_WHISPER_MODEL_DIR or TRISPR_WHISPER_MODEL.".to_string())?;

  let cli_path = resolve_whisper_cli_path();

  let mut command = if let Some(path) = cli_path {
    Command::new(path)
  } else {
    Command::new("whisper-cli")
  };

  let threads = std::thread::available_parallelism()
    .map(|n| n.get().to_string())
    .unwrap_or_else(|_| "4".to_string());

  command
    .arg("-m")
    .arg(model_path)
    .arg("-f")
    .arg(&wav_path)
    .arg("-t")
    .arg(threads)
    .arg("-l")
    .arg("auto")
    .arg("-nt")
    .arg("-otxt")
    .arg("-of")
    .arg(&output_base)
    .arg("-np")
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

  let output = command.output().map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(format!("whisper-cli failed: {}", stderr));
  }

  let txt_path = output_base.with_extension("txt");
  let text = fs::read_to_string(&txt_path).map_err(|e| e.to_string())?;

  let _ = fs::remove_file(&wav_path);
  let _ = fs::remove_file(&txt_path);

  Ok(text.trim().to_string())
}

#[derive(Deserialize)]
struct CloudResponse {
  text: String,
}

fn transcribe_cloud(wav_bytes: &[u8]) -> Result<String, String> {
  let endpoint = std::env::var("TRISPR_CLOUD_ENDPOINT").unwrap_or_default();
  if endpoint.trim().is_empty() {
    return Err("Cloud fallback not configured".to_string());
  }

  let token = std::env::var("TRISPR_CLOUD_TOKEN").unwrap_or_default();
  let mut req = ureq::post(&endpoint).set("Content-Type", "audio/wav");
  if !token.trim().is_empty() {
    req = req.set("Authorization", &format!("Bearer {}", token));
  }

  let resp = req.send_bytes(wav_bytes).map_err(|e| e.to_string())?;
  let parsed: CloudResponse = resp.into_json().map_err(|e| e.to_string())?;
  Ok(parsed.text)
}

fn resolve_whisper_cli_path() -> Option<PathBuf> {
  if let Ok(path) = std::env::var("TRISPR_WHISPER_CLI") {
    let candidate = PathBuf::from(path);
    if candidate.exists() {
      return Some(candidate);
    }
  }

  let mut candidates = Vec::new();
  if let Ok(cwd) = std::env::current_dir() {
    candidates.push(cwd.join("whisper-cli"));
    candidates.push(cwd.join("whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../../whisper.cpp/build/bin/whisper-cli"));
  }

  for path in candidates {
    if path.exists() {
      return Some(path);
    }
  }

  None
}

/// Initialize logging with tracing
fn init_logging() {
  use tracing_subscriber::{fmt, EnvFilter};

  // Try to create log file in app data directory
  // For now, just log to stdout in development
  let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new("info"));

  fmt()
    .with_env_filter(filter)
    .with_target(false)
    .with_thread_ids(false)
    .with_file(true)
    .with_line_number(true)
    .init();

  info!("Trispr Flow starting up");
}

/// Emit an error event to the frontend
fn emit_error(app: &AppHandle, error: AppError, context: Option<&str>) {
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

fn paste_text(text: &str) -> Result<(), String> {
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

      app.manage(AppState {
        settings: Mutex::new(settings.clone()),
        history: Mutex::new(history),
        recorder: Mutex::new(Recorder::new()),
        downloads: Mutex::new(HashSet::new()),
      });

      if let Err(err) = register_hotkeys(app.handle(), &settings) {
        eprintln!("⚠ Failed to register hotkeys: {}", err);
      }

      if settings.mode == "vad" {
        if let Err(err) = start_vad_monitor(app.handle(), &app.state::<AppState>(), &settings) {
          eprintln!("⚠ Failed to start VAD monitor: {}", err);
        }
      }

      // Create overlay window at startup
      if let Err(err) = overlay::create_overlay_window(&app.handle()) {
        eprintln!("⚠ Failed to create overlay window: {}", err);
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

      let _tray_icon = tauri::tray::TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Trispr Flow")
        .on_tray_icon_event(|tray, event| {
          use tauri::tray::{MouseButton, TrayIconEvent};
          match event {
            TrayIconEvent::Click { button: MouseButton::Left, .. } => {
              toggle_main_window(tray.app_handle());
            }
            _ => {}
          }
        })
        .on_menu_event(|app, event| {
          match event.id.as_ref() {
            "show" => {
              show_main_window(app);
            }
            "toggle-cloud" => {
              let state = app.state::<AppState>();
              let mut current = state.settings.lock().unwrap();
              current.cloud_fallback = !current.cloud_fallback;
              let _ = save_settings_file(app, &current);
              let _ = register_hotkeys(app, &current);
              let _ = app.emit("settings-changed", current.clone());
              let _ = app.emit("menu:update-cloud", current.cloud_fallback.to_string());
            }
            "quit" => {
              app.exit(0);
            }
            _ => {}
          }
        })
        .menu({
          let cloud_item = CheckMenuItem::with_id(
            app,
            "toggle-cloud",
            "Claude fallback",
            true,
            settings.cloud_fallback,
            None::<&str>,
          )?;

          let cloud_item_clone = cloud_item.clone();
          app.listen("menu:update-cloud", move |event| {
            let checked = event.payload() == "true";
            let _ = cloud_item_clone.set_checked(checked);
          });

          &tauri::menu::Menu::with_items(
            app,
            &[
              &tauri::menu::MenuItem::with_id(app, "show", "Open Trispr Flow", true, None::<&str>)?,
              &tauri::menu::PredefinedMenuItem::separator(app)?,
              &cloud_item,
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
      list_models,
      download_model,
      get_history,
      add_history_entry,
      start_recording,
      stop_recording,
      validate_hotkey,
      test_hotkey,
      get_hotkey_conflicts,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
