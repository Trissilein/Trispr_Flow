use crate::ai_fallback::models::{AIFallbackSettings, AIProvidersSettings};
use crate::ai_fallback::provider::{is_local_ollama_endpoint, prompt_for_profile};
use crate::audio::Recorder;
use crate::constants::{
    HALLUCINATION_MAX_CHARS, HALLUCINATION_MAX_DURATION_MS, HALLUCINATION_MAX_WORDS,
    HALLUCINATION_RMS_THRESHOLD, VAD_SILENCE_MS_DEFAULT, VAD_THRESHOLD_START_DEFAULT,
    VAD_THRESHOLD_SUSTAIN_DEFAULT,
};
use crate::paths::{resolve_config_path, resolve_data_path};
use crate::transcription::TranscribeRecorder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

const HISTORY_LOCK_WARN_MS: u128 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetupSettings {
    pub(crate) local_ai_wizard_completed: bool,
    pub(crate) local_ai_wizard_pending: bool,
    pub(crate) ollama_remote_expert_opt_in: bool,
}

impl Default for SetupSettings {
    fn default() -> Self {
        Self {
            local_ai_wizard_completed: false,
            local_ai_wizard_pending: true,
            ollama_remote_expert_opt_in: false,
        }
    }
}

fn default_accent_color() -> String {
    "#4be0d4".to_string()
}

fn default_overlay_refining_indicator_enabled() -> bool {
    true
}

fn default_overlay_refining_indicator_preset() -> String {
    "standard".to_string()
}

fn default_overlay_refining_indicator_color() -> String {
    "#6ec8ff".to_string()
}

fn default_overlay_refining_indicator_speed_ms() -> u64 {
    1_150
}

fn default_overlay_refining_indicator_range() -> f32 {
    100.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) mode: String,
    pub(crate) hotkey_ptt: String,
    pub(crate) hotkey_toggle: String,
    pub(crate) input_device: String,
    pub(crate) language_mode: String,
    pub(crate) language_pinned: bool,
    pub(crate) model: String,
    // Legacy toggle kept for backward compatibility with old cloud transcription paths.
    pub(crate) cloud_fallback: bool,
    // v0.7.0 AI Fallback settings
    pub(crate) ai_fallback: AIFallbackSettings,
    pub(crate) providers: AIProvidersSettings,
    // First-run setup flags
    pub(crate) setup: SetupSettings,
    pub(crate) audio_cues: bool,
    pub(crate) audio_cues_volume: f32,
    pub(crate) ptt_use_vad: bool, // Enable VAD threshold check even in PTT mode
    pub(crate) vad_threshold: f32, // Legacy: now maps to vad_threshold_start
    pub(crate) vad_threshold_start: f32,
    pub(crate) vad_threshold_sustain: f32,
    pub(crate) vad_silence_ms: u64,
    pub(crate) transcribe_enabled: bool,
    pub(crate) transcribe_hotkey: String,
    pub(crate) hotkey_toggle_activation_words: String,
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
    pub(crate) hidden_external_models: HashSet<String>,
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
    #[serde(default = "default_accent_color")]
    pub(crate) accent_color: String,
    #[serde(default = "default_overlay_refining_indicator_enabled")]
    pub(crate) overlay_refining_indicator_enabled: bool,
    #[serde(default = "default_overlay_refining_indicator_preset")]
    pub(crate) overlay_refining_indicator_preset: String, // "subtle" | "standard" | "intense"
    #[serde(default = "default_overlay_refining_indicator_color")]
    pub(crate) overlay_refining_indicator_color: String,
    #[serde(default = "default_overlay_refining_indicator_speed_ms")]
    pub(crate) overlay_refining_indicator_speed_ms: u64,
    #[serde(default = "default_overlay_refining_indicator_range")]
    pub(crate) overlay_refining_indicator_range: f32,
    pub(crate) overlay_kitt_min_width: f32,
    pub(crate) overlay_kitt_max_width: f32,
    pub(crate) overlay_kitt_height: f32,
    pub(crate) hallucination_filter_enabled: bool,
    pub(crate) hallucination_rms_threshold: f32,
    pub(crate) hallucination_max_duration_ms: u64,
    pub(crate) hallucination_max_words: u32,
    pub(crate) hallucination_max_chars: u32,
    pub(crate) activation_words_enabled: bool,
    pub(crate) activation_words: Vec<String>,
    // Post-processing settings
    pub(crate) postproc_enabled: bool,
    pub(crate) postproc_language: String,
    pub(crate) postproc_punctuation_enabled: bool,
    pub(crate) postproc_capitalization_enabled: bool,
    pub(crate) postproc_numbers_enabled: bool,
    pub(crate) postproc_custom_vocab_enabled: bool,
    pub(crate) postproc_custom_vocab: HashMap<String, String>,
    pub(crate) postproc_llm_enabled: bool,
    pub(crate) postproc_llm_provider: String,
    #[serde(skip_serializing)]
    pub(crate) postproc_llm_api_key: String,
    pub(crate) postproc_llm_model: String,
    pub(crate) postproc_llm_prompt: String,
    // Chapter settings (v0.5.0)
    pub(crate) chapters_enabled: bool,
    pub(crate) chapters_show_in: String, // "conversation" | "all"
    pub(crate) chapters_method: String,  // "silence" | "time" | "hybrid"
    // Legacy chapter detection settings
    pub(crate) chapter_silence_enabled: bool,
    pub(crate) chapter_silence_threshold_ms: u64,
    // Analysis launcher settings (external tool)
    pub(crate) opus_enabled: bool,
    pub(crate) opus_bitrate_kbps: u32,
    pub(crate) auto_save_system_audio: bool, // Auto-save system audio as OPUS
    pub(crate) auto_save_mic_audio: bool,    // Auto-save mic continuous audio as OPUS
    // Intelligent continuous dump settings
    pub(crate) continuous_dump_enabled: bool,
    pub(crate) continuous_dump_profile: String, // "balanced" | "low_latency" | "high_quality"
    pub(crate) continuous_soft_flush_ms: u64,
    pub(crate) continuous_silence_flush_ms: u64,
    pub(crate) continuous_hard_cut_ms: u64,
    pub(crate) continuous_min_chunk_ms: u64,
    pub(crate) continuous_pre_roll_ms: u64,
    pub(crate) continuous_post_roll_ms: u64,
    pub(crate) continuous_idle_keepalive_ms: u64,
    pub(crate) continuous_mic_override_enabled: bool,
    pub(crate) continuous_mic_soft_flush_ms: u64,
    pub(crate) continuous_mic_silence_flush_ms: u64,
    pub(crate) continuous_mic_hard_cut_ms: u64,
    pub(crate) continuous_system_override_enabled: bool,
    pub(crate) continuous_system_soft_flush_ms: u64,
    pub(crate) continuous_system_silence_flush_ms: u64,
    pub(crate) continuous_system_hard_cut_ms: u64,
    pub(crate) transcribe_backend: String, // "whisper_cpp" | future backends
    // Session consolidation settings (v0.7.0)
    pub(crate) session_idle_timeout_ms: u64, // Auto-finalize session after N ms of silence
    pub(crate) ptt_session_grouping_enabled: bool, // Group multiple PTT presses into one session
    pub(crate) ptt_session_group_timeout_s: u64, // PTT presses within this window → same session
    // Main window state
    pub(crate) main_window_x: Option<i32>,
    pub(crate) main_window_y: Option<i32>,
    pub(crate) main_window_width: Option<u32>,
    pub(crate) main_window_height: Option<u32>,
    pub(crate) main_window_monitor: Option<String>,
    /// Window visibility state at shutdown: "normal", "minimized", or "tray"
    pub(crate) main_window_start_state: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
      mode: "ptt".to_string(),
      hotkey_ptt: "CommandOrControl+Shift+Space".to_string(),
      hotkey_toggle: "CommandOrControl+Shift+M".to_string(),
      input_device: "default".to_string(),
      language_mode: "auto".to_string(),
      language_pinned: false,
      model: "whisper-large-v3".to_string(),
      cloud_fallback: false,
      ai_fallback: AIFallbackSettings::default(),
      providers: AIProvidersSettings::default(),
      setup: SetupSettings::default(),
      audio_cues: true,
      audio_cues_volume: 0.3,
      ptt_use_vad: false,
      vad_threshold: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_start: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_sustain: VAD_THRESHOLD_SUSTAIN_DEFAULT,
      vad_silence_ms: VAD_SILENCE_MS_DEFAULT,
      transcribe_enabled: false,
      transcribe_hotkey: "CommandOrControl+Shift+T".to_string(),
      hotkey_toggle_activation_words: "CommandOrControl+Shift+A".to_string(),
      transcribe_output_device: "default".to_string(),
      transcribe_vad_mode: false,
      transcribe_vad_threshold: 0.04,
      transcribe_vad_silence_ms: 900,
      transcribe_batch_interval_ms: 8000,
      transcribe_chunk_overlap_ms: 1000,
      transcribe_input_gain_db: 0.0,
      mic_input_gain_db: 0.0,
      capture_enabled: false,
      model_source: "default".to_string(),
      model_custom_url: "".to_string(),
      model_storage_dir: "".to_string(),
      hidden_external_models: HashSet::new(),
      overlay_color: "#ff3d2e".to_string(),
      overlay_min_radius: 16.0,
      overlay_max_radius: 64.0,
      overlay_rise_ms: 60,
      overlay_fall_ms: 140,
      overlay_opacity_inactive: 0.1,
      overlay_opacity_active: 0.97,
      overlay_kitt_color: "#ff3d2e".to_string(),
      overlay_kitt_rise_ms: 60,
      overlay_kitt_fall_ms: 140,
      overlay_kitt_opacity_inactive: 0.1,
      overlay_kitt_opacity_active: 1.0,
      overlay_pos_x: 50.0,          // 50% = horizontal center
      overlay_pos_y: 90.0,          // 90% = bottom area
      overlay_kitt_pos_x: 50.0,     // 50% = horizontal center
      overlay_kitt_pos_y: 90.0,     // 90% = bottom area
      overlay_style: "dot".to_string(),
      accent_color: "#4be0d4".to_string(),
      overlay_refining_indicator_enabled: true,
      overlay_refining_indicator_preset: "standard".to_string(),
      overlay_refining_indicator_color: "#6ec8ff".to_string(),
      overlay_refining_indicator_speed_ms: 1_150,
      overlay_refining_indicator_range: 100.0,
      overlay_kitt_min_width: 20.0,
      overlay_kitt_max_width: 700.0,
      overlay_kitt_height: 13.0,
      hallucination_filter_enabled: true,
      hallucination_rms_threshold: HALLUCINATION_RMS_THRESHOLD,
      hallucination_max_duration_ms: HALLUCINATION_MAX_DURATION_MS,
      hallucination_max_words: HALLUCINATION_MAX_WORDS as u32,
      hallucination_max_chars: HALLUCINATION_MAX_CHARS as u32,
      activation_words_enabled: false,
      activation_words: vec!["computer".to_string(), "hey assistant".to_string()],
      postproc_enabled: false,
      postproc_language: "en".to_string(),
      postproc_punctuation_enabled: true,
      postproc_capitalization_enabled: true,
      postproc_numbers_enabled: true,
      postproc_custom_vocab_enabled: false,
      postproc_custom_vocab: HashMap::new(),
      postproc_llm_enabled: false,
      postproc_llm_provider: "ollama".to_string(),
      postproc_llm_api_key: String::new(),
      postproc_llm_model: String::new(),
      postproc_llm_prompt: "Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.".to_string(),
      // Chapter settings (v0.5.0) - disabled by default per DEC-018
      chapters_enabled: false,
      chapters_show_in: "conversation".to_string(),
      chapters_method: "hybrid".to_string(),
      // Legacy chapter settings
      chapter_silence_enabled: false,
      chapter_silence_threshold_ms: 10000, // 10 seconds
      opus_enabled: true,
      opus_bitrate_kbps: 64,
      auto_save_system_audio: false,
      auto_save_mic_audio: false,
      continuous_dump_enabled: true,
      continuous_dump_profile: "balanced".to_string(),
      continuous_soft_flush_ms: 10_000,
      continuous_silence_flush_ms: 1_200,
      continuous_hard_cut_ms: 45_000,
      continuous_min_chunk_ms: 1_000,
      continuous_pre_roll_ms: 300,
      continuous_post_roll_ms: 200,
      continuous_idle_keepalive_ms: 60_000,
      continuous_mic_override_enabled: false,
      continuous_mic_soft_flush_ms: 10_000,
      continuous_mic_silence_flush_ms: 1_200,
      continuous_mic_hard_cut_ms: 45_000,
      continuous_system_override_enabled: false,
      continuous_system_soft_flush_ms: 10_000,
      continuous_system_silence_flush_ms: 1_200,
      continuous_system_hard_cut_ms: 45_000,
      transcribe_backend: "whisper_cpp".to_string(),
      session_idle_timeout_ms: 60_000,       // 60 seconds
      ptt_session_grouping_enabled: true,
      ptt_session_group_timeout_s: 120,      // 2 minutes
      main_window_x: None,
      main_window_y: None,
      main_window_width: None,
      main_window_height: None,
      main_window_monitor: None,
      main_window_start_state: "normal".to_string(),
    }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct HistoryRefinement {
    pub(crate) job_id: String,
    pub(crate) raw: String,
    pub(crate) refined: String,
    pub(crate) status: String, // "idle" | "refining" | "refined" | "error"
    pub(crate) model: String,
    pub(crate) execution_time_ms: Option<u64>,
    pub(crate) error: String,
}

impl Default for HistoryRefinement {
    fn default() -> Self {
        Self {
            job_id: String::new(),
            raw: String::new(),
            refined: String::new(),
            status: "idle".to_string(),
            model: String::new(),
            execution_time_ms: None,
            error: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HistoryEntry {
    pub(crate) id: String,
    pub(crate) text: String,
    pub(crate) timestamp_ms: u64,
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) refinement: Option<HistoryRefinement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Chapter {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) timestamp_ms: u64,
    pub(crate) entry_count: u32,
}

pub(crate) struct AppState {
    pub(crate) settings: Mutex<Settings>,
    pub(crate) history: Mutex<Vec<HistoryEntry>>,
    pub(crate) history_transcribe: Mutex<Vec<HistoryEntry>>,
    pub(crate) chapters: Mutex<Vec<Chapter>>,
    pub(crate) recorder: Mutex<Recorder>,
    pub(crate) transcribe: Mutex<TranscribeRecorder>,
    pub(crate) downloads: Mutex<HashSet<String>>,
    pub(crate) ollama_pulls: Mutex<HashSet<String>>,
    pub(crate) transcribe_active: AtomicBool,
    pub(crate) refinement_active_count: AtomicUsize,
    pub(crate) refinement_watchdog_generation: AtomicU64,
    pub(crate) refinement_last_change_ms: AtomicU64,
    pub(crate) runtime_start_attempts: AtomicU64,
    pub(crate) runtime_start_failures: AtomicU64,
    pub(crate) refinement_timeouts: AtomicU64,
    pub(crate) refinement_fallback_failed: AtomicU64,
    pub(crate) refinement_fallback_timed_out: AtomicU64,
    /// Last recorded OPUS file path for mic input.
    pub(crate) last_mic_recording_path: Mutex<Option<String>>,
    /// Last recorded OPUS file path for system audio.
    pub(crate) last_system_recording_path: Mutex<Option<String>>,
    /// Handle to the managed Ollama child process for cleanup on app exit.
    pub(crate) managed_ollama_child: Mutex<Option<std::process::Child>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeMetricsSnapshot {
    pub(crate) runtime_start_attempts: u64,
    pub(crate) runtime_start_failures: u64,
    pub(crate) refinement_timeouts: u64,
    pub(crate) refinement_fallback_failed: u64,
    pub(crate) refinement_fallback_timed_out: u64,
}

pub(crate) fn record_runtime_start_attempt(state: &AppState) {
    state.runtime_start_attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn record_runtime_start_failure(state: &AppState) {
    state.runtime_start_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn record_refinement_timeout(state: &AppState) {
    state.refinement_timeouts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn record_refinement_fallback_failed(state: &AppState) {
    state
        .refinement_fallback_failed
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn record_refinement_fallback_timed_out(state: &AppState) {
    state
        .refinement_fallback_timed_out
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn get_runtime_metrics_snapshot(state: &AppState) -> RuntimeMetricsSnapshot {
    RuntimeMetricsSnapshot {
        runtime_start_attempts: state
            .runtime_start_attempts
            .load(std::sync::atomic::Ordering::Relaxed),
        runtime_start_failures: state
            .runtime_start_failures
            .load(std::sync::atomic::Ordering::Relaxed),
        refinement_timeouts: state
            .refinement_timeouts
            .load(std::sync::atomic::Ordering::Relaxed),
        refinement_fallback_failed: state
            .refinement_fallback_failed
            .load(std::sync::atomic::Ordering::Relaxed),
        refinement_fallback_timed_out: state
            .refinement_fallback_timed_out
            .load(std::sync::atomic::Ordering::Relaxed),
    }
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
            normalize_continuous_dump_fields(&mut settings);
            if settings.transcribe_backend.trim().is_empty() {
                settings.transcribe_backend = "whisper_cpp".to_string();
            }
            if settings.transcribe_backend != "whisper_cpp" {
                settings.transcribe_backend = "whisper_cpp".to_string();
            }
            // Validate language_mode
            let valid_languages = [
                "auto", "en", "de", "fr", "es", "it", "pt", "nl", "pl", "ru", "ja", "ko", "zh",
                "ar", "tr", "hi",
            ];
            if !valid_languages.contains(&settings.language_mode.as_str()) {
                settings.language_mode = "auto".to_string();
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
            settings.transcribe_input_gain_db =
                settings.transcribe_input_gain_db.clamp(-30.0, 30.0);
            settings.mic_input_gain_db = settings.mic_input_gain_db.clamp(-30.0, 30.0);
            #[cfg(target_os = "windows")]
            if settings.transcribe_output_device != "default"
                && !settings.transcribe_output_device.starts_with("wasapi:")
            {
                settings.transcribe_output_device = "default".to_string();
            }
            if !settings.overlay_min_radius.is_finite() {
                settings.overlay_min_radius = 16.0;
            }
            if !settings.overlay_max_radius.is_finite() {
                settings.overlay_max_radius = 64.0;
            }
            // Keep dot dimensions in sane bounds; monitor-relative 50% cap is
            // applied at runtime in overlay.rs.
            settings.overlay_min_radius = settings.overlay_min_radius.clamp(4.0, 5_000.0);
            settings.overlay_max_radius = settings.overlay_max_radius.clamp(8.0, 10_000.0);
            if settings.overlay_max_radius < settings.overlay_min_radius {
                settings.overlay_max_radius = settings.overlay_min_radius;
            }
            if settings.overlay_rise_ms < 20 {
                settings.overlay_rise_ms = 20;
            }
            if settings.overlay_rise_ms > 200 {
                settings.overlay_rise_ms = 200;
            }
            if settings.overlay_fall_ms < 20 {
                settings.overlay_fall_ms = 20;
            }
            if settings.overlay_fall_ms > 200 {
                settings.overlay_fall_ms = 200;
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
            if approx_eq(
                settings.overlay_kitt_opacity_inactive,
                defaults.overlay_kitt_opacity_inactive,
            ) && !approx_eq(
                settings.overlay_opacity_inactive,
                defaults.overlay_opacity_inactive,
            ) {
                settings.overlay_kitt_opacity_inactive = settings.overlay_opacity_inactive;
            }
            if approx_eq(
                settings.overlay_kitt_opacity_active,
                defaults.overlay_kitt_opacity_active,
            ) && !approx_eq(
                settings.overlay_opacity_active,
                defaults.overlay_opacity_active,
            ) {
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
            if !settings.overlay_kitt_min_width.is_finite() {
                settings.overlay_kitt_min_width = 20.0;
            }
            if !settings.overlay_kitt_max_width.is_finite() {
                settings.overlay_kitt_max_width = 700.0;
            }
            if !settings.overlay_kitt_height.is_finite() {
                settings.overlay_kitt_height = 13.0;
            }

            // Keep KITT dimensions in sane bounds; monitor-relative 50% cap is
            // applied at runtime in overlay.rs.
            settings.overlay_kitt_min_width = settings.overlay_kitt_min_width.clamp(4.0, 10_000.0);
            settings.overlay_kitt_max_width = settings.overlay_kitt_max_width.clamp(50.0, 20_000.0);
            if settings.overlay_kitt_max_width < settings.overlay_kitt_min_width {
                settings.overlay_kitt_max_width = settings.overlay_kitt_min_width.max(50.0);
            }
            settings.overlay_kitt_height = settings.overlay_kitt_height.clamp(8.0, 400.0);
            if settings.overlay_kitt_rise_ms < 20 {
                settings.overlay_kitt_rise_ms = 20;
            }
            if settings.overlay_kitt_rise_ms > 200 {
                settings.overlay_kitt_rise_ms = 200;
            }
            if settings.overlay_kitt_fall_ms < 20 {
                settings.overlay_kitt_fall_ms = 20;
            }
            if settings.overlay_kitt_fall_ms > 200 {
                settings.overlay_kitt_fall_ms = 200;
            }
            if !["subtle", "standard", "intense"]
                .contains(&settings.overlay_refining_indicator_preset.as_str())
            {
                settings.overlay_refining_indicator_preset = "standard".to_string();
            }
            if !settings.overlay_refining_indicator_color.starts_with('#')
                || settings.overlay_refining_indicator_color.len() != 7
            {
                settings.overlay_refining_indicator_color = "#6ec8ff".to_string();
            }
            if !settings.accent_color.starts_with('#') || settings.accent_color.len() != 7 {
                settings.accent_color = "#4be0d4".to_string();
            }
            if settings.overlay_refining_indicator_speed_ms < 450 {
                settings.overlay_refining_indicator_speed_ms = 450;
            }
            if settings.overlay_refining_indicator_speed_ms > 3_000 {
                settings.overlay_refining_indicator_speed_ms = 3_000;
            }
            if settings.overlay_refining_indicator_range < 60.0 {
                settings.overlay_refining_indicator_range = 60.0;
            }
            if settings.overlay_refining_indicator_range > 180.0 {
                settings.overlay_refining_indicator_range = 180.0;
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
            // Validate main_window_start_state
            if !["normal", "minimized", "tray"].contains(&settings.main_window_start_state.as_str())
            {
                settings.main_window_start_state = "normal".to_string();
            }
            // Normalize v0.7 AI fallback settings and legacy compatibility fields.
            normalize_ai_fallback_fields(&mut settings);
            if settings.setup.local_ai_wizard_completed {
                settings.setup.local_ai_wizard_pending = false;
            }

            // Transcribe enablement is session-only; always start disabled.
            settings.transcribe_enabled = false;
            settings
        }
        Err(_) => Settings::default(),
    }
}

pub(crate) fn normalize_ai_fallback_fields(settings: &mut Settings) {
    fn normalize_cloud_provider(provider: &str) -> Option<String> {
        match provider.trim().to_lowercase().as_str() {
            "claude" => Some("claude".to_string()),
            "openai" => Some("openai".to_string()),
            "gemini" => Some("gemini".to_string()),
            _ => None,
        }
    }

    fn is_verified_auth_status(status: &str) -> bool {
        matches!(
            status.trim(),
            "verified_api_key" | "verified_oauth"
        )
    }

    // Migrate legacy postproc_llm toggle and values if still used.
    if settings.postproc_llm_enabled && !settings.ai_fallback.enabled {
        settings.ai_fallback.enabled = true;
    }
    if settings.ai_fallback.provider.trim().is_empty()
        && !settings.postproc_llm_provider.trim().is_empty()
    {
        settings.ai_fallback.provider = settings.postproc_llm_provider.clone();
    }
    if settings.ai_fallback.model.trim().is_empty()
        && !settings.postproc_llm_model.trim().is_empty()
    {
        settings.ai_fallback.model = settings.postproc_llm_model.clone();
    }
    if settings.ai_fallback.custom_prompt.trim().is_empty()
        && !settings.postproc_llm_prompt.trim().is_empty()
    {
        settings.ai_fallback.custom_prompt = settings.postproc_llm_prompt.clone();
        settings.ai_fallback.custom_prompt_enabled = true;
        settings.ai_fallback.use_default_prompt = false;
    }
    if settings.ai_fallback.prompt_profile.trim().is_empty() {
        settings.ai_fallback.prompt_profile = if settings.ai_fallback.custom_prompt_enabled
            && !settings.ai_fallback.use_default_prompt
        {
            "custom".to_string()
        } else {
            "wording".to_string()
        };
    }

    // Preserve legacy cloud provider selection as optional fallback candidate.
    if settings.ai_fallback.fallback_provider.is_none()
        && settings.ai_fallback.provider != "ollama"
    {
        settings.ai_fallback.fallback_provider =
            normalize_cloud_provider(&settings.ai_fallback.provider);
    }

    settings.ai_fallback.normalize();
    settings.ai_fallback.custom_prompt_enabled = settings.ai_fallback.prompt_profile == "custom";
    settings.ai_fallback.use_default_prompt = false;
    settings.providers.normalize();

    // Migration rule: cloud providers remain locked until explicit verify succeeds.
    for provider in ["claude", "openai", "gemini"] {
        if let Some(config) = settings.providers.get_mut(provider) {
            if !is_verified_auth_status(&config.auth_status) {
                config.auth_status = "locked".to_string();
                config.auth_verified_at = None;
            }
            if !config.api_key_stored && config.auth_status != "verified_oauth" {
                config.auth_status = "locked".to_string();
                config.auth_verified_at = None;
            }
        }
    }

    settings.ai_fallback.execution_mode = if settings.ai_fallback.execution_mode == "online_fallback"
    {
        "online_fallback".to_string()
    } else {
        "local_primary".to_string()
    };

    settings.ai_fallback.fallback_provider = settings
        .ai_fallback
        .fallback_provider
        .as_ref()
        .and_then(|provider| normalize_cloud_provider(provider));

    if settings.ai_fallback.execution_mode == "online_fallback" {
        let verified = settings
            .ai_fallback
            .fallback_provider
            .as_ref()
            .map(|provider| settings.providers.is_verified(provider))
            .unwrap_or(false);

        if verified {
            if let Some(provider) = settings.ai_fallback.fallback_provider.clone() {
                settings.ai_fallback.provider = provider;
            } else {
                settings.ai_fallback.execution_mode = "local_primary".to_string();
                settings.ai_fallback.provider = "ollama".to_string();
            }
        } else {
            settings.ai_fallback.execution_mode = "local_primary".to_string();
            settings.ai_fallback.provider = "ollama".to_string();
        }
    } else {
        settings.ai_fallback.provider = "ollama".to_string();
    }

    if settings.ai_fallback.provider == "ollama"
        && settings.ai_fallback.strict_local_mode
        && !is_local_ollama_endpoint(&settings.providers.ollama.endpoint)
    {
        settings.providers.ollama.endpoint = "http://localhost:11434".to_string();
    }
    settings
        .providers
        .sync_from_ai_fallback(&settings.ai_fallback);

    // Keep legacy post-processing fields synchronized for compatibility with older code paths.
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    settings.postproc_llm_provider = settings.ai_fallback.provider.clone();
    settings.postproc_llm_model = settings.ai_fallback.model.clone();
    if let Some(prompt) = prompt_for_profile(
        &settings.ai_fallback.prompt_profile,
        &settings.language_mode,
        Some(settings.ai_fallback.custom_prompt.as_str()),
    ) {
        settings.postproc_llm_prompt = prompt;
    }
    if settings.setup.local_ai_wizard_completed {
        settings.setup.local_ai_wizard_pending = false;
    }
}

pub(crate) fn normalize_continuous_dump_fields(settings: &mut Settings) {
    let defaults = Settings::default();

    if settings.continuous_dump_profile != "balanced"
        && settings.continuous_dump_profile != "low_latency"
        && settings.continuous_dump_profile != "high_quality"
    {
        settings.continuous_dump_profile = "balanced".to_string();
    }

    // Legacy migration from interval/overlap system-audio settings.
    if settings.continuous_soft_flush_ms == defaults.continuous_soft_flush_ms
        && settings.transcribe_batch_interval_ms != defaults.transcribe_batch_interval_ms
    {
        settings.continuous_soft_flush_ms = settings.transcribe_batch_interval_ms;
    }
    if settings.continuous_system_soft_flush_ms == defaults.continuous_system_soft_flush_ms
        && settings.transcribe_batch_interval_ms != defaults.transcribe_batch_interval_ms
    {
        settings.continuous_system_soft_flush_ms = settings.transcribe_batch_interval_ms;
    }
    if settings.continuous_silence_flush_ms == defaults.continuous_silence_flush_ms
        && settings.transcribe_vad_silence_ms != defaults.transcribe_vad_silence_ms
    {
        settings.continuous_silence_flush_ms = settings.transcribe_vad_silence_ms;
    }
    if settings.continuous_system_silence_flush_ms == defaults.continuous_system_silence_flush_ms
        && settings.transcribe_vad_silence_ms != defaults.transcribe_vad_silence_ms
    {
        settings.continuous_system_silence_flush_ms = settings.transcribe_vad_silence_ms;
    }
    if settings.continuous_pre_roll_ms == defaults.continuous_pre_roll_ms
        && settings.transcribe_chunk_overlap_ms != defaults.transcribe_chunk_overlap_ms
    {
        settings.continuous_pre_roll_ms = settings.transcribe_chunk_overlap_ms;
    }

    settings.continuous_soft_flush_ms = settings.continuous_soft_flush_ms.clamp(4_000, 30_000);
    settings.continuous_silence_flush_ms = settings.continuous_silence_flush_ms.clamp(300, 5_000);
    settings.continuous_hard_cut_ms = settings.continuous_hard_cut_ms.clamp(15_000, 120_000);
    settings.continuous_min_chunk_ms = settings.continuous_min_chunk_ms.clamp(250, 5_000);
    settings.continuous_pre_roll_ms = settings.continuous_pre_roll_ms.clamp(0, 1_500);
    settings.continuous_post_roll_ms = settings.continuous_post_roll_ms.clamp(0, 1_500);
    settings.continuous_idle_keepalive_ms =
        settings.continuous_idle_keepalive_ms.clamp(10_000, 120_000);

    settings.continuous_mic_soft_flush_ms =
        settings.continuous_mic_soft_flush_ms.clamp(4_000, 30_000);
    settings.continuous_mic_silence_flush_ms =
        settings.continuous_mic_silence_flush_ms.clamp(300, 5_000);
    settings.continuous_mic_hard_cut_ms =
        settings.continuous_mic_hard_cut_ms.clamp(15_000, 120_000);

    settings.continuous_system_soft_flush_ms = settings
        .continuous_system_soft_flush_ms
        .clamp(4_000, 30_000);
    settings.continuous_system_silence_flush_ms = settings
        .continuous_system_silence_flush_ms
        .clamp(300, 5_000);
    settings.continuous_system_hard_cut_ms = settings
        .continuous_system_hard_cut_ms
        .clamp(15_000, 120_000);

    // Keep legacy controls in sync while old UI / settings still exist.
    let legacy_soft = if settings.continuous_system_override_enabled {
        settings.continuous_system_soft_flush_ms
    } else {
        settings.continuous_soft_flush_ms
    };
    let legacy_silence = if settings.continuous_system_override_enabled {
        settings.continuous_system_silence_flush_ms
    } else {
        settings.continuous_silence_flush_ms
    };
    settings.transcribe_batch_interval_ms = legacy_soft.clamp(4_000, 15_000);
    settings.transcribe_vad_silence_ms = legacy_silence.clamp(200, 5_000);
    settings.transcribe_chunk_overlap_ms = settings.continuous_pre_roll_ms.min(3_000);
    if settings.transcribe_chunk_overlap_ms > settings.transcribe_batch_interval_ms {
        settings.transcribe_chunk_overlap_ms = settings.transcribe_batch_interval_ms / 2;
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
    load_history_from_path(&path)
}

pub(crate) fn save_history_file(app: &AppHandle, history: &[HistoryEntry]) -> Result<(), String> {
    let path = resolve_data_path(app, "history.json");
    save_history_to_path(&path, history)
}

pub(crate) fn load_chapters(app: &AppHandle) -> Vec<Chapter> {
    let path = resolve_data_path(app, "chapters.json");
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn save_chapters_file(app: &AppHandle, chapters: &[Chapter]) -> Result<(), String> {
    let path = resolve_data_path(app, "chapters.json");
    let raw = serde_json::to_string_pretty(chapters).map_err(|e| e.to_string())?;
    fs::write(path, raw).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn load_transcribe_history(app: &AppHandle) -> Vec<HistoryEntry> {
    let path = resolve_data_path(app, "history_transcribe.json");
    load_history_from_path(&path)
}

pub(crate) fn save_transcribe_history_file(
    app: &AppHandle,
    history: &[HistoryEntry],
) -> Result<(), String> {
    let path = resolve_data_path(app, "history_transcribe.json");
    save_history_to_path(&path, history)
}

pub(crate) fn load_history_from_path(path: &std::path::Path) -> Vec<HistoryEntry> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn save_history_to_path(
    path: &std::path::Path,
    history: &[HistoryEntry],
) -> Result<(), String> {
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
    let lock_started = Instant::now();
    let mut history = history.lock().unwrap();
    let entry = HistoryEntry {
        id: format!("h_{}", crate::util::now_ms()),
        text,
        timestamp_ms: crate::util::now_ms(),
        source,
        refinement: None,
    };
    history.insert(0, entry);
    let updated = history.clone();
    let lock_elapsed_ms = lock_started.elapsed().as_millis();
    drop(history);
    if lock_elapsed_ms > HISTORY_LOCK_WARN_MS {
        warn!(
            "History lock hold exceeded threshold in push_history_entry_inner: {}ms",
            lock_elapsed_ms
        );
    }
    save_history_file(app, &updated)?;
    Ok(updated)
}

pub(crate) fn push_transcribe_entry_inner(
    app: &AppHandle,
    history: &Mutex<Vec<HistoryEntry>>,
    text: String,
) -> Result<Vec<HistoryEntry>, String> {
    let lock_started = Instant::now();
    let mut history = history.lock().unwrap();
    let entry = HistoryEntry {
        id: format!("o_{}", crate::util::now_ms()),
        text,
        timestamp_ms: crate::util::now_ms(),
        source: "output".to_string(),
        refinement: None,
    };
    history.insert(0, entry);
    let updated = history.clone();
    let lock_elapsed_ms = lock_started.elapsed().as_millis();
    drop(history);
    if lock_elapsed_ms > HISTORY_LOCK_WARN_MS {
        warn!(
            "History lock hold exceeded threshold in push_transcribe_entry_inner: {}ms",
            lock_elapsed_ms
        );
    }
    save_transcribe_history_file(app, &updated)?;
    let _ = app.emit("transcribe:history-updated", updated.clone());
    Ok(updated)
}

fn emit_updated_history(app: &AppHandle, event_name: &str, updated: Vec<HistoryEntry>) {
    let _ = app.emit(event_name, updated);
}

fn update_history_entry_in_store<F>(
    app: &AppHandle,
    store: &Mutex<Vec<HistoryEntry>>,
    save_fn: fn(&AppHandle, &[HistoryEntry]) -> Result<(), String>,
    event_name: &str,
    entry_id: &str,
    apply: &mut F,
) -> Result<bool, String>
where
    F: FnMut(&mut HistoryEntry),
{
    let lock_started = Instant::now();
    let mut history = store.lock().unwrap();
    let Some(entry) = history.iter_mut().find(|entry| entry.id == entry_id) else {
        return Ok(false);
    };
    apply(entry);
    let updated = history.clone();
    let lock_elapsed_ms = lock_started.elapsed().as_millis();
    drop(history);
    if lock_elapsed_ms > HISTORY_LOCK_WARN_MS {
        warn!(
            "History lock hold exceeded threshold in update_history_entry_in_store ({}): {}ms",
            event_name, lock_elapsed_ms
        );
    }
    save_fn(app, &updated)?;
    emit_updated_history(app, event_name, updated);
    Ok(true)
}

fn update_history_entry_refinement<F>(
    app: &AppHandle,
    entry_id: &str,
    mut apply: F,
) -> Result<(), String>
where
    F: FnMut(&mut HistoryEntry),
{
    if entry_id.trim().is_empty() {
        return Ok(());
    }
    let state = app.state::<AppState>();
    if update_history_entry_in_store(
        app,
        &state.history,
        save_history_file,
        "history:updated",
        entry_id,
        &mut apply,
    )? {
        return Ok(());
    }
    let _ = update_history_entry_in_store(
        app,
        &state.history_transcribe,
        save_transcribe_history_file,
        "transcribe:history-updated",
        entry_id,
        &mut apply,
    )?;
    Ok(())
}

fn ensure_history_refinement(entry: &mut HistoryEntry) -> &mut HistoryRefinement {
    if entry.refinement.is_none() {
        entry.refinement = Some(HistoryRefinement::default());
    }
    entry.refinement.as_mut().expect("refinement just initialized")
}

pub(crate) fn mark_entry_refinement_started(
    app: &AppHandle,
    entry_id: &str,
    job_id: &str,
    raw_text: &str,
) -> Result<(), String> {
    update_history_entry_refinement(app, entry_id, |entry| {
        let fallback_raw = entry.text.clone();
        let refinement = ensure_history_refinement(entry);
        if !job_id.trim().is_empty() {
            refinement.job_id = job_id.to_string();
        }
        if !raw_text.trim().is_empty() {
            refinement.raw = raw_text.to_string();
        } else if refinement.raw.trim().is_empty() {
            refinement.raw = fallback_raw;
        }
        refinement.status = "refining".to_string();
        refinement.error.clear();
    })
}

pub(crate) fn mark_entry_refinement_success(
    app: &AppHandle,
    entry_id: &str,
    job_id: &str,
    raw_text: &str,
    refined_text: &str,
    model: &str,
    execution_time_ms: u64,
) -> Result<(), String> {
    update_history_entry_refinement(app, entry_id, |entry| {
        let fallback_raw = entry.text.clone();
        let refinement = ensure_history_refinement(entry);
        if !job_id.trim().is_empty() {
            refinement.job_id = job_id.to_string();
        }
        if !raw_text.trim().is_empty() {
            refinement.raw = raw_text.to_string();
        } else if refinement.raw.trim().is_empty() {
            refinement.raw = fallback_raw;
        }
        refinement.refined = refined_text.to_string();
        refinement.status = "refined".to_string();
        refinement.model = model.to_string();
        refinement.execution_time_ms = Some(execution_time_ms);
        refinement.error.clear();
    })
}

pub(crate) fn mark_entry_refinement_failed(
    app: &AppHandle,
    entry_id: &str,
    job_id: &str,
    raw_text: &str,
    error_text: &str,
) -> Result<(), String> {
    update_history_entry_refinement(app, entry_id, |entry| {
        let fallback_raw = entry.text.clone();
        let refinement = ensure_history_refinement(entry);
        if !job_id.trim().is_empty() {
            refinement.job_id = job_id.to_string();
        }
        if !raw_text.trim().is_empty() {
            refinement.raw = raw_text.to_string();
        } else if refinement.raw.trim().is_empty() {
            refinement.raw = fallback_raw;
        }
        refinement.status = "error".to_string();
        refinement.error = error_text.to_string();
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_history_entry(id: &str, status: &str, refined: &str, error: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            text: "raw transcript".to_string(),
            timestamp_ms: 1_772_101_100_000,
            source: "local".to_string(),
            refinement: Some(HistoryRefinement {
                job_id: format!("job-{id}"),
                raw: "raw transcript".to_string(),
                refined: refined.to_string(),
                status: status.to_string(),
                model: "qwen3:14b".to_string(),
                execution_time_ms: Some(1234),
                error: error.to_string(),
            }),
        }
    }

    #[test]
    fn history_roundtrip_persists_refinement_states() {
        let temp_path = std::env::temp_dir().join(format!(
            "trispr_history_roundtrip_{}_{}.json",
            std::process::id(),
            crate::util::now_ms()
        ));

        let original = vec![
            sample_history_entry("a", "refined", "refined transcript", ""),
            sample_history_entry("b", "error", "", "network timeout"),
            sample_history_entry("c", "refining", "", ""),
        ];

        save_history_to_path(&temp_path, &original).expect("save history");
        let restored = load_history_from_path(&temp_path);
        let _ = fs::remove_file(&temp_path);

        assert_eq!(restored.len(), original.len());
        assert_eq!(
            restored[0].refinement.as_ref().map(|r| r.status.as_str()),
            Some("refined")
        );
        assert_eq!(
            restored[0].refinement.as_ref().map(|r| r.refined.as_str()),
            Some("refined transcript")
        );
        assert_eq!(
            restored[1].refinement.as_ref().map(|r| r.status.as_str()),
            Some("error")
        );
        assert_eq!(
            restored[1].refinement.as_ref().map(|r| r.error.as_str()),
            Some("network timeout")
        );
        assert_eq!(
            restored[2].refinement.as_ref().map(|r| r.status.as_str()),
            Some("refining")
        );
    }
}
