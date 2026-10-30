//! Single owner of the paste outcome for every transcription job.
//!
//! Multiple asynchronous sources race to decide what gets pasted for a job:
//! the refinement worker (success/failure), the deadline timer, and the
//! bypass path in `finish_transcription`. Before this module the arbitration
//! lived in frontend JS timers, which Chromium throttles whenever the main
//! window is hidden — exactly the situation while the user dictates into
//! another application. The arbiter keeps the whole decision in Rust:
//! first `settle()` wins and pastes, every later call is a no-op.

use std::collections::HashMap;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::state::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PasteOutcome {
    /// Bypass or non-deferred path: raw transcript pasted immediately.
    Raw,
    /// Refinement finished before the deadline: refined text pasted.
    Refined,
    /// Refinement failed: raw transcript pasted as fallback.
    RawFallback,
    /// Deadline expired before refinement finished: raw transcript pasted.
    RawTimeout,
}

struct PendingJob {
    raw_text: String,
}

#[derive(Default)]
pub(crate) struct PasteArbiter {
    jobs: Mutex<HashMap<String, PendingJob>>,
    /// Serializes the actual clipboard+keystroke sequence so two settles
    /// (e.g. a timeout for job A and a bypass for job B) never interleave.
    paste_order: Mutex<()>,
}

impl PasteArbiter {
    /// Register a job's raw text before any settle source can fire.
    pub(crate) fn register(&self, job_id: &str, raw_text: String) {
        let mut jobs = self
            .jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        jobs.insert(job_id.to_string(), PendingJob { raw_text });
    }

    /// Atomically claim the job and paste. Returns `true` if this call won
    /// the race (and pasted), `false` if the job was already settled or never
    /// registered. `text_override` replaces the raw text (refined output).
    pub(crate) fn settle(
        &self,
        app_handle: &AppHandle,
        job_id: &str,
        outcome: PasteOutcome,
        text_override: Option<&str>,
    ) -> bool {
        let job = {
            let mut jobs = self
                .jobs
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            jobs.remove(job_id)
        };
        let Some(job) = job else {
            return false;
        };

        let text = text_override.unwrap_or(&job.raw_text);
        let paste_error = if text.trim().is_empty() {
            None
        } else {
            let _order = self
                .paste_order
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            crate::paste_text(app_handle, text).err()
        };

        if let Some(err) = &paste_error {
            warn!("[paste_arbiter:{job_id}] paste failed outcome={outcome:?}: {err}");
        } else {
            info!(
                "[paste_arbiter:{job_id}] settled outcome={outcome:?} bytes={}",
                text.len()
            );
        }
        let _ = app_handle.emit(
            "paste:settled",
            serde_json::json!({
                "job_id": job_id,
                "outcome": outcome,
                "text": text,
                "paste_error": paste_error,
            }),
        );
        true
    }
}

/// Spawn the deadline that guarantees a paste even if the refinement worker
/// hangs past every soft timeout. Lives in Rust so window visibility and
/// WebView timer throttling cannot delay it.
pub(crate) fn schedule_deadline(app_handle: AppHandle, job_id: String, timeout_ms: u64) {
    crate::util::spawn_guarded("paste_arbiter_deadline", move || {
        thread::sleep(Duration::from_millis(timeout_ms));
        let state = app_handle.state::<AppState>();
        let timed_out =
            state
                .paste_arbiter
                .settle(&app_handle, &job_id, PasteOutcome::RawTimeout, None);
        if timed_out {
            crate::state::record_refinement_fallback_timed_out(state.inner());
            warn!("[paste_arbiter:{job_id}] deadline hit after {timeout_ms}ms — raw pasted");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_without_register_is_noop() {
        let arbiter = PasteArbiter::default();
        let job = {
            let mut jobs = arbiter.jobs.lock().unwrap();
            jobs.remove("missing")
        };
        assert!(job.is_none());
    }

    #[test]
    fn first_claim_wins_second_is_noop() {
        let arbiter = PasteArbiter::default();
        arbiter.register("job-1", "raw text".to_string());
        let first = {
            let mut jobs = arbiter.jobs.lock().unwrap();
            jobs.remove("job-1")
        };
        let second = {
            let mut jobs = arbiter.jobs.lock().unwrap();
            jobs.remove("job-1")
        };
        assert!(first.is_some());
        assert!(second.is_none());
        assert_eq!(first.unwrap().raw_text, "raw text");
    }

    #[test]
    fn register_overwrites_previous_job_with_same_id() {
        let arbiter = PasteArbiter::default();
        arbiter.register("job-1", "old".to_string());
        arbiter.register("job-1", "new".to_string());
        let job = {
            let mut jobs = arbiter.jobs.lock().unwrap();
            jobs.remove("job-1")
        };
        assert_eq!(job.unwrap().raw_text, "new");
    }
}
