use crate::audio::Recorder;
use crate::constants::{
  HALLUCINATION_MAX_CHARS,
  HALLUCINATION_MAX_DURATION_MS,
  HALLUCINATION_MAX_WORDS,
  HALLUCINATION_RMS_THRESHOLD,
  VAD_SILENCE_MS_DEFAULT,
  VAD_THRESHOLD_START_DEFAULT,
  VAD_THRESHOLD_SUSTAIN_DEFAULT,
};
use crate::paths::{resolve_config_path, resolve_data_path};
use crate::transcription::TranscribeRecorder;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
  pub(crate) mode: String,
  pub(crate) hotkey_ptt: String,
  pub(crate) hotkey_toggle: String,
  pub(crate) input_device: String,
  pub(crate) language_mode: String,
  pub(crate) model: String,
  pub(crate) cloud_fallback: bool,
  pub(crate) audio_cues: bool,
  pub(crate) audio_cues_volume: f32,
  pub(crate) ptt_use_vad: bool, // Enable VAD threshold check even in PTT mode
  pub(crate) vad_threshold: f32, // Legacy: now maps to vad_threshold_start
  pub(crate) vad_threshold_start: f32,
  pub(crate) vad_threshold_sustain: f32,
  pub(crate) vad_silence_ms: u64,
  pub(crate) transcribe_enabled: bool,
  pub(crate) transcribe_hotkey: String,
  pub(crate) transcribe_output_device: String,
  pub(crate) transcribe_vad_mode: bool,
  pub(crate) transcribe_vad_threshold: f32,
  pub(crate) transcribe_vad_silence_ms: u64,
  pub(crate) transcribe_batch_interval_ms: u64,
  pub(crate) transcribe_chunk_overlap_ms: u64,
  pub(crate) transcribe_input_gain_db: f32,
  pub(crate) mic_input_gain_db: f32,
  pub(crate) capture_enabled: bool,
  pub(crate) model_source: String,
  pub(crate) model_custom_url: String,
  pub(crate) model_storage_dir: String,
  pub(crate) overlay_color: String,
  pub(crate) overlay_min_radius: f32,
  pub(crate) overlay_max_radius: f32,
  pub(crate) overlay_rise_ms: u64,
  pub(crate) overlay_fall_ms: u64,
  pub(crate) overlay_opacity_inactive: f32,
  pub(crate) overlay_opacity_active: f32,
  pub(crate) overlay_kitt_color: String,
  pub(crate) overlay_kitt_rise_ms: u64,
  pub(crate) overlay_kitt_fall_ms: u64,
  pub(crate) overlay_kitt_opacity_inactive: f32,
  pub(crate) overlay_kitt_opacity_active: f32,
  pub(crate) overlay_pos_x: f64,
  pub(crate) overlay_pos_y: f64,
  pub(crate) overlay_kitt_pos_x: f64,
  pub(crate) overlay_kitt_pos_y: f64,
  pub(crate) overlay_style: String, // "dot" | "kitt"
  pub(crate) overlay_kitt_min_width: f32,
  pub(crate) overlay_kitt_max_width: f32,
  pub(crate) overlay_kitt_height: f32,
  pub(crate) hallucination_filter_enabled: bool,
  pub(crate) hallucination_rms_threshold: f32,
  pub(crate) hallucination_max_duration_ms: u64,
  pub(crate) hallucination_max_words: u32,
  pub(crate) hallucination_max_chars: u32,
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
      ptt_use_vad: false,
      vad_threshold: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_start: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_sustain: VAD_THRESHOLD_SUSTAIN_DEFAULT,
      vad_silence_ms: VAD_SILENCE_MS_DEFAULT,
      transcribe_enabled: false,
      transcribe_hotkey: "CommandOrControl+Shift+O".to_string(),
      transcribe_output_device: "default".to_string(),
      transcribe_vad_mode: false,
      transcribe_vad_threshold: 0.04,
      transcribe_vad_silence_ms: 900,
      transcribe_batch_interval_ms: 8000,
      transcribe_chunk_overlap_ms: 1000,
      transcribe_input_gain_db: 0.0,
      mic_input_gain_db: 0.0,
      capture_enabled: true,
      model_source: "default".to_string(),
      model_custom_url: "".to_string(),
      model_storage_dir: "".to_string(),
      overlay_color: "#ff3d2e".to_string(),
      overlay_min_radius: 8.0,
      overlay_max_radius: 24.0,
      overlay_rise_ms: 80,
      overlay_fall_ms: 160,
      overlay_opacity_inactive: 0.2,
      overlay_opacity_active: 0.8,
      overlay_kitt_color: "#ff3d2e".to_string(),
      overlay_kitt_rise_ms: 80,
      overlay_kitt_fall_ms: 160,
      overlay_kitt_opacity_inactive: 0.2,
      overlay_kitt_opacity_active: 0.8,
      overlay_pos_x: 50.0,          // 50% = horizontal center
      overlay_pos_y: 90.0,          // 90% = bottom area
      overlay_kitt_pos_x: 50.0,     // 50% = horizontal center
      overlay_kitt_pos_y: 90.0,     // 90% = bottom area
      overlay_style: "dot".to_string(),
      overlay_kitt_min_width: 20.0,
      overlay_kitt_max_width: 200.0,
      overlay_kitt_height: 20.0,
      hallucination_filter_enabled: true,
      hallucination_rms_threshold: HALLUCINATION_RMS_THRESHOLD,
      hallucination_max_duration_ms: HALLUCINATION_MAX_DURATION_MS,
      hallucination_max_words: HALLUCINATION_MAX_WORDS as u32,
      hallucination_max_chars: HALLUCINATION_MAX_CHARS as u32,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HistoryEntry {
  pub(crate) id: String,
  pub(crate) text: String,
  pub(crate) timestamp_ms: u64,
  pub(crate) source: String,
}

pub(crate) struct AppState {
  pub(crate) settings: Mutex<Settings>,
  pub(crate) history: Mutex<Vec<HistoryEntry>>,
  pub(crate) history_transcribe: Mutex<Vec<HistoryEntry>>,
  pub(crate) recorder: Mutex<Recorder>,
  pub(crate) transcribe: Mutex<TranscribeRecorder>,
  pub(crate) downloads: Mutex<HashSet<String>>,
  pub(crate) transcribe_active: AtomicBool,
}

pub(crate) fn load_settings(app: &AppHandle) -> Settings {
  let path = resolve_config_path(app, "settings.json");
  match fs::read_to_string(path) {
    Ok(raw) => {
      let mut settings: Settings = serde_json::from_str(&raw).unwrap_or_default();
      if settings.mode != "ptt" && settings.mode != "vad" {
        settings.mode = "ptt".to_string();
      }
      // Migrate legacy vad_threshold to new dual-threshold system
      if settings.vad_threshold_start <= 0.0 {
        settings.vad_threshold_start = if settings.vad_threshold > 0.0 {
          settings.vad_threshold
        } else {
          VAD_THRESHOLD_START_DEFAULT
        };
      }
      if settings.vad_threshold_sustain <= 0.0 {
        settings.vad_threshold_sustain = VAD_THRESHOLD_SUSTAIN_DEFAULT;
      }
      // Clamp thresholds to valid range
      if !(0.001..=1.0).contains(&settings.vad_threshold_start) {
        settings.vad_threshold_start = VAD_THRESHOLD_START_DEFAULT;
      }
      if !(0.001..=1.0).contains(&settings.vad_threshold_sustain) {
        settings.vad_threshold_sustain = VAD_THRESHOLD_SUSTAIN_DEFAULT;
      }
      // Ensure sustain <= start
      if settings.vad_threshold_sustain > settings.vad_threshold_start {
        settings.vad_threshold_sustain = settings.vad_threshold_start;
      }
      // Sync legacy field
      settings.vad_threshold = settings.vad_threshold_start;
      if settings.vad_silence_ms < 100 {
        settings.vad_silence_ms = VAD_SILENCE_MS_DEFAULT;
      }
      if !(0.0..=1.0).contains(&settings.transcribe_vad_threshold) {
        settings.transcribe_vad_threshold = 0.04;
      }
      if settings.transcribe_batch_interval_ms < 4000 {
        settings.transcribe_batch_interval_ms = 4000;
      }
      if settings.transcribe_batch_interval_ms > 15000 {
        settings.transcribe_batch_interval_ms = 15000;
      }
      if settings.transcribe_chunk_overlap_ms > settings.transcribe_batch_interval_ms {
        settings.transcribe_chunk_overlap_ms = settings.transcribe_batch_interval_ms / 2;
      }
      if settings.transcribe_chunk_overlap_ms > 3000 {
        settings.transcribe_chunk_overlap_ms = 3000;
      }
      if settings.transcribe_vad_silence_ms < 200 {
        settings.transcribe_vad_silence_ms = 200;
      }
      if settings.transcribe_vad_silence_ms > 5000 {
        settings.transcribe_vad_silence_ms = 5000;
      }
      if settings.model_source.trim().is_empty() {
        settings.model_source = "default".to_string();
      }
      if settings.model_storage_dir.trim().is_empty() {
        if let Ok(dir) = std::env::var("TRISPR_WHISPER_MODEL_DIR") {
          settings.model_storage_dir = dir;
        } else {
          settings.model_storage_dir = "".to_string();
        }
      }
      sync_model_dir_env(&settings);
      settings.transcribe_input_gain_db = settings.transcribe_input_gain_db.clamp(-30.0, 30.0);
      settings.mic_input_gain_db = settings.mic_input_gain_db.clamp(-30.0, 30.0);
      #[cfg(target_os = "windows")]
      if settings.transcribe_output_device != "default"
        && !settings.transcribe_output_device.starts_with("wasapi:")
      {
        settings.transcribe_output_device = "default".to_string();
      }
      if settings.overlay_min_radius < 4.0 {
        settings.overlay_min_radius = 4.0;
      }
      if settings.overlay_max_radius < settings.overlay_min_radius {
        settings.overlay_max_radius = settings.overlay_min_radius + 4.0;
      }
      if settings.overlay_max_radius > 64.0 {
        settings.overlay_max_radius = 64.0;
      }
      if settings.overlay_rise_ms < 20 {
        settings.overlay_rise_ms = 20;
      }
      if settings.overlay_fall_ms < 20 {
        settings.overlay_fall_ms = 20;
      }
      if !(0.0..=1.0).contains(&settings.overlay_opacity_inactive) {
        settings.overlay_opacity_inactive = 0.2;
      }
      if !(0.0..=1.0).contains(&settings.overlay_opacity_active) {
        settings.overlay_opacity_active = 0.8;
      }
      if settings.overlay_opacity_inactive < 0.05 {
        settings.overlay_opacity_inactive = 0.05;
      }
      if settings.overlay_opacity_active < 0.05 {
        settings.overlay_opacity_active = 0.05;
      }
      if settings.overlay_opacity_active < settings.overlay_opacity_inactive {
        settings.overlay_opacity_active = settings.overlay_opacity_inactive;
      }
      let defaults = Settings::default();
      let approx_eq = |a: f32, b: f32| (a - b).abs() < 0.0001;
      if settings.overlay_kitt_color == defaults.overlay_kitt_color
        && settings.overlay_color != defaults.overlay_color
      {
        settings.overlay_kitt_color = settings.overlay_color.clone();
      }
      if settings.overlay_kitt_rise_ms == defaults.overlay_kitt_rise_ms
        && settings.overlay_rise_ms != defaults.overlay_rise_ms
      {
        settings.overlay_kitt_rise_ms = settings.overlay_rise_ms;
      }
      if settings.overlay_kitt_fall_ms == defaults.overlay_kitt_fall_ms
        && settings.overlay_fall_ms != defaults.overlay_fall_ms
      {
        settings.overlay_kitt_fall_ms = settings.overlay_fall_ms;
      }
      if approx_eq(settings.overlay_kitt_opacity_inactive, defaults.overlay_kitt_opacity_inactive)
        && !approx_eq(settings.overlay_opacity_inactive, defaults.overlay_opacity_inactive)
      {
        settings.overlay_kitt_opacity_inactive = settings.overlay_opacity_inactive;
      }
      if approx_eq(settings.overlay_kitt_opacity_active, defaults.overlay_kitt_opacity_active)
        && !approx_eq(settings.overlay_opacity_active, defaults.overlay_opacity_active)
      {
        settings.overlay_kitt_opacity_active = settings.overlay_opacity_active;
      }
      if settings.overlay_kitt_pos_x.is_nan() || settings.overlay_kitt_pos_y.is_nan() {
        settings.overlay_kitt_pos_x = settings.overlay_pos_x;
        settings.overlay_kitt_pos_y = settings.overlay_pos_y;
      }
      if settings.overlay_kitt_pos_x < 0.0 {
        settings.overlay_kitt_pos_x = 0.0;
      }
      if settings.overlay_kitt_pos_y < 0.0 {
        settings.overlay_kitt_pos_y = 0.0;
      }
      if settings.overlay_pos_x < 0.0 {
        settings.overlay_pos_x = 0.0;
      }
      if settings.overlay_pos_y < 0.0 {
        settings.overlay_pos_y = 0.0;
      }
      if (settings.overlay_kitt_pos_x - 12.0).abs() < 0.001
        && (settings.overlay_kitt_pos_y - 12.0).abs() < 0.001
        && ((settings.overlay_pos_x - 12.0).abs() > 0.001
          || (settings.overlay_pos_y - 12.0).abs() > 0.001)
      {
        settings.overlay_kitt_pos_x = settings.overlay_pos_x;
        settings.overlay_kitt_pos_y = settings.overlay_pos_y;
      }
      if settings.overlay_kitt_color.trim().is_empty() {
        settings.overlay_kitt_color = "#ff3d2e".to_string();
      }
      if settings.overlay_kitt_min_width < 4.0 {
        settings.overlay_kitt_min_width = 4.0;
      }
      if settings.overlay_kitt_max_width < settings.overlay_kitt_min_width {
        settings.overlay_kitt_max_width = settings.overlay_kitt_min_width;
      }
      if settings.overlay_kitt_max_width > 800.0 {
        settings.overlay_kitt_max_width = 800.0;
      }
      if settings.overlay_kitt_height < 8.0 {
        settings.overlay_kitt_height = 8.0;
      }
      if settings.overlay_kitt_height > 40.0 {
        settings.overlay_kitt_height = 40.0;
      }
      if settings.overlay_kitt_rise_ms < 20 {
        settings.overlay_kitt_rise_ms = 20;
      }
      if settings.overlay_kitt_fall_ms < 20 {
        settings.overlay_kitt_fall_ms = 20;
      }
      if !(0.0..=1.0).contains(&settings.overlay_kitt_opacity_inactive) {
        settings.overlay_kitt_opacity_inactive = 0.2;
      }
      if !(0.0..=1.0).contains(&settings.overlay_kitt_opacity_active) {
        settings.overlay_kitt_opacity_active = 0.8;
      }
      if settings.overlay_kitt_opacity_inactive < 0.05 {
        settings.overlay_kitt_opacity_inactive = 0.05;
      }
      if settings.overlay_kitt_opacity_active < 0.05 {
        settings.overlay_kitt_opacity_active = 0.05;
      }
      if settings.overlay_kitt_opacity_active < settings.overlay_kitt_opacity_inactive {
        settings.overlay_kitt_opacity_active = settings.overlay_kitt_opacity_inactive;
      }
      // Transcribe enablement is session-only; always start disabled.
      settings.transcribe_enabled = false;
      settings
    }
    Err(_) => Settings::default(),
  }
}

pub(crate) fn save_settings_file(app: &AppHandle, settings: &Settings) -> Result<(), String> {
  let path = resolve_config_path(app, "settings.json");
  let mut persisted = settings.clone();
  // Do not persist session-only transcribe enablement.
  persisted.transcribe_enabled = false;
  let raw = serde_json::to_string_pretty(&persisted).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn sync_model_dir_env(settings: &Settings) {
  let trimmed = settings.model_storage_dir.trim();
  if trimmed.is_empty() {
    std::env::remove_var("TRISPR_WHISPER_MODEL_DIR");
  } else {
    std::env::set_var("TRISPR_WHISPER_MODEL_DIR", trimmed);
  }
}

pub(crate) fn load_history(app: &AppHandle) -> Vec<HistoryEntry> {
  let path = resolve_data_path(app, "history.json");
  match fs::read_to_string(path) {
    Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
    Err(_) => Vec::new(),
  }
}

pub(crate) fn save_history_file(app: &AppHandle, history: &[HistoryEntry]) -> Result<(), String> {
  let path = resolve_data_path(app, "history.json");
  let raw = serde_json::to_string_pretty(history).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn load_transcribe_history(app: &AppHandle) -> Vec<HistoryEntry> {
  let path = resolve_data_path(app, "history_transcribe.json");
  match fs::read_to_string(path) {
    Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
    Err(_) => Vec::new(),
  }
}

pub(crate) fn save_transcribe_history_file(
  app: &AppHandle,
  history: &[HistoryEntry],
) -> Result<(), String> {
  let path = resolve_data_path(app, "history_transcribe.json");
  let raw = serde_json::to_string_pretty(history).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

pub(crate) fn push_history_entry_inner(
  app: &AppHandle,
  history: &Mutex<Vec<HistoryEntry>>,
  text: String,
  source: String,
) -> Result<Vec<HistoryEntry>, String> {
  let mut history = history.lock().unwrap();
  let entry = HistoryEntry {
    id: format!("h_{}", crate::util::now_ms()),
    text,
    timestamp_ms: crate::util::now_ms(),
    source,
  };
  history.insert(0, entry);
  save_history_file(app, &history)?;
  Ok(history.clone())
}

pub(crate) fn push_transcribe_entry_inner(
  app: &AppHandle,
  history: &Mutex<Vec<HistoryEntry>>,
  text: String,
) -> Result<Vec<HistoryEntry>, String> {
  let mut history = history.lock().unwrap();
  let entry = HistoryEntry {
    id: format!("o_{}", crate::util::now_ms()),
    text,
    timestamp_ms: crate::util::now_ms(),
    source: "output".to_string(),
  };
  history.insert(0, entry);
  save_transcribe_history_file(app, &history)?;
  let _ = app.emit("transcribe:history-updated", history.clone());
  Ok(history.clone())
}
