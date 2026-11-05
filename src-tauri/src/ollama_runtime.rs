use crate::ai_fallback::provider::{
    list_ollama_models, ping_ollama, ping_ollama_quick, ping_ollama_quick_fresh,
    reset_quick_ping_cache,
};
use crate::managed_child_slot_status;
use crate::now_iso;
use crate::paths::resolve_data_path;
use crate::spawn_managed_child;
use crate::state::{save_settings_file, AppState, Settings};
use crate::terminate_managed_child_slot;
use crate::{update_runtime_diagnostics, update_startup_status};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{info, warn};
use url::Url;
use zip::ZipArchive;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const DEFAULT_RUNTIME_VERSION: &str = "0.40.14";
const MANAGED_RUNTIME_SOURCE: &str = "per_user_zip";
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
    release_name: Option<String>,
    release_notes_summary: Option<String>,
}

/// Only the recommended runtime is pinned (works offline, checksum compiled in).
/// Every other version is resolved from the official GitHub release and verified
/// against its published SHA-256 digest before install.
const WINDOWS_MANIFESTS: [RuntimeManifest; 1] = [RuntimeManifest {
    version: "0.40.14",
    url: "https://github.com/ollama/ollama/releases/download/v0.40.14/ollama-windows-amd64.zip",
    sha256: "65d68cade6cf6375f398b518224f8c8a15e062387f38cf477abfc848de7e7e29",
}];

const GITHUB_TAGS_API: &str = "https://api.github.com/repos/ollama/ollama/git/matching-refs/tags/v";
const RELEASE_DOWNLOAD_PREFIX: &str = "https://github.com/ollama/ollama/releases/download/";
const RUNTIME_ARCHIVE_NAME: &str = "ollama-windows-amd64.zip";
/// How many minor lines (0.40, 0.35, ...) the catalog shows, newest patch each.
const CATALOG_MAX_MINOR_LINES: usize = 6;
const CATALOG_CACHE_TTL_MS: u64 = 6 * 60 * 60 * 1000;
/// Hard cap against disk-fill from a bogus or hijacked download.
const MAX_RUNTIME_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Exact value of the `O=` field in the signer certificate subject.
const OLLAMA_SIGNER_ORGANIZATION: &str = "Ollama Inc.";
/// Env var that carries the path to the PowerShell signature check (never spliced into the script).
#[cfg(target_os = "windows")]
const SIGNATURE_TARGET_ENV: &str = "TRISPR_SIGNATURE_TARGET";
/// Unpacked runtime is ~2.2 GB today; this only stops decompression bombs.
const MAX_RUNTIME_EXTRACTED_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_RUNTIME_ARCHIVE_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, Serialize)]
pub struct OllamaRuntimeInstallProgress {
    pub stage: String,
    pub message: String,
    pub downloaded: Option<u64>,
    pub total: Option<u64>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct OllamaRuntimeVersionInfo {
    pub version: String,
    pub source: String, // "pinned" | "online"
    pub selected: bool,
    pub installed: bool,
    pub recommended: bool,
    /// Newest stable release in the fetched catalog.
    pub latest: bool,
    pub prerelease: bool,
    pub installable: bool,
    pub installable_reason: Option<String>,
    pub release_name: Option<String>,
    pub release_notes_summary: Option<String>,
    /// GitHub `published_at` (ISO 8601), when the release was published.
    pub released_at: Option<String>,
    /// Size of the Windows archive in bytes (GitHub asset size).
    pub size_bytes: Option<u64>,
    /// When the runtime folder was installed on this machine (epoch ms).
    pub installed_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnlineRuntimeVersionCandidate {
    version: String,
    prerelease: bool,
    installable: bool,
    installable_reason: Option<String>,
    release_name: Option<String>,
    release_notes_summary: Option<String>,
    released_at: Option<String>,
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnlineCatalogCache {
    fetched_at_ms: u64,
    versions: Vec<OnlineRuntimeVersionCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeArchiveMetadata {
    version: String,
    sha256: String,
    url: String,
    release_name: Option<String>,
    release_notes_summary: Option<String>,
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
    /// `runtime_target_version` has a folder with `ollama.exe` in the managed runtime root.
    pub target_installed: bool,
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
            release_name: None,
            release_notes_summary: None,
        })
}

fn checksum_entry_matches_file_name(candidate: &str, file_name: &str) -> bool {
    let candidate = candidate.trim().trim_start_matches('*');
    if candidate.eq_ignore_ascii_case(file_name) {
        return true;
    }
    let normalized_name = Path::new(candidate)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(candidate);
    normalized_name.eq_ignore_ascii_case(file_name)
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
        if tokens[1..]
            .iter()
            .any(|token| checksum_entry_matches_file_name(token, file_name))
        {
            let hash = tokens[0].trim().trim_start_matches('*');
            if hash.len() == 64 && hash.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Some(hash.to_ascii_lowercase());
            }
        }
    }
    None
}

fn asset_digest_sha256(asset: &serde_json::Value) -> Option<String> {
    let digest = asset.get("digest")?.as_str()?.trim();
    let digest = digest.strip_prefix("sha256:").unwrap_or(digest);
    if digest.len() == 64 && digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(digest.to_ascii_lowercase())
    } else {
        None
    }
}

fn summarize_release_notes(body: Option<&str>) -> Option<String> {
    let body = body?.trim();
    if body.is_empty() {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let line = line
            .trim_start_matches(|c: char| c == '-' || c == '*' || c == '•' || c == '>' || c == '#')
            .trim();
        if line.is_empty() {
            continue;
        }

        let lowered = line.to_ascii_lowercase();
        if lowered.starts_with("full changelog")
            || lowered.starts_with("see full changelog")
            || lowered.starts_with("changelog")
        {
            continue;
        }

        parts.push(line.to_string());
        if parts.len() >= 3 {
            break;
        }
    }

    if parts.is_empty() {
        return None;
    }

    let mut summary = parts.join(" · ");
    const MAX_LEN: usize = 240;
    if summary.chars().count() > MAX_LEN {
        summary = summary.chars().take(MAX_LEN.saturating_sub(1)).collect();
        summary.push('…');
    }
    Some(summary)
}

fn runtime_archive_metadata_path(archive: &Path) -> PathBuf {
    let mut metadata_path = archive.to_path_buf();
    metadata_path.set_extension("json");
    metadata_path
}

fn runtime_manifest_from_metadata(metadata: RuntimeArchiveMetadata) -> RuntimeManifestResolved {
    RuntimeManifestResolved {
        version: metadata.version,
        url: metadata.url,
        sha256: metadata.sha256,
        release_name: metadata.release_name,
        release_notes_summary: metadata.release_notes_summary,
    }
}

fn load_runtime_archive_metadata(archive: &Path) -> Option<RuntimeArchiveMetadata> {
    let metadata_path = runtime_archive_metadata_path(archive);
    let text = fs::read_to_string(metadata_path).ok()?;
    serde_json::from_str::<RuntimeArchiveMetadata>(&text).ok()
}

fn write_runtime_archive_metadata(
    archive: &Path,
    manifest: &RuntimeManifestResolved,
) -> Result<(), String> {
    let metadata_path = runtime_archive_metadata_path(archive);
    let metadata = RuntimeArchiveMetadata {
        version: manifest.version.clone(),
        sha256: manifest.sha256.clone(),
        url: manifest.url.clone(),
        release_name: manifest.release_name.clone(),
        release_notes_summary: manifest.release_notes_summary.clone(),
    };
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|e| format!("Failed to serialize runtime archive metadata: {}", e))?;
    fs::write(&metadata_path, json).map_err(|e| {
        format!(
            "Failed to write runtime archive metadata '{}': {}",
            metadata_path.display(),
            e
        )
    })
}

fn infer_runtime_version_from_archive_path(archive: &Path) -> Option<String> {
    let file_name = archive.file_name()?.to_str()?.trim();
    let stem = file_name.strip_suffix(".zip").unwrap_or(file_name);
    let version = stem.strip_prefix("ollama-windows-amd64-v")?;
    let version = version.trim().trim_start_matches('v');
    if version.is_empty() {
        None
    } else {
        Some(version.to_string())
    }
}

fn fetch_online_manifest(version: &str) -> Result<RuntimeManifestResolved, String> {
    fetch_online_manifest_from(&GithubApi::official(), version)
}

fn fetch_online_manifest_from(
    api: &GithubApi,
    version: &str,
) -> Result<RuntimeManifestResolved, String> {
    if !is_safe_runtime_version(version) {
        return Err(format!("Unsafe runtime version string '{}'.", version));
    }
    let tag = format!("v{}", version);
    let release_url = format!("{}/tags/{}", api.releases_url, tag);
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(3))
        .timeout_read(Duration::from_secs(5))
        .build();

    let release_json = agent
        .get(&release_url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| describe_github_error(&format!("reading release {}", tag), &e))?
        .into_json::<serde_json::Value>()
        .map_err(|e| {
            format!(
                "GitHub's answer for release '{}' could not be read (cut off or not JSON): {}",
                tag, e
            )
        })?;

    let assets = release_json
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Release '{}' does not expose assets.", tag))?;
    let release_name = release_json
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());
    let release_notes_summary =
        summarize_release_notes(release_json.get("body").and_then(|v| v.as_str()));

    let mut archive_url: Option<String> = None;
    let mut archive_sha256: Option<String> = None;
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
            archive_sha256 = asset_digest_sha256(asset);
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

    let sha256 = if let Some(digest) = archive_sha256 {
        digest
    } else {
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

        checksum.ok_or_else(|| {
            format!(
                "Release '{}' is missing a parseable checksum for ollama-windows-amd64.zip.",
                tag
            )
        })?
    };

    Ok(RuntimeManifestResolved {
        version: version.to_string(),
        url: archive_url,
        sha256,
        release_name,
        release_notes_summary,
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

fn resolve_runtime_archive_manifest_for_install(
    archive: &Path,
    archive_digest: &str,
) -> Result<RuntimeManifestResolved, String> {
    if let Some(metadata) = load_runtime_archive_metadata(archive) {
        if !metadata.version.trim().is_empty()
            && metadata.sha256.eq_ignore_ascii_case(archive_digest)
        {
            if crate::state::diagnostic_logging_enabled() {
                info!(
                    "[ollama.runtime] install manifest resolved from sidecar archive={} version={}",
                    archive.display(),
                    metadata.version
                );
            }
            return Ok(runtime_manifest_from_metadata(metadata));
        }
    }

    if let Some(manifest) = WINDOWS_MANIFESTS
        .iter()
        .find(|m| m.sha256.eq_ignore_ascii_case(archive_digest))
    {
        if crate::state::diagnostic_logging_enabled() {
            info!(
                "[ollama.runtime] install manifest resolved from pinned checksum archive={} version={}",
                archive.display(),
                manifest.version
            );
        }
        return Ok(RuntimeManifestResolved {
            version: manifest.version.to_string(),
            url: manifest.url.to_string(),
            sha256: manifest.sha256.to_string(),
            release_name: None,
            release_notes_summary: None,
        });
    }

    let inferred_version = infer_runtime_version_from_archive_path(archive).ok_or_else(|| {
        "Archive checksum is not in the pinned runtime manifest and the version could not be inferred from the file name. Use a runtime archive named ollama-windows-amd64-vX.Y.Z.zip or download it through the app.".to_string()
    })?;

    let manifest = fetch_online_manifest(&inferred_version).map_err(|e| {
        format!(
            "Archive checksum is not in the pinned runtime manifest and could not be verified against official Ollama release '{}': {}",
            inferred_version,
            e
        )
    })?;

    if !manifest.sha256.eq_ignore_ascii_case(archive_digest) {
        return Err(format!(
            "Archive checksum does not match official Ollama release {}. Expected {}, got {}",
            inferred_version, manifest.sha256, archive_digest
        ));
    }
    if crate::state::diagnostic_logging_enabled() {
        info!(
            "[ollama.runtime] install manifest resolved from official release archive={} version={}",
            archive.display(),
            inferred_version
        );
    }

    Ok(manifest)
}

fn classify_online_release_installability(assets: &[serde_json::Value]) -> (bool, Option<String>) {
    let mut has_windows_archive = false;
    let mut has_archive_digest = false;
    let mut has_checksum_asset = false;

    for asset in assets {
        let Some(name) = asset.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if name.eq_ignore_ascii_case("ollama-windows-amd64.zip") {
            has_windows_archive = true;
            has_archive_digest = asset_digest_sha256(asset).is_some();
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".sha256")
            || lower.contains("sha256sum")
            || lower.contains("checksums")
            || lower.contains("sha256")
        {
            has_checksum_asset = true;
        }
    }

    if !has_windows_archive {
        return (
            false,
            Some("Missing ollama-windows-amd64.zip asset.".to_string()),
        );
    }
    if !has_archive_digest && !has_checksum_asset {
        return (
            false,
            Some("Missing checksum asset or asset digest for runtime verification.".to_string()),
        );
    }
    (true, None)
}

fn classify_online_release_installability_with_checksum_probe(
    agent: &ureq::Agent,
    assets: &[serde_json::Value],
) -> (bool, Option<String>) {
    let (basic_ok, basic_reason) = classify_online_release_installability(assets);
    if !basic_ok {
        return (false, basic_reason);
    }

    if assets.iter().any(|asset| {
        asset
            .get("name")
            .and_then(|v| v.as_str())
            .map(|name| name.eq_ignore_ascii_case("ollama-windows-amd64.zip"))
            .unwrap_or(false)
            && asset_digest_sha256(asset).is_some()
    }) {
        return (true, None);
    }

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
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".sha256")
            || lower.contains("sha256sum")
            || lower.contains("checksums")
            || lower.contains("sha256")
        {
            checksum_urls.push(url);
        }
    }

    if checksum_urls.is_empty() {
        return (
            false,
            Some("Missing checksum asset for runtime verification.".to_string()),
        );
    }

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
        if parse_sha256_from_text(&body, "ollama-windows-amd64.zip").is_some() {
            return (true, None);
        }
    }

    (
        false,
        Some(
            "Checksum assets found, but no parseable hash for ollama-windows-amd64.zip."
                .to_string(),
        ),
    )
}

/// Strict `X.Y.Z` (optionally `v`-prefixed). Pre-release tags like `0.1.32-rc1` are rejected.
pub(crate) fn parse_stable_semver(raw: &str) -> Option<(u32, u32, u32)> {
    let mut parts = raw.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// Versions end up in folder names and download URLs, so only plain release strings pass.
fn is_safe_runtime_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 32
        && !version.contains("..")
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// Newest patch of each of the `max_lines` newest minor lines, newest first.
/// For 0.x software the minor number is the "major" release line.
fn select_major_line_versions(tags: &[String], max_lines: usize) -> Vec<String> {
    let mut newest: HashMap<(u32, u32), (u32, u32, u32)> = HashMap::new();
    for tag in tags {
        if let Some(version) = parse_stable_semver(tag) {
            let entry = newest.entry((version.0, version.1)).or_insert(version);
            if version > *entry {
                *entry = version;
            }
        }
    }
    let mut lines: Vec<(u32, u32, u32)> = newest.into_values().collect();
    lines.sort_by(|a, b| b.cmp(a));
    lines
        .into_iter()
        .take(max_lines)
        .map(|(major, minor, patch)| format!("{}.{}.{}", major, minor, patch))
        .collect()
}

/// Downloads are only accepted from the official Ollama release path of the requested version.
fn validate_runtime_download_url(url: &str, version: &str) -> Result<(), String> {
    if !is_safe_runtime_version(version) {
        return Err(format!("Unsafe runtime version string '{}'.", version));
    }
    let expected_prefix = format!("{}v{}/", RELEASE_DOWNLOAD_PREFIX, version);
    if url.starts_with(&expected_prefix)
        && url.ends_with(&format!("/{}", RUNTIME_ARCHIVE_NAME))
        && !url.contains("..")
        && !url.contains('@')
    {
        Ok(())
    } else {
        Err(format!(
            "Refusing runtime download from '{}': only {}v{}/{} is allowed.",
            url, RELEASE_DOWNLOAD_PREFIX, version, RUNTIME_ARCHIVE_NAME
        ))
    }
}

fn catalog_agent() -> ureq::Agent {
    ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .build()
}

/// GitHub API roots. Injectable so tests can stand in for GitHub; the app uses `official()`.
#[derive(Debug, Clone)]
struct GithubApi {
    /// Lists `refs/tags/v*`.
    tags_url: String,
    /// `<releases_url>/tags/v<version>` is one release.
    releases_url: String,
}

impl GithubApi {
    fn official() -> Self {
        Self {
            tags_url: GITHUB_TAGS_API.to_string(),
            releases_url: GITHUB_RELEASES_API.to_string(),
        }
    }
}

fn now_epoch_secs() -> u64 {
    crate::util::now_ms() / 1000
}

/// "45 s", "12 min", "1 h 5 min": a wait, rounded up to whole minutes from one minute on.
fn format_wait(secs: u64) -> String {
    if secs < 60 {
        return format!("{} s", secs.max(1));
    }
    let minutes = secs.div_ceil(60);
    if minutes < 60 {
        return format!("{} min", minutes);
    }
    match minutes % 60 {
        0 => format!("{} h", minutes / 60),
        rest => format!("{} h {} min", minutes / 60, rest),
    }
}

/// Seconds GitHub asks us to wait: `Retry-After`, otherwise `X-RateLimit-Reset` (epoch
/// seconds) minus now. `None` when neither header says.
fn github_wait_secs(
    retry_after: Option<&str>,
    rate_limit_reset: Option<&str>,
    now_epoch_s: u64,
) -> Option<u64> {
    if let Some(secs) = retry_after.and_then(|v| v.trim().parse::<u64>().ok()) {
        return Some(secs);
    }
    let reset = rate_limit_reset.and_then(|v| v.trim().parse::<u64>().ok())?;
    Some(reset.saturating_sub(now_epoch_s))
}

/// One sentence the user can act on for a failed GitHub API call (`what` completes
/// "... while <what>"): rate limit with the wait, blocked, unreachable, or server trouble.
fn describe_github_error(what: &str, error: &ureq::Error) -> String {
    describe_github_error_at(what, error, now_epoch_secs())
}

fn describe_github_error_at(what: &str, error: &ureq::Error, now_epoch_s: u64) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let retry_after = response.header("retry-after");
            let limit_exhausted =
                response.header("x-ratelimit-remaining").map(str::trim) == Some("0");
            if *code == 429 || (*code == 403 && (limit_exhausted || retry_after.is_some())) {
                let wait = github_wait_secs(
                    retry_after,
                    response.header("x-ratelimit-reset"),
                    now_epoch_s,
                );
                let advice = match wait {
                    Some(0) => "Try again in a moment.".to_string(),
                    Some(secs) => format!("Try again in {}.", format_wait(secs)),
                    None => "Try again in about an hour.".to_string(),
                };
                format!(
                    "GitHub rate limit reached while {} (without a token GitHub allows 60 requests per hour). {}",
                    what, advice
                )
            } else if *code == 403 {
                format!(
                    "GitHub refused the request while {} (HTTP 403). A proxy or firewall may be blocking api.github.com.",
                    what
                )
            } else if *code == 404 {
                format!("GitHub has no such release (HTTP 404) while {}.", what)
            } else if *code >= 500 {
                format!(
                    "GitHub is having trouble right now (HTTP {}) while {}. Try again later.",
                    code, what
                )
            } else {
                format!("GitHub answered HTTP {} while {}.", code, what)
            }
        }
        ureq::Error::Transport(transport) => format!(
            "Could not reach GitHub while {}: {}. Check the internet connection or proxy settings.",
            what, transport
        ),
    }
}

/// A release lookup that failed. `fatal`: asking GitHub more questions is pointless right now
/// (rate limit, unreachable), so the remaining lookups are skipped.
struct ReleaseLookupFailure {
    message: String,
    fatal: bool,
}

fn candidate_from_release_json(
    agent: &ureq::Agent,
    version: &str,
    item: &serde_json::Value,
) -> OnlineRuntimeVersionCandidate {
    let assets = item
        .get("assets")
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);
    let (installable, installable_reason) =
        classify_online_release_installability_with_checksum_probe(agent, assets);
    let size_bytes = assets
        .iter()
        .find(|asset| {
            asset
                .get("name")
                .and_then(|v| v.as_str())
                .map(|name| name.eq_ignore_ascii_case(RUNTIME_ARCHIVE_NAME))
                .unwrap_or(false)
        })
        .and_then(|asset| asset.get("size").and_then(|v| v.as_u64()));
    OnlineRuntimeVersionCandidate {
        version: version.to_string(),
        prerelease: item
            .get("prerelease")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        installable,
        installable_reason,
        release_name: item
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string()),
        release_notes_summary: summarize_release_notes(item.get("body").and_then(|v| v.as_str())),
        released_at: item
            .get("published_at")
            .and_then(|v| v.as_str())
            .map(|value| value.to_string()),
        size_bytes,
    }
}

fn fetch_release_candidate(
    api: &GithubApi,
    agent: &ureq::Agent,
    version: &str,
) -> Result<OnlineRuntimeVersionCandidate, ReleaseLookupFailure> {
    let url = format!("{}/tags/v{}", api.releases_url, version);
    let what = format!("reading release v{}", version);
    let response = agent
        .get(&url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| ReleaseLookupFailure {
            fatal: match &e {
                ureq::Error::Transport(_) => true,
                ureq::Error::Status(code, _) => matches!(*code, 403 | 429),
            },
            message: describe_github_error(&what, &e),
        })?;
    let item = response
        .into_json::<serde_json::Value>()
        .map_err(|e| ReleaseLookupFailure {
            fatal: false,
            message: format!(
                "GitHub's answer for release v{} could not be read (cut off or not JSON): {}",
                version, e
            ),
        })?;
    Ok(candidate_from_release_json(agent, version, &item))
}

/// What one catalog fetch produced.
struct CatalogFetch {
    versions: Vec<OnlineRuntimeVersionCandidate>,
    /// Every wanted version was resolved. A partial catalog is shown but never cached, so the
    /// next refresh asks GitHub again instead of serving gaps for hours.
    complete: bool,
}

/// All release lookups together may not take longer than this (each has its own timeout, and
/// a black-holed GitHub would otherwise cost one timeout per version).
const CATALOG_FETCH_BUDGET: Duration = Duration::from_secs(60);

fn unresolved_candidate(version: String, reason: String) -> OnlineRuntimeVersionCandidate {
    OnlineRuntimeVersionCandidate {
        version,
        prerelease: false,
        installable: false,
        installable_reason: Some(reason),
        release_name: None,
        release_notes_summary: None,
        released_at: None,
        size_bytes: None,
    }
}

/// Lightweight online catalog: one tag listing (~250 KB) plus one release lookup per
/// shown version. `extra_versions` (installed / selected) are always resolved too.
/// Fails (instead of returning a catalog full of "not installable" entries) when GitHub
/// answers nothing usable.
fn fetch_online_catalog_from(
    api: &GithubApi,
    extra_versions: &[String],
) -> Result<CatalogFetch, String> {
    let agent = catalog_agent();
    let refs = agent
        .get(&api.tags_url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| describe_github_error("listing Ollama releases", &e))?
        .into_json::<serde_json::Value>()
        .map_err(|e| {
            format!(
                "GitHub's release list could not be read (cut off or not JSON): {}. Try again.",
                e
            )
        })?;
    let Some(items) = refs.as_array() else {
        let said = refs
            .get("message")
            .and_then(|v| v.as_str())
            .map(|m| m.chars().take(160).collect::<String>());
        return Err(match said {
            Some(message) => format!("GitHub did not return a release list: {}", message),
            None => "GitHub did not return a release list (unexpected answer).".to_string(),
        });
    };
    let tags: Vec<String> = items
        .iter()
        .filter_map(|item| item.get("ref").and_then(|v| v.as_str()))
        .map(|name| name.trim_start_matches("refs/tags/").to_string())
        .collect();
    if tags.is_empty() {
        return Err("GitHub returned no Ollama release tags.".to_string());
    }

    let mut wanted = select_major_line_versions(&tags, CATALOG_MAX_MINOR_LINES);
    for extra in extra_versions {
        let extra = extra.trim().trim_start_matches('v').to_string();
        if is_safe_runtime_version(&extra) && !wanted.contains(&extra) {
            wanted.push(extra);
        }
    }
    if wanted.is_empty() {
        return Err("GitHub's tag list contains no stable Ollama release.".to_string());
    }

    let started = Instant::now();
    let mut out = Vec::with_capacity(wanted.len());
    let mut failures = 0usize;
    let mut first_failure: Option<String> = None;
    let mut stop_reason: Option<String> = None;
    for version in wanted {
        if stop_reason.is_none() && started.elapsed() > CATALOG_FETCH_BUDGET {
            stop_reason =
                Some("GitHub answered too slowly; this version was not checked.".to_string());
        }
        if let Some(reason) = stop_reason.clone() {
            failures += 1;
            first_failure.get_or_insert_with(|| reason.clone());
            out.push(unresolved_candidate(version, reason));
            continue;
        }
        match fetch_release_candidate(api, &agent, &version) {
            Ok(candidate) => out.push(candidate),
            Err(failure) => {
                failures += 1;
                first_failure.get_or_insert_with(|| failure.message.clone());
                if failure.fatal {
                    stop_reason = Some(failure.message.clone());
                }
                out.push(unresolved_candidate(version, failure.message));
            }
        }
    }
    if failures == out.len() {
        return Err(first_failure
            .unwrap_or_else(|| "GitHub returned no usable Ollama release.".to_string()));
    }
    Ok(CatalogFetch {
        versions: out,
        complete: failures == 0,
    })
}

fn catalog_cache_path(app: &AppHandle) -> PathBuf {
    resolve_runtime_cache_dir(app).join("online-catalog.json")
}

/// The cached catalog, or `None` when the file is missing, empty, cut off or not a catalog.
/// Entries with a version string that could not be a release are dropped (the cache is just
/// a file on disk; its content is not trusted).
fn load_catalog_cache_at(path: &Path) -> Option<OnlineCatalogCache> {
    let raw = fs::read_to_string(path).ok()?;
    let mut cache: OnlineCatalogCache = serde_json::from_str(&raw).ok()?;
    cache
        .versions
        .retain(|candidate| is_safe_runtime_version(&candidate.version));
    Some(cache)
}

fn load_catalog_cache(app: &AppHandle) -> Option<OnlineCatalogCache> {
    load_catalog_cache_at(&catalog_cache_path(app))
}

/// Writes next to the target and renames, so a crash or a second writer never leaves a
/// half-written cache behind.
fn save_catalog_cache_at(path: &Path, cache: &OnlineCatalogCache) -> Result<(), String> {
    let raw = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        crate::util::now_ms()
    ));
    fs::write(&tmp, raw).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("replace {}: {}", path.display(), e)
    })
}

/// A cache stamped further in the future than clock skew explains is distrusted: it would
/// otherwise count as "fresh" until the clock catches up, however far ahead it points.
const CATALOG_CLOCK_SKEW_MS: u64 = 5 * 60 * 1000;
/// A forced refresh right behind another one reuses it (double click, two windows): GitHub
/// allows only 60 anonymous requests per hour and one catalog costs about 8.
const CATALOG_FORCE_REUSE_MS: u64 = 15_000;

fn catalog_cache_is_fresh(cache: &OnlineCatalogCache, now_ms: u64, max_age_ms: u64) -> bool {
    !cache.versions.is_empty()
        && cache.fetched_at_ms <= now_ms.saturating_add(CATALOG_CLOCK_SKEW_MS)
        && now_ms.saturating_sub(cache.fetched_at_ms) < max_age_ms
}

/// One catalog refresh at a time: concurrent callers queue up and then find the cache the
/// first one wrote, instead of each spending GitHub requests.
static CATALOG_FLIGHT: Mutex<()> = Mutex::new(());

/// The catalog the UI shows after a refresh: the cache while it is fresh (unless `force`),
/// otherwise GitHub. A failed refresh never loses what is on disk and never panics; a
/// complete one is cached. Errors name the cause (rate limit with the wait, offline, ...).
fn refresh_online_catalog(
    cache_path: &Path,
    api: &GithubApi,
    extra_versions: &[String],
    force: bool,
    now_ms: u64,
) -> Result<Vec<OnlineRuntimeVersionCandidate>, String> {
    let _flight = CATALOG_FLIGHT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cache = load_catalog_cache_at(cache_path);
    if let Some(cache) = &cache {
        let max_age = if force {
            CATALOG_FORCE_REUSE_MS
        } else {
            CATALOG_CACHE_TTL_MS
        };
        if catalog_cache_is_fresh(cache, now_ms, max_age) {
            return Ok(cache.versions.clone());
        }
    }
    let have_cached_list = cache.as_ref().is_some_and(|c| !c.versions.is_empty());
    let fetch = fetch_online_catalog_from(api, extra_versions).map_err(|e| {
        if have_cached_list {
            format!("{} Showing the last cached list.", e)
        } else {
            e
        }
    })?;
    if fetch.complete {
        let cache = OnlineCatalogCache {
            fetched_at_ms: crate::util::now_ms(),
            versions: fetch.versions.clone(),
        };
        if let Err(error) = save_catalog_cache_at(cache_path, &cache) {
            // The list in hand is still good; only the next start has to ask again.
            warn!(
                "[ollama.runtime] could not cache the version catalog: {}",
                error
            );
        }
    }
    Ok(fetch.versions)
}

/// Runtime folders that contain an `ollama.exe`, keyed by version (folder name) with
/// the folder's modification time in epoch ms (= when it was installed).
fn scan_installed_runtime_versions(runtime_root: &Path) -> HashMap<String, u64> {
    let mut found = HashMap::new();
    let Ok(entries) = fs::read_dir(runtime_root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') || !path.is_dir() || !is_safe_runtime_version(name) {
            continue;
        }
        if find_file_recursive(&path, "ollama.exe").is_none() {
            continue;
        }
        let installed_at = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        found.insert(name.to_string(), installed_at);
    }
    found
}

fn merge_online_candidate_into_version_entry(
    entry: &mut OllamaRuntimeVersionInfo,
    candidate: &OnlineRuntimeVersionCandidate,
) {
    if entry.release_name.is_none() {
        entry.release_name = candidate.release_name.clone();
    }
    if entry.release_notes_summary.is_none() {
        entry.release_notes_summary = candidate.release_notes_summary.clone();
    }
    if entry.released_at.is_none() {
        entry.released_at = candidate.released_at.clone();
    }
    if entry.size_bytes.is_none() {
        entry.size_bytes = candidate.size_bytes;
    }
    if entry.source != "pinned" {
        entry.installable = candidate.installable;
        entry.installable_reason = candidate.installable_reason.clone();
        entry.source = "online".to_string();
    }
}

/// Resolves the root directory for the managed Ollama runtime installation.
///
/// This intentionally does NOT use paths.rs because:
/// 1. On Windows, the runtime lives under %LOCALAPPDATA%\Trispr Flow\ollama-runtime
///    to keep the path short and predictable (Ollama is sensitive to long paths).
///    Legacy installs may still have %LOCALAPPDATA%\TrisprFlow\ollama-runtime —
///    see find_existing_runtime_root() for the migration fallback.
/// 2. paths.rs uses Tauri's app_data_dir / app_config_dir which resolve to an
///    app-identifier-based subdirectory that differs from what we need here.
fn resolve_runtime_root(app: &AppHandle) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            let path = PathBuf::from(local_app_data)
                .join("Trispr Flow")
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
    let state = app.state::<AppState>();
    update_startup_status(app, state.inner(), |status| {
        status.ollama_ready = ok;
        if ok {
            status.ollama_starting = false;
        }
    });
    let _ = app.emit(
        "ollama:runtime-health",
        OllamaRuntimeHealth {
            ok,
            endpoint,
            models_count,
        },
    );
}

fn update_ollama_runtime_diagnostics(
    app: &AppHandle,
    settings: &Settings,
    detected: bool,
    spawn_stage: &str,
    managed_pid: Option<u32>,
    reachable: bool,
    last_error: Option<String>,
) {
    let state = app.state::<AppState>();
    if reachable || matches!(spawn_stage, "ready" | "running_externally") {
        // Reset exponential backoff so diagnostics polls resume immediately
        crate::OLLAMA_DIAG_FAIL_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
        crate::OLLAMA_DIAG_NEXT_MS.store(0, std::sync::atomic::Ordering::Relaxed);
        update_startup_status(app, state.inner(), |status| {
            if status.ollama_starting {
                info!("Ollama startup status synced to ready via runtime diagnostics");
            }
            status.ollama_ready = true;
            status.ollama_starting = false;
        });
    }
    update_runtime_diagnostics(app, state.inner(), |diagnostics| {
        diagnostics.ollama.configured_path = settings.providers.ollama.runtime_path.clone();
        diagnostics.ollama.detected = detected;
        diagnostics.ollama.spawn_stage = spawn_stage.to_string();
        diagnostics.ollama.last_error = last_error.unwrap_or_default();
        diagnostics.ollama.managed_pid = managed_pid;
        diagnostics.ollama.endpoint = settings.providers.ollama.endpoint.clone();
        diagnostics.ollama.reachable = reachable;
    });
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
    sha256_reader(&mut file)
}

fn sha256_reader<R: Read>(reader: &mut R) -> Result<String, String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 64];
    let mut processed_since_pause = 0u64;
    loop {
        let read = reader
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

/// Deepest folder level `find_file_recursive` descends to. A runtime is 2-3 levels deep.
const FIND_MAX_DEPTH: usize = 8;
/// Most directory entries `find_file_recursive` looks at before it gives up.
const FIND_MAX_ENTRIES: usize = 100_000;

/// First file called `target_name` (case-insensitive) below `root`. Bounded on purpose: a
/// junction or symlink cycle in the runtime folder (the user can create one with `mklink /J`)
/// must not hang the UI, which scans these folders on every refresh, so links are not followed
/// and depth and entry count are capped. An unreadable subfolder is skipped, not fatal.
fn find_file_recursive(root: &Path, target_name: &str) -> Option<PathBuf> {
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((path, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > FIND_MAX_ENTRIES {
                return None;
            }
            let entry_path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            // Links are never descended into (a cycle would never end); a link that itself
            // is the wanted file still counts.
            if file_type.is_dir() && !file_type.is_symlink() {
                if depth < FIND_MAX_DEPTH {
                    stack.push((entry_path, depth + 1));
                }
                continue;
            }
            if file_type.is_symlink() && entry_path.is_dir() {
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
    // The path alone is not an identity: an updater may replace the file in place (the Ollama
    // desktop app does), so size and modification time are part of the key.
    let cache_key = {
        let stamp = fs::metadata(binary_path)
            .ok()
            .map(|meta| {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_nanos());
                format!("{}|{}", meta.len(), modified)
            })
            .unwrap_or_default();
        format!("{}|{}", binary_path.to_string_lossy(), stamp)
    };

    if let Ok(guard) = cache.lock() {
        if let Some(version) = guard.get(&cache_key) {
            return version.clone();
        }
    }

    let parsed = probe_binary_version(binary_path, VERSION_PROBE_TIMEOUT);
    // Never cache a miss: one failed probe would otherwise pin "unknown" for this path
    // and silently disable the version-switch check until the app restarts.
    if !parsed.is_empty() {
        if let Ok(mut guard) = cache.lock() {
            guard.insert(cache_key, parsed.clone());
        }
    }
    parsed
}

/// Runs `<binary> --version` (at most `timeout`) and returns the binary's own version, or an
/// empty string when it did not say: could not run, no readable answer, or hung.
fn probe_binary_version(binary_path: &Path, timeout: Duration) -> String {
    // Point the client at a dead port so it never reports a running server's version.
    let mut version_cmd = Command::new(binary_path);
    version_cmd
        .arg("--version")
        .env("OLLAMA_HOST", "127.0.0.1:1");
    #[cfg(target_os = "windows")]
    {
        version_cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let text = match run_with_timeout(&mut version_cmd, timeout) {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            format!("{}\n{}", stdout.trim(), stderr.trim())
        }
        Err(RunError::TimedOut) => {
            warn!(
                "[ollama.runtime] '{} --version' did not answer within {} s",
                binary_path.display(),
                timeout.as_secs()
            );
            String::new()
        }
        Err(RunError::Spawn(_)) => String::new(),
    };
    parse_version_output(&text)
}

/// Extracts the binary's own version from `ollama --version` output.
///
/// With a server running, the client prints the SERVER version first
/// ("ollama version is 0.30.4") and its own version in a warning line
/// ("Warning: client version is 0.40.7"). The client line wins.
fn parse_version_output(text: &str) -> String {
    let version_after = |marker: &str| -> Option<String> {
        text.lines().find_map(|line| {
            let lowered = line.to_ascii_lowercase();
            let idx = lowered.find(marker)?;
            line[idx + marker.len()..]
                .split_whitespace()
                .next()
                .map(|token| token.trim().trim_start_matches('v').to_string())
                .filter(|token| token.chars().any(|c| c.is_ascii_digit()))
        })
    };
    version_after("client version is")
        .or_else(|| version_after("version is"))
        .unwrap_or_default()
}

/// Version to report next to `binary_path`: the BINARY's own, never the one stored for another
/// path. The stored version counts only for the path it was recorded with; a managed binary
/// otherwise gets the version of its folder; anything else is "unknown" (empty).
fn detect_runtime_version_hint(
    settings: &Settings,
    source: &str,
    binary_path: &Path,
    managed_root: Option<&Path>,
) -> String {
    let ollama = &settings.providers.ollama;
    let stored_version = ollama.runtime_version.trim();
    if !stored_version.is_empty() && Path::new(ollama.runtime_path.trim()) == binary_path {
        return stored_version.to_string();
    }
    if source == "per_user_zip" {
        return managed_root
            .and_then(|root| managed_runtime_version_of(root, binary_path))
            .unwrap_or_default();
    }
    // Another binary's version is never reported next to this one.
    String::new()
}

/// Managed runtimes live in `<root>/<version>/…/ollama.exe`, and the install only swaps a
/// folder in after the binary inside reported exactly that version. So the folder name is the
/// binary's version without spawning it (detect has a 2.5 s budget in the UI).
fn managed_runtime_version_of(managed_root: &Path, binary: &Path) -> Option<String> {
    let mut parts = binary.strip_prefix(managed_root).ok()?.components();
    let version = parts.next()?.as_os_str().to_str()?;
    parts.next()?; // the binary lies inside the version folder, it is not the folder itself
    (!version.starts_with('.') && is_safe_runtime_version(version)).then(|| version.to_string())
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

/// Directory holding the managed runtime versions (`<root>/<version>/ollama.exe`): the
/// current root, or the legacy `TrisprFlow` one for installs that were never migrated.
fn managed_runtime_root(app: &AppHandle) -> Option<PathBuf> {
    let runtime_root = resolve_runtime_root(app);
    if runtime_root.is_dir() {
        return Some(runtime_root);
    }
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let legacy_root = PathBuf::from(local_app_data)
        .join("TrisprFlow")
        .join("ollama-runtime");
    legacy_root.is_dir().then_some(legacy_root)
}

/// Phase 1 of `detect_ollama_runtime` (filesystem only): the runtime the UI reports as the
/// CURRENT one, as `(source, binary, ITS version)`.
///
/// Unlike `select_runtime_binary_in` (what the next start launches) this keeps the stored
/// `runtime_path` first on purpose: the UI compares the reported version with
/// `runtime_target_version` to offer "Switch to X". Reporting the target here would hide that
/// action while the old server still answers. Whether the target is installed is reported
/// separately (`target_installed`).
fn detect_runtime_in(
    settings: &Settings,
    managed_root: Option<&Path>,
) -> Option<(String, PathBuf, String)> {
    let configured = settings.providers.ollama.runtime_path.trim();
    let configured_binary = (!configured.is_empty())
        .then(|| PathBuf::from(configured))
        .filter(|path| path.exists());
    let path = configured_binary
        .or_else(|| managed_root.and_then(|root| find_managed_runtime_binary_in(root, settings)))?;
    let source = MANAGED_RUNTIME_SOURCE.to_string();
    let version = detect_runtime_version_hint(settings, &source, &path, managed_root);
    Some((source, path, version))
}

/// `<runtime_root>/<runtime_target_version>/ollama.exe`, when that exact version is installed.
fn target_runtime_binary_in(
    runtime_root: &Path,
    settings: &crate::state::Settings,
) -> Option<PathBuf> {
    let target = settings.providers.ollama.runtime_target_version.trim();
    if target.is_empty() || !is_safe_runtime_version(target) {
        return None;
    }
    let dir = runtime_root.join(target);
    if !dir.is_dir() {
        return None;
    }
    find_file_recursive(&dir, "ollama.exe")
}

fn find_managed_runtime_binary_in(
    runtime_root: &Path,
    settings: &crate::state::Settings,
) -> Option<PathBuf> {
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
        if !is_safe_runtime_version(&version) {
            continue;
        }
        let candidate_dir = runtime_root.join(version);
        if !candidate_dir.is_dir() {
            continue;
        }
        if let Some(path) = find_file_recursive(&candidate_dir, "ollama.exe") {
            return Some(path);
        }
    }

    let mut discovered: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(entries) = fs::read_dir(runtime_root) {
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
            // `.staging-*` / `.old-*` are install leftovers, never a runtime to launch.
            if name.starts_with('.') {
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
    select_runtime_binary_in(settings, managed_runtime_root(app).as_deref())
}

/// Which binary to launch: the selected version (`runtime_target_version`) wins whenever its
/// folder is installed; the stored `runtime_path` only remembers what ran last and must not pin
/// the old build after the user switched version.
fn select_runtime_binary_in(
    settings: &crate::state::Settings,
    managed_root: Option<&Path>,
) -> Result<(PathBuf, String), String> {
    let source = MANAGED_RUNTIME_SOURCE.to_string();
    if let Some(target) = managed_root.and_then(|root| target_runtime_binary_in(root, settings)) {
        return Ok((target, source));
    }
    let configured = settings.providers.ollama.runtime_path.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.exists() {
            return Ok((path, source));
        }
    }
    if let Some(managed) =
        managed_root.and_then(|root| find_managed_runtime_binary_in(root, settings))
    {
        return Ok((managed, source));
    }
    Err(
        "Managed local runtime is selected, but no local Ollama runtime is installed yet. Use 'Install local runtime'."
            .to_string(),
    )
}

/// The version to store next to `binary_path` in the settings: always the BINARY's own.
/// A server answering on the endpoint may run from another binary (an external Ollama), and
/// its version must never be paired with this path. When the binary's version is unknown the
/// stored one is kept only if it was stored for this very path; otherwise it is left empty.
fn version_to_record_for_binary(
    settings: &crate::state::Settings,
    binary_path: &Path,
    binary_version: &str,
) -> String {
    let binary_version = binary_version.trim();
    if !binary_version.is_empty() {
        return binary_version.to_string();
    }
    let ollama = &settings.providers.ollama;
    if Path::new(ollama.runtime_path.trim()) == binary_path {
        ollama.runtime_version.trim().to_string()
    } else {
        String::new()
    }
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
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        settings.providers.ollama.runtime_source = source;
        settings.providers.ollama.runtime_path = runtime_path;
        settings.providers.ollama.runtime_version = runtime_version;
        // A runtime newer than an implicit target just started: lift the target to it (a
        // frontend snapshot can bring the old one back, `normalize` heals that on every save).
        settings.providers.ollama.heal_stale_runtime_target();
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
    // Uncached: callers decide about starting and stopping servers on this answer.
    if ping_ollama_quick_fresh(endpoint).is_ok() {
        return true;
    }

    // Slightly slower probe (2 s) for busy-but-running Ollama.
    // Do NOT use the shared agent — its 60 s read timeout causes full freezes when Ollama
    // is simply not running.
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(2))
        .build();
    crate::ai_fallback::provider::ollama_endpoint_candidates(endpoint)
        .into_iter()
        .any(|candidate| agent.get(&format!("{}/api/tags", candidate)).call().is_ok())
}

fn managed_child_status(state: &AppState) -> (Option<u32>, bool) {
    managed_child_slot_status(&state.managed_ollama_child)
}

/// What `start_ollama_runtime` has to do, decided from observations only (no side effects).
#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeStartPlan {
    /// Nothing is listening: spawn the selected binary.
    Spawn,
    /// A server already answers and is left alone. `version` is the version that is really
    /// running (the selected binary's version only when the server did not tell).
    AlreadyRunning { version: String },
    /// Our own managed server runs a different build than the selected binary: stop it,
    /// wait for the port, then spawn.
    SwitchManaged { running: String, wanted: String },
    /// Our own managed server is alive, young and not answering yet: wait for it instead of
    /// killing it mid-start.
    AwaitStarting,
    /// Nothing of ours runs and nothing answers like Ollama, yet the port is taken.
    PortBlocked,
}

/// `reachable`: something answers on the endpoint. `running_version`: what its
/// `/api/version` reports. `binary_version`: version of the selected binary (empty when
/// unknown). `managed_alive`: the Ollama child we spawned ourselves is still alive.
fn plan_runtime_start(
    reachable: bool,
    running_version: Option<&str>,
    binary_version: &str,
    managed_alive: bool,
) -> RuntimeStartPlan {
    if !reachable {
        return RuntimeStartPlan::Spawn;
    }
    if managed_alive {
        if let Some(running) = running_version {
            if !binary_version.is_empty() && running != binary_version {
                return RuntimeStartPlan::SwitchManaged {
                    running: running.to_string(),
                    wanted: binary_version.to_string(),
                };
            }
        }
    }
    RuntimeStartPlan::AlreadyRunning {
        version: running_version.unwrap_or(binary_version).to_string(),
    }
}

/// True while something still accepts TCP connections on the endpoint's port. Uncached and
/// so it answers "is the port released yet?".
fn endpoint_port_in_use(endpoint: &str) -> bool {
    use std::net::ToSocketAddrs;
    let Ok(host_port) = endpoint_host_port(endpoint) else {
        return false;
    };
    let Ok(addrs) = host_port.to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_ok())
}

/// Version reported by the server that is actually answering on `endpoint`.
fn query_running_ollama_version(endpoint: &str) -> Option<String> {
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(2))
        .build();
    let url = format!("{}/api/version", endpoint.trim_end_matches('/'));
    let json = agent
        .get(&url)
        .call()
        .ok()?
        .into_json::<serde_json::Value>()
        .ok()?;
    json.get("version")?
        .as_str()
        .map(|v| v.trim().trim_start_matches('v').to_string())
        .filter(|v| !v.is_empty())
}

// ── PID lockfile helpers ──────────────────────────────────────────────────────
// The Job Object (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE) is the primary cleanup
// mechanism, but fails when the host process is already inside a non-nested
// job (e.g. Windows Terminal, VSCode). The PID lockfile is the fallback: on
// every spawn we persist the PID, and on the next startup (or explicit stop)
// we kill any process that matches, ensuring no zombie Ollama server lingers.

const OLLAMA_PID_LOCKFILE: &str = "ollama_managed.pid";

pub(crate) fn write_ollama_pid_lockfile(app: &AppHandle) {
    // Read current managed PID from the slot
    let state = app.state::<AppState>();
    let pid = match state.managed_ollama_child.lock() {
        Ok(guard) => guard.as_ref().map(|c| c.id()),
        Err(_) => return,
    };
    let Some(pid) = pid else { return };
    let path = resolve_data_path(app, OLLAMA_PID_LOCKFILE);
    let _ = std::fs::write(&path, pid.to_string());
}

pub(crate) fn kill_stale_ollama_pid(app: &AppHandle) {
    kill_stale_ollama_pid_in(&resolve_data_path(app, OLLAMA_PID_LOCKFILE));
}

/// Kills the process the lockfile names, but only while that process still IS an `ollama.exe`.
/// Process IDs are reused: after the old server died (crash, reboot, a long session) the
/// number may belong to an unrelated program, and killing "the stale Ollama" would take that
/// program down with its whole process tree.
fn kill_stale_ollama_pid_in(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(pid) = content.trim().parse::<u32>() else {
        let _ = std::fs::remove_file(path);
        return;
    };

    if crate::state::diagnostic_logging_enabled() {
        info!("Killing stale managed Ollama pid {pid} from previous session");
    }

    #[cfg(target_os = "windows")]
    {
        // Both filters must match: this pid AND the image name.
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args([
            "/FI",
            &format!("PID eq {}", pid),
            "/FI",
            "IMAGENAME eq ollama.exe",
            "/T",
            "/F",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.status();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status();
    }

    let _ = std::fs::remove_file(path);
}

pub(crate) fn clear_ollama_pid_lockfile(app: &AppHandle) {
    let path = resolve_data_path(app, OLLAMA_PID_LOCKFILE);
    let _ = std::fs::remove_file(&path);
}

/// Returns only pinned (bundled) versions — no network call. Fast and safe for automatic use.
/// Version list without network access: pinned + last cached online catalog + what is
/// installed on disk. Cheap enough to call on every UI refresh.
#[tauri::command]
pub fn list_ollama_runtime_versions(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<OllamaRuntimeVersionInfo>, String> {
    let snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let cached = load_catalog_cache(&app)
        .map(|cache| cache.versions)
        .unwrap_or_default();
    let runtime_root = resolve_runtime_root(&app);
    build_version_list(&snapshot, &cached, Some(runtime_root.as_path()))
}

/// Refreshes the online catalog from GitHub (cached for 6 h unless `force`).
/// Errors are returned to the caller instead of being swallowed; the UI keeps showing
/// the cached list in that case.
#[tauri::command]
pub async fn fetch_ollama_online_versions(
    app: AppHandle,
    force: Option<bool>,
) -> Result<Vec<OllamaRuntimeVersionInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let snapshot = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let runtime_root = resolve_runtime_root(&app);
        let extras = vec![
            snapshot.providers.ollama.runtime_version.trim().to_string(),
            snapshot
                .providers
                .ollama
                .runtime_target_version
                .trim()
                .to_string(),
        ];
        let versions = refresh_online_catalog(
            &catalog_cache_path(&app),
            &GithubApi::official(),
            &extras,
            force.unwrap_or(false),
            crate::util::now_ms(),
        )?;
        build_version_list(&snapshot, &versions, Some(runtime_root.as_path()))
    })
    .await
    .map_err(|e| format!("Online version fetch failed: {}", e))?
}

fn build_version_list(
    snapshot: &crate::state::Settings,
    online: &[OnlineRuntimeVersionCandidate],
    runtime_root: Option<&Path>,
) -> Result<Vec<OllamaRuntimeVersionInfo>, String> {
    let selected = snapshot
        .providers
        .ollama
        .runtime_target_version
        .trim()
        .to_string();
    let settings_version = snapshot.providers.ollama.runtime_version.trim().to_string();
    let on_disk = runtime_root
        .map(scan_installed_runtime_versions)
        .unwrap_or_default();
    let is_installed = |version: &str| {
        (!settings_version.is_empty() && settings_version == version)
            || on_disk.contains_key(version)
    };
    let installed_at = |version: &str| on_disk.get(version).copied().filter(|ms| *ms > 0);

    let mut merged: HashMap<String, OllamaRuntimeVersionInfo> = HashMap::new();

    for manifest in WINDOWS_MANIFESTS {
        merged.insert(
            manifest.version.to_string(),
            OllamaRuntimeVersionInfo {
                version: manifest.version.to_string(),
                source: "pinned".to_string(),
                selected: selected == manifest.version,
                installed: is_installed(manifest.version),
                recommended: manifest.version == DEFAULT_RUNTIME_VERSION,
                installable: true,
                installed_at_ms: installed_at(manifest.version),
                ..Default::default()
            },
        );
    }

    for candidate in online {
        merged
            .entry(candidate.version.clone())
            .and_modify(|entry| merge_online_candidate_into_version_entry(entry, candidate))
            .or_insert_with(|| OllamaRuntimeVersionInfo {
                version: candidate.version.clone(),
                source: "online".to_string(),
                selected: selected == candidate.version,
                installed: is_installed(&candidate.version),
                recommended: candidate.version == DEFAULT_RUNTIME_VERSION,
                prerelease: candidate.prerelease,
                installable: candidate.installable,
                installable_reason: candidate.installable_reason.clone(),
                release_name: candidate.release_name.clone(),
                release_notes_summary: candidate.release_notes_summary.clone(),
                released_at: candidate.released_at.clone(),
                size_bytes: candidate.size_bytes,
                installed_at_ms: installed_at(&candidate.version),
                ..Default::default()
            });
    }

    // Selected / installed versions that are not part of the catalog stay visible.
    // An installed runtime is simply "installed" — it does not need to be installable.
    let mut extras: Vec<String> = on_disk.keys().cloned().collect();
    if !settings_version.is_empty() {
        extras.push(settings_version.clone());
    }
    if !selected.is_empty() {
        extras.push(selected.clone());
    }
    for version in extras {
        if merged.contains_key(&version) {
            continue;
        }
        let installed = is_installed(&version);
        merged.insert(
            version.clone(),
            OllamaRuntimeVersionInfo {
                version: version.clone(),
                source: "online".to_string(),
                selected: selected == version,
                installed,
                installable: false,
                installable_reason: Some(if installed {
                    "Installed locally. Not part of the current catalog, so it cannot be reinstalled from here."
                        .to_string()
                } else {
                    "Not verified yet. Use \"Get versions\" to check this version against the official release list."
                        .to_string()
                }),
                installed_at_ms: installed_at(&version),
                ..Default::default()
            },
        );
    }

    // Newest stable, verified version in the catalog.
    // Decided on the fetched catalog only, so a pinned version that is also the newest release
    // gets the marker, and nothing is called "latest" while no catalog has been fetched.
    let latest_version = online
        .iter()
        .filter(|candidate| !candidate.prerelease && candidate.installable)
        .filter_map(|candidate| {
            parse_stable_semver(&candidate.version).map(|v| (v, candidate.version.clone()))
        })
        .max()
        .map(|(_, version)| version);
    if let Some(latest) = latest_version {
        if let Some(entry) = merged.get_mut(&latest) {
            entry.latest = true;
        }
    }

    let mut out = merged.into_values().collect::<Vec<_>>();

    out.sort_by(|a, b| {
        b.selected
            .cmp(&a.selected)
            .then_with(|| b.installed.cmp(&a.installed))
            .then_with(|| b.recommended.cmp(&a.recommended))
            .then_with(|| b.installable.cmp(&a.installable))
            .then_with(|| compare_runtime_versions_desc(&a.version, &b.version))
    });

    Ok(out)
}

fn detect_ollama_runtime_impl(app: &AppHandle) -> Result<OllamaRuntimeDetectResult, String> {
    let state = app.state::<AppState>();
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let (managed_pid, managed_alive) = managed_child_status(state.inner());
    let endpoint = settings.providers.ollama.endpoint.clone();

    // Phase 1: filesystem-only binary search (no network).
    let managed_root = managed_runtime_root(app);
    let target_installed = managed_root
        .as_deref()
        .and_then(|root| target_runtime_binary_in(root, &settings))
        .is_some();
    let binary_info = detect_runtime_in(&settings, managed_root.as_deref());

    let result = match binary_info {
        None => OllamaRuntimeDetectResult {
            found: false,
            is_serving: false,
            source: MANAGED_RUNTIME_SOURCE.to_string(),
            path: String::new(),
            version: String::new(),
            managed_pid,
            managed_alive,
            target_installed,
        },
        Some((source, path, version)) => {
            // Phase 2: single quick ping (≤ 300 ms) — never blocks the UI thread noticeably.
            let is_serving = ping_ollama_quick(&endpoint).is_ok();
            OllamaRuntimeDetectResult {
                found: true,
                is_serving,
                source,
                version,
                path: path.to_string_lossy().to_string(),
                managed_pid,
                managed_alive,
                target_installed,
            }
        }
    };
    if crate::state::diagnostic_logging_enabled() {
        info!(
            "detect_ollama_runtime => found={} serving={} source={} managed_alive={} endpoint={}",
            result.found, result.is_serving, result.source, result.managed_alive, endpoint
        );
    }
    update_startup_status(&app, state.inner(), |status| {
        status.ollama_ready = result.is_serving;
        status.ollama_starting = !result.is_serving && result.managed_alive;
    });
    update_ollama_runtime_diagnostics(
        &app,
        &settings,
        result.found,
        if result.is_serving {
            if result.managed_alive {
                "ready"
            } else {
                "running_externally"
            }
        } else if result.managed_alive {
            "starting"
        } else if result.found {
            "detected"
        } else {
            "not_detected"
        },
        result.managed_pid,
        result.is_serving,
        if result.found {
            None
        } else {
            Some("No Ollama runtime detected.".to_string())
        },
    );
    Ok(result)
}

#[tauri::command]
pub async fn detect_ollama_runtime(app: AppHandle) -> Result<OllamaRuntimeDetectResult, String> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || detect_ollama_runtime_impl(&app_handle))
        .await
        .map_err(|e| format!("Runtime detect task failed: {}", e))?
}

/// Answer of `delete_ollama_runtime_version`. A refusal is `deleted: false` with the reason in
/// `detail`; `Err` is reserved for a request that cannot be a runtime version at all.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OllamaRuntimeDeleteResult {
    pub deleted: bool,
    pub freed_bytes: u64,
    pub detail: Option<String>,
}

impl OllamaRuntimeDeleteResult {
    fn refused(detail: impl Into<String>) -> Self {
        Self {
            deleted: false,
            freed_bytes: 0,
            detail: Some(detail.into()),
        }
    }
}

/// What is known about the running system when a runtime folder is about to be deleted.
struct RuntimeDeleteContext<'a> {
    runtime_root: &'a Path,
    /// `runtime_target_version`: the version the next start launches.
    target_version: &'a str,
    /// `runtime_version` / `runtime_path` of the settings: the binary the app last started.
    recorded_version: &'a str,
    recorded_path: &'a str,
    /// What `/api/version` of the endpoint answers right now (None: nothing answers).
    running_version: Option<&'a str>,
    /// The Ollama child this app spawned is alive.
    managed_alive: bool,
    retry_delay: Duration,
}

/// Tries per file-system step that Windows can block for a moment (antivirus, indexer).
const RUNTIME_DELETE_ATTEMPTS: u32 = 4;
const RUNTIME_DELETE_RETRY_DELAY: Duration = Duration::from_millis(400);

fn same_runtime_version(left: &str, right: &str) -> bool {
    let clean = |value: &str| value.trim().trim_start_matches('v').to_ascii_lowercase();
    let (left, right) = (clean(left), clean(right));
    !left.is_empty() && left == right
}

/// Case-insensitive, separator-neutral prefix test (the recorded path is text from the settings).
fn path_text_is_inside(path: &str, folder: &Path) -> bool {
    let norm = |value: &str| value.trim().replace('\\', "/").to_ascii_lowercase();
    let folder = norm(&folder.to_string_lossy());
    let folder = folder.trim_end_matches('/');
    let path = norm(path);
    !folder.is_empty() && path.starts_with(&format!("{}/", folder))
}

/// Total size of the files below `dir`. Links are not followed (a junction to a big folder
/// elsewhere is not part of the runtime), and the walk is bounded like `find_file_recursive`.
fn directory_size_without_links(dir: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    let mut seen = 0usize;
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > FIND_MAX_ENTRIES {
                return total;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

fn retry_io<T>(
    attempts: u32,
    delay: Duration,
    mut step: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut last = None;
    for attempt in 0..attempts.max(1) {
        match step() {
            Ok(value) => return Ok(value),
            Err(error) => last = Some(error),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(delay);
        }
    }
    Err(last.expect("at least one attempt ran"))
}

/// Deletes `<runtime_root>/<version>` after checking that nothing needs it. Order of the safety
/// net: the name must be a plain version, the folder a real directory (not a link) directly
/// below the managed root with an `ollama.exe` in it, and the version neither running, nor the
/// recorded binary, nor the selected target. The folder is renamed away first (atomic: a failure
/// leaves the runtime untouched), then removed; `remove_dir_all` does not follow links.
fn delete_runtime_version_in(
    ctx: &RuntimeDeleteContext<'_>,
    version: &str,
) -> Result<OllamaRuntimeDeleteResult, String> {
    let version = version.trim();
    if !is_safe_runtime_version(version) || version.starts_with('.') {
        return Err(format!("'{}' is not a valid runtime version.", version));
    }
    let dir = ctx.runtime_root.join(version);

    if ctx
        .running_version
        .is_some_and(|running| same_runtime_version(running, version))
    {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} is running right now. Switch to another version first.",
            version
        )));
    }
    if same_runtime_version(ctx.recorded_version, version)
        || path_text_is_inside(ctx.recorded_path, &dir)
    {
        let who = if ctx.managed_alive {
            "The managed Ollama process runs from"
        } else {
            "The app is set up to run from"
        };
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "{} runtime {}. Switch to another version first.",
            who, version
        )));
    }
    if same_runtime_version(ctx.target_version, version) {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} is the selected target. Select another version first, then delete it.",
            version
        )));
    }

    let Ok(link_meta) = fs::symlink_metadata(&dir) else {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} is not installed.",
            version
        )));
    };
    if link_meta.file_type().is_symlink() || !link_meta.is_dir() {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} is not a plain folder (a link or a file). Remove it by hand if you want it gone.",
            version
        )));
    }
    let inside_root = match (fs::canonicalize(ctx.runtime_root), fs::canonicalize(&dir)) {
        (Ok(root), Ok(real)) => real.parent() == Some(root.as_path()),
        _ => false,
    };
    if !inside_root {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} is not inside the managed runtime folder.",
            version
        )));
    }
    let Some(binary) = find_file_recursive(&dir, "ollama.exe") else {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Folder {} holds no ollama.exe, so it is not a managed runtime and is left alone.",
            version
        )));
    };
    if executable_in_use(&binary) {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} is in use: its ollama.exe is running. Stop Ollama or switch version first.",
            version
        )));
    }

    let size = directory_size_without_links(&dir);
    let trash = ctx
        .runtime_root
        .join(format!(".deleted-{}-{}", version, unique_install_stamp()));
    if let Err(error) = retry_io(RUNTIME_DELETE_ATTEMPTS, ctx.retry_delay, || {
        fs::rename(&dir, &trash)
    }) {
        return Ok(OllamaRuntimeDeleteResult::refused(format!(
            "Runtime {} could not be removed: {}. Another program may be using files in it. Close it and retry.",
            version, error
        )));
    }

    // The version is gone from the list now. Whatever a lock keeps behind sits in a dot folder
    // that no scan counts and the next install cleanup removes.
    match retry_io(RUNTIME_DELETE_ATTEMPTS, ctx.retry_delay, || {
        fs::remove_dir_all(&trash)
    }) {
        Ok(()) => Ok(OllamaRuntimeDeleteResult {
            deleted: true,
            freed_bytes: size,
            detail: None,
        }),
        Err(error) => {
            let left = directory_size_without_links(&trash);
            warn!(
                "[ollama.runtime] runtime {} removed from the list, leftovers stay in {}: {}",
                version,
                trash.display(),
                error
            );
            Ok(OllamaRuntimeDeleteResult {
                deleted: true,
                freed_bytes: size.saturating_sub(left),
                detail: Some(format!(
                    "Runtime {} was removed, but some files are still locked ({}). They are cleaned up later.",
                    version, error
                )),
            })
        }
    }
}

/// Deletes an installed managed runtime version. Refuses (`deleted: false` + reason) while the
/// version is running, is the recorded binary or the selected target.
#[tauri::command]
pub async fn delete_ollama_runtime_version(
    app: AppHandle,
    version: String,
) -> Result<OllamaRuntimeDeleteResult, String> {
    tauri::async_runtime::spawn_blocking(move || delete_ollama_runtime_version_impl(&app, &version))
        .await
        .map_err(|e| format!("Runtime delete task failed: {}", e))?
}

fn delete_ollama_runtime_version_impl(
    app: &AppHandle,
    version: &str,
) -> Result<OllamaRuntimeDeleteResult, String> {
    let version = version.trim();
    if !is_safe_runtime_version(version) || version.starts_with('.') {
        return Err(format!("'{}' is not a valid runtime version.", version));
    }
    let _job = RuntimeJob::begin(
        format!("delete:{}", version),
        format!("Runtime {} is already being deleted.", version),
    )?;
    let installing = RUNTIME_JOBS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .any(|key| key == "install");
    if installing {
        return Ok(OllamaRuntimeDeleteResult::refused(
            "A runtime install is running. Try again when it is done.",
        ));
    }

    let state = app.state::<AppState>();
    let ollama = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .providers
        .ollama
        .clone();
    let (_, managed_alive) = managed_child_status(state.inner());
    let running_version = query_running_ollama_version(&ollama.endpoint);
    let runtime_root = resolve_runtime_root(app);
    let result = delete_runtime_version_in(
        &RuntimeDeleteContext {
            runtime_root: &runtime_root,
            target_version: &ollama.runtime_target_version,
            recorded_version: &ollama.runtime_version,
            recorded_path: &ollama.runtime_path,
            running_version: running_version.as_deref(),
            managed_alive,
            retry_delay: RUNTIME_DELETE_RETRY_DELAY,
        },
        version,
    )?;
    info!(
        "[ollama.runtime] delete runtime {} => deleted={} freed_bytes={} detail={:?}",
        version, result.deleted, result.freed_bytes, result.detail
    );
    Ok(result)
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

/// Longest silence tolerated on a download connection. The timeout applies to each read, not
/// to the whole transfer, so a healthy multi-GB download never trips it, while a connection
/// that went dead (Wi-Fi drop without a reset) ends after a minute instead of hanging.
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(60);

/// Names of the operations running right now. The same one never runs twice at once: two
/// downloads of one version would write the same `.zip.part`, and two installs would race on
/// the staging and target folders.
static RUNTIME_JOBS: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct RuntimeJob(String);

impl RuntimeJob {
    fn begin(key: String, busy_message: String) -> Result<Self, String> {
        let mut jobs = RUNTIME_JOBS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if jobs.contains(&key) {
            return Err(busy_message);
        }
        jobs.push(key.clone());
        Ok(Self(key))
    }
}

impl Drop for RuntimeJob {
    fn drop(&mut self) {
        let mut jobs = RUNTIME_JOBS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        jobs.retain(|key| key != &self.0);
    }
}

/// The disk (or quota) is full: `ERROR_DISK_FULL` / `ERROR_HANDLE_DISK_FULL` on Windows.
fn is_disk_full(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded
    ) || matches!(error.raw_os_error(), Some(112) | Some(39) | Some(28))
}

/// A write failure the user can act on: a full disk is said so.
fn describe_write_error(what: &str, error: &std::io::Error) -> String {
    if is_disk_full(error) {
        format!(
            "Not enough free disk space to {} (the runtime needs about 2.5 GB). Free up space and try again.",
            what
        )
    } else {
        format!("Failed to {}: {}", what, error)
    }
}

fn describe_download_error(error: &ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, _) => format!(
            "the download server answered HTTP {} (the release file may have been removed)",
            code
        ),
        ureq::Error::Transport(transport) => format!(
            "could not reach the download server: {}. Check the internet connection or proxy settings",
            transport
        ),
    }
}

/// Why the body of a download stopped coming.
fn describe_stream_error(error: &std::io::Error, downloaded: u64, total: Option<u64>) -> String {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => format!(
            "The download stalled (no data for {} s) after {} bytes. Check the connection and try again.",
            DOWNLOAD_STALL_TIMEOUT.as_secs(),
            downloaded
        ),
        std::io::ErrorKind::UnexpectedEof => format!(
            "The connection was closed before the download finished ({} of {} bytes). Try again.",
            downloaded,
            total.map_or_else(|| "?".to_string(), |t| t.to_string())
        ),
        _ => format!(
            "The connection broke during the download after {} bytes: {}. Try again.",
            downloaded, error
        ),
    }
}

enum StreamError {
    Read(std::io::Error),
    Write(std::io::Error),
    TooLarge,
}

/// Copies `reader` to `writer`, at most `max_bytes`, throttled like all background IO.
/// `on_chunk` gets the running total. Returns the bytes copied.
fn stream_download<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    max_bytes: u64,
    on_chunk: &mut dyn FnMut(u64),
) -> Result<u64, StreamError> {
    let mut buf = vec![0u8; 1024 * 256];
    let mut downloaded = 0u64;
    let mut processed_since_pause = 0u64;
    loop {
        let read = reader.read(&mut buf).map_err(StreamError::Read)?;
        if read == 0 {
            return Ok(downloaded);
        }
        writer.write_all(&buf[..read]).map_err(StreamError::Write)?;
        downloaded = downloaded.saturating_add(read as u64);
        if downloaded > max_bytes {
            return Err(StreamError::TooLarge);
        }
        processed_since_pause += read as u64;
        maybe_throttle_background_io(&mut processed_since_pause);
        on_chunk(downloaded);
    }
}

/// `(message, downloaded, total)` of a running download.
type DownloadProgress<'a> = dyn FnMut(&str, Option<u64>, Option<u64>) + 'a;

/// Downloads the runtime archive of `manifest` into `cache_dir` and verifies its SHA-256.
/// On every failure (connection lost or stalled, disk full, too large, wrong checksum) nothing
/// is left behind: no `.zip.part`, no archive that failed its check. A cached archive that
/// still matches the checksum is reused.
/// `on_progress(message, downloaded, total)` reports what happens.
fn download_runtime_archive(
    manifest: &RuntimeManifestResolved,
    cache_dir: &Path,
    max_bytes: u64,
    stall_timeout: Duration,
    on_progress: &mut DownloadProgress<'_>,
) -> Result<PathBuf, String> {
    let _job = RuntimeJob::begin(
        format!("download:{}", manifest.version),
        format!(
            "A download of Ollama runtime {} is already running.",
            manifest.version
        ),
    )?;
    let archive_path = cache_dir.join(format!("ollama-windows-amd64-v{}.zip", manifest.version));
    let temp_path = archive_path.with_extension("zip.part");
    // A failed or aborted download must not leave a multi-GB `.part` behind (no resume exists:
    // the next attempt starts over and overwrites it).
    let mut part_guard = RemoveOnDrop::new(temp_path.clone());

    if archive_path.exists() {
        let current_hash = sha256_file(&archive_path)?;
        if current_hash.eq_ignore_ascii_case(&manifest.sha256) {
            let _ = write_runtime_archive_metadata(&archive_path, manifest);
            on_progress(
                &format!("Runtime archive already cached ({})", manifest.version),
                None,
                None,
            );
            return Ok(archive_path);
        }
        let _ = fs::remove_file(&archive_path);
        let _ = fs::remove_file(runtime_archive_metadata_path(&archive_path));
    }

    on_progress(
        &format!("Downloading Ollama runtime {}", manifest.version),
        Some(0),
        None,
    );

    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(stall_timeout)
        .build();
    let response = agent
        .get(&manifest.url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .call()
        .map_err(|e| {
            format!(
                "Failed to download runtime archive: {}",
                describe_download_error(&e)
            )
        })?;

    let total = response
        .header("Content-Length")
        .and_then(|h| h.parse::<u64>().ok());
    if total.map(|t| t > max_bytes).unwrap_or(false) {
        return Err(format!(
            "Runtime archive is larger than the {} GiB safety limit; download aborted.",
            max_bytes / (1024 * 1024 * 1024)
        ));
    }

    let mut reader = response.into_reader();
    let mut out = File::create(&temp_path)
        .map_err(|e| describe_write_error("create the download file", &e))?;
    let mut last_emit = Instant::now();
    let mut last_seen = 0u64;
    let streamed = stream_download(&mut reader, &mut out, max_bytes, &mut |downloaded| {
        last_seen = downloaded;
        if last_emit.elapsed() >= Duration::from_millis(250) {
            on_progress("Downloading runtime archive...", Some(downloaded), total);
            last_emit = Instant::now();
        }
    });
    let copied = streamed.map_err(|error| match error {
        StreamError::TooLarge => {
            "Runtime download exceeded the size safety limit and was aborted.".to_string()
        }
        StreamError::Read(e) => describe_stream_error(&e, last_seen, total),
        StreamError::Write(e) => describe_write_error("write the runtime archive to disk", &e),
    })?;
    if let Some(expected) = total {
        if copied != expected {
            return Err(format!(
                "The download ended early: {} of {} bytes arrived. Try again.",
                copied, expected
            ));
        }
    }
    drop(out);
    fs::rename(&temp_path, &archive_path)
        .map_err(|e| format!("Failed to finalize downloaded archive: {}", e))?;
    part_guard.disarm();

    let digest = sha256_file(&archive_path)?;
    if !digest.eq_ignore_ascii_case(&manifest.sha256) {
        let _ = fs::remove_file(&archive_path);
        return Err(format!(
            "Runtime archive checksum mismatch. Expected {}, got {}",
            manifest.sha256, digest
        ));
    }

    on_progress(
        "Download complete and checksum verified.",
        Some(copied),
        total.or(Some(copied)),
    );
    let _ = write_runtime_archive_metadata(&archive_path, manifest);
    Ok(archive_path)
}

fn download_ollama_runtime_impl(
    app: &AppHandle,
    version: Option<String>,
) -> Result<OllamaRuntimeDownloadResult, String> {
    let fail = |msg: String| {
        emit_install_error(app, "download_runtime", msg.clone());
        msg
    };
    // (as before: a failed manifest lookup is only returned, the install-error event is for the
    // download itself)
    let manifest = resolve_manifest(version.as_deref())?;
    validate_runtime_download_url(&manifest.url, &manifest.version).map_err(fail)?;
    let cache_dir = resolve_runtime_cache_dir(app);
    cleanup_stale_download_parts(&cache_dir, Duration::from_secs(3600));
    let archive_path = download_runtime_archive(
        &manifest,
        &cache_dir,
        MAX_RUNTIME_ARCHIVE_BYTES,
        DOWNLOAD_STALL_TIMEOUT,
        &mut |message, downloaded, total| {
            emit_install_progress(
                app,
                "download_runtime",
                message.to_string(),
                downloaded,
                total,
                Some(manifest.version.to_string()),
            );
        },
    )
    .map_err(fail)?;
    Ok(OllamaRuntimeDownloadResult {
        archive_path: archive_path.to_string_lossy().to_string(),
        sha256_ok: true,
        version: manifest.version.to_string(),
    })
}

/// Removes `*.zip.part` files nobody has written to for `max_age`: the leftovers of a download
/// whose process was killed.
fn cleanup_stale_download_parts(cache_dir: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_part = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".zip.part"));
        if !is_part {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > max_age);
        if stale {
            let _ = fs::remove_file(&path);
        }
    }
}

/// A crash between the two renames of `swap_runtime_dir` leaves `<version>` missing and the
/// previous install in `.old-<version>-<ms>`. Puts the newest such folder back so the user
/// does not lose a working runtime. Returns the restored versions.
fn recover_interrupted_swaps(runtime_root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(runtime_root) else {
        return Vec::new();
    };
    // version -> (stamp, folder) of the newest leftover
    let mut newest: HashMap<String, (u64, PathBuf)> = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(rest) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(".old-"))
        else {
            continue;
        };
        let Some((version, stamp)) = rest.rsplit_once('-') else {
            continue;
        };
        let Ok(stamp) = stamp.parse::<u64>() else {
            continue;
        };
        if !is_safe_runtime_version(version) || !path.is_dir() {
            continue;
        }
        if newest.get(version).is_none_or(|(best, _)| stamp > *best) {
            newest.insert(version.to_string(), (stamp, path));
        }
    }
    let mut restored = Vec::new();
    for (version, (_, old_dir)) in newest {
        let target = runtime_root.join(&version);
        if target.exists() || find_file_recursive(&old_dir, "ollama.exe").is_none() {
            continue;
        }
        if fs::rename(&old_dir, &target).is_ok() {
            warn!(
                "[ollama.runtime] restored runtime {} from an interrupted install",
                version
            );
            restored.push(version);
        }
    }
    restored
}

/// Cleans up after interrupted installs: first puts a runtime back that a crash left in
/// `.old-*` (see `recover_interrupted_swaps`), then removes `.staging-*` / `.old-*` / `.deleted-*` folders
/// older than one hour.
fn cleanup_stale_runtime_dirs(runtime_root: &Path) {
    cleanup_stale_runtime_dirs_older_than(runtime_root, Duration::from_secs(3600));
}

fn cleanup_stale_runtime_dirs_older_than(runtime_root: &Path, max_age: Duration) {
    recover_interrupted_swaps(runtime_root);
    let Ok(entries) = fs::read_dir(runtime_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !(name.starts_with(".staging-")
            || name.starts_with(".old-")
            || name.starts_with(".deleted-"))
        {
            continue;
        }
        let age_ok = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .map(|age| age > max_age)
            .unwrap_or(false);
        if age_ok {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

/// Removes a file or directory on drop unless disarmed, so a failed install or an aborted
/// download never leaves a multi-GB leftover behind.
struct RemoveOnDrop {
    path: PathBuf,
    armed: bool,
}

impl RemoveOnDrop {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if self.path.is_dir() {
            let _ = fs::remove_dir_all(&self.path);
        } else {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Opens the archive so nothing else can write to, rename or delete it while the handle is
/// held. Hash and unpack must use this one handle, otherwise the file could be swapped
/// between "checksum verified" and "extracted".
fn open_archive_locked(path: &Path) -> std::io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1); // FILE_SHARE_READ: no concurrent writers, no rename/delete
    }
    options.open(path)
}

/// Unpacks `zip` into `staging_dir`. Hardened against zip-slip (`enclosed_name`) and against
/// decompression bombs: the declared sizes are summed up front and the zip reader refuses
/// entries that outgrow their declared size.
fn extract_runtime_zip<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    staging_dir: &Path,
    max_total_bytes: u64,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    let total_entries = zip.len();
    if total_entries > MAX_RUNTIME_ARCHIVE_ENTRIES {
        return Err(format!(
            "Runtime archive has {} entries (limit {}). Install aborted.",
            total_entries, MAX_RUNTIME_ARCHIVE_ENTRIES
        ));
    }
    let mut declared_total = 0u64;
    for idx in 0..total_entries {
        let entry = zip
            .by_index_raw(idx)
            .map_err(|e| format!("Failed to read ZIP entry {}: {}", idx, e))?;
        declared_total = declared_total.saturating_add(entry.size());
    }
    if declared_total > max_total_bytes {
        return Err(format!(
            "Runtime archive unpacks to {} bytes (limit {}). Install aborted.",
            declared_total, max_total_bytes
        ));
    }

    for idx in 0..total_entries {
        let mut entry = zip
            .by_index(idx)
            .map_err(|e| format!("Failed to read ZIP entry {}: {}", idx, e))?;

        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| {
                let name = entry.name().map(|n| n.into_owned()).unwrap_or_default();
                format!("Unsafe ZIP entry path rejected: {}", name)
            })?
            .to_owned();
        let out_path = staging_dir.join(enclosed);

        if entry.is_dir() {
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
            // The zip reader itself aborts an entry that outgrows its declared size, so the
            // declared-size sum checked above is a real bound (see the deflate-bomb test).
            copy_with_background_throttle(&mut entry, &mut out)
                .map_err(|e| format!("Failed to extract '{}': {}", out_path.display(), e))?;
        }

        if idx == 0 || idx + 1 == total_entries || (idx + 1) % 10 == 0 {
            on_progress(idx + 1, total_entries);
        }
    }
    Ok(())
}

/// True while a process runs from this executable.
///
/// Windows lets an updater rename the folder of a running `ollama.exe` and even delete the
/// files in it (Rust deletes with POSIX semantics where available): the server keeps running,
/// but without its runner libraries, and fails at the next model load. What it does refuse is
/// opening a mapped image for writing (sharing violation, os error 32). So a write-open that
/// writes, truncates and creates nothing is a reliable, harmless "is it running?" probe.
/// (Measured against a real 0.30.4 server: renaming and replacing its folder succeeded.)
fn executable_in_use(binary: &Path) -> bool {
    #[cfg(target_os = "windows")]
    const SHARING_VIOLATION: i32 = 32;
    #[cfg(not(target_os = "windows"))]
    const SHARING_VIOLATION: i32 = 26; // ETXTBSY
    matches!(
        fs::OpenOptions::new().write(true).open(binary),
        Err(error) if error.raw_os_error() == Some(SHARING_VIOLATION)
    )
}

/// The error for a runtime folder whose `ollama.exe` is running right now.
fn runtime_in_use_error(target: &Path, detail: &str) -> String {
    format!(
        "Runtime {} is in use and cannot be replaced. Stop Ollama (or select another version) and retry{}",
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("folder"),
        detail
    )
}

/// `Err` when an `ollama.exe` inside `target` is running.
fn ensure_runtime_not_running(target: &Path) -> Result<(), String> {
    match find_file_recursive(target, "ollama.exe") {
        Some(binary) if executable_in_use(&binary) => Err(runtime_in_use_error(
            target,
            &format!(": '{}' is running.", binary.display()),
        )),
        _ => Ok(()),
    }
}

/// Puts the verified `staging` folder at `target` without ever leaving the user without a
/// working runtime: the previous folder goes to `backup` first and comes back if the final
/// move fails; on success the backup is deleted (best effort).
fn swap_runtime_dir(staging: &Path, target: &Path, backup: &Path) -> Result<(), String> {
    let had_previous = target.exists();
    if had_previous {
        // A rename would succeed even under a running server (see `executable_in_use`).
        ensure_runtime_not_running(target)?;
        if let Err(e) = fs::rename(target, backup) {
            return Err(runtime_in_use_error(target, &format!(": {}", e)));
        }
    }
    if let Err(e) = fs::rename(staging, target) {
        let note = if !had_previous {
            String::new()
        } else if fs::rename(backup, target).is_ok() {
            " The previous runtime was restored.".to_string()
        } else {
            format!(" The previous runtime is kept in '{}'.", backup.display())
        };
        return Err(format!(
            "Failed to move runtime into final location: {}.{}",
            e, note
        ));
    }
    if had_previous {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

/// True when the certificate subject (RFC 4514 style, as printed by .NET) carries exactly
/// `organization` in its `O=` field. A substring match would also accept values like
/// `O=Ollama Inc.Fake` or an `O=` hidden inside another field.
fn subject_has_organization(subject: &str, organization: &str) -> bool {
    // Split on commas that are not inside a quoted value.
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in subject.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => fields.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    fields.push(current);

    fields.iter().any(|field| {
        let Some((key, value)) = field.split_once('=') else {
            return false;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .map(|v| v.replace("\"\"", "\""))
            .unwrap_or_else(|| value.to_string());
        key.trim().eq_ignore_ascii_case("O") && value == organization
    })
}

/// Judges the `<Status>|<Subject>` line printed by the PowerShell signature probe.
fn evaluate_signature_report(report: &str) -> Result<(), String> {
    let (status, subject) = report.split_once('|').unwrap_or((report, ""));
    if status.trim() != "Valid" {
        return Err(format!(
            "Runtime binary signature check failed (status: {}). Install aborted.",
            status.trim()
        ));
    }
    if !subject_has_organization(subject, OLLAMA_SIGNER_ORGANIZATION) {
        return Err(format!(
            "Runtime binary is signed by an unexpected publisher ({}). Install aborted.",
            subject.trim()
        ));
    }
    Ok(())
}

/// Absolute path of Windows PowerShell, so a planted `powershell.exe` earlier in PATH or in
/// the working directory can never answer the signature probe.
#[cfg(target_os = "windows")]
fn windows_powershell_path() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|root| {
            PathBuf::from(root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
        })
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("powershell"))
}

/// How long the signature probe may run. `Get-AuthenticodeSignature` builds the certificate
/// chain and checks revocation over the network; where the CA cannot be reached (offline,
/// firewall, proxy) that can take minutes.
const SIGNATURE_PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// `ollama --version` of a freshly unpacked binary (first run: virus scanner, cold disk).
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// What the signature check could say about a binary.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SignatureVerdict {
    /// Valid Authenticode signature of Ollama Inc.
    Verified,
    /// The check could not reach a verdict (PowerShell missing or blocked, no output, timed
    /// out). Says nothing against the binary; the caller relies on the SHA-256 that was
    /// already verified and must tell the user.
    Unverified(String),
}

struct CapturedOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
enum RunError {
    Spawn(String),
    TimedOut,
}

/// Reads a pipe to its end on its own thread; the bytes arrive on the returned channel.
fn drain_pipe<R: Read + Send + 'static>(pipe: Option<R>) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        let _ = sender.send(bytes);
    });
    receiver
}

/// Kills the child and everything it started.
fn kill_process_tree(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs `cmd` to completion, but never longer than `timeout`: a process that is still running
/// then is killed with its children and `TimedOut` is returned, so a hung child cannot hang
/// the install or the start with it.
fn run_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<CapturedOutput, RunError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| RunError::Spawn(e.to_string()))?;
    let stdout = drain_pipe(child.stdout.take());
    let stderr = drain_pipe(child.stderr.take());
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(e) => {
                kill_process_tree(&mut child);
                return Err(RunError::Spawn(e.to_string()));
            }
        }
        if Instant::now() >= deadline {
            // The reader threads end by themselves once the killed process closes its pipes.
            kill_process_tree(&mut child);
            return Err(RunError::TimedOut);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    // A grandchild that kept the pipe open must not hold us either.
    let grace = Duration::from_secs(2);
    Ok(CapturedOutput {
        stdout: stdout.recv_timeout(grace).unwrap_or_default(),
        stderr: stderr.recv_timeout(grace).unwrap_or_default(),
    })
}

/// The PowerShell snippet of the probe. The path travels in an environment variable and is
/// read by the script as data, so no character in it (quotes incl. curly ones, `$`, backticks,
/// `;`) can alter the script.
#[cfg(target_os = "windows")]
fn signature_probe_script() -> String {
    format!(
        "$s = Get-AuthenticodeSignature -LiteralPath $env:{}; \"$($s.Status)|$($s.SignerCertificate.Subject)\"",
        SIGNATURE_TARGET_ENV
    )
}

/// Checks that the binary carries a valid Authenticode signature issued to Ollama Inc.
/// A signature that is present but invalid or from someone else is a hard failure (`Err`).
///
/// If the check itself cannot reach a verdict (PowerShell unavailable or blocked, no output,
/// or no answer within `SIGNATURE_PROBE_TIMEOUT`) the result is
/// `Ok(SignatureVerdict::Unverified(..))`: **fail-open on purpose.** The archive's SHA-256 has
/// already been verified (compiled-in pin or the digest GitHub publishes), so what runs is
/// byte-for-byte the published release. A hung revocation lookup is an environment problem and
/// no evidence of tampering; failing closed would make installing impossible exactly on offline
/// and locked-down machines. The caller must surface "unverified" to the user.
#[cfg(target_os = "windows")]
fn verify_authenticode_signer(binary: &Path) -> Result<SignatureVerdict, String> {
    verify_authenticode_signer_with(&windows_powershell_path(), binary)
}

#[cfg(target_os = "windows")]
fn verify_authenticode_signer_with(
    powershell: &Path,
    binary: &Path,
) -> Result<SignatureVerdict, String> {
    verify_authenticode_signer_timed(
        powershell,
        binary,
        &signature_probe_script(),
        SIGNATURE_PROBE_TIMEOUT,
    )
}

/// `verify_authenticode_signer_with` with the script and the time limit as parameters.
#[cfg(target_os = "windows")]
fn verify_authenticode_signer_timed(
    powershell: &Path,
    binary: &Path,
    script: &str,
    timeout: Duration,
) -> Result<SignatureVerdict, String> {
    let mut cmd = Command::new(powershell);
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ])
    // A PSModulePath inherited from PowerShell 7 points Windows PowerShell 5.1 at modules it cannot load,
    // so Get-AuthenticodeSignature yields an empty status. Let it rebuild its own default.
    .env_remove("PSModulePath")
    .env(SIGNATURE_TARGET_ENV, binary)
    .creation_flags(CREATE_NO_WINDOW);
    let output = match run_with_timeout(&mut cmd, timeout) {
        Ok(output) => output,
        Err(RunError::Spawn(err)) => {
            warn!("[ollama.runtime] signature check could not run: {}", err);
            return Ok(SignatureVerdict::Unverified(format!(
                "the signature check could not run ({})",
                err
            )));
        }
        Err(RunError::TimedOut) => {
            warn!(
                "[ollama.runtime] signature check timed out after {} s; relying on SHA-256",
                timeout.as_secs()
            );
            return Ok(SignatureVerdict::Unverified(format!(
                "the signature check did not finish within {} s",
                timeout.as_secs().max(1)
            )));
        }
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        warn!("[ollama.runtime] signature check returned no result; relying on SHA-256");
        return Ok(SignatureVerdict::Unverified(
            "the signature check returned no result".to_string(),
        ));
    }
    evaluate_signature_report(&stdout).map(|()| SignatureVerdict::Verified)
}

#[cfg(not(target_os = "windows"))]
fn verify_authenticode_signer(_binary: &Path) -> Result<SignatureVerdict, String> {
    Ok(SignatureVerdict::Unverified(
        "signatures are only checked on Windows".to_string(),
    ))
}

/// Unique per call and strictly increasing (milliseconds scaled by 1000, bumped past the last
/// stamp handed out): names the staging and backup folders of one install.
/// `recover_interrupted_swaps` reads it back as "newest".
fn unique_install_stamp() -> u64 {
    static LAST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let wanted = crate::util::now_ms().saturating_mul(1000);
    let mut previous = LAST.load(std::sync::atomic::Ordering::Relaxed);
    loop {
        let next = wanted.max(previous.saturating_add(1));
        match LAST.compare_exchange_weak(
            previous,
            next,
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
        ) {
            Ok(_) => return next,
            Err(actual) => previous = actual,
        }
    }
}

/// Everything between "archive verified" and "settings updated": unpack into a staging
/// folder, verify the staged binary, swap it into `<runtime_root>/<version>`. Returns the
/// final `ollama.exe`. The staging folder is gone afterwards on every outcome, and an
/// existing install of the same version survives any failure.
fn stage_and_swap_runtime<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    runtime_root: &Path,
    version: &str,
    max_total_bytes: u64,
    verify_staged: &dyn Fn(&Path) -> Result<(), String>,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<PathBuf, String> {
    if !is_safe_runtime_version(version) {
        return Err(format!("Unsafe runtime version string '{}'.", version));
    }
    let target_dir = runtime_root.join(version);
    // Before 2 GB are unpacked for nothing: is the folder to replace being served from?
    ensure_runtime_not_running(&target_dir)?;
    // Unique per call: two installs of one version (should the job guard ever be bypassed)
    // must not share, and clear out, each other's staging folder.
    let stamp = unique_install_stamp();
    let staging_dir = runtime_root.join(format!(".staging-{}-{}", version, stamp));
    let _ = fs::remove_dir_all(&staging_dir);
    fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Failed to create staging directory: {}", e))?;
    // Every early return below removes the staging folder (a half-unpacked runtime is GBs).
    let mut staging_guard = RemoveOnDrop::new(staging_dir.clone());

    extract_runtime_zip(zip, &staging_dir, max_total_bytes, on_progress)?;
    let staged_binary = find_file_recursive(&staging_dir, "ollama.exe")
        .ok_or_else(|| "Installed runtime does not contain ollama.exe".to_string())?;
    verify_staged(&staged_binary)?;

    let backup_dir = runtime_root.join(format!(".old-{}-{}", version, stamp));
    swap_runtime_dir(&staging_dir, &target_dir, &backup_dir)?;
    staging_guard.disarm();

    find_file_recursive(&target_dir, "ollama.exe")
        .ok_or_else(|| "Installed runtime binary not found in final location".to_string())
}

/// Final gate before the staged runtime replaces anything: it must report the expected
/// version and be signed by Ollama Inc. The verdict says whether the signature was actually
/// verified (`Unverified`: the check could not run or time out, see `verify_authenticode_signer`).
fn verify_staged_runtime_verdict(
    binary: &Path,
    expected_version: &str,
) -> Result<SignatureVerdict, String> {
    let verdict = verify_authenticode_signer(binary)?;
    let reported = parse_ollama_version(binary);
    if reported.is_empty() {
        return Err("Staged runtime did not report a version. Install aborted.".to_string());
    }
    if reported != expected_version {
        return Err(format!(
            "Staged runtime reports version {} but {} was expected. Install aborted.",
            reported, expected_version
        ));
    }
    Ok(verdict)
}

#[cfg(test)]
fn verify_staged_runtime(binary: &Path, expected_version: &str) -> Result<(), String> {
    verify_staged_runtime_verdict(binary, expected_version).map(|_| ())
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

/// The tail of an install: rewinds the archive that was just hashed, opens it as a ZIP and
/// runs the stage / verify / swap pipeline. A damaged archive fails here, before anything is
/// written, or while unpacking, when the staging folder is removed again; either way an
/// existing install stays as it was.
fn install_archive<R: Read + Seek>(
    mut archive: R,
    runtime_root: &Path,
    version: &str,
    max_total_bytes: u64,
    verify_staged: &dyn Fn(&Path) -> Result<(), String>,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<PathBuf, String> {
    archive
        .seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to rewind runtime archive: {}", e))?;
    let mut zip =
        ZipArchive::new(archive).map_err(|e| format!("Invalid runtime ZIP archive: {}", e))?;
    stage_and_swap_runtime(
        &mut zip,
        runtime_root,
        version,
        max_total_bytes,
        verify_staged,
        on_progress,
    )
}

/// Replacing the runtime folder of the version that is serving right now cannot work (Windows
/// keeps the running `ollama.exe` open), and the user would only learn that after unpacking
/// 2 GB. Says so up front.
fn install_blocked_by_running_server(
    version: &str,
    running_version: Option<&str>,
    managed_alive: bool,
) -> Option<String> {
    (managed_alive && running_version == Some(version)).then(|| {
        format!(
            "Ollama {} is running right now and cannot be replaced. Stop Ollama or switch to another runtime version, then install again.",
            version
        )
    })
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
    // Two installs at once would race on the staging and target folders.
    let _install_job = RuntimeJob::begin(
        "install".to_string(),
        "Another runtime install is already running.".to_string(),
    )?;

    // One locked handle for hashing AND unpacking: the archive cannot be swapped in between.
    let mut archive_file = open_archive_locked(&archive)
        .map_err(|e| format!("Failed to open runtime archive: {}", e))?;
    let archive_digest = sha256_reader(&mut archive_file)?;
    let manifest = resolve_runtime_archive_manifest_for_install(&archive, &archive_digest)?;
    if !is_safe_runtime_version(&manifest.version) {
        return Err(format!(
            "Refusing to install runtime with unsafe version string '{}'.",
            manifest.version
        ));
    }
    if managed_child_status(state.inner()).1 {
        let endpoint = state
            .settings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .providers
            .ollama
            .endpoint
            .clone();
        if let Some(message) = install_blocked_by_running_server(
            &manifest.version,
            query_running_ollama_version(&endpoint).as_deref(),
            true,
        ) {
            return Err(message);
        }
    }

    emit_install_progress(
        app,
        "install_runtime",
        format!("Installing Ollama runtime {}", manifest.version),
        None,
        None,
        Some(manifest.version.to_string()),
    );
    let _ = write_runtime_archive_metadata(&archive, &manifest);

    let runtime_root = resolve_runtime_root(app);
    cleanup_stale_runtime_dirs(&runtime_root);

    let runtime_binary = install_archive(
        archive_file, // moved: the archive lock is released when the ZIP reader is dropped
        &runtime_root,
        &manifest.version,
        MAX_RUNTIME_EXTRACTED_BYTES,
        &|staged_binary| {
            emit_install_progress(
                app,
                "install_runtime",
                "Verifying runtime signature and version...".to_string(),
                None,
                None,
                Some(manifest.version.to_string()),
            );
            match verify_staged_runtime_verdict(staged_binary, &manifest.version) {
                Ok(SignatureVerdict::Verified) => Ok(()),
                Ok(SignatureVerdict::Unverified(reason)) => {
                    // Explicit, never silent: the install goes on because the SHA-256 matched.
                    emit_install_progress(
                        app,
                        "install_runtime",
                        format!(
                            "Publisher signature not verified: {}. Continuing because the download matches its published SHA-256.",
                            reason
                        ),
                        None,
                        None,
                        Some(manifest.version.to_string()),
                    );
                    Ok(())
                }
                Err(msg) => {
                    emit_install_error(app, "install_runtime", msg.clone());
                    Err(msg)
                }
            }
        },
        &mut |done, total| {
            emit_install_progress(
                app,
                "install_runtime",
                format!("Extracting runtime files ({}/{})", done, total),
                Some(done as u64),
                Some(total as u64),
                Some(manifest.version.to_string()),
            );
        },
    )?;

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

// ── starting the managed server ───────────────────────────────────────────────

/// How long a managed server that does not answer yet is given to finish starting. Within it a
/// new start request waits for that server; after it the server counts as stuck and a fresh
/// one is spawned. (Cold GPU init or a virus scan of the 2 GB runtime can take longer than the
/// 12 s a single start waits in the foreground.)
const STARTING_GRACE: Duration = Duration::from_secs(90);

/// When the managed server was last spawned.
static MANAGED_SPAWNED_AT: Mutex<Option<Instant>> = Mutex::new(None);

fn note_managed_spawn() {
    *MANAGED_SPAWNED_AT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Instant::now());
}

fn managed_spawn_age() -> Option<Duration> {
    MANAGED_SPAWNED_AT
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .map(|spawned| spawned.elapsed())
}

/// One start at a time. Two at once (backend bootstrap + frontend autostart) would each stop
/// and respawn the other's server; the second one waits and then finds the first one's result
/// (`AlreadyRunning`). Held by `resolve_start` for its whole observe-and-act sequence.
static START_LOCK: Mutex<()> = Mutex::new(());

/// What the process table and the port add to a `Spawn` decision:
/// - our own server is alive but silent and young: wait for it instead of killing it mid-start;
/// - nobody of ours runs, nothing answers, yet the port accepts connections: another program
///   holds it, and a second `ollama serve` would only die with "address already in use".
fn refine_spawn_plan(
    managed_alive: bool,
    managed_age: Option<Duration>,
    port_in_use: impl FnOnce() -> bool,
) -> RuntimeStartPlan {
    if managed_alive {
        return if managed_age.is_some_and(|age| age < STARTING_GRACE) {
            RuntimeStartPlan::AwaitStarting
        } else {
            RuntimeStartPlan::Spawn
        };
    }
    if port_in_use() {
        RuntimeStartPlan::PortBlocked
    } else {
        RuntimeStartPlan::Spawn
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChildState {
    Alive,
    /// The process ended (exit code when it had one); the slot is emptied.
    Exited(Option<i32>),
    /// No child in the slot: it was stopped by someone else.
    Gone,
}

fn managed_child_state(slot: &Mutex<Option<std::process::Child>>) -> ChildState {
    let Ok(mut guard) = slot.lock() else {
        return ChildState::Gone;
    };
    let Some(child) = guard.as_mut() else {
        return ChildState::Gone;
    };
    match child.try_wait() {
        Ok(None) => ChildState::Alive,
        Ok(Some(status)) => {
            *guard = None;
            ChildState::Exited(status.code())
        }
        Err(_) => ChildState::Exited(None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WaitOutcome {
    Ready,
    Exited(Option<i32>),
    Gone,
    TimedOut,
}

/// Waits up to `wait` for the server of `slot` to answer. A child that dies, or is stopped by
/// someone else, ends the wait at once: waiting the full time on a corpse only delays the
/// error and makes a dead start look like a slow one.
fn wait_for_server(
    slot: &Mutex<Option<std::process::Child>>,
    wait: Duration,
    poll: Duration,
    answers: &mut dyn FnMut(u32) -> bool,
) -> WaitOutcome {
    let deadline = Instant::now() + wait;
    let mut attempt: u32 = 0;
    loop {
        match managed_child_state(slot) {
            ChildState::Exited(code) => return WaitOutcome::Exited(code),
            ChildState::Gone => return WaitOutcome::Gone,
            ChildState::Alive => {}
        }
        if answers(attempt) {
            return WaitOutcome::Ready;
        }
        if Instant::now() >= deadline {
            return WaitOutcome::TimedOut;
        }
        attempt = attempt.saturating_add(1);
        std::thread::sleep(poll);
    }
}

/// How one attempt to bring a server up ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchOutcome {
    Ready {
        pid: u32,
        waited: Duration,
    },
    /// Spawned, but not answering yet when the foreground wait ended.
    Pending {
        pid: u32,
        waited: Duration,
    },
    /// Our process ended, yet a server answers on the endpoint: another program took the port.
    TakenOver,
    Failed {
        stage: &'static str,
        message: String,
    },
}

/// Everything `resolve_start` observes and does to the outside world, so the decision logic
/// can be driven by a fake in tests (the production host is `RealHost`).
trait StartHost {
    fn endpoint_reachable(&self) -> bool;
    /// Version the answering server reports.
    fn running_version(&self) -> Option<String>;
    fn managed_alive(&self) -> bool;
    fn managed_spawn_age(&self) -> Option<Duration>;
    fn port_in_use(&self) -> bool;
    fn endpoint_label(&self) -> String;
    /// Frees a port that an Ollama of this app holds without answering (a hung leftover of a
    /// crashed session, found through its pid file; only a process that is still an
    /// `ollama.exe` is killed). `true` when the port is free afterwards.
    fn reclaim_stale_port(&mut self) -> bool;
    /// Stops our managed server and waits for its port to be released; `false` when it is
    /// still taken afterwards.
    fn stop_managed_and_free_port(&mut self) -> bool;
    /// Waits for our already running, still silent server to come up.
    fn await_starting_server(&mut self) -> LaunchOutcome;
    /// Spawns the server from `binary` and waits for it.
    fn launch(&mut self, binary: &Path) -> LaunchOutcome;
}

struct LaunchResult {
    /// What happened with `binary`.
    outcome: LaunchOutcome,
    binary: PathBuf,
    /// The wanted runtime failed and `binary` is the previous one that was started instead.
    fell_back: bool,
    /// Why the wanted runtime failed (set when `fell_back`).
    wanted_failure: Option<String>,
}

/// Starts `wanted`; when that cannot be brought up and a `previous` runtime is known, starts
/// that one so the user is not left without a runtime after the old server was already stopped
/// for the switch. A server that is merely slow (`Pending`) is no failure and no fallback.
fn launch_with_fallback(
    host: &mut dyn StartHost,
    wanted: &Path,
    previous: Option<&Path>,
) -> LaunchResult {
    let first = host.launch(wanted);
    let wanted_failure = match &first {
        LaunchOutcome::Failed { message, .. } => Some(message.clone()),
        _ => None,
    };
    match (
        wanted_failure,
        previous.filter(|previous| *previous != wanted),
    ) {
        (Some(reason), Some(previous)) => LaunchResult {
            outcome: host.launch(previous),
            binary: previous.to_path_buf(),
            fell_back: true,
            wanted_failure: Some(reason),
        },
        _ => LaunchResult {
            outcome: first,
            binary: wanted.to_path_buf(),
            fell_back: false,
            wanted_failure: None,
        },
    }
}

/// How a start request ended up.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StartResolution {
    /// A server answers and is left alone (`running_version` is what it reports).
    AlreadyRunning { running_version: String },
    /// The wanted runtime is up (`Ready`), still coming up (`Pending`) or another program
    /// answers on the port (`TakenOver`).
    Launched(LaunchOutcome),
    /// The wanted runtime failed to start and the previous one was started instead.
    FellBack {
        outcome: LaunchOutcome,
        binary: PathBuf,
        previous_version: String,
        /// Text for the user: what failed and that the previous runtime serves again.
        message: String,
    },
    Failed {
        stage: &'static str,
        message: String,
    },
}

fn runtime_label(version: &str) -> String {
    if version.is_empty() {
        "the selected runtime".to_string()
    } else {
        format!("Ollama {}", version)
    }
}

/// Observes the endpoint and brings the wanted runtime up: the whole sequence runs under
/// `START_LOCK`, so a second request that arrives meanwhile queues up and then sees the
/// first one's server. `previous_of(running_version)` names the binary of the server that is
/// replaced by a version switch (for going back to it when the switch fails).
fn resolve_start(
    host: &mut dyn StartHost,
    wanted: &Path,
    wanted_version: &str,
    previous_of: &dyn Fn(&str) -> Option<PathBuf>,
) -> StartResolution {
    let _serial = START_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reachable = host.endpoint_reachable();
    let running_version = if reachable {
        host.running_version()
    } else {
        None
    };
    let managed_alive = host.managed_alive();
    let mut plan = plan_runtime_start(
        reachable,
        running_version.as_deref(),
        wanted_version,
        managed_alive,
    );
    if plan == RuntimeStartPlan::Spawn {
        plan = refine_spawn_plan(managed_alive, host.managed_spawn_age(), || {
            host.port_in_use()
        });
    }

    // Version switch: our own managed server serves another build than the selected runtime.
    // Stop it so the selected binary is the one that runs, but remember what ran so a failing
    // switch can go back to it.
    let mut previous: Option<(PathBuf, String)> = None;
    match plan {
        RuntimeStartPlan::AlreadyRunning { version } => {
            return StartResolution::AlreadyRunning {
                running_version: version,
            }
        }
        RuntimeStartPlan::PortBlocked => {
            // Before the port counts as foreign: a hung Ollama that this app left behind
            // (crash, hard kill) is ours to clear away, as it always was.
            if !host.reclaim_stale_port() {
                return StartResolution::Failed {
                    stage: "port_in_use",
                    message: format!(
                        "{} is already used by a program that does not answer like Ollama (or by an Ollama that is too busy to answer). Close that program or change the Ollama endpoint, then try again.",
                        host.endpoint_label()
                    ),
                };
            }
        }
        RuntimeStartPlan::SwitchManaged { running, wanted } => {
            info!(
                "[ollama.runtime] switching managed runtime {} -> {}",
                running, wanted
            );
            previous = previous_of(&running).map(|path| (path, running.clone()));
            if !host.stop_managed_and_free_port() {
                return StartResolution::Failed {
                    stage: "version_switch_failed",
                    message: format!(
                        "Previous Ollama runtime {} is still bound to {}. Stop it and retry.",
                        running,
                        host.endpoint_label()
                    ),
                };
            }
        }
        RuntimeStartPlan::AwaitStarting => {
            // Our own server is still starting: give it the time instead of killing it. If it
            // died meanwhile, a fresh one is started below.
            let outcome = host.await_starting_server();
            if matches!(
                outcome,
                LaunchOutcome::Ready { .. } | LaunchOutcome::Pending { .. }
            ) {
                return StartResolution::Launched(outcome);
            }
        }
        RuntimeStartPlan::Spawn => {}
    }

    let launched = launch_with_fallback(
        host,
        wanted,
        previous.as_ref().map(|(path, _)| path.as_path()),
    );
    if !launched.fell_back {
        return match launched.outcome {
            LaunchOutcome::Failed { stage, message } => StartResolution::Failed {
                stage,
                message: format!("Failed to start Ollama runtime ({}): {}", stage, message),
            },
            other => StartResolution::Launched(other),
        };
    }

    let previous_version = previous.map(|(_, version)| version).unwrap_or_default();
    let wanted_failure = launched.wanted_failure.unwrap_or_default();
    let wanted_label = runtime_label(wanted_version);
    match launched.outcome {
        LaunchOutcome::Failed { message, .. } => StartResolution::Failed {
            stage: "version_switch_failed",
            message: format!(
                "Could not start {} ({}), and the previous runtime {} could not be started again either ({}). Start Ollama manually or select another runtime version.",
                wanted_label, wanted_failure, previous_version, message
            ),
        },
        outcome => StartResolution::FellBack {
            outcome,
            binary: launched.binary,
            message: format!(
                "Could not start {} ({}). The previous runtime {} was started again, so Ollama keeps working.",
                wanted_label, wanted_failure, previous_version
            ),
            previous_version,
        },
    }
}

/// The runtime that serves right now (`running` version), to go back to when a switch fails:
/// the stored path when it belongs to that version, else the managed folder of that version.
fn previous_runtime_binary(
    settings: &Settings,
    managed_root: Option<&Path>,
    running: &str,
) -> Option<PathBuf> {
    let ollama = &settings.providers.ollama;
    let stored = ollama.runtime_path.trim();
    if !stored.is_empty() && ollama.runtime_version.trim() == running {
        let path = PathBuf::from(stored);
        if path.is_file() {
            return Some(path);
        }
    }
    if !is_safe_runtime_version(running) {
        return None;
    }
    let dir = managed_root?.join(running);
    if dir.is_dir() {
        find_file_recursive(&dir, "ollama.exe")
    } else {
        None
    }
}

fn describe_exit(code: Option<i32>) -> String {
    match code {
        Some(code) => format!(
            "Ollama exited right after it was started (exit code {}). The port may be in use by another program or the runtime files may be damaged.",
            code
        ),
        None => "Ollama exited right after it was started. The port may be in use by another program or the runtime files may be damaged.".to_string(),
    }
}

/// The production `StartHost`: probes the real endpoint, spawns `ollama serve`.
struct RealHost<'a> {
    app: &'a AppHandle,
    state: &'a AppState,
    settings: &'a Settings,
    endpoint: &'a str,
    /// `host:port` the server binds to.
    host: &'a str,
}

impl RealHost<'_> {
    fn answers(&self, attempt: u32) -> bool {
        // Uncached on purpose (see `ping_ollama_quick_fresh`): the cached answer still
        // describes the server that was just stopped for the switch.
        ping_ollama_quick_fresh(self.endpoint).is_ok()
            || (attempt % 4 == 3 && ping_ollama(self.endpoint).is_ok())
    }

    fn finish_wait(&self, outcome: WaitOutcome, pid: u32, waited: Duration) -> LaunchOutcome {
        match outcome {
            WaitOutcome::Ready => LaunchOutcome::Ready { pid, waited },
            WaitOutcome::TimedOut => LaunchOutcome::Pending { pid, waited },
            WaitOutcome::Exited(code) => {
                clear_ollama_pid_lockfile(self.app);
                if runtime_endpoint_reachable(self.endpoint) {
                    LaunchOutcome::TakenOver
                } else {
                    LaunchOutcome::Failed {
                        stage: "spawn_exited",
                        message: describe_exit(code),
                    }
                }
            }
            WaitOutcome::Gone => LaunchOutcome::Failed {
                stage: "stopped",
                message: "Ollama was stopped while it was starting.".to_string(),
            },
        }
    }
}

impl StartHost for RealHost<'_> {
    fn endpoint_reachable(&self) -> bool {
        runtime_endpoint_reachable(self.endpoint)
    }

    fn running_version(&self) -> Option<String> {
        query_running_ollama_version(self.endpoint)
    }

    fn managed_alive(&self) -> bool {
        managed_child_status(self.state).1
    }

    fn managed_spawn_age(&self) -> Option<Duration> {
        managed_spawn_age()
    }

    fn port_in_use(&self) -> bool {
        endpoint_port_in_use(self.endpoint)
    }

    fn endpoint_label(&self) -> String {
        endpoint_host_port(self.endpoint).unwrap_or_else(|_| self.endpoint.to_string())
    }

    fn reclaim_stale_port(&mut self) -> bool {
        // Kills the process of the pid file when it still is an ollama.exe (no-op otherwise).
        kill_stale_ollama_pid(self.app);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && endpoint_port_in_use(self.endpoint) {
            std::thread::sleep(Duration::from_millis(200));
        }
        !endpoint_port_in_use(self.endpoint)
    }

    fn stop_managed_and_free_port(&mut self) -> bool {
        terminate_managed_child_slot("managed Ollama runtime", &self.state.managed_ollama_child);
        reset_quick_ping_cache();
        clear_ollama_pid_lockfile(self.app);
        // Wait for the PORT to be free (not for "any endpoint answers").
        let release_deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < release_deadline && endpoint_port_in_use(self.endpoint) {
            std::thread::sleep(Duration::from_millis(300));
        }
        !endpoint_port_in_use(self.endpoint)
    }

    fn await_starting_server(&mut self) -> LaunchOutcome {
        let pid = managed_child_status(self.state).0.unwrap_or_default();
        let started = Instant::now();
        let outcome = wait_for_server(
            &self.state.managed_ollama_child,
            Duration::from_millis(STARTUP_FOREGROUND_WAIT_MS),
            Duration::from_millis(500),
            &mut |attempt| self.answers(attempt),
        );
        self.finish_wait(outcome, pid, started.elapsed())
    }

    fn launch(&mut self, binary_path: &Path) -> LaunchOutcome {
        let (app, state, endpoint) = (self.app, self.state, self.endpoint);
        let runtime_dep_dirs = runtime_dependency_dirs(binary_path);
        let runners_dir = binary_path
            .parent()
            .map(|dir| dir.join("lib").join("ollama"))
            .filter(|dir| dir.is_dir());
        // Kill any Ollama left over from a previous session (crash or hard-kill)
        kill_stale_ollama_pid(app);
        terminate_managed_child_slot("managed Ollama runtime", &state.managed_ollama_child);
        let mut cmd = Command::new(binary_path);
        // Default to a short post-request keep-alive window so real refinement stays
        // responsive without forcing idle GPU residency forever.
        let server_keep_alive = std::env::var("TRISPR_OLLAMA_KEEP_ALIVE")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| "90s".to_string());
        if crate::state::diagnostic_logging_enabled() {
            info!(
                "[ollama.runtime] spawning managed runtime binary={} host={} keep_alive={} runners_dir_present={} dependency_dirs={}",
                binary_path.display(),
                self.host,
                server_keep_alive,
                runners_dir.is_some(),
                runtime_dep_dirs.len()
            );
        }
        cmd.arg("serve")
            .env("OLLAMA_HOST", self.host)
            .env("OLLAMA_NO_CLOUD", "1")
            .env("OLLAMA_KEEP_ALIVE", &server_keep_alive)
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
        let spawn_result = match spawn_managed_child(
            state,
            "managed Ollama runtime",
            &state.managed_ollama_child,
            &mut cmd,
        ) {
            Ok(result) => result,
            Err(e) => {
                return LaunchOutcome::Failed {
                    stage: "spawn_failed",
                    message: e.trim_start_matches("spawn_failed: ").trim().to_string(),
                }
            }
        };
        note_managed_spawn();
        reset_quick_ping_cache();
        let pid = spawn_result.pid;
        if crate::state::diagnostic_logging_enabled() {
            info!(
                "[ollama.runtime] spawn success pid={} job_assigned={} endpoint={} keep_alive={}",
                pid, spawn_result.job_assigned, endpoint, server_keep_alive
            );
        }
        // Persist PID so we can clean it up even if the app crashes before proper shutdown
        write_ollama_pid_lockfile(app);
        if !spawn_result.job_assigned {
            update_ollama_runtime_diagnostics(
                app,
                self.settings,
                true,
                "job_assign_failed",
                Some(pid),
                false,
                Some(
                    "Job assignment failed; explicit process-tree cleanup remains active."
                        .to_string(),
                ),
            );
        }

        let wait_started = Instant::now();
        let outcome = wait_for_server(
            &state.managed_ollama_child,
            Duration::from_millis(STARTUP_FOREGROUND_WAIT_MS),
            Duration::from_millis(500),
            &mut |attempt| self.answers(attempt),
        );
        self.finish_wait(outcome, pid, wait_started.elapsed())
    }
}

/// Heals what an interrupted install left in the runtime folder before a binary is chosen
/// (a crash between the two renames of the swap would otherwise cost the user their runtime),
/// and removes old unpacked leftovers and abandoned downloads. Skipped while an install runs.
fn tidy_runtime_folders(app: &AppHandle) {
    let Ok(_job) = RuntimeJob::begin("install".to_string(), String::new()) else {
        return;
    };
    if let Some(root) = managed_runtime_root(app) {
        cleanup_stale_runtime_dirs(&root);
    }
    cleanup_stale_download_parts(&resolve_runtime_cache_dir(app), Duration::from_secs(3600));
}

/// Everything about one start request that stays the same across its outcomes.
struct StartContext<'a> {
    app: &'a AppHandle,
    state: &'a AppState,
    settings: &'a Settings,
    endpoint: &'a str,
}

impl StartContext<'_> {
    /// A server answers that this start did not spawn: the selected binary and ITS version are
    /// recorded, the server is left alone.
    fn already_running(
        &self,
        source: &str,
        binary: &Path,
        recorded_version: &str,
        running_version: &str,
    ) -> OllamaRuntimeStartResult {
        let _ = update_runtime_in_settings(
            self.app,
            self.state,
            source.to_string(),
            binary.to_string_lossy().to_string(),
            recorded_version.to_string(),
            Some(now_iso()),
            false,
        );
        if crate::state::diagnostic_logging_enabled() {
            info!(
                "[ollama.runtime] already reachable externally endpoint={} source={} running_version={} binary_version={}",
                self.endpoint, source, running_version, recorded_version
            );
        }
        let models = list_ollama_models(self.endpoint);
        emit_runtime_health(self.app, self.endpoint.to_string(), models.len(), true);
        update_ollama_runtime_diagnostics(
            self.app,
            self.settings,
            true,
            "running_externally",
            None,
            true,
            None,
        );
        OllamaRuntimeStartResult {
            pid: None,
            endpoint: self.endpoint.to_string(),
            source: source.to_string(),
            already_running: true,
            pending_start: false,
            startup_wait_ms: 0,
        }
    }

    fn ready(
        &self,
        source: &str,
        binary: &Path,
        recorded_version: &str,
        pid: u32,
        waited: Duration,
    ) -> Result<OllamaRuntimeStartResult, String> {
        update_runtime_in_settings(
            self.app,
            self.state,
            source.to_string(),
            binary.to_string_lossy().to_string(),
            recorded_version.to_string(),
            Some(now_iso()),
            false,
        )?;
        let models = list_ollama_models(self.endpoint);
        if crate::state::diagnostic_logging_enabled() {
            info!(
                "[ollama.runtime] ready pid={} endpoint={} models={} elapsed_ms={}",
                pid,
                self.endpoint,
                models.len(),
                waited.as_millis()
            );
        }
        emit_runtime_health(self.app, self.endpoint.to_string(), models.len(), true);
        update_ollama_runtime_diagnostics(
            self.app,
            self.settings,
            true,
            "ready",
            Some(pid),
            true,
            None,
        );
        Ok(OllamaRuntimeStartResult {
            pid: Some(pid),
            endpoint: self.endpoint.to_string(),
            source: source.to_string(),
            already_running: false,
            pending_start: false,
            startup_wait_ms: waited.as_millis() as u64,
        })
    }

    fn pending(
        &self,
        source: &str,
        binary: &Path,
        recorded_version: &str,
        pid: u32,
        waited: Duration,
    ) -> OllamaRuntimeStartResult {
        let _ = update_runtime_in_settings(
            self.app,
            self.state,
            source.to_string(),
            binary.to_string_lossy().to_string(),
            recorded_version.to_string(),
            Some(now_iso()),
            false,
        );
        if crate::state::diagnostic_logging_enabled() {
            info!(
                "[ollama.runtime] startup pending pid={} endpoint={} elapsed_ms={}",
                pid,
                self.endpoint,
                waited.as_millis()
            );
        }
        emit_runtime_health(self.app, self.endpoint.to_string(), 0, false);
        update_ollama_runtime_diagnostics(
            self.app,
            self.settings,
            true,
            "health_timeout",
            Some(pid),
            false,
            Some(format!(
                "Local runtime started but is not reachable yet after {} ms.",
                STARTUP_FOREGROUND_WAIT_MS
            )),
        );
        OllamaRuntimeStartResult {
            pid: Some(pid),
            endpoint: self.endpoint.to_string(),
            source: source.to_string(),
            already_running: false,
            pending_start: true,
            startup_wait_ms: waited.as_millis() as u64,
        }
    }
}

fn start_ollama_runtime_impl(app: &AppHandle) -> Result<OllamaRuntimeStartResult, String> {
    let state = app.state::<AppState>();
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if crate::state::diagnostic_logging_enabled() {
        info!(
            "[ollama.runtime] start requested endpoint={} runtime_path={} runtime_version={} model={}",
            settings_snapshot.providers.ollama.endpoint,
            settings_snapshot.providers.ollama.runtime_path,
            settings_snapshot.providers.ollama.runtime_version,
            settings_snapshot.ai_fallback.model
        );
    }
    let mark_failure = |stage: &str, message: String| {
        update_startup_status(app, state.inner(), |status| {
            status.ollama_ready = false;
            status.ollama_starting = false;
        });
        update_ollama_runtime_diagnostics(
            app,
            &settings_snapshot,
            !settings_snapshot
                .providers
                .ollama
                .runtime_path
                .trim()
                .is_empty(),
            stage,
            None,
            false,
            Some(message.clone()),
        );
        message
    };
    update_startup_status(app, state.inner(), |status| {
        // Guard against concurrent start calls clobbering a ready state.
        // Two start_ollama_runtime invocations can race (e.g. backend re-enable
        // bootstrap + frontend autostart). If the first call has already set
        // ollama_ready=true, the second call's entry must not reset it back to
        // starting=true, otherwise the UI keeps showing "Ollama is starting in
        // background." and the warmup readiness check sees stale false.
        if !status.ollama_ready {
            status.ollama_starting = true;
        }
    });
    update_ollama_runtime_diagnostics(
        app,
        &settings_snapshot,
        !settings_snapshot
            .providers
            .ollama
            .runtime_path
            .trim()
            .is_empty(),
        "starting",
        None,
        false,
        None,
    );
    let endpoint = settings_snapshot
        .providers
        .ollama
        .endpoint
        .trim()
        .to_string();
    if endpoint.is_empty() {
        return Err(mark_failure(
            "invalid_configuration",
            "Ollama endpoint is empty.".to_string(),
        ));
    }

    tidy_runtime_folders(app);
    let managed_root = managed_runtime_root(app);
    let (binary_path, source) = select_runtime_binary(app, &settings_snapshot)
        .map_err(|err| mark_failure("runtime_not_found", err))?;
    let version = parse_ollama_version(&binary_path);
    // (runtime_path, runtime_version) are stored as one pair: the selected binary and ITS
    // version, never the version of whatever server happens to answer.
    let recorded_version = version_to_record_for_binary(&settings_snapshot, &binary_path, &version);
    let host =
        endpoint_host_port(&endpoint).map_err(|err| mark_failure("invalid_configuration", err))?;
    let context = StartContext {
        app,
        state: state.inner(),
        settings: &settings_snapshot,
        endpoint: &endpoint,
    };
    let mut real_host = RealHost {
        app,
        state: state.inner(),
        settings: &settings_snapshot,
        endpoint: &endpoint,
        host: &host,
    };

    let resolution = resolve_start(&mut real_host, &binary_path, &version, &|running| {
        previous_runtime_binary(&settings_snapshot, managed_root.as_deref(), running)
    });
    match resolution {
        StartResolution::AlreadyRunning { running_version } => {
            Ok(context.already_running(&source, &binary_path, &recorded_version, &running_version))
        }
        StartResolution::Launched(LaunchOutcome::Ready { pid, waited }) => {
            context.ready(&source, &binary_path, &recorded_version, pid, waited)
        }
        StartResolution::Launched(LaunchOutcome::Pending { pid, waited }) => {
            Ok(context.pending(&source, &binary_path, &recorded_version, pid, waited))
        }
        StartResolution::Launched(LaunchOutcome::TakenOver) => {
            let running = query_running_ollama_version(&endpoint).unwrap_or_default();
            Ok(context.already_running(&source, &binary_path, &recorded_version, &running))
        }
        StartResolution::Launched(LaunchOutcome::Failed { stage, message }) => {
            Err(mark_failure(stage, message))
        }
        StartResolution::Failed { stage, message } => Err(mark_failure(stage, message)),
        StartResolution::FellBack {
            outcome,
            binary,
            previous_version,
            message,
        } => {
            // The selected runtime failed; the previous one serves again. The request still
            // failed (the user asked for another version), but the server is up.
            let previous_source = source.clone();
            let own_version = parse_ollama_version(&binary);
            let previous_recorded = version_to_record_for_binary(
                &settings_snapshot,
                &binary,
                if own_version.is_empty() {
                    &previous_version
                } else {
                    &own_version
                },
            );
            let (pid, reachable) = match outcome {
                LaunchOutcome::Ready { pid, waited } => {
                    let _ =
                        context.ready(&previous_source, &binary, &previous_recorded, pid, waited);
                    (Some(pid), true)
                }
                LaunchOutcome::Pending { pid, waited } => {
                    let _ =
                        context.pending(&previous_source, &binary, &previous_recorded, pid, waited);
                    (Some(pid), false)
                }
                _ => {
                    let running = query_running_ollama_version(&endpoint).unwrap_or_default();
                    let _ = context.already_running(
                        &previous_source,
                        &binary,
                        &previous_recorded,
                        &running,
                    );
                    (None, true)
                }
            };
            update_ollama_runtime_diagnostics(
                app,
                &settings_snapshot,
                true,
                "version_switch_failed",
                pid,
                reachable,
                Some(message.clone()),
            );
            Err(message)
        }
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
    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let endpoint = settings_snapshot
        .providers
        .ollama
        .endpoint
        .trim()
        .to_string();
    if crate::state::diagnostic_logging_enabled() {
        info!(
            "[ollama.runtime] verify requested endpoint={} source={} runtime_path={} runtime_version={}",
            endpoint,
            settings_snapshot.providers.ollama.runtime_source,
            settings_snapshot.providers.ollama.runtime_path,
            settings_snapshot.providers.ollama.runtime_version
        );
    }
    if endpoint.is_empty() {
        update_ollama_runtime_diagnostics(
            app,
            &settings_snapshot,
            !settings_snapshot
                .providers
                .ollama
                .runtime_path
                .trim()
                .is_empty(),
            "verify_failed",
            None,
            false,
            Some("Ollama endpoint is empty.".to_string()),
        );
        return Err("Ollama endpoint is empty.".to_string());
    }
    let models = list_ollama_models(&endpoint);
    if models.is_empty() {
        if let Err(err) = ping_ollama_quick_fresh(&endpoint) {
            update_startup_status(app, state.inner(), |status| {
                status.ollama_ready = false;
                status.ollama_starting = false;
            });
            emit_runtime_health(app, endpoint.clone(), 0, false);
            update_ollama_runtime_diagnostics(
                app,
                &settings_snapshot,
                !settings_snapshot
                    .providers
                    .ollama
                    .runtime_path
                    .trim()
                    .is_empty(),
                "verify_failed",
                None,
                false,
                Some(err.to_string()),
            );
            return Err(err.to_string());
        }
    }

    emit_runtime_health(app, endpoint.clone(), models.len(), true);
    // Reset exponential backoff so diagnostics polls resume immediately after verify
    crate::OLLAMA_DIAG_FAIL_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
    crate::OLLAMA_DIAG_NEXT_MS.store(0, std::sync::atomic::Ordering::Relaxed);
    update_startup_status(app, state.inner(), |status| {
        if status.ollama_starting {
            if crate::state::diagnostic_logging_enabled() {
                info!("Ollama startup status synced to ready via verify");
            }
        }
        status.ollama_ready = true;
        status.ollama_starting = false;
    });
    update_ollama_runtime_diagnostics(
        app,
        &settings_snapshot,
        !settings_snapshot
            .providers
            .ollama
            .runtime_path
            .trim()
            .is_empty(),
        "ready",
        managed_child_status(state.inner()).0,
        true,
        None,
    );
    if crate::state::diagnostic_logging_enabled() {
        info!(
            "[ollama.runtime] verify ok endpoint={} models={} managed_pid={:?}",
            endpoint,
            models.len(),
            managed_child_status(state.inner()).0
        );
    }

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

    let settings_snapshot = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    let endpoint = settings_snapshot.providers.ollama.endpoint.clone();
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
        let mut settings = state
            .settings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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

#[cfg(test)]
mod reliability_tests;

#[cfg(test)]
mod delete_tests;

#[cfg(test)]
mod target_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_settings() -> crate::state::Settings {
        let mut settings = crate::state::Settings::default();
        settings.providers.ollama.runtime_target_version = DEFAULT_RUNTIME_VERSION.to_string();
        settings.providers.ollama.runtime_version = String::new();
        settings
    }

    fn candidate(
        version: &str,
        prerelease: bool,
        installable: bool,
        reason: Option<&str>,
    ) -> OnlineRuntimeVersionCandidate {
        OnlineRuntimeVersionCandidate {
            version: version.to_string(),
            prerelease,
            installable,
            installable_reason: reason.map(str::to_string),
            release_name: Some(format!("Ollama {}", version)),
            release_notes_summary: None,
            released_at: Some("2026-10-18T00:28:57Z".to_string()),
            size_bytes: Some(1_582_514_985),
        }
    }

    #[test]
    fn build_version_list_includes_non_installable_online_versions() {
        let settings = base_settings();
        let online = vec![
            candidate("0.18.0", false, true, None),
            candidate(
                "0.18.1",
                true,
                false,
                Some("Missing ollama-windows-amd64.zip asset."),
            ),
        ];

        let versions = build_version_list(&settings, &online, None).expect("version list");

        assert!(versions.iter().any(|entry| entry.version == "0.18.0"));
        let blocked = versions
            .iter()
            .find(|entry| entry.version == "0.18.1")
            .expect("non-installable version should remain visible");
        assert!(!blocked.installable);
        assert!(blocked
            .installable_reason
            .as_deref()
            .unwrap_or_default()
            .contains("Missing ollama-windows-amd64.zip asset"));
    }

    #[test]
    fn build_version_list_keeps_selected_non_installable_with_reason() {
        let mut settings = base_settings();
        settings.providers.ollama.runtime_target_version = "0.18.1".to_string();
        let online = vec![candidate(
            "0.18.1",
            true,
            false,
            Some("Missing checksum asset for runtime verification."),
        )];

        let versions = build_version_list(&settings, &online, None).expect("version list");
        let selected = versions
            .iter()
            .find(|entry| entry.version == "0.18.1")
            .expect("selected version should remain visible");

        assert!(selected.selected);
        assert!(!selected.installable);
        assert_eq!(
            selected.installable_reason.as_deref(),
            Some("Missing checksum asset for runtime verification.")
        );
    }

    #[test]
    fn build_version_list_keeps_installed_non_installable_entry() {
        let mut settings = base_settings();
        settings.providers.ollama.runtime_version = "0.19.2".to_string();

        let versions = build_version_list(&settings, &[], None).expect("version list");
        let installed = versions
            .iter()
            .find(|entry| entry.version == "0.19.2")
            .expect("installed version should remain visible");

        assert!(installed.installed);
        assert!(!installed.installable);
        assert!(installed
            .installable_reason
            .as_deref()
            .unwrap_or_default()
            .contains("Installed locally"));
    }

    #[test]
    fn build_version_list_marks_latest_and_carries_release_details() {
        let settings = base_settings();
        let online = vec![
            candidate("0.40.13", false, true, None),
            candidate("0.35.1", false, true, None),
            candidate("0.41.0", true, true, None), // prerelease never counts as latest
        ];

        let versions = build_version_list(&settings, &online, None).expect("version list");
        let latest: Vec<_> = versions.iter().filter(|e| e.latest).collect();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].version, "0.40.13");
        assert_eq!(
            latest[0].released_at.as_deref(),
            Some("2026-10-18T00:28:57Z")
        );
        assert_eq!(latest[0].size_bytes, Some(1_582_514_985));
    }

    #[test]
    fn build_version_list_detects_runtimes_installed_on_disk() {
        let root =
            std::env::temp_dir().join(format!("trispr-runtime-test-{}", crate::util::now_ms()));
        let version_dir = root.join("0.30.4");
        fs::create_dir_all(&version_dir).expect("create version dir");
        fs::write(version_dir.join("ollama.exe"), b"stub").expect("write stub");
        fs::create_dir_all(root.join(".staging-0.99.0-1")).expect("create staging dir");

        let settings = base_settings();
        let versions =
            build_version_list(&settings, &[], Some(root.as_path())).expect("version list");
        let _ = fs::remove_dir_all(&root);

        let on_disk = versions
            .iter()
            .find(|entry| entry.version == "0.30.4")
            .expect("disk-installed version should be listed");
        assert!(on_disk.installed);
        assert!(on_disk.installed_at_ms.is_some());
        assert!(!versions.iter().any(|entry| entry.version == "0.99.0"));
    }

    #[test]
    fn select_major_line_versions_keeps_newest_patch_per_minor_line() {
        let tags: Vec<String> = [
            "v0.40.13",
            "v0.40.2",
            "v0.35.1",
            "v0.35.0",
            "v0.34.4",
            "v0.1.32-rc1",
            "v0.1.32",
            "vfoo",
            "v0.40",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(
            select_major_line_versions(&tags, 3),
            vec!["0.40.13", "0.35.1", "0.34.4"]
        );
        assert_eq!(select_major_line_versions(&tags, 10).len(), 4);
    }

    #[test]
    fn parse_version_output_prefers_client_version_over_running_server() {
        let with_server = "ollama version is 0.30.4\nWarning: client version is 0.40.7\n";
        let without_server =
            "Warning: could not connect to a running Ollama instance\nWarning: client version is 0.40.7\n";
        let plain = "ollama version is 0.20.2\n";
        assert_eq!(parse_version_output(with_server), "0.40.7");
        assert_eq!(parse_version_output(without_server), "0.40.7");
        assert_eq!(parse_version_output(plain), "0.20.2");
        assert_eq!(parse_version_output("garbage"), "");
    }

    #[test]
    fn runtime_download_url_must_be_the_official_release_asset() {
        let ok =
            "https://github.com/ollama/ollama/releases/download/v0.40.7/ollama-windows-amd64.zip";
        assert!(validate_runtime_download_url(ok, "0.40.7").is_ok());
        // wrong host, wrong version, credentials trick, wrong file, traversal
        assert!(validate_runtime_download_url(
            "https://evil.example/ollama/ollama/releases/download/v0.40.7/ollama-windows-amd64.zip",
            "0.40.7"
        )
        .is_err());
        assert!(validate_runtime_download_url(ok, "0.40.8").is_err());
        assert!(validate_runtime_download_url(
            "https://github.com/ollama/ollama/releases/download/v0.40.7/ollama-windows-amd64.zip@evil.example/x",
            "0.40.7"
        )
        .is_err());
        assert!(validate_runtime_download_url(
            "https://github.com/ollama/ollama/releases/download/v0.40.7/other.zip",
            "0.40.7"
        )
        .is_err());
        assert!(validate_runtime_download_url(ok, "../0.40.7").is_err());
    }

    /// Hits the real GitHub API. Run with: cargo test --lib live_online_catalog -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_online_catalog_lists_major_lines_with_verifiable_assets() {
        let versions = fetch_online_catalog_from(&GithubApi::official(), &["0.30.4".to_string()])
            .expect("online catalog")
            .versions;
        for v in &versions {
            println!(
                "{} installable={} prerelease={} released={:?} size={:?} reason={:?}",
                v.version,
                v.installable,
                v.prerelease,
                v.released_at,
                v.size_bytes,
                v.installable_reason
            );
        }
        assert!(versions.len() >= 2);
        assert!(versions.iter().any(|v| v.version == "0.30.4"));
        assert!(versions
            .iter()
            .any(|v| v.installable && v.size_bytes.is_some()));
    }

    #[test]
    fn runtime_version_strings_cannot_escape_the_runtime_folder() {
        assert!(is_safe_runtime_version("0.40.7"));
        assert!(is_safe_runtime_version("0.30.0-rc23"));
        assert!(!is_safe_runtime_version(""));
        assert!(!is_safe_runtime_version("..\\evil"));
        assert!(!is_safe_runtime_version("0.40.7/../../x"));
        assert!(!is_safe_runtime_version("a b"));
    }

    #[test]
    fn classify_online_release_installability_requires_archive_and_checksum() {
        let installable_assets = vec![
            json!({ "name": "ollama-windows-amd64.zip" }),
            json!({ "name": "checksums.sha256" }),
        ];
        let missing_archive = vec![json!({ "name": "checksums.sha256" })];
        let missing_checksum = vec![json!({ "name": "ollama-windows-amd64.zip" })];

        let (ok_installable, ok_reason) =
            classify_online_release_installability(&installable_assets);
        let (archive_installable, archive_reason) =
            classify_online_release_installability(&missing_archive);
        let (checksum_installable, checksum_reason) =
            classify_online_release_installability(&missing_checksum);

        assert!(ok_installable);
        assert!(ok_reason.is_none());
        assert!(!archive_installable);
        assert!(archive_reason
            .as_deref()
            .unwrap_or_default()
            .contains("ollama-windows-amd64.zip"));
        assert!(!checksum_installable);
        assert!(checksum_reason
            .as_deref()
            .unwrap_or_default()
            .contains("checksum"));
    }

    #[test]
    fn classify_online_release_installability_accepts_archive_digest_without_checksum_asset() {
        let installable_assets = vec![json!({
            "name": "ollama-windows-amd64.zip",
            "digest": "sha256:43cb0e09c62d8a3d1a9219ad5cc3ec3ca5a9070da3ead95578b62efb51b87b67"
        })];

        let (ok_installable, ok_reason) =
            classify_online_release_installability(&installable_assets);

        assert!(ok_installable);
        assert!(ok_reason.is_none());
    }

    #[test]
    fn build_version_list_merges_release_metadata_into_pinned_versions() {
        let settings = base_settings();
        let mut pinned_candidate = candidate(
            DEFAULT_RUNTIME_VERSION,
            false,
            false,
            Some("Missing checksum asset for runtime verification."),
        );
        pinned_candidate.release_name = Some("Ollama 0.20.2".to_string());
        pinned_candidate.release_notes_summary =
            Some("Stability fixes · Windows runtime polish".to_string());
        let online = vec![pinned_candidate];

        let versions = build_version_list(&settings, &online, None).expect("version list");
        let pinned = versions
            .iter()
            .find(|entry| entry.version == DEFAULT_RUNTIME_VERSION)
            .expect("pinned version should remain visible");

        assert_eq!(pinned.source, "pinned");
        assert!(pinned.installable);
        assert_eq!(pinned.release_name.as_deref(), Some("Ollama 0.20.2"));
        assert_eq!(
            pinned.release_notes_summary.as_deref(),
            Some("Stability fixes · Windows runtime polish")
        );
    }

    #[test]
    fn infer_runtime_version_from_archive_path_accepts_versioned_zip_name() {
        let archive = Path::new("C:/temp/ollama-windows-amd64-v0.30.0-rc23.zip");
        assert_eq!(
            infer_runtime_version_from_archive_path(archive).as_deref(),
            Some("0.30.0-rc23")
        );
    }

    #[test]
    fn summarize_release_notes_skips_noise_and_limits_length() {
        let body = r#"
## What's Changed
- Fix startup churn
- Add checksum sidecar

Full Changelog: https://github.com/ollama/ollama/compare/v0.20.1...v0.20.2
        "#;

        let summary = summarize_release_notes(Some(body)).expect("summary");
        assert!(summary.contains("Fix startup churn"));
        assert!(summary.contains("Add checksum sidecar"));
        assert!(!summary.contains("Full Changelog"));
    }

    #[test]
    fn parse_sha256_from_text_accepts_prefixed_relative_paths() {
        let text =
            "f26ead8baa55e919828ffe88cddf3e588444891e43e19a059f1d6c933fb1a5b6  ./ollama-windows-amd64.zip\n";
        let parsed = parse_sha256_from_text(text, "ollama-windows-amd64.zip");
        assert_eq!(
            parsed.as_deref(),
            Some("f26ead8baa55e919828ffe88cddf3e588444891e43e19a059f1d6c933fb1a5b6")
        );
    }

    // ── helpers for the proof tests below ────────────────────────────────────────

    /// Self-cleaning scratch directory under the system temp dir.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "trispr-ollama-rt-{}-{}-{}-{}",
                tag,
                std::process::id(),
                crate::util::now_ms(),
                unique
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// The signed 0.40.7 binary of the dev machine; tests skip cleanly when it is absent.
    fn real_signed_binary() -> Option<PathBuf> {
        let local = std::env::var_os("LOCALAPPDATA")?;
        let path = PathBuf::from(local)
            .join("Trispr Flow")
            .join("ollama-runtime")
            .join("0.40.7")
            .join("ollama.exe");
        if path.is_file() {
            Some(path)
        } else {
            eprintln!("SKIPPED: {} not present", path.display());
            None
        }
    }

    // ── 1. Authenticode proof ────────────────────────────────────────────────────

    #[cfg(target_os = "windows")]
    #[test]
    fn authenticode_accepts_the_real_signed_binary() {
        let Some(binary) = real_signed_binary() else {
            return;
        };
        verify_authenticode_signer(&binary).expect("genuine Ollama binary must pass");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn authenticode_rejects_a_binary_with_one_flipped_byte() {
        let Some(binary) = real_signed_binary() else {
            return;
        };
        let scratch = TempDir::new("tamper");
        let copy = scratch.path().join("ollama.exe");
        let mut bytes = fs::read(&binary).expect("read real binary");
        // Flip a byte in the middle of the image, far from the PE header and the
        // certificate table, so only the Authenticode hash can notice.
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0x01;
        fs::write(&copy, &bytes).expect("write tampered copy");

        let err = verify_authenticode_signer(&copy)
            .expect_err("tampered binary must NOT pass the signature check");
        assert!(err.contains("HashMismatch"), "unexpected error: {}", err);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn authenticode_rejects_unsigned_and_non_pe_files() {
        let scratch = TempDir::new("unsigned");
        let text = scratch.path().join("ollama.exe");
        fs::write(&text, b"definitely not a signed executable").expect("write text file");
        assert!(verify_authenticode_signer(&text).is_err());

        // The test binary itself is a real, unsigned PE file.
        let own = std::env::current_exe().expect("current exe");
        let unsigned_pe = scratch.path().join("unsigned.exe");
        fs::copy(&own, &unsigned_pe).expect("copy test exe");
        let err = verify_authenticode_signer(&unsigned_pe).expect_err("unsigned PE must fail");
        assert!(err.contains("NotSigned"), "unexpected error: {}", err);
    }

    #[cfg(target_os = "windows")]
    /// Installs the real archive into the managed runtime root through the same hardened path
    /// as the app (locked handle, pinned SHA-256, limits, signature + version check, safe swap).
    /// Run: TRISPR_LIVE_RUNTIME_ZIP=<zip> TRISPR_LIVE_RUNTIME_VERSION=0.40.13 cargo test --lib
    ///      live_install_real_archive -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_install_real_archive_into_managed_root() {
        let (Ok(zip_path), Ok(version)) = (
            std::env::var("TRISPR_LIVE_RUNTIME_ZIP"),
            std::env::var("TRISPR_LIVE_RUNTIME_VERSION"),
        ) else {
            println!("skipped: TRISPR_LIVE_RUNTIME_ZIP / TRISPR_LIVE_RUNTIME_VERSION not set");
            return;
        };
        let pinned = resolve_pinned_manifest(&version).expect("version must be the pinned one");
        let mut file = open_archive_locked(Path::new(&zip_path)).expect("open archive");
        let digest = sha256_reader(&mut file).expect("hash archive");
        assert_eq!(
            digest, pinned.sha256,
            "archive must match the pinned SHA-256"
        );
        file.seek(SeekFrom::Start(0)).expect("rewind");
        let mut zip = ZipArchive::new(file).expect("valid zip");
        let root = PathBuf::from(std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA"))
            .join("Trispr Flow")
            .join("ollama-runtime");
        cleanup_stale_runtime_dirs(&root);

        let started = Instant::now();
        let binary = stage_and_swap_runtime(
            &mut zip,
            &root,
            &version,
            MAX_RUNTIME_EXTRACTED_BYTES,
            &|staged| verify_staged_runtime(staged, &version),
            &mut |_, _| {},
        )
        .expect("hardened install");
        println!(
            "installed {} -> {} in {:.1}s",
            version,
            binary.display(),
            started.elapsed().as_secs_f32()
        );
        assert!(binary.exists());
        assert_eq!(parse_ollama_version(&binary), version);
        assert!(
            fs::read_dir(&root)
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with(".staging-")),
            "no staging leftovers"
        );
    }

    #[test]
    fn verify_staged_runtime_rejects_a_wrong_expected_version() {
        let Some(binary) = real_signed_binary() else {
            return;
        };
        verify_staged_runtime(&binary, "0.40.7").expect("matching version passes");
        let err = verify_staged_runtime(&binary, "0.40.8")
            .expect_err("wrong expected version must be rejected");
        assert!(
            err.contains("reports version 0.40.7") && err.contains("0.40.8 was expected"),
            "unexpected error: {}",
            err
        );
    }

    // ── 1b. Signature probe: hostile paths, subject parsing, PowerShell missing ─────

    #[cfg(target_os = "windows")]
    #[test]
    fn authenticode_probe_treats_hostile_paths_as_data() {
        let Some(binary) = real_signed_binary() else {
            return;
        };
        // U+2019 closes a PowerShell single-quoted string just like `'` does. The previous
        // implementation spliced the path into the script and only doubled the ASCII quote,
        // so this folder name ran `New-Item` (verified by hand against that version).
        let marker = format!("trispr-injection-{}.marker", std::process::id());
        let scratch = TempDir::new("hostile");
        let hostile_dir = scratch.path().join(format!(
            "x\u{2019}; New-Item -ItemType File -Name {}; \u{2019}y $(Get-Date) `n 'q' &%^!",
            marker
        ));
        fs::create_dir_all(&hostile_dir).expect("create hostile dir");

        let good = hostile_dir.join("ollama.exe");
        fs::copy(&binary, &good).expect("copy real binary");
        verify_authenticode_signer(&good).expect("genuine binary passes from a hostile path");

        // A pass alone could be the fail-open branch (empty probe output). The tampered
        // copy at the same path must be judged for real.
        let mut bytes = fs::read(&binary).expect("read real binary");
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0x01;
        let tampered = hostile_dir.join("tampered.exe");
        fs::write(&tampered, &bytes).expect("write tampered copy");
        let err = verify_authenticode_signer(&tampered).expect_err("tampered copy must fail");
        assert!(err.contains("HashMismatch"), "unexpected error: {}", err);

        let injected = Path::new(&marker).exists();
        let _ = fs::remove_file(&marker);
        assert!(!injected, "path was executed as PowerShell code");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn signature_check_fails_open_when_powershell_is_missing() {
        // Policy, written down on purpose: if the probe cannot run at all, the install
        // continues on the SHA-256 that was already verified. A tampered binary is NOT
        // caught in that case. Switching to fail-closed is a one-line change in
        // `verify_authenticode_signer_with` and must flip this test.
        let Some(binary) = real_signed_binary() else {
            return;
        };
        let scratch = TempDir::new("nops");
        let mut bytes = fs::read(&binary).expect("read real binary");
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0x01;
        let tampered = scratch.path().join("ollama.exe");
        fs::write(&tampered, &bytes).expect("write tampered copy");

        let missing = scratch.path().join("no-such-powershell.exe");
        assert!(verify_authenticode_signer_with(&missing, &tampered).is_ok());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_is_resolved_to_an_absolute_system_path() {
        let path = windows_powershell_path();
        assert!(path.is_absolute(), "got {}", path.display());
        assert!(path.is_file());
        assert!(path.ends_with(Path::new("WindowsPowerShell/v1.0/powershell.exe")));
    }

    #[test]
    fn subject_organization_must_match_exactly() {
        let real =
            "CN=Ollama Inc., O=Ollama Inc., L=Toronto, S=Ontario, C=CA, SERIALNUMBER=2713355";
        assert!(subject_has_organization(real, "Ollama Inc."));
        assert!(subject_has_organization("O=Ollama Inc.", "Ollama Inc."));
        assert!(subject_has_organization(
            "CN=x, O=\"Ollama Inc.\"",
            "Ollama Inc."
        ));
        // substring tricks the old `contains("O=Ollama Inc.")` accepted
        assert!(!subject_has_organization(
            "CN=x, O=Ollama Inc.Fake",
            "Ollama Inc."
        ));
        assert!(!subject_has_organization(
            "CN=O=Ollama Inc., O=Evil",
            "Ollama Inc."
        ));
        assert!(!subject_has_organization("OU=O=Ollama Inc.", "Ollama Inc."));
        assert!(!subject_has_organization(
            "CN=x, O=\"Ollama Inc., Evil\"",
            "Ollama Inc."
        ));
        assert!(!subject_has_organization("CN=Ollama Inc.", "Ollama Inc."));
        assert!(!subject_has_organization("", "Ollama Inc."));
    }

    #[test]
    fn signature_report_is_judged_strictly() {
        let genuine = "Valid|CN=Ollama Inc., O=Ollama Inc., L=Toronto, C=CA";
        assert!(evaluate_signature_report(genuine).is_ok());

        let tampered = evaluate_signature_report("HashMismatch|CN=Ollama Inc., O=Ollama Inc.");
        assert!(tampered.unwrap_err().contains("HashMismatch"));
        let unsigned = evaluate_signature_report("NotSigned|");
        assert!(unsigned.unwrap_err().contains("NotSigned"));
        let stranger = evaluate_signature_report("Valid|CN=Somebody Else, O=Somebody Else");
        assert!(stranger.unwrap_err().contains("unexpected publisher"));
        // status alone, no subject: still not good enough
        assert!(evaluate_signature_report("Valid").is_err());
    }

    // ── 2. plan_runtime_start (version switch decision) ─────────────────────────────

    #[test]
    fn start_plan_nothing_listening_means_spawn() {
        assert_eq!(
            plan_runtime_start(false, None, "0.40.7", false),
            RuntimeStartPlan::Spawn
        );
        // stale observations are irrelevant when nothing answers
        assert_eq!(
            plan_runtime_start(false, Some("0.30.4"), "0.40.7", true),
            RuntimeStartPlan::Spawn
        );
    }

    #[test]
    fn start_plan_same_version_is_already_running() {
        assert_eq!(
            plan_runtime_start(true, Some("0.40.7"), "0.40.7", true),
            RuntimeStartPlan::AlreadyRunning {
                version: "0.40.7".into()
            }
        );
        assert_eq!(
            plan_runtime_start(true, Some("0.40.7"), "0.40.7", false),
            RuntimeStartPlan::AlreadyRunning {
                version: "0.40.7".into()
            }
        );
    }

    #[test]
    fn start_plan_managed_server_on_other_version_is_switched() {
        assert_eq!(
            plan_runtime_start(true, Some("0.30.4"), "0.40.7", true),
            RuntimeStartPlan::SwitchManaged {
                running: "0.30.4".into(),
                wanted: "0.40.7".into()
            }
        );
        // downgrades switch just the same
        assert_eq!(
            plan_runtime_start(true, Some("0.40.7"), "0.30.4", true),
            RuntimeStartPlan::SwitchManaged {
                running: "0.40.7".into(),
                wanted: "0.30.4".into()
            }
        );
    }

    #[test]
    fn start_plan_external_server_is_left_alone_and_real_version_reported() {
        // Not our child (e.g. Ollama desktop app): never touched, and the version that
        // is really running is reported, not the selected binary's.
        assert_eq!(
            plan_runtime_start(true, Some("0.30.4"), "0.40.7", false),
            RuntimeStartPlan::AlreadyRunning {
                version: "0.30.4".into()
            }
        );
    }

    #[test]
    fn start_plan_unknown_versions_never_trigger_a_switch() {
        // /api/version unreadable: keep the server, report the binary's version
        assert_eq!(
            plan_runtime_start(true, None, "0.40.7", true),
            RuntimeStartPlan::AlreadyRunning {
                version: "0.40.7".into()
            }
        );
        // binary version unknown: cannot prove a mismatch, so no kill
        assert_eq!(
            plan_runtime_start(true, Some("0.30.4"), "", true),
            RuntimeStartPlan::AlreadyRunning {
                version: "0.30.4".into()
            }
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn a_dead_managed_child_still_has_a_pid_but_is_not_alive() {
        // `managed_child_status(..).0.is_some()` (the previous "is it ours?" test) is true for
        // exactly one call after the child died. With an external server on the port and a
        // different version, that sent us down the switch path and into a 10 s wait for a port
        // that never frees up. The decision must use the alive flag.
        let mut child = Command::new("cmd")
            .args(["/C", "exit", "0"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("spawn short-lived child");
        let pid = child.id();
        let _ = child.wait();
        let slot = Mutex::new(Some(child));

        let (reported_pid, alive) = crate::managed_child_slot_status(&slot);
        assert_eq!(reported_pid, Some(pid));
        assert!(!alive);
        assert_eq!(
            plan_runtime_start(true, Some("0.30.4"), "0.40.7", alive),
            RuntimeStartPlan::AlreadyRunning {
                version: "0.30.4".into()
            }
        );
        assert_eq!(crate::managed_child_slot_status(&slot), (None, false));
    }

    /// One-shot HTTP/1.1 server on an ephemeral loopback port.
    fn serve_once(body: &'static str) -> (u16, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (port, handle)
    }

    #[test]
    fn running_version_is_read_from_the_server_that_answers() {
        let (port, server) = serve_once(r#"{"version":"v0.30.4"}"#);
        let version = query_running_ollama_version(&format!("http://127.0.0.1:{}", port));
        server.join().expect("server thread");
        assert_eq!(version.as_deref(), Some("0.30.4"));

        let (port, server) = serve_once("not json");
        let garbage = query_running_ollama_version(&format!("http://127.0.0.1:{}/", port));
        server.join().expect("server thread");
        assert_eq!(garbage, None);
    }

    #[test]
    fn port_in_use_follows_the_listener_not_any_fallback() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        assert!(endpoint_port_in_use(&endpoint));
        drop(listener);
        assert!(!endpoint_port_in_use(&endpoint));
        assert!(!endpoint_port_in_use("not a url"));
    }

    // ── 3. directory swap, cleanup, guards ──────────────────────────────────────────

    fn write_marker(dir: &Path, text: &str) {
        fs::create_dir_all(dir).expect("create dir");
        fs::write(dir.join("ollama.exe"), text).expect("write marker");
    }

    fn read_marker(dir: &Path) -> String {
        fs::read_to_string(dir.join("ollama.exe")).unwrap_or_default()
    }

    fn leftovers(root: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(root)
            .expect("read root")
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| n.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn swap_installs_into_an_empty_target() {
        let root = TempDir::new("swap-fresh");
        let staging = root.path().join(".staging-0.40.7-1");
        let target = root.path().join("0.40.7");
        let backup = root.path().join(".old-0.40.7-1");
        write_marker(&staging, "new");

        swap_runtime_dir(&staging, &target, &backup).expect("swap");

        assert_eq!(read_marker(&target), "new");
        assert!(!staging.exists());
        assert!(!backup.exists());
    }

    #[test]
    fn swap_replaces_the_previous_install_and_deletes_the_backup() {
        let root = TempDir::new("swap-replace");
        let staging = root.path().join(".staging-0.40.7-1");
        let target = root.path().join("0.40.7");
        let backup = root.path().join(".old-0.40.7-1");
        write_marker(&target, "old");
        write_marker(&staging, "new");

        swap_runtime_dir(&staging, &target, &backup).expect("swap");

        assert_eq!(read_marker(&target), "new");
        assert!(!staging.exists());
        assert!(!backup.exists());
        assert_eq!(leftovers(root.path()), Vec::<String>::new());
    }

    #[test]
    fn swap_failure_puts_the_previous_install_back() {
        let root = TempDir::new("swap-restore");
        let staging = root.path().join(".staging-0.40.7-1"); // never created: final move fails
        let target = root.path().join("0.40.7");
        let backup = root.path().join(".old-0.40.7-1");
        write_marker(&target, "old");

        let err = swap_runtime_dir(&staging, &target, &backup).expect_err("must fail");

        assert!(
            err.contains("previous runtime was restored"),
            "got: {}",
            err
        );
        assert_eq!(read_marker(&target), "old");
        assert!(!backup.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn swap_leaves_everything_untouched_while_the_old_runtime_is_in_use() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = TempDir::new("swap-locked");
        let staging = root.path().join(".staging-0.40.7-1");
        let target = root.path().join("0.40.7");
        let backup = root.path().join(".old-0.40.7-1");
        write_marker(&target, "old");
        write_marker(&staging, "new");

        // Like a running ollama.exe: an open handle that forbids rename/delete.
        let running = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(target.join("ollama.exe"))
            .expect("hold binary open");
        let err = swap_runtime_dir(&staging, &target, &backup).expect_err("must refuse");
        assert!(err.contains("is in use"), "got: {}", err);
        // (the locked binary itself cannot be read while we hold it; check the folders)
        assert!(target.join("ollama.exe").exists());
        assert!(staging.join("ollama.exe").exists());
        assert!(!backup.exists());

        drop(running);
        assert_eq!(read_marker(&target), "old");
        assert_eq!(read_marker(&staging), "new");
        swap_runtime_dir(&staging, &target, &backup).expect("retry after stopping succeeds");
        assert_eq!(read_marker(&target), "new");
    }

    #[test]
    fn cleanup_keeps_young_leftovers_and_removes_old_ones() {
        let root = TempDir::new("cleanup");
        write_marker(&root.path().join(".staging-0.40.7-1"), "x");
        write_marker(&root.path().join(".old-0.30.4-1"), "x");
        write_marker(&root.path().join("0.30.4"), "installed"); // else the .old folder is healed
        write_marker(&root.path().join("0.40.7"), "keep");
        write_marker(&root.path().join(".unrelated"), "keep");

        // production threshold (1 h): everything created just now stays
        cleanup_stale_runtime_dirs(root.path());
        assert_eq!(
            leftovers(root.path()),
            vec![".old-0.30.4-1", ".staging-0.40.7-1", ".unrelated"]
        );

        // age is read from the filesystem timestamp; make "older than 10 ms" true
        std::thread::sleep(Duration::from_millis(50));
        cleanup_stale_runtime_dirs_older_than(root.path(), Duration::from_millis(10));
        assert_eq!(leftovers(root.path()), vec![".unrelated"]);
        assert_eq!(read_marker(&root.path().join("0.40.7")), "keep");
    }

    #[test]
    fn remove_on_drop_cleans_dirs_and_files_unless_disarmed() {
        let root = TempDir::new("guard");
        let dir = root.path().join("dir");
        let file = root.path().join("file.part");
        let kept = root.path().join("kept");
        write_marker(&dir, "x");
        fs::write(&file, b"x").unwrap();
        write_marker(&kept, "x");

        drop(RemoveOnDrop::new(dir.clone()));
        drop(RemoveOnDrop::new(file.clone()));
        let mut disarmed = RemoveOnDrop::new(kept.clone());
        disarmed.disarm();
        drop(disarmed);

        assert!(!dir.exists());
        assert!(!file.exists());
        assert!(kept.exists());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn archive_lock_blocks_swapping_the_file_between_hash_and_unpack() {
        let root = TempDir::new("archive-lock");
        let archive = root.path().join("ollama-windows-amd64-v0.40.7.zip");
        fs::write(&archive, b"original").unwrap();

        let held = open_archive_locked(&archive).expect("lock");
        assert!(fs::OpenOptions::new().append(true).open(&archive).is_err());
        assert!(fs::write(&archive, b"swapped").is_err());
        assert!(fs::rename(&archive, root.path().join("moved.zip")).is_err());
        assert!(fs::remove_file(&archive).is_err());
        assert!(
            File::open(&archive).is_ok(),
            "plain readers are still allowed"
        );
        assert_eq!(fs::read(&archive).unwrap(), b"original");

        drop(held);
        fs::write(&archive, b"swapped").expect("writable again after release");
    }

    #[test]
    fn sha256_reader_matches_the_known_vector() {
        let mut input = std::io::Cursor::new(b"abc".to_vec());
        assert_eq!(
            sha256_reader(&mut input).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    // ── 3b. extraction limits and the full stage-and-swap pipeline ──────────────────

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, options).expect("start entry");
            writer.write_all(data).expect("write entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    fn open_zip(bytes: Vec<u8>) -> ZipArchive<std::io::Cursor<Vec<u8>>> {
        ZipArchive::new(std::io::Cursor::new(bytes)).expect("valid zip")
    }

    #[test]
    fn extract_writes_files_and_reports_progress() {
        let root = TempDir::new("extract-ok");
        let mut zip = open_zip(build_zip(&[
            ("ollama.exe", b"binary"),
            ("lib/ollama/runner.dll", b"dll"),
        ]));
        let mut seen: Vec<(usize, usize)> = Vec::new();

        extract_runtime_zip(&mut zip, root.path(), 1024, &mut |d, t| seen.push((d, t)))
            .expect("extract");

        assert_eq!(read_marker(root.path()), "binary");
        assert_eq!(
            fs::read(root.path().join("lib/ollama/runner.dll")).unwrap(),
            b"dll"
        );
        assert_eq!(seen, vec![(1, 2), (2, 2)]);
    }

    #[test]
    fn extract_rejects_zip_slip_entries() {
        let root = TempDir::new("extract-slip");
        let inner = root.path().join("staging");
        fs::create_dir_all(&inner).unwrap();
        let mut zip = open_zip(build_zip(&[("../escaped.txt", b"evil")]));

        let err = extract_runtime_zip(&mut zip, &inner, 1024, &mut |_, _| {})
            .expect_err("path traversal must be rejected");

        assert!(err.contains("Unsafe ZIP entry path"), "got: {}", err);
        assert!(!root.path().join("escaped.txt").exists());
    }

    #[test]
    fn extract_rejects_oversized_archives_before_writing_anything() {
        let root = TempDir::new("extract-big");
        let mut zip = open_zip(build_zip(&[("a.bin", &[0u8; 10]), ("b.bin", &[0u8; 10])]));

        let err = extract_runtime_zip(&mut zip, root.path(), 15, &mut |_, _| {})
            .expect_err("20 declared bytes > 15 byte limit");

        assert!(err.contains("unpacks to 20 bytes"), "got: {}", err);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn extract_rejects_archives_with_too_many_entries() {
        let root = TempDir::new("extract-many");
        let names: Vec<String> = (0..=MAX_RUNTIME_ARCHIVE_ENTRIES)
            .map(|i| format!("f{}", i))
            .collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let mut zip = open_zip(build_zip(&entries));

        let err = extract_runtime_zip(&mut zip, root.path(), u64::MAX, &mut |_, _| {})
            .expect_err("entry limit");

        assert!(err.contains("entries"), "got: {}", err);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn extract_stops_a_deflate_bomb_that_lies_about_its_size() {
        // 4 MiB of zeros deflate to a few KiB. Patch the declared uncompressed size down to
        // 10 bytes in both headers so the up-front check passes; the zip reader's own
        // declared-size enforcement is then the only thing left. If a zip upgrade ever drops
        // it, this test fails and a bytes-written cap has to come back.
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("ollama.exe", options).unwrap();
        writer.write_all(&vec![0u8; 4 * 1024 * 1024]).unwrap();
        let mut bytes = writer.finish().unwrap().into_inner();
        let central = bytes
            .windows(4)
            .position(|w| w == b"PK\x01\x02")
            .expect("central header");
        bytes[central + 24..central + 28].copy_from_slice(&10u32.to_le_bytes());
        let local = bytes
            .windows(4)
            .position(|w| w == b"PK\x03\x04")
            .expect("local header");
        bytes[local + 22..local + 26].copy_from_slice(&10u32.to_le_bytes());

        let root = TempDir::new("extract-bomb");
        let mut zip = open_zip(bytes);
        let err = extract_runtime_zip(&mut zip, root.path(), 1000, &mut |_, _| {})
            .expect_err("bomb must not unpack");
        println!("bomb stopped by: {}", err);

        let on_disk = fs::metadata(root.path().join("ollama.exe"))
            .map(|m| m.len())
            .unwrap_or(0);
        assert!(on_disk <= 1000, "{} bytes reached the disk", on_disk);
    }

    fn assert_no_staging_leftovers(root: &Path) {
        let left = leftovers(root);
        assert!(left.is_empty(), "leftovers: {:?}", left);
    }

    #[test]
    fn stage_and_swap_cleans_staging_when_extraction_fails() {
        let root = TempDir::new("stage-extract-fail");
        let mut zip = open_zip(build_zip(&[("ollama.exe", &[0u8; 64])]));
        write_marker(&root.path().join("0.40.7"), "previous");

        let err = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.7",
            16,
            &|_| Ok(()),
            &mut |_, _| {},
        )
        .expect_err("over the limit");

        assert!(err.contains("unpacks to 64 bytes"), "got: {}", err);
        assert_no_staging_leftovers(root.path());
        assert_eq!(read_marker(&root.path().join("0.40.7")), "previous");
    }

    #[test]
    fn stage_and_swap_cleans_staging_when_the_runtime_has_no_binary() {
        let root = TempDir::new("stage-no-binary");
        let mut zip = open_zip(build_zip(&[("readme.txt", b"hi")]));

        let err = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.7",
            1024,
            &|_| Ok(()),
            &mut |_, _| {},
        )
        .expect_err("no ollama.exe");

        assert!(err.contains("does not contain ollama.exe"));
        assert_no_staging_leftovers(root.path());
        assert!(!root.path().join("0.40.7").exists());
    }

    #[test]
    fn stage_and_swap_keeps_the_previous_install_when_verification_fails() {
        let root = TempDir::new("stage-verify-fail");
        let mut zip = open_zip(build_zip(&[("ollama.exe", b"new")]));
        write_marker(&root.path().join("0.40.7"), "previous");

        let err = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.7",
            1024,
            &|_| Err("rejected by verification".to_string()),
            &mut |_, _| {},
        )
        .expect_err("verification failure");

        assert_eq!(err, "rejected by verification");
        assert_no_staging_leftovers(root.path());
        assert_eq!(read_marker(&root.path().join("0.40.7")), "previous");
    }

    #[test]
    fn stage_and_swap_installs_and_replaces_when_verification_passes() {
        let root = TempDir::new("stage-ok");
        write_marker(&root.path().join("0.40.7"), "previous");
        let mut zip = open_zip(build_zip(&[("bin/ollama.exe", b"new")]));

        let binary = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.7",
            1024,
            &|staged| {
                assert!(staged.starts_with(root.path()));
                Ok(())
            },
            &mut |_, _| {},
        )
        .expect("install");

        assert_eq!(binary, root.path().join("0.40.7/bin/ollama.exe"));
        assert_eq!(fs::read_to_string(&binary).unwrap(), "new");
        assert_no_staging_leftovers(root.path());
    }

    #[test]
    fn stage_and_swap_refuses_unsafe_version_strings() {
        let root = TempDir::new("stage-unsafe");
        let mut zip = open_zip(build_zip(&[("ollama.exe", b"x")]));
        let err = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "..\\..\\evil",
            1024,
            &|_| Ok(()),
            &mut |_, _| {},
        )
        .expect_err("unsafe version");
        assert!(err.contains("Unsafe runtime version"));
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    /// Zip that contains the real signed 0.40.7 binary (optionally with one flipped byte).
    #[cfg(target_os = "windows")]
    fn zip_with_real_binary(tamper: bool) -> Option<Vec<u8>> {
        let binary = real_signed_binary()?;
        let mut bytes = fs::read(binary).expect("read real binary");
        if tamper {
            let mid = bytes.len() / 2;
            bytes[mid] ^= 0x01;
        }
        Some(build_zip(&[("ollama.exe", &bytes)]))
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn full_install_pipeline_accepts_the_genuine_runtime() {
        let Some(archive) = zip_with_real_binary(false) else {
            return;
        };
        let root = TempDir::new("pipeline-ok");
        let mut zip = open_zip(archive);

        let binary = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.7",
            MAX_RUNTIME_EXTRACTED_BYTES,
            &|staged| verify_staged_runtime(staged, "0.40.7"),
            &mut |_, _| {},
        )
        .expect("genuine runtime installs");

        assert_eq!(binary, root.path().join("0.40.7").join("ollama.exe"));
        assert_no_staging_leftovers(root.path());
        verify_staged_runtime(&binary, "0.40.7").expect("installed binary still verifies");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn full_install_pipeline_rejects_a_tampered_runtime_and_keeps_the_old_one() {
        let Some(archive) = zip_with_real_binary(true) else {
            return;
        };
        let root = TempDir::new("pipeline-tamper");
        write_marker(&root.path().join("0.40.7"), "previous-good-install");
        let mut zip = open_zip(archive);

        let err = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.7",
            MAX_RUNTIME_EXTRACTED_BYTES,
            &|staged| verify_staged_runtime(staged, "0.40.7"),
            &mut |_, _| {},
        )
        .expect_err("tampered runtime must not install");

        assert!(err.contains("HashMismatch"), "got: {}", err);
        assert_no_staging_leftovers(root.path());
        assert_eq!(
            read_marker(&root.path().join("0.40.7")),
            "previous-good-install"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn full_install_pipeline_rejects_a_mislabeled_version() {
        // Genuine, signed 0.40.7 shipped as "0.40.8" (downgrade / relabel attack).
        let Some(archive) = zip_with_real_binary(false) else {
            return;
        };
        let root = TempDir::new("pipeline-mislabel");
        let mut zip = open_zip(archive);

        let err = stage_and_swap_runtime(
            &mut zip,
            root.path(),
            "0.40.8",
            MAX_RUNTIME_EXTRACTED_BYTES,
            &|staged| verify_staged_runtime(staged, "0.40.8"),
            &mut |_, _| {},
        )
        .expect_err("version mismatch");

        assert!(err.contains("reports version 0.40.7"), "got: {}", err);
        assert_no_staging_leftovers(root.path());
        assert!(!root.path().join("0.40.8").exists());
    }

    // ── 5. review fixes ─────────────────────────────────────────────────────────────

    #[test]
    fn leftover_install_dirs_are_never_picked_as_the_managed_runtime() {
        let root = TempDir::new("find-managed");
        write_marker(&root.path().join("0.40.7"), "good");
        write_marker(&root.path().join("0.30.4"), "older");
        // `.old-0.40.7-1` parses as (40, 0, 0) and would outrank 0.40.7
        write_marker(&root.path().join(".old-0.40.7-1"), "leftover");
        write_marker(&root.path().join(".staging-9.9.9-1"), "leftover");

        let mut settings = base_settings();
        settings.providers.ollama.runtime_target_version = String::new();
        settings.providers.ollama.runtime_version = String::new();
        let found = find_managed_runtime_binary_in(root.path(), &settings).expect("binary");
        assert_eq!(found, root.path().join("0.40.7").join("ollama.exe"));

        // an explicit selection wins over "highest version"
        settings.providers.ollama.runtime_target_version = "0.30.4".to_string();
        let found = find_managed_runtime_binary_in(root.path(), &settings).expect("binary");
        assert_eq!(found, root.path().join("0.30.4").join("ollama.exe"));

        // a selection that is not a plain version string is ignored, never joined into a path
        settings.providers.ollama.runtime_target_version = "..\\0.30.4".to_string();
        settings.providers.ollama.runtime_version = String::new();
        let found = find_managed_runtime_binary_in(root.path(), &settings).expect("binary");
        assert_eq!(found, root.path().join("0.40.7").join("ollama.exe"));
    }

    // ── 6. target-aware runtime selection + consistent (path, version) pair ──────────

    fn managed_settings(target: &str, configured_path: &Path) -> crate::state::Settings {
        let mut settings = base_settings();
        settings.providers.ollama.runtime_source = "per_user_zip".to_string();
        settings.providers.ollama.runtime_target_version = target.to_string();
        settings.providers.ollama.runtime_path = configured_path.to_string_lossy().to_string();
        settings.providers.ollama.runtime_version = "0.40.7".to_string();
        settings
    }

    #[test]
    fn managed_runtime_prefers_the_selected_target_folder_over_the_remembered_path() {
        let root = TempDir::new("select-target");
        write_marker(&root.path().join("0.40.7"), "old");
        write_marker(&root.path().join("0.40.13"), "new");
        let remembered = root.path().join("0.40.7").join("ollama.exe");
        let settings = managed_settings("0.40.13", &remembered);

        let (path, source) =
            select_runtime_binary_in(&settings, Some(root.path())).expect("binary");

        assert_eq!(path, root.path().join("0.40.13").join("ollama.exe"));
        assert_eq!(source, "per_user_zip");
    }

    #[test]
    fn managed_runtime_keeps_the_remembered_path_when_the_target_is_not_installed() {
        let root = TempDir::new("select-fallback");
        write_marker(&root.path().join("0.40.7"), "old");
        let remembered = root.path().join("0.40.7").join("ollama.exe");

        // target folder absent, e.g. a legacy "0.20.2" that was never installed
        let settings = managed_settings("0.20.2", &remembered);
        let (path, _) = select_runtime_binary_in(&settings, Some(root.path())).expect("binary");
        assert_eq!(path, remembered);

        // an empty target folder (no ollama.exe inside) does not count as installed
        fs::create_dir_all(root.path().join("0.40.13")).expect("empty target dir");
        let settings = managed_settings("0.40.13", &remembered);
        let (path, _) = select_runtime_binary_in(&settings, Some(root.path())).expect("binary");
        assert_eq!(path, remembered);

        // a target that is not a plain version string is never joined into a path
        let settings = managed_settings("..\\0.40.7", &remembered);
        let (path, _) = select_runtime_binary_in(&settings, Some(root.path())).expect("binary");
        assert_eq!(path, remembered);
    }

    #[test]
    fn managed_selection_without_any_runtime_root_reports_the_missing_install() {
        let settings = managed_settings("0.40.13", Path::new("Z:\\does\\not\\exist\\ollama.exe"));
        let err = select_runtime_binary_in(&settings, None).expect_err("nothing installed");
        assert!(
            err.contains("no local Ollama runtime is installed"),
            "got: {}",
            err
        );
    }

    #[test]
    fn recorded_version_is_the_binarys_own_never_the_servers() {
        let path = Path::new("C:\\rt\\0.40.13\\ollama.exe");
        let mut settings = base_settings();
        settings.providers.ollama.runtime_path = "C:\\rt\\0.40.7\\ollama.exe".to_string();
        settings.providers.ollama.runtime_version = "0.30.4".to_string();

        // the binary reports its own version: that is what gets paired with its path
        assert_eq!(
            version_to_record_for_binary(&settings, path, " 0.40.13 "),
            "0.40.13"
        );
        // unknown binary version + stored version belongs to ANOTHER path: leave it empty
        assert_eq!(version_to_record_for_binary(&settings, path, ""), "");
        // unknown binary version + stored version belongs to this very path: keep it
        settings.providers.ollama.runtime_path = path.to_string_lossy().to_string();
        assert_eq!(version_to_record_for_binary(&settings, path, ""), "0.30.4");
    }

    #[test]
    fn online_manifest_lookup_refuses_unsafe_versions_before_any_request() {
        for bad in ["", "../x", "0.40.7/../../x", "0.40.7?x=1", "a b"] {
            let err = fetch_online_manifest(bad).expect_err("unsafe version");
            assert!(err.contains("Unsafe runtime version"), "{}: {}", bad, err);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn version_probe_uses_a_dead_host_and_does_not_cache_misses() {
        let root = TempDir::new("version-probe");
        let script = root.path().join("fake-ollama.cmd");

        // 1st probe: binary missing -> unknown, and the miss must not be remembered
        assert_eq!(parse_ollama_version(&script), "");

        // Prints the "running server" version unless the client was pointed at a dead host.
        fs::write(
            &script,
            "@echo off\r\nif \"%OLLAMA_HOST%\"==\"127.0.0.1:1\" (echo Warning: client version is 9.8.7) else (echo ollama version is 0.0.0)\r\n",
        )
        .expect("write fake binary");
        assert_eq!(parse_ollama_version(&script), "9.8.7");
    }

    // ── 7. detect: the current runtime, always as a consistent (path, version) pair ──

    fn detect_settings(
        target: &str,
        stored_path: &Path,
        stored_version: &str,
    ) -> crate::state::Settings {
        let mut settings = managed_settings(target, stored_path);
        settings.providers.ollama.runtime_version = stored_version.to_string();
        settings
    }

    fn detect(settings: &crate::state::Settings, root: &Path) -> Option<(String, PathBuf, String)> {
        detect_runtime_in(settings, Some(root))
    }

    #[test]
    fn detect_pairs_the_reported_binary_with_its_own_version_when_the_stored_path_is_gone() {
        let root = TempDir::new("detect-gone");
        write_marker(&root.path().join("0.40.7"), "target");
        // 0.30.4 was uninstalled after it was recorded; or the path was reset to "" (managed switch)
        let gone = root.path().join("0.30.4").join("ollama.exe");

        for stored_path in [gone.as_path(), Path::new("")] {
            let settings = detect_settings("0.40.7", stored_path, "0.30.4");
            let (source, path, version) = detect(&settings, root.path()).expect("runtime");

            assert_eq!(source, "per_user_zip");
            assert_eq!(path, root.path().join("0.40.7").join("ollama.exe"));
            assert_eq!(
                version,
                "0.40.7",
                "version of another binary was reported for {}",
                path.display()
            );
        }
    }

    #[test]
    fn detect_keeps_reporting_the_current_runtime_while_the_target_is_only_selected() {
        // The UI offers "Switch to <target>" by comparing this version with the selected target.
        // Reporting the target here would hide that action while the old server still answers.
        let root = TempDir::new("detect-current");
        write_marker(&root.path().join("0.30.4"), "current");
        write_marker(&root.path().join("0.40.7"), "target");
        let current = root.path().join("0.30.4").join("ollama.exe");

        let mut settings = detect_settings("0.40.7", &current, "0.30.4");
        let (source, path, version) = detect(&settings, root.path()).expect("runtime");
        assert_eq!(
            (source.as_str(), path, version.as_str()),
            ("per_user_zip", current.clone(), "0.30.4")
        );
        // ... and the target on disk stays discoverable for the `target_installed` flag
        assert_eq!(
            target_runtime_binary_in(root.path(), &settings),
            Some(root.path().join("0.40.7").join("ollama.exe"))
        );

        // a lost stored version is recovered from the binary's own managed folder
        settings.providers.ollama.runtime_version = String::new();
        let (_, path, version) = detect(&settings, root.path()).expect("runtime");
        assert_eq!((path, version.as_str()), (current, "0.30.4"));
    }

    #[test]
    fn detect_without_an_installed_target_behaves_as_before() {
        let root = TempDir::new("detect-no-target");
        write_marker(&root.path().join("0.40.7"), "current");
        let current = root.path().join("0.40.7").join("ollama.exe");

        // target not on disk: the stored pair is reported unchanged
        let settings = detect_settings("0.40.13", &current, "0.40.7");
        assert_eq!(target_runtime_binary_in(root.path(), &settings), None);
        let (source, path, version) = detect(&settings, root.path()).expect("runtime");
        assert_eq!(
            (source.as_str(), path, version.as_str()),
            ("per_user_zip", current.clone(), "0.40.7")
        );

        // stored path gone: the installed runtime is found, with its own version
        let gone = root.path().join("0.30.4").join("ollama.exe");
        let settings = detect_settings("0.40.13", &gone, "0.30.4");
        let (_, path, version) = detect(&settings, root.path()).expect("runtime");
        assert_eq!((path, version.as_str()), (current, "0.40.7"));

        // nothing anywhere
        let settings = detect_settings("0.40.13", &gone, "0.30.4");
        assert_eq!(detect_runtime_in(&settings, None), None);
    }

    #[test]
    fn managed_binary_version_is_the_folder_under_the_runtime_root() {
        let root = Path::new("rt-root");
        let version_of = |binary: PathBuf| managed_runtime_version_of(root, &binary);

        assert_eq!(
            version_of(root.join("0.40.7").join("ollama.exe")),
            Some("0.40.7".to_string())
        );
        assert_eq!(
            version_of(root.join("0.40.7").join("bin").join("ollama.exe")),
            Some("0.40.7".to_string())
        );
        // install leftovers, files directly in the root, foreign folders, non-version names
        assert_eq!(
            version_of(root.join(".old-0.40.7-1").join("ollama.exe")),
            None
        );
        assert_eq!(version_of(root.join("ollama.exe")), None);
        assert_eq!(version_of(root.join("0.40.7")), None);
        assert_eq!(
            version_of(Path::new("elsewhere").join("0.40.7").join("ollama.exe")),
            None
        );
        assert_eq!(version_of(root.join("a b").join("ollama.exe")), None);
    }
}
