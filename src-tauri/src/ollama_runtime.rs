use crate::ai_fallback::provider::{
    is_local_ollama_endpoint, list_ollama_models, ping_ollama, ping_ollama_quick,
};
use crate::check_strict_local_mode;
use crate::now_iso;
use crate::paths::resolve_data_path;
use crate::state::{
    record_runtime_start_attempt, record_runtime_start_failure, save_settings_file, AppState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use url::Url;
use which::which;
use zip::ZipArchive;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const DEFAULT_RUNTIME_VERSION: &str = "0.17.5";
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/ollama/ollama/releases";
const BACKGROUND_IO_THROTTLE_BYTES: u64 = 16 * 1024 * 1024;
const BACKGROUND_IO_THROTTLE_SLEEP_MS: u64 = 2;
const STARTUP_FOREGROUND_WAIT_MS: u64 = 12_000;

struct RuntimeManifest {
    version: &'static str,
    url: &'static str,
    sha256: &'static str,
}

#[derive(Debug, Clone)]
struct RuntimeManifestResolved {
    version: String,
    url: String,
    sha256: String,
}

const WINDOWS_MANIFESTS: [RuntimeManifest; 1] = [RuntimeManifest {
    version: "0.17.5",
    url: "https://github.com/ollama/ollama/releases/download/v0.17.5/ollama-windows-amd64.zip",
    sha256: "2748fe1a44a2cef4c3071f84d000e5cbe1ff614c574465f4404f66f559e414b6",
}];

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeInstallProgress {
    pub stage: String,
    pub message: String,
    pub downloaded: Option<u64>,
    pub total: Option<u64>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeVersionInfo {
    pub version: String,
    pub source: String, // "pinned" | "online"
    pub selected: bool,
    pub installed: bool,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeInstallComplete {
    pub version: String,
    pub runtime_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeInstallError {
    pub stage: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeHealth {
    pub ok: bool,
    pub endpoint: String,
    pub models_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeDetectResult {
    pub found: bool,
    pub is_serving: bool,
    pub source: String,
    pub path: String,
    pub version: String,
    pub managed_pid: Option<u32>,
    pub managed_alive: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeDownloadResult {
    pub archive_path: String,
    pub sha256_ok: bool,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeInstallResult {
    pub runtime_path: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeStartResult {
    pub pid: Option<u32>,
    pub endpoint: String,
    pub source: String,
    pub already_running: bool,
    pub pending_start: bool,
    pub startup_wait_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeVerifyResult {
    pub ok: bool,
    pub endpoint: String,
    pub models_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaImportResult {
    pub model_name: String,
}

fn normalize_runtime_version(version: Option<&str>) -> String {
    let requested = version
        .map(|v| v.trim().trim_start_matches('v').to_string())
        .unwrap_or_else(|| DEFAULT_RUNTIME_VERSION.to_string());
    if requested.eq_ignore_ascii_case("latest") {
        DEFAULT_RUNTIME_VERSION.to_string()
    } else {
        requested
    }
}

fn resolve_pinned_manifest(version: &str) -> Option<RuntimeManifestResolved> {
    WINDOWS_MANIFESTS
        .iter()
        .find(|m| m.version == version)
        .map(|m| RuntimeManifestResolved {
            version: m.version.to_string(),
            url: m.url.to_string(),
            sha256: m.sha256.to_string(),
        })
}

fn parse_sha256_from_text(text: &str, file_name: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            continue;
        }
        if tokens.len() == 1 {
            let only = tokens[0].trim().trim_start_matches('*');
            if only.len() == 64 && only.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Some(only.to_ascii_lowercase());
            }
            continue;
        }
        if tokens[1..].iter().any(|token| token.trim_start_matches('*') == file_name) {
            let hash = tokens[0].trim().trim_start_matches('*');
            if hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Some(hash.to_ascii_lowercase());
            }
        }
    }
    None
}

fn fetch_online_manifest(version: &str) -> Result<RuntimeManifestResolved, String> {
    let tag = format!("v{}", version);
    let release_url = format!("{}/tags/{}", GITHUB_RELEASES_API, tag);
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build();

    let release_json = agent
        .get(&release_url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Failed to fetch Ollama release '{}': {}", tag, e))?
        .into_json::<serde_json::Value>()
        .map_err(|e| format!("Failed to parse Ollama release '{}': {}", tag, e))?;

    let assets = release_json
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Release '{}' does not expose assets.", tag))?;

    let mut archive_url: Option<String> = None;
    let mut checksum_urls: Vec<String> = Vec::new();

    for asset in assets {
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(url) = asset
            .get("browser_download_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
        else {
            continue;
        };

        if name.eq_ignore_ascii_case("ollama-windows-amd64.zip") {
            archive_url = Some(url);
            continue;
        }

        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".sha256")
            || lower.contains("sha256sum")
            || lower.contains("checksums")
            || lower.contains("sha256")
        {
            checksum_urls.push(url);
        }
    }

    let archive_url = archive_url
        .ok_or_else(|| format!("Release '{}' has no ollama-windows-amd64.zip asset.", tag))?;

    let mut checksum: Option<String> = None;
    for checksum_url in checksum_urls {
        let body = match agent
            .get(&checksum_url)
            .set("User-Agent", "TrisprFlow/RuntimeInstaller")
            .call()
        {
            Ok(resp) => resp.into_string().unwrap_or_default(),
            Err(_) => String::new(),
        };
        if body.trim().is_empty() {
            continue;
        }
        if let Some(parsed) = parse_sha256_from_text(&body, "ollama-windows-amd64.zip") {
            checksum = Some(parsed);
            break;
        }
    }

    let sha256 = checksum.ok_or_else(|| {
        format!(
            "Release '{}' is missing a parseable checksum for ollama-windows-amd64.zip.",
            tag
        )
    })?;

    Ok(RuntimeManifestResolved {
        version: version.to_string(),
        url: archive_url,
        sha256,
    })
}

fn resolve_manifest(version: Option<&str>) -> Result<RuntimeManifestResolved, String> {
    let requested = normalize_runtime_version(version);
    if let Some(manifest) = resolve_pinned_manifest(&requested) {
        return Ok(manifest);
    }

    fetch_online_manifest(&requested).map_err(|e| {
        let supported = WINDOWS_MANIFESTS
            .iter()
            .map(|m| m.version)
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Runtime version '{}' is not pinned and online lookup failed: {}. Pinned versions: {}",
            requested, e, supported
        )
    })
}

fn list_online_release_versions(limit: usize) -> Result<Vec<String>, String> {
    let url = format!("{}?per_page={}", GITHUB_RELEASES_API, limit.max(1));
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .build();
    let releases = agent
        .get(&url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Failed to fetch online runtime versions: {}", e))?
        .into_json::<serde_json::Value>()
        .map_err(|e| format!("Failed to parse online runtime versions: {}", e))?;
    let mut versions = Vec::new();
    let Some(items) = releases.as_array() else {
        return Ok(versions);
    };
    for item in items {
        if item
            .get("draft")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let Some(tag_name) = item.get("tag_name").and_then(|v| v.as_str()) else {
            continue;
        };
        let normalized = tag_name.trim().trim_start_matches('v').to_string();
        if normalized.is_empty() || versions.iter().any(|v| v == &normalized) {
            continue;
        }
        versions.push(normalized);
    }
    Ok(versions)
}

/// Resolves the root directory for the managed Ollama runtime installation.
///
/// This intentionally does NOT use paths.rs because:
/// 1. On Windows, the runtime must live under %LOCALAPPDATA%\TrisprFlow to keep
///    the path short and predictable (Ollama is sensitive to long paths).
/// 2. paths.rs uses Tauri's app_data_dir / app_config_dir which resolve to an
///    app-identifier-based subdirectory that differs from what we need here.
fn resolve_runtime_root(app: &AppHandle) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let path = PathBuf::from(local_app_data)
                .join("TrisprFlow")
                .join("ollama-runtime");
            let _ = fs::create_dir_all(&path);
            return path;
        }
    }

    let fallback = app
        .path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("ollama-runtime");
    let _ = fs::create_dir_all(&fallback);
    fallback
}

fn resolve_runtime_cache_dir(app: &AppHandle) -> PathBuf {
    let dir = resolve_data_path(app, "runtime-cache");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn emit_install_progress(
    app: &AppHandle,
    stage: &str,
    message: String,
    downloaded: Option<u64>,
    total: Option<u64>,
    version: Option<String>,
) {
    let _ = app.emit(
        "ollama:runtime-install-progress",
        OllamaRuntimeInstallProgress {
            stage: stage.to_string(),
            message,
            downloaded,
            total,
            version,
        },
    );
}

fn emit_install_error(app: &AppHandle, stage: &str, error: String) {
    let _ = app.emit(
        "ollama:runtime-install-error",
        OllamaRuntimeInstallError {
            stage: stage.to_string(),
            error,
        },
    );
}

fn emit_runtime_health(app: &AppHandle, endpoint: String, models_count: usize, ok: bool) {
    let _ = app.emit(
        "ollama:runtime-health",
        OllamaRuntimeHealth {
            ok,
            endpoint,
            models_count,
        },
    );
}

fn maybe_throttle_background_io(processed_since_pause: &mut u64) {
    if *processed_since_pause < BACKGROUND_IO_THROTTLE_BYTES {
        return;
    }
    *processed_since_pause = 0;
    std::thread::sleep(Duration::from_millis(BACKGROUND_IO_THROTTLE_SLEEP_MS));
}

fn copy_with_background_throttle<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; 1024 * 256];
    let mut total_written = 0u64;
    let mut processed_since_pause = 0u64;

    loop {
        let read = reader.read(&mut buf)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buf[..read])?;
        total_written += read as u64;
        processed_since_pause += read as u64;
        maybe_throttle_background_io(&mut processed_since_pause);
    }

    Ok(total_written)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|e| format!("Failed to open file for hashing: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 64];
    let mut processed_since_pause = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|e| format!("Failed to read file for hashing: {}", e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        processed_since_pause += read as u64;
        maybe_throttle_background_io(&mut processed_since_pause);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn find_file_recursive(root: &Path, target_name: &str) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = fs::read_dir(&path).ok()?;
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                stack.push(entry_path);
                continue;
            }
            if entry_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.eq_ignore_ascii_case(target_name))
                .unwrap_or(false)
            {
                return Some(entry_path);
            }
        }
    }
    None
}

fn runtime_dependency_dirs(binary_path: &Path) -> Vec<PathBuf> {
    let Some(runtime_dir) = binary_path.parent() else {
        return Vec::new();
    };

    let ollama_lib_dir = runtime_dir.join("lib").join("ollama");
    let candidates = [
        runtime_dir.to_path_buf(),
        ollama_lib_dir.clone(),
        ollama_lib_dir.join("cuda_v13"),
        ollama_lib_dir.join("cuda_v12"),
        ollama_lib_dir.join("vulkan"),
    ];

    candidates
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

fn with_runtime_paths_prepend(dependency_dirs: &[PathBuf]) -> Option<std::ffi::OsString> {
    if dependency_dirs.is_empty() {
        return None;
    }

    let mut merged = dependency_dirs.to_vec();
    if let Some(current) = std::env::var_os("PATH") {
        merged.extend(std::env::split_paths(&current));
    }

    std::env::join_paths(merged).ok()
}

fn parse_ollama_version(binary_path: &Path) -> String {
    static VERSION_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = VERSION_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let cache_key = binary_path.to_string_lossy().to_string();

    if let Ok(guard) = cache.lock() {
        if let Some(version) = guard.get(&cache_key) {
            return version.clone();
        }
    }

    let output = Command::new(binary_path).arg("--version").output();
    let text = match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            format!("{}\n{}", stdout.trim(), stderr.trim())
        }
        Err(_) => String::new(),
    };
    for token in text.split_whitespace() {
        if token.chars().any(|c| c.is_ascii_digit())
            && token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == 'v')
        {
            let parsed = token.trim().trim_start_matches('v').to_string();
            if let Ok(mut guard) = cache.lock() {
                guard.insert(cache_key, parsed.clone());
            }
            return parsed;
        }
    }
    let empty = String::new();
    if let Ok(mut guard) = cache.lock() {
        guard.insert(cache_key, empty.clone());
    }
    empty
}

fn sanitize_model_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == ':' || c == '.' || c == '-' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "imported-model".to_string()
    } else {
        out
    }
}

fn endpoint_host_port(endpoint: &str) -> Result<String, String> {
    let parsed = Url::parse(endpoint).map_err(|e| format!("Invalid endpoint URL: {}", e))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "Endpoint URL is missing host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(11434);
    Ok(format!("{}:{}", host, port))
}

fn parse_runtime_version_tuple(version: &str) -> (u32, u32, u32) {
    let mut parts = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .filter_map(|p| p.parse::<u32>().ok());
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn compare_runtime_versions_desc(left: &str, right: &str) -> Ordering {
    let l = parse_runtime_version_tuple(left);
    let r = parse_runtime_version_tuple(right);
    r.cmp(&l).then_with(|| right.cmp(left))
}

fn find_managed_runtime_binary(
    app: &AppHandle,
    settings: &crate::state::Settings,
) -> Option<PathBuf> {
    let runtime_root = resolve_runtime_root(app);
    if !runtime_root.is_dir() {
        return None;
    }

    let mut preferred_versions: Vec<String> = Vec::new();
    let target = settings.providers.ollama.runtime_target_version.trim();
    if !target.is_empty() {
        preferred_versions.push(target.to_string());
    }
    let current = settings.providers.ollama.runtime_version.trim();
    if !current.is_empty() && !preferred_versions.iter().any(|v| v == current) {
        preferred_versions.push(current.to_string());
    }

    for version in preferred_versions {
        let candidate_dir = runtime_root.join(version);
        if !candidate_dir.is_dir() {
            continue;
        }
        if let Some(path) = find_file_recursive(&candidate_dir, "ollama.exe") {
            return Some(path);
        }
    }

    let mut discovered: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(&runtime_root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.starts_with(".staging-") {
                continue;
            }
            if let Some(path) = find_file_recursive(&dir, "ollama.exe") {
                discovered.push((name, path));
            }
        }
    }

    discovered.sort_by(|a, b| compare_runtime_versions_desc(&a.0, &b.0));
    discovered.into_iter().map(|(_, path)| path).next()
}

fn select_runtime_binary(
    app: &AppHandle,
    settings: &crate::state::Settings,
) -> Result<(PathBuf, String), String> {
    let configured_source = settings
        .providers
        .ollama
        .runtime_source
        .trim()
        .to_lowercase();
    let configured = settings.providers.ollama.runtime_path.trim();

    if configured_source == "system" {
        if let Ok(system) = which("ollama") {
            return Ok((system, "system".to_string()));
        }
        if !configured.is_empty() {
            let path = PathBuf::from(configured);
            if path.exists() {
                return Ok((path, "system".to_string()));
            }
        }
        if let Some(managed) = find_managed_runtime_binary(app, settings) {
            return Ok((managed, "per_user_zip".to_string()));
        }
        return Err(
            "System runtime was selected, but no Ollama was found in PATH. Use 'Use managed runtime' or install Ollama system-wide."
                .to_string(),
        );
    }

    if configured_source == "per_user_zip" {
        if !configured.is_empty() {
            let path = PathBuf::from(configured);
            if path.exists() {
                return Ok((path, "per_user_zip".to_string()));
            }
        }
        if let Some(managed) = find_managed_runtime_binary(app, settings) {
            return Ok((managed, "per_user_zip".to_string()));
        }
        return Err(
            "Managed local runtime is selected, but no local Ollama runtime is installed yet. Use 'Install local runtime'."
                .to_string(),
        );
    }

    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.exists() {
            let source = if configured_source == "per_user_zip" {
                "per_user_zip"
            } else if configured_source == "system" {
                "system"
            } else {
                "manual"
            };
            return Ok((path, source.to_string()));
        }
    }

    if let Ok(system) = which("ollama") {
        return Ok((system, "system".to_string()));
    }
    if let Some(managed) = find_managed_runtime_binary(app, settings) {
        return Ok((managed, "per_user_zip".to_string()));
    }
    Err("No Ollama runtime found. Install local runtime or install Ollama system-wide.".to_string())
}

fn update_runtime_in_settings(
    app: &AppHandle,
    state: &AppState,
    source: String,
    runtime_path: String,
    runtime_version: String,
    health_check: Option<String>,
    mark_setup_complete: bool,
) -> Result<(), String> {
    let snapshot = {
        let mut settings = state.settings.lock().unwrap();
        settings.providers.ollama.runtime_source = source;
        settings.providers.ollama.runtime_path = runtime_path;
        settings.providers.ollama.runtime_version = runtime_version;
        if let Some(ts) = health_check {
            settings.providers.ollama.last_health_check = Some(ts);
        }
        if mark_setup_complete {
            settings.setup.local_ai_wizard_completed = true;
            settings.setup.local_ai_wizard_pending = false;
        } else if settings.setup.local_ai_wizard_completed {
            settings.setup.local_ai_wizard_pending = false;
        }
        settings.clone()
    };
    save_settings_file(app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot);
    Ok(())
}

fn runtime_endpoint_reachable(endpoint: &str) -> bool {
    if ping_ollama_quick(endpoint).is_ok() {
        return true;
    }

    // Fallback to a slower probe. Quick ping can miss a busy but already
    // serving runtime, which would otherwise leave the UI in "starting".
    ping_ollama(endpoint).is_ok()
}

fn managed_child_status(state: &AppState) -> (Option<u32>, bool) {
    let Ok(mut guard) = state.managed_ollama_child.lock() else {
        return (None, false);
    };

    let Some(child) = guard.as_mut() else {
        return (None, false);
    };

    let pid = Some(child.id());
    match child.try_wait() {
        Ok(Some(_)) => {
            *guard = None;
            (pid, false)
        }
        Ok(None) => (pid, true),
        Err(_) => (pid, false),
    }
}

#[tauri::command]
pub fn list_ollama_runtime_versions(
    state: State<'_, AppState>,
) -> Result<Vec<OllamaRuntimeVersionInfo>, String> {
    let snapshot = state.settings.lock().unwrap().clone();
    let selected = snapshot.providers.ollama.runtime_target_version.clone();
    let installed_version = snapshot.providers.ollama.runtime_version.clone();

    let mut merged: Vec<String> = WINDOWS_MANIFESTS
        .iter()
        .map(|m| m.version.to_string())
        .collect();
    if let Ok(online) = list_online_release_versions(20) {
        for version in online {
            if !merged.iter().any(|v| v == &version) {
                merged.push(version);
            }
        }
    }
    if !installed_version.trim().is_empty() && !merged.iter().any(|v| v == &installed_version) {
        merged.push(installed_version.clone());
    }

    let mut out = merged
        .into_iter()
        .map(|version| OllamaRuntimeVersionInfo {
            version: version.clone(),
            source: if WINDOWS_MANIFESTS.iter().any(|m| m.version == version) {
                "pinned".to_string()
            } else {
                "online".to_string()
            },
            selected: selected == version,
            installed: !installed_version.is_empty() && installed_version == version,
            recommended: version == DEFAULT_RUNTIME_VERSION,
        })
        .collect::<Vec<_>>();

    out.sort_by(|a, b| {
        b.selected
            .cmp(&a.selected)
            .then_with(|| b.installed.cmp(&a.installed))
            .then_with(|| b.recommended.cmp(&a.recommended))
            .then_with(|| b.version.cmp(&a.version))
    });

    Ok(out)
}

#[tauri::command]
pub fn detect_ollama_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OllamaRuntimeDetectResult, String> {
    let settings = state.settings.lock().unwrap().clone();
    let (managed_pid, managed_alive) = managed_child_status(state.inner());
    let endpoint = settings.providers.ollama.endpoint.clone();
    let source_hint = settings
        .providers
        .ollama
        .runtime_source
        .trim()
        .to_lowercase();

    // Phase 1: filesystem-only binary search (no network).
    let binary_info: Option<(String, PathBuf)> = {
        let configured = settings.providers.ollama.runtime_path.trim();
        if !configured.is_empty() {
            let path = PathBuf::from(configured);
            if path.exists() {
                let source = if source_hint == "system" {
                    "system".to_string()
                } else if source_hint == "per_user_zip" {
                    "per_user_zip".to_string()
                } else {
                    "manual".to_string()
                };
                Some((source, path))
            } else {
                None
            }
        } else {
            None
        }
    };
    let binary_info = binary_info
        .or_else(|| {
            if source_hint == "per_user_zip" || source_hint == "system" {
                find_managed_runtime_binary(&app, &settings)
                    .map(|p| ("per_user_zip".to_string(), p))
            } else {
                None
            }
        })
        .or_else(|| which("ollama").ok().map(|p| ("system".to_string(), p)))
        .or_else(|| {
            find_managed_runtime_binary(&app, &settings).map(|p| ("per_user_zip".to_string(), p))
        });

    match binary_info {
        None => Ok(OllamaRuntimeDetectResult {
            found: false,
            is_serving: false,
            source: "manual".to_string(),
            path: String::new(),
            version: String::new(),
            managed_pid,
            managed_alive,
        }),
        Some((source, path)) => {
            // Phase 2: single quick ping (≤ 300 ms) — never blocks the UI thread noticeably.
            let is_serving = ping_ollama_quick(&endpoint).is_ok();
            Ok(OllamaRuntimeDetectResult {
                found: true,
                is_serving,
                source,
                version: parse_ollama_version(&path),
                path: path.to_string_lossy().to_string(),
                managed_pid,
                managed_alive,
            })
        }
    }
}

#[tauri::command]
pub async fn download_ollama_runtime(
    app: AppHandle,
    version: Option<String>,
) -> Result<OllamaRuntimeDownloadResult, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || download_ollama_runtime_impl(&app_handle, version))
        .await
        .map_err(|e| format!("Runtime download task failed: {}", e))?
}

fn download_ollama_runtime_impl(
    app: &AppHandle,
    version: Option<String>,
) -> Result<OllamaRuntimeDownloadResult, String> {
    let manifest = resolve_manifest(version.as_deref())?;
    let cache_dir = resolve_runtime_cache_dir(app);
    let archive_path = cache_dir.join(format!("ollama-windows-amd64-v{}.zip", manifest.version));
    let temp_path = archive_path.with_extension("zip.part");

    if archive_path.exists() {
        let current_hash = sha256_file(&archive_path)?;
        if current_hash.eq_ignore_ascii_case(&manifest.sha256) {
            emit_install_progress(
                app,
                "download_runtime",
                format!("Runtime archive already cached ({})", manifest.version),
                None,
                None,
                Some(manifest.version.to_string()),
            );
            return Ok(OllamaRuntimeDownloadResult {
                archive_path: archive_path.to_string_lossy().to_string(),
                sha256_ok: true,
                version: manifest.version.to_string(),
            });
        }
        let _ = fs::remove_file(&archive_path);
    }

    emit_install_progress(
        app,
        "download_runtime",
        format!("Downloading Ollama runtime {}", manifest.version),
        Some(0),
        None,
        Some(manifest.version.to_string()),
    );

    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(60 * 60 * 4))
        .build();
    let response = agent
        .get(&manifest.url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .call()
        .map_err(|e| {
            let msg = format!("Failed to download runtime archive: {}", e);
            emit_install_error(app, "download_runtime", msg.clone());
            msg
        })?;

    let total = response
        .header("Content-Length")
        .and_then(|h| h.parse::<u64>().ok());

    let mut reader = response.into_reader();
    let mut out = File::create(&temp_path).map_err(|e| {
        let msg = format!("Failed to create temp archive file: {}", e);
        emit_install_error(app, "download_runtime", msg.clone());
        msg
    })?;
    let mut buf = vec![0u8; 1024 * 256];
    let mut downloaded = 0u64;
    let mut last_emit = Instant::now();
    let mut processed_since_pause = 0u64;
    loop {
        let read = reader.read(&mut buf).map_err(|e| {
            let msg = format!("Failed while downloading runtime archive: {}", e);
            emit_install_error(app, "download_runtime", msg.clone());
            msg
        })?;
        if read == 0 {
            break;
        }
        out.write_all(&buf[..read]).map_err(|e| {
            let msg = format!("Failed to write runtime archive to disk: {}", e);
            emit_install_error(app, "download_runtime", msg.clone());
            msg
        })?;
        downloaded += read as u64;
        processed_since_pause += read as u64;
        maybe_throttle_background_io(&mut processed_since_pause);
        if last_emit.elapsed() >= Duration::from_millis(250) {
            emit_install_progress(
                app,
                "download_runtime",
                "Downloading runtime archive...".to_string(),
                Some(downloaded),
                total,
                Some(manifest.version.to_string()),
            );
            last_emit = Instant::now();
        }
    }

    fs::rename(&temp_path, &archive_path).map_err(|e| {
        let msg = format!("Failed to finalize downloaded archive: {}", e);
        emit_install_error(app, "download_runtime", msg.clone());
        msg
    })?;

    let digest = sha256_file(&archive_path)?;
    if !digest.eq_ignore_ascii_case(&manifest.sha256) {
        let _ = fs::remove_file(&archive_path);
        let msg = format!(
            "Runtime archive checksum mismatch. Expected {}, got {}",
            manifest.sha256, digest
        );
        emit_install_error(app, "download_runtime", msg.clone());
        return Err(msg);
    }

    emit_install_progress(
        app,
        "download_runtime",
        "Download complete and checksum verified.".to_string(),
        Some(downloaded),
        total.or(Some(downloaded)),
        Some(manifest.version.to_string()),
    );

    Ok(OllamaRuntimeDownloadResult {
        archive_path: archive_path.to_string_lossy().to_string(),
        sha256_ok: true,
        version: manifest.version.to_string(),
    })
}

#[tauri::command]
pub async fn install_ollama_runtime(
    app: AppHandle,
    archive_path: String,
) -> Result<OllamaRuntimeInstallResult, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        install_ollama_runtime_impl(&app_handle, archive_path)
    })
    .await
    .map_err(|e| format!("Runtime install task failed: {}", e))?
}

fn install_ollama_runtime_impl(
    app: &AppHandle,
    archive_path: String,
) -> Result<OllamaRuntimeInstallResult, String> {
    let state = app.state::<AppState>();
    let archive = PathBuf::from(archive_path.trim());
    if !archive.exists() {
        return Err("Archive file does not exist.".to_string());
    }

    let archive_digest = sha256_file(&archive)?;
    let manifest = WINDOWS_MANIFESTS
        .iter()
        .find(|m| m.sha256.eq_ignore_ascii_case(&archive_digest))
        .ok_or_else(|| {
            "Archive checksum is not in the pinned runtime manifest. Refusing installation."
                .to_string()
        })?;

    emit_install_progress(
        app,
        "install_runtime",
        format!("Installing Ollama runtime {}", manifest.version),
        None,
        None,
        Some(manifest.version.to_string()),
    );

    let runtime_root = resolve_runtime_root(app);
    let target_dir = runtime_root.join(&manifest.version);
    let staging_dir = runtime_root.join(format!(
        ".staging-{}-{}",
        manifest.version,
        crate::util::now_ms()
    ));
    let _ = fs::remove_dir_all(&staging_dir);
    fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Failed to create staging directory: {}", e))?;

    let file =
        File::open(&archive).map_err(|e| format!("Failed to open runtime archive: {}", e))?;
    let mut zip =
        ZipArchive::new(file).map_err(|e| format!("Invalid runtime ZIP archive: {}", e))?;
    let total_entries = zip.len();

    for idx in 0..total_entries {
        let mut entry = zip
            .by_index(idx)
            .map_err(|e| format!("Failed to read ZIP entry {}: {}", idx, e))?;

        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| format!("Unsafe ZIP entry path rejected: {}", entry.name()))?
            .to_owned();
        let out_path = staging_dir.join(enclosed);

        if entry.name().ends_with('/') {
            fs::create_dir_all(&out_path).map_err(|e| {
                format!("Failed to create directory '{}': {}", out_path.display(), e)
            })?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!(
                        "Failed to create parent directory '{}': {}",
                        parent.display(),
                        e
                    )
                })?;
            }
            let mut out = File::create(&out_path)
                .map_err(|e| format!("Failed to create file '{}': {}", out_path.display(), e))?;
            copy_with_background_throttle(&mut entry, &mut out)
                .map_err(|e| format!("Failed to extract '{}': {}", out_path.display(), e))?;
        }

        if idx == 0 || idx + 1 == total_entries || (idx + 1) % 10 == 0 {
            emit_install_progress(
                app,
                "install_runtime",
                format!("Extracting runtime files ({}/{})", idx + 1, total_entries),
                Some((idx + 1) as u64),
                Some(total_entries as u64),
                Some(manifest.version.to_string()),
            );
        }
    }

    let staged_binary = find_file_recursive(&staging_dir, "ollama.exe")
        .ok_or_else(|| "Installed runtime does not contain ollama.exe".to_string())?;

    // Remove existing target first; Windows rename fails if target dir exists.
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| {
            format!(
                "Failed to remove previous runtime at '{}': {}",
                target_dir.display(),
                e
            )
        })?;
    }
    fs::rename(&staging_dir, &target_dir)
        .map_err(|e| format!("Failed to move runtime into final location: {}", e))?;

    let runtime_binary = find_file_recursive(&target_dir, "ollama.exe")
        .ok_or_else(|| "Installed runtime binary not found in final location".to_string())?;

    let _ = staged_binary; // Explicitly keep extraction validation before rename.

    update_runtime_in_settings(
        app,
        state.inner(),
        "per_user_zip".to_string(),
        runtime_binary.to_string_lossy().to_string(),
        manifest.version.to_string(),
        None,
        false,
    )?;

    let _ = app.emit(
        "ollama:runtime-install-complete",
        OllamaRuntimeInstallComplete {
            version: manifest.version.to_string(),
            runtime_path: runtime_binary.to_string_lossy().to_string(),
        },
    );

    Ok(OllamaRuntimeInstallResult {
        runtime_path: runtime_binary.to_string_lossy().to_string(),
        version: manifest.version.to_string(),
    })
}

#[tauri::command]
pub async fn start_ollama_runtime(app: AppHandle) -> Result<OllamaRuntimeStartResult, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || start_ollama_runtime_impl(&app_handle))
        .await
        .map_err(|e| format!("Runtime start task failed: {}", e))?
}

fn start_ollama_runtime_impl(app: &AppHandle) -> Result<OllamaRuntimeStartResult, String> {
    let state = app.state::<AppState>();
    record_runtime_start_attempt(state.inner());
    let settings_snapshot = state.settings.lock().unwrap().clone();
    let endpoint = settings_snapshot
        .providers
        .ollama
        .endpoint
        .trim()
        .to_string();
    if endpoint.is_empty() {
        record_runtime_start_failure(state.inner());
        return Err("Ollama endpoint is empty.".to_string());
    }
    if settings_snapshot.ai_fallback.strict_local_mode && !is_local_ollama_endpoint(&endpoint) {
        record_runtime_start_failure(state.inner());
        return Err(
            "Strict local mode is enabled. Only localhost/127.0.0.1 endpoints are allowed."
                .to_string(),
        );
    }
    if !is_local_ollama_endpoint(&endpoint) {
        record_runtime_start_failure(state.inner());
        return Err(
            "Runtime autostart only supports local endpoints. Configure a local endpoint first."
                .to_string(),
        );
    }

    let (binary_path, source) = select_runtime_binary(app, &settings_snapshot).map_err(|err| {
        record_runtime_start_failure(state.inner());
        err
    })?;
    let version = parse_ollama_version(&binary_path);
    if runtime_endpoint_reachable(&endpoint) {
        let ts = now_iso();
        let _ = update_runtime_in_settings(
            app,
            state.inner(),
            source.clone(),
            binary_path.to_string_lossy().to_string(),
            version.clone(),
            Some(ts),
            false,
        );
        let models = list_ollama_models(&endpoint);
        emit_runtime_health(app, endpoint.clone(), models.len(), true);
        return Ok(OllamaRuntimeStartResult {
            pid: None,
            endpoint,
            source,
            already_running: true,
            pending_start: false,
            startup_wait_ms: 0,
        });
    }

    let host = endpoint_host_port(&endpoint).map_err(|err| {
        record_runtime_start_failure(state.inner());
        err
    })?;
    let runtime_dep_dirs = runtime_dependency_dirs(&binary_path);
    let runners_dir = binary_path
        .parent()
        .map(|dir| dir.join("lib").join("ollama"))
        .filter(|dir| dir.is_dir());
    let mut cmd = Command::new(&binary_path);
    cmd.arg("serve")
        .env("OLLAMA_HOST", host)
        .env("OLLAMA_NO_CLOUD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(runtime_dir) = binary_path.parent() {
        cmd.current_dir(runtime_dir);
    }
    if let Some(path_value) = with_runtime_paths_prepend(&runtime_dep_dirs) {
        cmd.env("PATH", path_value);
    }
    if let Some(dir) = runners_dir {
        cmd.env("OLLAMA_RUNNERS_DIR", dir.as_os_str());
    }
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let child = cmd.spawn().map_err(|e| {
        record_runtime_start_failure(state.inner());
        format!("Failed to start Ollama runtime: {}", e)
    })?;
    let pid = child.id();
    // Store child handle for cleanup on app exit
    *state.managed_ollama_child.lock().unwrap() = Some(child);

    let wait_started = Instant::now();
    let deadline = wait_started + Duration::from_millis(STARTUP_FOREGROUND_WAIT_MS);
    let mut probe_attempt: u32 = 0;
    loop {
        let reachable = ping_ollama_quick(&endpoint).is_ok()
            || (probe_attempt % 4 == 3 && ping_ollama(&endpoint).is_ok());

        if reachable {
            let ts = now_iso();
            update_runtime_in_settings(
                app,
                state.inner(),
                source.clone(),
                binary_path.to_string_lossy().to_string(),
                version.clone(),
                Some(ts),
                false,
            )?;
            let models = list_ollama_models(&endpoint);
            emit_runtime_health(app, endpoint.clone(), models.len(), true);
            return Ok(OllamaRuntimeStartResult {
                pid: Some(pid),
                endpoint,
                source,
                already_running: false,
                pending_start: false,
                startup_wait_ms: wait_started.elapsed().as_millis() as u64,
            });
        }
        if Instant::now() >= deadline {
            let ts = now_iso();
            let _ = update_runtime_in_settings(
                app,
                state.inner(),
                source.clone(),
                binary_path.to_string_lossy().to_string(),
                version.clone(),
                Some(ts),
                false,
            );
            emit_runtime_health(app, endpoint.clone(), 0, false);
            return Ok(OllamaRuntimeStartResult {
                pid: Some(pid),
                endpoint,
                source,
                already_running: false,
                pending_start: true,
                startup_wait_ms: wait_started.elapsed().as_millis() as u64,
            });
        }
        probe_attempt = probe_attempt.saturating_add(1);
        std::thread::sleep(Duration::from_millis(500));
    }
}

#[tauri::command]
pub async fn verify_ollama_runtime(app: AppHandle) -> Result<OllamaRuntimeVerifyResult, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || verify_ollama_runtime_impl(&app_handle))
        .await
        .map_err(|e| format!("Runtime verify task failed: {}", e))?
}

fn verify_ollama_runtime_impl(app: &AppHandle) -> Result<OllamaRuntimeVerifyResult, String> {
    let state = app.state::<AppState>();
    let settings_snapshot = state.settings.lock().unwrap().clone();
    let endpoint = settings_snapshot
        .providers
        .ollama
        .endpoint
        .trim()
        .to_string();
    if endpoint.is_empty() {
        return Err("Ollama endpoint is empty.".to_string());
    }
    check_strict_local_mode(&settings_snapshot)?;
    let models = list_ollama_models(&endpoint);
    if models.is_empty() {
        ping_ollama_quick(&endpoint).map_err(|e| e.to_string())?;
    }

    emit_runtime_health(app, endpoint.clone(), models.len(), true);

    Ok(OllamaRuntimeVerifyResult {
        ok: true,
        endpoint,
        models_count: models.len(),
    })
}

#[tauri::command]
pub fn import_ollama_model_from_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    mode: String,
) -> Result<OllamaImportResult, String> {
    let source_path = PathBuf::from(path.trim());
    if !source_path.exists() {
        return Err("Import file does not exist.".to_string());
    }
    if !source_path.is_file() {
        return Err("Import path must point to a file.".to_string());
    }

    let settings_snapshot = state.settings.lock().unwrap().clone();
    let endpoint = settings_snapshot.providers.ollama.endpoint.clone();
    check_strict_local_mode(&settings_snapshot)?;
    ping_ollama(&endpoint).map_err(|e| {
        format!(
            "Ollama runtime is not reachable. Start runtime first: {}",
            e
        )
    })?;

    let (binary_path, _) = select_runtime_binary(&app, &settings_snapshot)?;

    let mode = mode.trim().to_lowercase();
    let mut temp_modelfile_path: Option<PathBuf> = None;
    let modelfile_path = if mode == "gguf" {
        let temp_path =
            resolve_data_path(&app, &format!("import-{}.modelfile", crate::util::now_ms()));
        fs::write(
            &temp_path,
            format!("FROM \"{}\"\n", source_path.to_string_lossy()),
        )
        .map_err(|e| format!("Failed to create temporary Modelfile: {}", e))?;
        temp_modelfile_path = Some(temp_path.clone());
        temp_path
    } else if mode == "modelfile" {
        source_path.clone()
    } else {
        return Err("Unsupported import mode. Use 'gguf' or 'modelfile'.".to_string());
    };

    let default_name = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-model");
    let model_name = sanitize_model_name(default_name);
    let host = endpoint_host_port(&endpoint)?;

    let output = Command::new(&binary_path)
        .arg("create")
        .arg(&model_name)
        .arg("-f")
        .arg(&modelfile_path)
        .env("OLLAMA_HOST", host)
        .env("OLLAMA_NO_CLOUD", "1")
        .output()
        .map_err(|e| format!("Failed to run ollama create: {}", e))?;

    if let Some(temp) = temp_modelfile_path {
        let _ = fs::remove_file(temp);
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!(
            "Model import failed for '{}': {}",
            model_name,
            if detail.is_empty() {
                "unknown error".to_string()
            } else {
                detail
            }
        ));
    }

    let models = list_ollama_models(&endpoint);
    let snapshot = {
        let mut settings = state.settings.lock().unwrap();
        settings.providers.ollama.available_models = models.clone();
        if !models.contains(&settings.providers.ollama.preferred_model) {
            settings.providers.ollama.preferred_model = model_name.clone();
        }
        if settings.ai_fallback.model.trim().is_empty()
            || !models.contains(&settings.ai_fallback.model)
        {
            settings.ai_fallback.model = model_name.clone();
        }
        settings.setup.local_ai_wizard_completed = true;
        settings.setup.local_ai_wizard_pending = false;
        settings.clone()
    };
    save_settings_file(&app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot);

    Ok(OllamaImportResult { model_name })
}

#[tauri::command]
pub fn set_strict_local_mode(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<serde_json::Value, String> {
    let snapshot = {
        let mut settings = state.settings.lock().unwrap();
        settings.ai_fallback.strict_local_mode = enabled;
        if enabled && !is_local_ollama_endpoint(&settings.providers.ollama.endpoint) {
            settings.providers.ollama.endpoint = "http://localhost:11434".to_string();
        }
        settings.clone()
    };
    save_settings_file(&app, &snapshot)?;
    let _ = app.emit("settings-changed", snapshot.clone());
    Ok(serde_json::json!({
        "status": "success",
        "strict_local_mode": snapshot.ai_fallback.strict_local_mode,
        "endpoint": snapshot.providers.ollama.endpoint,
    }))
}
