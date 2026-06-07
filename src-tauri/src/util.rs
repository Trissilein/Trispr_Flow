use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;
use tracing::{error, info, warn};

use crate::state::AppState;

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tauri::command]
pub(crate) fn frontend_heartbeat(state: State<'_, AppState>) {
    state
        .frontend_last_heartbeat_ms
        .store(now_ms(), Ordering::Relaxed);
}

#[tauri::command]
pub(crate) fn log_frontend_event(
    level: String,
    context: String,
    message: String,
) -> Result<(), String> {
    let normalized_context = context.trim();
    let normalized_message = message.trim();
    if normalized_message.is_empty() {
        return Ok(());
    }
    match level.trim().to_ascii_lowercase().as_str() {
        "error" => error!(
            "[frontend:{}] {}",
            if normalized_context.is_empty() {
                "unknown"
            } else {
                normalized_context
            },
            normalized_message
        ),
        "warn" => warn!(
            "[frontend:{}] {}",
            if normalized_context.is_empty() {
                "unknown"
            } else {
                normalized_context
            },
            normalized_message
        ),
        _ => info!(
            "[frontend:{}] {}",
            if normalized_context.is_empty() {
                "unknown"
            } else {
                normalized_context
            },
            normalized_message
        ),
    }
    Ok(())
}

/// Spawn a thread wrapped in `catch_unwind` so that a panic inside `f` is
/// logged via tracing instead of silently killing the thread (and potentially
/// poisoning shared mutexes).
pub(crate) fn spawn_guarded<F>(label: &'static str, f: F) -> JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        if let Err(payload) = result {
            let msg = crate::format_panic_payload(&*payload);
            error!("Thread '{}' panicked: {}", label, msg);
        }
    })
}

/// Like `spawn_guarded` but with a return value. Returns `None` on panic.
#[allow(dead_code)]
pub(crate) fn spawn_guarded_with_result<F, T>(label: &'static str, f: F) -> JoinHandle<Option<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::spawn(
        move || match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(val) => Some(val),
            Err(payload) => {
                let msg = crate::format_panic_payload(&*payload);
                error!("Thread '{}' panicked: {}", label, msg);
                None
            }
        },
    )
}
