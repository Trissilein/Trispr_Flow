//! Module delivery layer: discover, download, verify, install, update, and
//! uninstall on-demand modules published as GitHub release assets in the
//! `Trissilein/Trispr_Flow` repository.
//!
//! This is the runtime half of the "lean core + on-demand modules" model. The
//! install-by-unpack engine (`super::package`) already handles staged, atomic,
//! validated installation from a local directory; this module adds the network
//! side: fetch a stable `modules-index.json`, download + SHA256-verify the asset
//! zip, unpack it, and hand the unpacked directory to `install_package_from_dir`.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use super::package::{self, ModulePackageInstallResult};
use crate::paths::resolve_modules_dir;

/// Stable URL of the module index. Published once under its own release tag so
/// the URL never changes as app releases come and go.
const MODULES_INDEX_URL: &str =
    "https://github.com/Trissilein/Trispr_Flow/releases/download/modules-index/modules-index.json";
const USER_AGENT: &str = "TrisprFlow/ModuleDelivery";
const DOWNLOAD_EVENT: &str = "module:download-progress";

/// One entry in `modules-index.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulesIndexEntry {
    pub id: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    pub name: String,
    pub version: String,
    /// Direct download URL of the package zip (a GitHub release asset).
    pub asset_url: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub min_app_version: String,
}

fn default_kind() -> String {
    "assets".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct ModulesIndex {
    schema_version: u16,
    modules: Vec<ModulesIndexEntry>,
}

impl Default for ModulesIndex {
    fn default() -> Self {
        Self {
            schema_version: 1,
            modules: Vec::new(),
        }
    }
}

/// A module the user could add, enriched with local install state.
#[derive(Debug, Clone, Serialize)]
pub struct AvailableModule {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub update_available: bool,
}

#[derive(Clone, Serialize)]
struct ModuleDownloadProgress {
    module_id: String,
    stage: String,
    message: String,
    downloaded: Option<u64>,
    total: Option<u64>,
    percent: Option<u8>,
}

fn emit_progress(
    app: &AppHandle,
    module_id: &str,
    stage: &str,
    message: impl Into<String>,
    downloaded: Option<u64>,
    total: Option<u64>,
) {
    let percent = match (downloaded, total) {
        (Some(d), Some(t)) if t > 0 => Some(((d as f64 / t as f64) * 100.0).round() as u8),
        _ => None,
    };
    let _ = app.emit(
        DOWNLOAD_EVENT,
        ModuleDownloadProgress {
            module_id: module_id.to_string(),
            stage: stage.to_string(),
            message: message.into(),
            downloaded,
            total,
            percent,
        },
    );
}

/// Parse a "major.minor.patch" string into a comparable tuple. Missing or
/// non-numeric components are treated as 0 so comparison never panics.
fn parse_version(version: &str) -> (u64, u64, u64) {
    let mut parts = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.trim().parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// True when `candidate` is strictly newer than `current`.
fn is_newer(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

fn http_agent() -> ureq::Agent {
    ureq::builder()
        .timeout_connect(std::time::Duration::from_secs(15))
        .timeout_read(std::time::Duration::from_secs(1800))
        .build()
}

/// Fetch and parse the module index from the stable release URL.
fn fetch_index() -> Result<ModulesIndex, String> {
    let response = http_agent()
        .get(MODULES_INDEX_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .call()
        .map_err(|error| format!("Failed to fetch module index: {error}"))?;
    response
        .into_json::<ModulesIndex>()
        .map_err(|error| format!("Failed to parse module index: {error}"))
}

/// Read the installed version of a module from its on-disk manifest, if present.
fn installed_version(modules_dir: &Path, module_id: &str) -> Option<String> {
    let package_dir = modules_dir.join(module_id);
    package::scan_package_dir(&package_dir)
        .ok()
        .map(|pkg| pkg.manifest.version)
}

/// List every module in the index, annotated with local install state.
pub fn list_available(app: &AppHandle) -> Result<Vec<AvailableModule>, String> {
    let index = fetch_index()?;
    let modules_dir = resolve_modules_dir(app);

    let available = index
        .modules
        .into_iter()
        .map(|entry| {
            let local_version = installed_version(&modules_dir, &entry.id);
            let installed = local_version.is_some();
            let update_available = local_version
                .as_deref()
                .map(|current| is_newer(&entry.version, current))
                .unwrap_or(false);
            AvailableModule {
                id: entry.id,
                kind: entry.kind,
                name: entry.name,
                version: entry.version,
                size: entry.size,
                installed,
                installed_version: local_version,
                update_available,
            }
        })
        .collect();
    Ok(available)
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Failed to open '{}': {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Download, verify, unpack and install a module by id. If the module is already
/// installed it is removed first (so this also performs updates).
pub fn download_and_install(
    app: &AppHandle,
    module_id: &str,
) -> Result<ModulePackageInstallResult, String> {
    let index = fetch_index()?;
    let entry = index
        .modules
        .into_iter()
        .find(|m| m.id == module_id)
        .ok_or_else(|| format!("Module '{module_id}' is not in the index."))?;

    let modules_dir = resolve_modules_dir(app);
    fs::create_dir_all(&modules_dir)
        .map_err(|error| format!("Failed to create modules dir: {error}"))?;

    let download_path = modules_dir.join(format!(".{module_id}.download.zip"));
    let unpack_dir = modules_dir.join(format!(".{module_id}.unpack"));
    // Best-effort cleanup of stale temp artifacts from an interrupted run.
    let _ = fs::remove_file(&download_path);
    let _ = fs::remove_dir_all(&unpack_dir);

    // --- download ---
    emit_progress(
        app,
        module_id,
        "downloading",
        "Starting download…",
        Some(0),
        entry.size_opt(),
    );
    let response = http_agent()
        .get(&entry.asset_url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("Failed to download module '{module_id}': {error}"))?;
    let total = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .or(entry.size_opt());

    let mut reader = response.into_reader();
    let mut out = fs::File::create(&download_path)
        .map_err(|error| format!("Failed to create download file: {error}"))?;
    let mut buffer = [0u8; 262144];
    let mut downloaded: u64 = 0;
    let mut last_emit = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Download read error: {error}"))?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut out, &buffer[..read])
            .map_err(|error| format!("Download write error: {error}"))?;
        downloaded += read as u64;
        if downloaded - last_emit >= 1_048_576 {
            last_emit = downloaded;
            emit_progress(
                app,
                module_id,
                "downloading",
                "Downloading…",
                Some(downloaded),
                total,
            );
        }
    }
    drop(out);

    // --- verify ---
    if !entry.sha256.trim().is_empty() {
        emit_progress(app, module_id, "verifying", "Verifying…", None, None);
        let actual = sha256_of(&download_path)?;
        if !actual.eq_ignore_ascii_case(entry.sha256.trim()) {
            let _ = fs::remove_file(&download_path);
            return Err(format!(
                "Checksum mismatch for module '{module_id}': expected {}, got {actual}.",
                entry.sha256.trim()
            ));
        }
    }

    // --- unpack ---
    emit_progress(app, module_id, "installing", "Unpacking…", None, None);
    unzip_into(&download_path, &unpack_dir)?;
    let _ = fs::remove_file(&download_path);

    // A zip may contain the package at its root or nested in a single folder.
    let source_dir = resolve_package_root(&unpack_dir)?;

    // Remove any existing install so this doubles as an update path.
    let target_dir = modules_dir.join(module_id);
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)
            .map_err(|error| format!("Failed to replace existing module: {error}"))?;
    }

    let result = package::install_package_from_dir(&source_dir, &modules_dir);
    let _ = fs::remove_dir_all(&unpack_dir);
    let result = result?;

    emit_progress(app, module_id, "complete", "Installed.", None, None);
    Ok(result)
}

/// Remove an installed module from disk. Idempotent.
pub fn uninstall(app: &AppHandle, module_id: &str) -> Result<(), String> {
    let modules_dir = resolve_modules_dir(app);
    let target_dir = modules_dir.join(module_id);
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir)
            .map_err(|error| format!("Failed to uninstall module '{module_id}': {error}"))?;
    }
    Ok(())
}

fn unzip_into(zip_path: &Path, dest_dir: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path)
        .map_err(|error| format!("Failed to open downloaded zip: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Failed to read zip archive: {error}"))?;
    fs::create_dir_all(dest_dir)
        .map_err(|error| format!("Failed to create unpack dir: {error}"))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|error| format!("Failed to read zip entry: {error}"))?;
        // `enclosed_name` rejects path traversal (../, absolute paths).
        let relative = match entry.enclosed_name() {
            Some(path) => path.to_path_buf(),
            None => return Err("Zip contains an unsafe path entry.".to_string()),
        };
        let out_path = dest_dir.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)
                .map_err(|error| format!("Failed to create dir during unpack: {error}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("Failed to create dir during unpack: {error}"))?;
            }
            let mut out = fs::File::create(&out_path)
                .map_err(|error| format!("Failed to write file during unpack: {error}"))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|error| format!("Failed to extract file during unpack: {error}"))?;
        }
    }
    Ok(())
}

/// Find the directory that actually contains the module manifest: either
/// `dest_dir` itself, or a single top-level subdirectory inside it.
fn resolve_package_root(dest_dir: &Path) -> Result<PathBuf, String> {
    if dest_dir.join(package::MODULE_MANIFEST_FILE).is_file() {
        return Ok(dest_dir.to_path_buf());
    }
    let mut subdirs: Vec<PathBuf> = fs::read_dir(dest_dir)
        .map_err(|error| format!("Failed to read unpack dir: {error}"))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_dir())
        .collect();
    if subdirs.len() == 1 {
        let candidate = subdirs.remove(0);
        if candidate.join(package::MODULE_MANIFEST_FILE).is_file() {
            return Ok(candidate);
        }
    }
    Err(format!(
        "Downloaded package is missing '{}' at its root.",
        package::MODULE_MANIFEST_FILE
    ))
}

impl ModulesIndexEntry {
    fn size_opt(&self) -> Option<u64> {
        if self.size > 0 {
            Some(self.size)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_comparison_handles_normal_and_messy_inputs() {
        assert!(is_newer("1.2.0", "1.1.9"));
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        // tolerate a leading v and missing components
        assert!(is_newer("v2", "1.9.9"));
        assert!(!is_newer("garbage", "0.0.1"));
    }
}
