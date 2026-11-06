use crate::ai_fallback::models::{AIFallbackSettings, AIProvidersSettings};
use crate::ai_fallback::provider::prompt_for_profile;
use crate::audio::Recorder;
use crate::constants::{
    HALLUCINATION_MAX_CHARS, HALLUCINATION_MAX_DURATION_MS, HALLUCINATION_MAX_WORDS,
    HALLUCINATION_RMS_THRESHOLD, VAD_SILENCE_MS_DEFAULT, VAD_THRESHOLD_START_DEFAULT,
    VAD_THRESHOLD_SUSTAIN_DEFAULT,
};
use crate::history_partition::PartitionedHistory;
use crate::overlay::OverlayController;
use crate::paths::resolve_config_path;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

const HISTORY_LOCK_WARN_MS: u128 = 20;

/// Debounce flags for asynchronous history persistence.  When set, a background
/// thread is already scheduled to flush the corresponding history to disk within
/// 200 ms — additional writes during that window are coalesced.
static HISTORY_SAVE_PENDING: AtomicBool = AtomicBool::new(false);
static DIAGNOSTIC_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

pub(crate) fn diagnostic_logging_enabled() -> bool {
    DIAGNOSTIC_LOGGING_ENABLED.load(Ordering::Relaxed)
}

pub(crate) fn sync_diagnostic_logging_enabled(settings: &Settings) {
    DIAGNOSTIC_LOGGING_ENABLED.store(settings.diagnostic_logging_enabled, Ordering::Relaxed);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct SetupSettings {
    pub(crate) local_ai_wizard_completed: bool,
    pub(crate) local_ai_wizard_pending: bool,
}

impl Default for SetupSettings {
    fn default() -> Self {
        Self {
            local_ai_wizard_completed: false,
            local_ai_wizard_pending: true,
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

fn default_history_alias_mic() -> String {
    "Input".to_string()
}

fn default_topic_keywords() -> HashMap<String, Vec<String>> {
    let mut topics: HashMap<String, Vec<String>> = HashMap::new();
    topics.insert(
        "technical".to_string(),
        vec![
            "code",
            "coding",
            "debug",
            "debugging",
            "bug",
            "error",
            "stacktrace",
            "exception",
            "function",
            "variable",
            "api",
            "endpoint",
            "database",
            "sql",
            "query",
            "schema",
            "deploy",
            "deployment",
            "build",
            "compile",
            "performance",
            "latency",
            "memory",
            "thread",
            "integration",
            "schnittstelle",
            "fehler",
            "datenbank",
            "abfrage",
            "bereitstellung",
            "leistung",
            "speicher",
            "konfiguration",
            "version",
            "docker",
            "kubernetes",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    );
    topics.insert(
        "meeting".to_string(),
        vec![
            "meeting",
            "agenda",
            "minutes",
            "action",
            "action item",
            "deadline",
            "owner",
            "follow-up",
            "stakeholder",
            "alignment",
            "decision",
            "next step",
            "roadmap",
            "priority",
            "milestone",
            "planning",
            "sync",
            "standup",
            "retrospective",
            "workshop",
            "besprechung",
            "termin",
            "protokoll",
            "entscheidung",
            "naechster schritt",
            "prioritaet",
            "meilenstein",
            "planung",
            "abstimmung",
            "aufgabe",
            "verantwortlich",
            "rueckmeldung",
            "review",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    );
    topics.insert(
        "personal".to_string(),
        vec![
            "personal",
            "note",
            "reminder",
            "todo",
            "to-do",
            "follow-up",
            "errand",
            "appointment",
            "family",
            "health",
            "habit",
            "journal",
            "private",
            "vacation",
            "shopping",
            "budget",
            "finance",
            "bank",
            "insurance",
            "medicine",
            "persoenlich",
            "erinnerung",
            "notiz",
            "einkauf",
            "urlaub",
            "arzt",
            "haushalt",
            "konto",
            "rechnung",
            "gesundheit",
            "routine",
            "privat",
            "aufraeumen",
        ]
        .into_iter()
        .map(str::to_string)
        .collect(),
    );
    topics
}

fn derive_postproc_language_from_asr(language_mode: &str, language_pinned: bool) -> String {
    if !language_pinned {
        return "multi".to_string();
    }
    match language_mode.trim().to_lowercase().as_str() {
        "en" => "en".to_string(),
        "de" => "de".to_string(),
        _ => "multi".to_string(),
    }
}

fn effective_asr_language_hint(language_mode: &str, language_pinned: bool) -> String {
    if language_pinned {
        language_mode.trim().to_lowercase()
    } else {
        "auto".to_string()
    }
}

fn default_whisper_gpu_layers() -> Option<usize> {
    Some(35)
}

fn default_local_backend_preference() -> String {
    "auto".to_string()
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
    // v0.7.0 AI Fallback settings
    pub(crate) ai_fallback: AIFallbackSettings,
    pub(crate) providers: AIProvidersSettings,
    // First-run setup flags
    pub(crate) setup: SetupSettings,
    pub(crate) audio_cues: bool,
    pub(crate) audio_cues_volume: f32,
    #[serde(default)]
    pub(crate) diagnostic_logging_enabled: bool,
    pub(crate) ptt_use_vad: bool, // Enable VAD threshold check even in PTT mode
    pub(crate) ptt_hot_keepalive_ms: u64, // Warm standby window after PTT release
    pub(crate) vad_threshold: f32, // Legacy: now maps to vad_threshold_start
    pub(crate) vad_threshold_start: f32,
    pub(crate) vad_threshold_sustain: f32,
    pub(crate) vad_silence_ms: u64,
    pub(crate) hotkey_toggle_activation_words: String,
    pub(crate) mic_input_gain_db: f32,
    #[serde(default = "default_history_alias_mic")]
    pub(crate) history_alias_mic: String,
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
    #[serde(default = "default_topic_keywords")]
    pub(crate) topic_keywords: HashMap<String, Vec<String>>,
    // Post-processing settings
    pub(crate) postproc_enabled: bool,
    pub(crate) postproc_language: String,
    pub(crate) postproc_punctuation_enabled: bool,
    pub(crate) postproc_capitalization_enabled: bool,
    pub(crate) postproc_numbers_enabled: bool,
    pub(crate) postproc_custom_vocab_enabled: bool,
    pub(crate) postproc_custom_vocab: HashMap<String, String>,
    /// Learned proper nouns, acronyms, and project-specific terms (targets of corrections the
    /// user repeated). Sent as Whisper initial prompt (CLI `--prompt` and server `prompt` field)
    /// and to the LLM refinement prompt (see `vocab_hints`).
    #[serde(default)]
    pub(crate) vocab_terms: Vec<String>,
    /// Substitution pairs observed from user edits: pending ones accumulate toward promotion,
    /// promoted ones stay (with count/recency) so the hints can be ranked.
    #[serde(default)]
    pub(crate) edit_substitutions: Vec<EditSubstitution>,
    /// Marker for the one-time edit-delta migration (sets the flag only, deletes nothing).
    #[serde(default)]
    pub(crate) edit_delta_migrated: bool,
    pub(crate) postproc_llm_enabled: bool,
    pub(crate) postproc_llm_model: String,
    pub(crate) postproc_llm_prompt: String,
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
    pub(crate) transcribe_backend: String, // "whisper_cpp" | future backends
    #[serde(default = "default_local_backend_preference")]
    pub(crate) local_backend_preference: String, // "auto" | "cuda" | "vulkan"
    // Main window state
    pub(crate) main_window_x: Option<i32>,
    pub(crate) main_window_y: Option<i32>,
    pub(crate) main_window_width: Option<u32>,
    pub(crate) main_window_height: Option<u32>,
    pub(crate) main_window_monitor: Option<String>,
    /// Window visibility state at shutdown: "normal" or "minimized" (legacy "tray" maps to "normal")
    pub(crate) main_window_start_state: String,
    /// GPU layers for Whisper (CUDA acceleration). Default: 35
    #[serde(default = "default_whisper_gpu_layers")]
    pub(crate) whisper_gpu_layers: Option<usize>,
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
      model: "whisper-large-v3-turbo".to_string(),
      ai_fallback: AIFallbackSettings::default(),
      providers: AIProvidersSettings::default(),
      setup: SetupSettings::default(),
      audio_cues: true,
      audio_cues_volume: 0.3,
      diagnostic_logging_enabled: false,
      ptt_use_vad: false,
      ptt_hot_keepalive_ms: 600_000,
      vad_threshold: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_start: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_sustain: VAD_THRESHOLD_SUSTAIN_DEFAULT,
      vad_silence_ms: VAD_SILENCE_MS_DEFAULT,
      hotkey_toggle_activation_words: "CommandOrControl+Shift+A".to_string(),
      mic_input_gain_db: 0.0,
      history_alias_mic: default_history_alias_mic(),
      capture_enabled: true,
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
      topic_keywords: default_topic_keywords(),
      postproc_enabled: false,
      postproc_language: "multi".to_string(),
      postproc_punctuation_enabled: true,
      postproc_capitalization_enabled: true,
      postproc_numbers_enabled: true,
      postproc_custom_vocab_enabled: false,
      postproc_custom_vocab: HashMap::new(),
      vocab_terms: Vec::new(),
      edit_substitutions: Vec::new(),
      edit_delta_migrated: false,
      postproc_llm_enabled: false,
      postproc_llm_model: String::new(),
      postproc_llm_prompt: "Refine this voice transcription: fix punctuation, capitalization, and obvious errors. Keep the original meaning. Output only the refined text.".to_string(),
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
      transcribe_backend: "whisper_cpp".to_string(),
      local_backend_preference: default_local_backend_preference(),
      main_window_x: None,
      main_window_y: None,
      main_window_width: None,
      main_window_height: None,
      main_window_monitor: None,
      main_window_start_state: "normal".to_string(),
      whisper_gpu_layers: default_whisper_gpu_layers(),
    }
    }
}

/// A correction the user made by editing the pasted refinement output before submitting.
/// Mirrors `src/types.ts::EditSubstitution`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EditSubstitution {
    /// Original token as it appeared in the refinement output.
    pub(crate) from: String,
    /// What the user typed instead.
    pub(crate) to: String,
    /// Number of separate edits in which this correction has been observed.
    pub(crate) count: u32,
    pub(crate) first_seen_ms: u64,
    pub(crate) last_seen_ms: u64,
    /// True once the correction reached the threshold and is in use (Whisper/LLM hints,
    /// optionally a find-replace rule). Missing in older settings files = still pending.
    #[serde(default)]
    pub(crate) promoted: bool,
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
    pub(crate) speaker_name: Option<String>,
    #[serde(default)]
    pub(crate) refinement: Option<HistoryRefinement>,
    /// Why this dictation was pasted raw without (or before) a refinement result; one of the
    /// reason codes in `refinement_health::reason`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) refinement_skip_reason: Option<String>,
}

#[cfg(target_os = "windows")]
pub(crate) struct ManagedProcessJob {
    pub(crate) handle: isize,
}

#[cfg(target_os = "windows")]
impl Drop for ManagedProcessJob {
    fn drop(&mut self) {
        if self.handle == 0 {
            return;
        }
        unsafe {
            let _ = windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct StartupStatus {
    pub(crate) interactive: bool,
    pub(crate) transcription_ready: bool,
    pub(crate) rules_ready: bool,
    pub(crate) ollama_ready: bool,
    pub(crate) ollama_starting: bool,
    pub(crate) degraded_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct OllamaRuntimeDiagnostics {
    pub(crate) configured_path: String,
    pub(crate) detected: bool,
    pub(crate) spawn_stage: String,
    pub(crate) last_error: String,
    pub(crate) managed_pid: Option<u32>,
    pub(crate) endpoint: String,
    pub(crate) reachable: bool,
    pub(crate) active_requests: usize,
    pub(crate) idle_release_ms: u64,
}

impl Default for OllamaRuntimeDiagnostics {
    fn default() -> Self {
        Self {
            configured_path: String::new(),
            detected: false,
            spawn_stage: "idle".to_string(),
            last_error: String::new(),
            managed_pid: None,
            endpoint: String::new(),
            reachable: false,
            active_requests: 0,
            idle_release_ms: 90_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct WhisperRuntimeDiagnostics {
    pub(crate) cli_path: String,
    pub(crate) server_path: String,
    pub(crate) backend_selected: String,
    pub(crate) mode: String,
    pub(crate) accelerator: String,
    pub(crate) gpu_layers_requested: Option<usize>,
    pub(crate) gpu_layers_applied: Option<usize>,
    pub(crate) active_requests: usize,
    pub(crate) warm_until_ms: u64,
    pub(crate) idle_retire_ms: u64,
    pub(crate) last_error: String,
}

impl Default for WhisperRuntimeDiagnostics {
    fn default() -> Self {
        Self {
            cli_path: String::new(),
            server_path: String::new(),
            backend_selected: "unknown".to_string(),
            mode: "idle".to_string(),
            accelerator: "cpu".to_string(),
            gpu_layers_requested: None,
            gpu_layers_applied: None,
            active_requests: 0,
            warm_until_ms: 0,
            idle_retire_ms: 300_000,
            last_error: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RuntimeDiagnostics {
    pub(crate) ollama: OllamaRuntimeDiagnostics,
    pub(crate) whisper: WhisperRuntimeDiagnostics,
    pub(crate) frontend_watchdog: FrontendWatchdogDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct FrontendWatchdogDiagnostics {
    pub(crate) recovery_count: u64,
    pub(crate) restart_count: u64,
    pub(crate) last_recovery_reason: String,
    pub(crate) last_degraded_reason: String,
}

impl Default for FrontendWatchdogDiagnostics {
    fn default() -> Self {
        Self {
            recovery_count: 0,
            restart_count: 0,
            last_recovery_reason: String::new(),
            last_degraded_reason: String::new(),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct FrontendWatchdogState {
    pub(crate) recovery_timestamps_ms: VecDeque<u64>,
    pub(crate) restart_timestamps_ms: VecDeque<u64>,
    pub(crate) recovery_count: u64,
    pub(crate) restart_count: u64,
    pub(crate) last_recovery_reason: String,
    pub(crate) last_degraded_reason: String,
}

/// Window/control identity captured at paste time. Used by the Enter-handler
/// to validate that the user has not switched to a different target before
/// pressing Enter — without this, any focused window would feed the learner.
#[derive(Debug, Clone)]
pub struct TargetIdentity {
    pub hwnd: isize,
    pub pid: u32,
}

#[derive(Default)]
pub struct EnterCaptureState {
    /// Text + timestamp together: prevents a race between Load and Lock.
    pub last_paste: std::sync::Mutex<Option<(String, u64)>>,
    /// Foreground window/process at paste time. Cleared on Enter-mismatch.
    pub last_paste_target: std::sync::Mutex<Option<TargetIdentity>>,
    /// Shutdown signal sender. Dropping this lets the worker thread exit cleanly.
    pub shutdown_tx: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
    /// Hook-thread ID for PostThreadMessageW(WM_QUIT) on exit. 0 = not set.
    pub hook_thread_id: std::sync::atomic::AtomicU32,
}

pub(crate) struct AppState {
    pub(crate) settings: RwLock<Settings>,
    pub(crate) history: Mutex<PartitionedHistory>,
    pub(crate) recorder: Mutex<Recorder>,
    pub(crate) downloads: Mutex<HashSet<String>>,
    pub(crate) ollama_pulls: Mutex<HashSet<String>>,
    pub(crate) refinement_active_count: AtomicUsize,
    pub(crate) refinement_watchdog_generation: AtomicU64,
    pub(crate) refinement_last_change_ms: AtomicU64,
    pub(crate) refinement_last_success_ms: AtomicU64,
    pub(crate) refinement_last_success_model: Mutex<Option<String>>,
    /// Failure streak and recovery episode of the refinement self-heal (`refinement_health.rs`).
    pub(crate) refinement_health: Mutex<crate::refinement_health::RefinementHealth>,
    pub(crate) ollama_idle_release_generation: AtomicU64,
    /// Single owner of every job's paste outcome (see `paste_arbiter.rs`).
    pub(crate) paste_arbiter: crate::paste_arbiter::PasteArbiter,
    /// Handle to the managed Ollama child process for cleanup on app exit.
    pub(crate) managed_ollama_child: Mutex<Option<std::process::Child>>,
    /// Handle to the managed Whisper-Server child process for cleanup on app exit.
    pub(crate) managed_whisper_server_child: Mutex<Option<std::process::Child>>,
    /// Port for Whisper-Server HTTP API (default 8178).
    pub(crate) whisper_server_port: AtomicU16,
    pub(crate) whisper_server_warmup_started: AtomicBool,
    /// True once the OLLAMA model is confirmed loaded in VRAM (after warmup or
    /// successful refinement). Reset to false when the model is unloaded.
    pub(crate) ollama_model_warm: AtomicBool,
    /// True while the PTT warmup thread is running (between spawn and completion).
    /// Combined with ollama_model_warm, gives Cold/Loading/Warm tri-state for the overlay.
    pub(crate) ollama_warmup_in_progress: AtomicBool,
    /// True when sustained high GPU compute load (≥80% for BUSY_HOLD polls) blocks
    /// OLLAMA refinement to avoid multi-second OLLAMA dispatch delays under contention.
    /// Cleared when util drops below threshold for FREE_HOLD consecutive polls.
    pub(crate) gpu_busy: AtomicBool,
    pub(crate) gpu_util_high_streak: AtomicU32,
    pub(crate) gpu_util_low_streak: AtomicU32,
    pub(crate) whisper_server_warm_until_ms: AtomicU64,
    pub(crate) whisper_server_retire_generation: AtomicU64,
    pub(crate) startup_status: Mutex<StartupStatus>,
    pub(crate) runtime_diagnostics: Mutex<RuntimeDiagnostics>,
    pub(crate) overlay_controller: Mutex<OverlayController>,
    pub(crate) frontend_last_heartbeat_ms: AtomicU64,
    pub(crate) frontend_watchdog_last_reload_ms: AtomicU64,
    pub(crate) frontend_watchdog_reload_count: AtomicU64,
    pub(crate) frontend_watchdog_state: Mutex<FrontendWatchdogState>,
    pub(crate) enter_capture: EnterCaptureState,
    #[cfg(target_os = "windows")]
    #[cfg(target_os = "windows")]
    pub(crate) managed_process_job: Option<ManagedProcessJob>,
}

/// Returns `(reason, attempts)` of a self-heal episode that was still open, so the caller can
/// announce it as recovered.
pub(crate) fn record_refinement_success(
    state: &AppState,
    model: &str,
) -> Option<(&'static str, u32)> {
    state
        .refinement_last_success_ms
        .store(crate::util::now_ms(), std::sync::atomic::Ordering::Relaxed);
    {
        let mut last_model = state
            .refinement_last_success_model
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let trimmed = model.trim();
        *last_model = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    let mut health = state
        .refinement_health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::refinement_health::on_success(&mut health)
}

/// Only "minimized" is a real start state; unknown values and the legacy "tray" become "normal".
fn normalize_main_window_start_state(settings: &mut Settings) {
    if settings.main_window_start_state != "minimized" {
        settings.main_window_start_state = "normal".to_string();
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
            normalize_continuous_dump_fields(&mut settings);
            normalize_history_alias_fields(&mut settings);
            if settings.transcribe_backend.trim().is_empty() {
                settings.transcribe_backend = "whisper_cpp".to_string();
            }
            if settings.transcribe_backend != "whisper_cpp" {
                settings.transcribe_backend = "whisper_cpp".to_string();
            }
            settings.local_backend_preference = match settings
                .local_backend_preference
                .trim()
                .to_ascii_lowercase()
                .as_str()
            {
                "cuda" => "cuda".to_string(),
                "vulkan" => "vulkan".to_string(),
                _ => "auto".to_string(),
            };
            // Validate language_mode
            let valid_languages = [
                "auto", "en", "de", "fr", "es", "it", "pt", "nl", "pl", "ru", "ja", "ko", "zh",
                "ar", "tr", "hi",
            ];
            if !valid_languages.contains(&settings.language_mode.as_str()) {
                settings.language_mode = "auto".to_string();
            }
            settings.postproc_language = derive_postproc_language_from_asr(
                &settings.language_mode,
                settings.language_pinned,
            );
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
            settings.mic_input_gain_db = settings.mic_input_gain_db.clamp(-30.0, 30.0);
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
            normalize_main_window_start_state(&mut settings);
            normalize_topic_keywords_fields(&mut settings);
            // Normalize v0.7 AI fallback settings and legacy compatibility fields.
            normalize_ai_fallback_fields(&mut settings);
            if settings.setup.local_ai_wizard_completed {
                settings.setup.local_ai_wizard_pending = false;
            }
            settings
        }
        Err(_) => Settings::default(),
    }
}

pub(crate) fn normalize_topic_keywords_fields(settings: &mut Settings) {
    let defaults = default_topic_keywords();
    if settings.topic_keywords.is_empty() {
        settings.topic_keywords = defaults;
        return;
    }

    let mut normalized: HashMap<String, Vec<String>> = HashMap::new();
    for (topic, words) in settings.topic_keywords.iter() {
        let key = topic.trim().to_lowercase();
        if key.is_empty() {
            continue;
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut cleaned: Vec<String> = Vec::new();
        for word in words {
            let normalized_word = word.trim().to_lowercase();
            if normalized_word.is_empty() {
                continue;
            }
            if seen.insert(normalized_word.clone()) {
                cleaned.push(normalized_word);
            }
        }
        if !cleaned.is_empty() {
            normalized.insert(key, cleaned);
        }
    }

    if normalized.is_empty() {
        settings.topic_keywords = defaults;
        return;
    }

    for (topic, words) in defaults {
        normalized.entry(topic).or_insert(words);
    }
    settings.topic_keywords = normalized;
}

pub(crate) fn normalize_ai_fallback_fields(settings: &mut Settings) {
    // Migrate legacy postproc_llm toggle and values if still used.
    if settings.postproc_llm_enabled && !settings.ai_fallback.enabled {
        settings.ai_fallback.enabled = true;
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

    // A model name saved for a removed provider (e.g. `claude-…`) is no Ollama model: swap it for
    // the Ollama preference, else the catalog default. Stored `ollama` keeps its model untouched.
    let provider = settings.ai_fallback.provider.trim();
    let from_removed_provider = !provider.is_empty() && !provider.eq_ignore_ascii_case("ollama");
    settings.ai_fallback.normalize();
    if from_removed_provider {
        let preferred = settings.providers.ollama.preferred_model.trim();
        settings.ai_fallback.model = if preferred.is_empty() {
            AIFallbackSettings::default().model
        } else {
            preferred.to_string()
        };
    }
    settings.ai_fallback.custom_prompt_enabled = settings.ai_fallback.prompt_profile == "custom";
    settings.ai_fallback.use_default_prompt = false;
    // A user who ran on a system / manual Ollama has no managed runtime yet and may have no Ollama
    // running at all: the start-up health check then reads nothing and never reopens the setup, so
    // the migration reopens it once (the source is "managed" from here on).
    let left_foreign_runtime = settings.providers.has_foreign_runtime();
    settings.providers.normalize();
    if left_foreign_runtime && settings.ai_fallback.enabled {
        settings.setup.local_ai_wizard_completed = false;
        settings.setup.local_ai_wizard_pending = true;
    }

    // Keep legacy post-processing fields synchronized for compatibility with older code paths.
    settings.postproc_llm_enabled = settings.ai_fallback.enabled;
    settings.postproc_llm_model = settings.ai_fallback.model.clone();
    settings.postproc_language =
        derive_postproc_language_from_asr(&settings.language_mode, settings.language_pinned);
    let effective_language =
        effective_asr_language_hint(&settings.language_mode, settings.language_pinned);
    if let Some(prompt) = prompt_for_profile(
        &settings.ai_fallback.prompt_profile,
        &effective_language,
        Some(settings.ai_fallback.custom_prompt.as_str()),
        settings.ai_fallback.preserve_source_language,
    ) {
        settings.postproc_llm_prompt = prompt;
    }
    if settings.setup.local_ai_wizard_completed {
        settings.setup.local_ai_wizard_pending = false;
    }
}

pub(crate) fn normalize_continuous_dump_fields(settings: &mut Settings) {
    if settings.continuous_dump_profile != "balanced"
        && settings.continuous_dump_profile != "low_latency"
        && settings.continuous_dump_profile != "high_quality"
    {
        settings.continuous_dump_profile = "balanced".to_string();
    }

    settings.continuous_soft_flush_ms = settings.continuous_soft_flush_ms.clamp(4_000, 30_000);
    settings.continuous_silence_flush_ms = settings.continuous_silence_flush_ms.clamp(300, 5_000);
    settings.continuous_hard_cut_ms = settings.continuous_hard_cut_ms.clamp(15_000, 120_000);
    settings.continuous_min_chunk_ms = settings.continuous_min_chunk_ms.clamp(250, 5_000);
    settings.continuous_pre_roll_ms = settings.continuous_pre_roll_ms.clamp(0, 1_500);
    settings.continuous_post_roll_ms = settings.continuous_post_roll_ms.clamp(0, 1_500);
    settings.continuous_idle_keepalive_ms =
        settings.continuous_idle_keepalive_ms.clamp(10_000, 120_000);
    if settings.ptt_hot_keepalive_ms == 30_000 {
        settings.ptt_hot_keepalive_ms = 600_000;
    }
    settings.ptt_hot_keepalive_ms = settings.ptt_hot_keepalive_ms.clamp(5_000, 600_000);

    settings.continuous_mic_soft_flush_ms =
        settings.continuous_mic_soft_flush_ms.clamp(4_000, 30_000);
    settings.continuous_mic_silence_flush_ms =
        settings.continuous_mic_silence_flush_ms.clamp(300, 5_000);
    settings.continuous_mic_hard_cut_ms =
        settings.continuous_mic_hard_cut_ms.clamp(15_000, 120_000);
}

fn normalize_history_alias_value(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.chars().take(32).collect()
    }
}

pub(crate) fn normalize_history_alias_fields(settings: &mut Settings) {
    settings.history_alias_mic =
        normalize_history_alias_value(&settings.history_alias_mic, &default_history_alias_mic());
}

pub(crate) fn speaker_name_for_source(settings: &Settings, source: &str) -> String {
    match source {
        "mic" => settings.history_alias_mic.clone(),
        _ => source.to_string(),
    }
}

pub(crate) fn save_settings_file(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = resolve_config_path(app, "settings.json");
    let mut persisted = settings.clone();
    normalize_history_alias_fields(&mut persisted);
    let raw = serde_json::to_string_pretty(&persisted).map_err(|e| e.to_string())?;
    // Atomic write: write to .tmp then rename to avoid partial/corrupted JSON on crash.
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, &raw).map_err(|e| e.to_string())?;
    fs::rename(&tmp_path, &path).map_err(|e| e.to_string())?;
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

#[cfg(test)]
pub(crate) fn load_history_from_path(
    path: &std::path::Path,
) -> std::collections::VecDeque<HistoryEntry> {
    match fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => std::collections::VecDeque::new(),
    }
}

#[cfg(test)]
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
    history: &Mutex<PartitionedHistory>,
    text: String,
    source: String,
) -> Result<Vec<HistoryEntry>, String> {
    let speaker_name = {
        let state = app.state::<AppState>();
        let settings = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Some(speaker_name_for_source(&settings, &source))
    };
    let lock_started = Instant::now();
    let mut ph = history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = HistoryEntry {
        id: format!("h_{}", crate::util::now_ms()),
        text,
        timestamp_ms: crate::util::now_ms(),
        source,
        speaker_name,
        refinement: None,
        refinement_skip_reason: None,
    };
    ph.push_entry(entry);
    let updated: Vec<HistoryEntry> = ph.active.iter().cloned().collect();
    let lock_elapsed_ms = lock_started.elapsed().as_millis();
    drop(ph);
    if lock_elapsed_ms > HISTORY_LOCK_WARN_MS {
        warn!(
            "History lock hold exceeded threshold in push_history_entry_inner: {}ms",
            lock_elapsed_ms
        );
    }

    // Debounced persist: only schedule a disk write if none is already pending.
    if !HISTORY_SAVE_PENDING.swap(true, Ordering::AcqRel) {
        let app_clone = app.clone();
        crate::util::spawn_guarded("history_save_debounce", move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            HISTORY_SAVE_PENDING.store(false, Ordering::Release);
            let state = app_clone.state::<AppState>();
            let ph = state
                .history
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if let Err(e) = ph.flush_to_disk() {
                warn!("Debounced history save failed: {}", e);
            }
        });
    }

    Ok(updated)
}

fn emit_updated_history(app: &AppHandle, event_name: &str, updated: Vec<HistoryEntry>) {
    let _ = app.emit(event_name, updated);
}

fn update_history_entry_in_store<F>(
    app: &AppHandle,
    store: &Mutex<PartitionedHistory>,
    event_name: &str,
    entry_id: &str,
    apply: &mut F,
) -> Result<bool, String>
where
    F: FnMut(&mut HistoryEntry),
{
    let lock_started = Instant::now();
    let mut ph = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(entry) = ph.active.iter_mut().find(|entry| entry.id == entry_id) else {
        return Ok(false);
    };
    apply(entry);
    let updated: Vec<HistoryEntry> = ph.active.iter().cloned().collect();
    ph.flush_to_disk().ok();
    let lock_elapsed_ms = lock_started.elapsed().as_millis();
    drop(ph);
    if lock_elapsed_ms > HISTORY_LOCK_WARN_MS {
        warn!(
            "History lock hold exceeded threshold in update_history_entry_in_store ({}): {}ms",
            event_name, lock_elapsed_ms
        );
    }
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
    update_history_entry_in_store(app, &state.history, "history:updated", entry_id, &mut apply)?;
    Ok(())
}

fn ensure_history_refinement(entry: &mut HistoryEntry) -> &mut HistoryRefinement {
    if entry.refinement.is_none() {
        entry.refinement = Some(HistoryRefinement::default());
    }
    entry
        .refinement
        .as_mut()
        .expect("refinement just initialized")
}

/// Remember why a dictation was pasted raw (see `HistoryEntry::refinement_skip_reason`).
pub(crate) fn mark_entry_refinement_skipped(
    app: &AppHandle,
    entry_id: &str,
    skip_reason: &str,
) -> Result<(), String> {
    update_history_entry_refinement(app, entry_id, |entry| {
        entry.refinement_skip_reason = Some(skip_reason.to_string());
    })
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

    #[test]
    fn legacy_tray_start_state_normalizes_to_normal() {
        assert_eq!(Settings::default().main_window_start_state, "normal");
        for (stored, expected) in [
            ("tray", "normal"),
            ("normal", "normal"),
            ("minimized", "minimized"),
            ("bogus", "normal"),
        ] {
            let mut settings = Settings::default();
            settings.main_window_start_state = stored.to_string();
            normalize_main_window_start_state(&mut settings);
            assert_eq!(settings.main_window_start_state, expected);
        }
    }

    #[test]
    fn legacy_vocab_cleanup_field_and_pending_pairs_without_promoted_flag_still_load() {
        // The LLM vocab cleanup job (and its throttle field) was removed; old files still carry it.
        let settings: Settings = serde_json::from_str(
            r#"{ "last_vocab_cleanup_ms": 1762000000000,
                 "edit_delta_migrated": true,
                 "edit_substitutions": [
                   { "from": "Trispa", "to": "Trispr", "count": 1,
                     "first_seen_ms": 1, "last_seen_ms": 2 } ] }"#,
        )
        .expect("legacy vocab settings must still deserialize");
        assert_eq!(settings.edit_substitutions.len(), 1);
        assert!(!settings.edit_substitutions[0].promoted);
        let rewritten = serde_json::to_string(&settings).expect("serialize settings");
        assert!(!rewritten.contains("last_vocab_cleanup_ms"));
        assert!(rewritten.contains("\"promoted\":false"));
    }

    #[test]
    fn removed_remote_expert_opt_in_in_old_setup_block_still_loads() {
        let settings: Settings = serde_json::from_str(
            r#"{ "setup": { "local_ai_wizard_completed": true, "local_ai_wizard_pending": false,
                            "ollama_remote_expert_opt_in": true } }"#,
        )
        .expect("old setup block must still deserialize");
        assert!(settings.setup.local_ai_wizard_completed);
        assert!(!settings.setup.local_ai_wizard_pending);
        let rewritten = serde_json::to_string(&settings).expect("serialize settings");
        assert!(!rewritten.contains("ollama_remote_expert_opt_in"));
    }

    #[test]
    fn settings_with_removed_assistant_gdd_confluence_task_capture_fields_still_load() {
        // Shape of a settings.json written before the Assistant / GDD / Confluence /
        // Task Capture modules were removed. Unknown fields are ignored by serde.
        let raw = r#"{
            "mode": "ptt",
            "hotkey_ptt": "CommandOrControl+Shift+Space",
            "product_mode": "assistant",
            "hotkey_product_mode_toggle": "CommandOrControl+Shift+P",
            "gdd_module_settings": { "enabled": true, "default_preset_id": "x", "preset_clones": [] },
            "confluence_settings": { "enabled": true, "site_base_url": "https://example.atlassian.net" },
            "workflow_agent": { "enabled": true, "wakewords": ["trispr"], "hands_free_enabled": true },
            "task_capture_settings": { "routes": [], "match_mode": "contains" },
            "assistant_presence_enabled": true,
            "assistant_presence_window_x": 10,
            "module_settings": {
                "enabled_modules": ["assistant_core", "assistant_presence", "gdd", "task_capture", "ai_refinement"],
                "consented_permissions": { "assistant_core": ["filesystem_history"], "integrations_confluence": [] },
                "module_overrides": {
                    "assistant_core.migrated_legacy_binding": true,
                    "workflow_agent.last_error": "stale",
                    "ai_refinement.migrated_legacy": true
                }
            }
        }"#;

        let settings: Settings =
            serde_json::from_str(raw).expect("old settings.json must still deserialize");

        assert_eq!(settings.mode, "ptt");
        assert_eq!(settings.hotkey_ptt, "CommandOrControl+Shift+Space");
        let rewritten = serde_json::to_string(&settings).expect("serialize settings");
        assert!(!rewritten.contains("module_settings"));
    }

    #[test]
    fn settings_with_removed_module_settings_still_load_and_keep_ai_refinement_flag() {
        let raw = r#"{
            "ai_fallback": { "enabled": true, "model": "gemma4:e4b" },
            "module_settings": {
                "enabled_modules": ["ai_refinement", "opus"],
                "consented_permissions": { "opus": ["filesystem_exports"] },
                "module_overrides": { "ai_refinement.migrated_legacy": true }
            }
        }"#;
        let settings: Settings = serde_json::from_str(raw).expect("old settings.json must load");
        assert!(settings.ai_fallback.enabled);
        let rewritten = serde_json::to_string(&settings).expect("serialize settings");
        assert!(!rewritten.contains("module_settings"));
        assert!(!rewritten.contains("enabled_modules"));
    }

    #[test]
    fn settings_with_removed_system_audio_opus_recording_fields_still_load() {
        // Shape of a settings.json written before System-Audio transcription, Opus / recording
        // export and the session manager were removed. Unknown fields are ignored by serde; the
        // kept mic fields next to them must survive untouched.
        let raw = r##"{
            "mode": "ptt",
            "hotkey_ptt": "CommandOrControl+Shift+Space",
            "continuous_dump_profile": "low_latency",
            "continuous_mic_soft_flush_ms": 12000,
            "transcribe_enabled": true,
            "transcribe_hotkey": "CommandOrControl+Shift+T",
            "transcribe_output_device": "wasapi:{0.0.0.00000000}.{abc}",
            "transcribe_vad_mode": true,
            "transcribe_vad_threshold": 0.05,
            "transcribe_vad_silence_ms": 1000,
            "transcribe_batch_interval_ms": 9000,
            "transcribe_chunk_overlap_ms": 500,
            "transcribe_input_gain_db": 3.0,
            "history_alias_system": "Speakers",
            "opus_enabled": true,
            "opus_bitrate_kbps": 96,
            "auto_save_system_audio": true,
            "auto_save_mic_audio": true,
            "continuous_system_override_enabled": true,
            "continuous_system_soft_flush_ms": 9000,
            "continuous_system_silence_flush_ms": 900,
            "continuous_system_hard_cut_ms": 60000,
            "session_idle_timeout_ms": 60000,
            "ptt_session_grouping_enabled": true,
            "ptt_session_group_timeout_s": 120,
            "module_settings": { "enabled_modules": ["opus", "ai_refinement"] }
        }"##;

        let mut settings: Settings =
            serde_json::from_str(raw).expect("old settings.json must still deserialize");
        normalize_continuous_dump_fields(&mut settings);
        assert_eq!(settings.mode, "ptt");
        assert_eq!(settings.continuous_dump_profile, "low_latency");
        assert_eq!(settings.continuous_mic_soft_flush_ms, 12_000);
    }

    #[test]
    fn settings_with_removed_vision_voice_output_video_fields_still_load() {
        // Shape of a settings.json written before the Vision / Voice Output (TTS, Piper) /
        // Video Generation modules were removed. Unknown fields are ignored by serde.
        let raw = r##"{
            "mode": "ptt",
            "hotkey_ptt": "CommandOrControl+Shift+Space",
            "hotkey_tts_stop": "CommandOrControl+Shift+F12",
            "overlay_tts_stop_enabled": true,
            "overlay_tts_stop_shape": "round",
            "overlay_tts_stop_color": "#ff0000",
            "vision_input_settings": { "enabled": true, "fps": 4, "source_scope": "active_window" },
            "voice_output_settings": {
                "enabled": true,
                "default_provider": "local_custom",
                "output_policy": "agent_replies_only",
                "piper_binary_path": "C:/piper/piper.exe",
                "qwen3_tts_enabled": true
            },
            "video_generation_settings": { "enabled": true, "default_fps": 60, "narrative_mode": true },
            "module_settings": {
                "enabled_modules": ["input_vision", "output_voice_tts", "output_video_generation", "piper_tts", "ai_refinement"],
                "consented_permissions": {
                    "input_vision": ["screen_capture"],
                    "output_voice_tts": ["audio_output"],
                    "output_video_generation": ["filesystem_exports"],
                    "piper_tts": ["audio_output"]
                },
                "module_overrides": {
                    "output_voice_tts.last_error": "stale",
                    "output_video_generation.installed": true,
                    "piper_tts.installed": true,
                    "ai_refinement.migrated_legacy": true
                }
            }
        }"##;

        let settings: Settings =
            serde_json::from_str(raw).expect("old settings.json must still deserialize");

        assert_eq!(settings.mode, "ptt");
        assert_eq!(settings.hotkey_ptt, "CommandOrControl+Shift+Space");
        // Removed fields must not be written back out.
        let rewritten = serde_json::to_string(&settings).expect("serialize settings");
        for removed in [
            "hotkey_tts_stop",
            "overlay_tts_stop",
            "vision_input_settings",
            "voice_output_settings",
            "video_generation_settings",
        ] {
            assert!(!rewritten.contains(removed), "{removed} must not persist");
        }
    }

    #[test]
    fn leaving_a_system_runtime_reopens_the_setup_once_for_ai_users() {
        let raw = |enabled: bool| {
            format!(
                r#"{{
                "ai_fallback": {{ "enabled": {enabled}, "model": "gemma4:e4b" }},
                "providers": {{ "ollama": {{ "runtime_source": "system", "runtime_path": "C:/Program Files/Ollama/ollama.exe" }} }},
                "setup": {{ "local_ai_wizard_completed": true, "local_ai_wizard_pending": false }}
            }}"#
            )
        };

        let mut used_ai: Settings = serde_json::from_str(&raw(true)).expect("deserialize");
        normalize_ai_fallback_fields(&mut used_ai);
        assert!(!used_ai.setup.local_ai_wizard_completed);
        assert!(used_ai.setup.local_ai_wizard_pending);
        assert_eq!(used_ai.providers.ollama.runtime_source, "per_user_zip");

        // Second load: already managed, a wizard the user finished stays finished.
        used_ai.setup.local_ai_wizard_completed = true;
        used_ai.setup.local_ai_wizard_pending = false;
        normalize_ai_fallback_fields(&mut used_ai);
        assert!(used_ai.setup.local_ai_wizard_completed);
        assert!(!used_ai.setup.local_ai_wizard_pending);

        // Refinement off: nothing to set up, no wizard popup.
        let mut no_ai: Settings = serde_json::from_str(&raw(false)).expect("deserialize");
        normalize_ai_fallback_fields(&mut no_ai);
        assert!(no_ai.setup.local_ai_wizard_completed);
        assert!(!no_ai.setup.local_ai_wizard_pending);
    }

    #[test]
    fn settings_with_removed_cloud_provider_lm_studio_and_runtime_variant_fields_still_load() {
        // Shape of a settings.json written before cloud providers, LM Studio / Oobabooga and the
        // system / manual Ollama runtime variants were removed. Unknown fields are ignored by
        // serde, known ones with removed values must land on Ollama + the managed runtime.
        let raw = r#"{
            "mode": "ptt",
            "cloud_fallback": true,
            "postproc_llm_provider": "claude",
            "postproc_llm_api_key": "must-never-persist",
            "ai_fallback": {
                "enabled": true,
                "provider": "claude",
                "fallback_provider": "openai",
                "execution_mode": "online_fallback",
                "strict_local_mode": false,
                "model": "gemma4:e4b"
            },
            "providers": {
                "claude": { "api_key_stored": true, "auth_status": "verified_api_key", "preferred_model": "claude-3-5-haiku-20241022" },
                "openai": { "api_key_stored": true, "auth_status": "verified_api_key" },
                "gemini": { "api_key_stored": false, "auth_status": "locked" },
                "lm_studio": { "endpoint": "http://127.0.0.1:1234", "api_key": "lm-key", "preferred_model": "x" },
                "oobabooga": { "endpoint": "http://127.0.0.1:5000", "api_key": "" },
                "ollama": {
                    "endpoint": "http://192.168.1.20:11434",
                    "fallback_endpoints": ["http://127.0.0.1:1234", "http://127.0.0.1:5000"],
                    "runtime_source": "system",
                    "runtime_path": "C:/Program Files/Ollama/ollama.exe",
                    "runtime_version": "0.9.0",
                    "preferred_model": "gemma4:e4b",
                    "available_models": ["gemma4:e4b"]
                }
            },
            "module_settings": { "enabled_modules": ["ai_refinement"] }
        }"#;

        let mut settings: Settings =
            serde_json::from_str(raw).expect("old settings.json must still deserialize");
        for stale_provider in [
            "claude",
            "openai",
            "gemini",
            "lm_studio",
            "oobabooga",
            "manual",
        ] {
            settings.ai_fallback.provider = stale_provider.to_string();
            settings.providers.ollama.runtime_source = "manual".to_string();
            normalize_ai_fallback_fields(&mut settings);
            assert_eq!(settings.ai_fallback.provider, "ollama", "{stale_provider}");
            assert_eq!(settings.providers.ollama.runtime_source, "per_user_zip");
        }

        let mut settings: Settings = serde_json::from_str(raw).expect("deserialize again");
        normalize_ai_fallback_fields(&mut settings);
        assert!(settings.ai_fallback.enabled);
        assert_eq!(settings.ai_fallback.provider, "ollama");
        assert_eq!(settings.providers.ollama.runtime_source, "per_user_zip");
        assert_eq!(settings.providers.ollama.endpoint, "http://127.0.0.1:11434");
        // The system Ollama was somebody else's binary: its path and version are forgotten.
        assert!(settings.providers.ollama.runtime_path.is_empty());
        assert!(settings.providers.ollama.runtime_version.is_empty());
        assert_eq!(settings.providers.ollama.preferred_model, "gemma4:e4b");

        // A managed runtime that is already recorded keeps its path.
        let mut managed: Settings = serde_json::from_str(
            r#"{ "providers": { "ollama": {
                "runtime_source": "per_user_zip",
                "runtime_path": "C:/rt/0.40.13/ollama.exe",
                "runtime_version": "0.40.13" } } }"#,
        )
        .expect("managed settings");
        normalize_ai_fallback_fields(&mut managed);
        assert_eq!(
            managed.providers.ollama.runtime_path,
            "C:/rt/0.40.13/ollama.exe"
        );
        assert_eq!(managed.providers.ollama.runtime_version, "0.40.13");

        // Removed fields (and any secret) must not be written back out.
        let rewritten = serde_json::to_string(&settings).expect("serialize settings");
        for removed in [
            "cloud_fallback",
            "postproc_llm_provider",
            "postproc_llm_api_key",
            "must-never-persist",
            "lm-key",
            "fallback_provider",
            "fallback_endpoints",
            "execution_mode",
            "strict_local_mode",
            "api_key_stored",
            "lm_studio",
            "oobabooga",
            "claude",
            "openai",
            "gemini",
        ] {
            assert!(!rewritten.contains(removed), "{removed} must not persist");
        }
    }

    fn settings_from(ai_fallback: &str, providers: &str) -> Settings {
        serde_json::from_str(&format!(
            r#"{{ "ai_fallback": {ai_fallback}, "providers": {providers} }}"#
        ))
        .expect("settings json")
    }

    #[test]
    fn model_of_a_removed_provider_is_replaced_by_the_ollama_model() {
        // (a) no Ollama preference: catalog default.
        let mut a = settings_from(
            r#"{ "provider": "claude", "model": "claude-3-5-sonnet" }"#,
            "{}",
        );
        normalize_ai_fallback_fields(&mut a);
        assert_eq!(a.ai_fallback.provider, "ollama");
        assert_eq!(a.ai_fallback.model, "gemma4:e4b");
        assert_eq!(a.postproc_llm_model, "gemma4:e4b");

        // (b) Ollama preference wins.
        let mut b = settings_from(
            r#"{ "provider": "lm_studio", "model": "local-model" }"#,
            r#"{ "ollama": { "preferred_model": "qwen3.5:4b" } }"#,
        );
        normalize_ai_fallback_fields(&mut b);
        assert_eq!(b.ai_fallback.provider, "ollama");
        assert_eq!(b.ai_fallback.model, "qwen3.5:4b");

        // (c) A stored Ollama provider keeps its model.
        let mut c = settings_from(
            r#"{ "provider": "ollama", "model": "gemma4:e2b" }"#,
            r#"{ "ollama": { "preferred_model": "qwen3.5:4b" } }"#,
        );
        normalize_ai_fallback_fields(&mut c);
        assert_eq!(c.ai_fallback.model, "gemma4:e2b");
    }

    fn sample_history_entry(id: &str, status: &str, refined: &str, error: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            text: "raw transcript".to_string(),
            timestamp_ms: 1_772_101_100_000,
            source: "local".to_string(),
            speaker_name: Some("local".to_string()),
            refinement: Some(HistoryRefinement {
                job_id: format!("job-{id}"),
                raw: "raw transcript".to_string(),
                refined: refined.to_string(),
                status: status.to_string(),
                model: "qwen3:14b".to_string(),
                execution_time_ms: Some(1234),
                error: error.to_string(),
            }),
            refinement_skip_reason: None,
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

    #[test]
    fn skip_reason_is_persisted_only_when_set_and_old_entries_still_load() {
        let mut entry = sample_history_entry("a", "idle", "", "");
        let plain = serde_json::to_value(&entry).expect("serialize entry");
        assert!(
            plain.get("refinement_skip_reason").is_none(),
            "an entry without a reason must stay byte-compatible with older builds"
        );

        entry.refinement_skip_reason = Some("model_not_loaded".to_string());
        let with_reason = serde_json::to_value(&entry).expect("serialize entry");
        assert_eq!(with_reason["refinement_skip_reason"], "model_not_loaded");
        let restored: HistoryEntry = serde_json::from_value(with_reason).expect("reload entry");
        assert_eq!(
            restored.refinement_skip_reason.as_deref(),
            Some("model_not_loaded")
        );

        // history written before the field existed
        let old: HistoryEntry = serde_json::from_str(
            r#"{ "id": "h_1", "text": "hi", "timestamp_ms": 1, "source": "local" }"#,
        )
        .expect("old history entry must still load");
        assert_eq!(old.refinement_skip_reason, None);
    }
}
