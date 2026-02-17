use crate::paths::{resolve_models_dir, resolve_quantize_path};
use crate::state::{save_settings_file, AppState};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, info, warn};
use url::Url;

const DEFAULT_MODEL_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";
const MAX_MODEL_SIZE_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5 GB
const DOWNLOAD_TIMEOUT_SECS: u64 = 30; // Timeout for stalled downloads
const DOWNLOAD_CONNECT_TIMEOUT_SECS: u64 = 10;
const DOWNLOAD_READ_TIMEOUT_SECS: u64 = 30;
const DOWNLOAD_REDIRECT_LIMIT: u32 = 5;

/// URL validation levels for model downloads
///
/// Security model:
/// - Enforce HTTPS, no userinfo, no localhost/private IPs.
/// - DNS validation for Strict/Redirect modes.
/// - No domain whitelist (URLs are only sourced from curated model lists).
///
/// This prevents:
/// - SSRF attacks (localhost, private IPs blocked in all modes)
///
/// While allowing:
/// - Legitimate CDN redirects (common with HuggingFace, ggerganov.com, etc.)
/// - Future-proof operation (no whitelist maintenance)
#[derive(Clone, Copy, PartialEq, Eq)]
enum UrlSafety {
  Basic,      // Basic validation only (HTTPS, no userinfo, no localhost, no DNS)
  Strict,     // Full validation (Basic + DNS resolution)
  Redirect,   // Validation for HTTP redirects (Basic + DNS resolution)
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

  if let Ok(ip) = host.parse::<IpAddr>() {
    if !is_public_ip(ip) {
      return Err("IP address is not public".to_string());
    }
  } else if mode == UrlSafety::Strict || mode == UrlSafety::Redirect {
    let port = parsed.port_or_known_default().unwrap_or(443);
    let mut resolved = false;
    let addrs = (host.as_str(), port)
      .to_socket_addrs()
      .map_err(|e| format!("DNS lookup failed for {host}: {e}"))?;
    for addr in addrs {
      resolved = true;
      if !is_public_ip(addr.ip()) {
        return Err(format!("Resolved IP {} for {} is not public", addr.ip(), host));
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
    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
  {
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
  let mut is_first = true;

  for _ in 0..=DOWNLOAD_REDIRECT_LIMIT {
    // Use Strict validation for initial URL, Redirect validation for subsequent redirects
    let safety_mode = if is_first { UrlSafety::Strict } else { UrlSafety::Redirect };
    let parsed = validate_model_url(&current, safety_mode)?;
    is_first = false;
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
    "Too many redirects (>{DOWNLOAD_REDIRECT_LIMIT}) while downloading model"
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

const EXTRA_MODEL_FILES: &[(&str, &str)] = &[
  ("whisper-large-v3-turbo-german", "ggml-large-v3-turbo-german.bin"),
];

// Model integrity verification (SHA256 checksums)
// These checksums ensure downloaded models haven't been tampered with
// Format: (file_name, sha256_hex)
const MODEL_CHECKSUMS: &[(&str, &str)] = &[
  (
    "ggml-large-v3.bin",
    "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
  ),
  (
    "ggml-large-v3-turbo.bin",
    "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
  ),
  (
    "ggml-distil-large-v3.bin",
    "2883a11b90fb10ed592d826edeaee7d2929bf1ab985109fe9e1e7b4d2b69a298",
  ),
];

fn verify_model_checksum(path: &std::path::Path, expected_hash: &str) -> Result<(), String> {
  let mut file = fs::File::open(path)
    .map_err(|e| format!("Failed to open model file for checksum verification: {e}"))?;

  let mut hasher = Sha256::new();
  let mut buffer = [0u8; 8192];

  loop {
    let n = file
      .read(&mut buffer)
      .map_err(|e| format!("Failed to read model file for checksum: {e}"))?;
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
    Err("Model integrity check failed: checksum mismatch (possible corruption or tampering)"
      .to_string())
  }
}

fn lookup_model_checksum(file_name: &str) -> Option<&'static str> {
  MODEL_CHECKSUMS
    .iter()
    .find(|(name, _)| name.eq_ignore_ascii_case(file_name))
    .map(|(_, hash)| *hash)
}

fn model_spec(model_id: &str) -> Option<&'static ModelSpec> {
  MODEL_SPECS.iter().find(|spec| spec.id == model_id)
}

fn extra_model_file(model_id: &str) -> Option<&'static str> {
  EXTRA_MODEL_FILES
    .iter()
    .find(|(id, _)| *id == model_id)
    .map(|(_, file)| *file)
}

fn model_candidates(spec: &ModelSpec) -> Vec<String> {
  vec![spec.file_name.to_string()]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn validate_model_url_accepts_https() {
    let url = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin";
    assert!(validate_model_url(url, UrlSafety::Basic).is_ok());
  }

  #[test]
  fn validate_model_url_rejects_http() {
    let url = "http://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin";
    assert!(validate_model_url(url, UrlSafety::Basic).is_err());
  }

  #[test]
  fn validate_model_url_rejects_userinfo() {
    let url = "https://user:pass@huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin";
    assert!(validate_model_url(url, UrlSafety::Basic).is_err());
  }

  #[test]
  fn validate_model_url_rejects_non_standard_port() {
    let url = "https://huggingface.co:444/ggml-large-v3.bin";
    assert!(validate_model_url(url, UrlSafety::Basic).is_err());
  }

  #[test]
  fn validate_model_url_redirect_mode_allows_cdn_domains() {
    let cdn_url = "https://cas-bridge.xethub.hf.co/some/path/model.bin";
    // This should either succeed or fail due to DNS or IP checks.
    let result = validate_model_url(cdn_url, UrlSafety::Redirect);
    if let Err(e) = result {
      assert!(
        !e.contains("not in whitelist"),
        "Whitelist checks should not be enforced"
      );
    }
  }

  #[test]
  fn validate_model_file_names() {
    assert!(validate_model_file_name("ggml-large-v3.bin").is_ok());
    assert!(validate_model_file_name("ggml-large-v3.gguf").is_ok());
    assert!(validate_model_file_name("../ggml-large-v3.bin").is_err());
    assert!(validate_model_file_name("ggml large.bin").is_err());
    assert!(validate_model_file_name("ggml-large-v3.exe").is_err());
  }
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

pub(crate) fn resolve_model_path(app: &AppHandle, model_id: &str) -> Option<PathBuf> {
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
    let mut candidates = Vec::new();
    if let Some(extra) = extra_model_file(model_id) {
      candidates.push(extra.to_string());
    }
    candidates.push(model_id.to_string());
    candidates.push(format!("{model_id}.bin"));
    candidates.push(format!("{model_id}.gguf"));
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
        exe_dir.join("models"),                   // models/ next to .exe
        exe_dir.join("../models"),                // models/ one level up
        exe_dir.join("../whisper.cpp/models"),     // whisper.cpp/models one level up
        exe_dir.join("../../whisper.cpp/models"),  // whisper.cpp/models two levels up
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

#[derive(Debug, Clone)]
struct SourceModel {
  id: String,
  label: String,
  file_name: String,
  size_mb: u32,
  download_url: String,
  source: String,
}

fn load_custom_source_models(custom_url: &str) -> Result<Vec<SourceModel>, String> {
  let custom_url = custom_url.trim();
  if custom_url.is_empty() {
    return Ok(Vec::new());
  }

  // Security: Validate custom URL before fetching
  validate_model_url(custom_url, UrlSafety::Strict)?;

  if custom_url.ends_with(".bin") || custom_url.ends_with(".gguf") {
    let file_name = filename_from_url(custom_url).ok_or_else(|| "Invalid model URL".to_string())?;
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

  let index: ModelIndex =
    serde_json::from_str(&body).map_err(|_| "Unsupported model index format".to_string())?;
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

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ModelInfo {
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
pub(crate) struct DownloadProgress {
  id: String,
  downloaded: u64,
  total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadComplete {
  id: String,
  path: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DownloadError {
  id: String,
  error: String,
}

#[tauri::command]
pub(crate) fn list_models(app: AppHandle, state: State<'_, AppState>) -> Vec<ModelInfo> {
  let downloads = state.downloads.lock().unwrap();
  let settings = state.settings.lock().unwrap().clone();
  let models_dir = resolve_models_dir(&app);
  let hidden_external = settings.hidden_external_models.clone();

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
      // Add ?download=true for better HuggingFace CDN handling
      let download_url = format!("{}/{}?download=true", base_url.trim_end_matches('/'), spec.file_name);
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

    // German-optimized large-v3-turbo (fine-tuned by cstr for German speech)
    let german_url = "https://huggingface.co/cstr/whisper-large-v3-turbo-german-ggml/resolve/main/ggml-model.bin?download=true";
    if let Err(err) = is_url_safe(german_url, UrlSafety::Basic) {
      warn!("Skipping unsafe German model URL: {}", err);
    } else {
      defaults.push(SourceModel {
        id: "whisper-large-v3-turbo-german".to_string(),
        label: "Whisper large-v3-turbo (DE)".to_string(),
        file_name: "ggml-large-v3-turbo-german.bin".to_string(),
        size_mb: 1650,
        download_url: german_url.to_string(),
        source: "german".to_string(),
      });
    }

    defaults
  };

  let mut seen_files = HashSet::new();
  let mut models: Vec<ModelInfo> = source_models
    .into_iter()
    .map(|model| {
      let mut path = if settings.model_source == "default" {
        resolve_model_path(&app, &model.id)
      } else {
        resolve_model_path_by_file(&app, &model.file_name)
      };
      if !model.file_name.is_empty() {
        seen_files.insert(model.file_name.clone());
      }
      let mut removable = path
        .as_ref()
        .map(|p| p.starts_with(&models_dir))
        .unwrap_or(false);
      if let Some(p) = path.as_ref() {
        let path_str = p.to_string_lossy().to_string();
        if !removable && hidden_external.contains(&path_str) {
          path = None;
          removable = false;
        }
      }
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
      let label = match file_name.as_str() {
        "ggml-large-v3-turbo-q5_0.bin" => "Whisper large-v3-turbo (q5_0)".to_string(),
        _ => file_name.clone(),
      };
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
        label,
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
pub(crate) fn download_model(
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
    // Add ?download=true for better HuggingFace CDN handling
    let url = format!("{}/{}?download=true", base_url.trim_end_matches('/'), spec.file_name);
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
pub(crate) fn remove_model(app: AppHandle, file_name: String) -> Result<(), String> {
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
pub(crate) fn quantize_model(
  app: AppHandle,
  file_name: String,
  quant: Option<String>,
) -> Result<(), String> {
  if file_name.trim().is_empty() {
    return Err("Missing model file name".to_string());
  }
  validate_model_file_name(&file_name)?;

  if !file_name.ends_with(".bin") {
    return Err("Only .bin models can be quantized".to_string());
  }
  if file_name.contains("-q5_0") {
    return Err("Model already looks quantized".to_string());
  }

  let quant_type = quant.unwrap_or_else(|| "q5_0".to_string());
  if quant_type != "q5_0" {
    return Err("Only q5_0 is supported for now".to_string());
  }

  let models_dir = resolve_models_dir(&app);
  let input_path = models_dir.join(&file_name);
  if !input_path.exists() {
    return Err("Model file not found in app cache".to_string());
  }

  let output_name = file_name.trim_end_matches(".bin").to_string() + "-q5_0.bin";
  validate_model_file_name(&output_name)?;
  let output_path = models_dir.join(&output_name);
  if output_path.exists() {
    return Err("Quantized model already exists".to_string());
  }

  let quantize_path = resolve_quantize_path(&app)
    .ok_or_else(|| "quantize.exe not found. Install/bundle it or set TRISPR_WHISPER_QUANTIZE.".to_string())?;

  let mut quantize_cmd = std::process::Command::new(quantize_path);
  quantize_cmd
    .arg(&input_path)
    .arg(&output_path)
    .arg(&quant_type);

  #[cfg(target_os = "windows")]
  {
    use std::os::windows::process::CommandExt;
    quantize_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
  }

  let status = quantize_cmd
    .status()
    .map_err(|e| format!("Failed to launch quantize: {e}"))?;

  if !status.success() {
    return Err(format!("Quantize failed with exit code {}", status));
  }

  Ok(())
}

#[tauri::command]
pub(crate) fn hide_external_model(
  app: AppHandle,
  state: State<'_, AppState>,
  path: String,
) -> Result<(), String> {
  if path.trim().is_empty() {
    return Err("Missing model path".to_string());
  }
  let mut settings = state.settings.lock().unwrap();
  settings.hidden_external_models.insert(path);
  let persisted = settings.clone();
  drop(settings);
  save_settings_file(&app, &persisted)?;
  Ok(())
}

#[tauri::command]
pub(crate) fn clear_hidden_external_models(
  app: AppHandle,
  state: State<'_, AppState>,
) -> Result<(), String> {
  let mut settings = state.settings.lock().unwrap();
  settings.hidden_external_models.clear();
  let persisted = settings.clone();
  drop(settings);
  save_settings_file(&app, &persisted)?;
  Ok(())
}

#[tauri::command]
pub(crate) fn pick_model_dir() -> Option<String> {
  rfd::FileDialog::new()
    .pick_folder()
    .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
pub(crate) fn get_models_dir(app: AppHandle) -> String {
  resolve_models_dir(&app).to_string_lossy().to_string()
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

#[tauri::command]
pub(crate) fn check_model_available(app: AppHandle, model_id: String) -> bool {
  resolve_model_path(&app, &model_id).is_some()
}
