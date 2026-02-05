// Trispr Flow - core app runtime
#![allow(clippy::needless_return)]

mod errors;
mod hotkeys;
mod overlay;

use arboard::Clipboard;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use enigo::{Enigo, Key, KeyboardControllable};
use errors::{AppError, ErrorEvent};
use overlay::{OverlayState, update_overlay_state};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Emitter, Listener, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use url::Url;

const TARGET_SAMPLE_RATE: u32 = 16_000;
const MIN_AUDIO_MS: u64 = 250;
const VAD_THRESHOLD_START_DEFAULT: f32 = 0.02;
const VAD_THRESHOLD_SUSTAIN_DEFAULT: f32 = 0.01;
const VAD_SILENCE_MS_DEFAULT: u64 = 700;
const VAD_MIN_VOICE_MS: u64 = 250;
const DEFAULT_MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const MAX_MODEL_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5 GB
const DOWNLOAD_TIMEOUT_SECS: u64 = 30; // Timeout for stalled downloads
const DOWNLOAD_CONNECT_TIMEOUT_SECS: u64 = 10;
const DOWNLOAD_READ_TIMEOUT_SECS: u64 = 30;
const DOWNLOAD_REDIRECT_LIMIT: u32 = 5;
const TRAY_CLICK_DEBOUNCE_MS: u64 = 250;

static LAST_TRAY_CLICK_MS: AtomicU64 = AtomicU64::new(0);

// Allowed domains for model downloads (security: SSRF prevention)
const ALLOWED_MODEL_DOMAINS: &[&str] = &[
  "huggingface.co",
  "ggml.ggerganov.com",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum UrlSafety {
  Basic,
  Strict,
}

fn is_allowed_host(host: &str) -> bool {
  ALLOWED_MODEL_DOMAINS
    .iter()
    .any(|allowed| host == *allowed || host.ends_with(&format!(".{}", allowed)))
}

fn is_public_ip(ip: IpAddr) -> bool {
  match ip {
    IpAddr::V4(v4) => {
      if v4.is_private()
        || v4.is_loopback()
        || v4.is_link_local()
        || v4.is_multicast()
        || v4.is_broadcast()
        || v4.is_documentation()
        || v4.is_unspecified()
      {
        return false;
      }
      let oct = v4.octets();
      // Carrier-grade NAT: 100.64.0.0/10
      if oct[0] == 100 && (oct[1] & 0b1100_0000) == 0b0100_0000 {
        return false;
      }
      true
    }
    IpAddr::V6(v6) => {
      if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
        return false;
      }
      let seg = v6.segments();
      // Unique local: fc00::/7
      if (seg[0] & 0xfe00) == 0xfc00 {
        return false;
      }
      // Link-local: fe80::/10
      if (seg[0] & 0xffc0) == 0xfe80 {
        return false;
      }
      // Documentation: 2001:db8::/32
      if seg[0] == 0x2001 && seg[1] == 0x0db8 {
        return false;
      }
      true
    }
  }
}

fn validate_model_url(url: &str, mode: UrlSafety) -> Result<Url, String> {
  let parsed = Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

  if parsed.scheme() != "https" {
    return Err("Only HTTPS URLs allowed (not HTTP)".to_string());
  }

  if !parsed.username().is_empty() || parsed.password().is_some() {
    return Err("URL userinfo is not allowed".to_string());
  }

  let host = parsed
    .host_str()
    .ok_or_else(|| "URL missing host".to_string())?
    .to_lowercase();

  if host == "localhost" || host.ends_with(".localhost") {
    return Err("Localhost URLs not allowed for security reasons".to_string());
  }

  if let Some(port) = parsed.port() {
    if port != 443 {
      return Err(format!("Only HTTPS port 443 is allowed (got {port})"));
    }
  }

  if !is_allowed_host(&host) {
    return Err(format!(
      "Domain '{}' not in whitelist. Allowed: {}",
      host,
      ALLOWED_MODEL_DOMAINS.join(", ")
    ));
  }

  if let Ok(ip) = host.parse::<IpAddr>() {
    if !is_public_ip(ip) {
      return Err("IP address is not public".to_string());
    }
  } else if mode == UrlSafety::Strict {
    let port = parsed.port_or_known_default().unwrap_or(443);
    let mut resolved = false;
    let addrs = (host.as_str(), port)
      .to_socket_addrs()
      .map_err(|e| format!("DNS lookup failed for {host}: {e}"))?;
    for addr in addrs {
      resolved = true;
      if !is_public_ip(addr.ip()) {
        return Err(format!(
          "Resolved IP {} for {} is not public",
          addr.ip(),
          host
        ));
      }
    }
    if !resolved {
      return Err(format!("DNS lookup returned no results for {host}"));
    }
  }

  Ok(parsed)
}

fn is_url_safe(url: &str, mode: UrlSafety) -> Result<(), String> {
  validate_model_url(url, mode).map(|_| ())
}

fn validate_model_file_name(file_name: &str) -> Result<(), String> {
  if file_name.trim().is_empty() {
    return Err("Missing model file name".to_string());
  }
  if file_name.contains('/') || file_name.contains('\\') || file_name.contains("..") {
    return Err("Invalid model file name".to_string());
  }
  let lower = file_name.to_ascii_lowercase();
  if !(lower.ends_with(".bin") || lower.ends_with(".gguf")) {
    return Err("Only .bin or .gguf model files are allowed".to_string());
  }
  if !file_name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
    return Err("Model file name contains invalid characters".to_string());
  }
  Ok(())
}

fn resolve_model_base_url() -> String {
  if let Ok(custom) = std::env::var("TRISPR_WHISPER_MODEL_BASE_URL") {
    match validate_model_url(&custom, UrlSafety::Basic) {
      Ok(_) => return custom,
      Err(err) => warn!("Ignoring unsafe TRISPR_WHISPER_MODEL_BASE_URL: {}", err),
    }
  }
  DEFAULT_MODEL_BASE_URL.to_string()
}

fn build_download_agent() -> ureq::Agent {
  ureq::builder()
    .timeout_connect(Duration::from_secs(DOWNLOAD_CONNECT_TIMEOUT_SECS))
    .timeout_read(Duration::from_secs(DOWNLOAD_READ_TIMEOUT_SECS))
    .timeout_write(Duration::from_secs(DOWNLOAD_READ_TIMEOUT_SECS))
    .redirects(0)
    .build()
}

fn http_get_with_redirects(url: &str) -> Result<ureq::Response, String> {
  let agent = build_download_agent();
  let mut current = url.to_string();

  for _ in 0..=DOWNLOAD_REDIRECT_LIMIT {
    let parsed = validate_model_url(&current, UrlSafety::Strict)?;
    let response = match agent.get(parsed.as_str()).call() {
      Ok(resp) => resp,
      Err(ureq::Error::Status(code, resp)) => {
        if (300..400).contains(&code) {
          resp
        } else {
          return Err(format!("HTTP {code} for {}", parsed.as_str()));
        }
      }
      Err(err) => return Err(err.to_string()),
    };

    let status = response.status();
    if (300..400).contains(&status) {
      let location = response.header("Location").ok_or_else(|| {
        format!("Redirect without Location header from {}", parsed.as_str())
      })?;
      let next = parsed
        .join(location)
        .map_err(|e| format!("Invalid redirect URL: {e}"))?;
      current = next.to_string();
      continue;
    }

    return Ok(response);
  }

  Err(format!(
    "Too many redirects (>{}) while downloading model",
    DOWNLOAD_REDIRECT_LIMIT
  ))
}

struct ModelSpec {
  id: &'static str,
  label: &'static str,
  file_name: &'static str,
  size_mb: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelIndex {
  #[serde(default)]
  base_url: Option<String>,
  #[serde(default)]
  models: Vec<ModelIndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelIndexEntry {
  id: String,
  #[serde(default)]
  label: String,
  #[serde(alias = "file", alias = "file_name")]
  file_name: String,
  #[serde(default)]
  size_mb: u32,
  #[serde(default)]
  url: Option<String>,
}

const MODEL_SPECS: &[ModelSpec] = &[
  ModelSpec {
    id: "whisper-large-v3",
    label: "Whisper large-v3",
    file_name: "ggml-large-v3.bin",
    size_mb: 2900,
  },
  ModelSpec {
    id: "whisper-large-v3-turbo",
    label: "Whisper large-v3-turbo",
    file_name: "ggml-large-v3-turbo.bin",
    size_mb: 1500,
  },
];

// Model integrity verification (SHA256 checksums)
// These checksums ensure downloaded models haven't been tampered with
// Format: (file_name, sha256_hex)
// NOTE: Update these after verifying actual model checksums
const MODEL_CHECKSUMS: &[(&str, &str)] = &[
  ("ggml-large-v3.bin", "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2"),
  ("ggml-large-v3-turbo.bin", "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69"),
  ("ggml-distil-large-v3.bin", "2883a11b90fb10ed592d826edeaee7d2929bf1ab985109fe9e1e7b4d2b69a298"),
];

/// Verifies a downloaded model file against its expected SHA256 checksum
fn verify_model_checksum(path: &std::path::Path, expected_hash: &str) -> Result<(), String> {
  use std::io::Read;

  let mut file = fs::File::open(path).map_err(|e| {
    format!("Failed to open model file for checksum verification: {}", e)
  })?;

  let mut hasher = Sha256::new();
  let mut buffer = [0u8; 8192];

  loop {
    let n = file.read(&mut buffer).map_err(|e| {
      format!("Failed to read model file for checksum: {}", e)
    })?;
    if n == 0 {
      break;
    }
    hasher.update(&buffer[..n]);
  }

  let result = hasher.finalize();
  let actual_hash = hex::encode(result);

  if actual_hash.eq_ignore_ascii_case(expected_hash) {
    info!("Model checksum verified: {}", path.display());
    Ok(())
  } else {
    error!(
      "Model checksum mismatch for {}: expected {}, got {}",
      path.display(),
      expected_hash,
      actual_hash
    );
    Err(format!(
      "Model integrity check failed: checksum mismatch (possible corruption or tampering)"
    ))
  }
}

fn lookup_model_checksum(file_name: &str) -> Option<&'static str> {
  MODEL_CHECKSUMS
    .iter()
    .find(|(name, _)| name.eq_ignore_ascii_case(file_name))
    .map(|(_, hash)| *hash)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
  mode: String,
  hotkey_ptt: String,
  hotkey_toggle: String,
  input_device: String,
  language_mode: String,
  model: String,
  cloud_fallback: bool,
  audio_cues: bool,
  audio_cues_volume: f32,
  ptt_use_vad: bool,  // Enable VAD threshold check even in PTT mode
  vad_threshold: f32,  // Legacy: now maps to vad_threshold_start
  vad_threshold_start: f32,
  vad_threshold_sustain: f32,
  vad_silence_ms: u64,
  transcribe_enabled: bool,
  transcribe_hotkey: String,
  transcribe_output_device: String,
  transcribe_vad_mode: bool,
  transcribe_vad_threshold: f32,
  transcribe_vad_silence_ms: u64,
  transcribe_batch_interval_ms: u64,
  transcribe_chunk_overlap_ms: u64,
  transcribe_input_gain_db: f32,
  mic_input_gain_db: f32,
  capture_enabled: bool,
  model_source: String,
  model_custom_url: String,
  model_storage_dir: String,
  overlay_color: String,
  overlay_min_radius: f32,
  overlay_max_radius: f32,
  overlay_rise_ms: u64,
  overlay_fall_ms: u64,
  overlay_opacity_inactive: f32,
  overlay_opacity_active: f32,
  overlay_kitt_color: String,
  overlay_kitt_rise_ms: u64,
  overlay_kitt_fall_ms: u64,
  overlay_kitt_opacity_inactive: f32,
  overlay_kitt_opacity_active: f32,
  overlay_pos_x: f64,
  overlay_pos_y: f64,
  overlay_kitt_pos_x: f64,
  overlay_kitt_pos_y: f64,
  overlay_style: String,        // "dot" | "kitt"
  overlay_kitt_min_width: f32,
  overlay_kitt_max_width: f32,
  overlay_kitt_height: f32,
  hallucination_filter_enabled: bool,
  hallucination_rms_threshold: f32,
  hallucination_max_duration_ms: u64,
  hallucination_max_words: u32,
  hallucination_max_chars: u32,
}

impl Default for Settings {
  fn default() -> Self {
    Self {
      mode: "ptt".to_string(),
      hotkey_ptt: "CommandOrControl+Shift+Space".to_string(),
      hotkey_toggle: "CommandOrControl+Shift+M".to_string(),
      input_device: "default".to_string(),
      language_mode: "auto".to_string(),
      model: "whisper-large-v3".to_string(),
      cloud_fallback: false,
      audio_cues: true,
      audio_cues_volume: 0.3,
      ptt_use_vad: false,  // Disabled by default
      vad_threshold: VAD_THRESHOLD_START_DEFAULT,  // Legacy field
      vad_threshold_start: VAD_THRESHOLD_START_DEFAULT,
      vad_threshold_sustain: VAD_THRESHOLD_SUSTAIN_DEFAULT,
      vad_silence_ms: VAD_SILENCE_MS_DEFAULT,
      transcribe_enabled: true,
      transcribe_hotkey: "CommandOrControl+Shift+O".to_string(),
      transcribe_output_device: "default".to_string(),
      transcribe_vad_mode: false,
      transcribe_vad_threshold: 0.04,
      transcribe_vad_silence_ms: 900,
      transcribe_batch_interval_ms: 8000,
      transcribe_chunk_overlap_ms: 1000,
      transcribe_input_gain_db: 0.0,
      mic_input_gain_db: 0.0,
      capture_enabled: true,
      model_source: "default".to_string(),
      model_custom_url: "".to_string(),
      model_storage_dir: "".to_string(),
      overlay_color: "#ff3d2e".to_string(),
      overlay_min_radius: 8.0,
      overlay_max_radius: 24.0,
      overlay_rise_ms: 80,
      overlay_fall_ms: 160,
      overlay_opacity_inactive: 0.2,
      overlay_opacity_active: 0.8,
      overlay_kitt_color: "#ff3d2e".to_string(),
      overlay_kitt_rise_ms: 80,
      overlay_kitt_fall_ms: 160,
      overlay_kitt_opacity_inactive: 0.2,
      overlay_kitt_opacity_active: 0.8,
      overlay_pos_x: 12.0,
      overlay_pos_y: 12.0,
      overlay_kitt_pos_x: 12.0,
      overlay_kitt_pos_y: 12.0,
      overlay_style: "dot".to_string(),
      overlay_kitt_min_width: 20.0,
      overlay_kitt_max_width: 200.0,
      overlay_kitt_height: 20.0,
      hallucination_filter_enabled: true,
      hallucination_rms_threshold: HALLUCINATION_RMS_THRESHOLD,
      hallucination_max_duration_ms: HALLUCINATION_MAX_DURATION_MS,
      hallucination_max_words: HALLUCINATION_MAX_WORDS as u32,
      hallucination_max_chars: HALLUCINATION_MAX_CHARS as u32,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
  id: String,
  text: String,
  timestamp_ms: u64,
  source: String,
}

#[derive(Debug, Clone, Serialize)]
struct AudioDevice {
  id: String,
  label: String,
}

#[derive(Debug, Clone, Serialize)]
struct ModelInfo {
  id: String,
  label: String,
  file_name: String,
  size_mb: u32,
  installed: bool,
  downloading: bool,
  path: Option<String>,
  source: String,
  available: bool,
  download_url: Option<String>,
  removable: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadProgress {
  id: String,
  downloaded: u64,
  total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadComplete {
  id: String,
  path: String,
}

#[derive(Debug, Clone, Serialize)]
struct DownloadError {
  id: String,
  error: String,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionResult {
  text: String,
  source: String,
}

#[derive(Default)]
struct CaptureBuffer {
  samples: Vec<i16>,
  resample_pos: f64,
}

impl CaptureBuffer {
  fn reset(&mut self) {
    self.samples.clear();
    self.resample_pos = 0.0;
  }

  fn len(&self) -> usize {
    self.samples.len()
  }

  fn drain(&mut self) -> Vec<i16> {
    let mut out = Vec::new();
    std::mem::swap(&mut out, &mut self.samples);
    self.resample_pos = 0.0;
    out
  }

  fn take_chunk(&mut self, chunk_samples: usize, overlap_samples: usize) -> Vec<i16> {
    if chunk_samples == 0 || self.samples.len() < chunk_samples {
      return Vec::new();
    }

    let chunk = self.samples[..chunk_samples].to_vec();
    let mut remaining = self.samples[chunk_samples..].to_vec();

    if overlap_samples > 0 && overlap_samples < chunk_samples {
      let overlap_start = chunk_samples.saturating_sub(overlap_samples);
      let mut new_samples = chunk[overlap_start..].to_vec();
      new_samples.append(&mut remaining);
      self.samples = new_samples;
    } else {
      self.samples = remaining;
    }

    chunk
  }

  fn push_samples(&mut self, input: &[f32], in_rate: u32) {
    if input.is_empty() {
      return;
    }

    if in_rate == TARGET_SAMPLE_RATE {
      for &sample in input {
        self.samples.push(float_to_i16(sample));
      }
      return;
    }

    let ratio = in_rate as f64 / TARGET_SAMPLE_RATE as f64;
    let mut pos = self.resample_pos;

    while pos + 1.0 < input.len() as f64 {
      let idx = pos.floor() as usize;
      let frac = pos - idx as f64;
      let a = input[idx] as f64;
      let b = input[idx + 1] as f64;
      let sample = (a * (1.0 - frac) + b * frac) as f32;
      self.samples.push(float_to_i16(sample));
      pos += ratio;
    }

    self.resample_pos = pos - input.len() as f64;
  }
}

struct Recorder {
  buffer: Arc<Mutex<CaptureBuffer>>,
  active: bool,
  transcribing: bool,
  stop_tx: Option<std::sync::mpsc::Sender<()>>,
  join_handle: Option<thread::JoinHandle<()>>,
  vad_tx: Option<std::sync::mpsc::Sender<VadEvent>>,
  vad_runtime: Option<Arc<VadRuntime>>,
  input_gain_db: Arc<AtomicI64>,
}

impl Recorder {
  fn new() -> Self {
    Self {
      buffer: Arc::new(Mutex::new(CaptureBuffer::default())),
      active: false,
      transcribing: false,
      stop_tx: None,
      join_handle: None,
      vad_tx: None,
      vad_runtime: None,
      input_gain_db: Arc::new(AtomicI64::new(0)),
    }
  }
}

struct TranscribeRecorder {
  active: bool,
  stop_tx: Option<std::sync::mpsc::Sender<()>>,
  join_handle: Option<thread::JoinHandle<()>>,
}

impl TranscribeRecorder {
  fn new() -> Self {
    Self {
      active: false,
      stop_tx: None,
      join_handle: None,
    }
  }
}

struct AppState {
  settings: Mutex<Settings>,
  history: Mutex<Vec<HistoryEntry>>,
  history_transcribe: Mutex<Vec<HistoryEntry>>,
  recorder: Mutex<Recorder>,
  transcribe: Mutex<TranscribeRecorder>,
  downloads: Mutex<HashSet<String>>,
  transcribe_active: AtomicBool,
}

fn resolve_config_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = app
    .path()
    .app_config_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

fn resolve_data_path(app: &AppHandle, filename: &str) -> PathBuf {
  let base = app
    .path()
    .app_data_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let _ = fs::create_dir_all(&base);
  base.join(filename)
}

fn load_settings(app: &AppHandle) -> Settings {
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
      if !(0.001..=0.5).contains(&settings.vad_threshold_start) {
        settings.vad_threshold_start = VAD_THRESHOLD_START_DEFAULT;
      }
      if !(0.001..=0.5).contains(&settings.vad_threshold_sustain) {
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
      settings.transcribe_input_gain_db = settings.transcribe_input_gain_db.clamp(-30.0, 30.0);
      settings.mic_input_gain_db = settings.mic_input_gain_db.clamp(-30.0, 30.0);
      #[cfg(target_os = "windows")]
      if settings.transcribe_output_device != "default"
        && !settings.transcribe_output_device.starts_with("wasapi:")
      {
        settings.transcribe_output_device = "default".to_string();
      }
      if settings.overlay_min_radius < 4.0 {
        settings.overlay_min_radius = 4.0;
      }
      if settings.overlay_max_radius < settings.overlay_min_radius {
        settings.overlay_max_radius = settings.overlay_min_radius + 4.0;
      }
      if settings.overlay_max_radius > 64.0 {
        settings.overlay_max_radius = 64.0;
      }
      if settings.overlay_rise_ms < 20 {
        settings.overlay_rise_ms = 20;
      }
      if settings.overlay_fall_ms < 20 {
        settings.overlay_fall_ms = 20;
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
      if approx_eq(settings.overlay_kitt_opacity_inactive, defaults.overlay_kitt_opacity_inactive)
        && !approx_eq(settings.overlay_opacity_inactive, defaults.overlay_opacity_inactive)
      {
        settings.overlay_kitt_opacity_inactive = settings.overlay_opacity_inactive;
      }
      if approx_eq(settings.overlay_kitt_opacity_active, defaults.overlay_kitt_opacity_active)
        && !approx_eq(settings.overlay_opacity_active, defaults.overlay_opacity_active)
      {
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
      if settings.overlay_kitt_min_width < 4.0 {
        settings.overlay_kitt_min_width = 4.0;
      }
      if settings.overlay_kitt_max_width < settings.overlay_kitt_min_width {
        settings.overlay_kitt_max_width = settings.overlay_kitt_min_width;
      }
      if settings.overlay_kitt_max_width > 800.0 {
        settings.overlay_kitt_max_width = 800.0;
      }
      if settings.overlay_kitt_height < 8.0 {
        settings.overlay_kitt_height = 8.0;
      }
      if settings.overlay_kitt_height > 40.0 {
        settings.overlay_kitt_height = 40.0;
      }
      if settings.overlay_kitt_rise_ms < 20 {
        settings.overlay_kitt_rise_ms = 20;
      }
      if settings.overlay_kitt_fall_ms < 20 {
        settings.overlay_kitt_fall_ms = 20;
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
      settings.capture_enabled = true;
      settings.transcribe_enabled = true;
      settings
    }
    Err(_) => Settings::default(),
  }
}

fn save_settings_file(app: &AppHandle, settings: &Settings) -> Result<(), String> {
  let path = resolve_config_path(app, "settings.json");
  let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

fn sync_model_dir_env(settings: &Settings) {
  let trimmed = settings.model_storage_dir.trim();
  if trimmed.is_empty() {
    std::env::remove_var("TRISPR_WHISPER_MODEL_DIR");
  } else {
    std::env::set_var("TRISPR_WHISPER_MODEL_DIR", trimmed);
  }
}

fn load_history(app: &AppHandle) -> Vec<HistoryEntry> {
  let path = resolve_data_path(app, "history.json");
  match fs::read_to_string(path) {
    Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
    Err(_) => Vec::new(),
  }
}

fn save_history_file(app: &AppHandle, history: &[HistoryEntry]) -> Result<(), String> {
  let path = resolve_data_path(app, "history.json");
  let raw = serde_json::to_string_pretty(history).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

fn load_transcribe_history(app: &AppHandle) -> Vec<HistoryEntry> {
  let path = resolve_data_path(app, "history_transcribe.json");
  match fs::read_to_string(path) {
    Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
    Err(_) => Vec::new(),
  }
}

fn save_transcribe_history_file(app: &AppHandle, history: &[HistoryEntry]) -> Result<(), String> {
  let path = resolve_data_path(app, "history_transcribe.json");
  let raw = serde_json::to_string_pretty(history).map_err(|e| e.to_string())?;
  fs::write(path, raw).map_err(|e| e.to_string())?;
  Ok(())
}

fn now_ms() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

fn model_spec(model_id: &str) -> Option<&'static ModelSpec> {
  MODEL_SPECS.iter().find(|spec| spec.id == model_id)
}

fn build_overlay_settings(settings: &Settings) -> overlay::OverlaySettings {
  let use_kitt = settings.overlay_style == "kitt";
  let (color, rise_ms, fall_ms, opacity_inactive, opacity_active, pos_x, pos_y) = if use_kitt {
    (
      settings.overlay_kitt_color.clone(),
      settings.overlay_kitt_rise_ms,
      settings.overlay_kitt_fall_ms,
      settings.overlay_kitt_opacity_inactive,
      settings.overlay_kitt_opacity_active,
      settings.overlay_kitt_pos_x,
      settings.overlay_kitt_pos_y,
    )
  } else {
    (
      settings.overlay_color.clone(),
      settings.overlay_rise_ms,
      settings.overlay_fall_ms,
      settings.overlay_opacity_inactive,
      settings.overlay_opacity_active,
      settings.overlay_pos_x,
      settings.overlay_pos_y,
    )
  };
  overlay::OverlaySettings {
    color,
    min_radius: settings.overlay_min_radius as f64,
    max_radius: settings.overlay_max_radius as f64,
    rise_ms,
    fall_ms,
    opacity_inactive: opacity_inactive as f64,
    opacity_active: opacity_active as f64,
    pos_x,
    pos_y,
    style: settings.overlay_style.clone(),
    kitt_min_width: settings.overlay_kitt_min_width as f64,
    kitt_max_width: settings.overlay_kitt_max_width as f64,
    kitt_height: settings.overlay_kitt_height as f64,
  }
}

fn resolve_models_dir(app: &AppHandle) -> PathBuf {
  if let Ok(dir) = std::env::var("TRISPR_WHISPER_MODEL_DIR") {
    let trimmed = dir.trim();
    if !trimmed.is_empty() {
      let path = PathBuf::from(trimmed);
      if fs::create_dir_all(&path).is_ok() {
        return path;
      }
    }
  }
  let base = app
    .path()
    .app_data_dir()
    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
  let dir = base.join("models");
  let _ = fs::create_dir_all(&dir);
  dir
}

fn model_candidates(spec: &ModelSpec) -> Vec<String> {
  let mut candidates = vec![spec.file_name.to_string()];
  if let Some(stripped) = spec.file_name.strip_suffix(".bin") {
    candidates.push(format!("{}-q5_0.bin", stripped));
  }
  candidates
}

fn find_model_in_dir(dir: &PathBuf, spec: &ModelSpec) -> Option<PathBuf> {
  for file in model_candidates(spec) {
    let candidate = dir.join(&file);
    if candidate.exists() {
      return Some(candidate);
    }
  }
  None
}

fn resolve_model_path(app: &AppHandle, model_id: &str) -> Option<PathBuf> {
  info!("Resolving model path for: {}", model_id);

  if let Ok(path) = std::env::var("TRISPR_WHISPER_MODEL") {
    let candidate = PathBuf::from(&path);
    info!("Checking TRISPR_WHISPER_MODEL env var: {}", path);
    if candidate.exists() {
      info!("Found model at: {}", candidate.display());
      return Some(candidate);
    }
  }

  let spec = model_spec(model_id);
  if spec.is_none() {
    let candidates = [
      model_id.to_string(),
      format!("{model_id}.bin"),
      format!("{model_id}.gguf"),
    ];
    for candidate in candidates {
      if let Some(path) = resolve_model_path_by_file(app, &candidate) {
        info!("Found model by file name: {}", path.display());
        return Some(path);
      }
    }
    warn!("Model not found for: {}", model_id);
    return None;
  }
  let spec = spec?;
  info!("Looking for model file: {}", spec.file_name);

  if let Ok(dir) = std::env::var("TRISPR_WHISPER_MODEL_DIR") {
    let dir = PathBuf::from(&dir);
    info!("Checking TRISPR_WHISPER_MODEL_DIR: {}", dir.display());
    if let Some(path) = find_model_in_dir(&dir, spec) {
      info!("Found model at: {}", path.display());
      return Some(path);
    }
  }

  let models_dir = resolve_models_dir(app);
  info!("Checking app data dir: {}", models_dir.display());
  if let Some(path) = find_model_in_dir(&models_dir, spec) {
    info!("Found model at: {}", path.display());
    return Some(path);
  }

  // Search relative to executable location (works for built .exe)
  if let Ok(exe_path) = std::env::current_exe() {
    if let Some(exe_dir) = exe_path.parent() {
      info!("Executable directory: {}", exe_dir.display());
      let exe_search_dirs = [
        exe_dir.join("models"),              // models/ next to .exe
        exe_dir.join("../models"),           // models/ one level up
        exe_dir.join("../whisper.cpp/models"), // whisper.cpp/models one level up
        exe_dir.join("../../whisper.cpp/models"), // whisper.cpp/models two levels up
      ];
      for dir in &exe_search_dirs {
        info!("Checking: {}", dir.display());
        if let Some(path) = find_model_in_dir(dir, spec) {
          info!("Found model at: {}", path.display());
          return Some(path);
        }
      }
    }
  }

  // Search relative to current working directory (works for dev)
  if let Ok(cwd) = std::env::current_dir() {
    info!("Current working directory: {}", cwd.display());
    let search_dirs = [
      cwd.join("models"),
      cwd.join("../whisper.cpp/models"),
      cwd.join("../../whisper.cpp/models"),
    ];
    for dir in &search_dirs {
      info!("Checking: {}", dir.display());
      if let Some(path) = find_model_in_dir(dir, spec) {
        info!("Found model at: {}", path.display());
        return Some(path);
      }
    }
  }

  warn!("Model not found for: {}", model_id);
  None
}

#[derive(Debug, Clone)]
struct SourceModel {
  id: String,
  label: String,
  file_name: String,
  size_mb: u32,
  download_url: String,
  source: String,
}

fn filename_from_url(url: &str) -> Option<String> {
  let trimmed = url.split('?').next().unwrap_or(url);
  trimmed.split('/').last().map(|name| name.to_string())
}

fn resolve_model_path_by_file(app: &AppHandle, file_name: &str) -> Option<PathBuf> {
  if let Ok(path) = std::env::var("TRISPR_WHISPER_MODEL") {
    let candidate = PathBuf::from(&path);
    if candidate.exists() {
      if let Some(name) = candidate.file_name().and_then(|s| s.to_str()) {
        if name.eq_ignore_ascii_case(file_name) {
          return Some(candidate);
        }
      }
    }
  }

  if let Ok(dir) = std::env::var("TRISPR_WHISPER_MODEL_DIR") {
    let dir = PathBuf::from(&dir);
    let candidate = dir.join(file_name);
    if candidate.exists() {
      return Some(candidate);
    }
  }

  let models_dir = resolve_models_dir(app);
  let candidate = models_dir.join(file_name);
  if candidate.exists() {
    return Some(candidate);
  }

  if let Ok(exe_path) = std::env::current_exe() {
    if let Some(exe_dir) = exe_path.parent() {
      let exe_search_dirs = [
        exe_dir.join("models"),
        exe_dir.join("../models"),
        exe_dir.join("../whisper.cpp/models"),
        exe_dir.join("../../whisper.cpp/models"),
      ];
      for dir in &exe_search_dirs {
        let candidate = dir.join(file_name);
        if candidate.exists() {
          return Some(candidate);
        }
      }
    }
  }

  if let Ok(cwd) = std::env::current_dir() {
    let search_dirs = [
      cwd.join("models"),
      cwd.join("../whisper.cpp/models"),
      cwd.join("../../whisper.cpp/models"),
    ];
    for dir in &search_dirs {
      let candidate = dir.join(file_name);
      if candidate.exists() {
        return Some(candidate);
      }
    }
  }

  None
}

fn load_custom_source_models(custom_url: &str) -> Result<Vec<SourceModel>, String> {
  let custom_url = custom_url.trim();
  if custom_url.is_empty() {
    return Ok(Vec::new());
  }

  // Security: Validate custom URL before fetching
  validate_model_url(custom_url, UrlSafety::Strict)?;

  if custom_url.ends_with(".bin") || custom_url.ends_with(".gguf") {
    let file_name = filename_from_url(custom_url)
      .ok_or_else(|| "Invalid model URL".to_string())?;
    validate_model_file_name(&file_name)?;
    let id = file_name
      .trim_end_matches(".bin")
      .trim_end_matches(".gguf")
      .to_string();
    let label = file_name.clone();
    return Ok(vec![SourceModel {
      id,
      label,
      file_name,
      size_mb: 0,
      download_url: custom_url.to_string(),
      source: "custom".to_string(),
    }]);
  }

  let response = http_get_with_redirects(custom_url)
    .map_err(|e| format!("Failed to fetch model index: {e}"))?;
  let mut body = String::new();
  response
    .into_reader()
    .read_to_string(&mut body)
    .map_err(|e| format!("Failed to read model index: {e}"))?;

  if let Ok(entries) = serde_json::from_str::<Vec<ModelIndexEntry>>(&body) {
    let mut results = Vec::new();
    for entry in entries {
      let label = if entry.label.trim().is_empty() {
        entry.id.clone()
      } else {
        entry.label.clone()
      };
      let download_url = entry
        .url
        .clone()
        .unwrap_or_else(|| custom_url.trim_end_matches('/').to_string() + "/" + &entry.file_name);
      if let Err(err) = validate_model_file_name(&entry.file_name) {
        warn!("Skipping model {}: {}", entry.id, err);
        continue;
      }
      if let Err(err) = is_url_safe(&download_url, UrlSafety::Basic) {
        warn!("Skipping model {}: {}", entry.id, err);
        continue;
      }
      results.push(SourceModel {
        id: entry.id,
        label,
        file_name: entry.file_name,
        size_mb: entry.size_mb,
        download_url,
        source: "custom".to_string(),
      });
    }
    return Ok(results);
  }

  let index: ModelIndex = serde_json::from_str(&body)
    .map_err(|_| "Unsupported model index format".to_string())?;
  let base_url = match index.base_url {
    Some(url) => match validate_model_url(&url, UrlSafety::Basic) {
      Ok(_) => url,
      Err(err) => {
        warn!("Ignoring unsafe model index base_url: {}", err);
        custom_url.trim_end_matches('/').to_string()
      }
    },
    None => custom_url.trim_end_matches('/').to_string(),
  };

  let mut results = Vec::new();
  for entry in index.models {
    let label = if entry.label.trim().is_empty() {
      entry.id.clone()
    } else {
      entry.label.clone()
    };
    let download_url = entry
      .url
      .clone()
      .unwrap_or_else(|| base_url.trim_end_matches('/').to_string() + "/" + &entry.file_name);
    if let Err(err) = validate_model_file_name(&entry.file_name) {
      warn!("Skipping model {}: {}", entry.id, err);
      continue;
    }
    if let Err(err) = is_url_safe(&download_url, UrlSafety::Basic) {
      warn!("Skipping model {}: {}", entry.id, err);
      continue;
    }
    results.push(SourceModel {
      id: entry.id,
      label,
      file_name: entry.file_name,
      size_mb: entry.size_mb,
      download_url,
      source: "custom".to_string(),
    });
  }

  Ok(results)
}

fn float_to_i16(sample: f32) -> i16 {
  let clamped = sample.clamp(-1.0, 1.0);
  (clamped * i16::MAX as f32) as i16
}

/// Dynamic threshold calculator with smoothed rise/fall
/// Tracks ambient noise floor and sets sustain threshold above it
struct DynamicThreshold {
  /// Smoothed ambient level (noise floor estimate)
  ambient_level: std::sync::atomic::AtomicU64,
  /// Current dynamic threshold
  dynamic_threshold: std::sync::atomic::AtomicU64,
  /// Minimum threshold (never go below this)
  min_threshold: f32,
  /// Maximum threshold (never exceed this - stays below start threshold)
  max_threshold: f32,
  /// Multiplier above ambient for threshold
  ambient_multiplier: f32,
  /// Rise time constant in ms (slower - for increasing threshold)
  rise_tau_ms: f32,
  /// Fall time constant in ms (faster - for decreasing threshold)
  fall_tau_ms: f32,
  /// Last update timestamp
  last_update_ms: AtomicU64,
}

impl DynamicThreshold {
  fn new(min_threshold: f32, max_threshold: f32) -> Self {
    // Start with low ambient estimate for fast initial sensitivity
    let initial_ambient = (min_threshold * 0.3 * 1_000_000.0) as u64;
    let initial_threshold = (min_threshold * 1_000_000.0) as u64;
    Self {
      ambient_level: std::sync::atomic::AtomicU64::new(initial_ambient),
      dynamic_threshold: std::sync::atomic::AtomicU64::new(initial_threshold),
      min_threshold,
      max_threshold: max_threshold.max(min_threshold), // Ensure max >= min
      ambient_multiplier: 1.5, // Threshold is 1.5x ambient level (reduced from 2.5)
      rise_tau_ms: 1000.0,     // 1 second rise time (faster than before)
      fall_tau_ms: 300.0,      // 0.3 second fall time (faster for sensitivity)
      last_update_ms: AtomicU64::new(0),
    }
  }

  /// Update with new audio level sample, returns current dynamic threshold
  fn update(&self, level: f32, now_ms: u64) -> f32 {
    let last = self.last_update_ms.swap(now_ms, Ordering::Relaxed);
    let dt_ms = now_ms.saturating_sub(last) as f32;
    if dt_ms <= 0.0 {
      return self.get_threshold();
    }

    // Get current values
    let current_ambient = self.ambient_level.load(Ordering::Relaxed) as f32 / 1_000_000.0;

    // Update ambient level with exponential smoothing
    // Use moderate time constant for ambient to track noise floor
    let ambient_tau_ms = 1500.0; // 1.5 second time constant for ambient (faster init)
    let ambient_alpha = 1.0 - (-dt_ms / ambient_tau_ms).exp();
    let new_ambient = current_ambient + (level - current_ambient) * ambient_alpha;
    self.ambient_level.store((new_ambient * 1_000_000.0) as u64, Ordering::Relaxed);

    // Calculate target threshold based on ambient
    let target_threshold = (new_ambient * self.ambient_multiplier).max(self.min_threshold);

    // Get current threshold and smooth toward target
    let current_threshold = self.dynamic_threshold.load(Ordering::Relaxed) as f32 / 1_000_000.0;

    // Use different time constants for rise vs fall
    let tau = if target_threshold > current_threshold {
      self.rise_tau_ms
    } else {
      self.fall_tau_ms
    };
    let alpha = 1.0 - (-dt_ms / tau).exp();
    let new_threshold = current_threshold + (target_threshold - current_threshold) * alpha;
    let clamped_threshold = new_threshold.clamp(self.min_threshold, self.max_threshold);

    self.dynamic_threshold.store((clamped_threshold * 1_000_000.0) as u64, Ordering::Relaxed);

    clamped_threshold
  }

  fn get_threshold(&self) -> f32 {
    self.dynamic_threshold.load(Ordering::Relaxed) as f32 / 1_000_000.0
  }
}

struct OverlayLevelEmitter {
  app: AppHandle,
  start: Instant,
  last_emit_ms: AtomicU64,
  dynamic_threshold: DynamicThreshold,
  last_threshold_emit_ms: AtomicU64,
  smooth_level: AtomicU64,
  last_smooth_ms: AtomicU64,
}

impl OverlayLevelEmitter {
  fn new(app: AppHandle, min_sustain_threshold: f32, start_threshold: f32) -> Self {
    // max_threshold is 90% of start_threshold to ensure sustain stays below start
    let max_threshold = start_threshold * 0.9;
    Self {
      app,
      start: Instant::now(),
      last_emit_ms: AtomicU64::new(0),
      dynamic_threshold: DynamicThreshold::new(min_sustain_threshold, max_threshold),
      last_threshold_emit_ms: AtomicU64::new(0),
      smooth_level: AtomicU64::new(0),
      last_smooth_ms: AtomicU64::new(0),
    }
  }

  fn emit_level(&self, level: f32) {
    let now_ms = self.start.elapsed().as_millis() as u64;
    let last = self.last_emit_ms.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < 50 {
      return;
    }
    self.last_emit_ms.store(now_ms, Ordering::Relaxed);

    let level_clamped = level.clamp(0.0, 1.0);

    // Update dynamic threshold with current level
    let dynamic_thresh = self.dynamic_threshold.update(level_clamped, now_ms);

    // audio:level as float 0.0-1.0 for main UI
    let _ = self.app.emit("audio:level", level_clamped);

    // Emit dynamic threshold periodically (every 200ms to reduce overhead)
    let last_thresh_emit = self.last_threshold_emit_ms.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last_thresh_emit) >= 200 {
      self.last_threshold_emit_ms.store(now_ms, Ordering::Relaxed);
      let _ = self.app.emit("vad:dynamic-threshold", dynamic_thresh);
    }

    // Update overlay directly via JS (most reliable method)
    if let Some(window) = self.app.get_webview_window("overlay") {
      if let Ok(state) = self.app.state::<AppState>().settings.lock() {
        let (rise_ms, fall_ms) = if state.overlay_style == "kitt" {
          (state.overlay_kitt_rise_ms, state.overlay_kitt_fall_ms)
        } else {
          (state.overlay_rise_ms, state.overlay_fall_ms)
        };

        let last_smooth = self.last_smooth_ms.load(Ordering::Relaxed);
        let mut current = self.smooth_level.load(Ordering::Relaxed) as f32 / 1_000_000.0;
        if last_smooth == 0 {
          current = level_clamped;
        } else {
          let dt = now_ms.saturating_sub(last_smooth).max(1) as f32;
          let tau = if level_clamped > current { rise_ms } else { fall_ms };
          let denom = tau.max(1) as f32;
          // Linear ramp: full-scale (0->1) takes ~tau ms
          let max_step = (dt / denom).min(1.0);
          let delta = level_clamped - current;
          if delta.abs() <= max_step {
            current = level_clamped;
          } else {
            current += max_step * delta.signum();
          }
        }
        self.last_smooth_ms.store(now_ms, Ordering::Relaxed);
        let clamped = current.clamp(0.0, 1.0);
        self.smooth_level.store((clamped * 1_000_000.0) as u64, Ordering::Relaxed);

        if state.overlay_style == "kitt" {
          let js = format!("if(window.setOverlayLevel){{window.setOverlayLevel({});}}", clamped);
          let _ = window.eval(&js);
        } else {
          let min_radius = state.overlay_min_radius.max(4.0) as f64;
          let max_radius = state.overlay_max_radius.max(min_radius as f32) as f64;
          // Use level 0.01-1.0 (never fully zero)
          let factor = (clamped as f64).max(0.01);
          let radius = min_radius + (max_radius - min_radius) * factor;
          let size = (radius * 2.0).round();
          let js = format!(
            "(function(){{const d=document.getElementById('dot');if(d){{d.style.width='{}px';d.style.height='{}px';}}}})();",
            size, size
          );
          let _ = window.eval(&js);
        }
      }
    }
  }
}

#[derive(Debug)]
struct VadRuntime {
  recording: std::sync::atomic::AtomicBool,
  pending_flush: std::sync::atomic::AtomicBool,
  processing: std::sync::atomic::AtomicBool,
  last_voice_ms: AtomicU64,
  start_ms: AtomicU64,
  audio_cues: bool,
  /// Start threshold (higher) - used to initiate recording
  threshold_start_scaled: AtomicU64,
  /// Sustain threshold (lower) - used to keep recording active
  threshold_sustain_scaled: AtomicU64,
  silence_ms: AtomicU64,
  /// Counter for consecutive chunks above threshold (spike filter)
  consecutive_above: AtomicU64,
}

/// Minimum consecutive chunks above threshold to start recording (spike filter)
const VAD_MIN_CONSECUTIVE_CHUNKS: u64 = 3;
const HALLUCINATION_RMS_THRESHOLD: f32 = 0.012; // ~ -38 dB
const HALLUCINATION_MAX_WORDS: usize = 2;
const HALLUCINATION_MAX_CHARS: usize = 12;
const HALLUCINATION_MAX_DURATION_MS: u64 = 1200;

const HALLUCINATION_PHRASES: &[&str] = &[
  "you",
  "thank you",
  "thanks",
  "okay",
  "ok",
  "yeah",
  "yes",
  "no",
  "uh",
  "um",
  "hmm",
  "huh",
];

fn normalize_transcript(text: &str) -> String {
  text
    .chars()
    .filter(|c| c.is_alphanumeric() || c.is_whitespace())
    .collect::<String>()
    .to_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

fn should_drop_transcript(text: &str, rms: f32, duration_ms: u64) -> bool {
  let normalized = normalize_transcript(text);
  if normalized.is_empty() {
    return true;
  }
  let word_count = normalized.split_whitespace().count();
  let is_short = word_count <= HALLUCINATION_MAX_WORDS || normalized.len() <= HALLUCINATION_MAX_CHARS;
  let is_low_energy = rms < HALLUCINATION_RMS_THRESHOLD;
  let is_short_audio = duration_ms <= HALLUCINATION_MAX_DURATION_MS;
  let matches_common = HALLUCINATION_PHRASES.iter().any(|p| *p == normalized);

  if matches_common && is_short_audio {
    return true;
  }

  is_low_energy && is_short_audio && is_short
}

impl VadRuntime {
  fn new(audio_cues: bool, threshold_start: f32, threshold_sustain: f32, silence_ms: u64) -> Self {
    let start_scaled = (threshold_start.clamp(0.001, 0.5) * 1_000_000.0) as u64;
    let sustain_scaled = (threshold_sustain.clamp(0.001, 0.5) * 1_000_000.0) as u64;
    Self {
      recording: std::sync::atomic::AtomicBool::new(false),
      pending_flush: std::sync::atomic::AtomicBool::new(false),
      processing: std::sync::atomic::AtomicBool::new(false),
      last_voice_ms: AtomicU64::new(0),
      start_ms: AtomicU64::new(0),
      audio_cues,
      threshold_start_scaled: AtomicU64::new(start_scaled),
      threshold_sustain_scaled: AtomicU64::new(sustain_scaled),
      silence_ms: AtomicU64::new(silence_ms.max(100)),
      consecutive_above: AtomicU64::new(0),
    }
  }

  /// Get threshold for starting recording (higher)
  fn threshold_start(&self) -> f32 {
    self.threshold_start_scaled.load(Ordering::Relaxed) as f32 / 1_000_000.0
  }

  /// Get threshold for sustaining recording (lower)
  fn threshold_sustain(&self) -> f32 {
    self.threshold_sustain_scaled.load(Ordering::Relaxed) as f32 / 1_000_000.0
  }

  fn update_thresholds(&self, threshold_start: f32, threshold_sustain: f32) {
    let start_scaled = (threshold_start.clamp(0.001, 0.5) * 1_000_000.0) as u64;
    let sustain_scaled = (threshold_sustain.clamp(0.001, 0.5) * 1_000_000.0) as u64;
    self.threshold_start_scaled.store(start_scaled, Ordering::Relaxed);
    self.threshold_sustain_scaled.store(sustain_scaled, Ordering::Relaxed);
  }

  fn silence_ms(&self) -> u64 {
    self.silence_ms.load(Ordering::Relaxed)
  }

  fn update_silence_ms(&self, silence_ms: u64) {
    self.silence_ms.store(silence_ms.max(100), Ordering::Relaxed);
  }
}

#[derive(Debug, Clone)]
enum VadEvent {
  Finalize(Vec<i16>),
}

#[derive(Clone)]
struct VadHandle {
  runtime: Arc<VadRuntime>,
  tx: std::sync::mpsc::Sender<VadEvent>,
  app: AppHandle,
}

fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
  let manager = app.global_shortcut();
  let _ = manager.unregister_all();

  let register_ptt = || -> Result<(), String> {
    let ptt = settings.hotkey_ptt.trim();
    if ptt.is_empty() {
      return Ok(());
    }
    info!("Registering PTT hotkey (hold): {}", ptt);
    match manager.on_shortcut(ptt, |app, _shortcut, event| {
      let app = app.clone();
      if event.state == ShortcutState::Pressed {
        info!("PTT hotkey pressed");
        let _ = handle_ptt_press(&app);
      } else {
        info!("PTT hotkey released");
        handle_ptt_release_async(app);
      }
    }) {
      Ok(_) => {
        info!("PTT hotkey registered successfully");
        Ok(())
      }
      Err(e) => {
        error!("Failed to register PTT hotkey '{}': {}", ptt, e);
        emit_error(app, AppError::Hotkey(format!("Could not register PTT hotkey '{}': {}. Try a different key.", ptt, e)), Some("Hotkey Registration"));
        Err(e.to_string())
      }
    }
  };

  let register_toggle = || -> Result<(), String> {
    let toggle = settings.hotkey_toggle.trim();
    if toggle.is_empty() {
      return Ok(());
    }
    info!("Registering Toggle hotkey (click): {}", toggle);
    match manager.on_shortcut(toggle, |app, _shortcut, event| {
      if event.state == ShortcutState::Pressed {
        info!("Toggle hotkey pressed");
        let app = app.clone();
        handle_toggle_async(app);
      }
    }) {
      Ok(_) => {
        info!("Toggle hotkey registered successfully");
        Ok(())
      }
      Err(e) => {
        error!("Failed to register Toggle hotkey '{}': {}", toggle, e);
        emit_error(app, AppError::Hotkey(format!("Could not register Toggle hotkey '{}': {}. Try a different key.", toggle, e)), Some("Hotkey Registration"));
        Err(e.to_string())
      }
    }
  };

  let register_transcribe = || -> Result<(), String> {
    let hotkey = settings.transcribe_hotkey.trim();
    if hotkey.is_empty() {
      return Ok(());
    }
    info!("Registering Transcribe hotkey (toggle): {}", hotkey);
    match manager.on_shortcut(hotkey, |app, _shortcut, event| {
      if event.state == ShortcutState::Pressed {
        let app = app.clone();
        toggle_transcribe_state(&app);
      }
    }) {
      Ok(_) => {
        info!("Transcribe hotkey registered successfully");
        Ok(())
      }
      Err(e) => {
        error!("Failed to register Transcribe hotkey '{}': {}", hotkey, e);
        emit_error(app, AppError::Hotkey(format!("Could not register Transcribe hotkey '{}': {}. Try a different key.", hotkey, e)), Some("Hotkey Registration"));
        Err(e.to_string())
      }
    }
  };

  match settings.mode.as_str() {
    "ptt" => {
      register_ptt()?;
      register_toggle()?;
    }
    "vad" => {}
    _ => {
      register_ptt()?;
      register_toggle()?;
    }
  }

  register_transcribe()?;

  Ok(())
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
  state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
  let (prev_mode, prev_device, prev_capture_enabled, prev_transcribe_enabled) = {
    let current = state.settings.lock().unwrap();
    (current.mode.clone(), current.input_device.clone(), current.capture_enabled, current.transcribe_enabled)
  };
  {
    let mut current = state.settings.lock().unwrap();
    *current = settings.clone();
  }
  sync_model_dir_env(&settings);
  save_settings_file(&app, &settings)?;
  register_hotkeys(&app, &settings)?;

  if let Ok(recorder) = state.recorder.lock() {
    recorder
      .input_gain_db
      .store((settings.mic_input_gain_db * 1000.0) as i64, Ordering::Relaxed);
  }

  let mode_changed = prev_mode != settings.mode;
  let device_changed = prev_device != settings.input_device;

  // Restart VAD monitor if mode changed or device changed (while in VAD mode)
  if mode_changed || (device_changed && settings.mode == "vad") {
    if prev_mode == "vad" || (settings.mode == "vad" && device_changed) {
      stop_vad_monitor(&app, &state);
    }
    if settings.mode == "vad" && settings.capture_enabled {
      let _ = start_vad_monitor(&app, &state, &settings);
    }
  } else if settings.mode == "vad" {
    // Only update thresholds/silence if we didn't restart the monitor
    if let Ok(recorder) = state.recorder.lock() {
      if let Some(runtime) = recorder.vad_runtime.as_ref() {
        runtime.update_thresholds(settings.vad_threshold_start, settings.vad_threshold_sustain);
        runtime.update_silence_ms(settings.vad_silence_ms);
      }
    }
  }

  // Handle capture_enabled changes
  let capture_enabled_changed = prev_capture_enabled != settings.capture_enabled;
  if capture_enabled_changed {
    if !settings.capture_enabled {
      stop_vad_monitor(&app, &state);
    } else if settings.mode == "vad" {
      // Start VAD monitor when enabling capture in VAD mode
      let _ = start_vad_monitor(&app, &state, &settings);
    }
  }

  // Handle transcribe_enabled changes
  let transcribe_enabled_changed = prev_transcribe_enabled != settings.transcribe_enabled;
  if transcribe_enabled_changed {
    if !settings.transcribe_enabled {
      stop_transcribe_monitor(&app, &state);
    } else {
      // Start transcribe monitor when enabling
      let _ = start_transcribe_monitor(&app, &state, &settings);
    }
  }

  let overlay_settings = build_overlay_settings(&settings);
  let _ = overlay::apply_overlay_settings(&app, &overlay_settings);

  // Update overlay state based on new mode
  let recorder = state.recorder.lock().unwrap();
  if !recorder.active {
    let _ = overlay::update_overlay_state(&app, overlay::OverlayState::Idle);
  }
  drop(recorder);

  let _ = app.emit("settings-changed", settings.clone());
  let _ = app.emit("menu:update-mic", settings.capture_enabled);
  let _ = app.emit("menu:update-transcribe", settings.transcribe_enabled);
  Ok(())
}

#[tauri::command]
fn list_audio_devices() -> Vec<AudioDevice> {
  let mut devices = vec![AudioDevice {
    id: "default".to_string(),
    label: "Default (System)".to_string(),
  }];

  let host = cpal::default_host();
  if let Ok(inputs) = host.input_devices() {
    for (index, device) in inputs.enumerate() {
      let name = device
        .name()
        .unwrap_or_else(|_| format!("Input {}", index + 1));
      let id = format!("input-{}-{}", index, name);
      devices.push(AudioDevice { id, label: name });
    }
  }

  devices
}

#[tauri::command]
fn list_output_devices() -> Vec<AudioDevice> {
  let mut devices = vec![AudioDevice {
    id: "default".to_string(),
    label: "System Default Output".to_string(),
  }];

  #[cfg(target_os = "windows")]
  {
    if let Ok(enumerator) = wasapi::DeviceEnumerator::new() {
      if let Ok(collection) = enumerator.get_device_collection(&wasapi::Direction::Render) {
        if let Ok(count) = collection.get_nbr_devices() {
          for index in 0..count {
            if let Ok(device) = collection.get_device_at_index(index) {
              let name = device
                .get_friendlyname()
                .unwrap_or_else(|_| format!("Output {}", index + 1));
              let id = device.get_id().unwrap_or_else(|_| format!("idx-{}", index));
              devices.push(AudioDevice {
                id: format!("wasapi:{}", id),
                label: name,
              });
            }
          }
        }
      }
    }
  }

  #[cfg(not(target_os = "windows"))]
  {
    let host = cpal::default_host();
    if let Ok(outputs) = host.output_devices() {
      for (index, device) in outputs.enumerate() {
        let name = device
          .name()
          .unwrap_or_else(|_| format!("Output {}", index + 1));
        let id = format!("output-{}-{}", index, name);
        devices.push(AudioDevice { id, label: name });
      }
    }
  }

  devices
}

#[tauri::command]
fn list_models(app: AppHandle, state: State<'_, AppState>) -> Vec<ModelInfo> {
  let downloads = state.downloads.lock().unwrap();
  let settings = state.settings.lock().unwrap().clone();
  let models_dir = resolve_models_dir(&app);

  let source_models: Vec<SourceModel> = if settings.model_source == "custom" {
    match load_custom_source_models(&settings.model_custom_url) {
      Ok(list) => list,
      Err(err) => {
        warn!("Failed to load custom model source: {}", err);
        Vec::new()
      }
    }
  } else {
    let base_url = resolve_model_base_url();
    let mut defaults: Vec<SourceModel> = Vec::new();
    for spec in MODEL_SPECS {
      let download_url = format!("{}/{}", base_url.trim_end_matches('/'), spec.file_name);
      if let Err(err) = is_url_safe(&download_url, UrlSafety::Basic) {
        warn!("Skipping unsafe model URL for {}: {}", spec.id, err);
        continue;
      }
      defaults.push(SourceModel {
        id: spec.id.to_string(),
        label: spec.label.to_string(),
        file_name: spec.file_name.to_string(),
        size_mb: spec.size_mb,
        download_url,
        source: "default".to_string(),
      });
    }

    let distil_url = "https://huggingface.co/distil-whisper/distil-large-v3-ggml/resolve/main/ggml-distil-large-v3.bin";
    if let Err(err) = is_url_safe(distil_url, UrlSafety::Basic) {
      warn!("Skipping unsafe distil model URL: {}", err);
    } else {
      defaults.push(SourceModel {
        id: "ggml-distil-large-v3".to_string(),
        label: "Distil-Whisper large-v3 (EN)".to_string(),
        file_name: "ggml-distil-large-v3.bin".to_string(),
        size_mb: 1520,
        download_url: distil_url.to_string(),
        source: "distil".to_string(),
      });
    }

    defaults
  };

  let mut seen_files = HashSet::new();
  let mut models: Vec<ModelInfo> = source_models
    .into_iter()
    .map(|model| {
      let path = if settings.model_source == "default" {
        resolve_model_path(&app, &model.id)
      } else {
        resolve_model_path_by_file(&app, &model.file_name)
      };
      if !model.file_name.is_empty() {
        seen_files.insert(model.file_name.clone());
      }
      let removable = path
        .as_ref()
        .map(|p| p.starts_with(&models_dir))
        .unwrap_or(false);
      ModelInfo {
        id: model.id.clone(),
        label: model.label.clone(),
        file_name: model.file_name.clone(),
        size_mb: model.size_mb,
        installed: path.is_some(),
        downloading: downloads.contains(&model.id),
        path: path.map(|p| p.to_string_lossy().to_string()),
        source: model.source.clone(),
        available: true,
        download_url: Some(model.download_url.clone()),
        removable,
      }
    })
    .collect();

  if let Ok(entries) = fs::read_dir(&models_dir) {
    for entry in entries.flatten() {
      let path = entry.path();
      let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
      if extension != "bin" && extension != "gguf" {
        continue;
      }
      let file_name = match path.file_name().and_then(|s| s.to_str()) {
        Some(name) => name.to_string(),
        None => continue,
      };
      if seen_files.contains(&file_name) {
        continue;
      }
      let size_mb = entry
        .metadata()
        .map(|m| (m.len() / (1024 * 1024)) as u32)
        .unwrap_or(0);
      let id = file_name
        .trim_end_matches(".bin")
        .trim_end_matches(".gguf")
        .to_string();
      models.push(ModelInfo {
        id,
        label: file_name.clone(),
        file_name: file_name.clone(),
        size_mb,
        installed: true,
        downloading: false,
        path: Some(path.to_string_lossy().to_string()),
        source: "local".to_string(),
        available: false,
        download_url: None,
        removable: true,
      });
    }
  }

  models
}

#[tauri::command]
fn download_model(
  app: AppHandle,
  state: State<'_, AppState>,
  model_id: String,
  download_url: Option<String>,
  file_name: Option<String>,
) -> Result<(), String> {
  let (url, name) = if let Some(url) = download_url.clone() {
    let name = file_name
      .or_else(|| filename_from_url(&url))
      .ok_or_else(|| "Missing file name for custom download".to_string())?;
    validate_model_file_name(&name)?;
    // Security: Validate URL before downloading
    is_url_safe(&url, UrlSafety::Strict)?;
    (url, name)
  } else {
    let spec = model_spec(&model_id).ok_or_else(|| "Unknown model".to_string())?;
    let base_url = resolve_model_base_url();
    let name = spec.file_name.to_string();
    validate_model_file_name(&name)?;
    let url = format!("{}/{}", base_url.trim_end_matches('/'), spec.file_name);
    is_url_safe(&url, UrlSafety::Strict)?;
    (url, name)
  };
  {
    let mut downloads = state.downloads.lock().unwrap();
    if downloads.contains(&model_id) {
      return Err("Download already in progress".to_string());
    }
    downloads.insert(model_id.clone());
  }

  let app_handle = app.clone();
  thread::spawn(move || {
    let result = download_model_file(&app_handle, &model_id, &url, &name);
    match result {
      Ok(path) => {
        let _ = app_handle.emit(
          "model:download-complete",
          DownloadComplete {
            id: model_id.clone(),
            path: path.to_string_lossy().to_string(),
          },
        );
      }
      Err(error) => {
        let _ = app_handle.emit(
          "model:download-error",
          DownloadError {
            id: model_id.clone(),
            error,
          },
        );
      }
    }

    let state = app_handle.state::<AppState>();
    let mut downloads = state.downloads.lock().unwrap();
    downloads.remove(&model_id);
  });

  Ok(())
}

#[tauri::command]
fn remove_model(app: AppHandle, file_name: String) -> Result<(), String> {
  if file_name.trim().is_empty() {
    return Err("Missing model file name".to_string());
  }
  validate_model_file_name(&file_name)?;
  let models_dir = resolve_models_dir(&app);
  let target = models_dir.join(&file_name);
  if !target.exists() {
    return Err("Model file not found in app cache".to_string());
  }
  fs::remove_file(&target).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
fn pick_model_dir() -> Option<String> {
  rfd::FileDialog::new()
    .pick_folder()
    .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn get_models_dir(app: AppHandle) -> String {
  resolve_models_dir(&app).to_string_lossy().to_string()
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
  state.history.lock().unwrap().clone()
}

#[tauri::command]
fn get_transcribe_history(state: State<'_, AppState>) -> Vec<HistoryEntry> {
  state.history_transcribe.lock().unwrap().clone()
}

#[tauri::command]
fn add_history_entry(
  app: AppHandle,
  state: State<'_, AppState>,
  text: String,
  source: Option<String>,
) -> Result<Vec<HistoryEntry>, String> {
  let source = source.unwrap_or_else(|| "local".to_string());
  push_history_entry(&app, &state, text, source)
}

#[tauri::command]
fn add_transcribe_entry(
  app: AppHandle,
  state: State<'_, AppState>,
  text: String,
) -> Result<Vec<HistoryEntry>, String> {
  push_transcribe_entry_inner(&app, &state.history_transcribe, text)
}

#[tauri::command]
fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
  let settings = state.settings.lock().unwrap().clone();
  start_recording_with_settings(&app, &state, &settings)
}

#[tauri::command]
fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
  stop_recording_async(app, &state);
  Ok(())
}

#[tauri::command]
fn toggle_transcribe(app: AppHandle) -> Result<(), String> {
  toggle_transcribe_state(&app);
  Ok(())
}

#[tauri::command]
fn open_conversation_window(app: AppHandle) -> Result<(), String> {
  if app.get_webview_window("conversation").is_some() {
    if let Some(window) = app.get_webview_window("conversation") {
      let _ = window.show();
      let _ = window.set_focus();
    }
    return Ok(());
  }

  let window = tauri::WebviewWindowBuilder::new(
    &app,
    "conversation",
    tauri::WebviewUrl::App("index.html".into()),
  )
  .title("Trispr Flow · Conversation")
  .inner_size(860.0, 680.0)
  .min_inner_size(640.0, 420.0)
  .resizable(true)
  .decorations(true)
  .transparent(false)
  .visible(true)
  .build()
  .map_err(|e| e.to_string())?;

  let _ = window.eval(
    "window.__TRISPR_VIEW__='conversation'; window.dispatchEvent(new CustomEvent('trispr:view', { detail: 'conversation' }));",
  );

  Ok(())
}

#[tauri::command]
fn validate_hotkey(key: String) -> hotkeys::ValidationResult {
  hotkeys::validate_hotkey_format(&key)
}

#[tauri::command]
fn test_hotkey(app: AppHandle, key: String) -> Result<(), String> {
  hotkeys::test_hotkey_registration(&app, &key)
}

#[tauri::command]
fn get_hotkey_conflicts(state: State<'_, AppState>) -> Vec<hotkeys::ConflictInfo> {
  let settings = state.settings.lock().unwrap();
  let hotkeys = vec![
    settings.hotkey_ptt.clone(),
    settings.hotkey_toggle.clone(),
    settings.transcribe_hotkey.clone(),
  ];
  hotkeys::detect_conflicts(hotkeys)
}

fn resolve_input_device(device_id: &str) -> Option<cpal::Device> {
  let host = cpal::default_host();
  if device_id == "default" {
    return host.default_input_device();
  }

  if let Ok(inputs) = host.input_devices() {
    for (index, device) in inputs.enumerate() {
      let name = device
        .name()
        .unwrap_or_else(|_| format!("Input {}", index + 1));
      let id = format!("input-{}-{}", index, name);
      if id == device_id {
        return Some(device);
      }
    }
  }

  host.default_input_device()
}

#[cfg(target_os = "windows")]
fn resolve_output_device(device_id: &str) -> Option<wasapi::Device> {
  let enumerator = wasapi::DeviceEnumerator::new().ok()?;
  if device_id == "default" {
    return enumerator.get_default_device(&wasapi::Direction::Render).ok();
  }

  if let Some(id) = device_id.strip_prefix("wasapi:") {
    if let Ok(device) = enumerator.get_device(id) {
      return Some(device);
    }
  }

  enumerator.get_default_device(&wasapi::Direction::Render).ok()
}

#[cfg(not(target_os = "windows"))]
fn resolve_output_device(_device_id: &str) -> Option<cpal::Device> {
  None
}

fn push_mono_samples(
  buffer: &Arc<Mutex<CaptureBuffer>>,
  mono: &[f32],
  sample_rate: u32,
) {
  if let Ok(mut guard) = buffer.lock() {
    guard.push_samples(mono, sample_rate);
  }
}

fn handle_vad_audio(
  vad_handle: &VadHandle,
  buffer: &Arc<Mutex<CaptureBuffer>>,
  mono: Vec<f32>,
  level: f32,
  sample_rate: u32,
) {
  let runtime = &vad_handle.runtime;
  let now = now_ms();
  let is_recording = runtime.recording.load(Ordering::Relaxed);

  // Use different thresholds based on recording state:
  // - Start threshold (higher) to initiate recording
  // - Sustain threshold (lower) to keep recording active
  let threshold = if is_recording {
    runtime.threshold_sustain()
  } else {
    runtime.threshold_start()
  };

  if level >= threshold {
    // Increment consecutive counter (spike filter)
    let consecutive = runtime.consecutive_above.fetch_add(1, Ordering::Relaxed) + 1;
    runtime.last_voice_ms.store(now, Ordering::Relaxed);

    // Only start recording if above START threshold for enough consecutive chunks
    if !is_recording && consecutive >= VAD_MIN_CONSECUTIVE_CHUNKS {
      runtime.recording.store(true, Ordering::Relaxed);
      runtime.start_ms.store(now, Ordering::Relaxed);
      runtime.pending_flush.store(false, Ordering::Relaxed);
      if let Ok(mut buf) = buffer.lock() {
        buf.reset();
      }
      let _ = vad_handle.app.emit("capture:state", "recording");
      let _ = update_overlay_state(&vad_handle.app, OverlayState::Recording);
      if runtime.audio_cues {
        let _ = vad_handle.app.emit("audio:cue", "start");
      }
    }
  } else {
    // Reset consecutive counter when below threshold
    // (only matters for START; sustain uses silence timeout)
    if !is_recording {
      runtime.consecutive_above.store(0, Ordering::Relaxed);
    }
  }

  if runtime.recording.load(Ordering::Relaxed) {
    push_mono_samples(buffer, &mono, sample_rate);

    let last = runtime.last_voice_ms.load(Ordering::Relaxed);
    let start = runtime.start_ms.load(Ordering::Relaxed);
    let silence_ms = runtime.silence_ms();
    if now.saturating_sub(last) > silence_ms && now.saturating_sub(start) > VAD_MIN_VOICE_MS {
      if !runtime.pending_flush.swap(true, Ordering::Relaxed) {
        runtime.recording.store(false, Ordering::Relaxed);
        let samples = {
          let mut buf = buffer.lock().unwrap();
          buf.drain()
        };
        let _ = vad_handle.tx.send(VadEvent::Finalize(samples));
      }
    }
  }
}

fn build_input_stream_f32(
  device: &cpal::Device,
  config: &StreamConfig,
  buffer: Arc<Mutex<CaptureBuffer>>,
  overlay: Option<Arc<OverlayLevelEmitter>>,
  vad: Option<VadHandle>,
  gain_db: Arc<AtomicI64>,
) -> Result<cpal::Stream, String> {
  let channels = config.channels as usize;
  let sample_rate = config.sample_rate.0;
  let err_fn = |err| eprintln!("audio stream error: {}", err);

  device
    .build_input_stream(
      config,
      move |data: &[f32], _| {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        let mut sum_squared = 0.0f32;
        let gain_db = gain_db.load(Ordering::Relaxed) as f32 / 1000.0;
        let gain = (10.0f32).powf(gain_db / 20.0);
        for frame in data.chunks(channels.max(1)) {
          let mut sum = 0.0f32;
          for &sample in frame {
            sum += sample;
          }
          let sample = (sum / channels.max(1) as f32 * gain).clamp(-1.0, 1.0);
          mono.push(sample);
          sum_squared += sample * sample;
        }
        // RMS calculation with scaling for better visualization
        let level = if mono.is_empty() {
          0.0
        } else {
          let rms = (sum_squared / mono.len() as f32).sqrt();
          // Scale RMS (typically 0.0-0.2 for speech) to 0.0-1.0 range
          (rms * 2.5).min(1.0)
        };
        if let Some(emitter) = overlay.as_ref() {
          emitter.emit_level(level);
        }
        if let Some(vad_handle) = vad.as_ref() {
          handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
        } else {
          push_mono_samples(&buffer, &mono, sample_rate);
        }
      },
      err_fn,
      None,
    )
    .map_err(|e| e.to_string())
}

fn build_input_stream_i16(
  device: &cpal::Device,
  config: &StreamConfig,
  buffer: Arc<Mutex<CaptureBuffer>>,
  overlay: Option<Arc<OverlayLevelEmitter>>,
  vad: Option<VadHandle>,
  gain_db: Arc<AtomicI64>,
) -> Result<cpal::Stream, String> {
  let channels = config.channels as usize;
  let sample_rate = config.sample_rate.0;
  let err_fn = |err| eprintln!("audio stream error: {}", err);

  device
    .build_input_stream(
      config,
      move |data: &[i16], _| {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        let mut sum_squared = 0.0f32;
        let gain_db = gain_db.load(Ordering::Relaxed) as f32 / 1000.0;
        let gain = (10.0f32).powf(gain_db / 20.0);
        for frame in data.chunks(channels.max(1)) {
          let mut sum = 0.0f32;
          for &sample in frame {
            sum += sample as f32 / i16::MAX as f32;
          }
          let sample = (sum / channels.max(1) as f32 * gain).clamp(-1.0, 1.0);
          mono.push(sample);
          sum_squared += sample * sample;
        }
        // RMS calculation with scaling for better visualization
        let level = if mono.is_empty() {
          0.0
        } else {
          let rms = (sum_squared / mono.len() as f32).sqrt();
          (rms * 2.5).min(1.0)
        };
        if let Some(emitter) = overlay.as_ref() {
          emitter.emit_level(level);
        }
        if let Some(vad_handle) = vad.as_ref() {
          handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
        } else {
          push_mono_samples(&buffer, &mono, sample_rate);
        }
      },
      err_fn,
      None,
    )
    .map_err(|e| e.to_string())
}

fn build_input_stream_u16(
  device: &cpal::Device,
  config: &StreamConfig,
  buffer: Arc<Mutex<CaptureBuffer>>,
  overlay: Option<Arc<OverlayLevelEmitter>>,
  vad: Option<VadHandle>,
  gain_db: Arc<AtomicI64>,
) -> Result<cpal::Stream, String> {
  let channels = config.channels as usize;
  let sample_rate = config.sample_rate.0;
  let err_fn = |err| eprintln!("audio stream error: {}", err);

  device
    .build_input_stream(
      config,
      move |data: &[u16], _| {
        let mut mono = Vec::with_capacity(data.len() / channels.max(1));
        let mut sum_squared = 0.0f32;
        let gain_db = gain_db.load(Ordering::Relaxed) as f32 / 1000.0;
        let gain = (10.0f32).powf(gain_db / 20.0);
        for frame in data.chunks(channels.max(1)) {
          let mut sum = 0.0f32;
          for &sample in frame {
            let centered = sample as f32 - 32768.0;
            sum += centered / 32768.0;
          }
          let sample = (sum / channels.max(1) as f32 * gain).clamp(-1.0, 1.0);
          mono.push(sample);
          sum_squared += sample * sample;
        }
        // RMS calculation with scaling for better visualization
        let level = if mono.is_empty() {
          0.0
        } else {
          let rms = (sum_squared / mono.len() as f32).sqrt();
          (rms * 2.5).min(1.0)
        };
        if let Some(emitter) = overlay.as_ref() {
          emitter.emit_level(level);
        }
        if let Some(vad_handle) = vad.as_ref() {
          handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
        } else {
          push_mono_samples(&buffer, &mono, sample_rate);
        }
      },
      err_fn,
      None,
    )
    .map_err(|e| e.to_string())
}

fn start_recording_with_settings(
  app: &AppHandle,
  state: &State<'_, AppState>,
  settings: &Settings,
) -> Result<(), String> {
  info!("start_recording_with_settings called");
  if !settings.capture_enabled {
    return Ok(());
  }
  let mut recorder = state.recorder.lock().unwrap();
  if recorder.active || recorder.transcribing {
    info!("Recording already active or transcribing, skipping");
    return Ok(());
  }

  if let Ok(mut buf) = recorder.buffer.lock() {
    buf.reset();
  }

  recorder
    .input_gain_db
    .store((settings.mic_input_gain_db * 1000.0) as i64, Ordering::Relaxed);
  let gain_db = recorder.input_gain_db.clone();
  let buffer = recorder.buffer.clone();
  let overlay_emitter = Arc::new(OverlayLevelEmitter::new(app.clone(), settings.vad_threshold_sustain, settings.vad_threshold_start));
  let device_id = settings.input_device.clone();
  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

  let join_handle = thread::spawn(move || {
    let result = (|| -> Result<(), String> {
      let device = resolve_input_device(&device_id)
        .ok_or_else(|| "No input device available".to_string())?;
      let config = device
        .default_input_config()
        .map_err(|e| e.to_string())?;
      let stream_config: StreamConfig = config.clone().into();

      let overlay = Some(overlay_emitter);
      let vad = None;
      let stream = match config.sample_format() {
        SampleFormat::F32 => build_input_stream_f32(&device, &stream_config, buffer, overlay.clone(), vad.clone(), gain_db.clone())?,
        SampleFormat::I16 => build_input_stream_i16(&device, &stream_config, buffer, overlay.clone(), vad.clone(), gain_db.clone())?,
        SampleFormat::U16 => build_input_stream_u16(&device, &stream_config, buffer, overlay.clone(), vad.clone(), gain_db.clone())?,
        _ => return Err("Unsupported sample format".to_string()),
      };

      stream.play().map_err(|e| e.to_string())?;
      let _ = ready_tx.send(Ok(()));

      let _ = stop_rx.recv();
      drop(stream);
      Ok(())
    })();

    if let Err(err) = result {
      let _ = ready_tx.send(Err(err));
    }
  });

  let start_result = match ready_rx.recv_timeout(Duration::from_secs(3)) {
    Ok(Ok(())) => Ok(()),
    Ok(Err(err)) => Err(err),
    Err(_) => Err("Failed to start audio stream".to_string()),
  };

  if let Err(err) = start_result {
    error!("Failed to start recording: {}", err);
    let _ = stop_tx.send(());
    let _ = join_handle.join();
    return Err(err);
  }

  recorder.stop_tx = Some(stop_tx);
  recorder.join_handle = Some(join_handle);
  recorder.active = true;

  info!("Recording started successfully, updating overlay");
  let _ = app.emit("capture:state", "recording");
  let _ = update_overlay_state(app, OverlayState::Recording);

  // Emit audio cue if enabled
  if settings.audio_cues {
    let _ = app.emit("audio:cue", "start");
  }

  Ok(())
}

fn start_vad_monitor(
  app: &AppHandle,
  state: &State<'_, AppState>,
  settings: &Settings,
) -> Result<(), String> {
  info!("start_vad_monitor called");
  if !settings.capture_enabled {
    return Ok(());
  }
  let mut recorder = state.recorder.lock().unwrap();
  if recorder.active || recorder.transcribing {
    info!("VAD already active or transcribing, skipping");
    return Ok(());
  }

  if let Ok(mut buf) = recorder.buffer.lock() {
    buf.reset();
  }

  recorder
    .input_gain_db
    .store((settings.mic_input_gain_db * 1000.0) as i64, Ordering::Relaxed);
  let gain_db = recorder.input_gain_db.clone();
  let buffer = recorder.buffer.clone();
  let overlay_emitter = Arc::new(OverlayLevelEmitter::new(app.clone(), settings.vad_threshold_sustain, settings.vad_threshold_start));
  let device_id = settings.input_device.clone();
  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
  let (vad_tx, vad_rx) = std::sync::mpsc::channel::<VadEvent>();

  let vad_runtime = Arc::new(VadRuntime::new(
    settings.audio_cues,
    settings.vad_threshold_start,
    settings.vad_threshold_sustain,
    settings.vad_silence_ms,
  ));
  let vad_handle = VadHandle {
    runtime: vad_runtime.clone(),
    tx: vad_tx.clone(),
    app: app.clone(),
  };

  let app_handle = app.clone();
  let settings_clone = settings.clone();
  let vad_runtime_clone = vad_runtime.clone();
  thread::spawn(move || {
    for event in vad_rx {
      match event {
        VadEvent::Finalize(samples) => {
          process_vad_segment(app_handle.clone(), settings_clone.clone(), samples, vad_runtime_clone.clone());
        }
      }
    }
  });

  let join_handle = thread::spawn(move || {
    let result = (|| -> Result<(), String> {
      let device = resolve_input_device(&device_id)
        .ok_or_else(|| "No input device available".to_string())?;
      let config = device
        .default_input_config()
        .map_err(|e| e.to_string())?;
      let stream_config: StreamConfig = config.clone().into();

      let overlay = Some(overlay_emitter);
      let vad = Some(vad_handle);
      let gain_db = gain_db.clone();
      let stream = match config.sample_format() {
        SampleFormat::F32 => build_input_stream_f32(&device, &stream_config, buffer, overlay.clone(), vad.clone(), gain_db.clone())?,
        SampleFormat::I16 => build_input_stream_i16(&device, &stream_config, buffer, overlay.clone(), vad.clone(), gain_db.clone())?,
        SampleFormat::U16 => build_input_stream_u16(&device, &stream_config, buffer, overlay.clone(), vad.clone(), gain_db.clone())?,
        _ => return Err("Unsupported sample format".to_string()),
      };

      stream.play().map_err(|e| e.to_string())?;
      let _ = ready_tx.send(Ok(()));

      let _ = stop_rx.recv();
      drop(stream);
      Ok(())
    })();

    if let Err(err) = result {
      let _ = ready_tx.send(Err(err));
    }
  });

  let start_result = match ready_rx.recv_timeout(Duration::from_secs(3)) {
    Ok(Ok(())) => Ok(()),
    Ok(Err(err)) => Err(err),
    Err(_) => Err("Failed to start audio stream".to_string()),
  };

  if let Err(err) = start_result {
    error!("Failed to start VAD monitor: {}", err);
    let _ = stop_tx.send(());
    let _ = join_handle.join();
    return Err(err);
  }

  recorder.stop_tx = Some(stop_tx);
  recorder.join_handle = Some(join_handle);
  recorder.active = true;
  recorder.vad_tx = Some(vad_tx);
  recorder.vad_runtime = Some(vad_runtime);

  let _ = app.emit("capture:state", "idle");
  let _ = update_overlay_state(app, OverlayState::Idle);
  Ok(())
}

fn stop_vad_monitor(app: &AppHandle, state: &State<'_, AppState>) {
  let (stop_tx, join_handle, vad_tx, vad_runtime) = {
    let mut recorder = state.recorder.lock().unwrap();
    if !recorder.active {
      return;
    }
    recorder.active = false;
    (
      recorder.stop_tx.take(),
      recorder.join_handle.take(),
      recorder.vad_tx.take(),
      recorder.vad_runtime.take(),
    )
  };

  if let Some(runtime) = vad_runtime {
    runtime.recording.store(false, Ordering::Relaxed);
    runtime.processing.store(false, Ordering::Relaxed);
    runtime.pending_flush.store(false, Ordering::Relaxed);
  }

  if let Some(tx) = vad_tx {
    drop(tx);
  }

  if let Some(tx) = stop_tx {
    let _ = tx.send(());
  }
  if let Some(join_handle) = join_handle {
    let _ = join_handle.join();
  }

  let _ = app.emit("capture:state", "idle");
  let _ = update_overlay_state(app, OverlayState::Idle);
}

fn process_vad_segment(
  app_handle: AppHandle,
  settings: Settings,
  samples: Vec<i16>,
  runtime: Arc<VadRuntime>,
) {
  let state = app_handle.state::<AppState>();
  if samples.is_empty() {
    runtime.pending_flush.store(false, Ordering::Relaxed);
    return;
  }
  if let Ok(mut recorder) = state.recorder.lock() {
    recorder.transcribing = true;
  }

  let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
  if samples.len() < min_samples {
    let _ = app_handle.emit("capture:state", "idle");
    let _ = update_overlay_state(&app_handle, OverlayState::Idle);
    let _ = app_handle.emit(
      "transcription:error",
      format!(
        "Audio too short ({} ms). Speak a bit longer.",
        (samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64)
      ),
    );
    runtime.processing.store(false, Ordering::Relaxed);
    runtime.pending_flush.store(false, Ordering::Relaxed);
    if let Ok(mut recorder) = state.recorder.lock() {
      recorder.transcribing = false;
    }
    return;
  }

  runtime.processing.store(true, Ordering::Relaxed);
  let _ = app_handle.emit("capture:state", "transcribing");
  let _ = update_overlay_state(&app_handle, OverlayState::Transcribing);

  let result = transcribe_audio(&app_handle, &settings, &samples);
  let level = rms_i16(&samples);
  let duration_ms = samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

  if let Ok(mut recorder) = state.recorder.lock() {
    recorder.transcribing = false;
  }

  runtime.processing.store(false, Ordering::Relaxed);
  runtime.pending_flush.store(false, Ordering::Relaxed);

  if runtime.recording.load(Ordering::Relaxed) {
    let _ = app_handle.emit("capture:state", "recording");
    let _ = update_overlay_state(&app_handle, OverlayState::Recording);
  } else {
    let _ = app_handle.emit("capture:state", "idle");
    let _ = update_overlay_state(&app_handle, OverlayState::Idle);
  }

  if settings.audio_cues {
    let _ = app_handle.emit("audio:cue", "stop");
  }

  match result {
    Ok((text, source)) => {
      if !text.trim().is_empty() && !should_drop_transcript(&text, level, duration_ms) {
        if let Ok(updated) = push_history_entry_inner(&app_handle, &state.history, text.clone(), source.clone()) {
          let _ = app_handle.emit("history:updated", updated);
        }
        let _ = app_handle.emit(
          "transcription:result",
          TranscriptionResult {
            text: text.clone(),
            source: source.clone(),
          },
        );
        if let Err(err) = paste_text(&text) {
          let _ = app_handle.emit("transcription:error", err);
        }
      }
    }
    Err(err) => {
      let _ = app_handle.emit("transcription:error", err);
    }
  }
}

fn stop_recording_async(app: AppHandle, state: &State<'_, AppState>) {
  let app_handle = app.clone();
  let settings = state.settings.lock().unwrap().clone();

  thread::spawn(move || {
    info!("stop_recording_async called");
    let state = app_handle.state::<AppState>();
    let (buffer, stop_tx, join_handle) = {
      let mut recorder = state.recorder.lock().unwrap();
      if !recorder.active {
        info!("Recording not active, skipping stop");
        return;
      }
      recorder.active = false;
      recorder.transcribing = true;
      let stop_tx = recorder.stop_tx.take();
      let join_handle = recorder.join_handle.take();
      (recorder.buffer.clone(), stop_tx, join_handle)
    };

    if let Some(tx) = stop_tx {
      let _ = tx.send(());
    }
    if let Some(join_handle) = join_handle {
      let _ = join_handle.join();
    }

    let samples = {
      let mut buf = buffer.lock().unwrap();
      buf.drain()
    };

    let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
    if samples.len() < min_samples {
      let _ = app_handle.emit("capture:state", "idle");
      let _ = update_overlay_state(&app_handle, OverlayState::Idle);
      let _ = app_handle.emit(
        "transcription:error",
        format!(
          "Audio too short ({} ms). Hold PTT a bit longer.",
          (samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64)
        ),
      );
      let mut recorder = state.recorder.lock().unwrap();
      recorder.transcribing = false;
      return;
    }

    let _ = app_handle.emit("capture:state", "transcribing");
    let _ = update_overlay_state(&app_handle, OverlayState::Transcribing);

    let result = transcribe_audio(&app_handle, &settings, &samples);
    let level = rms_i16(&samples);
    let duration_ms = samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

    let mut recorder = state.recorder.lock().unwrap();
    recorder.transcribing = false;
    drop(recorder);

    let _ = app_handle.emit("capture:state", "idle");
    let _ = update_overlay_state(&app_handle, OverlayState::Idle);

    // Emit audio cue if enabled
    if settings.audio_cues {
      let _ = app_handle.emit("audio:cue", "stop");
    }

    match result {
      Ok((text, source)) => {
        if !text.trim().is_empty() && !should_drop_transcript(&text, level, duration_ms) {
          if let Ok(updated) = push_history_entry_inner(&app_handle, &state.history, text.clone(), source.clone()) {
            let _ = app_handle.emit("history:updated", updated);
          }
          let _ = app_handle.emit(
            "transcription:result",
            TranscriptionResult {
              text: text.clone(),
              source: source.clone(),
            },
          );
          if let Err(err) = paste_text(&text) {
            let _ = app_handle.emit("transcription:error", err);
          }
        }
      }
      Err(err) => {
        let _ = app_handle.emit("transcription:error", err);
      }
    }
  });
}

fn handle_ptt_press(app: &AppHandle) -> Result<(), String> {
  let state = app.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if !settings.capture_enabled {
    return Ok(());
  }
  if settings.mode != "ptt" {
    return Ok(());
  }

  // If PTT+VAD is enabled, start VAD monitor instead of direct recording
  if settings.ptt_use_vad {
    start_vad_monitor(app, &state, &settings)
  } else {
    start_recording_with_settings(app, &state, &settings)
  }
}

fn handle_ptt_release_async(app: AppHandle) {
  let app_handle = app.clone();
  let state = app_handle.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if settings.mode != "ptt" {
    return;
  }

  // If PTT+VAD was enabled, stop VAD monitor (which also stops any active recording)
  if settings.ptt_use_vad {
    stop_vad_monitor(&app, &state);
  } else {
    stop_recording_async(app, &state);
  }
}

fn handle_toggle_async(app: AppHandle) {
  let app_handle = app.clone();
  let state = app_handle.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if !settings.capture_enabled {
    return;
  }
  if settings.mode != "ptt" {
    return;
  }

  let active = state.recorder.lock().unwrap().active;
  if active {
    stop_recording_async(app, &state);
  } else {
    let _ = start_recording_with_settings(&app, &state, &settings);
  }
}

fn start_transcribe_monitor(
  app: &AppHandle,
  state: &State<'_, AppState>,
  settings: &Settings,
) -> Result<(), String> {
  let mut recorder = state.transcribe.lock().unwrap();
  if recorder.active {
    return Ok(());
  }

  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let app_handle = app.clone();
  let settings = settings.clone();

  let join_handle = thread::spawn(move || {
    #[cfg(target_os = "windows")]
    {
      if let Err(err) = run_transcribe_loopback(app_handle.clone(), settings, stop_rx) {
        emit_error(&app_handle, AppError::AudioDevice(err), Some("System Audio"));
        let state = app_handle.state::<AppState>();
        state.transcribe_active.store(false, Ordering::Relaxed);
        if let Ok(mut transcribe) = state.transcribe.lock() {
          transcribe.active = false;
          transcribe.stop_tx = None;
          transcribe.join_handle = None;
        }
        let _ = app_handle.emit("transcribe:state", "idle");
      }
      return;
    }

    #[cfg(not(target_os = "windows"))]
    {
      let _ = stop_rx.recv();
      emit_error(&app_handle, AppError::AudioDevice("System audio capture is not supported on this OS yet.".to_string()), Some("System Audio"));
      let state = app_handle.state::<AppState>();
      state.transcribe_active.store(false, Ordering::Relaxed);
      if let Ok(mut transcribe) = state.transcribe.lock() {
        transcribe.active = false;
        transcribe.stop_tx = None;
        transcribe.join_handle = None;
      }
      let _ = app_handle.emit("transcribe:state", "idle");
    }
  });

  recorder.active = true;
  recorder.stop_tx = Some(stop_tx);
  recorder.join_handle = Some(join_handle);
  state.transcribe_active.store(true, Ordering::Relaxed);

  let _ = app.emit("transcribe:state", "recording");
  Ok(())
}

fn stop_transcribe_monitor(app: &AppHandle, state: &State<'_, AppState>) {
  let (stop_tx, join_handle) = {
    let mut recorder = state.transcribe.lock().unwrap();
    recorder.active = false;
    (recorder.stop_tx.take(), recorder.join_handle.take())
  };

  state.transcribe_active.store(false, Ordering::Relaxed);
  let _ = app.emit("transcribe:state", "idle");

  if let Some(tx) = stop_tx {
    let _ = tx.send(());
  }
  if let Some(handle) = join_handle {
    thread::spawn(move || {
      let _ = handle.join();
    });
  }
}

fn toggle_transcribe_state(app: &AppHandle) {
  let state = app.state::<AppState>();
  let settings = state.settings.lock().unwrap().clone();
  if !settings.transcribe_enabled {
    let _ = app.emit("transcribe:state", "idle");
    return;
  }
  let active = state.transcribe_active.load(Ordering::Relaxed);
  if active {
    stop_transcribe_monitor(app, &state);
  } else {
    if let Err(err) = start_transcribe_monitor(app, &state, &settings) {
      emit_error(app, AppError::AudioDevice(err), Some("System Audio"));
      state.transcribe_active.store(false, Ordering::Relaxed);
      let _ = app.emit("transcribe:state", "idle");
    }
  }
}

fn rms_f32(samples: &[f32]) -> f32 {
  if samples.is_empty() {
    return 0.0;
  }
  let mut sum = 0.0f32;
  for &sample in samples {
    sum += sample * sample;
  }
  (sum / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

fn rms_i16(samples: &[i16]) -> f32 {
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

fn transcribe_worker(
  app: AppHandle,
  settings: Settings,
  rx: std::sync::mpsc::Receiver<Vec<i16>>,
) {
  let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
  for chunk in rx {
    if chunk.len() < min_samples {
      continue;
    }
    let level = rms_i16(&chunk);
    let duration_ms = chunk.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

    if settings.transcribe_vad_mode {
      if level < settings.transcribe_vad_threshold {
        continue;
      }
    }

    let _ = app.emit("transcribe:state", "transcribing");
    let result = transcribe_audio(&app, &settings, &chunk);

    if app.state::<AppState>().transcribe_active.load(Ordering::Relaxed) {
      let _ = app.emit("transcribe:state", "recording");
    }

    match result {
      Ok((text, _source)) => {
        if !text.trim().is_empty() && !should_drop_transcript(&text, level, duration_ms) {
          let state = app.state::<AppState>();
          let _ = push_transcribe_entry_inner(&app, &state.history_transcribe, text);
        }
      }
      Err(err) => {
        let _ = app.emit("transcription:error", err);
      }
    }
  }
}

#[cfg(target_os = "windows")]
fn decode_wasapi_mono(
  raw: &[u8],
  channels: usize,
  bytes_per_sample: usize,
  sample_format: wasapi::SampleType,
) -> Vec<f32> {
  if channels == 0 || bytes_per_sample == 0 {
    return Vec::new();
  }

  let bytes_per_frame = channels * bytes_per_sample;
  let mut mono = Vec::with_capacity(raw.len() / bytes_per_frame);

  match sample_format {
    wasapi::SampleType::Float => {
      if bytes_per_sample != 4 {
        return Vec::new();
      }
      for frame in raw.chunks_exact(bytes_per_frame) {
        let mut sum = 0.0f32;
        for ch in 0..channels {
          let start = ch * bytes_per_sample;
          let sample = f32::from_le_bytes([
            frame[start],
            frame[start + 1],
            frame[start + 2],
            frame[start + 3],
          ]);
          sum += sample;
        }
        mono.push(sum / channels as f32);
      }
    }
    wasapi::SampleType::Int => {
      for frame in raw.chunks_exact(bytes_per_frame) {
        let mut sum = 0.0f32;
        for ch in 0..channels {
          let start = ch * bytes_per_sample;
          let sample = match bytes_per_sample {
            2 => {
              let value = i16::from_le_bytes([frame[start], frame[start + 1]]) as f32;
              value / i16::MAX as f32
            }
            3 => {
              let b0 = frame[start] as i32;
              let b1 = (frame[start + 1] as i32) << 8;
              let b2 = (frame[start + 2] as i32) << 16;
              let mut value = b0 | b1 | b2;
              if value & 0x800000 != 0 {
                value |= !0xFFFFFF;
              }
              value as f32 / 8_388_608.0
            }
            4 => {
              let value = i32::from_le_bytes([
                frame[start],
                frame[start + 1],
                frame[start + 2],
                frame[start + 3],
              ]) as f32;
              value / i32::MAX as f32
            }
            _ => 0.0,
          };
          sum += sample;
        }
        mono.push(sum / channels as f32);
      }
    }
  }

  mono
}

#[cfg(target_os = "windows")]
fn run_transcribe_loopback(
  app: AppHandle,
  settings: Settings,
  stop_rx: std::sync::mpsc::Receiver<()>,
) -> Result<(), String> {
  let hr = wasapi::initialize_mta();
  if hr.0 < 0 {
    return Err(format!("COM initialization failed: HRESULT={}", hr.0));
  }

  let device = resolve_output_device(&settings.transcribe_output_device)
    .ok_or_else(|| "No output device available".to_string())?;
  let mut audio_client = device.get_iaudioclient().map_err(|e| e.to_string())?;
  let mix_format = audio_client.get_mixformat().map_err(|e| e.to_string())?;

  let stream_mode = wasapi::StreamMode::PollingShared {
    autoconvert: true,
    buffer_duration_hns: 200_000,
  };
  audio_client
    .initialize_client(&mix_format, &wasapi::Direction::Capture, &stream_mode)
    .map_err(|e| e.to_string())?;

  let capture_client = audio_client.get_audiocaptureclient().map_err(|e| e.to_string())?;
  audio_client.start_stream().map_err(|e| e.to_string())?;

  let channels = mix_format.get_nchannels() as usize;
  let sample_rate = mix_format.get_samplespersec();
  let bytes_per_frame = mix_format.get_blockalign() as usize;
  let bytes_per_sample = if channels > 0 {
    bytes_per_frame / channels
  } else {
    0
  };
  let sample_format = mix_format
    .get_subformat()
    .unwrap_or(wasapi::SampleType::Int);

  let chunk_samples =
    (TARGET_SAMPLE_RATE as u64 * settings.transcribe_batch_interval_ms / 1000) as usize;
  let overlap_samples =
    (TARGET_SAMPLE_RATE as u64 * settings.transcribe_chunk_overlap_ms / 1000) as usize;
  let mut gain = (10.0f32).powf(settings.transcribe_input_gain_db / 20.0);
  let mut vad_enabled = settings.transcribe_vad_mode;
  let mut vad_threshold = settings.transcribe_vad_threshold;
  let mut vad_silence_ms = settings.transcribe_vad_silence_ms;
  let mut last_settings_check = Instant::now();
  let mut vad_last_hit_ms = Instant::now();

  let (chunk_tx, chunk_rx) = std::sync::mpsc::channel::<Vec<i16>>();
  let worker_app = app.clone();
  let worker_settings = settings.clone();
  let worker_handle = thread::spawn(move || {
    transcribe_worker(worker_app, worker_settings, chunk_rx);
  });

  let mut buffer = CaptureBuffer::default();
  let mut smooth_level = 0.0f32;
  let mut last_emit = Instant::now();

  loop {
    match stop_rx.try_recv() {
      Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
      Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }

    let packet_frames = capture_client
      .get_next_packet_size()
      .map_err(|e| e.to_string())?;
    let packet_frames = match packet_frames {
      Some(value) => value,
      None => {
        thread::sleep(Duration::from_millis(10));
        continue;
      }
    };
    if packet_frames == 0 {
      thread::sleep(Duration::from_millis(10));
      continue;
    }

    let mut raw = vec![0u8; packet_frames as usize * bytes_per_frame];
    let (frames_read, _) = capture_client
      .read_from_device(&mut raw)
      .map_err(|e| e.to_string())?;
    if frames_read == 0 {
      continue;
    }

    let valid_bytes = frames_read as usize * bytes_per_frame;
    if last_settings_check.elapsed() >= Duration::from_millis(200) {
      if let Ok(current) = app.state::<AppState>().settings.lock() {
        gain = (10.0f32).powf(current.transcribe_input_gain_db / 20.0);
        vad_enabled = current.transcribe_vad_mode;
        vad_threshold = current.transcribe_vad_threshold;
        vad_silence_ms = current.transcribe_vad_silence_ms;
      }
      last_settings_check = Instant::now();
    }

    let mut mono = decode_wasapi_mono(
      &raw[..valid_bytes],
      channels,
      bytes_per_sample,
      sample_format,
    );
    if mono.is_empty() {
      continue;
    }

    if gain != 1.0 {
      for sample in mono.iter_mut() {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
      }
    }

    let rms = rms_f32(&mono);
    if vad_enabled && rms >= vad_threshold {
      vad_last_hit_ms = Instant::now();
    }
    smooth_level = smooth_level * 0.8 + rms * 0.2;
    if last_emit.elapsed() >= Duration::from_millis(50) {
      let db = if smooth_level <= 0.000_01 {
        -60.0
      } else {
        (20.0 * smooth_level.log10()).max(-60.0).min(0.0)
      };
      let meter = (db + 60.0) / 60.0;
      let _ = app.emit("transcribe:level", meter.clamp(0.0, 1.0));
      let _ = app.emit("transcribe:db", db);
      last_emit = Instant::now();
    }

    buffer.push_samples(&mono, sample_rate);
    while buffer.len() >= chunk_samples {
      let chunk = buffer.take_chunk(chunk_samples, overlap_samples);
      if chunk.is_empty() {
        break;
      }
      if vad_enabled {
        if vad_last_hit_ms.elapsed() <= Duration::from_millis(vad_silence_ms) {
          let _ = chunk_tx.send(chunk);
        }
      } else {
        let _ = chunk_tx.send(chunk);
      }
    }
  }

  let leftover = buffer.drain();
  if !leftover.is_empty() {
    let _ = chunk_tx.send(leftover);
  }

  drop(chunk_tx);
  let _ = worker_handle.join();
  let _ = audio_client.stop_stream();
  Ok(())
}

fn push_history_entry(
  app: &AppHandle,
  state: &State<'_, AppState>,
  text: String,
  source: String,
) -> Result<Vec<HistoryEntry>, String> {
  push_history_entry_inner(app, &state.history, text, source)
}

fn push_history_entry_inner(
  app: &AppHandle,
  history: &Mutex<Vec<HistoryEntry>>,
  text: String,
  source: String,
) -> Result<Vec<HistoryEntry>, String> {
  let mut history = history.lock().unwrap();
  let entry = HistoryEntry {
    id: format!("h_{}", now_ms()),
    text,
    timestamp_ms: now_ms(),
    source,
  };
  history.insert(0, entry);
  save_history_file(app, &history)?;
  Ok(history.clone())
}

fn push_transcribe_entry_inner(
  app: &AppHandle,
  history: &Mutex<Vec<HistoryEntry>>,
  text: String,
) -> Result<Vec<HistoryEntry>, String> {
  let mut history = history.lock().unwrap();
  let entry = HistoryEntry {
    id: format!("o_{}", now_ms()),
    text,
    timestamp_ms: now_ms(),
    source: "output".to_string(),
  };
  history.insert(0, entry);
  save_transcribe_history_file(app, &history)?;
  let _ = app.emit("transcribe:history-updated", history.clone());
  Ok(history.clone())
}

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

fn download_model_file(
  app: &AppHandle,
  model_id: &str,
  download_url: &str,
  file_name: &str,
) -> Result<PathBuf, String> {
  validate_model_file_name(file_name)?;
  let models_dir = resolve_models_dir(app);
  let dest_path = models_dir.join(file_name);
  if dest_path.exists() {
    return Ok(dest_path);
  }

  let tmp_path = dest_path.with_extension("part");
  let result = (|| -> Result<PathBuf, String> {
    let response = http_get_with_redirects(download_url).map_err(|e| e.to_string())?;
    let total = response
      .header("Content-Length")
      .and_then(|value| value.parse::<u64>().ok());

    // Security: Enforce maximum model size to prevent disk exhaustion
    if let Some(size) = total {
      if size > MAX_MODEL_SIZE_BYTES {
        return Err(format!(
          "Model too large: {} MB (max {} MB)",
          size / 1024 / 1024,
          MAX_MODEL_SIZE_BYTES / 1024 / 1024
        ));
      }
    }

    let mut reader = response.into_reader();
    let mut file = fs::File::create(&tmp_path).map_err(|e| e.to_string())?;

    let mut downloaded = 0u64;
    let mut last_emit = Instant::now();
    let mut last_read = Instant::now(); // Track for timeout detection
    let mut buffer = [0u8; 64 * 1024];

    loop {
      // Timeout detection: fail if no data for DOWNLOAD_TIMEOUT_SECS
      if last_read.elapsed().as_secs() > DOWNLOAD_TIMEOUT_SECS {
        return Err(format!(
          "Download stalled: no data received for {} seconds",
          DOWNLOAD_TIMEOUT_SECS
        ));
      }

      let read_bytes = reader.read(&mut buffer).map_err(|e| e.to_string())?;
      if read_bytes == 0 {
        break;
      }

      last_read = Instant::now(); // Reset timeout on successful read

      file
        .write_all(&buffer[..read_bytes])
        .map_err(|e| e.to_string())?;
      downloaded += read_bytes as u64;
      if downloaded > MAX_MODEL_SIZE_BYTES {
        return Err(format!(
          "Model too large: exceeded {} MB limit",
          MAX_MODEL_SIZE_BYTES / 1024 / 1024
        ));
      }

      if last_emit.elapsed() >= Duration::from_millis(250) {
        let _ = app.emit(
          "model:download-progress",
          DownloadProgress {
            id: model_id.to_string(),
            downloaded,
            total,
          },
        );
        last_emit = Instant::now();
      }
    }

    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    // Optional: Verify model checksum if available (before renaming into place)
    // This prevents man-in-the-middle attacks and file corruption
    if let Some(expected_hash) = lookup_model_checksum(file_name) {
      verify_model_checksum(&tmp_path, expected_hash)?;
      info!("Model integrity verified for {}", file_name);
    } else {
      warn!("No checksum available for {}: skipping integrity check", file_name);
    }

    fs::rename(&tmp_path, &dest_path).map_err(|e| e.to_string())?;

    let _ = app.emit(
      "model:download-progress",
      DownloadProgress {
        id: model_id.to_string(),
        downloaded,
        total,
      },
    );

    Ok(dest_path)
  })();

  if result.is_err() {
    let _ = fs::remove_file(&tmp_path);
  }

  result
}

fn transcribe_audio(
  app: &AppHandle,
  settings: &Settings,
  samples: &[i16],
) -> Result<(String, String), String> {
  let wav_bytes = encode_wav_i16(samples, TARGET_SAMPLE_RATE);

  if settings.cloud_fallback {
    let text = transcribe_cloud(&wav_bytes)?;
    return Ok((text, "cloud".to_string()));
  }

  let text = transcribe_local(app, settings, &wav_bytes)?;
  Ok((text, "local".to_string()))
}

fn transcribe_local(app: &AppHandle, settings: &Settings, wav_bytes: &[u8]) -> Result<String, String> {
  let temp_dir = std::env::temp_dir();
  let stamp = now_ms();
  let base = temp_dir.join(format!("trispr_{}", stamp));
  let wav_path = base.with_extension("wav");
  let output_base = base.clone();

  fs::write(&wav_path, wav_bytes).map_err(|e| e.to_string())?;

  let model_path = resolve_model_path(app, &settings.model)
    .ok_or_else(|| "Model file not found. Set TRISPR_WHISPER_MODEL_DIR or TRISPR_WHISPER_MODEL.".to_string())?;

  let cli_path = resolve_whisper_cli_path();

  let mut command = if let Some(path) = cli_path {
    Command::new(path)
  } else {
    Command::new("whisper-cli")
  };

  let threads = std::thread::available_parallelism()
    .map(|n| n.get().to_string())
    .unwrap_or_else(|_| "4".to_string());

  command
    .arg("-m")
    .arg(model_path)
    .arg("-f")
    .arg(&wav_path)
    .arg("-t")
    .arg(threads)
    .arg("-l")
    .arg("auto")
    .arg("-nt")
    .arg("-otxt")
    .arg("-of")
    .arg(&output_base)
    .arg("-np")
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

  let output = command.output().map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(format!("whisper-cli failed: {}", stderr));
  }

  let txt_path = output_base.with_extension("txt");
  let text = fs::read_to_string(&txt_path).map_err(|e| e.to_string())?;

  let _ = fs::remove_file(&wav_path);
  let _ = fs::remove_file(&txt_path);

  Ok(text.trim().to_string())
}

#[derive(Deserialize)]
struct CloudResponse {
  text: String,
}

fn transcribe_cloud(wav_bytes: &[u8]) -> Result<String, String> {
  let endpoint = std::env::var("TRISPR_CLOUD_ENDPOINT").unwrap_or_default();
  if endpoint.trim().is_empty() {
    return Err("Cloud fallback not configured".to_string());
  }

  let token = std::env::var("TRISPR_CLOUD_TOKEN").unwrap_or_default();
  let mut req = ureq::post(&endpoint).set("Content-Type", "audio/wav");
  if !token.trim().is_empty() {
    req = req.set("Authorization", &format!("Bearer {}", token));
  }

  let resp = req.send_bytes(wav_bytes).map_err(|e| e.to_string())?;
  let parsed: CloudResponse = resp.into_json().map_err(|e| e.to_string())?;
  Ok(parsed.text)
}

fn resolve_whisper_cli_path() -> Option<PathBuf> {
  if let Ok(path) = std::env::var("TRISPR_WHISPER_CLI") {
    let candidate = PathBuf::from(path);
    if candidate.exists() {
      return Some(candidate);
    }
  }

  let mut candidates = Vec::new();
  if let Ok(cwd) = std::env::current_dir() {
    candidates.push(cwd.join("whisper-cli"));
    candidates.push(cwd.join("whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cpu/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/whisper-cli"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/whisper-cli.exe"));
    candidates.push(cwd.join("../whisper.cpp/build-cuda/bin/Release/whisper-cli.exe"));
    candidates.push(cwd.join("../../whisper.cpp/build/bin/whisper-cli"));
  }

  for path in candidates {
    if path.exists() {
      return Some(path);
    }
  }

  None
}

/// Initialize logging with tracing
fn init_logging() {
  use tracing_subscriber::{fmt, EnvFilter};

  // Try to create log file in app data directory
  // For now, just log to stdout in development
  let filter = EnvFilter::try_from_default_env()
    .unwrap_or_else(|_| EnvFilter::new("info"));

  fmt()
    .with_env_filter(filter)
    .with_target(false)
    .with_thread_ids(false)
    .with_file(true)
    .with_line_number(true)
    .init();

  info!("Trispr Flow starting up");
}

/// Emit an error event to the frontend
fn emit_error(app: &AppHandle, error: AppError, context: Option<&str>) {
  let event = if let Some(ctx) = context {
    ErrorEvent::new(error.clone()).with_context(ctx)
  } else {
    ErrorEvent::new(error.clone())
  };

  error!("{}: {}", error.title(), error.message());

  let _ = app.emit("app:error", event);
}

fn load_local_env() {
  let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
  let parent = cwd.parent().map(|p| p.to_path_buf());
  let grandparent = parent.as_ref().and_then(|p| p.parent().map(|gp| gp.to_path_buf()));
  let mut candidates = vec![cwd.join(".env.local"), cwd.join(".env")];
  if let Some(parent) = parent {
    candidates.push(parent.join(".env.local"));
    candidates.push(parent.join(".env"));
  }
  if let Some(grandparent) = grandparent {
    candidates.push(grandparent.join(".env.local"));
    candidates.push(grandparent.join(".env"));
  }

  for path in candidates {
    if !path.exists() {
      continue;
    }
    if let Ok(raw) = fs::read_to_string(&path) {
      for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
          continue;
        }
        let mut parts = line.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if key.is_empty() || value.is_empty() {
          continue;
        }
        if std::env::var(key).is_err() {
          std::env::set_var(key, value);
        }
      }
    }
  }
}

fn paste_text(text: &str) -> Result<(), String> {
  let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
  let previous = clipboard.get_text().ok();
  clipboard
    .set_text(text.to_string())
    .map_err(|e| e.to_string())?;

  send_paste_keystroke()?;

  if let Some(previous) = previous {
    thread::spawn(move || {
      thread::sleep(Duration::from_millis(150));
      if let Ok(mut clipboard) = Clipboard::new() {
        let _ = clipboard.set_text(previous);
      }
    });
  }

  Ok(())
}

fn send_paste_keystroke() -> Result<(), String> {
  let mut enigo = Enigo::new();
  if cfg!(target_os = "macos") {
    enigo.key_down(Key::Meta);
    enigo.key_click(Key::Layout('v'));
    enigo.key_up(Key::Meta);
  } else {
    enigo.key_down(Key::Control);
    enigo.key_click(Key::Layout('v'));
    enigo.key_up(Key::Control);
  }
  Ok(())
}

fn try_load_tray_icon(icon_path: &std::path::Path) -> Option<tauri::image::Image<'static>> {
  use tauri::image::Image;

  match std::fs::read(icon_path) {
    Ok(png_data) => {
      match image::load_from_memory_with_format(&png_data, image::ImageFormat::Png) {
        Ok(img) => {
          let rgba_img = img.to_rgba8();
          let (width, height) = rgba_img.dimensions();
          return Some(Image::new_owned(rgba_img.into_raw(), width, height));
        }
        Err(_) => {}
      }
    }
    Err(_) => {}
  }
  None
}

fn create_fallback_icon() -> tauri::image::Image<'static> {
  use tauri::image::Image;

  let mut pixels = vec![0u8; 64 * 64 * 4];
  for i in (0..pixels.len()).step_by(4) {
    pixels[i] = 40; // R
    pixels[i + 1] = 130; // G
    pixels[i + 2] = 140; // B
    pixels[i + 3] = 255; // A
  }

  Image::new_owned(pixels, 64, 64)
}

fn show_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window("main") {
    let _ = window.show();
    let _ = window.set_skip_taskbar(false);
    let _ = window.set_focus();
  }
}

fn hide_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window("main") {
    let _ = window.hide();
    let _ = window.set_skip_taskbar(true);
  }
}

fn should_handle_tray_click() -> bool {
  let now = now_ms();
  let last = LAST_TRAY_CLICK_MS.load(Ordering::Relaxed);
  if now.saturating_sub(last) <= TRAY_CLICK_DEBOUNCE_MS {
    return false;
  }
  LAST_TRAY_CLICK_MS.store(now, Ordering::Relaxed);
  true
}

fn toggle_main_window(app: &AppHandle) {
  if let Some(window) = app.get_webview_window("main") {
    let visible = window.is_visible().unwrap_or(true);
    if visible {
      hide_main_window(app);
    } else {
      show_main_window(app);
    }
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  init_logging();
  load_local_env();

  info!("Starting Trispr Flow application");
  tauri::Builder::default()
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())
    .setup(|app| {
      let mut settings = load_settings(app.handle());
      let history = load_history(app.handle());
      let history_transcribe = load_transcribe_history(app.handle());

      app.manage(AppState {
        settings: Mutex::new(settings.clone()),
        history: Mutex::new(history),
        history_transcribe: Mutex::new(history_transcribe),
        recorder: Mutex::new(Recorder::new()),
        transcribe: Mutex::new(TranscribeRecorder::new()),
        downloads: Mutex::new(HashSet::new()),
        transcribe_active: AtomicBool::new(false),
      });

      let _ = app.emit("transcribe:state", "idle");

      if let Err(err) = register_hotkeys(app.handle(), &settings) {
        eprintln!("⚠ Failed to register hotkeys: {}", err);
      }

      if settings.mode == "vad" && settings.capture_enabled {
        if let Err(err) = start_vad_monitor(app.handle(), &app.state::<AppState>(), &settings) {
          eprintln!("⚠ Failed to start VAD monitor: {}", err);
        }
      }

      let overlay_app = app.handle().clone();
      app.listen("overlay:ready", move |_| {
        overlay::mark_overlay_ready();
        let settings = overlay_app.state::<AppState>().settings.lock().unwrap().clone();
        let _ = overlay::apply_overlay_settings(&overlay_app, &build_overlay_settings(&settings));
      });
      // Create overlay window at startup
      if let Err(err) = overlay::create_overlay_window(&app.handle()) {
        eprintln!("⚠ Failed to create overlay window: {}", err);
      }
      let overlay_settings = build_overlay_settings(&settings);
      if let Some((pos_x, pos_y)) = overlay::resolve_overlay_position_for_settings(&app.handle(), &overlay_settings) {
        if overlay_settings.style == "kitt" {
          settings.overlay_kitt_pos_x = pos_x;
          settings.overlay_kitt_pos_y = pos_y;
        } else {
          settings.overlay_pos_x = pos_x;
          settings.overlay_pos_y = pos_y;
        }
        if let Ok(mut current) = app.state::<AppState>().settings.lock() {
          *current = settings.clone();
        }
        let _ = save_settings_file(app.handle(), &settings);
      }
      let _ = overlay::apply_overlay_settings(&app.handle(), &build_overlay_settings(&settings));
      let _ = overlay::update_overlay_state(&app.handle(), overlay::OverlayState::Idle);

      let icon = {
        let paths = [
          std::path::PathBuf::from("icons/icon.png"),
          std::path::PathBuf::from("src-tauri/icons/icon.png"),
          std::path::PathBuf::from("../icons/icon.png"),
          std::path::PathBuf::from("./icons/icon.png"),
        ];

        let mut loaded_icon = None;
        for path in &paths {
          if let Some(icon) = try_load_tray_icon(path) {
            loaded_icon = Some(icon);
            break;
          }
        }

        loaded_icon.unwrap_or_else(create_fallback_icon)
      };

      let _tray_icon = tauri::tray::TrayIconBuilder::new()
        .icon(icon)
        .tooltip("Trispr Flow")
        .on_tray_icon_event(|tray, event| {
          use tauri::tray::{MouseButton, TrayIconEvent};
          match event {
            TrayIconEvent::Click { button: MouseButton::Left, .. } => {
              if should_handle_tray_click() {
                toggle_main_window(tray.app_handle());
              }
            }
            _ => {}
          }
        })
        .on_menu_event(|app, event| {
          match event.id.as_ref() {
            "show" => {
              show_main_window(app);
            }
            "toggle-mic" => {
              let state = app.state::<AppState>();
              let mut current = state.settings.lock().unwrap().clone();
              current.capture_enabled = !current.capture_enabled;
              if let Err(err) = save_settings(app.clone(), state, current) {
                emit_error(app, AppError::Storage(err), Some("Tray menu"));
              }
            }
            "toggle-transcribe" => {
              let state = app.state::<AppState>();
              let mut current = state.settings.lock().unwrap().clone();
              current.transcribe_enabled = !current.transcribe_enabled;
              if let Err(err) = save_settings(app.clone(), state, current) {
                emit_error(app, AppError::Storage(err), Some("Tray menu"));
              }
            }
            "quit" => {
              app.exit(0);
            }
            _ => {}
          }
        })
        .menu({
          let mic_item = CheckMenuItem::with_id(
            app,
            "toggle-mic",
            "Microphone tracking",
            true,
            settings.capture_enabled,
            None::<&str>,
          )?;

          let mic_item_clone = mic_item.clone();
          app.listen("menu:update-mic", move |event| {
            if let Ok(checked) = serde_json::from_str::<bool>(event.payload()) {
              let _ = mic_item_clone.set_checked(checked);
            }
          });

          let transcribe_item = CheckMenuItem::with_id(
            app,
            "toggle-transcribe",
            "System audio transcription",
            true,
            settings.transcribe_enabled,
            None::<&str>,
          )?;

          let transcribe_item_clone = transcribe_item.clone();
          app.listen("menu:update-transcribe", move |event| {
            if let Ok(checked) = serde_json::from_str::<bool>(event.payload()) {
              let _ = transcribe_item_clone.set_checked(checked);
            }
          });

          &tauri::menu::Menu::with_items(
            app,
            &[
              &tauri::menu::MenuItem::with_id(app, "show", "Open Trispr Flow", true, None::<&str>)?,
              &tauri::menu::PredefinedMenuItem::separator(app)?,
              &mic_item,
              &transcribe_item,
              &tauri::menu::PredefinedMenuItem::separator(app)?,
              &tauri::menu::MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?,
            ],
          )?
        })
        .show_menu_on_left_click(false)
        .build(app);

      Ok(())
    })
    .on_window_event(|window, event| {
      if window.label() != "main" {
        return;
      }

      if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        hide_main_window(window.app_handle());
      }
    })
    .invoke_handler(tauri::generate_handler![
      get_settings,
      save_settings,
      list_audio_devices,
      list_output_devices,
      list_models,
      download_model,
      remove_model,
      pick_model_dir,
      get_models_dir,
      get_history,
      get_transcribe_history,
      add_history_entry,
      add_transcribe_entry,
      start_recording,
      stop_recording,
      toggle_transcribe,
      open_conversation_window,
      validate_hotkey,
      test_hotkey,
      get_hotkey_conflicts,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
