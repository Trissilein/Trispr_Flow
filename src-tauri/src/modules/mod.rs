pub mod health;
pub mod lifecycle;
pub mod permissions;
pub mod registry;

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::gdd::GddPresetClone;

pub const ASSISTANT_CORE_MODULE_ID: &str = "assistant_core";
pub const ASSISTANT_PRESENCE_MODULE_ID: &str = "assistant_presence";
pub const LEGACY_WORKFLOW_AGENT_MODULE_ID: &str = "workflow_agent";

pub fn canonicalize_module_id(module_id: &str) -> &str {
    match module_id.trim() {
        LEGACY_WORKFLOW_AGENT_MODULE_ID => ASSISTANT_CORE_MODULE_ID,
        other => other,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDescriptor {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: String, // "not_installed" | "installed" | "enabled" | "active" | "error"
    pub dependencies: Vec<String>,
    pub permissions: Vec<String>,
    pub restart_required: bool,
    pub last_error: Option<String>,
    pub bundled: bool,
    pub core: bool,
    pub toggleable: bool,
    pub surface: String,
    pub assistant_capable: bool,
    pub assistant_actions: Vec<AssistantActionDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantActionDescriptor {
    pub id: String,
    pub label: String,
    pub risk_level: String,
    pub requires_online: bool,
    pub allowlist_eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleHealthStatus {
    pub module_id: String,
    pub state: String, // "ok" | "degraded" | "error"
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleUpdateInfo {
    pub module_id: String,
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModuleSettings {
    pub enabled_modules: HashSet<String>,
    pub consented_permissions: HashMap<String, HashSet<String>>,
    pub module_overrides: HashMap<String, serde_json::Value>,
}

impl Default for ModuleSettings {
    fn default() -> Self {
        Self {
            enabled_modules: HashSet::new(),
            consented_permissions: HashMap::new(),
            module_overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GddModuleSettings {
    pub enabled: bool,
    pub default_preset_id: String,
    pub detect_preset_automatically: bool,
    pub prefer_one_click_publish: bool,
    pub workflow_mode_default: String, // "standard" | "advanced"
    pub transcript_source_default: String, // "runtime_session"
    pub target_routing_strategy: String, // "hybrid_memory" | "fixed" | "fresh_suggest"
    pub one_click_confidence_threshold: f32, // 0.0..1.0
    pub preset_clones: Vec<GddPresetClone>,
}

impl Default for GddModuleSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            default_preset_id: "universal_strict".to_string(),
            detect_preset_automatically: true,
            prefer_one_click_publish: false,
            workflow_mode_default: "standard".to_string(),
            transcript_source_default: "runtime_session".to_string(),
            target_routing_strategy: "hybrid_memory".to_string(),
            one_click_confidence_threshold: 0.75,
            preset_clones: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfluenceSettings {
    pub enabled: bool,
    pub site_base_url: String,
    pub oauth_cloud_id: String,
    pub default_space_key: String,
    pub api_user_email: String,
    pub default_parent_page_id: String,
    pub auth_mode: String,                       // "oauth" | "api_token"
    pub routing_memory: HashMap<String, String>, // key -> page_id
}

impl Default for ConfluenceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            site_base_url: String::new(),
            oauth_cloud_id: String::new(),
            default_space_key: String::new(),
            api_user_email: String::new(),
            default_parent_page_id: String::new(),
            auth_mode: "oauth".to_string(),
            routing_memory: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowAgentSettings {
    pub enabled: bool,
    pub wakewords: Vec<String>,
    pub wakeword_aliases: Vec<String>,
    pub intent_keywords: HashMap<String, Vec<String>>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub session_gap_minutes: u32,
    pub max_candidates: u8,
    pub hands_free_enabled: bool,
    pub confirm_timeout_sec: u16,
    pub reply_mode: String,
    pub online_enabled: bool,
    pub voice_feedback_enabled: bool,
    pub activation_mode: String,
    pub trusted_action_allowlist: Vec<String>,
    pub expert_yolo_enabled: bool,
}

impl Default for WorkflowAgentSettings {
    fn default() -> Self {
        let mut keywords = HashMap::new();
        keywords.insert(
            "gdd_generate_publish".to_string(),
            vec![
                "gdd",
                "game design document",
                "design document",
                "designdokument",
                "game design",
                "publish",
                "confluence",
                "draft",
                "generate",
                "create gdd",
                "erstelle gdd",
                "erstellen",
                "veroeffentlichen",
                "posten",
                "session",
                "meeting",
                "interview",
                "minutes",
                "zusammenfassung",
                "dokument",
                "doc",
                "spec",
                "gameplay",
                "feature",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        );

        Self {
            enabled: false,
            wakewords: vec![
                "trispr".to_string(),
                "hey trispr".to_string(),
                "trispr agent".to_string(),
            ],
            wakeword_aliases: Vec::new(),
            intent_keywords: keywords,
            model: "qwen3.5:4b".to_string(),
            temperature: 0.2,
            max_tokens: 512,
            session_gap_minutes: 20,
            max_candidates: 3,
            hands_free_enabled: false,
            confirm_timeout_sec: 45,
            reply_mode: "rule_only".to_string(),
            online_enabled: false,
            voice_feedback_enabled: false,
            activation_mode: "hotkey_first".to_string(),
            trusted_action_allowlist: Vec::new(),
            expert_yolo_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VisionInputSettings {
    pub enabled: bool,
    pub fps: u8,
    pub source_scope: String, // "all_monitors" | "active_monitor" | "active_window"
    pub max_width: u16,
    pub jpeg_quality: u8,
    pub ram_buffer_seconds: u16,
    pub all_monitors_default: bool,
}

impl Default for VisionInputSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            fps: 2,
            source_scope: "all_monitors".to_string(),
            max_width: 1280,
            jpeg_quality: 75,
            ram_buffer_seconds: 30,
            all_monitors_default: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VoiceOutputSettings {
    pub enabled: bool,
    pub default_provider: String, // "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts"
    pub fallback_provider: String, // "windows_native" | "windows_natural" | "local_custom" | "qwen3_tts"
    pub voice_id_windows: String,
    pub voice_id_windows_fallback: String,
    pub auto_voice_by_detected_language: bool,
    pub voice_id_local: String,
    pub rate: f32,   // 0.5..2.0
    pub volume: f32, // 0.0..1.0
    #[serde(default = "default_piper_gain_db")]
    pub piper_gain_db: f32, // -24.0..+6.0 (applies only to Piper)
    pub output_policy: String, // "agent_replies_only" | "replies_and_events" | "explicit_only"
    pub output_device: String, // "default" | "wasapi:<id>" (windows) | "output-<idx>-<name>" (non-windows)
    /// Full path to piper.exe. Empty = auto-resolve via PATH or %LOCALAPPDATA%\trispr-flow\piper\
    pub piper_binary_path: String,
    /// Full path to the active Piper voice model (.onnx file).
    pub piper_model_path: String,
    /// Directory to scan for available Piper voice models (.onnx files).
    pub piper_model_dir: String,
    /// OpenAI-compatible speech endpoint used by qwen3_tts runtime provider.
    pub qwen3_tts_endpoint: String,
    /// Qwen model id consumed by the qwen3_tts endpoint.
    pub qwen3_tts_model: String,
    /// Voice/speaker id for qwen3_tts endpoint.
    pub qwen3_tts_voice: String,
    /// Optional bearer token for qwen3_tts endpoint.
    pub qwen3_tts_api_key: String,
    /// Request timeout for qwen3_tts endpoint.
    pub qwen3_tts_timeout_sec: u64,
    /// Flag to enable qwen3_tts provider in UI and provider list (set by installer)
    #[serde(default)]
    pub qwen3_tts_enabled: bool,
}

impl Default for VoiceOutputSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            default_provider: "windows_native".to_string(),
            fallback_provider: "windows_native".to_string(),
            voice_id_windows: String::new(),
            voice_id_windows_fallback: String::new(),
            auto_voice_by_detected_language: false,
            voice_id_local: String::new(),
            rate: 1.0,
            volume: 1.0,
            piper_gain_db: default_piper_gain_db(),
            output_policy: "agent_replies_only".to_string(),
            output_device: "default".to_string(),
            piper_binary_path: String::new(),
            piper_model_path: String::new(),
            piper_model_dir: String::new(),
            qwen3_tts_endpoint: "http://127.0.0.1:8000/v1/audio/speech".to_string(),
            qwen3_tts_model: "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice".to_string(),
            qwen3_tts_voice: "vivian".to_string(),
            qwen3_tts_api_key: String::new(),
            qwen3_tts_timeout_sec: 45,
            qwen3_tts_enabled: false,
        }
    }
}

const fn default_piper_gain_db() -> f32 {
    -12.0
}

const REMOVED_PIPER_MODEL_KEYS: &[&str] = &["de_DE-mls-medium"];
const DEFAULT_PIPER_MODEL_KEY: &str = "de_DE-thorsten-medium";

fn is_removed_piper_model_key(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    REMOVED_PIPER_MODEL_KEYS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(trimmed))
}

pub fn normalize_module_settings(settings: &mut ModuleSettings) {
    let enabled = settings
        .enabled_modules
        .iter()
        .filter_map(|module_id| {
            let normalized = canonicalize_module_id(module_id);
            registry::find_manifest(normalized).map(|_| normalized.to_string())
        })
        .collect::<HashSet<_>>();
    settings.enabled_modules = enabled;

    let mut normalized_permissions: HashMap<String, HashSet<String>> = HashMap::new();
    for (module_id, permissions) in &settings.consented_permissions {
        let normalized_module_id = canonicalize_module_id(module_id).to_string();
        if let Some(manifest) = registry::find_manifest(&normalized_module_id) {
            let allowed = manifest
                .permissions
                .iter()
                .map(|permission| permission.to_string())
                .collect::<HashSet<_>>();
            let kept = permissions
                .iter()
                .filter(|permission| allowed.contains(*permission))
                .cloned()
                .collect::<HashSet<_>>();
            normalized_permissions
                .entry(normalized_module_id)
                .and_modify(|existing| existing.extend(kept.clone()))
                .or_insert(kept);
        }
    }
    settings.consented_permissions = normalized_permissions;

    let normalized_overrides = settings
        .module_overrides
        .iter()
        .map(|(key, value)| {
            if let Some(rest) = key.strip_prefix(&format!("{LEGACY_WORKFLOW_AGENT_MODULE_ID}.")) {
                (format!("{ASSISTANT_CORE_MODULE_ID}.{rest}"), value.clone())
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect::<HashMap<_, _>>();
    settings.module_overrides = normalized_overrides;
}

pub fn normalize_gdd_module_settings(settings: &mut GddModuleSettings) {
    // GDD is now a core capability and is always available.
    settings.enabled = true;
    if settings.default_preset_id.trim().is_empty() {
        settings.default_preset_id = "universal_strict".to_string();
    }
    settings.workflow_mode_default = match settings.workflow_mode_default.as_str() {
        "advanced" => "advanced".to_string(),
        _ => "standard".to_string(),
    };
    settings.transcript_source_default = "runtime_session".to_string();
    settings.target_routing_strategy = match settings.target_routing_strategy.as_str() {
        "fixed" => "fixed".to_string(),
        "fresh_suggest" => "fresh_suggest".to_string(),
        _ => "hybrid_memory".to_string(),
    };
    if !settings.one_click_confidence_threshold.is_finite() {
        settings.one_click_confidence_threshold = 0.75;
    }
    settings.one_click_confidence_threshold =
        settings.one_click_confidence_threshold.clamp(0.0, 1.0);
    settings
        .preset_clones
        .retain(|preset| !preset.id.trim().is_empty());
}

pub fn normalize_confluence_settings(settings: &mut ConfluenceSettings) {
    settings.site_base_url = settings
        .site_base_url
        .trim()
        .trim_end_matches('/')
        .to_string();
    settings.oauth_cloud_id = settings.oauth_cloud_id.trim().to_string();
    settings.default_space_key = settings.default_space_key.trim().to_string();
    settings.api_user_email = settings.api_user_email.trim().to_string();
    settings.default_parent_page_id = settings.default_parent_page_id.trim().to_string();
    settings.auth_mode = match settings.auth_mode.as_str() {
        "api_token" => "api_token".to_string(),
        _ => "oauth".to_string(),
    };
    settings
        .routing_memory
        .retain(|key, value| !key.trim().is_empty() && !value.trim().is_empty());
}

pub fn normalize_workflow_agent_settings(settings: &mut WorkflowAgentSettings) {
    settings.wakewords = settings
        .wakewords
        .iter()
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .collect();
    settings.wakewords.sort();
    settings.wakewords.dedup();
    if settings.wakewords.is_empty() {
        settings.wakewords = WorkflowAgentSettings::default().wakewords;
    }
    settings.wakeword_aliases = settings
        .wakeword_aliases
        .iter()
        .map(|word| word.trim().to_lowercase())
        .filter(|word| !word.is_empty())
        .collect();
    settings.wakeword_aliases.sort();
    settings.wakeword_aliases.dedup();
    settings.model = settings.model.trim().to_string();
    if settings.model.is_empty() {
        settings.model = "qwen3.5:4b".to_string();
    }
    if !settings.temperature.is_finite() {
        settings.temperature = 0.2;
    }
    settings.temperature = settings.temperature.clamp(0.0, 1.0);
    settings.max_tokens = settings.max_tokens.clamp(128, 4096);
    settings.session_gap_minutes = settings.session_gap_minutes.clamp(5, 240);
    settings.max_candidates = settings.max_candidates.clamp(1, 5);
    settings.confirm_timeout_sec = settings.confirm_timeout_sec.clamp(10, 300);
    settings.reply_mode = match settings.reply_mode.as_str() {
        "hybrid_local_llm" => "hybrid_local_llm".to_string(),
        _ => "rule_only".to_string(),
    };
    settings.activation_mode = match settings.activation_mode.as_str() {
        "wakeword_optional" => "wakeword_optional".to_string(),
        _ => "hotkey_first".to_string(),
    };
    settings.trusted_action_allowlist = settings
        .trusted_action_allowlist
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    settings.trusted_action_allowlist.sort();
    settings.trusted_action_allowlist.dedup();

    let defaults = WorkflowAgentSettings::default().intent_keywords;
    if settings.intent_keywords.is_empty() {
        settings.intent_keywords = defaults;
        return;
    }

    settings.intent_keywords.retain(|intent, words| {
        if intent.trim().is_empty() {
            return false;
        }
        words.retain(|word| !word.trim().is_empty());
        !words.is_empty()
    });
    for (intent, words) in defaults {
        settings.intent_keywords.entry(intent).or_insert(words);
    }
}

pub fn normalize_vision_input_settings(settings: &mut VisionInputSettings) {
    settings.fps = settings.fps.clamp(1, 10);
    settings.source_scope = match settings.source_scope.as_str() {
        "active_monitor" => "active_monitor".to_string(),
        "active_window" => "active_window".to_string(),
        _ => "all_monitors".to_string(),
    };
    settings.max_width = settings.max_width.clamp(640, 3840);
    settings.jpeg_quality = settings.jpeg_quality.clamp(40, 95);
    settings.ram_buffer_seconds = settings.ram_buffer_seconds.clamp(5, 120);
}

pub fn normalize_voice_output_settings(settings: &mut VoiceOutputSettings) {
    settings.default_provider = match settings.default_provider.as_str() {
        "windows_natural" => "windows_natural".to_string(),
        "local_custom" => "local_custom".to_string(),
        "qwen3_tts" => "qwen3_tts".to_string(),
        _ => "windows_native".to_string(),
    };
    settings.fallback_provider = match settings.fallback_provider.as_str() {
        "windows_natural" => "windows_natural".to_string(),
        "local_custom" => "local_custom".to_string(),
        "qwen3_tts" => "qwen3_tts".to_string(),
        _ => "windows_native".to_string(),
    };
    if !settings.rate.is_finite() {
        settings.rate = 1.0;
    }
    settings.rate = settings.rate.clamp(0.5, 2.0);
    if !settings.volume.is_finite() {
        settings.volume = 1.0;
    }
    settings.volume = settings.volume.clamp(0.0, 1.0);
    if !settings.piper_gain_db.is_finite() {
        settings.piper_gain_db = default_piper_gain_db();
    }
    settings.piper_gain_db = settings.piper_gain_db.clamp(-24.0, 6.0);
    settings.output_policy = match settings.output_policy.as_str() {
        "replies_and_events" => "replies_and_events".to_string(),
        "explicit_only" => "explicit_only".to_string(),
        _ => "agent_replies_only".to_string(),
    };
    settings.output_device = settings.output_device.trim().to_string();
    if settings.output_device.is_empty() {
        settings.output_device = "default".to_string();
    }
    #[cfg(target_os = "windows")]
    if settings.output_device != "default" && !settings.output_device.starts_with("wasapi:") {
        settings.output_device = "default".to_string();
    }
    #[cfg(not(target_os = "windows"))]
    if settings.output_device != "default" && !settings.output_device.starts_with("output-") {
        settings.output_device = "default".to_string();
    }

    settings.qwen3_tts_endpoint = settings.qwen3_tts_endpoint.trim().to_string();
    if settings.qwen3_tts_endpoint.is_empty() {
        settings.qwen3_tts_endpoint = "http://127.0.0.1:8000/v1/audio/speech".to_string();
    }
    settings.qwen3_tts_model = settings.qwen3_tts_model.trim().to_string();
    if settings.qwen3_tts_model.is_empty() {
        settings.qwen3_tts_model = "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice".to_string();
    }
    settings.qwen3_tts_voice = settings.qwen3_tts_voice.trim().to_string();
    if settings.qwen3_tts_voice.is_empty() {
        settings.qwen3_tts_voice = "vivian".to_string();
    }
    settings.qwen3_tts_api_key = settings.qwen3_tts_api_key.trim().to_string();
    settings.qwen3_tts_timeout_sec = settings.qwen3_tts_timeout_sec.clamp(3, 180);

    settings.piper_model_path = settings.piper_model_path.trim().to_string();
    if is_removed_piper_model_key(&settings.piper_model_path) {
        settings.piper_model_path = DEFAULT_PIPER_MODEL_KEY.to_string();
    }
    settings.voice_id_local = settings.voice_id_local.trim().to_string();
    if is_removed_piper_model_key(&settings.voice_id_local) {
        settings.voice_id_local = DEFAULT_PIPER_MODEL_KEY.to_string();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoGenerationSettings {
    pub enabled: bool,
    /// Output directory for rendered MP4 files. Empty = resolved via paths::resolve_video_output_dir().
    pub output_dir: String,
    /// Target resolution for new jobs: "1920x1080" | "1080x1920" | "1080x1080".
    pub default_resolution: String,
    /// Target frame rate for new jobs: 30 | 60.
    pub default_fps: u32,
    /// Default composition style: "caption" | "slideshow" | "diagram" | "game_viz".
    pub default_style: String,
    /// TTS provider for voiceover: "none" | "piper" | "windows_native".
    pub tts_provider: String,
    /// Full path to node.exe. Empty = resolved from bundled bin/node/.
    pub node_binary_path: String,
    /// Working directory for hyperframes CLI. Empty = resolved from bundled bin/hyperframes/.
    pub hyperframes_cwd: String,
    /// Upper bound (MB) for a single file accepted via drag & drop.
    pub max_upload_mb: u32,
    /// Ask hyperframes to use GPU-accelerated FFmpeg encoding (NVENC on
    /// NVIDIA, AMF on AMD, QSV on Intel). Skips CPU libx264 and dramatically
    /// reduces encode-phase CPU load. Driver support required.
    pub gpu_encoding: bool,
    /// Quality preset passed to hyperframes render: "draft" | "standard" | "high".
    /// Lower quality = faster render, smaller file. "draft" is ~3x faster
    /// than "standard" and good enough for iteration.
    pub render_quality: String,
    /// Parallel Chrome workers for frame capture. 0 = hyperframes default
    /// ("auto" = ~50% of CPU cores). Lowering this cuts CPU + RAM at the
    /// cost of slower renders. Each worker is roughly 250 MB RAM.
    pub render_workers: u32,
}

impl Default for VideoGenerationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            output_dir: String::new(),
            default_resolution: "1920x1080".to_string(),
            default_fps: 30,
            default_style: "slideshow".to_string(),
            tts_provider: "none".to_string(),
            node_binary_path: String::new(),
            hyperframes_cwd: String::new(),
            max_upload_mb: 500,
            gpu_encoding: false,
            render_quality: "standard".to_string(),
            render_workers: 0,
        }
    }
}

pub fn normalize_video_generation_settings(settings: &mut VideoGenerationSettings) {
    settings.output_dir = settings.output_dir.trim().to_string();
    settings.default_resolution = match settings.default_resolution.as_str() {
        "1080x1920" => "1080x1920".to_string(),
        "1080x1080" => "1080x1080".to_string(),
        _ => "1920x1080".to_string(),
    };
    settings.default_fps = match settings.default_fps {
        60 => 60,
        _ => 30,
    };
    settings.default_style = match settings.default_style.as_str() {
        "caption" => "caption".to_string(),
        "diagram" => "diagram".to_string(),
        "game_viz" => "game_viz".to_string(),
        _ => "slideshow".to_string(),
    };
    settings.tts_provider = match settings.tts_provider.as_str() {
        "piper" => "piper".to_string(),
        "windows_native" => "windows_native".to_string(),
        _ => "none".to_string(),
    };
    settings.node_binary_path = settings.node_binary_path.trim().to_string();
    settings.hyperframes_cwd = settings.hyperframes_cwd.trim().to_string();
    settings.max_upload_mb = settings.max_upload_mb.clamp(10, 4096);
    settings.render_quality = match settings.render_quality.as_str() {
        "draft" => "draft".to_string(),
        "high" => "high".to_string(),
        _ => "standard".to_string(),
    };
    // 0 means "use hyperframes default (auto)"; otherwise clamp to a sane range.
    if settings.render_workers != 0 {
        settings.render_workers = settings.render_workers.clamp(1, 16);
    }
}
