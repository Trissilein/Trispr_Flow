// Type definitions for Trispr Flow application

export type OverlayRefiningIndicatorPreset = "subtle" | "standard" | "intense";
export type ModuleId = "ai_refinement" | "opus";
export type ModuleState = "not_installed" | "installed" | "enabled" | "active" | "error";
export type ModulePermission = string;
export type RefinementPromptPreset =
  | "wording"
  | "summary"
  | "technical_specs"
  | "action_items"
  | "llm_prompt"
  | "custom";

export interface UserRefinementPromptPreset {
  id: string;
  name: string;
  prompt: string;
  previous_prompt?: string;
}

/** Per-built-in override strings. Replaces factory default for both EN and DE when present. */
export type PromptPresetOverrides = Partial<
  Record<Exclude<RefinementPromptPreset, "custom">, string>
>;

/** A correction the user made by editing the pasted refinement output before submitting. */
export interface EditSubstitution {
  /** Original token as it appeared in the refinement output. */
  from: string;
  /** What the user typed instead. */
  to: string;
  /** Number of times this correction has been observed. */
  count: number;
  first_seen_ms: number;
  last_seen_ms: number;
}

export interface ModuleDescriptor {
  id: ModuleId | string;
  name: string;
  version: string;
  state: ModuleState;
  dependencies: string[];
  permissions: ModulePermission[];
  restart_required: boolean;
  last_error?: string | null;
  bundled: boolean;
  core: boolean;
  toggleable: boolean;
}

export interface ModuleSettings {
  enabled_modules: string[];
  consented_permissions: Record<string, string[]>;
  module_overrides: Record<string, unknown>;
}

export interface AIFallbackSettings {
  enabled: boolean;
  /** Refinement is Ollama-only; the backend normalizes every saved value to "ollama". */
  provider: "ollama";
  preserve_source_language: boolean;
  model: string;
  temperature: number;
  max_tokens: number;
  low_latency_mode: boolean;
  prompt_profile: RefinementPromptPreset;
  custom_prompt_enabled: boolean;
  custom_prompt: string;
  use_default_prompt: boolean;
  prompt_presets?: UserRefinementPromptPreset[];
  active_prompt_preset_id?: string;
  prompt_preset_overrides?: PromptPresetOverrides;
}

export interface OllamaSettings {
  endpoint: string;
  available_models: string[];
  preferred_model: string;
  /** Always the managed per-user runtime. */
  runtime_source: "per_user_zip";
  runtime_path: string;
  runtime_version: string;
  runtime_target_version: string;
  /** The target was picked in the runtime version list; only then may it stay older than the running runtime. */
  runtime_target_explicit?: boolean;
  last_health_check: string | null;
}

export interface AIProvidersSettings {
  ollama: OllamaSettings;
}

export interface SetupSettings {
  local_ai_wizard_completed: boolean;
  local_ai_wizard_pending: boolean;
}

export interface Settings {
  mode: "ptt" | "vad";
  hotkey_ptt: string;
  hotkey_toggle: string;
  input_device: string;
  language_mode: "auto" | "en" | "de" | "fr" | "es" | "it" | "pt" | "nl" | "pl" | "ru" | "ja" | "ko" | "zh" | "ar" | "tr" | "hi";
  language_pinned: boolean;
  model: string;
  ai_fallback: AIFallbackSettings;
  providers: AIProvidersSettings;
  setup: SetupSettings;
  module_settings?: ModuleSettings;
  audio_cues: boolean;
  audio_cues_volume: number;
  diagnostic_logging_enabled?: boolean;
  ptt_use_vad: boolean;
  ptt_hot_keepalive_ms: number;
  vad_threshold: number;
  vad_threshold_start: number;
  vad_threshold_sustain: number;
  vad_silence_ms: number;
  transcribe_enabled: boolean;
  transcribe_hotkey: string;
  hotkey_toggle_activation_words: string;
  transcribe_output_device: string;
  transcribe_vad_mode: boolean;
  transcribe_vad_threshold: number;
  transcribe_vad_silence_ms: number;
  transcribe_batch_interval_ms: number;
  transcribe_chunk_overlap_ms: number;
  transcribe_input_gain_db: number;
  mic_input_gain_db: number;
  history_alias_mic: string;
  history_alias_system: string;
  capture_enabled: boolean;
  model_source: "default" | "custom";
  model_custom_url: string;
  model_storage_dir: string;
  hidden_external_models?: string[];
  overlay_color: string;
  overlay_min_radius: number;
  overlay_max_radius: number;
  overlay_rise_ms: number;
  overlay_fall_ms: number;
  overlay_opacity_inactive: number;
  overlay_opacity_active: number;
  overlay_kitt_color: string;
  overlay_kitt_rise_ms: number;
  overlay_kitt_fall_ms: number;
  overlay_kitt_opacity_inactive: number;
  overlay_kitt_opacity_active: number;
  overlay_pos_x: number;
  overlay_pos_y: number;
  overlay_kitt_pos_x: number;
  overlay_kitt_pos_y: number;
  overlay_style: string;
  overlay_refining_indicator_enabled: boolean;
  overlay_refining_indicator_preset: OverlayRefiningIndicatorPreset;
  overlay_refining_indicator_color: string;
  overlay_refining_indicator_speed_ms: number;
  overlay_refining_indicator_range: number;
  overlay_kitt_min_width: number;
  overlay_kitt_max_width: number;
  overlay_kitt_height: number;
  hallucination_filter_enabled: boolean;
  activation_words_enabled: boolean;
  activation_words: string[];
  topic_keywords: Record<string, string[]>;
  // Post-processing settings
  postproc_enabled: boolean;
  postproc_language: string;
  postproc_punctuation_enabled: boolean;
  postproc_capitalization_enabled: boolean;
  postproc_numbers_enabled: boolean;
  postproc_custom_vocab_enabled: boolean;
  postproc_custom_vocab: Record<string, string>;
  /**
   * Auto-learned proper nouns, acronyms, and project-specific terms.
   * Populated by the auto-learning heuristic as the user dictates — no
   * manual entry required. Injected as whisper-cli `--prompt` and forwarded
   * to the LLM refinement prompt as terms to preserve verbatim.
   */
  vocab_terms: string[];
  /** Substitution pairs observed from user edits, accumulating toward auto-promotion. */
  edit_substitutions?: EditSubstitution[];
  /** Set to true after the one-time migration that clears legacy heuristic data. */
  edit_delta_migrated?: boolean;
  /** Unix-ms timestamp of the last successful LLM vocab cleanup run. */
  last_vocab_cleanup_ms?: number;
  postproc_llm_enabled: boolean;
  postproc_llm_model: string;
  postproc_llm_prompt: string;
  // Recording export settings
  opus_enabled?: boolean;
  opus_bitrate_kbps?: number;
  auto_save_system_audio?: boolean;
  auto_save_mic_audio?: boolean;
  continuous_dump_enabled?: boolean;
  continuous_dump_profile?: "balanced" | "low_latency" | "high_quality";
  continuous_soft_flush_ms?: number;
  continuous_silence_flush_ms?: number;
  continuous_hard_cut_ms?: number;
  continuous_min_chunk_ms?: number;
  continuous_pre_roll_ms?: number;
  continuous_post_roll_ms?: number;
  continuous_idle_keepalive_ms?: number;
  continuous_mic_override_enabled?: boolean;
  continuous_mic_soft_flush_ms?: number;
  continuous_mic_silence_flush_ms?: number;
  continuous_mic_hard_cut_ms?: number;
  continuous_system_override_enabled?: boolean;
  continuous_system_soft_flush_ms?: number;
  continuous_system_silence_flush_ms?: number;
  continuous_system_hard_cut_ms?: number;
  transcribe_backend?: "whisper_cpp";
  local_backend_preference?: "auto" | "cuda" | "vulkan";
  // Window state fields from backend
  main_window_x?: number | null;
  main_window_y?: number | null;
  main_window_width?: number | null;
  main_window_height?: number | null;
  main_window_monitor?: string | null;
  conv_window_x?: number | null;
  conv_window_y?: number | null;
  conv_window_width?: number | null;
  conv_window_height?: number | null;
  conv_window_monitor?: string | null;
  conv_window_always_on_top?: boolean;
  main_window_start_state?: "normal" | "minimized";
  // UI theming
  accent_color: string;
}

export interface HistoryEntry {
  id: string;
  text: string;
  timestamp_ms: number;
  source: string;
  speaker_name?: string | null;
  refinement?: HistoryRefinement | null;
}

export interface HistoryRefinement {
  job_id: string;
  raw: string;
  refined: string;
  status: "idle" | "refining" | "refined" | "error";
  model: string;
  execution_time_ms?: number | null;
  error: string;
}

export interface TopicScore {
  topic: string;
  hits: number;
  share: number; // Percentage of total keyword hits (0..100)
}

export interface AudioDevice {
  id: string;
  label: string;
}

export interface ModelInfo {
  id: string;
  label: string;
  file_name: string;
  size_mb: number;
  installed: boolean;
  downloading: boolean;
  path?: string;
  source: string;
  available: boolean;
  download_url?: string;
  removable: boolean;
}

export interface DownloadProgress {
  id: string;
  downloaded: number;
  total?: number;
}

export interface DownloadComplete {
  id: string;
  path: string;
}

export interface DownloadError {
  id: string;
  error: string;
}

export interface QuantizeProgress {
  file_name: string;
  quant: string;
  phase: "starting" | "running" | "finalizing" | "done";
  percent?: number;
  message?: string;
}

export interface ValidationResult {
  valid: boolean;
  error: string | null;
  formatted: string | null;
}

export interface AppErrorType {
  type: "AudioDevice" | "Transcription" | "Hotkey" | "Storage" | "Network" | "Window" | "Other";
  message: string;
}

export interface ErrorEvent {
  error: AppErrorType;
  timestamp: number;
  context?: string;
}

export type DependencyPreflightStatus = "ok" | "warning" | "error";

export interface DependencyPreflightItem {
  id: string;
  status: DependencyPreflightStatus;
  required: boolean;
  message: string;
  hint?: string | null;
}

export interface DependencyPreflightReport {
  generated_at_ms: number;
  overall_status: DependencyPreflightStatus;
  blocking_count: number;
  warning_count: number;
  items: DependencyPreflightItem[];
}

export type ToastType = "error" | "success" | "warning" | "info";

export interface ToastOptions {
  type?: ToastType;
  title: string;
  message: string;
  duration?: number;
  icon?: string;
  actionLabel?: string;
  onAction?: () => void | Promise<void>;
  actionDismiss?: boolean;
}

export interface TranscribeBacklogStatus {
  queued_chunks: number;
  capacity_chunks: number;
  percent_used: number;
  dropped_chunks: number;
  suggested_capacity_chunks: number;
}

export interface TranscriptionResultEvent {
  text: string;
  source: string;
  job_id: string;
  paste_deferred?: boolean;
  paste_timeout_ms?: number;
  entry_id?: string;
  refinement_gate?: RefinementGateDecision;
}

export interface RefinementGateDecision {
  enabled: boolean;
  provider: string;
  model: string;
  should_refine: boolean;
  paste_deferred: boolean;
  skipped_reason?: string | null;
  ollama_model_loaded?: boolean;
  ollama_vram_bytes?: number;
}

export interface TranscriptionRefinementStartedEvent {
  job_id: string;
  entry_id?: string;
  source: string;
  original: string;
  model?: string;
}

export interface TranscriptionRefinedEvent {
  job_id: string;
  entry_id?: string;
  original: string;
  refined: string;
  source: string;
  model: string;
  execution_time_ms: number;
  /** True if the refined text won the paste race; false = history-only (deadline pasted raw first). */
  pasted?: boolean;
}

export type PasteSettledOutcome = "raw" | "refined" | "raw_fallback" | "raw_timeout";

export interface PasteSettledEvent {
  job_id: string;
  outcome: PasteSettledOutcome;
  text: string;
  paste_error?: string | null;
}

export type RefinementFailureReason =
  | "runtime_not_ready"
  | "queue_full"
  | "prepare_failed"
  | "provider_error";

export interface TranscriptionRefinementFailedEvent {
  job_id: string;
  entry_id?: string;
  source: string;
  original?: string;
  error: string;
  reason?: RefinementFailureReason | string;
}

export interface TranscriptionRefinementActivityEvent {
  active_count: number;
  state: "idle" | "active";
  reason: "started" | "finished" | "watchdog_reset" | "forced_reset";
}

export interface TranscriptionGpuActivityEvent {
  state: "idle" | "active" | "cpu" | "error";
  accelerator: "gpu" | "cpu";
  backend: string;
  source: "whisper";
  message?: string;
}

export type RecordingState = "disabled" | "idle" | "recording" | "transcribing";
export type HistoryTab = "mic" | "system" | "conversation";

// Ollama model pull events
export interface OllamaPullProgress {
  model: string;
  status: string;
  digest?: string;
  total?: number;
  completed?: number;
}

export interface OllamaPullComplete {
  model: string;
}

export interface OllamaPullError {
  model: string;
  error: string;
}

/** One row of `check_ollama_model_requirements` (Registry config blob `requires` vs. running runtime). */
export interface OllamaModelRequirement {
  name: string;
  /** Minimum Ollama version the model needs; null for Hugging Face refs and unreadable manifests. */
  requires: string | null;
  runtime_version: string | null;
  /** runtime >= requires; null when either side is not a strict x.y.z version. */
  compatible: boolean | null;
  detail?: string | null;
}

/** `verify_ollama_model`: functional check with the app's own request form. */
export interface OllamaModelVerifyResult {
  ok: boolean;
  latency_ms: number;
  load_ms: number;
  sample?: string | null;
  detail?: string | null;
}

/** `finalize_ollama_model_update`: the old build is only dropped after the new one passed. */
export interface OllamaModelFinalizeResult {
  verified: boolean;
  restored: boolean;
  /**
   * The check could not finish (timeout, transport error, HTTP 5xx, Ollama gone): neither passed nor
   * failed. The new build and the backup both stay; `detail` names the reason.
   */
  inconclusive?: boolean;
  detail?: string | null;
}

/** `rollback_ollama_model_update`: removes the new build and restores the backed-up one. */
export interface OllamaModelRollbackResult {
  restored: boolean;
  detail?: string | null;
}

/** What the local Ollama reports for an installed model (`get_ollama_model_details`, `/api/show`). */
export interface OllamaLocalModelInfo {
  license?: string | null;
  capabilities?: string[];
  family?: string | null;
  parameter_size?: string | null;
  quantization_level?: string | null;
  format?: string | null;
  context_length?: number | null;
  architecture?: string | null;
  parameter_count?: number | null;
  base_model?: string | null;
  requires?: string | null;
  modified_at?: string | null;
}

/** What the model's public page says (Hugging Face API or Ollama library page). */
export interface OllamaRemoteModelInfo {
  source: "huggingface" | "ollama";
  /** Hugging Face: repository created. The Ollama library has no release date. */
  released_at?: string | null;
  updated_at?: string | null;
  downloads?: number | null;
  /** Ollama shows rounded counts ("28.4M"). */
  downloads_text?: string | null;
  likes?: number | null;
  license?: string | null;
  pipeline_tag?: string | null;
  languages?: string[];
  base_model?: string | null;
  description?: string | null;
  gated?: boolean | null;
}

/** `get_ollama_model_details`: local data always, public data when online and not in strict local mode. */
export interface OllamaModelDetails {
  model: string;
  local?: OllamaLocalModelInfo | null;
  local_error?: string | null;
  remote?: OllamaRemoteModelInfo | null;
  remote_note?: string | null;
  remote_cached?: boolean;
  remote_stale?: boolean;
  remote_fetched_at_ms?: number | null;
}

/** `replace_ollama_model`: verify the new model first, then switch settings and delete the old one. */
export interface OllamaModelReplaceResult {
  verified: boolean;
  switched_active: boolean;
  deleted_old: boolean;
  latency_ms: number;
  detail?: string | null;
}

export interface OllamaRuntimeDetectResult {
  found: boolean;
  is_serving: boolean;
  source: "system" | "per_user_zip" | "manual";
  path: string;
  version: string;
  managed_pid?: number | null;
  managed_alive?: boolean;
  /** File-system truth: the selected target version's runtime folder exists with an ollama.exe. */
  target_installed?: boolean;
}

export interface OllamaRuntimeVersionInfo {
  version: string;
  source: "pinned" | "online";
  selected: boolean;
  installed: boolean;
  recommended: boolean;
  /** Newest stable verified release in the fetched catalog. */
  latest?: boolean;
  prerelease: boolean;
  installable: boolean;
  installable_reason?: string | null;
  release_name?: string | null;
  release_notes_summary?: string | null;
  /** ISO timestamp the release was published on GitHub. */
  released_at?: string | null;
  /** Size of the Windows archive in bytes. */
  size_bytes?: number | null;
  /** When the runtime folder was installed on this machine (epoch ms). */
  installed_at_ms?: number | null;
}

/** Answer of `delete_ollama_runtime_version`: a refusal is `deleted: false` with the reason in `detail`. */
export interface OllamaRuntimeDeleteResult {
  deleted: boolean;
  freed_bytes: number;
  detail?: string | null;
}

export interface OllamaRuntimeDownloadResult {
  archive_path: string;
  sha256_ok: boolean;
  version: string;
}

export interface OllamaRuntimeInstallResult {
  runtime_path: string;
  version: string;
}

export interface OllamaRuntimeStartResult {
  pid: number | null;
  endpoint: string;
  source: "system" | "per_user_zip" | "manual";
  already_running: boolean;
  pending_start: boolean;
  startup_wait_ms: number;
}

export interface OllamaRuntimeVerifyResult {
  ok: boolean;
  endpoint: string;
  models_count: number;
}

export interface OllamaImportResult {
  model_name: string;
}

export interface OllamaRuntimeInstallProgress {
  stage: string;
  message: string;
  downloaded?: number;
  total?: number;
  version?: string;
}

export interface OllamaRuntimeInstallComplete {
  version: string;
  runtime_path: string;
}

export interface OllamaRuntimeInstallError {
  stage: string;
  error: string;
}

export interface OllamaRuntimeHealth {
  ok: boolean;
  endpoint: string;
  models_count: number;
}

export interface OllamaRuntimeDiagnostics {
  configured_path: string;
  detected: boolean;
  spawn_stage: string;
  last_error: string;
  managed_pid?: number | null;
  endpoint: string;
  reachable: boolean;
  active_requests?: number;
  idle_release_ms?: number;
}

export interface WhisperRuntimeDiagnostics {
  cli_path: string;
  server_path: string;
  backend_selected: string;
  mode: string;
  accelerator: "gpu" | "cpu" | string;
  gpu_layers_requested?: number | null;
  gpu_layers_applied?: number | null;
  active_requests?: number;
  warm_until_ms?: number;
  idle_retire_ms?: number;
  last_error: string;
}

export interface FrontendWatchdogDiagnostics {
  recovery_count: number;
  restart_count: number;
  last_recovery_reason: string;
  last_degraded_reason: string;
}

export interface RuntimeDiagnostics {
  ollama: OllamaRuntimeDiagnostics;
  whisper: WhisperRuntimeDiagnostics;
  frontend_watchdog?: FrontendWatchdogDiagnostics;
}

export interface OverlayHealthEvent {
  status: "recovering" | "recovered" | "failed";
  attempt: number;
  reason: string;
}

export interface StabilityDegradedEvent {
  reason: string;
  recoveries_in_window: number;
  restarts_in_window: number;
  restart_blocked: boolean;
}

export interface StartupStatus {
  interactive: boolean;
  transcription_ready: boolean;
  rules_ready: boolean;
  ollama_ready: boolean;
  ollama_starting: boolean;
  degraded_reasons: string[];
}

export interface PartitionInfo {
  key: string;
  label: string;
  entry_count: number;
  size_bytes: number;
  is_active: boolean;
}

export interface ModuleHealthStatus {
  module_id: string;
  state: "ok" | "needs_setup" | "fallback_active" | "local_warning" | "release_blocker" | "degraded" | "error";
  detail: string;
}

export interface ModuleUpdateInfo {
  module_id: string;
  current_version: string;
  latest_version: string;
  update_available: boolean;
}
