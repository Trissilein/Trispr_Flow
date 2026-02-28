// Type definitions for Trispr Flow application

export type AIFallbackProvider = "claude" | "openai" | "gemini" | "ollama";
export type CloudAIFallbackProvider = Exclude<AIFallbackProvider, "ollama">;
export type AIExecutionMode = "local_primary" | "online_fallback";
export type AIProviderAuthStatus = "locked" | "verified_api_key" | "verified_oauth";
export type AIProviderAuthMethodPreference = "api_key" | "oauth";
export type OverlayRefiningIndicatorPreset = "subtle" | "standard" | "intense";
export type RefinementPromptPreset =
  | "wording"
  | "summary"
  | "technical_specs"
  | "action_items"
  | "custom";

export interface AIFallbackSettings {
  enabled: boolean;
  provider: AIFallbackProvider;
  fallback_provider: CloudAIFallbackProvider | null;
  execution_mode: AIExecutionMode;
  strict_local_mode: boolean;
  model: string;
  temperature: number;
  max_tokens: number;
  low_latency_mode: boolean;
  prompt_profile: RefinementPromptPreset;
  custom_prompt_enabled: boolean;
  custom_prompt: string;
  use_default_prompt: boolean;
}

export interface AIProviderSettings {
  api_key_stored: boolean;
  auth_method_preference: AIProviderAuthMethodPreference;
  auth_status: AIProviderAuthStatus;
  auth_verified_at: string | null;
  available_models: string[];
  preferred_model: string;
}

export interface OllamaSettings {
  endpoint: string;
  available_models: string[];
  preferred_model: string;
  runtime_source: "system" | "per_user_zip" | "manual";
  runtime_path: string;
  runtime_version: string;
  last_health_check: string | null;
}

export interface AIProvidersSettings {
  claude: AIProviderSettings;
  openai: AIProviderSettings;
  gemini: AIProviderSettings;
  ollama: OllamaSettings;
}

export interface SetupSettings {
  local_ai_wizard_completed: boolean;
  local_ai_wizard_pending: boolean;
  ollama_remote_expert_opt_in: boolean;
}

export interface Settings {
  mode: "ptt" | "vad";
  hotkey_ptt: string;
  hotkey_toggle: string;
  input_device: string;
  language_mode: "auto" | "en" | "de" | "fr" | "es" | "it" | "pt" | "nl" | "pl" | "ru" | "ja" | "ko" | "zh" | "ar" | "tr" | "hi";
  language_pinned: boolean;
  model: string;
  // Legacy compatibility toggle for optional old cloud transcription path.
  cloud_fallback: boolean;
  ai_fallback: AIFallbackSettings;
  providers: AIProvidersSettings;
  setup: SetupSettings;
  audio_cues: boolean;
  audio_cues_volume: number;
  ptt_use_vad: boolean;
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
  // Post-processing settings
  postproc_enabled: boolean;
  postproc_language: string;
  postproc_punctuation_enabled: boolean;
  postproc_capitalization_enabled: boolean;
  postproc_numbers_enabled: boolean;
  postproc_custom_vocab_enabled: boolean;
  postproc_custom_vocab: Record<string, string>;
  postproc_llm_enabled: boolean;
  postproc_llm_provider: string;
  postproc_llm_api_key: string;
  postproc_llm_model: string;
  postproc_llm_prompt: string;
  // Chapter settings
  chapters_enabled?: boolean;
  chapters_show_in?: "conversation" | "all";
  chapters_method?: "silence" | "time" | "hybrid";
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
  main_window_start_state?: "normal" | "minimized" | "tray";
  // UI theming
  accent_color?: string;
}

export interface HistoryEntry {
  id: string;
  text: string;
  timestamp_ms: number;
  source: string;
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
}

export interface TranscriptionRefinementStartedEvent {
  job_id: string;
  entry_id?: string;
  source: string;
  original: string;
}

export interface TranscriptionRefinedEvent {
  job_id: string;
  entry_id?: string;
  original: string;
  refined: string;
  source: string;
  model: string;
  execution_time_ms: number;
}

export interface TranscriptionRefinementFailedEvent {
  job_id: string;
  entry_id?: string;
  source: string;
  original?: string;
  error: string;
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

export interface OllamaRuntimeDetectResult {
  found: boolean;
  is_serving: boolean;
  source: "system" | "per_user_zip" | "manual";
  path: string;
  version: string;
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
