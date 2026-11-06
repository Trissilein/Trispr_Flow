use crate::constants::TARGET_SAMPLE_RATE;
use crate::models::resolve_model_path;
use crate::paths::{resolve_whisper_cli_path_for_backend, resolve_whisper_server_path_for_backend};
use crate::state::{AppState, Settings};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tracing::info;
use tracing::warn;

const TRANSCRIPTION_ACCEL_UNKNOWN: u8 = 0;
const TRANSCRIPTION_ACCEL_CPU: u8 = 1;
const TRANSCRIPTION_ACCEL_GPU: u8 = 2;
static LAST_TRANSCRIPTION_ACCELERATOR: AtomicU8 = AtomicU8::new(TRANSCRIPTION_ACCEL_UNKNOWN);
static LAST_TRANSCRIPTION_TIMING: OnceLock<Mutex<TranscriptionTimingSummary>> = OnceLock::new();
static CUDA_BACKEND_UNSTABLE: AtomicBool = AtomicBool::new(false);
static VULKAN_BACKEND_UNSTABLE: AtomicBool = AtomicBool::new(false);
pub(crate) const CUDA_RUNTIME_REQUIRED_FILES: &[&str] = &[
    "whisper-cli.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll",
    "ggml-cuda.dll",
    "cublas64_13.dll",
    "cublasLt64_13.dll",
    "cudart64_13.dll",
];
pub(crate) const VULKAN_RUNTIME_REQUIRED_FILES: &[&str] = &[
    "whisper-cli.exe",
    "whisper.dll",
    "ggml.dll",
    "ggml-base.dll",
    "ggml-cpu.dll",
    "ggml-vulkan.dll",
];

pub(crate) fn last_transcription_accelerator() -> &'static str {
    match LAST_TRANSCRIPTION_ACCELERATOR.load(Ordering::Relaxed) {
        TRANSCRIPTION_ACCEL_GPU => "gpu",
        TRANSCRIPTION_ACCEL_CPU => "cpu",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TranscriptionTimingSummary {
    pub(crate) whisper_path: String,
    pub(crate) backend: String,
    pub(crate) accelerator: String,
    pub(crate) language_pinned: bool,
    pub(crate) language_mode: String,
    pub(crate) model_class: String,
    pub(crate) model_path: String,
    pub(crate) model_drive: String,
    pub(crate) runtime_path: String,
    pub(crate) runtime_drive: String,
    pub(crate) ping_ms: Option<u64>,
    pub(crate) cold_server_start_ms: Option<u64>,
    pub(crate) warm_server_inference_ms: Option<u64>,
    pub(crate) cli_gpu_inference_ms: Option<u64>,
    pub(crate) cli_cpu_fallback_ms: Option<u64>,
    pub(crate) pipeline_overhead_ms: Option<u64>,
}

impl Default for TranscriptionTimingSummary {
    fn default() -> Self {
        Self {
            whisper_path: "unknown".to_string(),
            backend: "unknown".to_string(),
            accelerator: "unknown".to_string(),
            language_pinned: false,
            language_mode: "auto".to_string(),
            model_class: String::new(),
            model_path: String::new(),
            model_drive: String::new(),
            runtime_path: String::new(),
            runtime_drive: String::new(),
            ping_ms: None,
            cold_server_start_ms: None,
            warm_server_inference_ms: None,
            cli_gpu_inference_ms: None,
            cli_cpu_fallback_ms: None,
            pipeline_overhead_ms: None,
        }
    }
}

fn record_transcription_timing(summary: TranscriptionTimingSummary) {
    if let Ok(mut guard) = LAST_TRANSCRIPTION_TIMING
        .get_or_init(|| Mutex::new(TranscriptionTimingSummary::default()))
        .lock()
    {
        *guard = summary;
    }
}

fn reset_transcription_timing(settings: &Settings) {
    record_transcription_timing(TranscriptionTimingSummary {
        language_pinned: settings.language_pinned,
        language_mode: effective_language_mode(settings),
        model_class: settings.model.clone(),
        ..TranscriptionTimingSummary::default()
    });
}

fn effective_language_mode(settings: &Settings) -> String {
    if settings.language_pinned {
        settings.language_mode.clone()
    } else {
        "auto".to_string()
    }
}

fn path_drive_label(path: &Path) -> String {
    let rendered = path.to_string_lossy();
    let mut chars = rendered.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            format!("{}:", letter.to_ascii_uppercase())
        }
        _ => String::new(),
    }
}

fn timing_context(
    settings: &Settings,
    model_path: &Path,
    runtime_path: Option<&Path>,
) -> TranscriptionTimingSummary {
    TranscriptionTimingSummary {
        language_pinned: settings.language_pinned,
        language_mode: effective_language_mode(settings),
        model_class: settings.model.clone(),
        model_path: model_path.to_string_lossy().to_string(),
        model_drive: path_drive_label(model_path),
        runtime_path: runtime_path
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        runtime_drive: runtime_path.map(path_drive_label).unwrap_or_default(),
        cold_server_start_ms: crate::whisper_server::last_server_cold_start_ms(),
        ..TranscriptionTimingSummary::default()
    }
}

fn saturating_overhead_ms(total_ms: u64, primary_ms: u64) -> u64 {
    total_ms.saturating_sub(primary_ms)
}

fn truncate_cli_stream(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let mut result: String = trimmed.chars().take(max_chars).collect();
    result.push_str("...");
    result
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RefinementGateDecision {
    pub(crate) enabled: bool,
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) should_refine: bool,
    pub(crate) paste_deferred: bool,
    pub(crate) skipped_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ollama_model_loaded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ollama_vram_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) ollama_gpu_busy: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TranscriptionResult {
    pub(crate) text: String,
    pub(crate) source: String,
    pub(crate) job_id: String,
    pub(crate) paste_deferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) paste_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) entry_id: Option<String>,
    pub(crate) audio_duration_ms: u64,
    pub(crate) word_count: u32,
    pub(crate) refinement_gate: RefinementGateDecision,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionGpuActivityEvent {
    state: &'static str,
    accelerator: &'static str,
    backend: String,
    source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}
#[cfg(test)]
mod tests {
    use super::{
        gpu_backend_attempt_order, should_drop_transcript, whisper_runtime_preflight_issue,
        whisper_runtime_required, CUDA_BACKEND_UNSTABLE, CUDA_RUNTIME_REQUIRED_FILES,
    };
    use crate::state::Settings;
    use std::fs;
    use std::sync::atomic::Ordering;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn short_meaningful_transcript_is_not_dropped() {
        assert!(!should_drop_transcript("Bitte speichere das", 0.001, 450));
        assert!(!should_drop_transcript("das passt", 0.002, 300));
    }

    #[test]
    fn common_short_hallucination_is_dropped() {
        assert!(should_drop_transcript("thank you", 0.002, 500));
        assert!(should_drop_transcript("uh", 0.001, 400));
    }

    #[test]
    fn gpu_backend_attempt_order_default_is_cuda_then_vulkan() {
        CUDA_BACKEND_UNSTABLE.store(false, Ordering::Relaxed);
        let settings = Settings::default();
        assert_eq!(gpu_backend_attempt_order(&settings), vec!["cuda", "vulkan"]);
    }

    #[test]
    fn gpu_backend_attempt_order_vulkan_stays_vulkan_only() {
        let mut settings = Settings::default();
        settings.local_backend_preference = "vulkan".to_string();
        assert_eq!(gpu_backend_attempt_order(&settings), vec!["vulkan"]);
    }

    #[test]
    fn whisper_runtime_required_for_capture() {
        let mut settings = Settings::default();
        settings.capture_enabled = false;
        assert!(!whisper_runtime_required(&settings));

        settings.capture_enabled = true;
        assert!(whisper_runtime_required(&settings));
    }

    #[test]
    fn cuda_runtime_preflight_detects_missing_cublaslt() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("trispr-flow-cuda-preflight-{unique}"));
        let cuda_dir = base.join("cuda");
        fs::create_dir_all(&cuda_dir).expect("create cuda test dir");

        for file in CUDA_RUNTIME_REQUIRED_FILES {
            if *file == "cublasLt64_13.dll" {
                continue;
            }
            fs::write(cuda_dir.join(file), b"").expect("write placeholder runtime file");
        }

        let issue = whisper_runtime_preflight_issue(&cuda_dir.join("whisper-cli.exe"));
        assert!(issue.is_some(), "expected missing CUDA runtime issue");
        let message = issue.unwrap_or_default();
        assert!(
            message.contains("cublasLt64_13.dll"),
            "expected missing cublasLt64_13.dll in preflight issue, got: {message}"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn cuda_runtime_preflight_passes_when_required_files_exist() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let base = std::env::temp_dir().join(format!("trispr-flow-cuda-preflight-ok-{unique}"));
        let cuda_dir = base.join("cuda");
        fs::create_dir_all(&cuda_dir).expect("create cuda test dir");

        for file in CUDA_RUNTIME_REQUIRED_FILES {
            fs::write(cuda_dir.join(file), b"").expect("write placeholder runtime file");
        }

        let issue = whisper_runtime_preflight_issue(&cuda_dir.join("whisper-cli.exe"));
        assert!(issue.is_none(), "expected no CUDA runtime preflight issue");

        let _ = fs::remove_dir_all(&base);
    }
}

pub(crate) fn whisper_runtime_required(settings: &Settings) -> bool {
    settings.capture_enabled
}

pub(crate) fn rms_i16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for &sample in samples {
        let value = sample as f32 / i16::MAX as f32;
        sum += value * value;
    }
    (sum / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

fn normalize_transcript(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drop-filter for transcribed text: drops a known hallucination phrase only when the
/// captured audio segment is very short (<= HALLUCINATION_MAX_DURATION_MS). This
/// preserves genuine short dictations like "Stop" or "OK Google".
pub(crate) fn should_drop_transcript(text: &str, _rms: f32, duration_ms: u64) -> bool {
    let normalized = normalize_transcript(text);
    if normalized.is_empty() {
        return true;
    }

    let matches_common = HALLUCINATION_PHRASES.iter().any(|p| *p == normalized);
    let is_short_audio = duration_ms <= crate::constants::HALLUCINATION_MAX_DURATION_MS;
    matches_common && is_short_audio
}

pub(crate) fn should_drop_by_activation_words(
    text: &str,
    activation_words: &[String],
    enabled: bool,
) -> bool {
    if !enabled || activation_words.is_empty() {
        return false; // Don't drop
    }

    let normalized_text = normalize_transcript(text);
    let words: Vec<&str> = normalized_text.split_whitespace().collect();

    // Check if any activation word exists as complete word
    for activation_word in activation_words {
        for word in &words {
            if *word == activation_word.as_str() {
                return false; // Found activation word, don't drop
            }
        }
    }

    true // No activation word found, drop
}

const HALLUCINATION_PHRASES: &[&str] = &[
    // Filler sounds / acknowledgements
    "uh",
    "um",
    "hmm",
    "huh",
    "ah",
    "oh",
    "uh huh",
    // Single-word reactions
    "yes",
    "no",
    "okay",
    "ok",
    "yeah",
    "right",
    "sure",
    "fine",
    "good",
    "great",
    "nice",
    "wow",
    "cool",
    "really",
    "exactly",
    "absolutely",
    "definitely",
    "correct",
    "true",
    "hey",
    "hi",
    "hello",
    "bye",
    "goodbye",
    "welcome",
    "please",
    "wait",
    "sorry",
    // Gratitude / social phrases
    "you",
    "thank you",
    "thanks",
    // Two-word phrases common in background audio
    "all right",
    "alright",
    "oh no",
    "oh yeah",
    "oh well",
    "oh wow",
    "oh my",
    "come on",
    "go on",
    "hold on",
    "i see",
    "me too",
    "of course",
    "no no",
    "yes yes",
    "good job",
    "well done",
    "no problem",
    "no worries",
    "for sure",
    "see ya",
    "take care",
    "good luck",
    "good night",
    "good morning",
    "thats right",
    "youre right",
    "youre welcome",
    "not bad",
];

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

pub(crate) fn transcribe_audio(
    app: &AppHandle,
    settings: &Settings,
    samples: &[i16],
) -> Result<(String, String), String> {
    let wav_bytes = encode_wav_i16(samples, TARGET_SAMPLE_RATE);

    let text = transcribe_local(app, settings, &wav_bytes)?;
    Ok((text, "local".to_string()))
}

fn parse_env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        trimmed.parse::<usize>().ok()
    })
}

fn whisper_cli_looks_gpu_capable(cli_path: Option<&Path>) -> bool {
    cli_path
        .map(|path| path.to_string_lossy().to_lowercase())
        .map(|path| {
            path.contains("/cuda/")
                || path.contains("\\cuda\\")
                || path.contains("build-cuda")
                || path.contains("/vulkan/")
                || path.contains("\\vulkan\\")
                || path.contains("build-vulkan")
        })
        .unwrap_or(false)
}

fn resolve_whisper_gpu_layers(settings: &Settings) -> Option<usize> {
    // Priority: environment variable > settings > default
    parse_env_usize("TRISPR_WHISPER_GPU_LAYERS").or(settings.whisper_gpu_layers)
}

fn resolve_whisper_threads(gpu_hint: bool) -> usize {
    if let Some(explicit) = parse_env_usize("TRISPR_WHISPER_THREADS") {
        return explicit.max(1);
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    if gpu_hint {
        // GPU mode: keep CPU reserve to avoid UI stalls on Windows.
        let suggested = (cores / 2).max(2);
        return suggested.clamp(2, 8);
    }

    // CPU mode: avoid saturating all cores.
    cores.saturating_sub(1).clamp(2, 12)
}

fn whisper_cli_supports_gpu_layers(cli_path: &Path) -> bool {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<PathBuf, bool>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some(&cached) = guard.get(cli_path) {
            return cached;
        }
    }

    let result = whisper_cli_probe_gpu_layers(cli_path);

    if let Ok(mut guard) = cache.lock() {
        guard.insert(cli_path.to_path_buf(), result);
    }

    result
}

fn whisper_cli_supports_no_gpu(cli_path: &Path) -> bool {
    static CACHE: std::sync::OnceLock<Mutex<HashMap<PathBuf, bool>>> = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some(&cached) = guard.get(cli_path) {
            return cached;
        }
    }

    let result = whisper_cli_probe_no_gpu(cli_path);

    if let Ok(mut guard) = cache.lock() {
        guard.insert(cli_path.to_path_buf(), result);
    }

    result
}

fn whisper_cli_probe_no_gpu(cli_path: &Path) -> bool {
    whisper_cli_help_text(cli_path)
        .map(|help_text| help_text.contains("-ng") || help_text.contains("--no-gpu"))
        .unwrap_or(false)
}

fn whisper_cli_probe_gpu_layers(cli_path: &Path) -> bool {
    whisper_cli_help_text(cli_path)
        .map(|help_text| help_text.contains("-ngl") || help_text.contains("--gpu-layers"))
        .unwrap_or(false)
}

fn whisper_cli_help_text(cli_path: &Path) -> Option<String> {
    if let Some(issue) = whisper_runtime_preflight_issue(cli_path) {
        warn!(
            "Skipping whisper-cli arg probe for '{}' due to runtime preflight issue: {}",
            cli_path.display(),
            issue
        );
        return None;
    }

    let mut probe = Command::new(cli_path);
    #[cfg(target_os = "windows")]
    probe.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = match probe
        .arg("--help")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            warn!(
                "Failed to probe whisper-cli args for '{}': {}",
                cli_path.display(),
                err
            );
            return None;
        }
    };

    Some(
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .to_lowercase(),
    )
}

fn whisper_runtime_missing_message(detail: &str) -> String {
    format!(
        "Whisper runtime is missing or incomplete ({}). Restart Trispr Flow to download the speech engine again in the setup wizard, or reinstall Trispr Flow.",
        detail
    )
}

fn whisper_runtime_dependency_message(cli_path: &Path, err: &std::io::Error) -> String {
    format!(
        "Whisper runtime executable was found at '{}', but Windows could not load required runtime files (possible DLL dependency issue: {}). Reinstall Trispr Flow.",
        cli_path.display(),
        err
    )
}

pub(crate) fn whisper_runtime_preflight_issue(cli_path: &Path) -> Option<String> {
    let backend = whisper_backend_from_cli_path(cli_path);
    let required_files = match backend {
        "cuda" => CUDA_RUNTIME_REQUIRED_FILES,
        "vulkan" => VULKAN_RUNTIME_REQUIRED_FILES,
        _ => return None,
    };

    let runtime_dir = cli_path.parent().unwrap_or_else(|| Path::new("."));
    let mut missing_files: Vec<&str> = Vec::new();
    for file in required_files {
        if !runtime_dir.join(file).exists() {
            missing_files.push(*file);
        }
    }

    if missing_files.is_empty() {
        None
    } else {
        Some(format!(
            "Whisper {} runtime is incomplete for '{}'; missing: {}",
            backend.to_uppercase(),
            cli_path.display(),
            missing_files.join(", ")
        ))
    }
}

fn map_whisper_spawn_error(cli_path: &Path, err: std::io::Error) -> String {
    if !cli_path.exists() {
        return whisper_runtime_missing_message(&format!(
            "whisper-cli not found at '{}'",
            cli_path.display()
        ));
    }

    let code = err.raw_os_error();
    if matches!(err.kind(), ErrorKind::NotFound)
        || code == Some(2)
        || code == Some(126)
        || code == Some(193)
    {
        return whisper_runtime_dependency_message(cli_path, &err);
    }

    format!(
        "Failed to start Whisper runtime '{}': {}",
        cli_path.display(),
        err
    )
}

pub(crate) fn whisper_backend_from_cli_path(cli_path: &Path) -> &'static str {
    let lowered = cli_path.to_string_lossy().to_ascii_lowercase();
    if lowered.contains("/cuda/") || lowered.contains("\\cuda\\") || lowered.contains("build-cuda")
    {
        return "cuda";
    }
    if lowered.contains("/vulkan/")
        || lowered.contains("\\vulkan\\")
        || lowered.contains("build-vulkan")
    {
        return "vulkan";
    }
    "cpu"
}

fn whisper_stderr_indicates_gpu(stderr: &str) -> bool {
    let lowered = stderr.to_ascii_lowercase();
    lowered.contains("ggml_cuda_init")
        || lowered.contains("cuda devices")
        || lowered.contains("ggml_vulkan")
}

fn whisper_error_indicates_cuda_runtime_failure(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    lowered.contains("ggml_cuda_init")
        || lowered.contains("cuda error")
        || lowered.contains("dll dependency issue")
        || lowered.contains("cublas")
        || lowered.contains("cudart")
}

fn exit_indicates_illegal_instruction(message: &str) -> bool {
    message.contains("exit=-1073741795")
}

fn effective_cli_backend_preference(settings: &Settings) -> String {
    if let Ok(value) = std::env::var("TRISPR_LOCAL_BACKEND") {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized == "cuda" || normalized == "vulkan" {
            return normalized;
        }
    }
    let configured = settings
        .local_backend_preference
        .trim()
        .to_ascii_lowercase();
    if configured == "auto" && CUDA_BACKEND_UNSTABLE.load(Ordering::Relaxed) {
        "vulkan".to_string()
    } else {
        settings.local_backend_preference.clone()
    }
}

fn strict_backend_from_preference(settings: &Settings) -> Option<&'static str> {
    match effective_cli_backend_preference(settings).as_str() {
        "cuda" => Some("cuda"),
        "vulkan" => Some("vulkan"),
        _ => None,
    }
}

fn resolve_whisper_cli_path_for_exact_backend(backend: &str) -> Option<PathBuf> {
    let resolved = resolve_whisper_cli_path_for_backend(Some(backend))?;
    if whisper_backend_from_cli_path(resolved.as_path()).eq_ignore_ascii_case(backend) {
        Some(resolved)
    } else {
        None
    }
}

fn resolve_whisper_server_path_for_exact_backend(backend: &str) -> Option<PathBuf> {
    let resolved = resolve_whisper_server_path_for_backend(Some(backend))?;
    if whisper_backend_from_cli_path(resolved.as_path()).eq_ignore_ascii_case(backend) {
        Some(resolved)
    } else {
        None
    }
}

fn gpu_backend_attempt_order(settings: &Settings) -> Vec<&'static str> {
    let cuda_unstable = CUDA_BACKEND_UNSTABLE.load(Ordering::Relaxed);
    let vulkan_unstable = VULKAN_BACKEND_UNSTABLE.load(Ordering::Relaxed);
    match effective_cli_backend_preference(settings).as_str() {
        // Explicit Vulkan: if unstable, fall back to CUDA
        "vulkan" if vulkan_unstable => vec!["cuda"],
        "vulkan" => vec!["vulkan"],
        "cuda" => vec!["cuda", "vulkan"],
        // Auto/default: consider both unstable flags
        _ => match (cuda_unstable, vulkan_unstable) {
            (false, false) => vec!["cuda", "vulkan"],
            (true, false) => vec!["vulkan", "cuda"],
            (false, true) => vec!["cuda"],
            (true, true) => vec!["cuda", "vulkan"], // both unstable: start fresh with CUDA
        },
    }
}

fn resolve_gpu_cli_fallback_paths(settings: &Settings) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for backend in gpu_backend_attempt_order(settings) {
        if let Some(path) = resolve_whisper_cli_path_for_exact_backend(backend) {
            if let Some(issue) = whisper_runtime_preflight_issue(path.as_path()) {
                if backend == "cuda" {
                    CUDA_BACKEND_UNSTABLE.store(true, Ordering::Relaxed);
                }
                warn!(
                    "Skipping whisper backend '{}' due to runtime preflight issue: {}",
                    backend, issue
                );
                continue;
            }
            push_unique_path(&mut paths, path);
        }
    }
    paths
}

fn resolve_cpu_cli_fallback_path(settings: &Settings, attempted: &[PathBuf]) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for path in attempted {
        push_unique_path(&mut candidates, path.clone());
    }
    if let Some(path) = resolve_whisper_cli_path_for_exact_backend("cuda") {
        push_unique_path(&mut candidates, path);
    }
    if let Some(path) = resolve_whisper_cli_path_for_exact_backend("vulkan") {
        push_unique_path(&mut candidates, path);
    }
    let effective_backend = effective_cli_backend_preference(settings);
    if let Some(path) = resolve_whisper_cli_path_for_backend(Some(effective_backend.as_str())) {
        push_unique_path(&mut candidates, path);
    }
    if let Some(path) = resolve_whisper_cli_path_for_backend(Some("auto")) {
        push_unique_path(&mut candidates, path);
    }
    candidates.into_iter().find(|candidate| candidate.exists())
}

fn emit_transcription_gpu_activity(
    app: &AppHandle,
    state: &'static str,
    accelerator: &'static str,
    backend: &str,
    message: Option<String>,
) {
    let accel_code = if accelerator == "gpu" {
        TRANSCRIPTION_ACCEL_GPU
    } else {
        TRANSCRIPTION_ACCEL_CPU
    };
    LAST_TRANSCRIPTION_ACCELERATOR.store(accel_code, Ordering::Relaxed);
    let _ = app.emit(
        "transcription:gpu-activity",
        TranscriptionGpuActivityEvent {
            state,
            accelerator,
            backend: backend.to_string(),
            source: "whisper",
            message,
        },
    );
}

fn update_whisper_runtime_diagnostics(
    app: &AppHandle,
    settings: &Settings,
    mode: &str,
    accelerator: &str,
    gpu_layers_requested: Option<usize>,
    gpu_layers_applied: Option<usize>,
    last_error: Option<String>,
) {
    let strict_backend = strict_backend_from_preference(settings);
    let cli_path = strict_backend
        .and_then(resolve_whisper_cli_path_for_exact_backend)
        .or_else(|| {
            resolve_whisper_cli_path_for_backend(Some(settings.local_backend_preference.as_str()))
        });
    let server_path = strict_backend
        .and_then(resolve_whisper_server_path_for_exact_backend)
        .or_else(|| {
            resolve_whisper_server_path_for_backend(Some(
                settings.local_backend_preference.as_str(),
            ))
        });
    let backend_selected = if mode == "server" {
        server_path
            .as_deref()
            .map(whisper_backend_from_cli_path)
            .or_else(|| cli_path.as_deref().map(whisper_backend_from_cli_path))
            .or(strict_backend)
            .unwrap_or("unknown")
    } else {
        cli_path
            .as_deref()
            .map(whisper_backend_from_cli_path)
            .or(strict_backend)
            .unwrap_or("unknown")
    };

    let state = app.state::<AppState>();
    crate::update_runtime_diagnostics(app, state.inner(), |diagnostics| {
        diagnostics.whisper.cli_path = cli_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        diagnostics.whisper.server_path = server_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        diagnostics.whisper.backend_selected = backend_selected.to_string();
        diagnostics.whisper.mode = mode.to_string();
        diagnostics.whisper.accelerator = accelerator.to_string();
        diagnostics.whisper.gpu_layers_requested = gpu_layers_requested;
        diagnostics.whisper.gpu_layers_applied = gpu_layers_applied;
        diagnostics.whisper.active_requests = crate::whisper_server::active_request_count();
        diagnostics.whisper.warm_until_ms = state
            .whisper_server_warm_until_ms
            .load(std::sync::atomic::Ordering::Relaxed);
        diagnostics.whisper.idle_retire_ms = 300_000;
        diagnostics.whisper.last_error = last_error.unwrap_or_default();
    });
}

struct WhisperGpuActivityGuard {
    app: AppHandle,
    backend: String,
    accelerator: &'static str,
}

impl WhisperGpuActivityGuard {
    fn new(app: &AppHandle, accelerator: &'static str, backend: &str) -> Self {
        emit_transcription_gpu_activity(
            app,
            if accelerator == "gpu" {
                "active"
            } else {
                "cpu"
            },
            accelerator,
            backend,
            None,
        );
        Self {
            app: app.clone(),
            backend: backend.to_string(),
            accelerator,
        }
    }

    fn set_accelerator(&mut self, accelerator: &'static str) {
        if self.accelerator == accelerator {
            return;
        }
        self.accelerator = accelerator;
        emit_transcription_gpu_activity(
            &self.app,
            if accelerator == "gpu" {
                "active"
            } else {
                "cpu"
            },
            accelerator,
            &self.backend,
            None,
        );
    }
}

impl Drop for WhisperGpuActivityGuard {
    fn drop(&mut self) {
        emit_transcription_gpu_activity(&self.app, "idle", self.accelerator, &self.backend, None);
    }
}

/// RAII guard that deletes a temporary file when dropped.
/// Ensures cleanup on every early-return path and panics, not just happy path.
struct TempFileGuard {
    path: std::path::PathBuf,
}

impl TempFileGuard {
    fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// RAII guard that cleans whisper side-effect files for a given output base.
/// Keeps retry attempts isolated and prevents stale transcript reuse.
struct WhisperOutputGuard {
    output_base: std::path::PathBuf,
    wav_path: std::path::PathBuf,
}

impl WhisperOutputGuard {
    fn new(output_base: std::path::PathBuf, wav_path: std::path::PathBuf) -> Self {
        Self {
            output_base,
            wav_path,
        }
    }
}

impl Drop for WhisperOutputGuard {
    fn drop(&mut self) {
        cleanup_whisper_output_files(&self.output_base, &self.wav_path);
    }
}

fn transcribe_local(
    app: &AppHandle,
    settings: &Settings,
    wav_bytes: &[u8],
) -> Result<String, String> {
    let diagnostics_enabled = crate::state::diagnostic_logging_enabled();
    reset_transcription_timing(settings);
    let t0 = std::time::Instant::now();
    let temp_dir = std::env::temp_dir();
    let _ = fs::create_dir_all(&temp_dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = temp_dir.join(format!("trispr_{}_{}", std::process::id(), stamp));
    let wav_path = base.with_extension("wav");
    let output_base = base.clone();

    fs::write(&wav_path, wav_bytes).map_err(|e| {
        format!(
            "Failed to write temporary audio file '{}': {}",
            wav_path.display(),
            e
        )
    })?;
    if diagnostics_enabled {
        info!(
            "[TIMING] wav_write: {:.3}s ({} bytes)",
            t0.elapsed().as_secs_f32(),
            wav_bytes.len()
        );
    }
    // Guard ensures wav_path is deleted on every exit path (early returns, panic).
    let _wav_guard = TempFileGuard::new(wav_path.clone());

    let model_path = resolve_model_path(app, &settings.model).ok_or_else(|| {
        "Model file not found. Set TRISPR_WHISPER_MODEL_DIR or TRISPR_WHISPER_MODEL.".to_string()
    })?;
    let server_ping_ms: Option<u64>;

    // Try Whisper-Server first (persistent mode with pre-loaded model)
    {
        let state = app.state::<crate::state::AppState>();
        let port = state
            .whisper_server_port
            .load(std::sync::atomic::Ordering::Relaxed);

        let ping_started = std::time::Instant::now();
        let server_reachable = crate::whisper_server::ping_whisper_server(port);
        let ping_ms = ping_started.elapsed().as_millis() as u64;
        server_ping_ms = Some(ping_ms);
        if diagnostics_enabled {
            info!(
                "[TIMING] whisper_server_ping: {}ms reachable={}",
                ping_ms, server_reachable
            );
        }

        if server_reachable {
            let lang_str = effective_language_mode(settings);
            let effective_backend = effective_cli_backend_preference(settings);
            let server_path = strict_backend_from_preference(settings)
                .and_then(resolve_whisper_server_path_for_exact_backend)
                .or_else(|| {
                    resolve_whisper_server_path_for_backend(Some(effective_backend.as_str()))
                });

            if diagnostics_enabled {
                info!(
                    "[TIMING] whisper_server mode: sending {} bytes WAV",
                    wav_bytes.len()
                );
            }
            let t_server = std::time::Instant::now();

            let vocab_prompt = crate::vocab_hints::whisper_prompt(settings);
            match crate::whisper_server::transcribe_via_server(
                wav_bytes,
                port,
                &lang_str,
                vocab_prompt.as_deref(),
            ) {
                Ok(text) => {
                    let server_ms = t_server.elapsed().as_millis() as u64;
                    if diagnostics_enabled {
                        info!("[diagnostics] transcribe_via_server SUCCESS -> update(mode=server, backend=gpu, last_error=None)");
                    }
                    update_whisper_runtime_diagnostics(
                        app,
                        settings,
                        "server",
                        "gpu",
                        resolve_whisper_gpu_layers(settings),
                        resolve_whisper_gpu_layers(settings),
                        None,
                    );
                    crate::whisper_server::touch_whisper_server_recent_use(app, state.inner());
                    if diagnostics_enabled {
                        info!("[TIMING] whisper_server: {:.2}s", server_ms as f32 / 1000.0);
                    }
                    let mut summary = timing_context(settings, &model_path, server_path.as_deref());
                    summary.whisper_path = "server_warm".to_string();
                    summary.backend = server_path
                        .as_deref()
                        .map(whisper_backend_from_cli_path)
                        .unwrap_or("unknown")
                        .to_string();
                    summary.accelerator = "gpu".to_string();
                    summary.ping_ms = Some(ping_ms);
                    summary.warm_server_inference_ms = Some(server_ms);
                    summary.pipeline_overhead_ms = Some(saturating_overhead_ms(
                        t0.elapsed().as_millis() as u64,
                        server_ms,
                    ));
                    record_transcription_timing(summary);
                    return Ok(text);
                }
                Err(e) => {
                    warn!("whisper-server failed ({}), falling back to CLI", e);
                    // Inspect the captured stderr log for known fatal patterns
                    // (e.g. CUDA "no kernel image" — indicates the binary was
                    // built without this GPU's compute capability). Surface that
                    // as the diagnostic so the user sees an actionable message
                    // instead of the generic transport error.
                    let crash_hint = crate::whisper_server::inspect_recent_server_crash(app);
                    let diagnostic_error = match crash_hint.as_deref() {
                        Some(hint) => hint.to_string(),
                        None => format!("server unavailable, CLI active: {}", e),
                    };
                    if let Some(hint) = crash_hint.as_deref() {
                        warn!("whisper-server crash recognised: {}", hint);
                        // Emit to the frontend so the user sees an actionable
                        // toast right away. Frontend deduplicates by message,
                        // so we can fire on every recurrence without spamming.
                        let _ = tauri::Emitter::emit(
                            app,
                            "whisper:server-fatal",
                            serde_json::json!({ "message": hint }),
                        );
                    }
                    if diagnostics_enabled {
                        info!("[diagnostics] transcribe_via_server ERROR -> update(mode=cli, backend=cpu, last_error=\"{}\")", diagnostic_error);
                    }
                    update_whisper_runtime_diagnostics(
                        app,
                        settings,
                        "cli",
                        "cpu",
                        resolve_whisper_gpu_layers(settings),
                        None,
                        Some(diagnostic_error),
                    );
                    // Restart the server in the background so the next request
                    // can use the fast HTTP path again (server may have crashed).
                    {
                        let handle = app.clone();
                        let model_path_clone = model_path.clone();
                        crate::util::spawn_guarded(
                            "restart_whisper_server_after_crash",
                            move || {
                                let state = handle.state::<crate::state::AppState>();
                                let _ = crate::whisper_server::start_whisper_server(
                                    &handle,
                                    state.inner(),
                                    &model_path_clone,
                                );
                            },
                        );
                    }
                    // Continue to CLI fallback below
                }
            }
        } else {
            // Server is configured but unreachable. Stamp a fresh diagnostic so
            // the UI message reflects this attempt instead of a stale one. If a
            // managed child is tracked, kick a background restart so the next
            // call has a chance of using the fast HTTP path again.
            let managed_child_present = state
                .managed_whisper_server_child
                .lock()
                .map(|guard| guard.is_some())
                .unwrap_or(false);
            if managed_child_present {
                if diagnostics_enabled {
                    info!("[diagnostics] ping_whisper_server=false -> update(mode=cli, backend=cpu, last_error=\"whisper-server unreachable, restarting and using CLI fallback\")");
                }
                update_whisper_runtime_diagnostics(
                    app,
                    settings,
                    "cli",
                    "cpu",
                    resolve_whisper_gpu_layers(settings),
                    None,
                    Some(
                        "whisper-server unreachable, restarting and using CLI fallback".to_string(),
                    ),
                );
                let handle = app.clone();
                let model_path_clone = model_path.clone();
                crate::util::spawn_guarded("restart_whisper_server_after_ping_fail", move || {
                    let state = handle.state::<crate::state::AppState>();
                    let _ = crate::whisper_server::start_whisper_server(
                        &handle,
                        state.inner(),
                        &model_path_clone,
                    );
                });
            }
        }
    }

    let cli_backend_preference = effective_cli_backend_preference(settings);
    let gpu_cli_paths = resolve_gpu_cli_fallback_paths(settings);
    let mut errors: Vec<String> = Vec::new();
    let mut attempted_chain: Vec<String> = Vec::new();

    for cli_path in &gpu_cli_paths {
        let backend = whisper_backend_from_cli_path(cli_path.as_path());
        attempted_chain.push(format!("{} GPU", backend));
        let cli_started = std::time::Instant::now();
        match run_whisper_cli(
            app,
            settings,
            cli_path.as_path(),
            model_path.as_path(),
            wav_path.as_path(),
            output_base.as_path(),
            false,
        ) {
            Ok(text) => {
                let cli_ms = cli_started.elapsed().as_millis() as u64;
                if diagnostics_enabled {
                    info!(
                        "[TIMING] whisper_cli_attempt: backend={} force_cpu=false elapsed={}ms success=true",
                        backend, cli_ms
                    );
                }
                let accelerator = last_transcription_accelerator().to_string();
                let mut summary = timing_context(settings, &model_path, Some(cli_path.as_path()));
                summary.whisper_path = if accelerator == "gpu" {
                    "cli_gpu".to_string()
                } else {
                    "cli_cpu".to_string()
                };
                summary.backend = backend.to_string();
                summary.accelerator = accelerator.clone();
                summary.ping_ms = server_ping_ms;
                summary.cli_gpu_inference_ms = if accelerator == "gpu" {
                    Some(cli_ms)
                } else {
                    None
                };
                summary.cli_cpu_fallback_ms = if accelerator == "cpu" {
                    Some(cli_ms)
                } else {
                    None
                };
                summary.pipeline_overhead_ms = Some(saturating_overhead_ms(
                    t0.elapsed().as_millis() as u64,
                    cli_ms,
                ));
                record_transcription_timing(summary);
                return Ok(text);
            }
            Err(err) => {
                let cli_ms = cli_started.elapsed().as_millis() as u64;
                if diagnostics_enabled {
                    info!(
                        "[TIMING] whisper_cli_attempt: backend={} force_cpu=false elapsed={}ms success=false",
                        backend, cli_ms
                    );
                }
                if backend == "cuda" && whisper_error_indicates_cuda_runtime_failure(&err) {
                    CUDA_BACKEND_UNSTABLE.store(true, Ordering::Relaxed);
                }
                if backend == "vulkan" && exit_indicates_illegal_instruction(&err) {
                    VULKAN_BACKEND_UNSTABLE.store(true, Ordering::Relaxed);
                    warn!("Vulkan backend crashed with STATUS_ILLEGAL_INSTRUCTION — marking as unstable");
                }
                errors.push(format!(
                    "GPU backend '{}' failed ('{}'): {}",
                    backend,
                    cli_path.display(),
                    err
                ));
            }
        }
    }

    if let Some(cpu_cli_path) = resolve_cpu_cli_fallback_path(settings, &gpu_cli_paths) {
        warn!(
            "All GPU attempts failed; trying CLI CPU fallback via '{}'",
            cpu_cli_path.display()
        );
        attempted_chain.push(format!(
            "{} CLI CPU",
            whisper_backend_from_cli_path(cpu_cli_path.as_path())
        ));
        let cli_started = std::time::Instant::now();
        match run_whisper_cli(
            app,
            settings,
            cpu_cli_path.as_path(),
            model_path.as_path(),
            wav_path.as_path(),
            output_base.as_path(),
            true,
        ) {
            Ok(text) => {
                let cli_ms = cli_started.elapsed().as_millis() as u64;
                let backend = whisper_backend_from_cli_path(cpu_cli_path.as_path());
                if diagnostics_enabled {
                    info!(
                        "[TIMING] whisper_cli_attempt: backend={} force_cpu=true elapsed={}ms success=true",
                        backend, cli_ms
                    );
                }
                let mut summary =
                    timing_context(settings, &model_path, Some(cpu_cli_path.as_path()));
                summary.whisper_path = "cli_cpu".to_string();
                summary.backend = backend.to_string();
                summary.accelerator = "cpu".to_string();
                summary.ping_ms = server_ping_ms;
                summary.cli_cpu_fallback_ms = Some(cli_ms);
                summary.pipeline_overhead_ms = Some(saturating_overhead_ms(
                    t0.elapsed().as_millis() as u64,
                    cli_ms,
                ));
                record_transcription_timing(summary);
                return Ok(text);
            }
            Err(err) => {
                let cli_ms = cli_started.elapsed().as_millis() as u64;
                if diagnostics_enabled {
                    info!(
                        "[TIMING] whisper_cli_attempt: backend={} force_cpu=true elapsed={}ms success=false",
                        whisper_backend_from_cli_path(cpu_cli_path.as_path()),
                        cli_ms
                    );
                }
                errors.push(format!(
                    "CLI CPU fallback failed ('{}'): {}",
                    cpu_cli_path.display(),
                    err
                ));
            }
        }
    } else {
        let message = whisper_runtime_missing_message(&format!(
            "whisper-cli executable could not be located (backend preference '{}')",
            cli_backend_preference
        ));
        update_whisper_runtime_diagnostics(
            app,
            settings,
            "cli",
            "cpu",
            resolve_whisper_gpu_layers(settings),
            None,
            Some(message.clone()),
        );
        return Err(message);
    }

    let chain_label = if attempted_chain.is_empty() {
        "none".to_string()
    } else {
        attempted_chain.join(" -> ")
    };
    Err(format!(
        "Whisper transcription failed after fallback chain ({}): {}",
        chain_label,
        errors.join(" | ")
    ))
}

fn run_whisper_cli(
    app: &AppHandle,
    settings: &Settings,
    cli_path: &Path,
    model_path: &Path,
    wav_path: &Path,
    output_base: &Path,
    force_cpu: bool,
) -> Result<String, String> {
    let diagnostics_enabled = crate::state::diagnostic_logging_enabled();
    if let Some(issue) = whisper_runtime_preflight_issue(cli_path) {
        update_whisper_runtime_diagnostics(
            app,
            settings,
            "cli",
            "cpu",
            resolve_whisper_gpu_layers(settings),
            None,
            Some(issue.clone()),
        );
        return Err(issue);
    }

    // Ensure each run starts clean and always cleans side effects on return.
    cleanup_whisper_output_files(output_base, wav_path);
    let _output_guard = WhisperOutputGuard::new(output_base.to_path_buf(), wav_path.to_path_buf());

    let mut command = Command::new(cli_path);

    let gpu_layers = if force_cpu {
        None
    } else {
        resolve_whisper_gpu_layers(settings)
    };
    let backend_gpu_capable = whisper_cli_looks_gpu_capable(Some(cli_path));
    let gpu_hint = if force_cpu {
        false
    } else {
        gpu_layers
            .map(|layers| layers > 0)
            .unwrap_or(backend_gpu_capable)
    };
    let threads = resolve_whisper_threads(gpu_hint).to_string();

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000 | 0x00004000); // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS

    command
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(wav_path)
        .arg("-t")
        .arg(&threads)
        .arg("-l")
        .arg(if settings.language_pinned {
            &settings.language_mode
        } else {
            "auto"
        })
        .arg("-nt")
        .arg("-otxt")
        .arg("-of")
        .arg(output_base)
        .arg("-np");

    // Inject vocabulary terms as whisper-cli initial prompt. Whisper uses
    // this to bias recognition toward the listed words (proper nouns,
    // acronyms, project jargon), so they come out right on the first pass
    // instead of depending on post-processing. The server path sends the same prompt.
    if let Some(prompt) = crate::vocab_hints::whisper_prompt(settings) {
        command.arg("--prompt").arg(prompt);
    }

    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let requested_gpu_layers = gpu_layers.filter(|layers| *layers > 0);
    let mut applied_gpu_layers: Option<usize> = None;
    if let Some(layers) = requested_gpu_layers {
        if whisper_cli_supports_gpu_layers(cli_path) {
            command.arg("-ngl").arg(layers.to_string());
            applied_gpu_layers = Some(layers);
        } else {
            warn!(
                "Ignoring TRISPR_WHISPER_GPU_LAYERS={} because whisper-cli '{}' does not support -ngl/--gpu-layers.",
                layers,
                cli_path.display()
            );
        }
    }

    // Explicitly enable GPU on CUDA/Vulkan builds if detected.
    // CPU fallback mode intentionally skips this.
    if force_cpu && whisper_cli_supports_no_gpu(cli_path) {
        command.arg("-ng");
    }
    if backend_gpu_capable && !force_cpu {
        command.arg("-dev").arg("0");
    }

    let expected_gpu = if force_cpu {
        false
    } else if requested_gpu_layers.is_some() {
        applied_gpu_layers.is_some() || backend_gpu_capable
    } else {
        backend_gpu_capable
    };
    let backend = whisper_backend_from_cli_path(cli_path);
    let mut gpu_activity_guard =
        WhisperGpuActivityGuard::new(app, if expected_gpu { "gpu" } else { "cpu" }, backend);

    if diagnostics_enabled {
        info!(
            "[TIMING] whisper_spawn: model={}, gpu_layers={:?}, backend_gpu={}, force_cpu={}, threads={}",
            model_path.display(),
            gpu_layers,
            backend_gpu_capable,
            force_cpu,
            &threads
        );
    }
    let t_spawn = std::time::Instant::now();
    // Use spawn + polling instead of output() to enforce a hard timeout.
    // command.output() blocks forever if whisper-cli hangs (e.g. GPU deadlock).
    let mut child = command.spawn().map_err(|e| {
        let message = map_whisper_spawn_error(cli_path, e);
        update_whisper_runtime_diagnostics(
            app,
            settings,
            "cli",
            "cpu",
            requested_gpu_layers,
            applied_gpu_layers,
            Some(message.clone()),
        );
        message
    })?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let output = loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                break child
                    .wait_with_output()
                    .map_err(|e| format!("Failed to collect whisper-cli output: {}", e))?;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let message = format!(
                        "whisper-cli timed out after 120 seconds ('{}')",
                        cli_path.display()
                    );
                    update_whisper_runtime_diagnostics(
                        app,
                        settings,
                        "cli",
                        "cpu",
                        requested_gpu_layers,
                        applied_gpu_layers,
                        Some(message.clone()),
                    );
                    return Err(message);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(e) => {
                let message = format!("Failed to wait for whisper-cli: {}", e);
                update_whisper_runtime_diagnostics(
                    app,
                    settings,
                    "cli",
                    "cpu",
                    requested_gpu_layers,
                    applied_gpu_layers,
                    Some(message.clone()),
                );
                return Err(message);
            }
        }
    };
    if diagnostics_enabled {
        info!(
            "[TIMING] whisper_process: {:.2}s",
            t_spawn.elapsed().as_secs_f32()
        );
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_gpu = whisper_stderr_indicates_gpu(&stderr);
    if stderr_gpu {
        gpu_activity_guard.set_accelerator("gpu");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stderr.to_lowercase().contains("unknown argument:") {
        let message = format!(
            "whisper-cli argument mismatch ('{}'): {}",
            cli_path.display(),
            stderr.trim()
        );
        update_whisper_runtime_diagnostics(
            app,
            settings,
            "cli",
            "cpu",
            requested_gpu_layers,
            applied_gpu_layers,
            Some(message.clone()),
        );
        return Err(message);
    }
    if !output.status.success() {
        let exit_status = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| output.status.to_string());
        let stdout_detail = truncate_cli_stream(&stdout, 2000);
        let stderr_detail = truncate_cli_stream(&stderr, 4000);
        let details = match (stdout_detail.is_empty(), stderr_detail.is_empty()) {
            (true, true) => "no stdout/stderr".to_string(),
            (false, true) => format!("stdout: {}", stdout_detail),
            (true, false) => format!("stderr: {}", stderr_detail),
            (false, false) => format!("stderr: {} | stdout: {}", stderr_detail, stdout_detail),
        };
        let message = format!(
            "whisper-cli failed ('{}', exit={}): {}",
            cli_path.display(),
            exit_status,
            details
        );
        update_whisper_runtime_diagnostics(
            app,
            settings,
            "cli",
            if stderr_gpu { "gpu" } else { "cpu" },
            requested_gpu_layers,
            applied_gpu_layers,
            Some(message.clone()),
        );
        return Err(message);
    }

    let mut transcript_candidates: Vec<PathBuf> = Vec::new();
    let txt_path = output_base.with_extension("txt");
    push_unique_path(&mut transcript_candidates, txt_path.clone());
    push_unique_path(
        &mut transcript_candidates,
        Path::new(&format!("{}.txt", wav_path.display())).to_path_buf(),
    );
    push_unique_path(&mut transcript_candidates, wav_path.with_extension("txt"));
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = output_base.file_name().and_then(|name| name.to_str()) {
            push_unique_path(&mut transcript_candidates, cwd.join(format!("{name}.txt")));
        }
        if let Some(name) = wav_path.file_name().and_then(|name| name.to_str()) {
            push_unique_path(&mut transcript_candidates, cwd.join(format!("{name}.txt")));
        }
    }

    let mut text: Option<String> = None;
    for _ in 0..20 {
        if let Some((_, value)) = read_first_existing_text_file(&transcript_candidates) {
            text = Some(value);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let stdout_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let text = if let Some(text) = text {
        text
    } else if !stdout_text.is_empty() {
        stdout_text
    } else {
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        let expected = transcript_candidates
            .iter()
            .map(|path| format!("'{}'", path.display()))
            .collect::<Vec<_>>()
            .join(", ");
        let message = format!(
            "Whisper finished without producing transcript output. Checked: {}. whisper-cli: '{}'. stderr: {}",
            expected,
            cli_path.display(),
            stderr_text.trim()
        );
        update_whisper_runtime_diagnostics(
            app,
            settings,
            "cli",
            if stderr_gpu { "gpu" } else { "cpu" },
            requested_gpu_layers,
            applied_gpu_layers,
            Some(message.clone()),
        );
        return Err(message);
    };

    let accelerator = if stderr_gpu { "gpu" } else { "cpu" };
    // Preserve any fresher last_error already set in this call chain (e.g. by
    // the ping=false branch in transcribe_local). Only synthesize the generic
    // "server unavailable" hint when nothing more specific is set AND we
    // actually fell back to CPU despite a GPU backend being configured.
    let preserved_last_error = {
        let state = app.state::<crate::state::AppState>();
        state
            .runtime_diagnostics
            .lock()
            .map(|guard| guard.whisper.last_error.clone())
            .unwrap_or_default()
    };
    let preserved = !preserved_last_error.trim().is_empty();
    let last_error = if preserved {
        Some(preserved_last_error)
    } else if accelerator == "cpu" && backend != "cpu" {
        Some("server unavailable, CLI active".to_string())
    } else {
        None
    };
    let synthesized = accelerator == "cpu" && backend != "cpu" && !preserved;
    if diagnostics_enabled {
        info!("[diagnostics] cli_happy_path -> update(mode=cli, backend={}, last_error_preserved={}, last_error_synthesized={}, last_error={})",
            accelerator, preserved, synthesized,
            last_error.as_ref().map(|s| s.as_str()).unwrap_or("None"));
    }
    update_whisper_runtime_diagnostics(
        app,
        settings,
        "cli",
        accelerator,
        requested_gpu_layers,
        applied_gpu_layers,
        last_error,
    );

    if !force_cpu && (accelerator == "gpu" || backend != "cpu") {
        // Warm up server only when it resolves to the same backend we just used.
        // Avoids repeatedly starting CUDA server when CLI has already switched to Vulkan.
        let server_backend_matches = resolve_whisper_server_path_for_backend(Some(backend))
            .as_deref()
            .map(whisper_backend_from_cli_path)
            .map(|server_backend| server_backend == backend)
            .unwrap_or(false);
        if server_backend_matches {
            crate::whisper_server::schedule_whisper_server_warmup(
                app,
                app.state::<crate::state::AppState>().inner(),
                model_path,
                settings,
            );
        } else {
            if diagnostics_enabled {
                info!(
                    "Skipping whisper-server warmup: active backend '{}' has no matching server runtime.",
                    backend
                );
            }
        }
    }

    Ok(text.trim().to_string())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}

fn cleanup_whisper_output_files(output_base: &Path, wav_path: &Path) {
    let mut transcript_candidates: Vec<PathBuf> = Vec::new();
    push_unique_path(
        &mut transcript_candidates,
        output_base.with_extension("txt"),
    );
    push_unique_path(
        &mut transcript_candidates,
        Path::new(&format!("{}.txt", wav_path.display())).to_path_buf(),
    );
    push_unique_path(&mut transcript_candidates, wav_path.with_extension("txt"));

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = output_base.file_name().and_then(|name| name.to_str()) {
            push_unique_path(&mut transcript_candidates, cwd.join(format!("{name}.txt")));
        }
        if let Some(name) = wav_path.file_name().and_then(|name| name.to_str()) {
            push_unique_path(&mut transcript_candidates, cwd.join(format!("{name}.txt")));
        }
    }

    for path in &transcript_candidates {
        let _ = fs::remove_file(path);
    }

    for ext in &["srt", "vtt", "json", "lrc", "tsv"] {
        let _ = fs::remove_file(output_base.with_extension(ext));
        let _ = fs::remove_file(wav_path.with_extension(ext));
    }
}

fn read_first_existing_text_file(paths: &[PathBuf]) -> Option<(PathBuf, String)> {
    let mut first_non_not_found: Option<(PathBuf, std::io::Error)> = None;
    for path in paths {
        match fs::read_to_string(path) {
            Ok(content) => return Some((path.clone(), content)),
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => {
                if first_non_not_found.is_none() {
                    first_non_not_found = Some((path.clone(), err));
                }
            }
        }
    }

    if let Some((path, err)) = first_non_not_found {
        warn!(
            "Failed reading whisper transcript candidate '{}': {}",
            path.display(),
            err
        );
    }
    None
}
