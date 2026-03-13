use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
#[cfg(target_os = "windows")]
use tauri::{AppHandle, Manager};

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoiceInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSpeakRequest {
    pub provider: String,
    pub text: String,
    pub rate: Option<f32>,
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSpeakResult {
    pub provider_used: String,
    pub accepted: bool,
    pub message: String,
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
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
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

pub fn list_tts_providers() -> Vec<TtsProviderInfo> {
    vec![
        TtsProviderInfo {
            id: "windows_native".to_string(),
            label: "Windows Native TTS".to_string(),
            available: cfg!(target_os = "windows"),
        },
        TtsProviderInfo {
            id: "local_custom".to_string(),
            label: "Local Custom TTS (Piper)".to_string(),
            available: resolve_piper_binary("").is_some(),
        },
    ]
}

#[cfg(target_os = "windows")]
fn list_windows_voices() -> Vec<TtsVoiceInfo> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }",
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut voices = Vec::new();
            for line in text.lines() {
                let label = line.trim();
                if label.is_empty() {
                    continue;
                }
                voices.push(TtsVoiceInfo {
                    id: label.to_string(),
                    label: label.to_string(),
                    provider: "windows_native".to_string(),
                });
            }
            if voices.is_empty() {
                voices.push(TtsVoiceInfo {
                    id: "windows_default".to_string(),
                    label: "Windows Default Voice".to_string(),
                    provider: "windows_native".to_string(),
                });
            }
            voices
        }
        _ => vec![TtsVoiceInfo {
            id: "windows_default".to_string(),
            label: "Windows Default Voice".to_string(),
            provider: "windows_native".to_string(),
        }],
    }
}

#[cfg(not(target_os = "windows"))]
fn list_windows_voices() -> Vec<TtsVoiceInfo> {
    Vec::new()
}

pub fn list_tts_voices(provider: &str) -> Vec<TtsVoiceInfo> {
    match provider {
        "windows_native" => list_windows_voices(),
        _ => Vec::new(),
    }
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
            }
        })
        .collect();

    voices.sort_by(|a, b| a.label.cmp(&b.label));
    voices
}

#[cfg(target_os = "windows")]
pub fn speak_windows_native(text: &str, rate: f32, volume: f32) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("TTS text is empty.".to_string());
    }

    let rate = rate.clamp(0.5, 2.0);
    let volume = volume.clamp(0.0, 1.0);
    let sapi_rate = (((rate - 1.0) * 10.0).round() as i32).clamp(-10, 10);
    let sapi_volume = ((volume * 100.0).round() as i32).clamp(0, 100);
    let escaped_text = text.replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Rate = {sapi_rate}; $s.Volume = {sapi_volume}; $s.Speak('{escaped_text}')"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|error| format!("Failed to start Windows TTS: {}", error))?;
    if !output.status.success() {
        return Err(format!(
            "Windows TTS failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn speak_windows_native(_text: &str, _rate: f32, _volume: f32) -> Result<(), String> {
    Err("Windows native TTS is only available on Windows.".to_string())
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
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(p) = which::which("piper") {
        return Some(p);
    }
    if let Ok(p) = which::which("piper.exe") {
        return Some(p);
    }
    // Bundled with installer: <exe_dir>/resources/bin/piper/piper.exe
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled = exe_dir
                .join("resources")
                .join("bin")
                .join("piper")
                .join("piper.exe");
            if bundled.is_file() {
                return Some(bundled);
            }
        }
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let p = std::path::PathBuf::from(local_app_data)
            .join("trispr-flow")
            .join("piper")
            .join("piper.exe");
        if p.is_file() {
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

/// Synthesise `text` with Piper and play the result synchronously.
///
/// `rate` controls speech speed (0.5 = half speed, 2.0 = double speed).
/// Piper maps this to `--length_scale` (inverse: 1/rate).
/// `volume` scales WAV samples before playback (0.0..1.0).
pub fn speak_piper(
    text: &str,
    binary_path: &str,
    model_path: &str,
    rate: f32,
    volume: f32,
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
        voices.into_iter().next().map(|v| v.id).ok_or_else(|| {
            "No Piper voice model found. Run scripts/setup-piper.ps1 or set piper_model_path."
                .to_string()
        })?
    };
    let model_path = resolved_model.as_str();

    // Unique temp file per call to avoid collisions when called concurrently.
    let temp_path = std::env::temp_dir().join(format!(
        "trispr_tts_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));

    // Piper's --length_scale is the inverse of speed rate.
    let length_scale = format!("{:.3}", (1.0_f32 / rate.clamp(0.25, 4.0)));

    let mut child = Command::new(&binary)
        .args([
            "--model",
            model_path,
            "--output_file",
            temp_path.to_str().unwrap_or(""),
            "--length_scale",
            &length_scale,
        ])
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
        let _ = std::fs::remove_file(&temp_path);
        return Err(format!("Piper exited with status {status}"));
    }

    if !temp_path.is_file() {
        return Err("Piper produced no output file.".to_string());
    }

    let play_result = play_wav_blocking(&temp_path, volume);
    let _ = std::fs::remove_file(&temp_path);
    play_result
}

/// Read a WAV file and play it synchronously via cpal.
///
/// WASAPI shared mode performs internal SRC so no manual resampling is needed
/// for common Piper output rates (16 000 / 22 050 Hz).
fn play_wav_blocking(path: &std::path::Path, volume: f32) -> Result<(), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let reader = hound::WavReader::open(path).map_err(|e| format!("Cannot read WAV: {e}"))?;
    let spec = reader.spec();

    let vol = volume.clamp(0.0, 1.0);
    let samples: Vec<f32> = reader
        .into_samples::<i16>()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("WAV decode error: {e}"))?
        .into_iter()
        .map(|s| (s as f32 / i16::MAX as f32) * vol)
        .collect();

    if samples.is_empty() {
        return Ok(());
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| "No audio output device found".to_string())?;

    let config = cpal::StreamConfig {
        channels: spec.channels,
        sample_rate: cpal::SampleRate(spec.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let total = samples.len();
    let samples = Arc::new(samples);
    let pos = Arc::new(AtomicUsize::new(0));
    let (done_tx, done_rx) = std::sync::mpsc::sync_channel::<()>(1);

    let samples_c = samples.clone();
    let pos_c = pos.clone();
    let mut notified = false;

    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                for out in data.iter_mut() {
                    let p = pos_c.fetch_add(1, Ordering::Relaxed);
                    *out = if p < total { samples_c[p] } else { 0.0 };
                }
                if !notified && pos_c.load(Ordering::Relaxed) >= total {
                    notified = true;
                    let _ = done_tx.try_send(());
                }
            },
            |err| tracing::error!("Piper cpal playback error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())?;

    stream.play().map_err(|e| e.to_string())?;

    // Timeout = audio duration + 2 s grace, minimum 5 s.
    let duration_secs = total as u64 / spec.sample_rate as u64 / spec.channels as u64;
    let timeout = std::time::Duration::from_secs(duration_secs.max(3) + 2);
    let _ = done_rx.recv_timeout(timeout);

    drop(stream);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{VisionFrame, VisionFrameBuffer};

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
}
