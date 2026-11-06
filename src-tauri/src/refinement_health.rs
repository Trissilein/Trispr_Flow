//! Self-heal for dictation refinement.
//!
//! Refinement silently degrades to raw text whenever Ollama no longer holds the model (server
//! keep-alive, restart, another client, `purge_gpu_memory`). The cached `ollama_model_warm` flag
//! then keeps claiming the model is warm, so the PTT pre-warm is skipped and every following
//! dictation is raw until the one-hour idle release. This module
//!   * reconciles the flags with Ollama's `/api/ps` at the moment a dictation finds the model gone,
//!   * counts consecutive refinement failures and climbs a recovery ladder
//!     (force warm-up -> verify/start runtime -> restart our own managed runtime),
//!   * announces one episode per recovery as the `refinement:health` event.
//!
//! The decisions (`evaluate_gate`, `record_failure`, `decide_recovery`, `finish_action`) are pure
//! and unit-tested; the functions that need an `AppHandle` only execute them.

use std::sync::MutexGuard;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::ai_fallback::provider::OllamaModelLoadStatus;
use crate::state::AppState;

/// Reason codes shared by the `refinement:health` event, the gate decision and the history
/// entry's `refinement_skip_reason`. The frontend maps them to readable texts.
pub(crate) mod reason {
    pub(crate) const DISABLED: &str = "disabled";
    pub(crate) const MODEL_NOT_LOADED: &str = "model_not_loaded";
    pub(crate) const PS_QUERY_FAILED: &str = "ps_query_failed";
    pub(crate) const GPU_BUSY: &str = "gpu_busy";
    pub(crate) const COLD_START_RAW_FIRST: &str = "cold_start_raw_first";
    pub(crate) const RAW_TIMEOUT: &str = "raw_timeout";
    pub(crate) const QUEUE_FULL: &str = "queue_full";
    pub(crate) const PREPARE_FAILED: &str = "prepare_failed";
    pub(crate) const PROVIDER_ERROR: &str = "provider_error";
}

pub(crate) struct RecoveryConfig {
    /// Failures inside `window_ms` that start the ladder.
    pub(crate) threshold: u32,
    pub(crate) window_ms: u64,
    /// Minimum gap between two recovery actions.
    pub(crate) cooldown_ms: u64,
    /// At most one managed-runtime restart per this span.
    pub(crate) restart_limit_ms: u64,
    /// Minimum gap between two warm-ups started by the gate reconcile.
    pub(crate) gate_warmup_gap_ms: u64,
}

pub(crate) const RECOVERY_CFG: RecoveryConfig = RecoveryConfig {
    threshold: 3,
    window_ms: 10 * 60_000,
    cooldown_ms: 60_000,
    restart_limit_ms: 10 * 60_000,
    gate_warmup_gap_ms: 30_000,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    None,
    ForceWarmup,
    VerifyRuntime,
    RestartManaged,
}

/// Counters of the current failure streak plus the state of the open recovery episode. Reset by
/// `on_success`, except `last_restart_ms`, which is a hard rate limit and survives a success.
#[derive(Debug, Default)]
pub(crate) struct RefinementHealth {
    failures: u32,
    window_start_ms: u64,
    /// Recovery actions executed since the last success (0 none, 1 warm-up, 2 verify, 3 restart).
    stage: u8,
    last_action_ms: u64,
    last_restart_ms: u64,
    action_running: bool,
    /// Ollama is our own live child process (never true for an external server).
    managed_runtime: bool,
    episode_open: bool,
    attempts: u32,
    last_reason: &'static str,
    last_gate_warmup_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FailureOutcome {
    pub(crate) action: RecoveryAction,
    /// The action opens a new episode: announce `recovering`.
    pub(crate) new_episode: bool,
    pub(crate) attempt: u32,
}

fn restart_allowed(h: &RefinementHealth, now_ms: u64, cfg: &RecoveryConfig) -> bool {
    h.last_restart_ms == 0 || now_ms.saturating_sub(h.last_restart_ms) >= cfg.restart_limit_ms
}

/// Which action, if any, the current counters call for. Pure.
pub(crate) fn decide_recovery(
    h: &RefinementHealth,
    now_ms: u64,
    cfg: &RecoveryConfig,
) -> RecoveryAction {
    if h.action_running || h.failures < cfg.threshold {
        return RecoveryAction::None;
    }
    if now_ms.saturating_sub(h.window_start_ms) > cfg.window_ms {
        return RecoveryAction::None;
    }
    if h.last_action_ms != 0 && now_ms.saturating_sub(h.last_action_ms) < cfg.cooldown_ms {
        return RecoveryAction::None;
    }
    match h.stage {
        0 => RecoveryAction::ForceWarmup,
        1 => RecoveryAction::VerifyRuntime,
        2 if h.managed_runtime && restart_allowed(h, now_ms, cfg) => RecoveryAction::RestartManaged,
        _ => RecoveryAction::None,
    }
}

/// Counts one failure and, when the ladder calls for it, commits the next action (so a
/// concurrent failure cannot trigger it twice).
pub(crate) fn record_failure(
    h: &mut RefinementHealth,
    now_ms: u64,
    managed: bool,
    failure_reason: &'static str,
    cfg: &RecoveryConfig,
) -> FailureOutcome {
    h.managed_runtime = managed;
    h.last_reason = failure_reason;
    if h.failures == 0 || now_ms.saturating_sub(h.window_start_ms) > cfg.window_ms {
        h.failures = 0;
        h.window_start_ms = now_ms;
    }
    h.failures += 1;

    let action = decide_recovery(h, now_ms, cfg);
    if action == RecoveryAction::None {
        return FailureOutcome {
            action,
            new_episode: false,
            attempt: h.attempts,
        };
    }
    h.stage += 1;
    h.attempts += 1;
    h.last_action_ms = now_ms;
    h.action_running = true;
    if action == RecoveryAction::RestartManaged {
        h.last_restart_ms = now_ms;
    }
    let new_episode = !h.episode_open;
    h.episode_open = true;
    FailureOutcome {
        action,
        new_episode,
        attempt: h.attempts,
    }
}

/// A refinement succeeded: the streak is over. Returns `(reason, attempts)` of the episode that
/// was still open, so the caller can announce `recovered`.
pub(crate) fn on_success(h: &mut RefinementHealth) -> Option<(&'static str, u32)> {
    let open = h.episode_open.then_some((h.last_reason, h.attempts));
    h.failures = 0;
    h.window_start_ms = 0;
    h.stage = 0;
    h.last_action_ms = 0;
    h.episode_open = false;
    h.attempts = 0;
    open
}

/// Whether another ladder step can still follow once `stage` actions have run.
fn next_step_possible(h: &RefinementHealth, now_ms: u64, cfg: &RecoveryConfig) -> bool {
    match h.stage {
        0 | 1 => true,
        2 => h.managed_runtime && restart_allowed(h, now_ms, cfg),
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthState {
    Recovering,
    Recovered,
    Failed,
}

impl HealthState {
    fn as_str(self) -> &'static str {
        match self {
            HealthState::Recovering => "recovering",
            HealthState::Recovered => "recovered",
            HealthState::Failed => "failed",
        }
    }
}

/// An action ended. A healthy model closes the episode as `recovered`; an unhealthy one keeps it
/// open while a further step is possible and closes it as `failed` once the ladder is spent.
pub(crate) fn finish_action(
    h: &mut RefinementHealth,
    now_ms: u64,
    healthy: bool,
    cfg: &RecoveryConfig,
) -> Option<HealthState> {
    h.action_running = false;
    // A real success that landed meanwhile already closed (and announced) the episode.
    if !h.episode_open {
        return None;
    }
    if healthy {
        h.episode_open = false;
        Some(HealthState::Recovered)
    } else if !next_step_possible(h, now_ms, cfg) {
        h.episode_open = false;
        Some(HealthState::Failed)
    } else {
        None
    }
}

impl RefinementHealth {
    /// A warm-up started by the gate reconcile is due when none ran within `gate_warmup_gap_ms`.
    /// Claims the slot when it is.
    pub(crate) fn claim_gate_warmup(&mut self, now_ms: u64, cfg: &RecoveryConfig) -> bool {
        if self.last_gate_warmup_ms != 0
            && now_ms.saturating_sub(self.last_gate_warmup_ms) < cfg.gate_warmup_gap_ms
        {
            return false;
        }
        self.last_gate_warmup_ms = now_ms;
        true
    }
}

/// What the just-in-time gate learned from Ollama for one dictation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GateEval {
    pub(crate) should_refine: bool,
    /// Why the dictation is not refined inline; `None` when `should_refine`.
    pub(crate) skip_reason: Option<&'static str>,
    pub(crate) ps_ok: bool,
    pub(crate) loaded: bool,
    pub(crate) vram_bytes: u64,
    pub(crate) gpu_busy: bool,
    /// Refinement is on but the model is not loaded: the cached warm flag must be corrected.
    pub(crate) reconcile: bool,
}

/// `status` is Ollama's `/api/ps` answer for the configured model (`None` = query failed). It is
/// not called when refinement is off.
pub(crate) fn evaluate_gate(
    enabled: bool,
    gpu_busy: bool,
    status: impl FnOnce() -> Option<OllamaModelLoadStatus>,
) -> GateEval {
    if !enabled {
        return GateEval {
            should_refine: false,
            skip_reason: Some(reason::DISABLED),
            ps_ok: true,
            loaded: false,
            vram_bytes: 0,
            gpu_busy,
            reconcile: false,
        };
    }
    let status = status();
    let ps_ok = status.is_some();
    let loaded = status.is_some_and(|s| s.loaded);
    let vram_bytes = status.map_or(0, |s| s.size_vram);
    let skip_reason = if gpu_busy {
        Some(reason::GPU_BUSY)
    } else if !loaded {
        Some(if ps_ok {
            reason::MODEL_NOT_LOADED
        } else {
            reason::PS_QUERY_FAILED
        })
    } else {
        None
    };
    GateEval {
        should_refine: loaded && !gpu_busy,
        skip_reason,
        ps_ok,
        loaded,
        vram_bytes,
        gpu_busy,
        reconcile: !loaded,
    }
}

/// The reason to count and announce for a model that is not loaded, whatever the GPU is doing.
pub(crate) fn missing_model_reason(ps_ok: bool) -> &'static str {
    if ps_ok {
        reason::MODEL_NOT_LOADED
    } else {
        reason::PS_QUERY_FAILED
    }
}

// ── event ────────────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct RefinementHealthEvent<'a> {
    state: &'a str,
    reason: &'a str,
    message: &'a str,
    attempt: u32,
    ts_ms: u64,
}

fn health_message(state: HealthState, failure_reason: &str) -> &'static str {
    match (state, failure_reason) {
        (HealthState::Recovered, _) => "Refinement restored.",
        (HealthState::Failed, _) => "Refinement unavailable. Dictation continues as raw text.",
        (HealthState::Recovering, reason::PS_QUERY_FAILED) => {
            "Refinement paused: Ollama is not answering. Checking it now."
        }
        (HealthState::Recovering, reason::RAW_TIMEOUT | reason::PROVIDER_ERROR) => {
            "Refinement paused: Ollama is responding too slowly. Recovering."
        }
        (HealthState::Recovering, reason::PREPARE_FAILED) => {
            "Refinement paused: setup failed. Checking the runtime."
        }
        (HealthState::Recovering, _) => "Refinement paused: model not loaded. Reloading it now.",
    }
}

fn emit_health(app: &AppHandle, state: HealthState, failure_reason: &str, attempt: u32) {
    let message = health_message(state, failure_reason);
    info!(
        "[selfheal] event state={} reason={} attempt={}",
        state.as_str(),
        failure_reason,
        attempt
    );
    let _ = app.emit(
        "refinement:health",
        RefinementHealthEvent {
            state: state.as_str(),
            reason: failure_reason,
            message,
            attempt,
            ts_ms: crate::util::now_ms(),
        },
    );
}

/// A refinement succeeded while an episode was still open: announce `recovered`.
pub(crate) fn announce_recovered_after_success(app: &AppHandle, episode: (&str, u32)) {
    emit_health(app, HealthState::Recovered, episode.0, episode.1);
}

// ── execution ────────────────────────────────────────────────────────────────────────────────

fn health_lock(state: &AppState) -> MutexGuard<'_, RefinementHealth> {
    state
        .refinement_health
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Counts a refinement failure and, when the ladder calls for it, runs the next recovery step on
/// its own thread. Never blocks the caller (paste path, deadline timer, refinement worker).
pub(crate) fn note_failure(app: &AppHandle, failure_reason: &'static str) -> RecoveryAction {
    let state = app.state::<AppState>();
    // Read outside the health lock: it takes the child-slot lock.
    let managed = crate::managed_child_slot_status(&state.managed_ollama_child).1;
    let (outcome, failures, stage) = {
        let mut h = health_lock(&state);
        let outcome = record_failure(
            &mut h,
            crate::util::now_ms(),
            managed,
            failure_reason,
            &RECOVERY_CFG,
        );
        (outcome, h.failures, h.stage)
    };
    info!(
        "[selfheal] failure reason={} failures={} stage={} managed={} action={:?}",
        failure_reason, failures, stage, managed, outcome.action
    );
    if outcome.action != RecoveryAction::None {
        let app = app.clone();
        crate::util::spawn_guarded("refinement_selfheal", move || {
            run_recovery(&app, outcome, failure_reason)
        });
    }
    outcome.action
}

/// The model is not loaded although refinement is on. Corrects the cached warm flag, tells the UI,
/// counts the miss and loads the model again, all without waiting: the dictation is pasted raw
/// by the caller. While the GPU is busy nothing is counted or started.
pub(crate) fn on_model_missing(app: &AppHandle, gate: &GateEval) {
    let state = app.state::<AppState>();
    let missing_reason = missing_model_reason(gate.ps_ok);
    if !gate.ps_ok {
        warn!("[selfheal] /api/ps query failed, treating the model as not loaded");
    }
    // A warm-up already running will set the flags itself when it ends.
    if state
        .ollama_warmup_in_progress
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        return;
    }
    if state
        .ollama_model_warm
        .load(std::sync::atomic::Ordering::SeqCst)
    {
        info!("[selfheal] warm flag was stale: model is not loaded in Ollama, resetting it");
    }
    crate::audio::mark_ollama_model_cold(app);
    if gate.gpu_busy {
        return;
    }
    if note_failure(app, missing_reason) != RecoveryAction::None {
        return; // the recovery step warms the model itself
    }
    let due = health_lock(&state).claim_gate_warmup(crate::util::now_ms(), &RECOVERY_CFG);
    if due {
        info!("[selfheal] reloading the model after a missed dictation");
        crate::audio::spawn_ollama_warmup(app, "gate");
    }
}

fn model_is_loaded(app: &AppHandle) -> bool {
    let settings = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    crate::ai_fallback::provider::fetch_ollama_model_load_status(
        &settings.providers.ollama.endpoint,
        &settings.ai_fallback.model,
    )
    .is_some_and(|status| status.loaded)
}

/// Resets the cached flags and loads the model again; healthy when Ollama then reports it loaded.
fn force_warmup(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    state
        .ollama_model_warm
        .store(false, std::sync::atomic::Ordering::SeqCst);
    state
        .ollama_warmup_in_progress
        .store(false, std::sync::atomic::Ordering::SeqCst);
    match crate::audio::run_ollama_warmup(app, "selfheal") {
        Ok(()) => model_is_loaded(app),
        Err(error) => {
            warn!("[selfheal] warm-up failed: {error}");
            false
        }
    }
}

/// Starts the runtime when nothing answers (the autostart path never touches a server that does),
/// refreshing the startup status on the way.
fn ensure_runtime(app: &AppHandle) -> Result<(), String> {
    let settings = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    crate::ai_fallback::ensure_ollama_runtime_ready_for_refinement(app, &settings)
}

fn verify_runtime(app: &AppHandle) -> bool {
    if let Err(error) = tauri::async_runtime::block_on(
        crate::ai_fallback::commands::verify_ollama_runtime(app.clone()),
    ) {
        warn!("[selfheal] runtime verify failed: {error}");
        if let Err(error) = ensure_runtime(app) {
            warn!("[selfheal] runtime start failed: {error}");
            return false;
        }
    }
    force_warmup(app)
}

/// Stops and restarts the Ollama child process this app spawned. Refuses anything else.
fn restart_managed(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    if !crate::managed_child_slot_status(&state.managed_ollama_child).1 {
        warn!("[selfheal] restart skipped: no managed Ollama process is alive");
        return false;
    }
    crate::terminate_managed_child_slot("managed Ollama runtime", &state.managed_ollama_child);
    crate::ollama_runtime::clear_ollama_pid_lockfile(app);
    crate::ai_fallback::provider::reset_quick_ping_cache();
    crate::update_startup_status(app, state.inner(), |status| {
        status.ollama_ready = false;
        status.ollama_starting = false;
    });
    crate::audio::mark_ollama_model_cold(app);
    if let Err(error) = ensure_runtime(app) {
        warn!("[selfheal] runtime restart failed: {error}");
        return false;
    }
    force_warmup(app)
}

fn run_recovery(app: &AppHandle, outcome: FailureOutcome, failure_reason: &'static str) {
    if outcome.new_episode {
        emit_health(
            app,
            HealthState::Recovering,
            failure_reason,
            outcome.attempt,
        );
    }
    info!(
        "[selfheal] step start action={:?} attempt={} reason={}",
        outcome.action, outcome.attempt, failure_reason
    );
    let healthy = match outcome.action {
        RecoveryAction::ForceWarmup => force_warmup(app),
        RecoveryAction::VerifyRuntime => verify_runtime(app),
        RecoveryAction::RestartManaged => restart_managed(app),
        RecoveryAction::None => true,
    };
    let state = app.state::<AppState>();
    let terminal = finish_action(
        &mut health_lock(&state),
        crate::util::now_ms(),
        healthy,
        &RECOVERY_CFG,
    );
    info!(
        "[selfheal] step end action={:?} healthy={} terminal={:?}",
        outcome.action, healthy, terminal
    );
    if let Some(terminal) = terminal {
        emit_health(app, terminal, failure_reason, outcome.attempt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CFG: RecoveryConfig = RECOVERY_CFG;
    const T0: u64 = 1_000_000;

    fn fail(h: &mut RefinementHealth, at: u64) -> FailureOutcome {
        record_failure(h, at, true, reason::MODEL_NOT_LOADED, &CFG)
    }

    fn fail_external(h: &mut RefinementHealth, at: u64) -> FailureOutcome {
        record_failure(h, at, false, reason::MODEL_NOT_LOADED, &CFG)
    }

    fn loaded(loaded: bool) -> Option<OllamaModelLoadStatus> {
        Some(OllamaModelLoadStatus {
            loaded,
            size_vram: if loaded { 4096 } else { 0 },
        })
    }

    // ── decide_recovery / record_failure ─────────────────────────────────────────────────────

    #[test]
    fn two_failures_do_nothing_third_forces_a_warmup() {
        let mut h = RefinementHealth::default();
        assert_eq!(fail(&mut h, T0).action, RecoveryAction::None);
        assert_eq!(fail(&mut h, T0 + 5_000).action, RecoveryAction::None);
        let third = fail(&mut h, T0 + 10_000);
        assert_eq!(third.action, RecoveryAction::ForceWarmup);
        assert!(third.new_episode);
        assert_eq!(third.attempt, 1);
    }

    #[test]
    fn failures_spread_beyond_the_window_never_reach_the_threshold() {
        let mut h = RefinementHealth::default();
        let minute = 60_000;
        for i in 0..6 {
            // one failure every 6 minutes: never 3 inside 10 minutes
            assert_eq!(
                fail(&mut h, T0 + i * 6 * minute).action,
                RecoveryAction::None
            );
        }
    }

    #[test]
    fn cooldown_blocks_the_next_step_until_it_has_passed() {
        let mut h = RefinementHealth::default();
        for i in 0..3 {
            fail(&mut h, T0 + i * 1_000);
        }
        // action ends, model still failing
        assert_eq!(finish_action(&mut h, T0 + 3_000, false, &CFG), None);
        assert_eq!(fail(&mut h, T0 + 20_000).action, RecoveryAction::None);
        assert_eq!(fail(&mut h, T0 + 61_000).action, RecoveryAction::None); // 59 s after the action at T0+2 s
        assert_eq!(
            fail(&mut h, T0 + 63_000).action,
            RecoveryAction::VerifyRuntime
        );
    }

    #[test]
    fn a_running_action_blocks_further_actions() {
        let mut h = RefinementHealth::default();
        for i in 0..3 {
            fail(&mut h, T0 + i * 1_000);
        }
        assert!(h.action_running);
        assert_eq!(fail(&mut h, T0 + 500_000).action, RecoveryAction::None);
    }

    /// Runs the ladder to the restart with a managed runtime; returns the time of the restart.
    fn climb_to_restart(h: &mut RefinementHealth, start: u64) -> u64 {
        let mut now = start;
        for _ in 0..3 {
            fail(h, now);
            now += 1_000;
        }
        assert_eq!(finish_action(h, now, false, &CFG), None);
        now += 61_000;
        assert_eq!(fail(h, now).action, RecoveryAction::VerifyRuntime);
        assert_eq!(finish_action(h, now + 1_000, false, &CFG), None);
        now += 62_000;
        assert_eq!(fail(h, now).action, RecoveryAction::RestartManaged);
        now
    }

    #[test]
    fn ladder_climbs_warmup_verify_restart_for_a_managed_runtime() {
        let mut h = RefinementHealth::default();
        let restart_at = climb_to_restart(&mut h, T0);
        assert_eq!(h.stage, 3);
        assert_eq!(h.last_restart_ms, restart_at);
        // ladder spent: the restart ended unhealthy -> failed, and nothing follows
        assert_eq!(
            finish_action(&mut h, restart_at + 1_000, false, &CFG),
            Some(HealthState::Failed)
        );
        assert_eq!(
            fail(&mut h, restart_at + 200_000).action,
            RecoveryAction::None
        );
    }

    #[test]
    fn an_external_server_is_never_restarted() {
        let mut h = RefinementHealth::default();
        let mut now = T0;
        for _ in 0..3 {
            fail_external(&mut h, now);
            now += 1_000;
        }
        finish_action(&mut h, now, false, &CFG);
        now += 61_000;
        assert_eq!(
            fail_external(&mut h, now).action,
            RecoveryAction::VerifyRuntime
        );
        // verify was the last possible step for an external server: the episode fails
        assert_eq!(
            finish_action(&mut h, now + 1_000, false, &CFG),
            Some(HealthState::Failed)
        );
        for step in 1..=20u64 {
            assert_eq!(
                fail_external(&mut h, now + step * 120_000).action,
                RecoveryAction::None,
                "step {step}"
            );
        }
    }

    #[test]
    fn restart_is_limited_to_one_per_ten_minutes_even_across_a_success() {
        let mut h = RefinementHealth::default();
        let restart_at = climb_to_restart(&mut h, T0);
        finish_action(&mut h, restart_at + 1_000, false, &CFG);
        on_success(&mut h); // a success resets the ladder but not the restart limit

        // a second streak reaches stage 2 again within 10 minutes of the restart
        let mut now = restart_at + 5_000;
        for _ in 0..3 {
            fail(&mut h, now);
            now += 1_000;
        }
        finish_action(&mut h, now, false, &CFG);
        now += 61_000;
        assert_eq!(fail(&mut h, now).action, RecoveryAction::VerifyRuntime);
        // restart not allowed yet: the verify step is the last one and the episode fails
        assert_eq!(
            finish_action(&mut h, now + 1_000, false, &CFG),
            Some(HealthState::Failed)
        );
        assert!(now + 62_000 < restart_at + CFG.restart_limit_ms);
        assert_eq!(fail(&mut h, now + 62_000).action, RecoveryAction::None);

        // once the limit has passed the same ladder may restart again
        let later = restart_at + CFG.restart_limit_ms + 100_000;
        assert_eq!(fail(&mut h, later).action, RecoveryAction::None);
        assert_eq!(fail(&mut h, later + 1_000).action, RecoveryAction::None);
        assert_eq!(
            fail(&mut h, later + 2_000).action,
            RecoveryAction::RestartManaged
        );
    }

    #[test]
    fn success_resets_the_ladder() {
        let mut h = RefinementHealth::default();
        for i in 0..3 {
            fail(&mut h, T0 + i * 1_000);
        }
        finish_action(&mut h, T0 + 4_000, false, &CFG);
        on_success(&mut h);
        assert_eq!((h.failures, h.stage, h.last_action_ms), (0, 0, 0));
        // back to square one: three failures start with the warm-up again
        assert_eq!(fail(&mut h, T0 + 100_000).action, RecoveryAction::None);
        assert_eq!(fail(&mut h, T0 + 101_000).action, RecoveryAction::None);
        assert_eq!(
            fail(&mut h, T0 + 102_000).action,
            RecoveryAction::ForceWarmup
        );
    }

    // ── episodes / events ────────────────────────────────────────────────────────────────────

    #[test]
    fn one_recovering_per_episode_then_recovered_closes_it() {
        let mut h = RefinementHealth::default();
        let mut now = T0;
        let mut recovering = 0;
        for _ in 0..3 {
            if fail(&mut h, now).new_episode {
                recovering += 1;
            }
            now += 1_000;
        }
        assert_eq!(recovering, 1);
        // warm-up did not help, the next step belongs to the same episode
        assert_eq!(finish_action(&mut h, now, false, &CFG), None);
        now += 61_000;
        let second = fail(&mut h, now);
        assert_eq!(second.action, RecoveryAction::VerifyRuntime);
        assert!(!second.new_episode, "same episode must not announce again");
        assert_eq!(second.attempt, 2);
        assert_eq!(
            finish_action(&mut h, now + 1_000, true, &CFG),
            Some(HealthState::Recovered)
        );
        assert!(!h.episode_open);
    }

    #[test]
    fn a_success_during_an_open_episode_reports_it_once() {
        let mut h = RefinementHealth::default();
        for i in 0..3 {
            fail(&mut h, T0 + i * 1_000);
        }
        finish_action(&mut h, T0 + 4_000, false, &CFG);
        assert_eq!(on_success(&mut h), Some((reason::MODEL_NOT_LOADED, 1)));
        assert_eq!(on_success(&mut h), None);
    }

    #[test]
    fn an_episode_closed_by_a_real_success_is_not_announced_twice() {
        let mut h = RefinementHealth::default();
        for i in 0..3 {
            fail(&mut h, T0 + i * 1_000);
        }
        // the refinement succeeds while the warm-up action is still running
        assert_eq!(on_success(&mut h), Some((reason::MODEL_NOT_LOADED, 1)));
        assert_eq!(finish_action(&mut h, T0 + 4_000, true, &CFG), None);
        assert!(!h.action_running);
    }

    #[test]
    fn a_healthy_first_step_closes_the_episode_as_recovered() {
        let mut h = RefinementHealth::default();
        for i in 0..3 {
            fail(&mut h, T0 + i * 1_000);
        }
        assert_eq!(
            finish_action(&mut h, T0 + 4_000, true, &CFG),
            Some(HealthState::Recovered)
        );
        assert!(!h.action_running);
    }

    #[test]
    fn health_event_payload_matches_the_contract() {
        let json = serde_json::to_value(RefinementHealthEvent {
            state: HealthState::Recovering.as_str(),
            reason: reason::MODEL_NOT_LOADED,
            message: health_message(HealthState::Recovering, reason::MODEL_NOT_LOADED),
            attempt: 1,
            ts_ms: 42,
        })
        .unwrap();
        let object = json.as_object().unwrap();
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, ["attempt", "message", "reason", "state", "ts_ms"]);
        assert_eq!(object["state"], "recovering");
        assert_eq!(object["attempt"], 1);
        assert!(object["message"]
            .as_str()
            .unwrap()
            .starts_with("Refinement paused"));
        assert_eq!(HealthState::Recovered.as_str(), "recovered");
        assert_eq!(HealthState::Failed.as_str(), "failed");
    }

    // ── gate ─────────────────────────────────────────────────────────────────────────────────

    #[test]
    fn gate_refines_only_when_ollama_reports_the_model_loaded() {
        let gate = evaluate_gate(true, false, || loaded(true));
        assert!(gate.should_refine);
        assert_eq!(gate.skip_reason, None);
        assert!(gate.ps_ok && gate.loaded && !gate.reconcile);
        assert_eq!(gate.vram_bytes, 4096);
    }

    #[test]
    fn gate_reconciles_a_model_ollama_no_longer_holds() {
        let gate = evaluate_gate(true, false, || loaded(false));
        assert!(!gate.should_refine);
        assert_eq!(gate.skip_reason, Some(reason::MODEL_NOT_LOADED));
        assert!(gate.ps_ok && !gate.loaded);
        assert!(gate.reconcile, "the stale warm flag must be corrected");
    }

    #[test]
    fn gate_reports_a_failed_status_query_instead_of_hiding_it() {
        let gate = evaluate_gate(true, false, || None);
        assert!(!gate.should_refine);
        assert_eq!(gate.skip_reason, Some(reason::PS_QUERY_FAILED));
        assert!(!gate.ps_ok);
        assert!(gate.reconcile);
    }

    #[test]
    fn gate_busy_gpu_keeps_its_reason_but_still_reconciles_a_missing_model() {
        let busy_loaded = evaluate_gate(true, true, || loaded(true));
        assert!(!busy_loaded.should_refine);
        assert_eq!(busy_loaded.skip_reason, Some(reason::GPU_BUSY));
        assert!(!busy_loaded.reconcile);

        let busy_missing = evaluate_gate(true, true, || loaded(false));
        assert_eq!(busy_missing.skip_reason, Some(reason::GPU_BUSY));
        assert!(busy_missing.reconcile);
        assert!(busy_missing.gpu_busy);
    }

    #[test]
    fn gate_disabled_never_asks_ollama() {
        let gate = evaluate_gate(false, false, || panic!("must not query /api/ps when off"));
        assert!(!gate.should_refine);
        assert_eq!(gate.skip_reason, Some(reason::DISABLED));
        assert!(!gate.reconcile);
    }

    #[test]
    fn missing_model_reason_follows_the_query_result() {
        assert_eq!(missing_model_reason(true), reason::MODEL_NOT_LOADED);
        assert_eq!(missing_model_reason(false), reason::PS_QUERY_FAILED);
    }

    #[test]
    fn gate_with_the_real_status_query_reports_an_unreachable_ollama() {
        let endpoint = "http://127.0.0.1:1";
        let gate = evaluate_gate(true, false, || {
            crate::ai_fallback::provider::fetch_ollama_model_load_status(endpoint, "any:model")
        });
        assert!(
            !gate.ps_ok,
            "a refused connection is a failed query, not 'not loaded'"
        );
        assert_eq!(gate.skip_reason, Some(reason::PS_QUERY_FAILED));
        assert!(gate.reconcile);
    }

    /// Live check against a private Ollama (never the user's server): model loaded -> unloaded
    /// behind the app's back -> the gate notices -> the warm-up function loads it again.
    /// `TRISPR_LIVE_OLLAMA_ENDPOINT=http://127.0.0.1:11500 TRISPR_LIVE_OLLAMA_MODEL=gemma4:e2b
    /// cargo test --lib live_unload -- --ignored --nocapture`
    #[test]
    #[ignore = "needs a private Ollama: TRISPR_LIVE_OLLAMA_ENDPOINT / TRISPR_LIVE_OLLAMA_MODEL"]
    fn live_unload_is_noticed_by_the_gate_and_healed_by_the_warmup() {
        let endpoint = std::env::var("TRISPR_LIVE_OLLAMA_ENDPOINT").expect("endpoint");
        let model = std::env::var("TRISPR_LIVE_OLLAMA_MODEL").expect("model");
        assert!(
            !endpoint.contains("11434"),
            "the user's Ollama must never be used for tests"
        );
        let status =
            || crate::ai_fallback::provider::fetch_ollama_model_load_status(&endpoint, &model);
        let wait_until = |want_loaded: bool| {
            for _ in 0..40 {
                if status().map(|s| s.loaded) == Some(want_loaded) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
            panic!("model never reached loaded={want_loaded}");
        };
        let num_ctx = crate::ai_fallback::provider::num_ctx_for_tier(0);

        // 1. warm: the gate lets refinement run
        crate::warmup_ollama_model_impl(&endpoint, &model, num_ctx).expect("first warm-up");
        wait_until(true);
        let gate = evaluate_gate(true, false, status);
        println!("after warm-up      : {gate:?}");
        assert!(gate.should_refine && !gate.reconcile);

        // 2. the model disappears behind the app's back
        crate::unload_ollama_model_impl(&endpoint, &model).expect("unload");
        wait_until(false);
        let gate = evaluate_gate(true, false, status);
        println!("after keep_alive:0 : {gate:?}");
        assert_eq!(gate.skip_reason, Some(reason::MODEL_NOT_LOADED));
        assert!(gate.reconcile, "the stale warm flag must be reconciled");

        // 3. the warm-up function the gate starts brings it back
        crate::warmup_ollama_model_impl(&endpoint, &model, num_ctx).expect("healing warm-up");
        wait_until(true);
        let gate = evaluate_gate(true, false, status);
        println!("after healing      : {gate:?}");
        assert!(gate.should_refine && gate.skip_reason.is_none());

        // leave the private server empty
        crate::unload_ollama_model_impl(&endpoint, &model).expect("cleanup unload");
        wait_until(false);
    }

    #[test]
    fn gate_warmups_are_spaced_thirty_seconds_apart() {
        let mut h = RefinementHealth::default();
        assert!(h.claim_gate_warmup(T0, &CFG));
        assert!(!h.claim_gate_warmup(T0 + 29_999, &CFG));
        assert!(h.claim_gate_warmup(T0 + 30_000, &CFG));
    }
}
