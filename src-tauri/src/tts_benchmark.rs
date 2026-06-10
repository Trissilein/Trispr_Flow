use super::{now_iso, AppState};
use crate::multimodal_io::Qwen3TtsConfig;
use crate::state::Settings;
use crate::transcription::{
    last_transcription_accelerator, last_transcription_timing_summary, transcribe_audio,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tauri::{AppHandle, State};
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub(crate) struct LatencyBenchmarkRequest {
    fixture_paths: Vec<String>,
    warmup_runs: u32,
    measure_runs: u32,
    include_refinement: bool,
    refinement_model: Option<String>,
}

impl Default for LatencyBenchmarkRequest {
    fn default() -> Self {
        Self {
            fixture_paths: Vec::new(),
            warmup_runs: 5,
            measure_runs: 30,
            include_refinement: true,
            refinement_model: None,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct LatencyBenchmarkSample {
    fixture: String,
    whisper_ms: u64,
    refine_ms: u64,
    total_ms: u64,
    mode: String,
    accelerator: String,
    whisper_path: String,
    backend: String,
    language_pinned: bool,
    language_mode: String,
    model_class: String,
    model_path: String,
    model_drive: String,
    runtime_path: String,
    runtime_drive: String,
    ping_ms: Option<u64>,
    cold_server_start_ms: Option<u64>,
    warm_server_inference_ms: Option<u64>,
    cli_gpu_inference_ms: Option<u64>,
    cli_cpu_fallback_ms: Option<u64>,
    pipeline_overhead_ms: Option<u64>,
    refinement_model: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct LatencyBenchmarkPathSummary {
    whisper_path: String,
    sample_count: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct LatencyBenchmarkResult {
    warmup_runs: u32,
    measure_runs: u32,
    pub(crate) p50_ms: u64,
    pub(crate) p95_ms: u64,
    pub(crate) slo_p50_ms: u64,
    pub(crate) slo_p95_ms: u64,
    pub(crate) slo_pass: bool,
    classification_pass: bool,
    cold_server_start_ms: Option<u64>,
    cold_server_start_target_ms: u64,
    cold_server_start_target_pass: Option<bool>,
    whisper_path_summary: Vec<LatencyBenchmarkPathSummary>,
    samples: Vec<LatencyBenchmarkSample>,
    warnings: Vec<String>,
}

pub(crate) fn run_latency_benchmark_inner(
    app: &AppHandle,
    state: &AppState,
    request: &LatencyBenchmarkRequest,
) -> Result<LatencyBenchmarkResult, String> {
    let warmup_runs = request.warmup_runs.min(10);
    let measure_runs = request.measure_runs.clamp(1, 200);
    let include_refinement = request.include_refinement;

    let fixture_paths: Vec<PathBuf> = if request.fixture_paths.is_empty() {
        default_latency_fixture_paths()
    } else {
        let allowed_root = crate::paths::resolve_base_dir(&app);
        let mut validated = Vec::new();
        for path_str in &request.fixture_paths {
            validated.push(crate::paths::validate_path_within(path_str, &allowed_root)?);
        }
        validated
    };

    if fixture_paths.is_empty() {
        return Err(
            "No benchmark fixtures found. Add WAV files under bench/fixtures/short/.".to_string(),
        );
    }

    let mut fixtures: Vec<(String, Vec<i16>)> = Vec::new();
    for path in fixture_paths {
        let samples = read_wav_for_latency_benchmark(&path)?;
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.display().to_string());
        fixtures.push((label, samples));
    }

    let mut settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if include_refinement {
        if let Some(model) = request
            .refinement_model
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            let model = model.to_string();
            settings_snapshot.ai_fallback.enabled = true;
            settings_snapshot.ai_fallback.provider = "ollama".to_string();
            settings_snapshot.ai_fallback.execution_mode = "local_primary".to_string();
            settings_snapshot.ai_fallback.model = model.clone();
            settings_snapshot.postproc_llm_model = model.clone();
            settings_snapshot.providers.ollama.preferred_model = model;
        }
    }
    let active_refinement_model = if include_refinement && settings_snapshot.ai_fallback.enabled {
        Some(settings_snapshot.ai_fallback.model.clone())
    } else {
        None
    };
    let mut samples: Vec<LatencyBenchmarkSample> = Vec::with_capacity(measure_runs as usize);
    let mut warnings: Vec<String> = Vec::new();
    let total_runs = warmup_runs + measure_runs;

    for run_idx in 0..total_runs {
        let fixture_idx = run_idx as usize % fixtures.len();
        let (fixture_name, fixture_samples) = (&fixtures[fixture_idx].0, &fixtures[fixture_idx].1);

        let whisper_started = Instant::now();
        let (raw_text, _source) = transcribe_audio(app, &settings_snapshot, fixture_samples)?;
        let whisper_ms = whisper_started.elapsed().as_millis() as u64;

        let mut refine_ms = 0u64;
        let mut mode = "raw".to_string();
        let mut refinement_model_used = active_refinement_model.clone();
        if include_refinement && settings_snapshot.ai_fallback.enabled {
            let refine_started = Instant::now();
            match refine_transcript_for_benchmark(app, &settings_snapshot, &raw_text) {
                Ok(result) => {
                    refine_ms = refine_started.elapsed().as_millis() as u64;
                    mode = "refined".to_string();
                    refinement_model_used = Some(result.model);
                }
                Err(error) => {
                    refine_ms = refine_started.elapsed().as_millis() as u64;
                    mode = if error.to_lowercase().contains("timed out") {
                        "fallback_timeout".to_string()
                    } else {
                        "fallback_error".to_string()
                    };
                    warnings.push(format!("{}: {}", fixture_name, error));
                }
            }
        }

        if run_idx < warmup_runs {
            continue;
        }

        let total_ms = whisper_ms.saturating_add(refine_ms);
        let timing = last_transcription_timing_summary();
        samples.push(LatencyBenchmarkSample {
            fixture: fixture_name.clone(),
            whisper_ms,
            refine_ms,
            total_ms,
            mode,
            accelerator: last_transcription_accelerator().to_string(),
            whisper_path: timing.whisper_path,
            backend: timing.backend,
            language_pinned: timing.language_pinned,
            language_mode: timing.language_mode,
            model_class: timing.model_class,
            model_path: timing.model_path,
            model_drive: timing.model_drive,
            runtime_path: timing.runtime_path,
            runtime_drive: timing.runtime_drive,
            ping_ms: timing.ping_ms,
            cold_server_start_ms: timing.cold_server_start_ms,
            warm_server_inference_ms: timing.warm_server_inference_ms,
            cli_gpu_inference_ms: timing.cli_gpu_inference_ms,
            cli_cpu_fallback_ms: timing.cli_cpu_fallback_ms,
            pipeline_overhead_ms: timing.pipeline_overhead_ms,
            refinement_model: refinement_model_used,
        });
    }

    let mut totals: Vec<u64> = samples.iter().map(|sample| sample.total_ms).collect();
    totals.sort_unstable();
    let p50_ms = percentile(&totals, 0.50);
    let p95_ms = percentile(&totals, 0.95);
    let slo_p50_ms = 2_500;
    let slo_p95_ms = 4_000;
    let classification_pass = samples
        .iter()
        .all(|sample| !sample.whisper_path.trim().is_empty() && sample.whisper_path != "unknown");
    if !classification_pass {
        warnings
            .push("One or more latency samples have unknown Whisper execution path.".to_string());
    }
    let slo_pass = p50_ms <= slo_p50_ms && p95_ms <= slo_p95_ms && classification_pass;
    let cold_server_start_ms = crate::whisper_server::last_server_cold_start_ms();
    let cold_server_start_target_ms = 10_000;
    let cold_server_start_target_pass =
        cold_server_start_ms.map(|value| value <= cold_server_start_target_ms);
    let mut path_counts: HashMap<String, u32> = HashMap::new();
    for sample in &samples {
        *path_counts.entry(sample.whisper_path.clone()).or_insert(0) += 1;
    }
    let mut whisper_path_summary: Vec<LatencyBenchmarkPathSummary> = path_counts
        .into_iter()
        .map(|(whisper_path, sample_count)| LatencyBenchmarkPathSummary {
            whisper_path,
            sample_count,
        })
        .collect();
    whisper_path_summary.sort_by(|a, b| a.whisper_path.cmp(&b.whisper_path));

    Ok(LatencyBenchmarkResult {
        warmup_runs,
        measure_runs,
        p50_ms,
        p95_ms,
        slo_p50_ms,
        slo_p95_ms,
        slo_pass,
        classification_pass,
        cold_server_start_ms,
        cold_server_start_target_ms,
        cold_server_start_target_pass,
        whisper_path_summary,
        samples,
        warnings,
    })
}

pub(crate) fn write_latency_benchmark_report(
    result: &LatencyBenchmarkResult,
) -> Result<PathBuf, String> {
    let root = resolve_benchmark_root_dir();
    let out_dir = root.join("bench").join("results");
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        format!(
            "Failed creating benchmark output dir '{}': {}",
            out_dir.display(),
            e
        )
    })?;
    let out_path = out_dir.join("latest.json");
    let serialized = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, serialized).map_err(|e| {
        format!(
            "Failed writing benchmark report '{}': {}",
            out_path.display(),
            e
        )
    })?;
    Ok(out_path)
}

pub(crate) fn write_latency_benchmark_error(error: &str) -> Result<PathBuf, String> {
    let root = resolve_benchmark_root_dir();
    let out_dir = root.join("bench").join("results");
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        format!(
            "Failed creating benchmark output dir '{}': {}",
            out_dir.display(),
            e
        )
    })?;
    let out_path = out_dir.join("latest-error.txt");
    std::fs::write(&out_path, error).map_err(|e| {
        format!(
            "Failed writing benchmark error '{}': {}",
            out_path.display(),
            e
        )
    })?;
    Ok(out_path)
}

fn default_latency_fixture_paths() -> Vec<PathBuf> {
    let root = resolve_benchmark_root_dir();
    let fixture_dir = root.join("bench").join("fixtures").join("short");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&fixture_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_wav = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("wav"))
                .unwrap_or(false);
            if is_wav {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn resolve_benchmark_root_dir() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("bench").is_dir() {
        return cwd;
    }

    let mut candidate = cwd.clone();
    for _ in 0..4 {
        if let Some(parent) = candidate.parent() {
            if parent.join("bench").is_dir() {
                return parent.to_path_buf();
            }
            candidate = parent.to_path_buf();
        } else {
            break;
        }
    }

    cwd
}

fn read_wav_for_latency_benchmark(path: &Path) -> Result<Vec<i16>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("Failed to open WAV fixture '{}': {}", path.display(), e))?;
    let spec = reader.spec();
    if spec.sample_rate != crate::constants::TARGET_SAMPLE_RATE {
        return Err(format!(
            "Fixture '{}' uses unsupported sample rate {} (expected {}).",
            path.display(),
            spec.sample_rate,
            crate::constants::TARGET_SAMPLE_RATE
        ));
    }

    let channels = spec.channels.max(1) as usize;
    let mut mono = Vec::<i16>::new();

    match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample != 16 {
                return Err(format!(
                    "Fixture '{}' must be 16-bit PCM for benchmark (got {} bits).",
                    path.display(),
                    spec.bits_per_sample
                ));
            }
            let samples: Vec<i16> = reader
                .samples::<i16>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed reading fixture '{}': {}", path.display(), e))?;
            for frame in samples.chunks(channels) {
                if let Some(first) = frame.first() {
                    mono.push(*first);
                }
            }
        }
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed reading float fixture '{}': {}", path.display(), e))?;
            for frame in samples.chunks(channels) {
                if let Some(first) = frame.first() {
                    let clamped = first.clamp(-1.0, 1.0);
                    mono.push((clamped * i16::MAX as f32) as i16);
                }
            }
        }
    }

    if mono.is_empty() {
        return Err(format!(
            "Fixture '{}' has no audio samples.",
            path.display()
        ));
    }
    Ok(mono)
}

fn percentile(sorted_values: &[u64], quantile: f64) -> u64 {
    if sorted_values.is_empty() {
        return 0;
    }
    let q = quantile.clamp(0.0, 1.0);
    let idx = ((sorted_values.len() - 1) as f64 * q).round() as usize;
    sorted_values[idx]
}

fn refine_transcript_for_benchmark(
    app: &AppHandle,
    settings_snapshot: &Settings,
    transcript: &str,
) -> Result<crate::ai_fallback::models::RefinementResult, String> {
    let setup = crate::ai_fallback::prepare_refinement(app, settings_snapshot)?;

    setup
        .provider
        .refine_transcript(transcript, &setup.model, &setup.options, &setup.api_key)
        .map_err(|e| e.to_string())
}

pub(crate) fn latency_benchmark_request_from_env() -> LatencyBenchmarkRequest {
    let mut request = LatencyBenchmarkRequest::default();

    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_WARMUP_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.warmup_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_MEASURE_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.measure_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_INCLUDE_REFINEMENT") {
        request.include_refinement = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_FIXTURES") {
        let fixtures = value
            .split(';')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        if !fixtures.is_empty() {
            request.fixture_paths = fixtures;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_BENCHMARK_REFINE_MODEL") {
        let model = value.trim();
        if !model.is_empty() {
            request.refinement_model = Some(model.to_string());
        }
    }

    request
}

#[tauri::command]
pub(crate) fn run_latency_benchmark(
    app: AppHandle,
    state: State<'_, AppState>,
    request: Option<LatencyBenchmarkRequest>,
) -> Result<LatencyBenchmarkResult, String> {
    let request = request.unwrap_or_default();
    run_latency_benchmark_inner(&app, state.inner(), &request)
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
struct TtsBenchmarkScenario {
    id: String,
    text: String,
    length_bucket: String, // "short" | "long"
    language: String,      // "de" | "en"
    thermal: String,       // "cold" | "warm"
}

impl Default for TtsBenchmarkScenario {
    fn default() -> Self {
        Self {
            id: String::new(),
            text: String::new(),
            length_bucket: String::new(),
            language: String::new(),
            thermal: String::new(),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub(crate) struct TtsBenchmarkRequest {
    providers: Vec<String>,
    scenarios: Vec<TtsBenchmarkScenario>,
    warmup_runs: u32,
    measure_runs: u32,
    rate: f32,
    volume: f32,
    piper_binary_path: Option<String>,
    piper_model_path: Option<String>,
    qwen3_tts_endpoint: Option<String>,
    qwen3_tts_model: Option<String>,
    qwen3_tts_voice: Option<String>,
    qwen3_tts_api_key: Option<String>,
    qwen3_tts_timeout_sec: Option<u64>,
    lock_matrix: bool,
    run_runtime_smoke: bool,
}

impl Default for TtsBenchmarkRequest {
    fn default() -> Self {
        Self {
            providers: vec![
                "windows_native".to_string(),
                "local_custom".to_string(),
                "qwen3_tts".to_string(),
            ],
            scenarios: Vec::new(),
            warmup_runs: 1,
            measure_runs: 3,
            rate: 1.0,
            volume: 1.0,
            piper_binary_path: None,
            piper_model_path: None,
            qwen3_tts_endpoint: None,
            qwen3_tts_model: None,
            qwen3_tts_voice: None,
            qwen3_tts_api_key: None,
            qwen3_tts_timeout_sec: None,
            lock_matrix: true,
            run_runtime_smoke: true,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkSample {
    provider: String,
    scenario: String,
    run: u32,
    elapsed_ms: u64,
    success: bool,
    error: Option<String>,
    failure_category: Option<String>, // missing_binary | missing_model | endpoint_unreachable | auth_missing | runtime_error
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkProviderSummary {
    provider: String,
    attempts: u32,
    success_count: u32,
    failure_count: u32,
    success_rate: f32,
    p50_ms: Option<u64>,
    p95_ms: Option<u64>,
    avg_ms: Option<u64>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsBenchmarkGateConfig {
    reliability_min_success_rate: f32,
    latency_target_p50_ms: u64,
    latency_target_p95_ms: u64,
    min_success_per_scenario: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsProviderProfile {
    provider: String,
    surface: String, // "runtime_stable" | "benchmark_experimental"
    experimental_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsPreflightCheck {
    provider: String,
    check: String,
    passed: bool,
    category: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsRuntimeSmokeCheck {
    provider: String,
    passed: bool,
    category: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TtsProviderGateEvaluation {
    provider: String,
    evaluated_for_release: bool,
    passes_release_gate: bool,
    preflight_ok: bool,
    runtime_smoke_ok: bool,
    reliability_ok: bool,
    latency_ok: bool,
    scenario_success_ok: bool,
    success_rate: f32,
    p50_ms: Option<u64>,
    p95_ms: Option<u64>,
    min_success_in_any_scenario: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct TtsBenchmarkResult {
    artifact_version: String,
    generated_at: String,
    warmup_runs: u32,
    measure_runs: u32,
    providers: Vec<String>,
    scenarios: Vec<String>,
    scenario_matrix_locked: bool,
    gates: TtsBenchmarkGateConfig,
    provider_profiles: Vec<TtsProviderProfile>,
    preflight_checks: Vec<TtsPreflightCheck>,
    runtime_smoke_checks: Vec<TtsRuntimeSmokeCheck>,
    samples: Vec<TtsBenchmarkSample>,
    provider_summaries: Vec<TtsBenchmarkProviderSummary>,
    provider_gate_evaluations: Vec<TtsProviderGateEvaluation>,
    provider_consistency_ok: bool,
    provider_consistency_detail: String,
    fallback_order: Vec<String>,
    release_gate_pass: bool,
    release_gate_reason: String,
    recommended_default_provider: Option<String>,
    recommendation_reason: String,
    uncategorized_failure_count: u32,
    warnings: Vec<String>,
}

const TTS_PROVIDER_SURFACE_RUNTIME_STABLE: &str = "runtime_stable";
const TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL: &str = "benchmark_experimental";
const TTS_FAILURE_MISSING_BINARY: &str = "missing_binary";
const TTS_FAILURE_MISSING_MODEL: &str = "missing_model";
const TTS_FAILURE_ENDPOINT_UNREACHABLE: &str = "endpoint_unreachable";
const TTS_FAILURE_AUTH_MISSING: &str = "auth_missing";
const TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED: &str = "stream_config_unsupported";
const TTS_FAILURE_RUNTIME_ERROR: &str = "runtime_error";

fn default_tts_benchmark_gates() -> TtsBenchmarkGateConfig {
    TtsBenchmarkGateConfig {
        reliability_min_success_rate: 0.95,
        latency_target_p50_ms: 700,
        latency_target_p95_ms: 1500,
        min_success_per_scenario: 2,
    }
}

fn tts_provider_profile(provider: &str) -> TtsProviderProfile {
    match provider {
        "qwen3_tts" => TtsProviderProfile {
            provider: provider.to_string(),
            surface: TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL.to_string(),
            experimental_reason: Some(
                "Endpoint-backed runtime provider treated as experimental for release-gating."
                    .to_string(),
            ),
        },
        _ => TtsProviderProfile {
            provider: provider.to_string(),
            surface: TTS_PROVIDER_SURFACE_RUNTIME_STABLE.to_string(),
            experimental_reason: None,
        },
    }
}

fn is_runtime_stable_provider(provider: &str) -> bool {
    tts_provider_profile(provider).surface == TTS_PROVIDER_SURFACE_RUNTIME_STABLE
}

fn classify_tts_failure(error: &str) -> String {
    let normalized = error.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return TTS_FAILURE_RUNTIME_ERROR.to_string();
    }

    if normalized.contains("binary not found")
        || normalized.contains("npm.cmd not found")
        || normalized.contains("failed to start piper")
        || normalized.contains("no such file")
    {
        return TTS_FAILURE_MISSING_BINARY.to_string();
    }
    if normalized.contains("model not found")
        || normalized.contains("no piper voice model found")
        || normalized.contains("set piper_model_path")
        || normalized.contains("onnx")
    {
        return TTS_FAILURE_MISSING_MODEL.to_string();
    }
    if normalized.contains("http 401")
        || normalized.contains("http 403")
        || normalized.contains("unauthorized")
        || normalized.contains("forbidden")
        || normalized.contains("api key")
        || normalized.contains("authorization")
    {
        return TTS_FAILURE_AUTH_MISSING.to_string();
    }
    if normalized.contains("[tts_output_stream_config_unsupported]")
        || normalized.contains("stream configuration is not supported")
        || normalized.contains("streamconfignotsupported")
    {
        return TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED.to_string();
    }
    if normalized.contains("endpoint")
        || normalized.contains("timed out")
        || normalized.contains("connection")
        || normalized.contains("refused")
        || normalized.contains("dns")
        || normalized.contains("transport")
        || normalized.contains("failed to connect")
    {
        return TTS_FAILURE_ENDPOINT_UNREACHABLE.to_string();
    }
    TTS_FAILURE_RUNTIME_ERROR.to_string()
}

fn default_tts_benchmark_scenarios() -> Vec<TtsBenchmarkScenario> {
    vec![
        TtsBenchmarkScenario {
            id: "short_de_cold".to_string(),
            text: "Kurzer Benchmark-Check.".to_string(),
            length_bucket: "short".to_string(),
            language: "de".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "short_de_warm".to_string(),
            text: "Kurzer Benchmark-Check.".to_string(),
            length_bucket: "short".to_string(),
            language: "de".to_string(),
            thermal: "warm".to_string(),
        },
        TtsBenchmarkScenario {
            id: "short_en_cold".to_string(),
            text: "Short benchmark check.".to_string(),
            length_bucket: "short".to_string(),
            language: "en".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "short_en_warm".to_string(),
            text: "Short benchmark check.".to_string(),
            length_bucket: "short".to_string(),
            language: "en".to_string(),
            thermal: "warm".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_de_cold".to_string(),
            text: "Dies ist ein längerer deutscher Benchmark-Satz, der Antworttempo und Stabilität unter praxisnahen Bedingungen vergleicht."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "de".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_de_warm".to_string(),
            text: "Dies ist ein längerer deutscher Benchmark-Satz, der Antworttempo und Stabilität unter praxisnahen Bedingungen vergleicht."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "de".to_string(),
            thermal: "warm".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_en_cold".to_string(),
            text: "This is a longer benchmark sentence to compare synthesis latency and stability under realistic assistant output conditions."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "en".to_string(),
            thermal: "cold".to_string(),
        },
        TtsBenchmarkScenario {
            id: "long_en_warm".to_string(),
            text: "This is a longer benchmark sentence to compare synthesis latency and stability under realistic assistant output conditions."
                .to_string(),
            length_bucket: "long".to_string(),
            language: "en".to_string(),
            thermal: "warm".to_string(),
        },
    ]
}

fn normalize_tts_benchmark_providers(requested: &[String]) -> Vec<String> {
    let mut providers = Vec::<String>::new();
    for value in requested {
        let normalized = value.trim().to_lowercase();
        if normalized != "windows_native"
            && normalized != "windows_natural"
            && normalized != "local_custom"
            && normalized != "qwen3_tts"
        {
            continue;
        }
        if !providers.contains(&normalized) {
            providers.push(normalized);
        }
    }
    if providers.is_empty() {
        vec![
            "windows_native".to_string(),
            "local_custom".to_string(),
            "qwen3_tts".to_string(),
        ]
    } else {
        providers
    }
}

fn resolve_qwen3_tts_benchmark_config(request: &TtsBenchmarkRequest) -> Qwen3TtsConfig {
    let endpoint = request
        .qwen3_tts_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http://127.0.0.1:8000/v1/audio/speech")
        .to_string();
    let model = request
        .qwen3_tts_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice")
        .to_string();
    let voice = request
        .qwen3_tts_voice
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("vivian")
        .to_string();
    let api_key = request
        .qwen3_tts_api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let timeout_sec = request.qwen3_tts_timeout_sec.unwrap_or(45).clamp(3, 180);

    Qwen3TtsConfig {
        endpoint,
        model,
        voice,
        api_key,
        timeout_sec,
    }
}

fn benchmark_qwen3_tts_synthesis(
    text: &str,
    rate: f32,
    config: &Qwen3TtsConfig,
) -> Result<(), String> {
    let (bytes, content_type) =
        crate::multimodal_io::request_qwen3_tts_audio_bytes(text, rate, config)?;

    if bytes.is_empty() {
        return Err("Qwen3-TTS returned an empty response body.".to_string());
    }
    if content_type.contains("application/json") {
        let text = String::from_utf8_lossy(&bytes).trim().to_string();
        return Err(format!(
            "Qwen3-TTS returned JSON instead of audio: {}",
            text
        ));
    }

    Ok(())
}

fn normalize_tts_benchmark_scenarios(
    requested: &[TtsBenchmarkScenario],
    lock_matrix: bool,
) -> Vec<TtsBenchmarkScenario> {
    if lock_matrix {
        return default_tts_benchmark_scenarios();
    }

    let mut scenarios = Vec::<TtsBenchmarkScenario>::new();
    for (idx, scenario) in requested.iter().enumerate() {
        let text = scenario.text.trim();
        if text.is_empty() {
            continue;
        }
        let id = if scenario.id.trim().is_empty() {
            format!("scenario_{}", idx + 1)
        } else {
            scenario.id.trim().to_lowercase().replace(' ', "_")
        };
        let length_bucket = match scenario.length_bucket.trim().to_ascii_lowercase().as_str() {
            "short" => "short".to_string(),
            "long" => "long".to_string(),
            _ => "short".to_string(),
        };
        let language = match scenario.language.trim().to_ascii_lowercase().as_str() {
            "de" => "de".to_string(),
            "en" => "en".to_string(),
            _ => "en".to_string(),
        };
        let thermal = match scenario.thermal.trim().to_ascii_lowercase().as_str() {
            "cold" => "cold".to_string(),
            "warm" => "warm".to_string(),
            _ => "warm".to_string(),
        };
        scenarios.push(TtsBenchmarkScenario {
            id,
            text: text.to_string(),
            length_bucket,
            language,
            thermal,
        });
    }
    if scenarios.is_empty() {
        default_tts_benchmark_scenarios()
    } else {
        scenarios
    }
}

fn run_tts_provider_once(
    provider: &str,
    text: &str,
    rate: f32,
    volume: f32,
    windows_voice_id: &str,
    piper_binary_path: &str,
    piper_model_path: &str,
    qwen3_config: &Qwen3TtsConfig,
) -> Result<(), String> {
    let selected_windows_voice = windows_voice_id.trim();
    let selected_windows_voice = if selected_windows_voice.is_empty() {
        None
    } else {
        Some(selected_windows_voice)
    };
    match provider {
        "windows_native" => crate::multimodal_io::benchmark_windows_native_synthesis(
            text,
            rate,
            volume,
            selected_windows_voice,
        ),
        "windows_natural" => crate::multimodal_io::benchmark_windows_natural_synthesis(
            text,
            rate,
            volume,
            selected_windows_voice,
        ),
        "local_custom" => crate::multimodal_io::benchmark_piper_synthesis(
            text,
            piper_binary_path,
            piper_model_path,
            rate,
        ),
        "qwen3_tts" => benchmark_qwen3_tts_synthesis(text, rate, qwen3_config),
        _ => Err(format!(
            "Unsupported TTS benchmark provider '{}'.",
            provider
        )),
    }
}

fn run_tts_runtime_smoke_once(
    state: &AppState,
    provider: &str,
    rate: f32,
    windows_voice_id: &str,
    piper_binary_path: &str,
    piper_model_path: &str,
    output_device_id: &str,
) -> Result<(), String> {
    let smoke_text = "Trispr Flow runtime smoke test.";
    let selected_windows_voice = windows_voice_id.trim();
    let selected_windows_voice = if selected_windows_voice.is_empty() {
        None
    } else {
        Some(selected_windows_voice)
    };
    match provider {
        "windows_native" => crate::multimodal_io::speak_windows_native(
            smoke_text,
            rate,
            0.0,
            output_device_id,
            selected_windows_voice,
            None,
        ),
        "windows_natural" => crate::multimodal_io::speak_windows_natural(
            smoke_text,
            rate,
            0.0,
            output_device_id,
            selected_windows_voice,
            None,
        ),
        "local_custom" => crate::multimodal_io::speak_piper(
            &state.piper_daemon,
            smoke_text,
            piper_binary_path,
            piper_model_path,
            rate,
            0.0,
            output_device_id,
            None,
        ),
        _ => Err(format!(
            "Runtime smoke is unsupported for benchmark-only provider '{}'.",
            provider
        )),
    }
}

fn summarize_tts_provider(
    provider: &str,
    samples: &[TtsBenchmarkSample],
) -> TtsBenchmarkProviderSummary {
    let mut latencies: Vec<u64> = samples
        .iter()
        .filter(|sample| sample.success)
        .map(|sample| sample.elapsed_ms)
        .collect();
    latencies.sort_unstable();
    let attempts = samples.len() as u32;
    let success_count = latencies.len() as u32;
    let failure_count = attempts.saturating_sub(success_count);
    let success_rate = if attempts == 0 {
        0.0
    } else {
        success_count as f32 / attempts as f32
    };
    let avg_ms = if latencies.is_empty() {
        None
    } else {
        Some(latencies.iter().sum::<u64>() / latencies.len() as u64)
    };

    TtsBenchmarkProviderSummary {
        provider: provider.to_string(),
        attempts,
        success_count,
        failure_count,
        success_rate,
        p50_ms: if latencies.is_empty() {
            None
        } else {
            Some(percentile(&latencies, 0.50))
        },
        p95_ms: if latencies.is_empty() {
            None
        } else {
            Some(percentile(&latencies, 0.95))
        },
        avg_ms,
    }
}

fn build_tts_fallback_order(
    summaries: &[TtsBenchmarkProviderSummary],
    reliability_gate: f32,
) -> Vec<String> {
    let mut eligible: Vec<&TtsBenchmarkProviderSummary> = summaries
        .iter()
        .filter(|summary| summary.success_rate >= reliability_gate && summary.p95_ms.is_some())
        .collect();
    eligible.sort_by(|a, b| {
        a.p95_ms
            .unwrap_or(u64::MAX)
            .cmp(&b.p95_ms.unwrap_or(u64::MAX))
            .then_with(|| {
                a.p50_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.p50_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| a.provider.cmp(&b.provider))
    });

    let mut fallback_sorted: Vec<&TtsBenchmarkProviderSummary> = summaries.iter().collect();
    fallback_sorted.sort_by(|a, b| {
        b.success_rate
            .partial_cmp(&a.success_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.failure_count.cmp(&b.failure_count))
            .then_with(|| {
                a.p95_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.p95_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| {
                a.p50_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.p50_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| a.provider.cmp(&b.provider))
    });

    let mut order: Vec<String> = Vec::new();
    for item in eligible.into_iter().chain(fallback_sorted.into_iter()) {
        if !order.iter().any(|provider| provider == &item.provider) {
            order.push(item.provider.clone());
        }
    }
    order
}

fn scenario_success_counts_for_provider(
    provider: &str,
    scenarios: &[TtsBenchmarkScenario],
    samples: &[TtsBenchmarkSample],
) -> HashMap<String, u32> {
    let mut out = HashMap::<String, u32>::new();
    for scenario in scenarios {
        let success = samples
            .iter()
            .filter(|sample| {
                sample.provider == provider && sample.scenario == scenario.id && sample.success
            })
            .count() as u32;
        out.insert(scenario.id.clone(), success);
    }
    out
}

fn provider_consistency_from_runtime_surface(
    providers: &[String],
    qwen3_tts_enabled: bool,
) -> (bool, String) {
    let runtime_surface = crate::multimodal_io::list_tts_provider_infos(qwen3_tts_enabled)
        .into_iter()
        .map(|info| (info.id, info.surface))
        .collect::<HashMap<_, _>>();

    let mut mismatches: Vec<String> = Vec::new();
    for provider in providers {
        if let Some(surface) = runtime_surface.get(provider) {
            if provider == "qwen3_tts" && surface != TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL {
                mismatches.push(format!(
                    "{} should be '{}' in runtime surface, got '{}'",
                    provider, TTS_PROVIDER_SURFACE_BENCHMARK_EXPERIMENTAL, surface
                ));
            }
            if provider != "qwen3_tts" && surface != TTS_PROVIDER_SURFACE_RUNTIME_STABLE {
                mismatches.push(format!(
                    "{} should be '{}' in runtime surface, got '{}'",
                    provider, TTS_PROVIDER_SURFACE_RUNTIME_STABLE, surface
                ));
            }
        } else {
            mismatches.push(format!(
                "{} missing from runtime provider exposure list",
                provider
            ));
        }
    }

    if mismatches.is_empty() {
        (
            true,
            "Benchmark scope and runtime provider surface are consistent.".to_string(),
        )
    } else {
        (false, mismatches.join(" | "))
    }
}

#[cfg(test)]
mod tts_benchmark_tests {
    use super::{
        build_tts_fallback_order, classify_tts_failure, normalize_tts_benchmark_providers,
        TtsBenchmarkProviderSummary, TTS_FAILURE_AUTH_MISSING, TTS_FAILURE_ENDPOINT_UNREACHABLE,
        TTS_FAILURE_MISSING_BINARY, TTS_FAILURE_MISSING_MODEL, TTS_FAILURE_RUNTIME_ERROR,
        TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED,
    };

    #[test]
    fn fallback_order_prefers_reliability_gate_then_latency() {
        let summaries = vec![
            TtsBenchmarkProviderSummary {
                provider: "windows_native".to_string(),
                attempts: 9,
                success_count: 9,
                failure_count: 0,
                success_rate: 1.0,
                p50_ms: Some(190),
                p95_ms: Some(290),
                avg_ms: Some(210),
            },
            TtsBenchmarkProviderSummary {
                provider: "local_custom".to_string(),
                attempts: 9,
                success_count: 9,
                failure_count: 0,
                success_rate: 1.0,
                p50_ms: Some(170),
                p95_ms: Some(240),
                avg_ms: Some(185),
            },
        ];

        let order = build_tts_fallback_order(&summaries, 0.95);
        assert_eq!(
            order,
            vec!["local_custom".to_string(), "windows_native".to_string()]
        );
    }

    #[test]
    fn fallback_order_still_returns_best_available_when_gate_not_met() {
        let summaries = vec![
            TtsBenchmarkProviderSummary {
                provider: "windows_native".to_string(),
                attempts: 9,
                success_count: 7,
                failure_count: 2,
                success_rate: 7.0 / 9.0,
                p50_ms: Some(210),
                p95_ms: Some(330),
                avg_ms: Some(230),
            },
            TtsBenchmarkProviderSummary {
                provider: "local_custom".to_string(),
                attempts: 9,
                success_count: 5,
                failure_count: 4,
                success_rate: 5.0 / 9.0,
                p50_ms: Some(260),
                p95_ms: Some(390),
                avg_ms: Some(280),
            },
        ];

        let order = build_tts_fallback_order(&summaries, 0.95);
        assert_eq!(order.first().map(String::as_str), Some("windows_native"));
    }

    #[test]
    fn classifies_tts_failures_into_fixed_categories() {
        assert_eq!(
            classify_tts_failure("Piper TTS binary not found."),
            TTS_FAILURE_MISSING_BINARY.to_string()
        );
        assert_eq!(
            classify_tts_failure("Piper model not found: D:\\voices\\de.onnx"),
            TTS_FAILURE_MISSING_MODEL.to_string()
        );
        assert_eq!(
            classify_tts_failure("Qwen3-TTS benchmark request failed: connection refused"),
            TTS_FAILURE_ENDPOINT_UNREACHABLE.to_string()
        );
        assert_eq!(
            classify_tts_failure("Qwen3-TTS benchmark request failed with HTTP 401"),
            TTS_FAILURE_AUTH_MISSING.to_string()
        );
        assert_eq!(
            classify_tts_failure(
                "[tts_output_stream_config_unsupported] device='wasapi:xyz' wav=22050Hz/1ch/int16 -> target=48000Hz/2ch/f32 reason=The requested stream configuration is not supported by the device."
            ),
            TTS_FAILURE_STREAM_CONFIG_UNSUPPORTED.to_string()
        );
        assert_eq!(
            classify_tts_failure("unexpected panic in voice backend"),
            TTS_FAILURE_RUNTIME_ERROR.to_string()
        );
    }

    #[test]
    fn provider_normalization_accepts_windows_natural_qwen3_and_deduplicates() {
        let input = vec![
            " windows_native ".to_string(),
            "windows_natural".to_string(),
            "qwen3_tts".to_string(),
            "local_custom".to_string(),
            "QWEN3_TTS".to_string(),
            "unsupported".to_string(),
        ];
        let providers = normalize_tts_benchmark_providers(&input);
        assert_eq!(
            providers,
            vec![
                "windows_native".to_string(),
                "windows_natural".to_string(),
                "qwen3_tts".to_string(),
                "local_custom".to_string(),
            ]
        );
    }
}

pub(crate) fn run_tts_benchmark_inner(
    state: &AppState,
    request: &TtsBenchmarkRequest,
) -> Result<TtsBenchmarkResult, String> {
    let warmup_runs = request.warmup_runs.min(5);
    let measure_runs = request.measure_runs.clamp(3, 100);
    let gates = default_tts_benchmark_gates();
    let providers = normalize_tts_benchmark_providers(&request.providers);
    let scenarios = normalize_tts_benchmark_scenarios(&request.scenarios, request.lock_matrix);
    let qwen3_config = resolve_qwen3_tts_benchmark_config(request);
    let rate = if request.rate.is_finite() {
        request.rate.clamp(0.5, 2.0)
    } else {
        1.0
    };
    let volume = if request.volume.is_finite() {
        request.volume.clamp(0.0, 1.0)
    } else {
        1.0
    };

    let voice_settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .voice_output_settings
        .clone();
    let piper_binary_path = request
        .piper_binary_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| voice_settings.piper_binary_path.clone());
    let piper_model_path = request
        .piper_model_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| voice_settings.piper_model_path.clone());
    let piper_model_dir = voice_settings.piper_model_dir.clone();

    let mut samples: Vec<TtsBenchmarkSample> = Vec::new();
    let mut preflight_checks: Vec<TtsPreflightCheck> = Vec::new();
    let mut runtime_smoke_checks: Vec<TtsRuntimeSmokeCheck> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let provider_profiles = providers
        .iter()
        .map(|provider| tts_provider_profile(provider))
        .collect::<Vec<_>>();

    for provider in &providers {
        let mut provider_preflight: Vec<TtsPreflightCheck> = Vec::new();
        if is_runtime_stable_provider(provider) {
            let module_enabled = voice_settings.enabled;
            provider_preflight.push(TtsPreflightCheck {
                provider: provider.clone(),
                check: "module_enabled".to_string(),
                passed: module_enabled,
                category: if module_enabled {
                    None
                } else {
                    Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                },
                detail: if module_enabled {
                    "Voice output module is enabled.".to_string()
                } else {
                    "Voice output module is disabled. Enable module 'output_voice_tts' before release benchmarking."
                        .to_string()
                },
            });
        }
        match provider.as_str() {
            "windows_native" => {
                let passed = cfg!(target_os = "windows");
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "platform".to_string(),
                    passed,
                    category: if passed {
                        None
                    } else {
                        Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                    },
                    detail: if passed {
                        "Windows runtime detected for windows_native provider.".to_string()
                    } else {
                        "windows_native provider requires Windows runtime.".to_string()
                    },
                });
            }
            "windows_natural" => {
                let platform_ok = cfg!(target_os = "windows");
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "platform".to_string(),
                    passed: platform_ok,
                    category: if platform_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                    },
                    detail: if platform_ok {
                        "Windows runtime detected for windows_natural provider.".to_string()
                    } else {
                        "windows_natural provider requires Windows runtime.".to_string()
                    },
                });

                let natural_voices_ok = crate::multimodal_io::windows_natural_voice_available();
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "natural_voice".to_string(),
                    passed: natural_voices_ok,
                    category: if natural_voices_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_RUNTIME_ERROR.to_string())
                    },
                    detail: if natural_voices_ok {
                        "Detected Windows Natural voice(s) via SAPI.".to_string()
                    } else {
                        "No Windows Natural voice detected. Install NaturalVoiceSAPIAdapter and a Natural voice pack."
                            .to_string()
                    },
                });
            }
            "local_custom" => {
                let binary_preflight =
                    crate::multimodal_io::piper_binary_preflight(&piper_binary_path);
                let binary_ok = binary_preflight.is_ok();
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "binary".to_string(),
                    passed: binary_ok,
                    category: if binary_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_MISSING_BINARY.to_string())
                    },
                    detail: binary_preflight
                        .map(|_| "Piper runtime resolved.".to_string())
                        .unwrap_or_else(|error| error),
                });

                let model_ok = crate::multimodal_io::piper_model_available(
                    &piper_model_path,
                    &piper_model_dir,
                );
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "model".to_string(),
                    passed: model_ok,
                    category: if model_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_MISSING_MODEL.to_string())
                    },
                    detail: if model_ok {
                        "Piper model resolved.".to_string()
                    } else {
                        "Piper model not found. Configure piper_model_path or provide a voices directory."
                            .to_string()
                    },
                });
            }
            "qwen3_tts" => {
                let endpoint_ok = qwen3_config.endpoint.starts_with("http://")
                    || qwen3_config.endpoint.starts_with("https://");
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "endpoint_format".to_string(),
                    passed: endpoint_ok,
                    category: if endpoint_ok {
                        None
                    } else {
                        Some(TTS_FAILURE_ENDPOINT_UNREACHABLE.to_string())
                    },
                    detail: if endpoint_ok {
                        "Qwen3 endpoint format accepted.".to_string()
                    } else {
                        format!(
                            "Qwen3 endpoint '{}' is invalid. Expected http:// or https:// URL.",
                            qwen3_config.endpoint
                        )
                    },
                });

                if endpoint_ok {
                    let probe =
                        benchmark_qwen3_tts_synthesis("Preflight ping.", 1.0, &qwen3_config);
                    provider_preflight.push(TtsPreflightCheck {
                        provider: provider.clone(),
                        check: "endpoint_auth_probe".to_string(),
                        passed: probe.is_ok(),
                        category: probe
                            .as_ref()
                            .err()
                            .map(|error| classify_tts_failure(error)),
                        detail: match probe {
                            Ok(()) => "Qwen3 endpoint/auth probe succeeded.".to_string(),
                            Err(error) => format!("Qwen3 probe failed: {}", error),
                        },
                    });
                }
            }
            _ => {
                provider_preflight.push(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "provider".to_string(),
                    passed: false,
                    category: Some(TTS_FAILURE_RUNTIME_ERROR.to_string()),
                    detail: format!("Unsupported benchmark provider '{}'.", provider),
                });
            }
        }

        let preflight_ok = provider_preflight.iter().all(|check| check.passed);
        preflight_checks.extend(provider_preflight.clone());

        if is_runtime_stable_provider(provider) {
            if request.run_runtime_smoke {
                if preflight_ok {
                    match run_tts_runtime_smoke_once(
                        state,
                        provider,
                        rate,
                        &voice_settings.voice_id_windows,
                        &piper_binary_path,
                        &piper_model_path,
                        &voice_settings.output_device,
                    ) {
                        Ok(()) => runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                            provider: provider.clone(),
                            passed: true,
                            category: None,
                            detail: "Runtime smoke speak path succeeded.".to_string(),
                        }),
                        Err(error) => runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                            provider: provider.clone(),
                            passed: false,
                            category: Some(classify_tts_failure(&error)),
                            detail: format!("Runtime smoke speak path failed: {}", error),
                        }),
                    }
                } else {
                    runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                        provider: provider.clone(),
                        passed: false,
                        category: Some(TTS_FAILURE_RUNTIME_ERROR.to_string()),
                        detail: "Runtime smoke skipped due to preflight failure.".to_string(),
                    });
                }
            } else {
                runtime_smoke_checks.push(TtsRuntimeSmokeCheck {
                    provider: provider.clone(),
                    passed: true,
                    category: None,
                    detail: "Runtime smoke disabled by request.".to_string(),
                });
            }
        }

        if !preflight_ok {
            let first_failed = provider_preflight
                .iter()
                .find(|check| !check.passed)
                .cloned()
                .unwrap_or(TtsPreflightCheck {
                    provider: provider.clone(),
                    check: "unknown".to_string(),
                    passed: false,
                    category: Some(TTS_FAILURE_RUNTIME_ERROR.to_string()),
                    detail: "Unknown preflight failure.".to_string(),
                });
            warnings.push(format!(
                "provider={} preflight failed check={} category={} detail={}",
                provider,
                first_failed.check,
                first_failed
                    .category
                    .clone()
                    .unwrap_or_else(|| TTS_FAILURE_RUNTIME_ERROR.to_string()),
                first_failed.detail
            ));
            for scenario in &scenarios {
                for run in 1..=measure_runs {
                    samples.push(TtsBenchmarkSample {
                        provider: provider.clone(),
                        scenario: scenario.id.clone(),
                        run,
                        elapsed_ms: 0,
                        success: false,
                        error: Some(format!(
                            "Preflight failed ({}): {}",
                            first_failed.check, first_failed.detail
                        )),
                        failure_category: Some(
                            first_failed
                                .category
                                .clone()
                                .unwrap_or_else(|| TTS_FAILURE_RUNTIME_ERROR.to_string()),
                        ),
                    });
                }
            }
            continue;
        }

        for scenario in &scenarios {
            let scenario_warmup = if scenario.thermal == "warm" {
                warmup_runs
            } else {
                0
            };
            for run_idx in 0..(scenario_warmup + measure_runs) {
                let started = Instant::now();
                let outcome = run_tts_provider_once(
                    provider,
                    &scenario.text,
                    rate,
                    volume,
                    &voice_settings.voice_id_windows,
                    &piper_binary_path,
                    &piper_model_path,
                    &qwen3_config,
                );
                let elapsed_ms = started.elapsed().as_millis() as u64;

                if run_idx < scenario_warmup {
                    continue;
                }

                let run = run_idx - scenario_warmup + 1;
                match outcome {
                    Ok(()) => samples.push(TtsBenchmarkSample {
                        provider: provider.clone(),
                        scenario: scenario.id.clone(),
                        run,
                        elapsed_ms,
                        success: true,
                        error: None,
                        failure_category: None,
                    }),
                    Err(error) => {
                        let category = classify_tts_failure(&error);
                        warnings.push(format!(
                            "provider={} scenario={} run={} category={} error={}",
                            provider, scenario.id, run, category, error
                        ));
                        samples.push(TtsBenchmarkSample {
                            provider: provider.clone(),
                            scenario: scenario.id.clone(),
                            run,
                            elapsed_ms,
                            success: false,
                            error: Some(error),
                            failure_category: Some(category),
                        });
                    }
                }
            }
        }
    }

    let mut grouped: HashMap<String, Vec<TtsBenchmarkSample>> = HashMap::new();
    for sample in &samples {
        grouped
            .entry(sample.provider.clone())
            .or_default()
            .push(sample.clone());
    }

    if providers.iter().any(|provider| provider == "qwen3_tts") {
        warnings.push(format!(
            "provider=qwen3_tts config endpoint={} model={} voice={} timeout_sec={}",
            qwen3_config.endpoint, qwen3_config.model, qwen3_config.voice, qwen3_config.timeout_sec
        ));
    }
    if providers.iter().any(|provider| provider == "local_custom") {
        warnings.push(format!(
            "provider=local_custom config piper_binary_path={} piper_model_path={}",
            if piper_binary_path.trim().is_empty() {
                "<auto-resolve>"
            } else {
                piper_binary_path.as_str()
            },
            if piper_model_path.trim().is_empty() {
                "<auto-resolve>"
            } else {
                piper_model_path.as_str()
            }
        ));
    }

    let provider_summaries = providers
        .iter()
        .map(|provider| {
            let provider_samples = grouped.get(provider).cloned().unwrap_or_default();
            summarize_tts_provider(provider, &provider_samples)
        })
        .collect::<Vec<_>>();

    let preflight_ok_by_provider = providers
        .iter()
        .map(|provider| {
            let ok = preflight_checks
                .iter()
                .filter(|check| check.provider == *provider)
                .all(|check| check.passed);
            (provider.clone(), ok)
        })
        .collect::<HashMap<_, _>>();
    let smoke_ok_by_provider = runtime_smoke_checks
        .iter()
        .map(|check| (check.provider.clone(), check.passed))
        .collect::<HashMap<_, _>>();

    let provider_gate_evaluations = providers
        .iter()
        .map(|provider| {
            let summary = provider_summaries
                .iter()
                .find(|summary| summary.provider == *provider)
                .cloned()
                .unwrap_or(TtsBenchmarkProviderSummary {
                    provider: provider.clone(),
                    attempts: 0,
                    success_count: 0,
                    failure_count: 0,
                    success_rate: 0.0,
                    p50_ms: None,
                    p95_ms: None,
                    avg_ms: None,
                });
            let scenario_success =
                scenario_success_counts_for_provider(provider, &scenarios, &samples);
            let min_success_in_any_scenario = scenario_success.values().copied().min().unwrap_or(0);
            let evaluated_for_release = is_runtime_stable_provider(provider);
            let preflight_ok = *preflight_ok_by_provider.get(provider).unwrap_or(&false);
            let runtime_smoke_ok = if evaluated_for_release {
                *smoke_ok_by_provider.get(provider).unwrap_or(&false)
            } else {
                true
            };
            let reliability_ok = summary.success_rate >= gates.reliability_min_success_rate;
            let latency_ok = summary
                .p50_ms
                .map(|value| value <= gates.latency_target_p50_ms)
                .unwrap_or(false)
                && summary
                    .p95_ms
                    .map(|value| value <= gates.latency_target_p95_ms)
                    .unwrap_or(false);
            let scenario_success_ok = min_success_in_any_scenario >= gates.min_success_per_scenario;
            let passes_release_gate = evaluated_for_release
                && preflight_ok
                && runtime_smoke_ok
                && reliability_ok
                && latency_ok
                && scenario_success_ok;
            TtsProviderGateEvaluation {
                provider: provider.clone(),
                evaluated_for_release,
                passes_release_gate,
                preflight_ok,
                runtime_smoke_ok,
                reliability_ok,
                latency_ok,
                scenario_success_ok,
                success_rate: summary.success_rate,
                p50_ms: summary.p50_ms,
                p95_ms: summary.p95_ms,
                min_success_in_any_scenario,
            }
        })
        .collect::<Vec<_>>();

    let (provider_consistency_ok, provider_consistency_detail) =
        provider_consistency_from_runtime_surface(&providers, true); // TODO: pass qwen3_tts_enabled from settings if this function gets State access

    let release_evaluations = provider_gate_evaluations
        .iter()
        .filter(|evaluation| evaluation.evaluated_for_release)
        .collect::<Vec<_>>();
    let release_gate_pass = !release_evaluations.is_empty()
        && release_evaluations
            .iter()
            .all(|evaluation| evaluation.passes_release_gate);
    let release_gate_reason = if release_gate_pass {
        "All runtime-stable providers passed release gates.".to_string()
    } else if release_evaluations.is_empty() {
        "No runtime-stable providers available for release gate evaluation.".to_string()
    } else {
        let failed = release_evaluations
            .iter()
            .filter(|evaluation| !evaluation.passes_release_gate)
            .map(|evaluation| evaluation.provider.clone())
            .collect::<Vec<_>>();
        format!("Release gate failed for providers: {}", failed.join(", "))
    };

    let runtime_summaries = provider_summaries
        .iter()
        .filter(|summary| is_runtime_stable_provider(&summary.provider))
        .cloned()
        .collect::<Vec<_>>();
    let fallback_order =
        build_tts_fallback_order(&runtime_summaries, gates.reliability_min_success_rate);

    let recommended_default_provider = if release_gate_pass {
        fallback_order.first().cloned()
    } else {
        fallback_order
            .iter()
            .find(|provider| {
                provider_gate_evaluations
                    .iter()
                    .find(|evaluation| &evaluation.provider == *provider)
                    .map(|evaluation| {
                        evaluation.preflight_ok
                            && evaluation.runtime_smoke_ok
                            && evaluation.success_rate > 0.0
                    })
                    .unwrap_or(false)
            })
            .cloned()
    };
    let recommendation_reason = if let Some(provider) = recommended_default_provider.as_ref() {
        if release_gate_pass {
            format!(
                "Selected '{}' as default (release gate pass; deterministic fallback order applied).",
                provider
            )
        } else {
            format!(
                "Selected '{}' as best available runtime fallback while release gate is failing.",
                provider
            )
        }
    } else {
        "No runtime provider recommendation available. Resolve preflight/smoke failures first."
            .to_string()
    };

    let uncategorized_failure_count = samples
        .iter()
        .filter(|sample| !sample.success && sample.failure_category.is_none())
        .count() as u32;

    Ok(TtsBenchmarkResult {
        artifact_version: "tts-benchmark-v2".to_string(),
        generated_at: now_iso(),
        warmup_runs,
        measure_runs,
        providers,
        scenarios: scenarios
            .iter()
            .map(|scenario| scenario.id.clone())
            .collect(),
        scenario_matrix_locked: request.lock_matrix,
        gates,
        provider_profiles,
        preflight_checks,
        runtime_smoke_checks,
        samples,
        provider_summaries,
        provider_gate_evaluations,
        provider_consistency_ok,
        provider_consistency_detail,
        fallback_order,
        release_gate_pass,
        release_gate_reason,
        recommended_default_provider,
        recommendation_reason,
        uncategorized_failure_count,
        warnings,
    })
}

impl TtsBenchmarkResult {
    /// Emit a structured summary of this benchmark report via `tracing`.
    ///
    /// Keeps the report shape encapsulated: callers do not need to read
    /// individual fields of `TtsBenchmarkResult` to log the outcome.
    pub(crate) fn log_summary(&self, report_path: &Path) {
        info!(
            "TTS benchmark complete: recommended_default={:?} release_gate_pass={} (report: {})",
            self.recommended_default_provider,
            self.release_gate_pass,
            report_path.display()
        );
        if let Some(provider) = self.recommended_default_provider.as_ref() {
            info!(
                "TTS benchmark recommendation: provider='{}' reason='{}'",
                provider, self.recommendation_reason
            );
        } else {
            warn!(
                "TTS benchmark produced no recommendation: {}",
                self.recommendation_reason
            );
        }
        if !self.release_gate_pass {
            warn!("TTS release gate failed: {}", self.release_gate_reason);
        }
        if self.uncategorized_failure_count > 0 {
            warn!(
                "TTS benchmark uncategorized failures: {}",
                self.uncategorized_failure_count
            );
        }
    }
}

pub(crate) fn write_tts_benchmark_report(result: &TtsBenchmarkResult) -> Result<PathBuf, String> {
    let root = resolve_benchmark_root_dir();
    let out_dir = root.join("bench").join("results");
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        format!(
            "Failed creating benchmark output dir '{}': {}",
            out_dir.display(),
            e
        )
    })?;
    let out_path = out_dir.join("tts.latest.json");
    let serialized = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, serialized).map_err(|e| {
        format!(
            "Failed writing TTS benchmark report '{}': {}",
            out_path.display(),
            e
        )
    })?;
    Ok(out_path)
}

#[tauri::command]
pub(crate) fn run_tts_benchmark(
    state: State<'_, AppState>,
    request: Option<TtsBenchmarkRequest>,
) -> Result<TtsBenchmarkResult, String> {
    let request = request.unwrap_or_default();
    run_tts_benchmark_inner(state.inner(), &request)
}

pub(crate) fn tts_benchmark_request_from_env() -> TtsBenchmarkRequest {
    let mut request = TtsBenchmarkRequest::default();

    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_WARMUP_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.warmup_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_MEASURE_RUNS") {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            request.measure_runs = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_RATE") {
        if let Ok(parsed) = value.trim().parse::<f32>() {
            request.rate = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_VOLUME") {
        if let Ok(parsed) = value.trim().parse::<f32>() {
            request.volume = parsed;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_PROVIDERS") {
        let providers = value
            .split(';')
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect::<Vec<_>>();
        if !providers.is_empty() {
            request.providers = providers;
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_PIPER_BINARY_PATH") {
        let path = value.trim();
        if !path.is_empty() {
            request.piper_binary_path = Some(path.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_PIPER_MODEL_PATH") {
        let path = value.trim();
        if !path.is_empty() {
            request.piper_model_path = Some(path.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_ENDPOINT") {
        let endpoint = value.trim();
        if !endpoint.is_empty() {
            request.qwen3_tts_endpoint = Some(endpoint.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_MODEL") {
        let model = value.trim();
        if !model.is_empty() {
            request.qwen3_tts_model = Some(model.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_VOICE") {
        let voice = value.trim();
        if !voice.is_empty() {
            request.qwen3_tts_voice = Some(voice.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_API_KEY") {
        let key = value.trim();
        if !key.is_empty() {
            request.qwen3_tts_api_key = Some(key.to_string());
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_QWEN3_TIMEOUT_SEC") {
        if let Ok(parsed) = value.trim().parse::<u64>() {
            request.qwen3_tts_timeout_sec = Some(parsed);
        }
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_LOCK_MATRIX") {
        request.lock_matrix = !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        );
    }
    if let Ok(value) = std::env::var("TRISPR_TTS_BENCHMARK_RUNTIME_SMOKE") {
        request.run_runtime_smoke = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
    }

    request
}
