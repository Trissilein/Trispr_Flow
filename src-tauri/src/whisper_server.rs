//! Whisper-Server persistent process manager.
//! Keeps the Whisper model pre-loaded in GPU VRAM to eliminate per-transcription model-load overhead (~1.4s).

use crate::paths::resolve_whisper_server_path_for_backend;
use crate::spawn_managed_child;
use crate::state::{AppState, Settings};
use crate::terminate_managed_child_slot;
use crate::update_runtime_diagnostics;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tracing::{info, warn};

/// Default port for Whisper-Server HTTP API.
pub const WHISPER_SERVER_PORT: u16 = 8178;

/// Health check: ping the server's root endpoint.
///
/// Two-attempt probe: a fast first attempt covers the steady-state happy path
/// (~ms when healthy), and a longer retry absorbs transient post-decode pauses
/// where the server thread is briefly busy after a transcription completes.
pub fn ping_whisper_server(port: u16) -> bool {
    ping_whisper_server_with_attempts(port, 2)
}

fn ping_whisper_server_with_attempts(port: u16, attempts: u32) -> bool {
    info!("[ping] ping_whisper_server_with_attempts(port={}, attempts={})", port, attempts);
    for attempt in 0..attempts.max(1) {
        let timeout = if attempt == 0 {
            Duration::from_millis(400)
        } else {
            Duration::from_millis(1500)
        };
        info!("[ping] attempt {}/{}, timeout_ms={}", attempt + 1, attempts.max(1), timeout.as_millis());
        let agent = ureq::builder()
            .timeout_connect(timeout)
            .timeout_read(timeout)
            .build();
        let start = std::time::Instant::now();
        let result = agent
            .get(&format!("http://127.0.0.1:{port}/"))
            .call();
        let elapsed = start.elapsed();
        match result {
            Ok(_) => {
                info!("[ping] attempt {}/{} SUCCESS in {:.1}ms", attempt + 1, attempts.max(1), elapsed.as_secs_f64() * 1000.0);
                return true;
            }
            Err(e) => {
                info!("[ping] attempt {}/{} FAILED: {}", attempt + 1, attempts.max(1), e);
            }
        }
        if attempt + 1 < attempts {
            std::thread::sleep(Duration::from_millis(120));
        }
    }
    info!("[ping] ping_whisper_server_with_attempts(port={}) -> false", port);
    false
}

fn resolve_preferred_server_path(settings: &Settings) -> Option<PathBuf> {
    let preference = settings
        .local_backend_preference
        .trim()
        .to_ascii_lowercase();
    let resolved =
        resolve_whisper_server_path_for_backend(Some(settings.local_backend_preference.as_str()));
    if preference == "cuda" || preference == "vulkan" {
        if let Some(path) = resolved {
            let resolved_backend = crate::transcription::whisper_backend_from_cli_path(&path);
            if resolved_backend.eq_ignore_ascii_case(preference.as_str()) {
                return Some(path);
            }
            return None;
        }
        return None;
    }
    resolved
}

fn strict_backend_from_preference(settings: &Settings) -> Option<&'static str> {
    match settings
        .local_backend_preference
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "cuda" => Some("cuda"),
        "vulkan" => Some("vulkan"),
        _ => None,
    }
}

/// Start the Whisper-Server process and wait for it to be ready.
///
/// Returns Ok if the server is running (either just started or already running).
/// Returns Err if the binary is missing or startup failed after timeout.
pub fn start_whisper_server(
    app: &AppHandle,
    state: &AppState,
    model_path: &Path,
) -> Result<(), String> {
    info!("[whisper_server:startup] start_whisper_server(model_path={})", model_path.display());
    let port = state
        .whisper_server_port
        .load(std::sync::atomic::Ordering::Relaxed);
    let settings = state
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();

    let managed_child_slot = state
        .managed_whisper_server_child
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);
    info!("[whisper_server:startup] managed_whisper_server_child slot: is_some={}", managed_child_slot);

    // Already running?
    if ping_whisper_server(port) {
        info!("whisper-server already running on port {}", port);
        update_whisper_server_diagnostics(app, &settings, "server", "gpu", None);
        return Ok(());
    }

    let server_path = resolve_preferred_server_path(&settings).ok_or_else(|| {
        let message = if let Some(strict_backend) = strict_backend_from_preference(&settings) {
            format!(
                "whisper-server runtime for preferred backend '{}' not found; staying on CLI path.",
                strict_backend
            )
        } else {
            "whisper-server.exe not found (Phase 0 incomplete — binary not sourced)".to_string()
        };
        update_whisper_server_diagnostics(app, &settings, "cli", "cpu", Some(message.clone()));
        message
    })?;

    info!(
        "Starting whisper-server: {} -m {} --port {}",
        server_path.display(),
        model_path.display(),
        port
    );
    // Only invoke the kill path if a managed child is actually tracked. With
    // the more reliable two-attempt ping above, a ping=false here genuinely
    // means the process is gone or wedged; if no managed child is tracked,
    // there is nothing to terminate, just spawn fresh.
    let had_managed_child = state
        .managed_whisper_server_child
        .lock()
        .map(|guard| guard.is_some())
        .unwrap_or(false);
    info!("[whisper_server:startup] had_managed_child={}", had_managed_child);
    if had_managed_child {
        info!("[whisper_server:startup] terminating previous managed child");
        terminate_managed_child_slot(
            "managed Whisper-Server runtime",
            &state.managed_whisper_server_child,
        );
    }

    let mut cmd = std::process::Command::new(&server_path);
    cmd.arg("-m")
        .arg(model_path)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(port.to_string())
        .arg("-t")
        .arg(optimal_thread_count().to_string())
        .stdin(std::process::Stdio::null());

    // Capture stdout/stderr to a log file so we can see crashes / backend
    // errors. Append mode keeps history across restarts within a session.
    let logs_dir = crate::paths::resolve_base_dir(app).join("logs");
    let _ = std::fs::create_dir_all(&logs_dir);
    let log_path = logs_dir.join("whisper-server.stderr.log");
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => {
            let stderr = file.try_clone().unwrap_or_else(|_| {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .expect("re-open log file")
            });
            cmd.stdout(file).stderr(stderr);
            info!("[whisper_server:startup] stdout/stderr -> {}", log_path.display());
        }
        Err(e) => {
            warn!("[whisper_server:startup] could not open log file ({}), discarding stdout/stderr: {}", log_path.display(), e);
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
    }

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    info!("[whisper_server:startup] spawning whisper-server process");
    let spawn_result = spawn_managed_child(
        state,
        "managed Whisper-Server runtime",
        &state.managed_whisper_server_child,
        &mut cmd,
    )
    .map_err(|e| {
        let message = format!("Failed to spawn whisper-server ({})", e);
        info!("[whisper_server:startup] spawn FAILED: {}", message);
        update_whisper_server_diagnostics(app, &settings, "cli", "cpu", Some(message.clone()));
        message
    })?;
    info!("[whisper_server:startup] spawn SUCCESS (pid={}, job_assigned={})", spawn_result.pid, spawn_result.job_assigned);
    if !spawn_result.job_assigned {
        warn!(
            "whisper-server started without managed job assignment (pid {})",
            spawn_result.pid
        );
    }

    // Poll for server readiness (max 30 seconds, check every 250ms)
    // The large-v3-turbo model (~1.5 GB) can take 10-20s to load on first start.
    info!("[whisper_server:startup] polling for readiness (max 30s)");
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if ping_whisper_server(port) {
            info!("[whisper_server:startup] start_whisper_server() SUCCESS");
            info!("whisper-server ready on port {}", port);
            update_whisper_server_diagnostics(app, &settings, "server", "gpu", None);
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    // Timeout — log warning but don't fail. Fallback to CLI will be used.
    info!("[whisper_server:startup] start_whisper_server() TIMEOUT after 30s");
    warn!("whisper-server startup timeout (30s) — will fall back to CLI transcription for now");
    update_whisper_server_diagnostics(
        app,
        &settings,
        "cli",
        "cpu",
        Some("server unavailable, CLI active".to_string()),
    );
    Ok(())
}

/// Inspect the tail of whisper-server's captured stderr log for known crash
/// patterns. Returns a user-facing hint if a known fatal cause is recognised
/// (e.g. CUDA arch mismatch). Called after a transport error so the operator
/// sees an actionable message instead of a generic "connection closed".
pub fn inspect_recent_server_crash(app: &AppHandle) -> Option<String> {
    let log_path = crate::paths::resolve_base_dir(app)
        .join("logs")
        .join("whisper-server.stderr.log");
    let content = std::fs::read_to_string(&log_path).ok()?;
    let tail_start = content.len().saturating_sub(8 * 1024);
    let tail = &content[tail_start..];

    if tail.contains("no kernel image is available for execution on the device") {
        return Some(
            "GPU not supported by CUDA build. Switch backend to Vulkan in Settings.".to_string(),
        );
    }

    if tail.contains("out of memory") {
        return Some(
            "GPU out of memory. Try a smaller model or switch to Vulkan/CPU.".to_string(),
        );
    }

    if tail.contains("CUDA error") {
        return Some("CUDA error — see whisper-server.stderr.log.".to_string());
    }

    None
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
    write_multipart_field_file(
        &mut body,
        boundary,
        "file",
        "audio.wav",
        "audio/wav",
        wav_bytes,
    )
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

    info!("[server-request] POST /inference port={} wav_bytes={} body_bytes={} language={}",
        port, wav_bytes.len(), body.len(), language);
    let req_start = std::time::Instant::now();
    let response = agent
        .post(&format!("http://127.0.0.1:{port}/inference"))
        .set("Content-Type", &content_type)
        .send_bytes(&body)
        .map_err(|err| match err {
            ureq::Error::Status(code, response) => {
                let body = response.into_string().unwrap_or_default();
                let body = body.replace('\n', " ").replace('\r', " ");
                let body = body.trim();
                if body.is_empty() {
                    format!("whisper-server HTTP {} (port {})", code, port)
                } else {
                    format!("whisper-server HTTP {} (port {}): {}", code, port, body)
                }
            }
            ureq::Error::Transport(transport) => {
                format!("whisper-server transport error (port {}): {}", port, transport)
            }
        })
        .inspect_err(|e| {
            warn!("[server-request] FAILED after {:.2}s: {}", req_start.elapsed().as_secs_f64(), e);
        })?;
    info!("[server-request] response received in {:.2}s", req_start.elapsed().as_secs_f64());

    let json: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("Failed to parse JSON response: {}", e))?;

    json.get("result")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("text").and_then(|v| v.as_str()))
        .or_else(|| json.get("transcript").and_then(|v| v.as_str()))
        .map(|t| t.trim().to_string())
        .ok_or_else(|| {
            format!(
                "No transcript field in server response (expected result/text/transcript): {}",
                json
            )
        })
}

/// Restart the Whisper-Server if it's running.
/// Used when the user changes the model in Settings.
pub fn restart_whisper_server_if_running(
    app: &AppHandle,
    state: &AppState,
    new_model_path: &Path,
) -> Result<(), String> {
    let port = state
        .whisper_server_port
        .load(std::sync::atomic::Ordering::Relaxed);

    if ping_whisper_server(port) {
        state
            .whisper_server_warmup_started
            .store(false, Ordering::Relaxed);
        terminate_managed_child_slot(
            "managed Whisper-Server runtime",
            &state.managed_whisper_server_child,
        );
        std::thread::sleep(Duration::from_millis(500));

        // Start a new one
        start_whisper_server(app, state, new_model_path)?;
    }

    Ok(())
}

/// Kill the Whisper-Server process (called on app exit).
pub fn kill_whisper_server(state: &AppState) {
    state
        .whisper_server_warmup_started
        .store(false, Ordering::Relaxed);
    terminate_managed_child_slot(
        "managed Whisper-Server runtime",
        &state.managed_whisper_server_child,
    );
}

pub fn schedule_whisper_server_warmup(
    app: &AppHandle,
    state: &AppState,
    model_path: &Path,
    settings: &Settings,
) {
    let port = state.whisper_server_port.load(Ordering::Relaxed);
    if ping_whisper_server(port) {
        return;
    }
    if state
        .whisper_server_warmup_started
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }

    let handle = app.clone();
    let model_path = model_path.to_path_buf();
    let settings_snapshot = settings.clone();
    crate::util::spawn_guarded("whisper_server_warmup", move || {
        let state = handle.state::<AppState>();
        match start_whisper_server(&handle, state.inner(), &model_path) {
            Ok(()) => {
                if ping_whisper_server(port) {
                    info!("whisper-server warmup complete");
                } else {
                    warn!("whisper-server warmup finished without healthy server; CLI remains primary");
                }
            }
            Err(err) => {
                warn!("whisper-server warmup failed: {}", err);
                update_whisper_server_diagnostics(
                    &handle,
                    &settings_snapshot,
                    "cli",
                    "cpu",
                    Some(err),
                );
            }
        }
    });
}

fn update_whisper_server_diagnostics(
    app: &AppHandle,
    settings: &Settings,
    mode: &str,
    accelerator: &str,
    last_error: Option<String>,
) {
    let server_path = resolve_preferred_server_path(settings);
    let backend = server_path
        .as_deref()
        .map(crate::transcription::whisper_backend_from_cli_path)
        .map(|value| value.to_string())
        .or_else(|| strict_backend_from_preference(settings).map(|value| value.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let state = app.state::<AppState>();
    update_runtime_diagnostics(app, state.inner(), |diagnostics| {
        diagnostics.whisper.server_path = server_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        diagnostics.whisper.backend_selected = backend.clone();
        diagnostics.whisper.mode = mode.to_string();
        diagnostics.whisper.accelerator = accelerator.to_string();
        diagnostics.whisper.last_error = last_error.unwrap_or_default();
    });
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

#[cfg(test)]
mod tests {
    use super::strict_backend_from_preference;
    use crate::state::Settings;

    #[test]
    fn strict_backend_preference_only_for_explicit_gpu_backends() {
        let mut settings = Settings::default();

        settings.local_backend_preference = "cuda".to_string();
        assert_eq!(strict_backend_from_preference(&settings), Some("cuda"));

        settings.local_backend_preference = "vulkan".to_string();
        assert_eq!(strict_backend_from_preference(&settings), Some("vulkan"));

        settings.local_backend_preference = "auto".to_string();
        assert_eq!(strict_backend_from_preference(&settings), None);
    }
}
