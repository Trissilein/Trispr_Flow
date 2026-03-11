//! Whisper-Server persistent process manager.
//! Keeps the Whisper model pre-loaded in GPU VRAM to eliminate per-transcription model-load overhead (~1.4s).

use crate::paths::resolve_whisper_server_path;
use crate::state::AppState;
use std::io::Write;
use std::path::Path;
use std::time::Duration;
use tauri::AppHandle;
use tracing::{info, warn};

/// Default port for Whisper-Server HTTP API.
pub const WHISPER_SERVER_PORT: u16 = 8178;

/// Health check: ping the server's root endpoint.
pub fn ping_whisper_server(port: u16) -> bool {
    let agent = ureq::builder()
        .timeout_connect(Duration::from_millis(200))
        .timeout_read(Duration::from_millis(200))
        .build();

    agent
        .get(&format!("http://127.0.0.1:{port}/"))
        .call()
        .is_ok()
}

/// Start the Whisper-Server process and wait for it to be ready.
///
/// Returns Ok if the server is running (either just started or already running).
/// Returns Err if the binary is missing or startup failed after timeout.
pub fn start_whisper_server(
    _app: &AppHandle,
    state: &AppState,
    model_path: &Path,
) -> Result<(), String> {
    let port = state.whisper_server_port.load(std::sync::atomic::Ordering::Relaxed);

    // Already running?
    if ping_whisper_server(port) {
        info!("whisper-server already running on port {}", port);
        return Ok(());
    }

    let server_path = resolve_whisper_server_path().ok_or_else(|| {
        "whisper-server.exe not found (Phase 0 incomplete — binary not sourced)".to_string()
    })?;

    info!(
        "Starting whisper-server: {} -m {} --port {}",
        server_path.display(),
        model_path.display(),
        port
    );

    let mut cmd = std::process::Command::new(&server_path);
    cmd.arg("-m")
        .arg(model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("-t")
        .arg(optimal_thread_count().to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn().map_err(|e| format!("Failed to spawn whisper-server: {}", e))?;

    // Store the child handle for cleanup on app exit
    *state.managed_whisper_server_child.lock().unwrap() = Some(child);

    // Poll for server readiness (max 8 seconds, check every 250ms)
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while std::time::Instant::now() < deadline {
        if ping_whisper_server(port) {
            info!("whisper-server ready on port {}", port);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    // Timeout — log warning but don't fail. Fallback to CLI will be used.
    warn!(
        "whisper-server startup timeout (8s) — will fall back to CLI transcription for now"
    );
    Ok(())
}

/// Transcribe WAV bytes via HTTP to the Whisper-Server.
///
/// Builds multipart/form-data manually since ureq v2 has no multipart feature.
pub fn transcribe_via_server(
    wav_bytes: &[u8],
    port: u16,
    language: &str,
) -> Result<String, String> {
    let boundary = "trispr_boundary_8f3a2b";
    let mut body: Vec<u8> = Vec::new();

    // Add WAV file part
    write_multipart_field_file(&mut body, boundary, "file", "audio.wav", "audio/wav", wav_bytes)
        .map_err(|e| format!("Failed to encode multipart: {}", e))?;

    // Add response-format part
    write_multipart_field_text(&mut body, boundary, "response_format", "json")
        .map_err(|e| format!("Failed to encode multipart: {}", e))?;

    // Add language part
    write_multipart_field_text(&mut body, boundary, "language", language)
        .map_err(|e| format!("Failed to encode multipart: {}", e))?;

    // Close boundary
    write!(body, "--{}--\r\n", boundary)
        .map_err(|e| format!("Failed to close multipart: {}", e))?;

    let content_type = format!("multipart/form-data; boundary={}", boundary);

    let agent = ureq::builder()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(120)) // Long audio can take time
        .build();

    let response = agent
        .post(&format!("http://127.0.0.1:{port}/inference"))
        .set("Content-Type", &content_type)
        .send_bytes(&body)
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let json: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    json["result"]
        .as_str()
        .map(|t| t.trim().to_string())
        .ok_or_else(|| "No 'result' field in server response".to_string())
}

/// Restart the Whisper-Server if it's running.
/// Used when the user changes the model in Settings.
pub fn restart_whisper_server_if_running(
    app: &AppHandle,
    state: &AppState,
    new_model_path: &Path,
) -> Result<(), String> {
    let port = state.whisper_server_port.load(std::sync::atomic::Ordering::Relaxed);

    if ping_whisper_server(port) {
        // Kill the current server
        if let Some(mut child) = state.managed_whisper_server_child.lock().unwrap().take() {
            let _ = child.kill();
        }
        std::thread::sleep(Duration::from_millis(500));

        // Start a new one
        start_whisper_server(app, state, new_model_path)?;
    }

    Ok(())
}

/// Kill the Whisper-Server process (called on app exit).
pub fn kill_whisper_server(state: &AppState) {
    if let Some(mut child) = state.managed_whisper_server_child.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

// ────────────────────────────────────────────────────────────────────────────────
// Helper functions for manual multipart encoding
// ────────────────────────────────────────────────────────────────────────────────

/// Write a form field with file content to multipart body.
fn write_multipart_field_file(
    body: &mut Vec<u8>,
    boundary: &str,
    field_name: &str,
    file_name: &str,
    content_type: &str,
    data: &[u8],
) -> std::io::Result<()> {
    write!(
        body,
        "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
        boundary, field_name, file_name, content_type
    )?;
    body.write_all(data)?;
    write!(body, "\r\n")?;
    Ok(())
}

/// Write a form field with text content to multipart body.
fn write_multipart_field_text(
    body: &mut Vec<u8>,
    boundary: &str,
    field_name: &str,
    value: &str,
) -> std::io::Result<()> {
    write!(
        body,
        "--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n",
        boundary, field_name, value
    )?;
    Ok(())
}

/// Get optimal thread count for Whisper (CPU cores - 1, clamped to [2, 12]).
fn optimal_thread_count() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (cores.saturating_sub(1)).max(2).min(12)
}
