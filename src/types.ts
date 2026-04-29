// Type definitions for Trispr Flow application

export type LocalAIFallbackProvider = "ollama" | "lm_studio" | "oobabooga";
export type AIFallbackProvider = "claude" | "openai" | "gemini" | LocalAIFallbackProvider;
export type CloudAIFallbackProvider = Exclude<AIFallbackProvider, LocalAIFallbackProvider>;
export type AIExecutionMode = "local_primary" | "online_fallback";
export type ProductMode = "transcribe" | "assistant";
export type AIProviderAuthStatus = "locked" | "verified_api_key" | "verified_oauth";
export type AIProviderAuthMethodPreference = "api_key" | "oauth";
export type OverlayRefiningIndicatorPreset = "subtle" | "standard" | "intense";
export type ModuleSurface = "assistant" | "transcription" | "shared" | "ui";
export type AssistantActionRisk = "low" | "medium" | "high";
export type ModuleId =
  | "gdd"
  | "analysis"
  | "ai_refinement"
  | "integrations_confluence"
  | "assistant_core"
  | "assistant_presence"
  | "workflow_agent"
  | "input_vision"
  | "output_voice_tts";
export type ModuleState = "not_installed" | "installed" | "enabled" | "active" | "error";
export type ModulePermission =
  | "network_confluence"
  | "filesystem_history"
  | "filesystem_exports"
  | "keyring_access"
  | "screen_capture"
  | "audio_output";
export type AgentIntent =
  | "gdd_generate_publish"
  | "session_recap"
  | "plan_status"
  | "confirm_or_cancel"
  | "web_search"
  | "open_module"
  | "open_app"
  | "unknown";
export type AssistantOrchestratorState =
  | "idle"
  | "listening"
  | "parsing"
  | "planning"
  | "awaiting_confirm"
  | "executing"
  | "recovering";
export type AgentTargetLanguage =
  | "source"
  | "en"
  | "de"
  | "fr"
  | "es"
  | "it"
  | "pt"
  | "nl"
  | "pl"
  | "ru"
  | "ja"
  | "ko"
  | "zh"
  | "ar"
  | "tr"
  | "hi";
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
  surface?: ModuleSurface;
  assistant_capable?: boolean;
  assistant_actions?: AssistantActionDescriptor[];
}

export interface AssistantActionDescriptor {
  id: string;
  label: string;
  risk_level: AssistantActionRisk;
  requires_online: boolean;
  allowlist_eligible: boolean;
}

export interface ModuleSettings {
  enabled_modules: string[];
  consented_permissions: Record<string, string[]>;
  module_overrides: Record<string, unknown>;
}

export interface GddPresetSection {
  id: string;
  title: string;
  required: boolean;
}

export interface GddPreset {
  id: string;
  name: string;
  description: string;
  is_clone: boolean;
  base_preset_id?: string | null;
  detail_level: string;
  tone: string;
  keywords: string[];
  sections: GddPresetSection[];
}

export interface GddPresetClone {
  id: string;
  name: string;
  detail_level: string;
  tone: string;
  keywords: string[];
  section_order: string[];
  required_sections: string[];
}

export interface GddRecognitionCandidate {
  preset_id: string;
  label: string;
  score: number;
}

export interface GddRecognitionResult {
  suggested_preset_id: string;
  confidence: number;
  candidates: GddRecognitionCandidate[];
  reasoning_snippets: string[];
}

export interface GddSectionDraft {
  id: string;
  title: string;
  content: string;
  evidence_gap: boolean;
}

export interface GddDraft {
  preset_id: string;
  title: string;
  summary: string;
  sections: GddSectionDraft[];
  chunk_count: number;
  generated_at_iso: string;
}

export interface GddTemplateSourceResult {
  source_kind: "confluence" | "file" | string;
  source_label: string;
  source_ref: string;
  text: string;
  original_chars: number;
  truncated: boolean;
}

export interface GenerateGddDraftRequest {
  transcript: string;
  preset_id?: string | null;
  title?: string | null;
  max_chunk_chars?: number | null;
  template_hint?: string | null;
  template_label?: string | null;
}

export interface GddPublishRequest {
  title: string;
  storage_body: string;
  space_key: string;
  parent_page_id?: string | null;
  target_page_id?: string | null;
}

export interface GddPublishResult {
  page_id: string;
  page_url: string;
  created: boolean;
  version: number;
  message: string;
}

export interface GddPublishFallbackBundle {
  bundle_dir: string;
  draft_json_path: string;
  markdown_path: string;
  confluence_html_path: string;
  publish_request_path: string;
  manifest_path: string;
}

export interface GddPendingPublishJob {
  job_id: string;
  title: string;
  space_key: string;
  parent_page_id?: string | null;
  target_page_id?: string | null;
  created_at_iso: string;
  updated_at_iso: string;
  retry_count: number;
  last_error: string;
  bundle_dir: string;
  routing_confidence?: number | null;
  routing_reasoning?: string | null;
}

export interface GddPublishAttemptResult {
  status: "published" | "queued" | "failed";
  publish_result?: GddPublishResult;
  queued_job?: GddPendingPublishJob;
  error?: string;
}

export interface GddModuleSettings {
  enabled: boolean;
  default_preset_id: string;
  detect_preset_automatically: boolean;
  prefer_one_click_publish: boolean;
  workflow_mode_default: "standard" | "advanced";
  transcript_source_default: "runtime_session";
  target_routing_strategy: "hybrid_memory" | "fixed" | "fresh_suggest";
  one_click_confidence_threshold: number;
  preset_clones: GddPresetClone[];
}

export interface ConfluenceSettings {
  enabled: boolean;
  site_base_url: string;
  oauth_cloud_id: string;
  default_space_key: string;
  api_user_email: string;
  default_parent_page_id: string;
  auth_mode: "oauth" | "api_token";
  routing_memory: Record<string, string>;
}

export interface WorkflowAgentSettings {
  enabled: boolean;
  wakewords: string[];
  wakeword_aliases?: string[];
  intent_keywords: Record<string, string[]>;
  model: string;
  temperature: number;
  max_tokens: number;
  session_gap_minutes: number;
  max_candidates: number;
  hands_free_enabled?: boolean;
  confirm_timeout_sec?: number;
  reply_mode?: "rule_only" | "hybrid_local_llm";
  online_enabled?: boolean;
  voice_feedback_enabled?: boolean;
  activation_mode?: "hotkey_first" | "wakeword_optional";
  trusted_action_allowlist?: string[];
  expert_yolo_enabled?: boolean;
}

export interface VisionInputSettings {
  enabled: boolean;
  fps: number;
  source_scope: "all_monitors" | "active_monitor" | "active_window";
  max_width: number;
  jpeg_quality: number;
  ram_buffer_seconds: number;
  all_monitors_default: boolean;
}

export interface VoiceOutputSettings {
  enabled: boolean;
  default_provider: "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
  fallback_provider: "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
  voice_id_windows: string;
  voice_id_windows_fallback?: string;
  auto_voice_by_detected_language?: boolean;
  voice_id_local: string;
  rate: number;
  volume: number;
  piper_gain_db?: number;
  output_policy: "agent_replies_only" | "replies_and_events" | "explicit_only";
  output_device: string;
  piper_binary_path?: string;
  piper_model_path?: string;
  piper_model_dir?: string;
  qwen3_tts_endpoint?: string;
  qwen3_tts_model?: string;
  qwen3_tts_voice?: string;
  qwen3_tts_api_key?: string;
  qwen3_tts_timeout_sec?: number;
  qwen3_tts_enabled?: boolean;
}

export interface AgentCommandParseResult {
  detected: boolean;
  intent: AgentIntent;
  confidence: number;
  publish_requested: boolean;
  wakeword_matched: boolean;
  temporal_hint?: string | null;
  topic_hint?: string | null;
  reasoning: string;
  command_text: string;
}

export interface TranscriptSessionCandidate {
  session_id: string;
  start_ms: number;
  end_ms: number;
  entry_count: number;
  source_mix: string[];
  preview: string;
  score: number;
  reasoning: string;
}

export interface AgentExecutionStep {
  id: string;
  title: string;
  status: "pending" | "running" | "done" | "failed";
  detail?: string;
}

export interface AgentExecutionPlan {
  intent: AgentIntent;
  session_id: string;
  session_title: string;
  target_language: AgentTargetLanguage;
  publish: boolean;
  analysis_steps: AgentExecutionStep[];
  execution_steps: AgentExecutionStep[];
  steps: AgentExecutionStep[];
  recognized_signals: string[];
  assumptions: string[];
  proposed_actions: string[];
  summary: string;
}

export interface AgentBuildExecutionPlanRequest {
  intent: AgentIntent | string;
  session_id: string;
  target_language: AgentTargetLanguage | string;
  publish: boolean;
  command_text?: string | null;
  temporal_hint?: string | null;
  topic_hint?: string | null;
  parse_confidence?: number | null;
}

export interface AgentExecutionResult {
  status: "completed" | "queued" | "failed" | "cancelled";
  message: string;
  draft?: GddDraft;
  publish_result?: GddPublishResult;
  queued_job?: GddPendingPublishJob;
  error?: string;
}

export interface AgentReplyResult {
  text: string;
  source: "rule" | "local_llm";
  reason_code: string;
}

export interface AssistantCapabilitySnapshot {
  product_mode: ProductMode;
  assistant_mode: boolean;
  assistant_core_available?: boolean;
  workflow_agent_available?: boolean;
  tts_available: boolean;
  vision_available: boolean;
  degraded: boolean;
  hard_blocked: boolean;
  missing_capabilities: string[];
}

export interface AssistantStateChangedEvent {
  state: AssistantOrchestratorState;
  previous_state: AssistantOrchestratorState;
  reason: string;
  transition_id: number;
  changed_at_ms: number;
  capability: AssistantCapabilitySnapshot;
}

export interface AssistantPlanReadyEvent {
  state: AssistantOrchestratorState;
  reason: string;
  plan: AgentExecutionPlan;
  capability: AssistantCapabilitySnapshot;
}

export interface AssistantIntentDetectedEvent {
  state: AssistantOrchestratorState;
  reason: string;
  parse: AgentCommandParseResult;
  capability: AssistantCapabilitySnapshot;
}

export interface AssistantAwaitingConfirmationEvent {
  state: AssistantOrchestratorState;
  reason: string;
  plan: AgentExecutionPlan;
  confirm_token: string;
  confirm_timeout_sec: number;
  expires_at_ms: number;
  capability: AssistantCapabilitySnapshot;
}

export interface AssistantConfirmationExpiredEvent {
  state: AssistantOrchestratorState;
  reason: string;
  expired_at_ms: number;
  capability: AssistantCapabilitySnapshot;
}

export interface AssistantActionResultEvent {
  state: AssistantOrchestratorState;
  reason: string;
  result: AgentExecutionResult;
  capability: AssistantCapabilitySnapshot;
}

export interface AssistantReplyDraftEvent {
  text: string;
  reason: string;
  intent?: AgentIntent | string | null;
}

export interface AssistantReplyFinalEvent {
  text: string;
  reason: string;
  intent?: AgentIntent | string | null;
  source?: string | null;
}

export interface AssistantModuleOpenEvent {
  target: string;
  reason: string;
}

export interface VisionSourceInfo {
  id: string;
  label: string;
  width: number;
  height: number;
}

export interface VisionStreamHealth {
  running: boolean;
  fps: number;
  source_scope: string;
  started_at_ms?: number | null;
  frame_seq: number;
  buffered_frames: number;
  buffered_bytes: number;
  last_frame_timestamp_ms?: number | null;
  last_frame_width?: number | null;
  last_frame_height?: number | null;
}

export interface VisionSnapshotResult {
  captured: boolean;
  timestamp_ms: number;
  source_count: number;
  note: string;
  frame_seq?: number | null;
  width?: number | null;
  height?: number | null;
  bytes?: number | null;
  source_scope?: string | null;
  jpeg_base64?: string | null;
}

export interface TtsProviderInfo {
  id: "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
  label: string;
  available: boolean;
  surface: "runtime_stable" | "benchmark_experimental";
  reason?: string | null;
}

export interface TtsVoiceInfo {
  id: string;
  label: string;
  provider: "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts";
  locale?: string | null;
  profile?: string | null;
}

export interface PiperVoiceCatalogEntry {
  key: string;
  label: string;
  locale: string | null;
  quality: "x_low" | "low" | "medium" | "high";
  installed: boolean;
  curated: boolean;
  path: string | null;
}

export interface PiperVoiceDownloadProgress {
  voice_key: string;
  stage: "started" | "downloading" | "completed" | "error";
  file_name: string;
  downloaded_bytes: number;
  total_bytes?: number | null;
  percent?: number | null;
  message?: string | null;
}

export interface TtsSpeakResult {
  provider_used: "windows_native" | "windows_natural" | "local_custom" | string;
  accepted: boolean;
  message: string;
  used_fallback?: boolean | null;
  preferred_provider?: "windows_native" | "windows_natural" | "local_custom" | string | null;
  fallback_provider?: "windows_native" | "windows_natural" | "local_custom" | string | null;
  primary_error?: string | null;
}

export interface AIFallbackSettings {
  enabled: boolean;
  provider: AIFallbackProvider;
  fallback_provider: CloudAIFallbackProvider | null;
  execution_mode: AIExecutionMode;
  strict_local_mode: boolean;
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
  fallback_endpoints?: string[];
  available_models: string[];
  preferred_model: string;
  runtime_source: "system" | "per_user_zip" | "manual";
  runtime_path: string;
  runtime_version: string;
  runtime_target_version: string;
  last_health_check: string | null;
}

export interface OpenAICompatSettings {
  endpoint: string;
  api_key: string;
  preferred_model: string;
  available_models: string[];
}

export interface AIProvidersSettings {
  claude: AIProviderSettings;
  openai: AIProviderSettings;
  gemini: AIProviderSettings;
  ollama: OllamaSettings;
  lm_studio?: OpenAICompatSettings;
  oobabooga?: OpenAICompatSettings;
}

export interface SetupSettings {
  local_ai_wizard_completed: boolean;
  local_ai_wizard_pending: boolean;
  ollama_remote_expert_opt_in: boolean;
}

export interface Settings {
  mode: "ptt" | "vad";
  product_mode: ProductMode;
  hotkey_ptt: string;
  hotkey_toggle: string;
  hotkey_tts_stop: string;
  input_device: string;
  language_mode: "auto" | "en" | "de" | "fr" | "es" | "it" | "pt" | "nl" | "pl" | "ru" | "ja" | "ko" | "zh" | "ar" | "tr" | "hi";
  language_pinned: boolean;
  model: string;
  // Legacy compatibility toggle for optional old cloud transcription path.
  cloud_fallback: boolean;
  ai_fallback: AIFallbackSettings;
  providers: AIProvidersSettings;
  setup: SetupSettings;
  module_settings?: ModuleSettings;
  gdd_module_settings?: GddModuleSettings;
  confluence_settings?: ConfluenceSettings;
  workflow_agent?: WorkflowAgentSettings;
  vision_input_settings?: VisionInputSettings;
  voice_output_settings?: VoiceOutputSettings;
  assistant_presence_enabled?: boolean;
  assistant_presence_pinned?: boolean;
  assistant_presence_window_x?: number | null;
  assistant_presence_window_y?: number | null;
  assistant_presence_window_width?: number | null;
  assistant_presence_window_height?: number | null;
  assistant_presence_window_monitor?: string | null;
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
  hotkey_product_mode_toggle?: string;
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
  overlay_tts_stop_enabled: boolean;
  overlay_tts_stop_shape: "compact" | "round" | string;
  overlay_tts_stop_color: string;
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
  postproc_llm_enabled: boolean;
  postproc_llm_provider: string;
  postproc_llm_api_key: string;
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
  main_window_start_state?: "normal" | "minimized" | "tray";
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
}

export interface TranscriptionRawResultEvent {
  text: string;
  source: string;
  timestamp_ms: number;
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

export interface OllamaRuntimeDetectResult {
  found: boolean;
  is_serving: boolean;
  source: "system" | "per_user_zip" | "manual";
  path: string;
  version: string;
  managed_pid?: number | null;
  managed_alive?: boolean;
}

export interface OllamaRuntimeVersionInfo {
  version: string;
  source: "pinned" | "online";
  selected: boolean;
  installed: boolean;
  recommended: boolean;
  installable: boolean;
  installable_reason?: string | null;
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
}

export interface WhisperRuntimeDiagnostics {
  cli_path: string;
  server_path: string;
  backend_selected: string;
  mode: string;
  accelerator: "gpu" | "cpu" | string;
  gpu_layers_requested?: number | null;
  gpu_layers_applied?: number | null;
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
  state: "ok" | "degraded" | "error";
  detail: string;
}

export interface ModuleUpdateInfo {
  module_id: string;
  current_version: string;
  latest_version: string;
  update_available: boolean;
}

export interface ConfluenceSpace {
  id: string;
  key: string;
  name: string;
}

export interface ConfluenceTargetSuggestion {
  space_key: string;
  parent_page_id?: string | null;
  existing_page_id?: string | null;
  confidence: number;
  reasoning: string;
}

export interface HardwareInfo {
  gpu_name: string;
  gpu_vram: string;
  backend_recommended: string;
  cuda_available: boolean;
  driver_version: string;
  update_url?: string | null;
}
