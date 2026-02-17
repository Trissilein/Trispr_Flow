// OPUS audio encoding via FFmpeg
// Converts WAV/PCM audio to OPUS format for efficient storage and transmission

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::{error, info};

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
  Audio,
  LowDelay,
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
      OpusApplication::Audio => "audio",
      OpusApplication::LowDelay => "lowdelay",
    }
  }
}

/// Find FFmpeg executable
pub fn find_ffmpeg() -> Result<PathBuf, String> {
  // Try bundled FFmpeg first (Windows)
  #[cfg(target_os = "windows")]
  {
    let exe_dir = std::env::current_exe()
      .map_err(|e| format!("Failed to get exe dir: {}", e))?
      .parent()
      .ok_or("No parent directory")?
      .to_path_buf();

    let bundled_ffmpeg = exe_dir.join("resources").join("ffmpeg").join("ffmpeg.exe");
    if bundled_ffmpeg.exists() {
      info!("Using bundled FFmpeg: {:?}", bundled_ffmpeg);
      return Ok(bundled_ffmpeg);
    }
  }

  // Try system FFmpeg
  let ffmpeg_name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };

  which::which(ffmpeg_name).map_err(|_| {
    format!(
      "FFmpeg not found. Please install FFmpeg or place it in resources/ffmpeg/{}",
      ffmpeg_name
    )
  })
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
  let ffmpeg_path = find_ffmpeg()?;

  // Build FFmpeg command
  let mut cmd = Command::new(&ffmpeg_path);

  // Hide console window on Windows (prevents focus steal during paste)
  #[cfg(target_os = "windows")]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
  }

  cmd
    .arg("-i")
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
  find_ffmpeg().is_ok()
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
