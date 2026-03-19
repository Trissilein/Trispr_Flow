use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
