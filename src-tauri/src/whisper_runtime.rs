//! In-app download of the pinned whisper.cpp runtime (CUDA or Vulkan) into
//! `%LOCALAPPDATA%\Trispr Flow\runtime\{cuda,vulkan}` (see `paths::whisper_runtime_root`).
//!
//! The pin (URL, SHA-256 and size of the zip, name/length/SHA-256 of every file) is compiled in
//! from `runtime-manifests/whisper-runtime-1.9.5.json`; there is no manifest from the network.
//! Flow: URL allowlist -> download with progress -> zip checksum -> unpack into a staging
//! folder (only the files the pin lists, each checked against its length and hash) -> atomic
//! swap into `<root>/<variant>`. Any failure removes the download and the staging folder; an
//! earlier install of the variant is never touched before everything has been verified.
//! Download, streaming and cleanup helpers come from `ollama_runtime`.

use crate::ollama_runtime::{
    describe_download_error, describe_stream_error, executable_in_use, is_disk_full,
    open_archive_locked, sha256_reader, stream_download, unique_install_stamp, RemoveOnDrop,
    RuntimeJob, StreamError, DOWNLOAD_STALL_TIMEOUT,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};
use url::Url;
use zip::ZipArchive;

const MANIFEST_JSON: &str = include_str!("../runtime-manifests/whisper-runtime-1.9.5.json");
const DOWNLOAD_URL_PREFIX: &str = "https://github.com/Trissilein/Trispr_Flow/releases/download/";
const VARIANTS: [&str; 2] = ["cuda", "vulkan"];
const PROGRESS_EVENT: &str = "whisper-runtime:progress";
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(15);
/// The CUDA 13 runtime of the pinned build needs at least this NVIDIA driver (Windows branch).
const CUDA_MIN_DRIVER_HINT: &str = "580";

#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    whisper_cpp_version: String,
    variants: HashMap<String, Variant>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Variant {
    url: String,
    zip_sha256: String,
    zip_size: u64,
    files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct FileEntry {
    name: String,
    length: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeProgress {
    variant: String,
    /// `download` | `verify` | `extract` | `install` | `done`
    stage: String,
    message: String,
    downloaded: Option<u64>,
    total: Option<u64>,
}

type Progress<'a> = dyn FnMut(&str, &str, Option<u64>, Option<u64>) + 'a;

fn is_hex_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A file name that cannot leave the runtime folder: plain, flat, no separators or `..`.
fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

fn load_variant(variant: &str) -> Result<(String, Variant), String> {
    if !VARIANTS.contains(&variant) {
        return Err(format!(
            "Unknown speech engine variant '{}' (expected cuda or vulkan).",
            variant
        ));
    }
    let manifest: Manifest = serde_json::from_str(MANIFEST_JSON)
        .map_err(|e| format!("Built-in runtime manifest is invalid: {}", e))?;
    let entry = manifest
        .variants
        .get(variant)
        .ok_or_else(|| format!("Built-in runtime manifest has no '{}' variant.", variant))?
        .clone();
    validate_variant(&entry)?;
    Ok((manifest.whisper_cpp_version, entry))
}

fn validate_variant(variant: &Variant) -> Result<(), String> {
    check_pinned_url(&variant.url)?;
    if !is_hex_sha256(&variant.zip_sha256) || variant.zip_size == 0 || variant.files.is_empty() {
        return Err("Built-in runtime manifest has an invalid archive pin.".to_string());
    }
    let mut names = HashSet::new();
    for file in &variant.files {
        if !is_safe_file_name(&file.name)
            || !is_hex_sha256(&file.sha256)
            || !names.insert(file.name.as_str())
        {
            return Err(format!(
                "Built-in runtime manifest has an invalid file entry '{}'.",
                file.name
            ));
        }
    }
    Ok(())
}

/// Only the official release path of this project may serve the runtime. A URL that `Url`
/// would rewrite (`..`, default port, upper-case host) or that carries credentials, a query or
/// a fragment is refused as well.
fn check_pinned_url(url: &str) -> Result<(), String> {
    let refuse = || {
        Err(format!(
            "Refusing speech engine download from '{}': only {} is allowed.",
            url, DOWNLOAD_URL_PREFIX
        ))
    };
    let Ok(parsed) = Url::parse(url) else {
        return refuse();
    };
    if parsed.as_str() == url
        && url.starts_with(DOWNLOAD_URL_PREFIX)
        && parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
    {
        Ok(())
    } else {
        refuse()
    }
}

/// Tests serve the archive from a local socket; production only ever accepts the pinned URL.
fn validate_download_url(url: &str) -> Result<(), String> {
    if cfg!(test) && url.starts_with("http://127.0.0.1:") {
        return Ok(());
    }
    check_pinned_url(url)
}

fn describe_write_error(what: &str, error: &std::io::Error) -> String {
    if is_disk_full(error) {
        format!(
            "Not enough free disk space to {} (the speech engine needs up to about 1.5 GB). Free up space and try again.",
            what
        )
    } else {
        format!("Failed to {}: {}", what, error)
    }
}

fn remove_path(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// Removes what an interrupted run of `variant` left behind (`.download-*`, `.staging-*`) and
/// puts the newest `.old-*` back when the crash hit between the two renames of the swap.
fn clean_leftovers(root: &Path, variant: &str) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let prefix = |kind: &str| format!(".{}-{}-", kind, variant);
    let (download, staging, old) = (prefix("download"), prefix("staging"), prefix("old"));
    let target = root.join(variant);
    let mut backups: Vec<(u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with(&download) || name.starts_with(&staging) {
            remove_path(&path);
        } else if let Some(stamp) = name.strip_prefix(&old) {
            backups.push((stamp.parse().unwrap_or(0), path));
        }
    }
    backups.sort();
    if !target.exists() {
        if let Some((_, newest)) = backups.pop() {
            if fs::rename(&newest, &target).is_ok() {
                warn!(
                    "[whisper.runtime] restored {} from an interrupted update",
                    variant
                );
            }
        }
    }
    for (_, path) in backups {
        remove_path(&path);
    }
}

/// Downloads `url` into `dest`. The pinned `expected_size` is the size limit and must match
/// exactly; whatever happens, the caller's guard removes `dest`.
fn download_zip(
    url: &str,
    dest: &Path,
    expected_size: u64,
    stall_timeout: Duration,
    on_progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(stall_timeout)
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", "TrisprFlow/RuntimeInstaller")
        .call()
        .map_err(|e| {
            format!(
                "Failed to download the speech engine: {}",
                describe_download_error(&e)
            )
        })?;
    if let Some(total) = response
        .header("Content-Length")
        .and_then(|h| h.parse::<u64>().ok())
    {
        if total != expected_size {
            return Err(format!(
                "The download is {} bytes but {} were expected; download aborted.",
                total, expected_size
            ));
        }
    }

    let mut reader = response.into_reader();
    let mut out =
        File::create(dest).map_err(|e| describe_write_error("create the download file", &e))?;
    let mut last_emit = Instant::now();
    let mut last_seen = 0u64;
    let streamed = stream_download(&mut reader, &mut out, expected_size, &mut |downloaded| {
        last_seen = downloaded;
        if last_emit.elapsed() >= Duration::from_millis(250) {
            on_progress(downloaded, expected_size);
            last_emit = Instant::now();
        }
    });
    let copied = streamed.map_err(|error| match error {
        StreamError::TooLarge => {
            "The speech engine download is larger than expected and was aborted.".to_string()
        }
        StreamError::Read(e) => describe_stream_error(&e, last_seen, Some(expected_size)),
        StreamError::Write(e) => describe_write_error("write the download to disk", &e),
    })?;
    if copied != expected_size {
        return Err(format!(
            "The download ended early: {} of {} bytes arrived. Try again.",
            copied, expected_size
        ));
    }
    on_progress(copied, expected_size);
    Ok(())
}

/// Unpacks exactly the files `files` lists from `zip` into `staging` (flat). Every entry must
/// be on the list and match its pinned length and SHA-256; a missing, extra, duplicate, nested
/// or traversing entry fails the whole install. Output names come from the pin, never from the
/// archive.
fn extract_pinned<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    files: &[FileEntry],
    staging: &Path,
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    if zip.len() != files.len() {
        return Err(format!(
            "The speech engine archive holds {} entries but {} were expected.",
            zip.len(),
            files.len()
        ));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for idx in 0..zip.len() {
        let entry = zip
            .by_index(idx)
            .map_err(|e| format!("Failed to read archive entry {}: {}", idx, e))?;
        let name = entry
            .name()
            .map(|n| n.into_owned())
            .map_err(|e| format!("Unreadable archive entry name: {}", e))?;
        if entry.enclosed_name().is_none() || entry.is_dir() || entry.is_symlink() {
            return Err(format!("Unsafe archive entry rejected: '{}'.", name));
        }
        let expected = files
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| format!("Unexpected file in the archive: '{}'.", name))?;
        if !seen.insert(expected.name.as_str()) {
            return Err(format!("Duplicate file in the archive: '{}'.", name));
        }
        if entry.size() != expected.length {
            return Err(format!(
                "'{}' has {} bytes in the archive but {} were expected.",
                name,
                entry.size(),
                expected.length
            ));
        }

        let out_path = staging.join(&expected.name);
        let mut out = File::create(&out_path)
            .map_err(|e| describe_write_error(&format!("write '{}'", name), &e))?;
        let mut hasher = Sha256::new();
        let mut written = 0u64;
        let mut buf = vec![0u8; 256 * 1024];
        // One byte more than pinned: a longer entry is caught, not silently cut off.
        let mut limited = entry.take(expected.length + 1);
        loop {
            let read = limited
                .read(&mut buf)
                .map_err(|e| format!("Failed to unpack '{}': {}", name, e))?;
            if read == 0 {
                break;
            }
            out.write_all(&buf[..read])
                .map_err(|e| describe_write_error(&format!("write '{}'", name), &e))?;
            hasher.update(&buf[..read]);
            written += read as u64;
        }
        drop(out);
        if written != expected.length {
            return Err(format!(
                "'{}' unpacked to {} bytes but {} were expected.",
                name, written, expected.length
            ));
        }
        let digest = format!("{:x}", hasher.finalize());
        if !digest.eq_ignore_ascii_case(&expected.sha256) {
            return Err(format!(
                "'{}' failed its checksum (expected {}, got {}).",
                name, expected.sha256, digest
            ));
        }
        on_progress(idx + 1, files.len());
    }
    Ok(())
}

/// Puts the verified `staging` folder at `target`. The previous folder moves to `backup`
/// first and comes back if the final move fails.
fn swap_into_place(staging: &Path, target: &Path, backup: &Path) -> Result<(), String> {
    let had_previous = target.exists();
    if had_previous {
        // A rename would succeed even while whisper-server runs from the folder.
        for exe in ["whisper-server.exe", "whisper-cli.exe"] {
            let path = target.join(exe);
            if path.exists() && executable_in_use(&path) {
                return Err(
                    "The speech engine is in use and cannot be replaced. Close other dictation activity, restart Trispr Flow and try again."
                        .to_string(),
                );
            }
        }
        fs::rename(target, backup)
            .map_err(|e| format!("Failed to move the old speech engine aside: {}", e))?;
    }
    if let Err(e) = fs::rename(staging, target) {
        let note = if !had_previous {
            ""
        } else if fs::rename(backup, target).is_ok() {
            " The previous speech engine was restored."
        } else {
            " The previous speech engine is kept in a backup folder next to it."
        };
        return Err(format!(
            "Failed to move the speech engine into place: {}.{}",
            e, note
        ));
    }
    if had_previous {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

/// Downloads, verifies and installs `variant_name` into `<root>/<variant_name>`.
/// On every failure nothing but the previous install (if any) remains on disk.
pub(crate) fn install_variant(
    variant_name: &str,
    variant: &Variant,
    root: &Path,
    stall_timeout: Duration,
    on_progress: &mut Progress<'_>,
) -> Result<PathBuf, String> {
    validate_download_url(&variant.url)?;
    let _job = RuntimeJob::begin(
        format!("whisper-runtime:{}:{}", variant_name, root.display()),
        format!(
            "A download of the {} speech engine is already running.",
            variant_name
        ),
    )?;
    fs::create_dir_all(root)
        .map_err(|e| describe_write_error("create the speech engine folder", &e))?;
    clean_leftovers(root, variant_name);

    let stamp = unique_install_stamp();
    let zip_path = root.join(format!(".download-{}-{}.zip", variant_name, stamp));
    let staging = root.join(format!(".staging-{}-{}", variant_name, stamp));
    let backup = root.join(format!(".old-{}-{}", variant_name, stamp));
    let target = root.join(variant_name);
    // Both go away on every exit; the staging folder only stays once it is the install.
    let _zip_guard = RemoveOnDrop::new(zip_path.clone());
    let mut staging_guard = RemoveOnDrop::new(staging.clone());

    on_progress(
        "download",
        "Downloading speech engine...",
        Some(0),
        Some(variant.zip_size),
    );
    download_zip(
        &variant.url,
        &zip_path,
        variant.zip_size,
        stall_timeout,
        &mut |done, total| {
            on_progress(
                "download",
                "Downloading speech engine...",
                Some(done),
                Some(total),
            )
        },
    )?;

    on_progress("verify", "Verifying download...", None, None);
    // One locked handle for hash and unpack: the file cannot be swapped in between.
    let mut file = open_archive_locked(&zip_path)
        .map_err(|e| format!("Failed to open the downloaded archive: {}", e))?;
    let digest = sha256_reader(&mut file)?;
    if !digest.eq_ignore_ascii_case(&variant.zip_sha256) {
        return Err(format!(
            "Speech engine archive checksum mismatch. Expected {}, got {}.",
            variant.zip_sha256, digest
        ));
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("Failed to rewind the downloaded archive: {}", e))?;
    let mut zip = ZipArchive::new(file)
        .map_err(|e| format!("The downloaded archive is not a valid zip file: {}", e))?;

    fs::create_dir_all(&staging)
        .map_err(|e| describe_write_error("create the staging folder", &e))?;
    on_progress("extract", "Unpacking speech engine...", None, None);
    extract_pinned(&mut zip, &variant.files, &staging, &mut |done, total| {
        on_progress(
            "extract",
            &format!("Unpacking speech engine ({}/{})...", done, total),
            None,
            None,
        )
    })?;

    on_progress("install", "Installing speech engine...", None, None);
    swap_into_place(&staging, &target, &backup)?;
    staging_guard.disarm();
    on_progress("done", "Speech engine installed.", None, None);
    Ok(target)
}

// ---------------------------------------------------------------------------------------
// Status, GPU recommendation, self-test
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WhisperRuntimeVariantStatus {
    variant: String,
    /// Shipped with the installer (`bin/<variant>`), complete.
    bundled: bool,
    /// Downloaded by the setup wizard, every pinned file present with its pinned length.
    downloaded: bool,
    /// The app can run this variant (bundled or downloaded).
    ready: bool,
    zip_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WhisperRuntimeStatus {
    variants: Vec<WhisperRuntimeVariantStatus>,
    any_ready: bool,
    whisper_cpp_version: String,
    runtime_dir: String,
}

fn downloaded_complete(dir: &Path, files: &[FileEntry]) -> bool {
    files.iter().all(|f| {
        fs::metadata(dir.join(&f.name))
            .map(|m| m.is_file() && m.len() == f.length)
            .unwrap_or(false)
    })
}

/// Complete bundled runtime of `variant` that the app would run, if any.
fn bundled_cli_path(variant: &str, root: &Path) -> Option<PathBuf> {
    let cli = crate::paths::resolve_whisper_cli_path_for_backend(Some(variant))?;
    let is_variant = crate::transcription::whisper_backend_from_cli_path(&cli) == variant;
    (is_variant
        && !cli.starts_with(root)
        && crate::transcription::whisper_runtime_preflight_issue(&cli).is_none())
    .then_some(cli)
}

fn status_in(root: &Path) -> Result<WhisperRuntimeStatus, String> {
    let mut variants = Vec::new();
    let mut version = String::new();
    for name in VARIANTS {
        let (v, entry) = load_variant(name)?;
        version = v;
        let downloaded = downloaded_complete(&root.join(name), &entry.files);
        let bundled = bundled_cli_path(name, root).is_some();
        variants.push(WhisperRuntimeVariantStatus {
            variant: name.to_string(),
            bundled,
            downloaded,
            ready: bundled || downloaded,
            zip_size: entry.zip_size,
        });
    }
    Ok(WhisperRuntimeStatus {
        any_ready: variants.iter().any(|v| v.ready),
        variants,
        whisper_cpp_version: version,
        runtime_dir: root.display().to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeRecommendation {
    /// `cuda` | `vulkan`
    recommended: String,
    gpu_name: Option<String>,
    nvidia: bool,
    reason: String,
}

const NVIDIA_VENDOR_ID: u32 = 0x10DE;

/// The pinned CUDA 13 build ships kernels for Turing and newer (sm_75, sm_89, sm_120a); older
/// NVIDIA GPUs run Vulkan.
fn nvidia_gpu_lacks_cuda_build(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    [
        "gtx 7",
        "gtx 9",
        "gtx 10",
        "gt 10",
        "titan x",
        "titan v",
        "quadro k",
        "quadro m",
        "quadro p",
        "quadro gp",
        "quadro gv",
        "tesla",
    ]
    .iter()
    .any(|old| lowered.contains(old))
}

fn recommend_from_adapters(adapters: &[(u32, String)]) -> RuntimeRecommendation {
    let nvidia: Vec<&(u32, String)> = adapters
        .iter()
        .filter(|(vendor, _)| *vendor == NVIDIA_VENDOR_ID)
        .collect();
    if let Some((_, name)) = nvidia
        .iter()
        .find(|(_, name)| !nvidia_gpu_lacks_cuda_build(name))
    {
        return RuntimeRecommendation {
            recommended: "cuda".to_string(),
            gpu_name: Some(name.clone()),
            nvidia: true,
            reason: format!("{} detected: CUDA is the fastest engine for it.", name),
        };
    }
    if let Some((_, name)) = nvidia.first() {
        return RuntimeRecommendation {
            recommended: "vulkan".to_string(),
            gpu_name: Some(name.clone()),
            nvidia: true,
            reason: format!(
                "{} is too old for the CUDA engine (Turing / GTX 16 series or newer). Vulkan works on it.",
                name
            ),
        };
    }
    let first = adapters.first().map(|(_, name)| name.clone());
    RuntimeRecommendation {
        recommended: "vulkan".to_string(),
        reason: match &first {
            Some(name) => format!(
                "No NVIDIA GPU detected ({}). Vulkan runs on any modern GPU.",
                name
            ),
            None => "No GPU detected. Vulkan is the safe choice.".to_string(),
        },
        gpu_name: first,
        nvidia: false,
    }
}

#[cfg(target_os = "windows")]
fn dxgi_adapters() -> Vec<(u32, String)> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };
    let mut adapters = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else {
            return adapters;
        };
        let mut index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(index) {
            index += 1;
            let Ok(desc) = adapter.GetDesc1() else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let len = desc
                .Description
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(desc.Description.len());
            adapters.push((
                desc.VendorId,
                String::from_utf16_lossy(&desc.Description[..len]),
            ));
        }
    }
    adapters
}

#[cfg(not(target_os = "windows"))]
fn dxgi_adapters() -> Vec<(u32, String)> {
    Vec::new()
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WhisperRuntimeVerifyResult {
    variant: String,
    ok: bool,
    detail: String,
    /// Set when CUDA failed its self-test: the wizard loads (or switches to) this variant.
    fallback_variant: Option<String>,
}

fn cuda_error_message(code: i32) -> String {
    match code {
        35 => format!(
            "The NVIDIA driver is too old for the CUDA engine (driver {} or newer is needed).",
            CUDA_MIN_DRIVER_HINT
        ),
        100 => "No CUDA-capable NVIDIA GPU was found.".to_string(),
        other => format!("CUDA initialisation failed (error {}).", other),
    }
}

/// Asks the runtime's own `cudart` for the device count: the one check that fails without a
/// model when the driver is too old or no usable GPU exists.
#[cfg(target_os = "windows")]
fn cuda_device_count(dir: &Path) -> Result<i32, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{s, PCWSTR};
    use windows::Win32::Foundation::FreeLibrary;
    use windows::Win32::System::LibraryLoader::{
        GetProcAddress, LoadLibraryExW, LOAD_WITH_ALTERED_SEARCH_PATH,
    };
    let dll: Vec<u16> = dir
        .join("cudart64_13.dll")
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let module = LoadLibraryExW(PCWSTR(dll.as_ptr()), None, LOAD_WITH_ALTERED_SEARCH_PATH)
            .map_err(|e| format!("cudart64_13.dll could not be loaded: {}", e))?;
        let result = match GetProcAddress(module, s!("cudaGetDeviceCount")) {
            Some(function) => {
                let function: unsafe extern "system" fn(*mut i32) -> i32 =
                    std::mem::transmute(function);
                let mut count = 0i32;
                match function(&mut count) {
                    0 if count > 0 => Ok(count),
                    0 => Err(cuda_error_message(100)),
                    code => Err(cuda_error_message(code)),
                }
            }
            None => Err("cudart64_13.dll has no cudaGetDeviceCount.".to_string()),
        };
        let _ = FreeLibrary(module);
        result
    }
}

#[cfg(not(target_os = "windows"))]
fn cuda_device_count(_dir: &Path) -> Result<i32, String> {
    Err("CUDA is only available on Windows.".to_string())
}

/// `whisper-cli --version` must start and answer: catches a missing system DLL (or a loader
/// dialog nobody dismisses) with a timeout instead of hanging.
fn probe_whisper_cli(cli: &Path) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let dir = cli.parent().unwrap_or_else(|| Path::new("."));
    let mut command = Command::new(cli);
    command
        .arg("--version")
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(existing) = std::env::var_os("PATH") {
        let mut paths = vec![dir.to_path_buf()];
        paths.extend(std::env::split_paths(&existing));
        if let Ok(joined) = std::env::join_paths(paths) {
            command.env("PATH", joined);
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("whisper-cli could not be started: {}", e))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > VERSION_PROBE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "whisper-cli did not answer within 15 seconds (a Windows runtime library may be missing)."
                        .to_string(),
                );
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("whisper-cli could not be checked: {}", e)),
        }
    };
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    if !status.success() {
        return Err(format!(
            "whisper-cli exited with code 0x{:08x} (a Windows runtime library may be missing).",
            status.code().unwrap_or(-1) as u32
        ));
    }
    Ok(output.trim().to_string())
}

/// Self-test of the runtime the app would run for `variant` (bundled first, then downloaded).
fn verify_variant(variant: &str) -> Result<String, String> {
    let root = crate::paths::whisper_runtime_root();
    let cli = bundled_cli_path(variant, &root)
        .or_else(|| {
            let downloaded = root.join(variant).join("whisper-cli.exe");
            downloaded.is_file().then_some(downloaded)
        })
        .ok_or_else(|| format!("The {} speech engine is not installed.", variant))?;
    if let Some(issue) = crate::transcription::whisper_runtime_preflight_issue(&cli) {
        return Err(issue);
    }
    let version = probe_whisper_cli(&cli)?;
    let dir = cli.parent().unwrap_or(&root);
    let mut detail = version;
    if variant == "cuda" {
        let count = cuda_device_count(dir)?;
        detail = format!("{} ({} CUDA device(s))", detail, count);
    }
    Ok(detail)
}

fn emit_progress(
    app: &AppHandle,
    variant: &str,
    stage: &str,
    message: &str,
    done: Option<u64>,
    total: Option<u64>,
) {
    let _ = app.emit(
        PROGRESS_EVENT,
        RuntimeProgress {
            variant: variant.to_string(),
            stage: stage.to_string(),
            message: message.to_string(),
            downloaded: done,
            total,
        },
    );
}

#[tauri::command]
pub(crate) fn get_whisper_runtime_status() -> Result<WhisperRuntimeStatus, String> {
    status_in(&crate::paths::whisper_runtime_root())
}

#[tauri::command]
pub(crate) async fn download_whisper_runtime(
    app: AppHandle,
    variant: String,
) -> Result<WhisperRuntimeStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, entry) = load_variant(&variant)?;
        let root = crate::paths::whisper_runtime_root();
        info!(
            "[whisper.runtime] downloading {} into {}",
            variant,
            root.display()
        );
        let result = install_variant(
            &variant,
            &entry,
            &root,
            DOWNLOAD_STALL_TIMEOUT,
            &mut |stage, message, done, total| {
                emit_progress(&app, &variant, stage, message, done, total)
            },
        );
        if let Err(error) = &result {
            warn!("[whisper.runtime] {} download failed: {}", variant, error);
        }
        result?;
        status_in(&root)
    })
    .await
    .map_err(|e| format!("Speech engine download task failed: {}", e))?
}

#[tauri::command]
pub(crate) async fn detect_recommended_runtime() -> Result<RuntimeRecommendation, String> {
    tauri::async_runtime::spawn_blocking(|| recommend_from_adapters(&dxgi_adapters()))
        .await
        .map_err(|e| format!("GPU detection task failed: {}", e))
}

#[tauri::command]
pub(crate) async fn verify_whisper_runtime(
    variant: String,
) -> Result<WhisperRuntimeVerifyResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        load_variant(&variant)?;
        let outcome = verify_variant(&variant);
        if let Err(error) = &outcome {
            warn!(
                "[whisper.runtime] self-test of {} failed: {}",
                variant, error
            );
        }
        let failed_cuda = variant == "cuda" && outcome.is_err();
        Ok(WhisperRuntimeVerifyResult {
            ok: outcome.is_ok(),
            detail: outcome.unwrap_or_else(|e| e),
            fallback_variant: failed_cuda.then(|| "vulkan".to_string()),
            variant,
        })
    })
    .await
    .map_err(|e| format!("Speech engine self-test task failed: {}", e))?
}

#[cfg(test)]
mod tests;
