use crate::paths::resolve_config_path;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tracing::warn;

const PROFILE_FILENAME: &str = "refinement_latency_profile.json";
const PROFILE_VERSION: u32 = 1;
const FAST_SUCCESS_MS: u64 = 2_000;
const SLOW_SUCCESS_MS: u64 = 4_000;
const FAST_SUCCESS_STREAK_TO_RELAX: u32 = 3;
const EWMA_ALPHA: f64 = 0.35;

static PROFILE_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn profile_io_lock() -> &'static Mutex<()> {
    PROFILE_IO_LOCK.get_or_init(|| Mutex::new(()))
}

fn model_key(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_ascii_lowercase())
    }
}

fn is_fast_success(ms: u64) -> bool {
    ms <= FAST_SUCCESS_MS
}

fn is_slow_success(ms: u64) -> bool {
    ms >= SLOW_SUCCESS_MS
}

fn update_ewma(previous: Option<f64>, sample_ms: u64) -> f64 {
    match previous {
        Some(prev) => prev * (1.0 - EWMA_ALPHA) + (sample_ms as f64) * EWMA_ALPHA,
        None => sample_ms as f64,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RefinementLatencyProfile {
    pub(crate) version: u32,
    pub(crate) updated_ms: u64,
    pub(crate) models: BTreeMap<String, RefinementModelStats>,
}

impl Default for RefinementLatencyProfile {
    fn default() -> Self {
        Self {
            version: PROFILE_VERSION,
            updated_ms: 0,
            models: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct RefinementModelStats {
    pub(crate) total_runs: u64,
    pub(crate) success_runs: u64,
    pub(crate) timeout_runs: u64,
    pub(crate) failure_runs: u64,
    pub(crate) fast_success_streak: u32,
    pub(crate) slow_event_streak: u32,
    pub(crate) last_execution_ms: Option<u64>,
    pub(crate) last_success_ms: Option<u64>,
    pub(crate) last_timeout_ms: Option<u64>,
    pub(crate) ewma_success_ms: Option<f64>,
    pub(crate) prefer_low_latency: bool,
    pub(crate) last_outcome: String,
    pub(crate) last_updated_ms: u64,
}

impl Default for RefinementModelStats {
    fn default() -> Self {
        Self {
            total_runs: 0,
            success_runs: 0,
            timeout_runs: 0,
            failure_runs: 0,
            fast_success_streak: 0,
            slow_event_streak: 0,
            last_execution_ms: None,
            last_success_ms: None,
            last_timeout_ms: None,
            ewma_success_ms: None,
            prefer_low_latency: false,
            last_outcome: String::new(),
            last_updated_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RefinementObservation {
    Success { execution_ms: u64 },
    Timeout,
    Failure,
}

fn profile_path(app: &AppHandle) -> std::path::PathBuf {
    resolve_config_path(app, PROFILE_FILENAME)
}

fn load_profile_locked(app: &AppHandle) -> RefinementLatencyProfile {
    let path = profile_path(app);
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            RefinementLatencyProfile::default()
        }
        Err(err) => {
            warn!(
                "Failed to read refinement latency profile '{}': {}",
                path.display(),
                err
            );
            RefinementLatencyProfile::default()
        }
    }
}

fn save_profile_locked(app: &AppHandle, profile: &RefinementLatencyProfile) -> Result<(), String> {
    let path = profile_path(app);
    let payload = serde_json::to_string_pretty(profile).map_err(|err| err.to_string())?;
    fs::write(&path, payload).map_err(|err| {
        format!(
            "Failed to write refinement latency profile '{}': {}",
            path.display(),
            err
        )
    })
}

fn recompute_preference(stats: &mut RefinementModelStats) {
    let ewma = stats.ewma_success_ms.unwrap_or(0.0);
    let fast_to_relax =
        stats.fast_success_streak >= FAST_SUCCESS_STREAK_TO_RELAX && ewma <= 1_800.0;
    if fast_to_relax {
        stats.prefer_low_latency = false;
        return;
    }

    if stats.slow_event_streak > 0 || ewma >= 2_500.0 {
        stats.prefer_low_latency = true;
    }
}

pub(crate) fn record_refinement_observation(
    app: &AppHandle,
    model: &str,
    observation: RefinementObservation,
) {
    let Some(model_key) = model_key(model) else {
        return;
    };

    let lock = profile_io_lock().lock();
    let _guard = match lock {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    let mut profile = load_profile_locked(app);
    let now_ms = crate::util::now_ms();
    let stats = profile
        .models
        .entry(model_key)
        .or_insert_with(RefinementModelStats::default);

    stats.total_runs = stats.total_runs.saturating_add(1);
    stats.last_updated_ms = now_ms;

    match observation {
        RefinementObservation::Success { execution_ms } => {
            stats.success_runs = stats.success_runs.saturating_add(1);
            stats.last_success_ms = Some(now_ms);
            stats.last_execution_ms = Some(execution_ms);
            stats.last_outcome = "success".to_string();
            stats.ewma_success_ms = Some(update_ewma(stats.ewma_success_ms, execution_ms));
            if is_fast_success(execution_ms) {
                stats.fast_success_streak = stats.fast_success_streak.saturating_add(1);
                stats.slow_event_streak = 0;
            } else if is_slow_success(execution_ms) {
                stats.slow_event_streak = stats.slow_event_streak.saturating_add(1);
                stats.fast_success_streak = 0;
            } else {
                stats.fast_success_streak = 0;
                stats.slow_event_streak = 0;
            }
        }
        RefinementObservation::Timeout => {
            stats.timeout_runs = stats.timeout_runs.saturating_add(1);
            stats.last_timeout_ms = Some(now_ms);
            stats.last_outcome = "timeout".to_string();
            stats.slow_event_streak = stats.slow_event_streak.saturating_add(1);
            stats.fast_success_streak = 0;
        }
        RefinementObservation::Failure => {
            stats.failure_runs = stats.failure_runs.saturating_add(1);
            stats.last_outcome = "failure".to_string();
        }
    }

    recompute_preference(stats);
    profile.updated_ms = now_ms;

    if let Err(err) = save_profile_locked(app, &profile) {
        warn!("Failed to persist refinement latency profile: {}", err);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_run_enables_low_latency() {
        let mut stats = RefinementModelStats::default();
        stats.slow_event_streak = 1;
        recompute_preference(&mut stats);
        assert!(stats.prefer_low_latency);
    }

    #[test]
    fn three_fast_runs_relax_low_latency() {
        let mut stats = RefinementModelStats::default();
        stats.prefer_low_latency = true;
        stats.fast_success_streak = FAST_SUCCESS_STREAK_TO_RELAX;
        stats.ewma_success_ms = Some(900.0);
        recompute_preference(&mut stats);
        assert!(!stats.prefer_low_latency);
    }

    #[test]
    fn ewma_above_threshold_prefers_low_latency() {
        let mut stats = RefinementModelStats::default();
        stats.ewma_success_ms = Some(3_100.0);
        recompute_preference(&mut stats);
        assert!(stats.prefer_low_latency);
    }
}
