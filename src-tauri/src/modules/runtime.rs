//! Generic launcher and lifecycle manager for module sidecars.
//!
//! A *module sidecar* is a self-contained executable shipped inside a
//! downloaded module package (`kind = "sidecar"` or `kind = "runtime"`) that
//! communicates with core over stdio or a localhost HTTP port.
//!
//! Trispr already uses this pattern for whisper-server, FFmpeg, Piper, and
//! Ollama. This module provides the generic harness so code-out modules
//! (e.g. the `opus` sidecar) don't have to reinvent the spawn/poll/terminate
//! cycle.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::AppHandle;

use crate::paths::resolve_modules_dir;
use crate::state::AppState;

/// Resolve the path to a file inside an installed module package.
/// `rel` is a relative subpath from the module root, e.g. `"bin/trispr-opus.exe"`.
pub fn resolve_module_binary(app: &AppHandle, module_id: &str, rel: &str) -> PathBuf {
    resolve_modules_dir(app).join(module_id).join(rel)
}

/// HTTP readiness-poll config for a sidecar that listens on a localhost port.
pub struct SidecarHttpHealth {
    pub port: u16,
    /// Number of GET attempts before the poll is declared a failure.
    pub max_attempts: u32,
    /// Connection + read timeout per attempt.
    pub timeout_ms: u64,
}

impl Default for SidecarHttpHealth {
    fn default() -> Self {
        SidecarHttpHealth {
            port: 0,
            max_attempts: 3,
            timeout_ms: 400,
        }
    }
}

/// Spawn a module sidecar and optionally wait for it to become ready.
///
/// If a sidecar for `module_id` is already tracked it is terminated first
/// (restart semantics, matching `ensure_whisper_server_running`).
/// `health` drives an HTTP readiness poll — set `port = 0` or pass `None` to skip.
/// Returns the OS PID on success.
pub fn spawn_module_sidecar(
    state: &AppState,
    module_id: &str,
    cmd: &mut std::process::Command,
    health: Option<&SidecarHttpHealth>,
) -> Result<u32, String> {
    // Terminate any existing instance first (restart semantics).
    terminate_module_sidecar(state, module_id);

    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn module sidecar '{module_id}': {e}"))?;
    let pid = child.id();

    {
        let mut map = state
            .module_sidecars
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.insert(module_id.to_string(), child);
    }

    if let Some(poll) = health {
        if poll.port > 0 {
            poll_http_ready(poll.port, poll.max_attempts, poll.timeout_ms).map_err(|e| {
                // Kill the process so we don't leave it dangling.
                terminate_module_sidecar(state, module_id);
                format!("Module sidecar '{module_id}' health check failed: {e}")
            })?;
        }
    }

    Ok(pid)
}

/// Stop a running module sidecar by id. Idempotent — no-op if not running.
pub fn terminate_module_sidecar(state: &AppState, module_id: &str) {
    let mut map = state
        .module_sidecars
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(mut child) = map.remove(module_id) {
        kill_and_reap(&mut child);
    }
}

/// Stop all running module sidecars. Called from `cleanup_managed_processes` on app exit.
pub fn terminate_all_module_sidecars(state: &AppState) {
    let mut map = state
        .module_sidecars
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for (_, mut child) in map.drain() {
        kill_and_reap(&mut child);
    }
}

fn kill_and_reap(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Poll `http://127.0.0.1:{port}/` until the server responds or attempts run out.
/// Any HTTP status code (even 4xx/5xx) counts as "server is up".
fn poll_http_ready(port: u16, max_attempts: u32, timeout_ms: u64) -> Result<(), String> {
    let timeout = Duration::from_millis(timeout_ms);
    let agent = ureq::builder()
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .build();
    let attempts = max_attempts.max(1);
    for attempt in 0..attempts {
        match agent.get(&format!("http://127.0.0.1:{port}/")).call() {
            Ok(_) | Err(ureq::Error::Status(_, _)) => return Ok(()),
            Err(_) => {
                if attempt + 1 < attempts {
                    std::thread::sleep(Duration::from_millis(120));
                }
            }
        }
    }
    Err(format!(
        "port {port} did not respond after {attempts} attempts"
    ))
}

/// Type alias so callers importing only this module don't need to spell out
/// the full `std::collections` path.
pub type ModuleSidecarMap = Mutex<HashMap<String, std::process::Child>>;

/// Construct the default (empty) sidecar map for `AppState` initialization.
pub fn default_sidecar_map() -> ModuleSidecarMap {
    Mutex::new(HashMap::new())
}
