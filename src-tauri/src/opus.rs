// OPUS audio encoding via FFmpeg
// Converts WAV/PCM audio to OPUS format for efficient storage and transmission

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::AppHandle;
use tracing::{error, info, warn};

/// Result of OPUS encoding operation
#[derive(Serialize, Clone)]
pub struct OpusEncodeResult {
    pub output_path: String,
    pub input_size_bytes: u64,
    pub output_size_bytes: u64,
    pub compression_ratio: f32,
    pub duration_ms: u64,
}

/// OPUS encoder configuration
#[derive(Clone)]
pub struct OpusEncoderConfig {
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub channels: u32,
    pub compression_level: u32,
    pub vbr_enabled: bool,
    pub application: OpusApplication,
}

/// OPUS application mode
#[derive(Clone)]
pub enum OpusApplication {
    Voip,
}

impl Default for OpusEncoderConfig {
    fn default() -> Self {
        Self {
            bitrate_kbps: 64,
            sample_rate: 16000,
            channels: 1,
            compression_level: 10,
            vbr_enabled: true,
            application: OpusApplication::Voip,
        }
    }
}

impl OpusApplication {
    fn as_str(&self) -> &str {
        match self {
            OpusApplication::Voip => "voip",
        }
    }
}

fn ffmpeg_name() -> &'static str {
    if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

fn ffmpeg_supports_libopus(ffmpeg_path: &Path) -> bool {
    let mut probe = Command::new(ffmpeg_path);
    probe
        .arg("-hide_banner")
        .arg("-h")
        .arg("encoder=libopus")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        probe.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = match probe.output() {
        Ok(output) => output,
        Err(err) => {
            warn!(
                "Failed to probe FFmpeg encoder support at {}: {}",
                ffmpeg_path.display(),
                err
            );
            return false;
        }
    };

    if !output.status.success() {
        return false;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout.contains("Encoder libopus") || stderr.contains("Encoder libopus")
}

fn find_local_ffmpeg() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let ffmpeg = ffmpeg_name();

    #[cfg(target_os = "windows")]
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            candidates.push(exe_dir.join("resources").join("ffmpeg").join(ffmpeg));
        }
    }

    // Cargo builds resolve this to <repo>/src-tauri, useful for local dev.
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("bin")
            .join("ffmpeg")
            .join(ffmpeg),
    );

    // Also support invoking from repo root or src-tauri working directories.
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(
            cwd.join("src-tauri")
                .join("bin")
                .join("ffmpeg")
                .join(ffmpeg),
        );
        candidates.push(cwd.join("bin").join("ffmpeg").join(ffmpeg));
    }

    for candidate in candidates {
        if candidate.exists() {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
    }

    None
}

/// Find FFmpeg executable
pub fn find_ffmpeg() -> Result<PathBuf, String> {
    if let Some(local_ffmpeg) = find_local_ffmpeg() {
        info!("Using local/bundled FFmpeg: {:?}", local_ffmpeg);
        return Ok(local_ffmpeg);
    }

    // Try system FFmpeg
    let ffmpeg_name = ffmpeg_name();

    which::which(ffmpeg_name).map_err(|_| {
        format!(
            "FFmpeg not found. Install FFmpeg (with libopus) or place {} in resources/ffmpeg/ or src-tauri/bin/ffmpeg/.",
            ffmpeg_name
        )
    })
}

/// Find FFmpeg and ensure libopus encoder support is available.
pub fn find_ffmpeg_for_opus() -> Result<PathBuf, String> {
    let ffmpeg_path = find_ffmpeg()?;
    if ffmpeg_supports_libopus(&ffmpeg_path) {
        Ok(ffmpeg_path)
    } else {
        Err(format!(
            "FFmpeg found at '{}' but encoder 'libopus' is unavailable.",
            ffmpeg_path.display()
        ))
    }
}

/// Encode WAV file to OPUS format
pub fn encode_wav_to_opus(
    input_path: &Path,
    output_path: &Path,
    config: &OpusEncoderConfig,
) -> Result<OpusEncodeResult, String> {
    let start_time = std::time::Instant::now();

    // Validate input file exists
    if !input_path.exists() {
        return Err(format!("Input file does not exist: {:?}", input_path));
    }

    let input_size = fs::metadata(input_path)
        .map_err(|e| format!("Failed to get input file size: {}", e))?
        .len();

    info!(
        "Encoding {} ({} bytes) to OPUS...",
        input_path.display(),
        input_size
    );

    // Find FFmpeg
    let ffmpeg_path = find_ffmpeg_for_opus()?;

    // Build FFmpeg command
    let mut cmd = Command::new(&ffmpeg_path);

    // Hide console window on Windows (prevents focus steal during paste)
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd.arg("-i")
        .arg(input_path)
        .arg("-y")
        .arg("-c:a")
        .arg("libopus")
        .arg("-b:a")
        .arg(format!("{}k", config.bitrate_kbps))
        .arg("-vbr")
        .arg(if config.vbr_enabled { "on" } else { "off" })
        .arg("-compression_level")
        .arg(config.compression_level.to_string())
        .arg("-application")
        .arg(config.application.as_str())
        .arg("-ar")
        .arg(config.sample_rate.to_string())
        .arg("-ac")
        .arg(config.channels.to_string())
        .arg("-frame_duration")
        .arg("20")
        .arg(output_path)
        .arg("-loglevel")
        .arg("error")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Execute FFmpeg
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute FFmpeg: {}", e))?;

    // Check exit code
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("FFmpeg failed: {}", stderr);
        return Err(format!("FFmpeg encoding failed: {}", stderr));
    }

    // Validate output file exists
    if !output_path.exists() {
        return Err(format!("Output file was not created: {:?}", output_path));
    }

    let output_size = fs::metadata(output_path)
        .map_err(|e| format!("Failed to get output file size: {}", e))?
        .len();

    let compression_ratio = output_size as f32 / input_size as f32;
    let duration_ms = start_time.elapsed().as_millis() as u64;

    info!(
        "OPUS encoding complete: {} bytes → {} bytes ({:.1}% reduction) in {} ms",
        input_size,
        output_size,
        (1.0 - compression_ratio) * 100.0,
        duration_ms
    );

    Ok(OpusEncodeResult {
        output_path: output_path.to_string_lossy().to_string(),
        input_size_bytes: input_size,
        output_size_bytes: output_size,
        compression_ratio,
        duration_ms,
    })
}

/// Encode WAV file to OPUS with default settings
pub fn encode_wav_to_opus_default(
    input_path: &Path,
    output_path: &Path,
) -> Result<OpusEncodeResult, String> {
    encode_wav_to_opus(input_path, output_path, &OpusEncoderConfig::default())
}

/// Check if FFmpeg is available
pub fn check_ffmpeg_available() -> bool {
    find_ffmpeg_for_opus().is_ok()
}

/// Probe whether NVENC hardware encoding is functional on this machine.
///
/// Runs a 1-frame synthetic encode via h264_nvenc. Fails fast (<1s) if the
/// driver/hardware isn't present or if the NVENC SDK version is incompatible
/// (e.g. Blackwell GPUs with older SDK returning EINVAL or INVALID_VERSION).
/// Result is cached via OnceLock — only one FFmpeg spawn per process lifetime.
pub fn probe_nvenc_available() -> bool {
    use std::sync::OnceLock;
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        let Ok(ffmpeg) = find_ffmpeg() else {
            return false;
        };
        let mut cmd = Command::new(&ffmpeg);
        cmd.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=128x128:rate=1:duration=0.1",
            "-c:v",
            "h264_nvenc",
            "-frames:v",
            "1",
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        match cmd.output() {
            Ok(out) => {
                if out.status.success() {
                    true
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!("[nvenc-probe] h264_nvenc unavailable: {}", stderr.trim());
                    false
                }
            }
            Err(e) => {
                tracing::warn!("[nvenc-probe] FFmpeg spawn failed: {}", e);
                false
            }
        }
    })
}

/// Get FFmpeg version string
pub fn get_ffmpeg_version() -> Result<String, String> {
    let ffmpeg_path = find_ffmpeg()?;

    let mut version_cmd = Command::new(&ffmpeg_path);
    version_cmd.arg("-version");

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        version_cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = version_cmd
        .output()
        .map_err(|e| format!("Failed to run FFmpeg: {}", e))?;

    let version_output = String::from_utf8_lossy(&output.stdout);
    let first_line = version_output
        .lines()
        .next()
        .unwrap_or("Unknown version")
        .to_string();

    Ok(first_line)
}

#[tauri::command]
pub(crate) fn encode_to_opus(
    app: AppHandle,
    input_path: String,
    output_path: String,
    bitrate_kbps: Option<u32>,
) -> Result<OpusEncodeResult, String> {
    let allowed_root = crate::paths::resolve_base_dir(&app);

    let input = crate::paths::validate_path_within(&input_path, &allowed_root)?;
    let output = crate::paths::validate_path_within(&output_path, &allowed_root)?;

    if let Some(bitrate) = bitrate_kbps {
        let mut config = OpusEncoderConfig::default();
        config.bitrate_kbps = bitrate;
        encode_wav_to_opus(&input, &output, &config)
    } else {
        encode_wav_to_opus_default(&input, &output)
    }
}

#[tauri::command]
pub(crate) fn check_ffmpeg() -> Result<bool, String> {
    Ok(check_ffmpeg_available())
}

#[tauri::command]
pub(crate) fn get_ffmpeg_version_info() -> Result<String, String> {
    get_ffmpeg_version()
}
