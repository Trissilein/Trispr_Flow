use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(target_os = "windows")]
use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[cfg(target_os = "windows")]
fn apply_hidden_creation_flags(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn apply_hidden_creation_flags(_cmd: &mut Command) {}

fn file_is_non_empty(path: &std::path::Path) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|meta| meta.len() > 0)
            .unwrap_or(false)
}

const WINDOWS_NATURAL_NAME_HINTS: &[&str] = &[
    "aria", "conrad", "jenny", "guy", "ava", "libby", "sonia", "ryan",
];

fn windows_voice_matches_natural_profile(name: &str) -> bool {
    let normalized = name.trim().to_ascii_lowercase();
    normalized.contains("natural")
        || normalized.contains("multilingual")
        || normalized.contains("online")
        || WINDOWS_NATURAL_NAME_HINTS
            .iter()
            .any(|hint| normalized.contains(hint))
}

fn windows_natural_voice_priority(name: &str) -> u8 {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.contains("multilingual") {
        0
    } else if normalized.contains("natural") {
        1
    } else if normalized.contains("online") {
        2
    } else if WINDOWS_NATURAL_NAME_HINTS
        .iter()
        .any(|hint| normalized.contains(hint))
    {
        3
    } else {
        4
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionSourceInfo {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionStreamHealth {
    pub running: bool,
    pub fps: u8,
    pub source_scope: String,
    pub started_at_ms: Option<u64>,
    pub frame_seq: u64,
    pub buffered_frames: usize,
    pub buffered_bytes: usize,
    pub last_frame_timestamp_ms: Option<u64>,
    pub last_frame_width: Option<u32>,
    pub last_frame_height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionSnapshotResult {
    pub captured: bool,
    pub timestamp_ms: u64,
    pub source_count: usize,
    pub note: String,
    pub frame_seq: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: Option<usize>,
    pub source_scope: Option<String>,
    pub jpeg_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisionFrameMeta {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub source_scope: String,
    pub source_count: usize,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub buffered_frames: usize,
    pub buffered_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct VisionFrame {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub source_scope: String,
    pub source_count: usize,
    pub width: u32,
    pub height: u32,
    pub jpeg_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct VisionBufferStats {
    pub buffered_frames: usize,
    pub buffered_bytes: usize,
    pub last_frame_timestamp_ms: Option<u64>,
    pub last_frame_width: Option<u32>,
    pub last_frame_height: Option<u32>,
}

#[derive(Debug, Default)]
pub struct VisionFrameBuffer {
    frames: VecDeque<VisionFrame>,
    total_bytes: usize,
}

impl VisionFrameBuffer {
    pub fn clear(&mut self) {
        self.frames.clear();
        self.total_bytes = 0;
    }

    pub fn latest(&self) -> Option<&VisionFrame> {
        self.frames.back()
    }

    pub fn stats(&self) -> VisionBufferStats {
        VisionBufferStats {
            buffered_frames: self.frames.len(),
            buffered_bytes: self.total_bytes,
            last_frame_timestamp_ms: self.frames.back().map(|frame| frame.timestamp_ms),
            last_frame_width: self.frames.back().map(|frame| frame.width),
            last_frame_height: self.frames.back().map(|frame| frame.height),
        }
    }

    pub fn push(&mut self, frame: VisionFrame, max_frames: usize) -> VisionFrameMeta {
        self.total_bytes += frame.jpeg_bytes.len();
        self.frames.push_back(frame);

        let keep_frames = max_frames.max(1);
        while self.frames.len() > keep_frames {
            if let Some(removed) = self.frames.pop_front() {
                self.total_bytes = self.total_bytes.saturating_sub(removed.jpeg_bytes.len());
            }
        }

        let latest = self
            .frames
            .back()
            .expect("vision buffer just received a frame");
        VisionFrameMeta {
            seq: latest.seq,
            timestamp_ms: latest.timestamp_ms,
            source_scope: latest.source_scope.clone(),
            source_count: latest.source_count,
            width: latest.width,
            height: latest.height,
            bytes: latest.jpeg_bytes.len(),
            buffered_frames: self.frames.len(),
            buffered_bytes: self.total_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsProviderInfo {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub surface: String, // "runtime_stable" | "benchmark_experimental"
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoiceInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSpeakRequest {
    pub provider: String,
    pub text: String,
    pub rate: Option<f32>,
    pub volume: Option<f32>,
    /// Request context for policy enforcement.
    /// Supported values:
    /// - "agent_reply"
    /// - "agent_event"
    /// - "manual_user"
    /// - "manual_test"
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSpeakResult {
    pub provider_used: String,
    pub accepted: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TtsFallbackOutcome {
    pub provider_used: String,
    pub used_fallback: bool,
    pub primary_error: Option<String>,
}

fn is_windows_tts_provider(provider: &str) -> bool {
    provider == "windows_native" || provider == "windows_natural"
}

fn is_windows_audio_device_error(message: &str) -> bool {
    let lowered = message.to_ascii_lowercase();
    (lowered.contains("error code: 0x2")
        || lowered.contains("audioger")
        || lowered.contains("audio device error"))
        && lowered.contains("speak")
}

fn is_tts_audio_device_unavailable_tagged(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("[tts_audio_device_unavailable]")
}

fn windows_audio_device_error_hint(primary_error: &str) -> String {
    if is_tts_audio_device_unavailable_tagged(primary_error) {
        return primary_error.to_string();
    }
    format!(
        "[tts_audio_device_unavailable] {}. No default Windows playback device is currently available for SAPI output. Check default playback device / VoiceMeeter route / Windows Audio service, then retry.",
        primary_error
    )
}

pub fn execute_tts_with_fallback<F>(
    preferred_provider: &str,
    fallback_provider: &str,
    mut attempt: F,
) -> Result<TtsFallbackOutcome, String>
where
    F: FnMut(&str) -> Result<(), String>,
{
    match attempt(preferred_provider) {
        Ok(()) => Ok(TtsFallbackOutcome {
            provider_used: preferred_provider.to_string(),
            used_fallback: false,
            primary_error: None,
        }),
        Err(primary_error) => {
            if is_windows_audio_device_error(&primary_error)
                && is_windows_tts_provider(preferred_provider)
                && is_windows_tts_provider(fallback_provider)
            {
                return Err(windows_audio_device_error_hint(&primary_error));
            }

            if preferred_provider == fallback_provider {
                return Err(format!(
                    "[tts_fallback_no_alternative] Preferred provider '{}' failed and no distinct fallback is configured: {}",
                    preferred_provider, primary_error
                ));
            }
            if fallback_provider == "windows_natural" && !windows_natural_voice_available() {
                return Err(format!(
                    "[tts_fallback_unavailable] Preferred provider '{}' failed: {} | Fallback '{}' is unavailable (no Natural voice installed).",
                    preferred_provider, primary_error, fallback_provider
                ));
            }
            match attempt(fallback_provider) {
                Ok(()) => Ok(TtsFallbackOutcome {
                    provider_used: fallback_provider.to_string(),
                    used_fallback: true,
                    primary_error: Some(primary_error),
                }),
                Err(fallback_error) => Err(format!(
                    "[tts_fallback_both_failed] Preferred provider '{}' failed: {} | Fallback '{}' failed: {}",
                    preferred_provider, primary_error, fallback_provider, fallback_error
                )),
            }
        }
    }
}

pub fn is_tts_policy_allowed(policy: &str, context: &str) -> bool {
    let normalized_policy = policy.trim().to_lowercase();
    let normalized_context = context.trim().to_lowercase();
    match normalized_policy.as_str() {
        // Default mode: allow spoken agent replies and explicit provider tests.
        "agent_replies_only" => {
            normalized_context == "agent_reply" || normalized_context == "manual_test"
        }
        // Extended mode: allow both direct replies and event/status narration.
        "replies_and_events" => {
            normalized_context == "agent_reply"
                || normalized_context == "agent_event"
                || normalized_context == "manual_test"
        }
        // Explicit/manual mode: no autonomous agent speech.
        "explicit_only" => {
            normalized_context == "manual_user" || normalized_context == "manual_test"
        }
        _ => false,
    }
}

pub fn vision_snapshot_from_frame(frame: &VisionFrame, note: String) -> VisionSnapshotResult {
    VisionSnapshotResult {
        captured: true,
        timestamp_ms: frame.timestamp_ms,
        source_count: frame.source_count,
        note,
        frame_seq: Some(frame.seq),
        width: Some(frame.width),
        height: Some(frame.height),
        bytes: Some(frame.jpeg_bytes.len()),
        source_scope: Some(frame.source_scope.clone()),
        jpeg_base64: Some(base64::engine::general_purpose::STANDARD.encode(&frame.jpeg_bytes)),
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Copy)]
struct CaptureRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[cfg(target_os = "windows")]
fn resolve_capture_rect(
    app: &AppHandle,
    source_scope: &str,
) -> Result<(CaptureRect, usize), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window is unavailable for vision capture.".to_string())?;

    match source_scope {
        "active_monitor" | "active_window" => {
            let monitor = window
                .current_monitor()
                .map_err(|e| e.to_string())?
                .or_else(|| window.primary_monitor().ok().flatten())
                .ok_or_else(|| "No monitor is available for vision capture.".to_string())?;
            let pos = monitor.position();
            let size = monitor.size();
            Ok((
                CaptureRect {
                    x: pos.x,
                    y: pos.y,
                    width: size.width.max(1),
                    height: size.height.max(1),
                },
                1,
            ))
        }
        _ => {
            let monitors = window.available_monitors().map_err(|e| e.to_string())?;
            if monitors.is_empty() {
                return Err("No monitors are available for vision capture.".to_string());
            }

            let mut min_x = i32::MAX;
            let mut min_y = i32::MAX;
            let mut max_x = i32::MIN;
            let mut max_y = i32::MIN;
            for monitor in &monitors {
                let pos = monitor.position();
                let size = monitor.size();
                min_x = min_x.min(pos.x);
                min_y = min_y.min(pos.y);
                max_x = max_x.max(pos.x.saturating_add(size.width as i32));
                max_y = max_y.max(pos.y.saturating_add(size.height as i32));
            }

            Ok((
                CaptureRect {
                    x: min_x,
                    y: min_y,
                    width: (max_x - min_x).max(1) as u32,
                    height: (max_y - min_y).max(1) as u32,
                },
                monitors.len(),
            ))
        }
    }
}

#[cfg(target_os = "windows")]
fn capture_rect_jpeg(
    rect: CaptureRect,
    max_width: u16,
    jpeg_quality: u8,
) -> Result<Vec<u8>, String> {
    let max_width = max_width.clamp(640, 3840);
    let jpeg_quality = jpeg_quality.clamp(40, 95);
    let script = format!(
        r#"
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$rect = New-Object System.Drawing.Rectangle({x}, {y}, {width}, {height})
$bitmap = New-Object System.Drawing.Bitmap($rect.Width, $rect.Height)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Location, [System.Drawing.Point]::Empty, $rect.Size)
$image = $bitmap
if ($bitmap.Width -gt {max_width}) {{
    $scaledWidth = {max_width}
    $scaledHeight = [int][Math]::Round(($bitmap.Height * $scaledWidth) / [double]$bitmap.Width)
    $resized = New-Object System.Drawing.Bitmap($scaledWidth, $scaledHeight)
    $graphics2 = [System.Drawing.Graphics]::FromImage($resized)
    $graphics2.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics2.DrawImage($bitmap, 0, 0, $scaledWidth, $scaledHeight)
    $graphics2.Dispose()
    $image = $resized
}}
$stream = New-Object System.IO.MemoryStream
$codec = [System.Drawing.Imaging.ImageCodecInfo]::GetImageEncoders() | Where-Object {{ $_.MimeType -eq 'image/jpeg' }} | Select-Object -First 1
$encoder = [System.Drawing.Imaging.Encoder]::Quality
$parameters = New-Object System.Drawing.Imaging.EncoderParameters(1)
$parameters.Param[0] = New-Object System.Drawing.Imaging.EncoderParameter($encoder, [long]{jpeg_quality})
$image.Save($stream, $codec, $parameters)
[Console]::Out.Write([Convert]::ToBase64String($stream.ToArray()))
$stream.Dispose()
if ($image -ne $bitmap) {{ $image.Dispose() }}
$graphics.Dispose()
$bitmap.Dispose()
"#,
        x = rect.x,
        y = rect.y,
        width = rect.width,
        height = rect.height,
        max_width = max_width,
        jpeg_quality = jpeg_quality
    );
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-Command", &script]);
    apply_hidden_creation_flags(&mut cmd);
    let output = cmd
        .output()
        .map_err(|error| format!("Failed to start vision capture: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "Vision capture failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("Vision capture returned invalid UTF-8: {error}"))?;
    base64::engine::general_purpose::STANDARD
        .decode(stdout.trim())
        .map_err(|error| format!("Vision capture returned invalid base64: {error}"))
}

#[cfg(target_os = "windows")]
pub fn capture_vision_frame(
    app: &AppHandle,
    source_scope: &str,
    max_width: u16,
    jpeg_quality: u8,
) -> Result<VisionFrame, String> {
    let (rect, source_count) = resolve_capture_rect(app, source_scope)?;
    let jpeg_bytes = capture_rect_jpeg(rect, max_width, jpeg_quality)?;
    let image = image::load_from_memory(&jpeg_bytes)
        .map_err(|error| format!("Vision capture decode failed: {error}"))?;

    Ok(VisionFrame {
        seq: 0,
        timestamp_ms: crate::util::now_ms(),
        source_scope: source_scope.to_string(),
        source_count,
        width: image.width(),
        height: image.height(),
        jpeg_bytes,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn capture_vision_frame(
    _app: &tauri::AppHandle,
    _source_scope: &str,
    _max_width: u16,
    _jpeg_quality: u8,
) -> Result<VisionFrame, String> {
    Err("Vision capture is currently available on Windows only.".to_string())
}

pub fn list_tts_providers(qwen3_tts_enabled: bool) -> Vec<TtsProviderInfo> {
    let mut providers = vec![
        TtsProviderInfo {
            id: "windows_native".to_string(),
            label: "Windows Native TTS".to_string(),
            available: cfg!(target_os = "windows"),
            surface: "runtime_stable".to_string(),
            reason: None,
        },
        TtsProviderInfo {
            id: "windows_natural".to_string(),
            label: "Windows Natural Language (SAPI Adapter)".to_string(),
            available: windows_natural_voice_available(),
            surface: "runtime_stable".to_string(),
            reason: Some(
                "Requires NaturalVoiceSAPIAdapter (or equivalent SAPI natural voices) and at least one installed Natural voice."
                    .to_string(),
            ),
        },
        TtsProviderInfo {
            id: "local_custom".to_string(),
            label: "Local Custom TTS (Piper)".to_string(),
            available: resolve_piper_binary("").is_some(),
            surface: "runtime_stable".to_string(),
            reason: None,
        },
    ];
    if qwen3_tts_enabled {
        providers.push(TtsProviderInfo {
            id: "qwen3_tts".to_string(),
            label: "Qwen3-TTS (OpenAI-compatible endpoint)".to_string(),
            available: true,
            surface: "benchmark_experimental".to_string(),
            reason: Some(
                "Experimental runtime provider. Requires a running OpenAI-compatible /v1/audio/speech endpoint."
                    .to_string(),
            ),
        });
    }
    providers
}

#[cfg(target_os = "windows")]
fn run_hidden_powershell(script: &str, action_label: &str) -> Result<String, String> {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-Command", script]);
    apply_hidden_creation_flags(&mut cmd);
    let output = cmd
        .output()
        .map_err(|error| format!("Failed to start {}: {}", action_label, error))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed with status {:?}: {}",
            action_label,
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Debug, Clone, Deserialize)]
struct WindowsVoiceRecord {
    name: String,
    locale: Option<String>,
}

#[cfg(target_os = "windows")]
fn windows_voice_profile(name: &str) -> &'static str {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.contains("multilingual") {
        "multilingual"
    } else if normalized.contains("natural") {
        "natural"
    } else if normalized.contains("online") {
        "online"
    } else if WINDOWS_NATURAL_NAME_HINTS
        .iter()
        .any(|hint| normalized.contains(hint))
    {
        "natural"
    } else {
        "standard"
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_voice_profile(_name: &str) -> &'static str {
    "standard"
}

#[cfg(target_os = "windows")]
fn list_windows_voice_records() -> Result<Vec<WindowsVoiceRecord>, String> {
    let script = "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; try { $voices = @($s.GetInstalledVoices() | ForEach-Object { $info = $_.VoiceInfo; [PSCustomObject]@{ name = $info.Name; locale = if ($info.Culture -ne $null) { $info.Culture.Name } else { '' } } }); $voices | ConvertTo-Json -Compress } finally { $s.Dispose() }";
    let stdout = run_hidden_powershell(script, "Windows voice list")?;
    let payload = stdout.trim();
    if payload.is_empty() {
        return Ok(Vec::new());
    }
    let parsed: serde_json::Value = serde_json::from_str(payload).map_err(|error| {
        format!(
            "Failed to parse Windows voice metadata payload: {} | payload={}",
            error, payload
        )
    })?;
    let mut records: Vec<WindowsVoiceRecord> = match parsed {
        serde_json::Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value::<WindowsVoiceRecord>(item).ok())
            .collect(),
        serde_json::Value::Object(_) => vec![serde_json::from_value::<WindowsVoiceRecord>(parsed)
            .map_err(|error| {
                format!(
                    "Failed to parse single Windows voice metadata payload: {} | payload={}",
                    error, payload
                )
            })?],
        _ => Vec::new(),
    };
    records.retain(|record| !record.name.trim().is_empty());
    records.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
            .then_with(|| a.locale.cmp(&b.locale))
    });
    records.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    Ok(records)
}

#[cfg(not(target_os = "windows"))]
fn list_windows_voice_records() -> Result<Vec<WindowsVoiceRecord>, String> {
    Err("Windows voices are only available on Windows.".to_string())
}

fn list_windows_voices_filtered(provider: &str, natural_only: bool) -> Vec<TtsVoiceInfo> {
    let Ok(records) = list_windows_voice_records() else {
        return Vec::new();
    };

    let mut filtered = if natural_only {
        records
            .into_iter()
            .filter(|record| windows_voice_matches_natural_profile(&record.name))
            .collect::<Vec<_>>()
    } else {
        records
    };

    if natural_only {
        filtered.sort_by(|a, b| {
            windows_natural_voice_priority(&a.name)
                .cmp(&windows_natural_voice_priority(&b.name))
                .then_with(|| {
                    a.name
                        .to_ascii_lowercase()
                        .cmp(&b.name.to_ascii_lowercase())
                })
        });
    }

    filtered
        .into_iter()
        .map(|record| TtsVoiceInfo {
            id: record.name.clone(),
            label: record.name.clone(),
            provider: provider.to_string(),
            locale: record
                .locale
                .as_deref()
                .map(str::trim)
                .filter(|locale| !locale.is_empty())
                .map(|locale| locale.to_string()),
            profile: Some(windows_voice_profile(&record.name).to_string()),
        })
        .collect()
}

#[cfg(target_os = "windows")]
pub fn windows_natural_voice_available() -> bool {
    !list_windows_voices_filtered("windows_natural", true).is_empty()
}

#[cfg(not(target_os = "windows"))]
pub fn windows_natural_voice_available() -> bool {
    false
}

fn list_windows_voices() -> Vec<TtsVoiceInfo> {
    if !cfg!(target_os = "windows") {
        return Vec::new();
    }
    let mut voices = list_windows_voices_filtered("windows_native", false);
    if voices.is_empty() {
        voices.push(TtsVoiceInfo {
            id: "windows_default".to_string(),
            label: "Windows Default Voice".to_string(),
            provider: "windows_native".to_string(),
            locale: None,
            profile: None,
        });
    }
    voices
}

fn list_windows_natural_voices() -> Vec<TtsVoiceInfo> {
    if !cfg!(target_os = "windows") {
        return Vec::new();
    }
    let mut voices = list_windows_voices_filtered("windows_natural", true);
    if voices.is_empty() {
        voices.push(TtsVoiceInfo {
            id: "windows_natural_unavailable".to_string(),
            label: "No Natural voices detected".to_string(),
            provider: "windows_natural".to_string(),
            locale: None,
            profile: Some("missing".to_string()),
        });
    }
    voices
}

fn list_qwen3_tts_voices() -> Vec<TtsVoiceInfo> {
    const QWEN_CUSTOM_VOICES: &[&str] = &[
        "vivian", "serena", "dylan", "eric", "ryan", "aiden", "sohee", "ono_anna", "uncle_fu",
    ];
    QWEN_CUSTOM_VOICES
        .iter()
        .map(|voice| TtsVoiceInfo {
            id: (*voice).to_string(),
            label: (*voice).to_string(),
            provider: "qwen3_tts".to_string(),
            locale: None,
            profile: None,
        })
        .collect()
}

pub fn list_tts_voices(provider: &str) -> Vec<TtsVoiceInfo> {
    match provider {
        "windows_native" => list_windows_voices(),
        "windows_natural" => list_windows_natural_voices(),
        "qwen3_tts" => list_qwen3_tts_voices(),
        _ => Vec::new(),
    }
}

fn normalize_language_hint(language_hint: &str) -> Option<(String, Option<String>)> {
    let normalized = language_hint.trim().to_ascii_lowercase().replace('_', "-");
    if normalized.is_empty() {
        return None;
    }
    let mut parts = normalized.split('-');
    let language = parts.next()?.trim().to_string();
    if language.len() < 2 {
        return None;
    }
    let language = language.chars().take(2).collect::<String>();
    if language.is_empty() {
        return None;
    }
    let region = parts
        .next()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.chars().take(2).collect::<String>());
    Some((language, region))
}

fn select_voice_from_candidates_for_language(
    candidates: &[TtsVoiceInfo],
    language_hint: &str,
) -> Option<String> {
    let (language, region) = normalize_language_hint(language_hint)?;
    let exact = region
        .as_ref()
        .map(|value| format!("{}-{}", language, value));

    let mut best: Option<(i32, String)> = None;
    for voice in candidates {
        let mut score = 0_i32;
        let locale = voice
            .locale
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .replace('_', "-");
        if let Some(exact_locale) = exact.as_ref() {
            if !locale.is_empty() && locale == *exact_locale {
                score = 120;
            }
        }
        if score == 0
            && !locale.is_empty()
            && (locale.starts_with(&format!("{}-", language)) || locale == language)
        {
            score = 90;
        }
        if score == 0 {
            let id = voice.id.to_ascii_lowercase();
            if id.contains(&format!("{}-", language))
                || id.contains(&format!("_{}_", language))
                || id.contains(&format!(" {}", language))
            {
                score = 55;
            }
        }
        if score == 0 {
            continue;
        }
        match voice.profile.as_deref().unwrap_or_default() {
            "multilingual" => score += 4,
            "natural" => score += 2,
            "online" => score += 1,
            _ => {}
        }
        let current = (score, voice.id.clone());
        if let Some(existing) = &best {
            if current.0 > existing.0
                || (current.0 == existing.0
                    && current.1.to_ascii_lowercase() < existing.1.to_ascii_lowercase())
            {
                best = Some(current);
            }
        } else {
            best = Some(current);
        }
    }

    best.map(|(_, id)| id)
}

pub fn select_windows_voice_for_language(provider: &str, language_hint: &str) -> Option<String> {
    if provider != "windows_native" && provider != "windows_natural" {
        return None;
    }
    let natural_only = provider == "windows_natural";
    let candidates = list_windows_voices_filtered(provider, natural_only);
    select_voice_from_candidates_for_language(&candidates, language_hint)
}

/// Scan the resolved piper voice model directory for `.onnx` files.
/// `model_dir` overrides auto-discovery when non-empty; otherwise the bundled
/// installer path and %LOCALAPPDATA% fallback are tried automatically.
pub fn list_piper_voices(model_dir: &str) -> Vec<TtsVoiceInfo> {
    let dir = match resolve_piper_model_dir(model_dir) {
        Some(d) => d,
        None => return Vec::new(),
    };

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut voices: Vec<TtsVoiceInfo> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map_or(false, |ext| ext.eq_ignore_ascii_case("onnx"))
        })
        .map(|e| {
            let path = e.path();
            let label = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            TtsVoiceInfo {
                id: path.to_string_lossy().to_string(), // full path used as ID
                label,
                provider: "local_custom".to_string(),
                locale: None,
                profile: None,
            }
        })
        .collect();

    voices.sort_by(|a, b| a.label.cmp(&b.label));
    voices
}

#[cfg(target_os = "windows")]
fn build_windows_sapi_speech_script(
    text: &str,
    rate: f32,
    volume: f32,
    natural_only: bool,
    selected_voice: Option<&str>,
    benchmark_to_file: bool,
) -> String {
    let text = text.trim();
    let rate = rate.clamp(0.5, 2.0);
    let volume = volume.clamp(0.0, 1.0);
    let sapi_rate = (((rate - 1.0) * 10.0).round() as i32).clamp(-10, 10);
    let sapi_volume = ((volume * 100.0).round() as i32).clamp(0, 100);
    let escaped_text = text.replace('\'', "''");
    let selected_voice = selected_voice
        .map(str::trim)
        .filter(|voice| !voice.is_empty())
        .unwrap_or("");
    let escaped_selected_voice = selected_voice.replace('\'', "''");
    let voice_selection = if natural_only {
        format!(
            "$preferred = '{escaped_selected_voice}'; \
             $installed = @($s.GetInstalledVoices() | ForEach-Object {{ $_.VoiceInfo.Name }}); \
             if (-not [string]::IsNullOrWhiteSpace($preferred)) {{ \
               if ($installed -contains $preferred) {{ $voice = $preferred }} \
               else {{ throw \"Configured Windows voice '$preferred' is not installed.\" }} \
             }} else {{ \
               $candidates = $installed | Where-Object {{ $_ -match 'Natural|Multilingual|Online|Aria|Conrad|Jenny|Guy|Ava|Libby|Sonia|Ryan' }}; \
               $voice = $candidates | Sort-Object \
                 @{{Expression={{ if ($_ -match 'Multilingual') {{ 0 }} elseif ($_ -match 'Natural') {{ 1 }} elseif ($_ -match 'Online') {{ 2 }} else {{ 3 }} }}}}, \
                 @{{Expression={{ $_ }}}} | Select-Object -First 1; \
             }} \
             if ([string]::IsNullOrWhiteSpace($voice)) {{ \
               throw 'No Windows Natural voice found. Install NaturalVoiceSAPIAdapter and at least one natural voice.' \
             }}; \
             $s.SelectVoice($voice);"
        )
    } else {
        format!(
            "$preferred = '{escaped_selected_voice}'; if (-not [string]::IsNullOrWhiteSpace($preferred)) {{ $installed = @($s.GetInstalledVoices() | ForEach-Object {{ $_.VoiceInfo.Name }}); if ($installed -contains $preferred) {{ $s.SelectVoice($preferred) }} else {{ throw \"Configured Windows voice '$preferred' is not installed.\" }} }}"
        )
    };
    if benchmark_to_file {
        format!(
            "$wav=[System.IO.Path]::ChangeExtension([System.IO.Path]::GetTempFileName(),'wav'); Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; try {{ {voice_selection} $s.Rate = {sapi_rate}; $s.Volume = {sapi_volume}; $s.SetOutputToWaveFile($wav); $s.Speak('{escaped_text}'); $s.SetOutputToNull(); }} finally {{ $s.Dispose(); Remove-Item $wav -Force -ErrorAction SilentlyContinue }}"
        )
    } else {
        format!(
            "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; try {{ {voice_selection} $s.Rate = {sapi_rate}; $s.Volume = {sapi_volume}; $s.Speak('{escaped_text}'); }} finally {{ $s.Dispose() }}"
        )
    }
}

#[cfg(target_os = "windows")]
fn build_windows_sapi_wave_export_script(
    text: &str,
    rate: f32,
    volume: f32,
    natural_only: bool,
    selected_voice: Option<&str>,
) -> String {
    let text = text.trim();
    let rate = rate.clamp(0.5, 2.0);
    let volume = volume.clamp(0.0, 1.0);
    let sapi_rate = (((rate - 1.0) * 10.0).round() as i32).clamp(-10, 10);
    let sapi_volume = ((volume * 100.0).round() as i32).clamp(0, 100);
    let escaped_text = text.replace('\'', "''");
    let selected_voice = selected_voice
        .map(str::trim)
        .filter(|voice| !voice.is_empty())
        .unwrap_or("");
    let escaped_selected_voice = selected_voice.replace('\'', "''");
    let voice_selection = if natural_only {
        format!(
            "$preferred = '{escaped_selected_voice}'; \
             $installed = @($s.GetInstalledVoices() | ForEach-Object {{ $_.VoiceInfo.Name }}); \
             if (-not [string]::IsNullOrWhiteSpace($preferred)) {{ \
               if ($installed -contains $preferred) {{ $voice = $preferred }} \
               else {{ throw \"Configured Windows voice '$preferred' is not installed.\" }} \
             }} else {{ \
               $candidates = $installed | Where-Object {{ $_ -match 'Natural|Multilingual|Online|Aria|Conrad|Jenny|Guy|Ava|Libby|Sonia|Ryan' }}; \
               $voice = $candidates | Sort-Object \
                 @{{Expression={{ if ($_ -match 'Multilingual') {{ 0 }} elseif ($_ -match 'Natural') {{ 1 }} elseif ($_ -match 'Online') {{ 2 }} else {{ 3 }} }}}}, \
                 @{{Expression={{ $_ }}}} | Select-Object -First 1; \
             }} \
             if ([string]::IsNullOrWhiteSpace($voice)) {{ \
               throw 'No Windows Natural voice found. Install NaturalVoiceSAPIAdapter and at least one natural voice.' \
             }}; \
             $s.SelectVoice($voice);"
        )
    } else {
        format!(
            "$preferred = '{escaped_selected_voice}'; if (-not [string]::IsNullOrWhiteSpace($preferred)) {{ $installed = @($s.GetInstalledVoices() | ForEach-Object {{ $_.VoiceInfo.Name }}); if ($installed -contains $preferred) {{ $s.SelectVoice($preferred) }} else {{ throw \"Configured Windows voice '$preferred' is not installed.\" }} }}"
        )
    };
    format!(
        "$wav=[System.IO.Path]::ChangeExtension([System.IO.Path]::GetTempFileName(),'wav'); Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; try {{ {voice_selection} $s.Rate = {sapi_rate}; $s.Volume = {sapi_volume}; $s.SetOutputToWaveFile($wav); $s.Speak('{escaped_text}'); $s.SetOutputToNull(); Write-Output $wav; }} finally {{ $s.Dispose() }}"
    )
}

#[cfg(target_os = "windows")]
fn synthesize_windows_sapi_to_wav(
    text: &str,
    rate: f32,
    volume: f32,
    natural_only: bool,
    selected_voice: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let script =
        build_windows_sapi_wave_export_script(text, rate, volume, natural_only, selected_voice);
    let stdout = run_hidden_powershell(&script, "Windows TTS WAV synthesis")?;
    let wav_path = stdout
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "Windows TTS WAV synthesis produced no output path.".to_string())?;
    let path = std::path::PathBuf::from(wav_path);
    if !file_is_non_empty(&path) {
        return Err(format!(
            "Windows TTS WAV synthesis produced no playable file at '{}'.",
            path.display()
        ));
    }
    Ok(path)
}

#[cfg(target_os = "windows")]
fn speak_windows_sapi(
    text: &str,
    rate: f32,
    volume: f32,
    natural_only: bool,
    output_device_id: &str,
    selected_voice: Option<&str>,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("TTS text is empty.".to_string());
    }
    let output_device_id = {
        let trimmed = output_device_id.trim();
        if trimmed.is_empty() {
            "default"
        } else {
            trimmed
        }
    };
    if output_device_id != "default" {
        let wav_path =
            synthesize_windows_sapi_to_wav(text, rate, volume, natural_only, selected_voice)?;
        let play_result = play_wav_blocking(&wav_path, volume, output_device_id);
        let _ = std::fs::remove_file(&wav_path);
        return play_result.map_err(|play_error| {
            format!(
                "Windows TTS playback via selected device '{}' failed: {}",
                output_device_id, play_error
            )
        });
    }
    let script =
        build_windows_sapi_speech_script(text, rate, volume, natural_only, selected_voice, false);
    match run_hidden_powershell(&script, "Windows TTS") {
        Ok(_) => Ok(()),
        Err(error) => {
            if !is_windows_audio_device_error(&error) {
                return Err(error);
            }
            let hinted = windows_audio_device_error_hint(&error);
            match synthesize_windows_sapi_to_wav(text, rate, volume, natural_only, selected_voice) {
                Ok(wav_path) => {
                    let play_result = play_wav_blocking(&wav_path, volume, output_device_id);
                    let _ = std::fs::remove_file(&wav_path);
                    play_result.map_err(|play_error| {
                        format!(
                            "{} | Secondary fallback (SAPI->WAV playback) failed: {}",
                            hinted, play_error
                        )
                    })
                }
                Err(synth_error) => Err(format!(
                    "{} | Secondary fallback (SAPI->WAV synthesis) failed: {}",
                    hinted, synth_error
                )),
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn benchmark_windows_sapi_synthesis(
    text: &str,
    rate: f32,
    volume: f32,
    natural_only: bool,
    selected_voice: Option<&str>,
) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("TTS text is empty.".to_string());
    }
    let script =
        build_windows_sapi_speech_script(text, rate, volume, natural_only, selected_voice, true);
    run_hidden_powershell(&script, "Windows TTS benchmark synthesis").map(|_| ())
}

#[cfg(target_os = "windows")]
pub fn speak_windows_native(
    text: &str,
    rate: f32,
    volume: f32,
    output_device_id: &str,
    selected_voice: Option<&str>,
) -> Result<(), String> {
    speak_windows_sapi(text, rate, volume, false, output_device_id, selected_voice)
}

#[cfg(not(target_os = "windows"))]
pub fn speak_windows_native(
    _text: &str,
    _rate: f32,
    _volume: f32,
    _output_device_id: &str,
    _selected_voice: Option<&str>,
) -> Result<(), String> {
    Err("Windows native TTS is only available on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub fn speak_windows_natural(
    text: &str,
    rate: f32,
    volume: f32,
    output_device_id: &str,
    selected_voice: Option<&str>,
) -> Result<(), String> {
    speak_windows_sapi(text, rate, volume, true, output_device_id, selected_voice)
}

#[cfg(not(target_os = "windows"))]
pub fn speak_windows_natural(
    _text: &str,
    _rate: f32,
    _volume: f32,
    _output_device_id: &str,
    _selected_voice: Option<&str>,
) -> Result<(), String> {
    Err("Windows natural TTS is only available on Windows.".to_string())
}

#[cfg(target_os = "windows")]
pub fn benchmark_windows_native_synthesis(
    text: &str,
    rate: f32,
    volume: f32,
    selected_voice: Option<&str>,
) -> Result<(), String> {
    benchmark_windows_sapi_synthesis(text, rate, volume, false, selected_voice)
}

#[cfg(target_os = "windows")]
pub fn benchmark_windows_natural_synthesis(
    text: &str,
    rate: f32,
    volume: f32,
    selected_voice: Option<&str>,
) -> Result<(), String> {
    benchmark_windows_sapi_synthesis(text, rate, volume, true, selected_voice)
}

#[cfg(not(target_os = "windows"))]
pub fn benchmark_windows_native_synthesis(
    _text: &str,
    _rate: f32,
    _volume: f32,
    _selected_voice: Option<&str>,
) -> Result<(), String> {
    Err("Windows native TTS is only available on Windows.".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn benchmark_windows_natural_synthesis(
    _text: &str,
    _rate: f32,
    _volume: f32,
    _selected_voice: Option<&str>,
) -> Result<(), String> {
    Err("Windows natural TTS is only available on Windows.".to_string())
}

// ---------------------------------------------------------------------------
// Piper TTS — local neural voice engine
// ---------------------------------------------------------------------------

/// Resolve the piper binary path.
/// Search order:
///   1. Configured path (settings.piper_binary_path)
///   2. PATH
///   3. Tauri resource dir: <exe_dir>/resources/bin/piper/piper.exe  (bundled with installer)
///   4. %LOCALAPPDATA%\trispr-flow\piper\piper.exe                   (manual install)
fn resolve_piper_binary(configured: &str) -> Option<std::path::PathBuf> {
    if !configured.is_empty() {
        let p = std::path::PathBuf::from(configured);
        if file_is_non_empty(&p) {
            return Some(p);
        }
    }
    if let Ok(p) = which::which("piper") {
        if file_is_non_empty(&p) {
            return Some(p);
        }
    }
    if let Ok(p) = which::which("piper.exe") {
        if file_is_non_empty(&p) {
            return Some(p);
        }
    }
    // Local source checkout paths while developing from repository root or src-tauri/.
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(6) {
            let candidates = [
                ancestor.join("bin").join("piper").join("piper.exe"),
                ancestor
                    .join("src-tauri")
                    .join("bin")
                    .join("piper")
                    .join("piper.exe"),
                ancestor.join("piper").join("piper.exe"),
                ancestor.join("piper").join("build").join("piper.exe"),
            ];
            for candidate in candidates {
                if file_is_non_empty(&candidate) {
                    return Some(candidate);
                }
            }
            if let Some(parent) = ancestor.parent() {
                let sibling_candidates = [
                    parent.join("piper").join("piper.exe"),
                    parent.join("piper").join("build").join("piper.exe"),
                ];
                for candidate in sibling_candidates {
                    if file_is_non_empty(&candidate) {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    // Bundled with installer: <exe_dir>/resources/bin/piper/piper.exe
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled = exe_dir
                .join("resources")
                .join("bin")
                .join("piper")
                .join("piper.exe");
            if file_is_non_empty(&bundled) {
                return Some(bundled);
            }
        }
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let p = std::path::PathBuf::from(local_app_data)
            .join("trispr-flow")
            .join("piper")
            .join("piper.exe");
        if file_is_non_empty(&p) {
            return Some(p);
        }
    }
    None
}

/// Resolve the piper voice model directory.
/// Search order:
///   1. Configured path (settings.piper_model_dir)
///   2. Tauri resource dir: <exe_dir>/resources/bin/piper/voices/  (bundled with installer)
///   3. %LOCALAPPDATA%\trispr-flow\piper\voices\                    (manual install)
fn resolve_piper_model_dir(configured: &str) -> Option<std::path::PathBuf> {
    if !configured.is_empty() {
        let p = std::path::PathBuf::from(configured);
        if p.is_dir() {
            return Some(p);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        for ancestor in cwd.ancestors().take(6) {
            let candidates = [
                ancestor.join("bin").join("piper").join("voices"),
                ancestor
                    .join("src-tauri")
                    .join("bin")
                    .join("piper")
                    .join("voices"),
                ancestor.join("piper").join("voices"),
                ancestor.join("piper").join("models"),
            ];
            for candidate in candidates {
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
            if let Some(parent) = ancestor.parent() {
                let sibling_candidates = [
                    parent.join("piper").join("voices"),
                    parent.join("piper").join("models"),
                ];
                for candidate in sibling_candidates {
                    if candidate.is_dir() {
                        return Some(candidate);
                    }
                }
            }
        }
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled = exe_dir
                .join("resources")
                .join("bin")
                .join("piper")
                .join("voices");
            if bundled.is_dir() {
                return Some(bundled);
            }
        }
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let p = std::path::PathBuf::from(local_app_data)
            .join("trispr-flow")
            .join("piper")
            .join("voices");
        if p.is_dir() {
            return Some(p);
        }
    }
    None
}

/// Download a Piper voice model to `%LOCALAPPDATA%\trispr-flow\piper\voices\`.
///
/// `lang_code` must be one of the known bundled voices:
/// - `"de_DE-thorsten-medium"` (~53 MB)
/// - `"en_US-amy-medium"` (~63 MB)
///
/// Both `.onnx` and `.onnx.json` files are downloaded.
/// Skips silently if the files already exist.
/// Returns the path to the `.onnx` file on success.
pub fn download_piper_voice(lang_code: &str) -> Result<std::path::PathBuf, String> {
    let voices_dir = std::env::var_os("LOCALAPPDATA")
        .map(|d| {
            std::path::PathBuf::from(d)
                .join("trispr-flow")
                .join("piper")
                .join("voices")
        })
        .ok_or_else(|| "LOCALAPPDATA not set".to_string())?;

    std::fs::create_dir_all(&voices_dir)
        .map_err(|e| format!("Cannot create voices dir: {e}"))?;

    let onnx_path = voices_dir.join(format!("{lang_code}.onnx"));
    let json_path = voices_dir.join(format!("{lang_code}.onnx.json"));

    if file_is_non_empty(&onnx_path) && json_path.exists() {
        return Ok(onnx_path);
    }

    let hf_path = match lang_code {
        "de_DE-thorsten-medium" => "de/de_DE/thorsten/medium",
        "en_US-amy-medium" => "en/en_US/amy/medium",
        other => return Err(format!("Unknown voice model: {other}")),
    };

    let base = "https://huggingface.co/rhasspy/piper-voices/resolve/main";

    for (url, dest) in [
        (
            format!("{base}/{hf_path}/{lang_code}.onnx?download=true"),
            &onnx_path,
        ),
        (
            format!("{base}/{hf_path}/{lang_code}.onnx.json?download=true"),
            &json_path,
        ),
    ] {
        if file_is_non_empty(dest) {
            continue;
        }
        tracing::info!("[piper] Downloading voice file: {url}");
        let resp = ureq::get(&url)
            .call()
            .map_err(|e| format!("Voice download failed for {lang_code}: {e}"))?;
        let mut reader = resp.into_reader();
        let mut out = std::fs::File::create(dest)
            .map_err(|e| format!("Cannot write {}: {e}", dest.display()))?;
        std::io::copy(&mut reader, &mut out)
            .map_err(|e| format!("Cannot write voice data for {lang_code}: {e}"))?;
    }

    if file_is_non_empty(&onnx_path) {
        tracing::info!("[piper] Voice model ready: {}", onnx_path.display());
        Ok(onnx_path)
    } else {
        Err(format!("Voice model download succeeded but {lang_code}.onnx is empty"))
    }
}

pub fn piper_binary_available(configured: &str) -> bool {
    resolve_piper_binary(configured).is_some()
}

pub fn piper_model_available(model_path: &str, model_dir: &str) -> bool {
    if !model_path.trim().is_empty() {
        return std::path::Path::new(model_path.trim()).is_file();
    }
    !list_piper_voices(model_dir).is_empty()
}

/// Synthesise `text` with Piper and play the result synchronously.
///
/// `rate` controls speech speed (0.5 = half speed, 2.0 = double speed).
/// Piper maps this to `--length_scale` (inverse: 1/rate).
/// `volume` scales WAV samples before playback (0.0..1.0).
fn synthesize_piper_to_wav(
    text: &str,
    binary_path: &str,
    model_path: &str,
    rate: f32,
    output_path: &std::path::Path,
) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    let text = text.trim();
    if text.is_empty() {
        return Err("TTS text is empty.".to_string());
    }

    let binary = resolve_piper_binary(binary_path).ok_or_else(|| {
        "Piper TTS binary not found. Install piper or set piper_binary_path in Voice Output settings.".to_string()
    })?;

    // Resolve model path: use explicit setting, else auto-pick the first voice from the voices dir.
    let resolved_model: String = if !model_path.is_empty() {
        if !std::path::Path::new(model_path).is_file() {
            return Err(format!("Piper model not found: {model_path}"));
        }
        model_path.to_string()
    } else {
        // Auto-discover: take the first .onnx in the bundled/configured voices dir.
        let voices = list_piper_voices("");
        if let Some(v) = voices.into_iter().next() {
            v.id
        } else {
            // No voice found — attempt lazy download of the default German voice.
            tracing::info!("[piper] No voice model found locally, attempting on-demand download.");
            match download_piper_voice("de_DE-thorsten-medium") {
                Ok(p) => p.to_string_lossy().to_string(),
                Err(e) => {
                    return Err(format!(
                        "No Piper voice model found and auto-download failed: {e}. \
                         Connect to the internet and restart the app, or set piper_model_path manually."
                    ));
                }
            }
        }
    };
    let model_path = resolved_model.as_str();

    // Piper's --length_scale is the inverse of speed rate.
    let length_scale = format!("{:.3}", (1.0_f32 / rate.clamp(0.25, 4.0)));

    let mut cmd = Command::new(&binary);
    cmd.args([
        "--model",
        model_path,
        "--output_file",
        output_path.to_str().unwrap_or(""),
        "--length_scale",
        &length_scale,
    ]);
    if let Some(binary_dir) = binary.parent() {
        cmd.current_dir(binary_dir);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start piper: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }

    let status = child
        .wait()
        .map_err(|e| format!("Piper process error: {e}"))?;

    if !status.success() {
        return Err(format!("Piper exited with status {status}"));
    }

    if !output_path.is_file() {
        return Err("Piper produced no output file.".to_string());
    }

    Ok(())
}

pub fn speak_piper(
    text: &str,
    binary_path: &str,
    model_path: &str,
    rate: f32,
    volume: f32,
    output_device_id: &str,
) -> Result<(), String> {
    // Unique temp file per call to avoid collisions when called concurrently.
    let temp_path = std::env::temp_dir().join(format!(
        "trispr_tts_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    synthesize_piper_to_wav(text, binary_path, model_path, rate, &temp_path)?;
    let play_result = play_wav_blocking(&temp_path, volume, output_device_id);
    let _ = std::fs::remove_file(&temp_path);
    play_result
}

pub fn play_wav_bytes(bytes: &[u8], volume: f32, output_device_id: &str) -> Result<(), String> {
    if bytes.is_empty() {
        return Err("WAV payload is empty.".to_string());
    }
    let temp_path = std::env::temp_dir().join(format!(
        "trispr_tts_qwen_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    std::fs::write(&temp_path, bytes)
        .map_err(|error| format!("Failed to write temporary WAV file: {error}"))?;
    let play_result = play_wav_blocking(&temp_path, volume, output_device_id);
    let _ = std::fs::remove_file(&temp_path);
    play_result
}

pub fn benchmark_piper_synthesis(
    text: &str,
    binary_path: &str,
    model_path: &str,
    rate: f32,
) -> Result<(), String> {
    let temp_path = std::env::temp_dir().join(format!(
        "trispr_tts_bench_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    let result = synthesize_piper_to_wav(text, binary_path, model_path, rate, &temp_path);
    let _ = std::fs::remove_file(&temp_path);
    result
}

/// Read a WAV file and play it synchronously via cpal.
///
/// WASAPI shared mode performs internal SRC so no manual resampling is needed
/// for common Piper output rates (16 000 / 22 050 Hz).
fn resolve_playback_output_device(output_device_id: &str) -> Result<cpal::Device, String> {
    use cpal::traits::{DeviceTrait, HostTrait};

    let requested = {
        let trimmed = output_device_id.trim();
        if trimmed.is_empty() {
            "default"
        } else {
            trimmed
        }
    };
    let host = cpal::default_host();
    if requested == "default" {
        return host
            .default_output_device()
            .ok_or_else(|| "No audio output device found".to_string());
    }

    #[cfg(target_os = "windows")]
    let preferred_name = requested
        .strip_prefix("wasapi:")
        .and_then(|wasapi_id| {
            wasapi::DeviceEnumerator::new()
                .ok()?
                .get_device(wasapi_id)
                .ok()?
                .get_friendlyname()
                .ok()
        })
        .or_else(|| {
            requested
                .strip_prefix("output-")
                .and_then(|rest| rest.find('-').map(|pos| rest[pos + 1..].to_string()))
        });

    #[cfg(not(target_os = "windows"))]
    let preferred_name = requested
        .strip_prefix("output-")
        .and_then(|rest| rest.find('-').map(|pos| rest[pos + 1..].to_string()));

    let mut name_match: Option<cpal::Device> = None;
    if let Ok(outputs) = host.output_devices() {
        for (index, device) in outputs.enumerate() {
            let name = device
                .name()
                .unwrap_or_else(|_| format!("Output {}", index + 1));
            let generated_id = format!("output-{}-{}", index, name);
            if generated_id == requested {
                return Ok(device);
            }
            if name_match.is_none()
                && preferred_name
                    .as_deref()
                    .map(|preferred| name.eq_ignore_ascii_case(preferred))
                    .unwrap_or(false)
            {
                name_match = Some(device);
            }
        }
    }

    if let Some(device) = name_match {
        tracing::warn!(
            "TTS output device '{}' not matched by exact ID; matched by device name.",
            requested
        );
        return Ok(device);
    }

    Err(format!(
        "[tts_output_device_unavailable] Requested TTS output device '{}' is not available. Re-select a valid output device in Voice Output settings.",
        requested
    ))
}

#[derive(Debug, Clone)]
struct OutputStreamCandidate {
    stream_config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    source: &'static str,
}

fn sample_format_label(sample_format: cpal::SampleFormat) -> &'static str {
    match sample_format {
        cpal::SampleFormat::F32 => "f32",
        cpal::SampleFormat::I16 => "i16",
        cpal::SampleFormat::U16 => "u16",
        _ => "unknown",
    }
}

fn sample_format_rank(sample_format: cpal::SampleFormat) -> u8 {
    match sample_format {
        cpal::SampleFormat::F32 => 0,
        cpal::SampleFormat::I16 => 1,
        cpal::SampleFormat::U16 => 2,
        _ => 3,
    }
}

fn append_stream_candidate(
    candidates: &mut Vec<OutputStreamCandidate>,
    dedupe: &mut HashSet<String>,
    stream_config: cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    source: &'static str,
) {
    let key = format!(
        "{}:{}:{:?}",
        stream_config.channels, stream_config.sample_rate.0, sample_format
    );
    if dedupe.insert(key) {
        candidates.push(OutputStreamCandidate {
            stream_config,
            sample_format,
            source,
        });
    }
}

fn collect_output_stream_candidates(
    device: &cpal::Device,
    wav_spec: &hound::WavSpec,
) -> Result<Vec<OutputStreamCandidate>, String> {
    use cpal::traits::DeviceTrait;

    let mut candidates = Vec::<OutputStreamCandidate>::new();
    let mut dedupe = HashSet::<String>::new();

    if let Ok(default_config) = device.default_output_config() {
        append_stream_candidate(
            &mut candidates,
            &mut dedupe,
            default_config.config(),
            default_config.sample_format(),
            "default_output_config",
        );
    }

    if let Ok(ranges) = device.supported_output_configs() {
        for range in ranges {
            let min_rate = range.min_sample_rate().0;
            let max_rate = range.max_sample_rate().0;
            let target_rate = wav_spec.sample_rate.clamp(min_rate, max_rate);
            let supported = range.with_sample_rate(cpal::SampleRate(target_rate));
            append_stream_candidate(
                &mut candidates,
                &mut dedupe,
                supported.config(),
                supported.sample_format(),
                "supported_output_configs",
            );
        }
    }

    if candidates.is_empty() {
        return Err(
            "No supported audio output stream configuration was found for the selected device."
                .to_string(),
        );
    }

    if candidates.len() > 1 {
        let preferred_rate = wav_spec.sample_rate;
        let preferred_channels = wav_spec.channels;
        candidates[1..].sort_by_key(|candidate| {
            let rate_delta = candidate
                .stream_config
                .sample_rate
                .0
                .abs_diff(preferred_rate);
            let channel_delta = candidate
                .stream_config
                .channels
                .abs_diff(preferred_channels);
            (
                rate_delta,
                channel_delta,
                sample_format_rank(candidate.sample_format),
            )
        });
    }

    Ok(candidates)
}

fn decode_wav_to_f32(
    reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
    spec: hound::WavSpec,
) -> Result<Vec<f32>, String> {
    match spec.sample_format {
        hound::SampleFormat::Float => {
            if spec.bits_per_sample != 32 {
                return Err(format!(
                    "Unsupported float WAV bit depth: {} (expected 32).",
                    spec.bits_per_sample
                ));
            }
            reader
                .into_samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("WAV decode error: {e}"))
        }
        hound::SampleFormat::Int => {
            let bits = u32::from(spec.bits_per_sample.clamp(1, 32));
            let scale = if bits <= 1 {
                1.0
            } else if bits >= 32 {
                i32::MAX as f32
            } else {
                ((1_i64 << (bits - 1)) - 1) as f32
            };
            let decoded = reader
                .into_samples::<i32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("WAV decode error: {e}"))?;
            Ok(decoded
                .into_iter()
                .map(|sample| (sample as f32 / scale).clamp(-1.0, 1.0))
                .collect::<Vec<_>>())
        }
    }
}

fn remap_channels_interleaved(input: &[f32], src_channels: usize, dst_channels: usize) -> Vec<f32> {
    if src_channels == 0 || dst_channels == 0 || input.is_empty() {
        return Vec::new();
    }
    if src_channels == dst_channels {
        return input.to_vec();
    }

    let frame_count = input.len() / src_channels;
    let mut output = vec![0.0; frame_count * dst_channels];
    for frame in 0..frame_count {
        let src_base = frame * src_channels;
        let dst_base = frame * dst_channels;
        if src_channels == 1 {
            let value = input[src_base];
            for channel in 0..dst_channels {
                output[dst_base + channel] = value;
            }
        } else if dst_channels == 1 {
            let mut sum = 0.0;
            for channel in 0..src_channels {
                sum += input[src_base + channel];
            }
            output[dst_base] = sum / src_channels as f32;
        } else {
            for channel in 0..dst_channels {
                let src_channel = channel.min(src_channels - 1);
                output[dst_base + channel] = input[src_base + src_channel];
            }
        }
    }
    output
}

fn resample_interleaved_linear(
    input: &[f32],
    channels: usize,
    src_rate: u32,
    dst_rate: u32,
) -> Vec<f32> {
    if channels == 0 || input.is_empty() {
        return Vec::new();
    }
    if src_rate == dst_rate {
        return input.to_vec();
    }

    let src_frames = input.len() / channels;
    if src_frames == 0 {
        return Vec::new();
    }
    if src_frames == 1 {
        return input.to_vec();
    }

    let dst_frames = (((src_frames as u128 * dst_rate as u128) + (src_rate as u128 / 2))
        / src_rate as u128) as usize;
    let dst_frames = dst_frames.max(1);

    if dst_frames == src_frames {
        return input.to_vec();
    }

    let mut output = vec![0.0; dst_frames * channels];
    for dst_frame in 0..dst_frames {
        let src_pos = if dst_frames == 1 {
            0.0
        } else {
            dst_frame as f32 * (src_frames - 1) as f32 / (dst_frames - 1) as f32
        };
        let src_idx0 = src_pos.floor() as usize;
        let src_idx1 = (src_idx0 + 1).min(src_frames - 1);
        let frac = src_pos - src_idx0 as f32;

        for channel in 0..channels {
            let left = input[src_idx0 * channels + channel];
            let right = input[src_idx1 * channels + channel];
            output[dst_frame * channels + channel] = left + (right - left) * frac;
        }
    }
    output
}

fn convert_f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            if clamped <= -1.0 {
                i16::MIN
            } else {
                (clamped * i16::MAX as f32).round() as i16
            }
        })
        .collect()
}

fn convert_f32_to_u16(samples: &[f32]) -> Vec<u16> {
    samples
        .iter()
        .map(|sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            ((((clamped + 1.0) * 0.5) * u16::MAX as f32).round() as i32).clamp(0, u16::MAX as i32)
                as u16
        })
        .collect()
}

fn wav_spec_label(spec: &hound::WavSpec) -> String {
    let sample_kind = match spec.sample_format {
        hound::SampleFormat::Float => "float",
        hound::SampleFormat::Int => "int",
    };
    format!(
        "{}Hz/{}ch/{}{}",
        spec.sample_rate, spec.channels, sample_kind, spec.bits_per_sample
    )
}

fn format_stream_config_mismatch_error(
    requested_device_id: &str,
    source_spec: &hound::WavSpec,
    candidate: &OutputStreamCandidate,
    reason: &str,
) -> String {
    format!(
        "[tts_output_stream_config_unsupported] device='{}' wav={} -> target={}Hz/{}ch/{} ({}) reason={}",
        requested_device_id,
        wav_spec_label(source_spec),
        candidate.stream_config.sample_rate.0,
        candidate.stream_config.channels,
        sample_format_label(candidate.sample_format),
        candidate.source,
        reason
    )
}

fn play_interleaved_samples<T: cpal::SizedSample + Copy + Send + Sync + 'static>(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    samples: Vec<T>,
    silence: T,
    stream_label: &'static str,
) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, StreamTrait};

    if samples.is_empty() {
        return Ok(());
    }

    let total_samples = samples.len();
    let channels = usize::from(stream_config.channels.max(1));
    let sample_rate = stream_config.sample_rate.0.max(1);

    let samples = Arc::new(samples);
    let position = Arc::new(AtomicUsize::new(0));
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(1);

    let callback_samples = Arc::clone(&samples);
    let callback_pos = Arc::clone(&position);
    let mut notified = false;

    let stream = device
        .build_output_stream(
            stream_config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for out in data.iter_mut() {
                    let sample_index = callback_pos.fetch_add(1, Ordering::Relaxed);
                    *out = if sample_index < total_samples {
                        callback_samples[sample_index]
                    } else {
                        silence
                    };
                }
                if !notified && callback_pos.load(Ordering::Relaxed) >= total_samples {
                    notified = true;
                    let _ = done_tx.try_send(());
                }
            },
            move |error| tracing::error!("TTS playback stream error ({}): {}", stream_label, error),
            None,
        )
        .map_err(|error| error.to_string())?;

    stream.play().map_err(|error| error.to_string())?;

    let frame_count = total_samples / channels;
    let timeout = std::time::Duration::from_secs_f64(
        ((frame_count as f64 / sample_rate as f64).max(3.0)) + 2.0,
    );
    let _ = done_rx.recv_timeout(timeout);
    drop(stream);
    Ok(())
}

fn play_wav_blocking(
    path: &std::path::Path,
    volume: f32,
    output_device_id: &str,
) -> Result<(), String> {
    let reader = hound::WavReader::open(path).map_err(|e| format!("Cannot read WAV: {e}"))?;
    let spec = reader.spec();
    let decoded_samples = decode_wav_to_f32(reader, spec)?;
    if decoded_samples.is_empty() {
        return Ok(());
    }

    let device = resolve_playback_output_device(output_device_id)?;
    let candidates = collect_output_stream_candidates(&device, &spec)?;
    let requested = {
        let trimmed = output_device_id.trim();
        if trimmed.is_empty() {
            "default"
        } else {
            trimmed
        }
    };
    let mut attempt_errors = Vec::<String>::new();

    for candidate in &candidates {
        let remapped = remap_channels_interleaved(
            &decoded_samples,
            usize::from(spec.channels.max(1)),
            usize::from(candidate.stream_config.channels.max(1)),
        );
        let mut prepared = resample_interleaved_linear(
            &remapped,
            usize::from(candidate.stream_config.channels.max(1)),
            spec.sample_rate.max(1),
            candidate.stream_config.sample_rate.0.max(1),
        );
        let vol = volume.clamp(0.0, 1.0);
        if (vol - 1.0).abs() > f32::EPSILON {
            for sample in &mut prepared {
                *sample = (*sample * vol).clamp(-1.0, 1.0);
            }
        }

        let result = match candidate.sample_format {
            cpal::SampleFormat::F32 => play_interleaved_samples(
                &device,
                &candidate.stream_config,
                prepared,
                0.0_f32,
                "f32",
            ),
            cpal::SampleFormat::I16 => play_interleaved_samples(
                &device,
                &candidate.stream_config,
                convert_f32_to_i16(&prepared),
                0_i16,
                "i16",
            ),
            cpal::SampleFormat::U16 => play_interleaved_samples(
                &device,
                &candidate.stream_config,
                convert_f32_to_u16(&prepared),
                u16::MAX / 2,
                "u16",
            ),
            unsupported => Err(format!(
                "Unsupported output sample format '{}'.",
                sample_format_label(unsupported)
            )),
        };

        match result {
            Ok(()) => return Ok(()),
            Err(reason) => {
                let diagnostic =
                    format_stream_config_mismatch_error(requested, &spec, candidate, &reason);
                let reason_lower = reason.to_ascii_lowercase();
                if reason_lower.contains("stream configuration is not supported")
                    || reason_lower.contains("streamconfignotsupported")
                {
                    attempt_errors.push(diagnostic);
                    continue;
                }
                return Err(diagnostic);
            }
        }
    }

    if attempt_errors.is_empty() {
        return Err("[tts_output_stream_config_unsupported] Unable to open a compatible output stream for the selected device.".to_string());
    }
    if attempt_errors.len() == 1 {
        return Err(attempt_errors.remove(0));
    }
    Err(format!(
        "[tts_output_stream_config_unsupported] All candidate stream configs failed: {}",
        attempt_errors.join(" | ")
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        convert_f32_to_i16, convert_f32_to_u16, execute_tts_with_fallback,
        format_stream_config_mismatch_error, is_tts_audio_device_unavailable_tagged,
        is_tts_policy_allowed, remap_channels_interleaved, resample_interleaved_linear,
        select_voice_from_candidates_for_language, windows_audio_device_error_hint,
        windows_natural_voice_priority, windows_voice_matches_natural_profile,
        OutputStreamCandidate, TtsVoiceInfo, VisionFrame, VisionFrameBuffer,
    };

    fn frame(seq: u64, bytes: usize) -> VisionFrame {
        VisionFrame {
            seq,
            timestamp_ms: seq * 100,
            source_scope: "all_monitors".to_string(),
            source_count: 1,
            width: 640,
            height: 360,
            jpeg_bytes: vec![7; bytes],
        }
    }

    #[test]
    fn vision_buffer_evicts_oldest_frames() {
        let mut buffer = VisionFrameBuffer::default();

        let meta_a = buffer.push(frame(1, 10), 2);
        let meta_b = buffer.push(frame(2, 20), 2);
        let meta_c = buffer.push(frame(3, 30), 2);

        assert_eq!(meta_a.buffered_frames, 1);
        assert_eq!(meta_b.buffered_frames, 2);
        assert_eq!(meta_c.buffered_frames, 2);
        assert_eq!(meta_c.buffered_bytes, 50);
        assert_eq!(buffer.latest().map(|latest| latest.seq), Some(3));
        assert_eq!(buffer.stats().buffered_frames, 2);
        assert_eq!(buffer.stats().buffered_bytes, 50);
    }

    #[test]
    fn vision_buffer_clear_releases_stats() {
        let mut buffer = VisionFrameBuffer::default();
        buffer.push(frame(1, 42), 10);
        buffer.clear();

        let stats = buffer.stats();
        assert_eq!(stats.buffered_frames, 0);
        assert_eq!(stats.buffered_bytes, 0);
        assert!(stats.last_frame_timestamp_ms.is_none());
        assert!(buffer.latest().is_none());
    }

    #[test]
    fn tts_policy_agent_replies_only_allows_agent_reply_and_manual_test() {
        assert!(is_tts_policy_allowed("agent_replies_only", "agent_reply"));
        assert!(is_tts_policy_allowed("agent_replies_only", "manual_test"));
        assert!(!is_tts_policy_allowed("agent_replies_only", "agent_event"));
        assert!(!is_tts_policy_allowed("agent_replies_only", "manual_user"));
    }

    #[test]
    fn tts_policy_replies_and_events_allows_agent_event_lane() {
        assert!(is_tts_policy_allowed("replies_and_events", "agent_reply"));
        assert!(is_tts_policy_allowed("replies_and_events", "agent_event"));
        assert!(is_tts_policy_allowed("replies_and_events", "manual_test"));
        assert!(!is_tts_policy_allowed("replies_and_events", "manual_user"));
    }

    #[test]
    fn tts_policy_explicit_only_blocks_agent_contexts() {
        assert!(is_tts_policy_allowed("explicit_only", "manual_user"));
        assert!(is_tts_policy_allowed("explicit_only", "manual_test"));
        assert!(!is_tts_policy_allowed("explicit_only", "agent_reply"));
        assert!(!is_tts_policy_allowed("explicit_only", "agent_event"));
    }

    #[test]
    fn tts_fallback_matrix_uses_primary_when_available() {
        let outcome = execute_tts_with_fallback("windows_native", "local_custom", |provider| {
            if provider == "windows_native" {
                Ok(())
            } else {
                Err("unexpected fallback".to_string())
            }
        })
        .expect("primary provider should succeed");

        assert_eq!(outcome.provider_used, "windows_native");
        assert!(!outcome.used_fallback);
        assert!(outcome.primary_error.is_none());
    }

    #[test]
    fn tts_fallback_matrix_uses_fallback_when_primary_fails() {
        let outcome = execute_tts_with_fallback("windows_native", "local_custom", |provider| {
            if provider == "windows_native" {
                Err("powershell unavailable".to_string())
            } else {
                Ok(())
            }
        })
        .expect("fallback provider should succeed");

        assert_eq!(outcome.provider_used, "local_custom");
        assert!(outcome.used_fallback);
        assert_eq!(
            outcome.primary_error.as_deref(),
            Some("powershell unavailable")
        );
    }

    #[test]
    fn tts_fallback_matrix_reports_both_failures() {
        let error = execute_tts_with_fallback("windows_native", "local_custom", |provider| {
            if provider == "windows_native" {
                Err("primary failed".to_string())
            } else {
                Err("fallback failed".to_string())
            }
        })
        .expect_err("both providers should fail");

        assert!(error.contains("tts_fallback_both_failed"));
        assert!(error.contains("primary failed"));
        assert!(error.contains("fallback failed"));
    }

    #[test]
    fn tts_fallback_matrix_reports_missing_alternative_fallback() {
        let error = execute_tts_with_fallback("windows_native", "windows_native", |_provider| {
            Err("primary failed".to_string())
        })
        .expect_err("no alternative fallback configured");

        assert!(error.contains("tts_fallback_no_alternative"));
        assert!(error.contains("primary failed"));
    }

    #[test]
    fn tts_fallback_audio_device_error_short_circuits_windows_chain() {
        let error = execute_tts_with_fallback("windows_native", "windows_natural", |provider| {
            if provider == "windows_native" {
                Err("Windows TTS failed: Speak AudioException - Error Code: 0x2".to_string())
            } else {
                Err("fallback should not be attempted".to_string())
            }
        })
        .expect_err("audio device error should be surfaced directly");

        assert!(error.contains("tts_audio_device_unavailable"));
        assert!(!error.contains("fallback should not be attempted"));
    }

    #[test]
    fn tts_audio_device_hint_is_idempotent() {
        let first = windows_audio_device_error_hint(
            "Windows TTS failed: Speak AudioException - Error Code: 0x2",
        );
        let second = windows_audio_device_error_hint(&first);

        assert!(is_tts_audio_device_unavailable_tagged(&first));
        assert_eq!(first, second);
    }

    #[test]
    fn tts_audio_device_error_detector_matches_known_signatures() {
        assert!(super::is_windows_audio_device_error(
            "Windows TTS failed with status Some(1): AudioException Error Code: 0x2 at Speak"
        ));
        assert!(super::is_windows_audio_device_error(
            "Ausnahme ... Es wurde ein Audiogeraetefehler entdeckt ... Speak"
        ));
        assert!(!super::is_windows_audio_device_error(
            "No Windows Natural voice found."
        ));
    }

    #[test]
    fn channel_remap_and_resample_produce_expected_shape() {
        let mono = vec![0.0_f32, 0.5, 1.0, 0.5];
        let stereo = remap_channels_interleaved(&mono, 1, 2);
        assert_eq!(stereo.len(), mono.len() * 2);
        assert_eq!(stereo[0], 0.0);
        assert_eq!(stereo[1], 0.0);
        assert_eq!(stereo[2], 0.5);
        assert_eq!(stereo[3], 0.5);

        let resampled = resample_interleaved_linear(&stereo, 2, 22_050, 44_100);
        assert_eq!(resampled.len(), stereo.len() * 2);
    }

    #[test]
    fn sample_format_converters_clamp_values() {
        let input = vec![-2.0_f32, -1.0, 0.0, 1.0, 2.0];
        let as_i16 = convert_f32_to_i16(&input);
        assert_eq!(as_i16.first().copied(), Some(i16::MIN));
        assert_eq!(as_i16[2], 0);
        assert_eq!(as_i16.last().copied(), Some(i16::MAX));

        let as_u16 = convert_f32_to_u16(&input);
        assert_eq!(as_u16.first().copied(), Some(0));
        assert_eq!(as_u16[2], 32768);
        assert_eq!(as_u16.last().copied(), Some(u16::MAX));
    }

    #[test]
    fn stream_config_mismatch_error_uses_stable_reason_code_and_diag() {
        let candidate = OutputStreamCandidate {
            stream_config: cpal::StreamConfig {
                channels: 2,
                sample_rate: cpal::SampleRate(48_000),
                buffer_size: cpal::BufferSize::Default,
            },
            sample_format: cpal::SampleFormat::F32,
            source: "default_output_config",
        };
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22_050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let error = format_stream_config_mismatch_error(
            "wasapi:{device-id}",
            &spec,
            &candidate,
            "The requested stream configuration is not supported by the device.",
        );
        assert!(error.contains("[tts_output_stream_config_unsupported]"));
        assert!(error.contains("wav=22050Hz/1ch/int16"));
        assert!(error.contains("target=48000Hz/2ch/f32"));
    }

    #[test]
    fn natural_voice_profile_detects_multilingual_and_natural_markers() {
        assert!(windows_voice_matches_natural_profile(
            "Microsoft AvaMultilingual"
        ));
        assert!(windows_voice_matches_natural_profile(
            "Microsoft Aria (Natural)"
        ));
        assert!(windows_voice_matches_natural_profile("Edge Online Voice"));
        assert!(windows_voice_matches_natural_profile("Microsoft Aria"));
        assert!(windows_voice_matches_natural_profile("Microsoft Conrad"));
        assert!(!windows_voice_matches_natural_profile("Microsoft Zira"));
    }

    #[test]
    fn natural_voice_priority_prefers_multilingual_then_natural_then_online() {
        assert!(
            windows_natural_voice_priority("Microsoft AvaMultilingual")
                < windows_natural_voice_priority("Microsoft Aria (Natural)")
        );
        assert!(
            windows_natural_voice_priority("Microsoft Aria (Natural)")
                < windows_natural_voice_priority("Edge Online Voice")
        );
        assert!(
            windows_natural_voice_priority("Edge Online Voice")
                < windows_natural_voice_priority("Microsoft Aria")
        );
        assert_eq!(windows_natural_voice_priority("Microsoft Zira"), 4);
    }

    #[test]
    fn language_voice_selector_prefers_exact_locale_match() {
        let voices = vec![
            TtsVoiceInfo {
                id: "Microsoft Conrad".to_string(),
                label: "Microsoft Conrad".to_string(),
                provider: "windows_native".to_string(),
                locale: Some("de-DE".to_string()),
                profile: Some("natural".to_string()),
            },
            TtsVoiceInfo {
                id: "Microsoft Aria".to_string(),
                label: "Microsoft Aria".to_string(),
                provider: "windows_native".to_string(),
                locale: Some("en-US".to_string()),
                profile: Some("multilingual".to_string()),
            },
        ];
        let selected = select_voice_from_candidates_for_language(&voices, "de-DE");
        assert_eq!(selected.as_deref(), Some("Microsoft Conrad"));
    }

    #[test]
    fn language_voice_selector_returns_none_for_unmatched_language() {
        let voices = vec![TtsVoiceInfo {
            id: "Microsoft Aria".to_string(),
            label: "Microsoft Aria".to_string(),
            provider: "windows_native".to_string(),
            locale: Some("en-US".to_string()),
            profile: Some("multilingual".to_string()),
        }];
        let selected = select_voice_from_candidates_for_language(&voices, "ja-JP");
        assert!(selected.is_none());
    }
}
