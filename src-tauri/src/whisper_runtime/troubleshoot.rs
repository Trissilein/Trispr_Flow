//! Help for a speech engine setup that did not work: a structured cause + hint for every
//! failure, the manual way (open the pinned download in the browser, import the zip the user
//! downloaded) and a diagnostics summary the user can copy.
//!
//! The import runs the same pipeline as the download (`install_archive`): checksum against the
//! compiled-in pin, only the pinned files, staging folder, atomic swap. It adds the checks a
//! hand-picked file needs first (right extension, pinned size, pinned SHA-256, and a clear
//! message when the file is the other variant's package).

use super::*;

const RELEASE_PAGE_PREFIX: &str = "https://github.com/Trissilein/Trispr_Flow/releases/tag/";

// ---------------------------------------------------------------------------------------
// Structured errors
// ---------------------------------------------------------------------------------------

/// What the wizard shows when the engine setup fails: `message` is what went wrong, `hint` what
/// the user can do. `code` is stable for the UI and diagnostics, `detail` is the technical text.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeError {
    pub(super) code: &'static str,
    pub(super) message: String,
    pub(super) hint: String,
    pub(super) detail: String,
}

impl RuntimeError {
    fn new(code: &'static str, message: &str, hint: &str, detail: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            hint: hint.to_string(),
            detail: detail.to_string(),
        }
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.message, self.hint)
    }
}

/// The pipeline reports plain text; every command turns it into a structured error here.
impl From<String> for RuntimeError {
    fn from(raw: String) -> Self {
        classify_error(&raw)
    }
}

const USE_BROWSER_DOWNLOAD: &str =
    "download the file in your browser and use it under \"Download it yourself\"";

/// HTTP status of "the download server answered HTTP 404 (...)".
fn http_status(raw: &str) -> Option<u16> {
    let start = raw.find("HTTP ")? + "HTTP ".len();
    let digits: String = raw[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok().filter(|code| (100..600).contains(code))
}

/// Maps an error text of the download/install pipeline to a cause and a hint.
pub(super) fn classify_error(raw: &str) -> RuntimeError {
    let lower = raw.to_ascii_lowercase();
    let any = |needles: &[&str]| needles.iter().any(|needle| lower.contains(needle));
    let e =
        |code: &'static str, message: &str, hint: &str| RuntimeError::new(code, message, hint, raw);

    if any(&["already running"]) {
        return e(
            "busy",
            "Another speech engine setup is already running.",
            "Wait until it has finished, then try again.",
        );
    }
    if any(&["not enough free disk space"]) {
        return e(
            "disk_full",
            "There is not enough free disk space for the speech engine.",
            "Free up about 1.5 GB on the drive that holds %LOCALAPPDATA%, then retry.",
        );
    }
    if any(&["in use and cannot be replaced"]) {
        return e(
            "in_use",
            "The installed speech engine is in use and cannot be replaced.",
            "Restart Trispr Flow so that nothing is using it, then retry.",
        );
    }
    if any(&["checksum"]) {
        return e(
            "checksum",
            "The file is damaged or was changed on its way to this PC.",
            &format!(
                "Retry once. If it fails again, a proxy, firewall or antivirus may be altering the download: {} (compare the SHA-256 shown there).",
                USE_BROWSER_DOWNLOAD
            ),
        );
    }
    if let Some(status) = http_status(raw) {
        let (message, hint) = match status {
            404 | 410 => (
                format!(
                    "The download server says the file is gone (HTTP {}).",
                    status
                ),
                format!(
                    "{}. If it is really gone, a newer Trispr Flow version has a new link.",
                    capitalize(USE_BROWSER_DOWNLOAD)
                ),
            ),
            401 | 403 | 429 => (
                format!("GitHub refused the request (HTTP {}).", status),
                format!("Wait a few minutes and retry, or {}.", USE_BROWSER_DOWNLOAD),
            ),
            500..=599 => (
                format!("GitHub had a server problem (HTTP {}).", status),
                format!("Wait a few minutes and retry, or {}.", USE_BROWSER_DOWNLOAD),
            ),
            _ => (
                format!("The download server answered HTTP {}.", status),
                format!("Retry, or {}.", USE_BROWSER_DOWNLOAD),
            ),
        };
        return e("http_status", &message, &hint);
    }
    if any(&["could not reach the download server"]) {
        if any(&[
            "certificate",
            "handshake",
            "tls",
            "ssl",
            "proxy failed",
            "proxy credentials",
            "malformed proxy",
        ]) {
            return e(
                "tls_proxy",
                "A secure connection to github.com could not be set up.",
                &format!(
                    "A proxy, firewall or antivirus that inspects HTTPS may be in the way. {}.",
                    capitalize(USE_BROWSER_DOWNLOAD)
                ),
            );
        }
        return e(
            "offline",
            "Could not reach the download server (github.com).",
            &format!(
                "Check your internet connection, VPN and firewall. On a PC without internet, {} on another PC.",
                USE_BROWSER_DOWNLOAD
            ),
        );
    }
    if any(&["download stalled", "no data for"]) {
        return e(
            "timeout",
            "The download stalled.",
            &format!(
                "Your connection is slow or was cut. Retry, or {}.",
                USE_BROWSER_DOWNLOAD
            ),
        );
    }
    if any(&[
        "connection was closed",
        "connection broke",
        "ended early",
        "larger than expected",
        "the download is ",
    ]) {
        return e(
            "interrupted",
            "The download was interrupted.",
            &format!(
                "Retry. If it keeps happening, security software may be cutting the connection: {}.",
                USE_BROWSER_DOWNLOAD
            ),
        );
    }
    if any(&[
        "(os error 5)",
        "(os error 32)",
        "(os error 33)",
        "(os error 225)",
        "(os error 226)",
        "(os error 1224)",
        "failed to open the downloaded archive",
    ]) {
        return e(
            "file_access",
            "Windows or a security program blocked access to a file.",
            "Antivirus or Controlled folder access may be blocking Trispr Flow in %LOCALAPPDATA%\\Trispr Flow\\runtime. Allow the folder (or pause the scan briefly), then retry.",
        );
    }
    if any(&[
        "not a valid zip",
        "archive holds",
        "unexpected file",
        "unsafe archive",
        "duplicate file",
        "in the archive",
        "unpacked to",
        "unreadable archive",
        "failed to read archive entry",
        "failed to unpack",
    ]) {
        return e(
            "invalid_package",
            "The package does not contain what was expected.",
            "Download it again from the official release page and use that file.",
        );
    }
    e(
        "unknown",
        raw,
        "Retry. If it keeps failing, use \"Download it yourself\" below and copy the diagnostics when you ask for help.",
    )
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

// ---------------------------------------------------------------------------------------
// Pins, labels, manual info
// ---------------------------------------------------------------------------------------

type Pins = Vec<(&'static str, Variant)>;

pub(super) fn load_pins() -> Result<(String, Pins), String> {
    let mut pins = Vec::new();
    let mut version = String::new();
    for name in VARIANTS {
        let (v, entry) = load_variant(name)?;
        version = v;
        pins.push((name, entry));
    }
    Ok((version, pins))
}

fn variant_label(name: &str) -> &'static str {
    match name {
        "cuda" => "NVIDIA CUDA",
        _ => "Vulkan",
    }
}

fn which_gpu_hint(name: &str) -> String {
    match name {
        "cuda" => format!(
            "NVIDIA GeForce RTX, or GTX 16 series and newer, with a current driver (version {} or newer).",
            CUDA_MIN_DRIVER_HINT
        ),
        _ => "Everything else: Intel and AMD graphics, older NVIDIA cards, or when CUDA does not work on your PC.".to_string(),
    }
}

/// File name of the release asset (the last segment of the pinned URL).
fn asset_name(variant: &Variant) -> String {
    variant
        .url
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// `https://github.com/.../releases/download/<tag>/<asset>` -> `.../releases/tag/<tag>`.
fn release_page_url(variant: &Variant) -> Option<String> {
    let tag = variant
        .url
        .strip_prefix(DOWNLOAD_URL_PREFIX)?
        .split('/')
        .next()?;
    Some(format!("{}{}", RELEASE_PAGE_PREFIX, tag))
}

fn size_mb(bytes: u64) -> u64 {
    (bytes as f64 / 1024.0 / 1024.0).round() as u64
}

fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    if mb >= 10.0 {
        format!("{:.0} MB", mb)
    } else if mb >= 1.0 {
        format!("{:.1} MB", mb)
    } else {
        format!("{} bytes", bytes)
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ManualVariantInfo {
    pub(super) variant: String,
    pub(super) label: String,
    pub(super) asset: String,
    pub(super) url: String,
    pub(super) size_mb: u64,
    pub(super) size_bytes: u64,
    pub(super) sha256: String,
    pub(super) which_gpu_hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ManualInfo {
    pub(super) release_page_url: String,
    pub(super) whisper_cpp_version: String,
    pub(super) variants: Vec<ManualVariantInfo>,
}

pub(super) fn manual_info_from(version: &str, pins: &[(&str, Variant)]) -> ManualInfo {
    ManualInfo {
        release_page_url: pins
            .first()
            .and_then(|(_, v)| release_page_url(v))
            .unwrap_or_default(),
        whisper_cpp_version: version.to_string(),
        variants: pins
            .iter()
            .map(|(name, v)| ManualVariantInfo {
                variant: name.to_string(),
                label: variant_label(name).to_string(),
                asset: asset_name(v),
                url: v.url.clone(),
                size_mb: size_mb(v.zip_size),
                size_bytes: v.zip_size,
                sha256: v.zip_sha256.clone(),
                which_gpu_hint: which_gpu_hint(name),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------------------
// Open the pinned download in the browser
// ---------------------------------------------------------------------------------------

/// Only the pinned asset URLs and the pinned release page may be opened, and only when they
/// really are plain `https://github.com/Trissilein/Trispr_Flow/releases/...` URLs.
pub(super) fn check_openable_url(url: &str, pins: &[(&str, Variant)]) -> Result<(), String> {
    let pinned = pins
        .iter()
        .any(|(_, v)| v.url == url || release_page_url(v).as_deref() == Some(url));
    let official = is_plain_github_url(url, DOWNLOAD_URL_PREFIX)
        || is_plain_github_url(url, RELEASE_PAGE_PREFIX);
    if pinned && official {
        Ok(())
    } else {
        Err(format!(
            "Refusing to open '{}': only the pinned speech engine downloads can be opened.",
            url
        ))
    }
}

/// Opens the pinned download of `variant_name` through `launch` (the browser in production).
pub(super) fn open_download_page(
    variant_name: &str,
    pins: &[(&str, Variant)],
    launch: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    let (_, pin) = pins
        .iter()
        .find(|(name, _)| *name == variant_name)
        .ok_or_else(|| {
            format!(
                "Unknown speech engine variant '{}' (expected cuda or vulkan).",
                variant_name
            )
        })?;
    check_openable_url(&pin.url, pins)?;
    launch(&pin.url)
}

/// `cmd /c start "" <url>`: the URL is one argument, never part of a shell line built from
/// user input. It comes from the pin; the character check is a second line of defence against
/// the characters `cmd` treats specially.
#[cfg(target_os = "windows")]
pub(super) fn launch_browser(url: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if !url
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-._~:/".contains(&b))
    {
        return Err("Refusing to open a URL with unexpected characters.".to_string());
    }
    let status = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .status()
        .map_err(|e| format!("Could not open the browser: {}", e))?;
    if status.success() {
        Ok(())
    } else {
        Err("Could not open the browser.".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn launch_browser(_url: &str) -> Result<(), String> {
    Err("Opening the browser is only supported on Windows.".to_string())
}

// ---------------------------------------------------------------------------------------
// Import of a hand-downloaded zip
// ---------------------------------------------------------------------------------------

fn wrong_package(selected: &str, found: &str) -> RuntimeError {
    RuntimeError::new(
        "wrong_package",
        &format!(
            "This is the {} package, but you selected {}.",
            variant_label(found),
            variant_label(selected)
        ),
        &format!(
            "Use this file with the {} package, or download the {} package.",
            variant_label(found),
            variant_label(selected)
        ),
        &format!("file matches the pinned {} package", found),
    )
}

fn wrong_file(selected: &str, pin: &Variant, size: u64, digest: Option<&str>) -> RuntimeError {
    let detected = match digest {
        Some(digest) => format!("{}, SHA-256 {}", format_size(size), digest),
        None => format_size(size),
    };
    RuntimeError::new(
        "wrong_file",
        &format!(
            "This file does not match the expected {} package ({}). Detected: {}.",
            variant_label(selected),
            asset_name(pin),
            detected
        ),
        &format!(
            "Expected {}, SHA-256 {}. The file may be incomplete, damaged or a different file: download it again from the official release page.",
            format_size(pin.zip_size),
            pin.zip_sha256
        ),
        &format!("size {} bytes, sha256 {}", size, digest.unwrap_or("not computed")),
    )
}

fn unreadable_file(file_name: &str, error: &std::io::Error) -> RuntimeError {
    let detail = error.to_string();
    if error.kind() == std::io::ErrorKind::NotFound {
        return RuntimeError::new(
            "file_missing",
            &format!("The file '{}' could not be found.", file_name),
            "Choose the downloaded .zip file again.",
            &detail,
        );
    }
    RuntimeError::new(
        "file_locked",
        &format!("The file '{}' cannot be read right now.", file_name),
        "If your browser is still downloading it, wait until it has finished. Otherwise check that Windows or antivirus is not holding it, then try again.",
        &detail,
    )
}

/// Verifies and installs the zip the user picked for `variant_name`. The file is only read;
/// nothing is written to the runtime folder before size and SHA-256 match the pin.
pub(super) fn import_with_pins(
    root: &Path,
    variant_name: &str,
    pins: &[(&str, Variant)],
    path: &str,
    on_progress: &mut Progress<'_>,
) -> Result<PathBuf, RuntimeError> {
    let selected = &pins
        .iter()
        .find(|(name, _)| *name == variant_name)
        .ok_or_else(|| {
            RuntimeError::from(format!(
                "Unknown speech engine variant '{}' (expected cuda or vulkan).",
                variant_name
            ))
        })?
        .1;

    let file_path = Path::new(path.trim());
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_zip = file_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    if !is_zip {
        return Err(RuntimeError::new(
            "not_zip",
            "Please choose the .zip file you downloaded.",
            &format!(
                "The speech engine package is a .zip file named like {}.",
                asset_name(selected)
            ),
            &file_name,
        ));
    }

    match fs::metadata(file_path) {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => {
            return Err(RuntimeError::new(
                "file_missing",
                &format!("'{}' is not a file.", file_name),
                "Choose the downloaded .zip file again.",
                &file_name,
            ))
        }
        Err(e) => return Err(unreadable_file(&file_name, &e)),
    }

    let _job = begin_job(variant_name, root)?;
    on_progress("verify", "Checking the file...", None, None);
    let mut file = open_archive_locked(file_path).map_err(|e| unreadable_file(&file_name, &e))?;
    let size = file
        .metadata()
        .map_err(|e| unreadable_file(&file_name, &e))?
        .len();

    let others: Vec<&(&str, Variant)> = pins
        .iter()
        .filter(|(name, _)| *name != variant_name)
        .collect();
    // A different size can never carry the pinned hash: no need to read gigabytes to say so.
    if size != selected.zip_size && !others.iter().any(|(_, o)| o.zip_size == size) {
        return Err(wrong_file(variant_name, selected, size, None));
    }
    let digest = sha256_reader(&mut file)?;
    if !digest.eq_ignore_ascii_case(&selected.zip_sha256) {
        if let Some((other, _)) = others
            .iter()
            .find(|(_, o)| digest.eq_ignore_ascii_case(&o.zip_sha256))
        {
            return Err(wrong_package(variant_name, other));
        }
        return Err(wrong_file(variant_name, selected, size, Some(&digest)));
    }

    let stamp = prepare_root(root, variant_name)?;
    Ok(install_archive(
        file,
        &digest,
        variant_name,
        selected,
        root,
        stamp,
        on_progress,
    )?)
}

// ---------------------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DiagnosticsAdapter {
    pub(super) vendor_id: String,
    pub(super) name: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DiagnosticsVariant {
    pub(super) variant: String,
    pub(super) bundled: bool,
    pub(super) downloaded: bool,
}

/// What a support request needs and nothing personal: no user name (the profile folder is
/// written as `%LOCALAPPDATA%`), no settings, no tokens.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct SetupDiagnostics {
    pub(super) app_version: String,
    pub(super) os: String,
    pub(super) arch: String,
    pub(super) runtime_dir: String,
    pub(super) whisper_cpp_version: String,
    pub(super) adapters: Vec<DiagnosticsAdapter>,
    pub(super) variants: Vec<DiagnosticsVariant>,
}

/// Replaces the start of `path` by the environment variable that holds it.
pub(super) fn mask_user_dirs(
    path: &str,
    local_app_data: Option<&str>,
    user_profile: Option<&str>,
) -> String {
    let lowered = path.to_ascii_lowercase();
    for (prefix, name) in [
        (local_app_data, "%LOCALAPPDATA%"),
        (user_profile, "%USERPROFILE%"),
    ] {
        let Some(prefix) = prefix
            .map(|p| p.trim_end_matches(['\\', '/']))
            .filter(|p| !p.is_empty())
        else {
            continue;
        };
        let lowered_prefix = prefix.to_ascii_lowercase();
        if lowered.starts_with(&lowered_prefix)
            && matches!(
                path.as_bytes().get(prefix.len()),
                None | Some(b'\\') | Some(b'/')
            )
        {
            return format!("{}{}", name, &path[prefix.len()..]);
        }
    }
    path.to_string()
}

#[cfg(target_os = "windows")]
fn os_version() -> String {
    use windows::core::{s, w};
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    #[repr(C)]
    struct OsVersionInfo {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        service_pack: [u16; 128],
    }
    unsafe {
        let Ok(module) = GetModuleHandleW(w!("ntdll.dll")) else {
            return "Windows (version unknown)".to_string();
        };
        let Some(function) = GetProcAddress(module, s!("RtlGetVersion")) else {
            return "Windows (version unknown)".to_string();
        };
        let function: unsafe extern "system" fn(*mut OsVersionInfo) -> i32 =
            std::mem::transmute(function);
        let mut info = OsVersionInfo {
            size: std::mem::size_of::<OsVersionInfo>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            service_pack: [0; 128],
        };
        if function(&mut info) == 0 {
            format!("Windows {}.{}.{}", info.major, info.minor, info.build)
        } else {
            "Windows (version unknown)".to_string()
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn os_version() -> String {
    std::env::consts::OS.to_string()
}

pub(super) fn collect_diagnostics(root: &Path) -> SetupDiagnostics {
    let status = status_in(root).ok();
    SetupDiagnostics {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        os: os_version(),
        arch: std::env::consts::ARCH.to_string(),
        runtime_dir: mask_user_dirs(
            &root.display().to_string(),
            std::env::var("LOCALAPPDATA").ok().as_deref(),
            std::env::var("USERPROFILE").ok().as_deref(),
        ),
        whisper_cpp_version: status
            .as_ref()
            .map(|s| s.whisper_cpp_version.clone())
            .unwrap_or_default(),
        adapters: dxgi_adapters()
            .into_iter()
            .map(|(vendor, name)| DiagnosticsAdapter {
                vendor_id: format!("0x{:04X}", vendor),
                name,
            })
            .collect(),
        variants: status
            .map(|s| {
                s.variants
                    .into_iter()
                    .map(|v| DiagnosticsVariant {
                        variant: v.variant,
                        bundled: v.bundled,
                        downloaded: v.downloaded,
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn get_whisper_runtime_manual_info() -> Result<ManualInfo, RuntimeError> {
    let (version, pins) = load_pins()?;
    Ok(manual_info_from(&version, &pins))
}

#[tauri::command]
pub(crate) fn open_runtime_download_page(variant: String) -> Result<(), RuntimeError> {
    let (_, pins) = load_pins()?;
    Ok(open_download_page(&variant, &pins, launch_browser)?)
}

#[tauri::command]
pub(crate) async fn import_whisper_runtime_zip(
    app: AppHandle,
    variant: String,
    path: String,
) -> Result<WhisperRuntimeStatus, RuntimeError> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, pins) = load_pins()?;
        let root = crate::paths::whisper_runtime_root();
        info!("[whisper.runtime] importing a {} package by hand", variant);
        let result = import_with_pins(
            &root,
            &variant,
            &pins,
            &path,
            &mut |stage, message, done, total| {
                emit_progress(&app, &variant, stage, message, done, total)
            },
        );
        if let Err(error) = &result {
            warn!(
                "[whisper.runtime] {} import failed ({}): {}",
                variant, error.code, error.detail
            );
        }
        result?;
        Ok(status_in(&root)?)
    })
    .await
    .map_err(|e| RuntimeError::from(format!("Speech engine import task failed: {}", e)))?
}

#[tauri::command]
pub(crate) async fn get_whisper_runtime_diagnostics() -> Result<SetupDiagnostics, String> {
    tauri::async_runtime::spawn_blocking(|| {
        collect_diagnostics(&crate::paths::whisper_runtime_root())
    })
    .await
    .map_err(|e| format!("Diagnostics task failed: {}", e))
}
