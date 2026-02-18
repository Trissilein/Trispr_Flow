// VibeVoice-ASR Sidecar Integration Layer
// Handles communication with the Python FastAPI sidecar process:
// - HTTP client for /transcribe, /health, /reload-model endpoints
// - Request building (JSON body with local audio_path)
// - Response parsing (speaker-diarized segments)
// - Error handling with retry logic
// - Timeout management

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// ============================================================================
// Configuration
// ============================================================================

/// Default sidecar host and port
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8765;

/// Timeout settings
const HEALTH_CHECK_TIMEOUT_MS: u64 = 3000;
const TRANSCRIBE_TIMEOUT_MS: u64 = 300_000; // 5 minutes for long audio
const RELOAD_TIMEOUT_MS: u64 = 60_000; // 1 minute for model reload

/// Retry settings
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 1000;

// ============================================================================
// API Types
// ============================================================================

/// Single speaker-diarized transcription segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
  pub speaker: String,
  pub start_time: f64,
  pub end_time: f64,
  pub text: String,
}

/// Transcription metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionMetadata {
  pub duration: f64,
  pub language: String,
  pub processing_time: f64,
  pub model_precision: String,
  pub num_speakers: i32,
}

/// Full transcription response from sidecar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResponse {
  pub status: String,
  pub segments: Vec<TranscriptionSegment>,
  pub metadata: TranscriptionMetadata,
}

/// Health check response from sidecar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
  pub status: String,
  pub model_loaded: bool,
  pub gpu_available: bool,
  pub vram_used_mb: Option<i64>,
  pub vram_total_mb: Option<i64>,
}

/// Model reload response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReloadResponse {
  pub status: String,
  pub message: String,
}

/// Sidecar connection error types
#[derive(Debug)]
pub enum SidecarError {
  /// Sidecar process is not running
  NotRunning,
  /// HTTP request timed out
  Timeout(String),
  /// Server returned an error
  ServerError(u16, String),
  /// Network/connection error
  ConnectionError(String),
  /// Invalid response from server
  ParseError(String),
  /// GPU out of memory
  OutOfMemory(String),
}

impl std::fmt::Display for SidecarError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      SidecarError::NotRunning => write!(f, "Sidecar is not running"),
      SidecarError::Timeout(msg) => write!(f, "Request timed out: {}", msg),
      SidecarError::ServerError(code, msg) => write!(f, "Server error {}: {}", code, msg),
      SidecarError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
      SidecarError::ParseError(msg) => write!(f, "Parse error: {}", msg),
      SidecarError::OutOfMemory(msg) => write!(f, "GPU out of memory: {}", msg),
    }
  }
}

impl From<SidecarError> for String {
  fn from(err: SidecarError) -> String {
    err.to_string()
  }
}

// ============================================================================
// Sidecar Client
// ============================================================================

/// HTTP client for communicating with the VibeVoice-ASR sidecar
pub struct SidecarClient {
  base_url: String,
}

impl SidecarClient {
  /// Create a new client with default host/port
  pub fn new() -> Self {
    Self {
      base_url: format!("http://{}:{}", DEFAULT_HOST, DEFAULT_PORT),
    }
  }

  /// Create a new client with custom host/port
  pub fn with_url(host: &str, port: u16) -> Self {
    Self {
      base_url: format!("http://{}:{}", host, port),
    }
  }

  // --------------------------------------------------------------------------
  // Health Check
  // --------------------------------------------------------------------------

  /// Check if sidecar is running and healthy
  pub fn health_check(&self) -> Result<HealthResponse, SidecarError> {
    let url = format!("{}/health", self.base_url);

    let response = ureq::get(&url)
      .timeout(Duration::from_millis(HEALTH_CHECK_TIMEOUT_MS))
      .call()
      .map_err(|e| match e {
        ureq::Error::Transport(t) => {
          if t.kind() == ureq::ErrorKind::ConnectionFailed {
            SidecarError::NotRunning
          } else {
            SidecarError::ConnectionError(t.to_string())
          }
        }
        ureq::Error::Status(code, resp) => {
          let body = resp.into_string().unwrap_or_default();
          SidecarError::ServerError(code, body)
        }
      })?;

    let health: HealthResponse = response
      .into_json()
      .map_err(|e| SidecarError::ParseError(e.to_string()))?;

    Ok(health)
  }

  /// Check if sidecar is running (simple boolean)
  pub fn is_running(&self) -> bool {
    self.health_check().is_ok()
  }

  /// Wait for sidecar to become ready with timeout
  pub fn wait_for_ready(&self, timeout_ms: u64) -> Result<HealthResponse, SidecarError> {
    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);

    loop {
      match self.health_check() {
        Ok(health) => return Ok(health),
        Err(SidecarError::NotRunning) => {
          if start.elapsed() >= timeout {
            return Err(SidecarError::Timeout(
              "Sidecar did not start within timeout".to_string(),
            ));
          }
          std::thread::sleep(Duration::from_millis(500));
        }
        Err(e) => return Err(e),
      }
    }
  }

  // --------------------------------------------------------------------------
  // Transcription
  // --------------------------------------------------------------------------

  /// Send audio file for transcription with speaker diarization.
  /// Uses JSON body with file path (sidecar reads file locally).
  ///
  /// # Arguments
  /// * `audio_path` - Path to audio file (OPUS or WAV)
  /// * `precision` - Model precision ("fp16" or "int8"), None for default
  /// * `language` - Language code or "auto", None for auto-detection
  pub fn transcribe(
    &self,
    audio_path: &Path,
    precision: Option<&str>,
    language: Option<&str>,
  ) -> Result<TranscriptionResponse, SidecarError> {
    let url = format!("{}/transcribe", self.base_url);

    if !audio_path.exists() {
      return Err(SidecarError::ConnectionError(format!(
        "Audio file does not exist: {:?}",
        audio_path
      )));
    }

    let file_size = std::fs::metadata(audio_path)
      .map(|m| m.len())
      .unwrap_or(0);

    info!(
      "Requesting transcription for {} ({} bytes)",
      audio_path.display(),
      file_size
    );

    // Build JSON request body with file path
    // Since sidecar runs on same machine, it can read the file directly
    let body = serde_json::json!({
      "audio_path": audio_path.to_string_lossy(),
      "precision": precision.unwrap_or("fp16"),
      "language": language.unwrap_or("auto"),
    });

    let response = ureq::post(&url)
      .timeout(Duration::from_millis(TRANSCRIBE_TIMEOUT_MS))
      .send_json(body)
      .map_err(|e| match e {
        ureq::Error::Transport(t) => {
          if t.kind() == ureq::ErrorKind::ConnectionFailed {
            SidecarError::NotRunning
          } else {
            SidecarError::ConnectionError(t.to_string())
          }
        }
        ureq::Error::Status(code, resp) => {
          let body = resp.into_string().unwrap_or_default();
          if body.contains("CUDA out of memory") || body.contains("OutOfMemoryError") {
            SidecarError::OutOfMemory(body)
          } else {
            SidecarError::ServerError(code, body)
          }
        }
      })?;

    // Parse response
    let transcription: TranscriptionResponse = response
      .into_json()
      .map_err(|e| SidecarError::ParseError(e.to_string()))?;

    info!(
      "Transcription complete: {} segments, {} speakers, {:.1}s processing",
      transcription.segments.len(),
      transcription.metadata.num_speakers,
      transcription.metadata.processing_time
    );

    Ok(transcription)
  }

  // --------------------------------------------------------------------------
  // Model Reload
  // --------------------------------------------------------------------------

  /// Reload model with different precision
  pub fn reload_model(&self, precision: &str) -> Result<ReloadResponse, SidecarError> {
    let url = format!("{}/reload-model", self.base_url);

    let body = serde_json::json!({ "precision": precision });

    let response = ureq::post(&url)
      .timeout(Duration::from_millis(RELOAD_TIMEOUT_MS))
      .send_json(body)
      .map_err(|e| match e {
        ureq::Error::Transport(t) => SidecarError::ConnectionError(t.to_string()),
        ureq::Error::Status(code, resp) => {
          let body = resp.into_string().unwrap_or_default();
          SidecarError::ServerError(code, body)
        }
      })?;

    let reload: ReloadResponse = response
      .into_json()
      .map_err(|e| SidecarError::ParseError(e.to_string()))?;

    info!("Model reload: {}", reload.message);
    Ok(reload)
  }
}

impl Default for SidecarClient {
  fn default() -> Self {
    Self::new()
  }
}
