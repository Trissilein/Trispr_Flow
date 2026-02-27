#[cfg(target_os = "windows")]
use crate::audio::CaptureBuffer;
#[cfg(target_os = "windows")]
use crate::constants::TRANSCRIBE_IDLE_METER_MS;
#[cfg(target_os = "windows")]
use crate::constants::{MIN_AUDIO_MS, VAD_THRESHOLD_SUSTAIN_DEFAULT};
use crate::constants::{
    TARGET_SAMPLE_RATE, TRANSCRIBE_BACKLOG_EXPAND_DENOMINATOR, TRANSCRIBE_BACKLOG_EXPAND_NUMERATOR,
    TRANSCRIBE_BACKLOG_MIN_CHUNKS, TRANSCRIBE_BACKLOG_TARGET_MS,
};
#[cfg(any(test, target_os = "windows"))]
use crate::constants::TRANSCRIBE_BACKLOG_WARNING_PERCENT;
#[cfg(target_os = "windows")]
use crate::continuous_dump::{AdaptiveSegmenter, AdaptiveSegmenterConfig, SegmentFlushReason};
use crate::errors::AppError;
use crate::models::resolve_model_path;
use crate::overlay::{update_overlay_state, OverlayState};
#[cfg(target_os = "windows")]
use crate::paths::resolve_recordings_dir;
use crate::paths::resolve_whisper_cli_path;
#[cfg(target_os = "windows")]
use crate::postprocessing::process_transcript;
#[cfg(target_os = "windows")]
use crate::state::push_transcribe_entry_inner;
use crate::state::{AppState, Settings};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::io::ErrorKind;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, warn};
#[cfg(target_os = "windows")]
use tracing::info;

const TRANSCRIPTION_ACCEL_UNKNOWN: u8 = 0;
const TRANSCRIPTION_ACCEL_CPU: u8 = 1;
const TRANSCRIPTION_ACCEL_GPU: u8 = 2;
static LAST_TRANSCRIPTION_ACCELERATOR: AtomicU8 = AtomicU8::new(TRANSCRIPTION_ACCEL_UNKNOWN);

pub(crate) fn last_transcription_accelerator() -> &'static str {
    match LAST_TRANSCRIPTION_ACCELERATOR.load(Ordering::Relaxed) {
        TRANSCRIPTION_ACCEL_GPU => "gpu",
        TRANSCRIPTION_ACCEL_CPU => "cpu",
        _ => "unknown",
    }
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Serialize)]
struct ContinuousDumpEvent {
    source: &'static str,
    reason: SegmentFlushReason,
    duration_ms: u64,
    rms: f32,
    text_len: usize,
}

#[cfg(target_os = "windows")]
#[derive(Debug, Clone, Serialize)]
struct ContinuousDumpStats {
    source: &'static str,
    queued_chunks: usize,
    dropped_chunks: u64,
    percent_used: u8,
}

#[cfg(target_os = "windows")]
fn system_segmenter_config(settings: &Settings) -> AdaptiveSegmenterConfig {
    if !settings.continuous_dump_enabled {
        let mut legacy = AdaptiveSegmenterConfig::balanced_default();
        legacy.soft_flush_ms = settings.transcribe_batch_interval_ms;
        legacy.silence_flush_ms = 5_000;
        legacy.hard_cut_ms = 120_000;
        legacy.min_chunk_ms = MIN_AUDIO_MS;
        legacy.pre_roll_ms = settings.transcribe_chunk_overlap_ms.min(1_500);
        legacy.post_roll_ms = 0;
        legacy.idle_keepalive_ms = settings.transcribe_batch_interval_ms.max(10_000);
        legacy.threshold_start = settings.transcribe_vad_threshold.max(0.001);
        legacy.threshold_sustain =
            (legacy.threshold_start * 0.8).clamp(0.001, legacy.threshold_start);
        legacy.clamp();
        return legacy;
    }

    let mut cfg = AdaptiveSegmenterConfig::from_profile(&settings.continuous_dump_profile);
    cfg.soft_flush_ms = if settings.continuous_system_override_enabled {
        settings.continuous_system_soft_flush_ms
    } else {
        settings.continuous_soft_flush_ms
    };
    cfg.silence_flush_ms = if settings.continuous_system_override_enabled {
        settings.continuous_system_silence_flush_ms
    } else {
        settings.continuous_silence_flush_ms
    };
    cfg.hard_cut_ms = if settings.continuous_system_override_enabled {
        settings.continuous_system_hard_cut_ms
    } else {
        settings.continuous_hard_cut_ms
    };
    cfg.min_chunk_ms = settings.continuous_min_chunk_ms;
    cfg.pre_roll_ms = settings.continuous_pre_roll_ms;
    cfg.post_roll_ms = settings.continuous_post_roll_ms;
    cfg.idle_keepalive_ms = settings.continuous_idle_keepalive_ms;

    let start = if settings.transcribe_vad_mode {
        settings.transcribe_vad_threshold.max(0.001)
    } else {
        VAD_THRESHOLD_SUSTAIN_DEFAULT
    };
    cfg.threshold_start = start;
    cfg.threshold_sustain = (start * 0.8).clamp(0.001, start);
    cfg.clamp();
    cfg
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TranscriptionResult {
    pub(crate) text: String,
    pub(crate) source: String,
    pub(crate) job_id: String,
    pub(crate) paste_deferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) paste_timeout_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) entry_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct TranscriptionGpuActivityEvent {
    state: &'static str,
    accelerator: &'static str,
    backend: String,
    source: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Default)]
pub(crate) struct TranscribeRecorder {
    pub(crate) active: bool,
    pub(crate) stop_tx: Option<std::sync::mpsc::Sender<()>>,
    pub(crate) join_handle: Option<thread::JoinHandle<()>>,
    queue: Option<Arc<AudioQueue>>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TranscribeBacklogStatus {
    pub(crate) queued_chunks: usize,
    pub(crate) capacity_chunks: usize,
    pub(crate) percent_used: u8,
    pub(crate) dropped_chunks: u64,
    pub(crate) suggested_capacity_chunks: usize,
}

struct AudioQueueState {
    queue: VecDeque<Vec<i16>>,
    max_chunks: usize,
    dropped_chunks: u64,
    #[cfg(any(test, target_os = "windows"))]
    warned_for_capacity: usize,
}

struct AudioQueue {
    inner: Mutex<AudioQueueState>,
    cond: Condvar,
    closed: AtomicBool,
    app: Option<AppHandle>,
}

impl AudioQueue {
    fn new(max_chunks: usize, app: Option<AppHandle>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(AudioQueueState {
                queue: VecDeque::new(),
                max_chunks: max_chunks.max(1),
                dropped_chunks: 0,
                #[cfg(any(test, target_os = "windows"))]
                warned_for_capacity: 0,
            }),
            cond: Condvar::new(),
            closed: AtomicBool::new(false),
            app,
        })
    }

    #[cfg(any(test, target_os = "windows"))]
    fn push(&self, chunk: Vec<i16>) {
        let mut queue = self.inner.lock().unwrap();
        if queue.queue.len() >= queue.max_chunks {
            queue.queue.pop_front();
            queue.dropped_chunks = queue.dropped_chunks.saturating_add(1);
        }
        queue.queue.push_back(chunk);

        let warning_threshold = backlog_warning_threshold(queue.max_chunks);
        let should_warn =
            queue.warned_for_capacity != queue.max_chunks && queue.queue.len() >= warning_threshold;
        let warning_payload = if should_warn {
            queue.warned_for_capacity = queue.max_chunks;
            Some(backlog_status_from_queue(&queue))
        } else {
            None
        };

        self.cond.notify_one();
        drop(queue);

        if let Some(payload) = warning_payload {
            self.emit_event("transcribe:backlog-warning", payload);
        }
    }

    #[cfg(any(test, target_os = "windows"))]
    fn pop(&self) -> Option<Vec<i16>> {
        let mut queue = self.inner.lock().unwrap();
        loop {
            if let Some(chunk) = queue.queue.pop_front() {
                return Some(chunk);
            }
            if self.closed.load(Ordering::Relaxed) {
                return None;
            }
            queue = self.cond.wait(queue).unwrap();
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Relaxed);
        self.cond.notify_all();
    }

    #[cfg(any(test, target_os = "windows"))]
    fn status(&self) -> TranscribeBacklogStatus {
        let queue = self.inner.lock().unwrap();
        backlog_status_from_queue(&queue)
    }

    fn expand_capacity(&self) -> TranscribeBacklogStatus {
        let mut queue = self.inner.lock().unwrap();
        let current = queue.max_chunks;
        let expanded = expanded_capacity(current);
        queue.max_chunks = expanded.max(current + 1);
        let status = backlog_status_from_queue(&queue);
        drop(queue);
        self.emit_event("transcribe:backlog-expanded", status.clone());
        status
    }

    fn emit_event<T: Serialize + Clone>(&self, name: &str, payload: T) {
        if let Some(app) = &self.app {
            let _ = app.emit(name, payload);
        }
    }
}

fn backlog_capacity_for_batch_ms(batch_interval_ms: u64) -> usize {
    let interval_ms = batch_interval_ms.max(1000);
    let chunks = ((TRANSCRIBE_BACKLOG_TARGET_MS + interval_ms - 1) / interval_ms) as usize;
    chunks.max(TRANSCRIBE_BACKLOG_MIN_CHUNKS)
}

#[cfg(any(test, target_os = "windows"))]
fn backlog_warning_threshold(capacity: usize) -> usize {
    ((capacity * TRANSCRIBE_BACKLOG_WARNING_PERCENT as usize) + 99) / 100
}

fn expanded_capacity(current_capacity: usize) -> usize {
    let numerator = TRANSCRIBE_BACKLOG_EXPAND_NUMERATOR.max(1);
    let denominator = TRANSCRIBE_BACKLOG_EXPAND_DENOMINATOR.max(1);
    let expanded = current_capacity
        .saturating_mul(numerator)
        .saturating_add(denominator.saturating_sub(1))
        / denominator;
    expanded.max(current_capacity + 1)
}

fn backlog_status_from_queue(queue: &AudioQueueState) -> TranscribeBacklogStatus {
    let used = queue.queue.len();
    let capacity = queue.max_chunks.max(1);
    let percent_used = ((used * 100) / capacity).min(100) as u8;
    TranscribeBacklogStatus {
        queued_chunks: used,
        capacity_chunks: capacity,
        percent_used,
        dropped_chunks: queue.dropped_chunks,
        suggested_capacity_chunks: expanded_capacity(capacity),
    }
}

#[cfg(test)]
mod tests {
    use super::{backlog_capacity_for_batch_ms, should_drop_transcript, AudioQueue};

    #[test]
    fn audio_queue_drops_oldest_when_full() {
        let queue = AudioQueue::new(2, None);
        queue.push(vec![1]);
        queue.push(vec![2]);
        queue.push(vec![3]);

        assert_eq!(queue.pop().unwrap(), vec![2]);
        assert_eq!(queue.pop().unwrap(), vec![3]);

        queue.close();
        assert!(queue.pop().is_none());
    }

    #[test]
    fn audio_queue_close_unblocks_empty() {
        let queue = AudioQueue::new(1, None);
        queue.close();
        assert!(queue.pop().is_none());
    }

    #[test]
    fn audio_queue_expands_capacity() {
        let queue = AudioQueue::new(6, None);
        let before = queue.status();
        assert_eq!(before.capacity_chunks, 6);

        let after = queue.expand_capacity();
        assert_eq!(after.capacity_chunks, 9);
    }

    #[test]
    fn backlog_capacity_targets_ten_minutes() {
        assert_eq!(backlog_capacity_for_batch_ms(8_000), 75);
        assert_eq!(backlog_capacity_for_batch_ms(4_000), 150);
        assert_eq!(backlog_capacity_for_batch_ms(15_000), 40);
    }

    #[test]
    fn short_meaningful_transcript_is_not_dropped() {
        assert!(!should_drop_transcript("Bitte speichere das", 0.001, 450, false));
        assert!(!should_drop_transcript("das passt", 0.002, 300, false));
    }

    #[test]
    fn common_short_hallucination_is_dropped() {
        assert!(should_drop_transcript("thank you", 0.002, 500, false));
        assert!(should_drop_transcript("uh", 0.001, 400, false));
    }
}

fn emit_transcribe_idle(app: &AppHandle) {
    let _ = app.emit("transcribe:level", 0.0f32);
    let _ = app.emit("transcribe:db", -60.0f32);
}

fn update_transcribe_overlay(app: &AppHandle, active: bool) {
    if let Ok(recorder) = app.state::<AppState>().recorder.lock() {
        if recorder.active || recorder.transcribing {
            return;
        }
    }

    let state = if active {
        OverlayState::Transcribing
    } else {
        OverlayState::Idle
    };
    let _ = update_overlay_state(app, state);
}

impl TranscribeRecorder {
    pub(crate) fn new() -> Self {
        Self {
            active: false,
            stop_tx: None,
            join_handle: None,
            queue: None,
        }
    }
}

pub(crate) fn expand_transcribe_backlog(
    app: &AppHandle,
) -> Result<TranscribeBacklogStatus, String> {
    let queue = {
        let state = app.state::<AppState>();
        let recorder = state.transcribe.lock().unwrap();
        recorder.queue.clone()
    };
    let queue = queue.ok_or_else(|| "Output transcription is not active.".to_string())?;
    Ok(queue.expand_capacity())
}

pub(crate) fn start_transcribe_monitor(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &Settings,
) -> Result<(), String> {
    // CRITICAL SECURITY CHECK: Only start if explicitly enabled
    if !settings.transcribe_enabled {
        error!("SECURITY: Attempted to start transcribe monitor while transcribe_enabled=false. Blocking.");
        return Err("Transcription is disabled in settings".to_string());
    }

    let mut recorder = state.transcribe.lock().unwrap();
    if recorder.active {
        return Ok(());
    }

    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let app_handle = app.clone();
    let settings = settings.clone();
    let queue_capacity = backlog_capacity_for_batch_ms(settings.transcribe_batch_interval_ms);
    let queue = AudioQueue::new(queue_capacity, Some(app_handle.clone()));
    #[cfg(target_os = "windows")]
    let worker_queue = queue.clone();

    let join_handle = thread::spawn(move || {
        #[cfg(target_os = "windows")]
        {
            if let Err(err) =
                run_transcribe_loopback(app_handle.clone(), settings, stop_rx, worker_queue)
            {
                crate::emit_error(
                    &app_handle,
                    AppError::AudioDevice(err),
                    Some("System Audio"),
                );
                let state = app_handle.state::<AppState>();
                state.transcribe_active.store(false, Ordering::Relaxed);
                if let Ok(mut transcribe) = state.transcribe.lock() {
                    transcribe.active = false;
                    transcribe.stop_tx = None;
                    transcribe.join_handle = None;
                    transcribe.queue = None;
                }
                let _ = app_handle.emit("transcribe:state", "idle");
                emit_transcribe_idle(&app_handle);
                update_transcribe_overlay(&app_handle, false);
            }
            return;
        }

        #[cfg(not(target_os = "windows"))]
        {
            let _ = stop_rx.recv();
            crate::emit_error(
                &app_handle,
                AppError::AudioDevice(
                    "System audio capture is not supported on this OS yet.".to_string(),
                ),
                Some("System Audio"),
            );
            let state = app_handle.state::<AppState>();
            state.transcribe_active.store(false, Ordering::Relaxed);
            if let Ok(mut transcribe) = state.transcribe.lock() {
                transcribe.active = false;
                transcribe.stop_tx = None;
                transcribe.join_handle = None;
                transcribe.queue = None;
            }
            let _ = app_handle.emit("transcribe:state", "idle");
            emit_transcribe_idle(&app_handle);
            update_transcribe_overlay(&app_handle, false);
        }
    });

    recorder.active = true;
    recorder.stop_tx = Some(stop_tx);
    recorder.join_handle = Some(join_handle);
    recorder.queue = Some(queue);
    state.transcribe_active.store(true, Ordering::Relaxed);

    emit_transcribe_idle(app);
    let _ = app.emit("transcribe:state", "idle");
    Ok(())
}

pub(crate) fn stop_transcribe_monitor(app: &AppHandle, state: &State<'_, AppState>) {
    let (stop_tx, join_handle, queue) = {
        let mut recorder = state.transcribe.lock().unwrap();
        recorder.active = false;
        (
            recorder.stop_tx.take(),
            recorder.join_handle.take(),
            recorder.queue.take(),
        )
    };

    state.transcribe_active.store(false, Ordering::Relaxed);
    let _ = app.emit("transcribe:state", "idle");
    update_transcribe_overlay(app, false);
    emit_transcribe_idle(app);

    if let Some(queue) = queue {
        queue.close();
    }
    if let Some(tx) = stop_tx {
        let _ = tx.send(());
    }
    if let Some(handle) = join_handle {
        thread::spawn(move || {
            let _ = handle.join();
        });
    }
}

pub(crate) fn toggle_transcribe_state(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    if !settings.transcribe_enabled {
        let _ = app.emit("transcribe:state", "idle");
        emit_transcribe_idle(app);
        update_transcribe_overlay(app, false);
        return;
    }
    let active = state.transcribe_active.load(Ordering::Relaxed);
    if active {
        stop_transcribe_monitor(app, &state);
    } else if let Err(err) = start_transcribe_monitor(app, &state, &settings) {
        crate::emit_error(app, AppError::AudioDevice(err), Some("System Audio"));
        state.transcribe_active.store(false, Ordering::Relaxed);
        let _ = app.emit("transcribe:state", "idle");
    }
}

#[cfg(target_os = "windows")]
fn rms_f32(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for &sample in samples {
        sum += sample * sample;
    }
    (sum / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

pub(crate) fn rms_i16(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for &sample in samples {
        let value = sample as f32 / i16::MAX as f32;
        sum += value * value;
    }
    (sum / samples.len() as f32).sqrt().clamp(0.0, 1.0)
}

fn normalize_transcript(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Drop-filter for transcribed text.
///
/// * `strict = false` (mic input): drops a known hallucination phrase only when the
///   captured audio segment is very short (≤ HALLUCINATION_MAX_DURATION_MS).  This
///   preserves genuine short dictations like "Stop" or "OK Google".
///
/// * `strict = true` (system-audio loopback): applies two extra rules because
///   loopback audio produces far more false-positive fragments than a mic:
///   1. Known phrases are always dropped, regardless of segment duration.
///   2. Any utterance that is ≤ 2 words **and** ≤ 15 characters is dropped — these
///      are almost always background-audio noise ("All right.", "Oh.", "Fine.") that
///      Whisper transcribes but are not useful content.
pub(crate) fn should_drop_transcript(text: &str, _rms: f32, duration_ms: u64, strict: bool) -> bool {
    let normalized = normalize_transcript(text);
    if normalized.is_empty() {
        return true;
    }

    let matches_common = HALLUCINATION_PHRASES.iter().any(|p| *p == normalized);

    if strict {
        if matches_common {
            return true;
        }
        let word_count = normalized.split_whitespace().count();
        if word_count <= 2 && normalized.len() <= 15 {
            return true;
        }
    } else {
        let is_short_audio = duration_ms <= crate::constants::HALLUCINATION_MAX_DURATION_MS;
        if matches_common && is_short_audio {
            return true;
        }
    }

    false
}

pub(crate) fn should_drop_by_activation_words(
    text: &str,
    activation_words: &[String],
    enabled: bool,
) -> bool {
    if !enabled || activation_words.is_empty() {
        return false; // Don't drop
    }

    let normalized_text = normalize_transcript(text);
    let words: Vec<&str> = normalized_text.split_whitespace().collect();

    // Check if any activation word exists as complete word
    for activation_word in activation_words {
        for word in &words {
            if *word == activation_word.as_str() {
                return false; // Found activation word, don't drop
            }
        }
    }

    true // No activation word found, drop
}

const HALLUCINATION_PHRASES: &[&str] = &[
    // Filler sounds / acknowledgements
    "uh", "um", "hmm", "huh", "ah", "oh", "uh huh",
    // Single-word reactions
    "yes", "no", "okay", "ok", "yeah", "right", "sure", "fine",
    "good", "great", "nice", "wow", "cool", "really", "exactly",
    "absolutely", "definitely", "correct", "true",
    "hey", "hi", "hello", "bye", "goodbye", "welcome",
    "please", "wait", "sorry",
    // Gratitude / social phrases
    "you", "thank you", "thanks",
    // Two-word phrases common in background audio
    "all right", "alright", "oh no", "oh yeah", "oh well", "oh wow", "oh my",
    "come on", "go on", "hold on",
    "i see", "me too", "of course", "no no", "yes yes",
    "good job", "well done", "no problem", "no worries", "for sure",
    "see ya", "take care", "good luck", "good night", "good morning",
    "thats right", "youre right", "youre welcome", "not bad",
];

/// Flush accumulated system audio as a session chunk via SessionManager.
/// Replaces the old per-flush file approach: chunks go to a temp session dir
/// and are merged into a single session.opus when the session ends.
#[cfg(target_os = "windows")]
fn flush_system_audio_to_session(buffer: &mut Vec<i16>) {
    if buffer.is_empty() {
        return;
    }
    let duration_ms = buffer.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;
    if duration_ms < 10_000 {
        // Don't save chunks shorter than 10 seconds
        buffer.clear();
        return;
    }
    info!(
        "Flushing system audio chunk: {} samples ({} ms)",
        buffer.len(),
        duration_ms
    );
    if let Err(e) = crate::session_manager::flush_chunk(buffer, "output") {
        error!("Failed to flush system audio chunk: {}", e);
    }
    buffer.clear();
}

#[cfg(any(test, target_os = "windows"))]
fn append_chunk_for_session_recording(
    save_buffer: &mut Vec<i16>,
    chunk: &[i16],
    overlap_samples: usize,
    chunk_count: &mut u64,
) {
    if chunk.is_empty() {
        return;
    }

    let skip_prefix = if *chunk_count == 0 {
        0
    } else {
        overlap_samples.min(chunk.len())
    };

    if skip_prefix < chunk.len() {
        save_buffer.extend_from_slice(&chunk[skip_prefix..]);
    }
    *chunk_count = chunk_count.saturating_add(1);
}

#[cfg(test)]
mod session_recording_tests {
    use super::append_chunk_for_session_recording;

    #[test]
    fn overlap_prefix_is_removed_after_first_chunk() {
        let mut out = Vec::new();
        let mut chunk_count = 0u64;

        append_chunk_for_session_recording(&mut out, &[1, 2, 3, 4], 2, &mut chunk_count);
        append_chunk_for_session_recording(&mut out, &[3, 4, 5, 6], 2, &mut chunk_count);

        assert_eq!(out, vec![1, 2, 3, 4, 5, 6]);
    }
}

#[cfg(target_os = "windows")]
fn transcribe_worker(
    app: AppHandle,
    settings: Settings,
    queue: Arc<AudioQueue>,
    transcribing: Arc<AtomicBool>,
) {
    let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
    // System audio auto-save buffer (accumulates chunks before flushing to session)
    let auto_save = settings.auto_save_system_audio && settings.opus_enabled;
    let mut save_buffer: Vec<i16> = Vec::new();
    let mut saved_chunk_count: u64 = 0;
    let overlap_samples = 0usize;
    // Flush every 60 seconds of audio (960_000 samples at 16kHz)
    let flush_threshold = TARGET_SAMPLE_RATE as usize * 60;

    // Initialise SessionManager with the recordings directory for this session
    if auto_save {
        let recordings_dir = resolve_recordings_dir(&app);
        crate::session_manager::init(recordings_dir);
    }

    while let Some(chunk) = queue.pop() {
        if chunk.len() < min_samples {
            continue;
        }

        // Accumulate chunks for system audio session
        if auto_save {
            append_chunk_for_session_recording(
                &mut save_buffer,
                &chunk,
                overlap_samples,
                &mut saved_chunk_count,
            );
            if save_buffer.len() >= flush_threshold {
                flush_system_audio_to_session(&mut save_buffer);
            }
        }

        let level = rms_i16(&chunk);
        let duration_ms = chunk.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

        if settings.transcribe_vad_mode {
            if level < settings.transcribe_vad_threshold {
                continue;
            }
        }

        transcribing.store(true, Ordering::Relaxed);
        let _ = app.emit("transcribe:state", "transcribing");
        update_transcribe_overlay(&app, true);
        let result = transcribe_audio(&app, &settings, &chunk);
        transcribing.store(false, Ordering::Relaxed);
        update_transcribe_overlay(&app, false);

        match result {
            Ok((text, _source)) => {
                if !text.trim().is_empty()
                    && !should_drop_transcript(&text, level, duration_ms, true)
                    && !should_drop_by_activation_words(
                        &text,
                        &settings.activation_words,
                        settings.activation_words_enabled,
                    )
                {
                    // Apply post-processing if enabled
                    let processed_text = if settings.postproc_enabled {
                        match process_transcript(&text, &settings, &app) {
                            Ok(processed) => processed,
                            Err(e) => {
                                error!("Post-processing failed: {}", e);
                                text.clone() // Fallback to original
                            }
                        }
                    } else {
                        text.clone()
                    };

                    let state = app.state::<AppState>();
                    let _ = push_transcribe_entry_inner(
                        &app,
                        &state.history_transcribe,
                        processed_text,
                    );
                }
            }
            Err(err) => {
                let _ = app.emit("transcription:error", err);
            }
        }
    }

    // Flush remaining buffer and finalize the session on worker exit
    if auto_save {
        flush_system_audio_to_session(&mut save_buffer);
        match crate::session_manager::finalize_for("output") {
            Ok(Some(path)) => {
                let state = app.state::<AppState>();
                *state.last_system_recording_path.lock().unwrap() =
                    Some(path.to_string_lossy().to_string());
                info!("System audio session finalized");
            }
            Ok(None) => info!("System audio session ended with no chunks"),
            Err(e) => error!("Failed to finalize system audio session: {}", e),
        }
    }
}

#[cfg(target_os = "windows")]
fn decode_wasapi_mono(
    raw: &[u8],
    channels: usize,
    bytes_per_sample: usize,
    sample_format: wasapi::SampleType,
) -> Vec<f32> {
    if channels == 0 || bytes_per_sample == 0 {
        return Vec::new();
    }

    let bytes_per_frame = channels * bytes_per_sample;
    let mut mono = Vec::with_capacity(raw.len() / bytes_per_frame);

    match sample_format {
        wasapi::SampleType::Float => {
            if bytes_per_sample != 4 {
                return mono;
            }
            for frame in raw.chunks(bytes_per_frame) {
                let mut sum = 0.0f32;
                for sample in frame.chunks(bytes_per_sample) {
                    if sample.len() != 4 {
                        continue;
                    }
                    let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                    sum += value;
                }
                mono.push((sum / channels as f32).clamp(-1.0, 1.0));
            }
        }
        wasapi::SampleType::Int => {
            if bytes_per_sample == 2 {
                for frame in raw.chunks(bytes_per_frame) {
                    let mut sum = 0.0f32;
                    for sample in frame.chunks(bytes_per_sample) {
                        if sample.len() != 2 {
                            continue;
                        }
                        let value =
                            i16::from_le_bytes([sample[0], sample[1]]) as f32 / i16::MAX as f32;
                        sum += value;
                    }
                    mono.push((sum / channels as f32).clamp(-1.0, 1.0));
                }
            } else if bytes_per_sample == 3 {
                for frame in raw.chunks(bytes_per_frame) {
                    let mut sum = 0.0f32;
                    for sample in frame.chunks(bytes_per_sample) {
                        if sample.len() != 3 {
                            continue;
                        }
                        let value = ((sample[2] as i32) << 24
                            | (sample[1] as i32) << 16
                            | (sample[0] as i32) << 8)
                            >> 8;
                        let normalized = value as f32 / 8_388_608.0;
                        sum += normalized;
                    }
                    mono.push((sum / channels as f32).clamp(-1.0, 1.0));
                }
            } else if bytes_per_sample == 4 {
                for frame in raw.chunks(bytes_per_frame) {
                    let mut sum = 0.0f32;
                    for sample in frame.chunks(bytes_per_sample) {
                        if sample.len() != 4 {
                            continue;
                        }
                        let value = i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]])
                            as f32
                            / i32::MAX as f32;
                        sum += value;
                    }
                    mono.push((sum / channels as f32).clamp(-1.0, 1.0));
                }
            }
        }
    }

    mono
}

#[cfg(target_os = "windows")]
fn run_transcribe_loopback(
    app: AppHandle,
    settings: Settings,
    stop_rx: std::sync::mpsc::Receiver<()>,
    queue: Arc<AudioQueue>,
) -> Result<(), String> {
    let hr = wasapi::initialize_mta();
    if hr.0 < 0 {
        return Err(format!("WASAPI init error: 0x{:X}", hr.0));
    }
    let device = resolve_output_device(&settings.transcribe_output_device)
        .ok_or_else(|| "Output device not found".to_string())?;
    // Try to open the audio client, with one retry after a short delay.
    // WASAPI can fail on the first call when the audio subsystem is not yet fully
    // initialised at app start. Retrying avoids a silent fallback to the wrong device.
    let mut audio_client = match device.get_iaudioclient() {
        Ok(client) => client,
        Err(first_err) => {
            tracing::warn!(
                "WASAPI: get_iaudioclient() failed for '{}': {first_err}. Retrying in 400 ms.",
                settings.transcribe_output_device
            );
            std::thread::sleep(std::time::Duration::from_millis(400));
            device.get_iaudioclient().map_err(|e| {
                format!(
                    "WASAPI: could not open audio client for '{}' after retry: {e}",
                    settings.transcribe_output_device
                )
            })?
        }
    };

    let format = audio_client
        .get_mixformat()
        .map_err(|e| format!("WASAPI format error: {e}"))?;

    let channels = format.get_nchannels() as usize;
    let sample_rate = format.get_samplespersec();
    let bytes_per_sample = (format.get_bitspersample() as usize / 8).max(1);
    let bytes_per_frame = format.get_blockalign() as usize;
    let sample_format = format
        .get_subformat()
        .map_err(|e| format!("WASAPI sample type error: {e}"))?;

    let stream_mode = wasapi::StreamMode::PollingShared {
        autoconvert: true,
        buffer_duration_hns: 200_000,
    };
    audio_client
        .initialize_client(&format, &wasapi::Direction::Capture, &stream_mode)
        .map_err(|e| format!("WASAPI init error: {e}"))?;

    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| format!("WASAPI capture error: {e}"))?;

    audio_client.start_stream().map_err(|e| e.to_string())?;

    let mut segmenter = AdaptiveSegmenter::new(system_segmenter_config(&settings));
    let mut last_backpressure_check = Instant::now();
    let mut gain = (10.0f32).powf(settings.transcribe_input_gain_db / 20.0);
    let mut vad_enabled = settings.transcribe_vad_mode;
    let mut vad_threshold = settings.transcribe_vad_threshold;
    let mut vad_silence_ms = settings.transcribe_vad_silence_ms;
    let mut last_settings_check = Instant::now();
    let mut vad_last_hit_ms = Instant::now();

    let worker_app = app.clone();
    let worker_settings = settings.clone();
    let worker_queue = queue.clone();
    let transcribing = Arc::new(AtomicBool::new(false));
    let worker_transcribing = transcribing.clone();
    let worker_handle = thread::spawn(move || {
        transcribe_worker(
            worker_app,
            worker_settings,
            worker_queue,
            worker_transcribing,
        );
    });

    let mut buffer = CaptureBuffer::default();
    let mut smooth_level = 0.0f32;
    let mut last_emit = Instant::now();
    let mut last_idle_emit = Instant::now();
    let mut last_activity = Instant::now();
    let mut has_activity = false;
    let mut last_state = "idle";
    let mut was_transcribing = false;
    let mut monitor_threshold = if vad_enabled {
        vad_threshold
    } else {
        VAD_THRESHOLD_SUSTAIN_DEFAULT
    };
    let mut idle_grace_ms = if vad_enabled {
        vad_silence_ms
    } else {
        TRANSCRIBE_IDLE_METER_MS
    };

    // Chapter silence detection state
    let mut chapter_silence_enabled = settings.chapter_silence_enabled;
    let mut chapter_silence_threshold_ms = settings.chapter_silence_threshold_ms;
    let mut chapter_detected_for_current_silence = false;

    loop {
        match stop_rx.try_recv() {
            Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        let packet_frames = capture_client
            .get_next_packet_size()
            .map_err(|e| e.to_string())?;
        let packet_frames = match packet_frames {
            Some(value) => value,
            None => {
                if last_idle_emit.elapsed() >= Duration::from_millis(TRANSCRIBE_IDLE_METER_MS) {
                    let _ = app.emit("transcribe:level", 0.0f32);
                    let _ = app.emit("transcribe:db", -60.0f32);
                    last_idle_emit = Instant::now();
                }
                thread::sleep(Duration::from_millis(10));
                continue;
            }
        };
        if packet_frames == 0 {
            if last_idle_emit.elapsed() >= Duration::from_millis(TRANSCRIBE_IDLE_METER_MS) {
                let _ = app.emit("transcribe:level", 0.0f32);
                let _ = app.emit("transcribe:db", -60.0f32);
                last_idle_emit = Instant::now();
            }
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        let mut raw = vec![0u8; packet_frames as usize * bytes_per_frame];
        let (frames_read, _) = capture_client
            .read_from_device(&mut raw)
            .map_err(|e| e.to_string())?;
        if frames_read == 0 {
            continue;
        }

        let valid_bytes = frames_read as usize * bytes_per_frame;
        if last_settings_check.elapsed() >= Duration::from_millis(200) {
            if let Ok(current) = app.state::<AppState>().settings.lock() {
                gain = (10.0f32).powf(current.transcribe_input_gain_db / 20.0);
                vad_enabled = current.transcribe_vad_mode;
                vad_threshold = current.transcribe_vad_threshold;
                vad_silence_ms = current.transcribe_vad_silence_ms;
                segmenter.update_config(system_segmenter_config(&current));
                chapter_silence_enabled = current.chapter_silence_enabled;
                chapter_silence_threshold_ms = current.chapter_silence_threshold_ms;
                monitor_threshold = if vad_enabled {
                    vad_threshold
                } else {
                    VAD_THRESHOLD_SUSTAIN_DEFAULT
                };
                idle_grace_ms = if vad_enabled {
                    vad_silence_ms
                } else {
                    TRANSCRIBE_IDLE_METER_MS
                };
            }
            last_settings_check = Instant::now();
        }

        let mut mono = decode_wasapi_mono(
            &raw[..valid_bytes],
            channels,
            bytes_per_sample,
            sample_format,
        );
        if mono.is_empty() {
            continue;
        }

        if gain != 1.0 {
            for sample in mono.iter_mut() {
                *sample = (*sample * gain).clamp(-1.0, 1.0);
            }
        }

        let rms = rms_f32(&mono);
        if vad_enabled && rms >= vad_threshold {
            vad_last_hit_ms = Instant::now();
        }
        smooth_level = smooth_level * 0.8 + rms * 0.2;
        if smooth_level >= monitor_threshold {
            has_activity = true;
            last_activity = Instant::now();
        }
        if last_emit.elapsed() >= Duration::from_millis(50) {
            let db = if smooth_level <= 0.000_01 {
                -60.0
            } else {
                (20.0 * smooth_level.log10()).max(-60.0).min(0.0)
            };
            let meter = (db + 60.0) / 60.0;
            let _ = app.emit("transcribe:level", meter.clamp(0.0, 1.0));
            let _ = app.emit("transcribe:db", db);
            last_emit = Instant::now();
            last_idle_emit = last_emit;
        }
        let now_transcribing = transcribing.load(Ordering::Relaxed);
        if now_transcribing && !was_transcribing {
            last_state = "transcribing";
        }
        was_transcribing = now_transcribing;
        if !now_transcribing {
            let active =
                has_activity && last_activity.elapsed() <= Duration::from_millis(idle_grace_ms);
            let next_state = if active { "recording" } else { "idle" };
            if next_state != last_state {
                let _ = app.emit("transcribe:state", next_state);
                last_state = next_state;
            }

            // Chapter silence detection
            if chapter_silence_enabled && !active {
                let silence_duration_ms = last_activity.elapsed().as_millis() as u64;
                if silence_duration_ms >= chapter_silence_threshold_ms
                    && !chapter_detected_for_current_silence
                {
                    let timestamp_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64;
                    let _ = app.emit("chapter:detected", timestamp_ms);
                    chapter_detected_for_current_silence = true;
                }
            } else if active {
                chapter_detected_for_current_silence = false;
            }
        }

        buffer.push_samples(&mono, sample_rate);
        let resampled = buffer.take_all_samples();
        if !resampled.is_empty() {
            let segments = segmenter.push_samples(&resampled, smooth_level.max(rms));
            for mut segment in segments {
                if segment.samples.is_empty() {
                    continue;
                }
                if vad_enabled
                    && segment.rms < vad_threshold
                    && vad_last_hit_ms.elapsed() > Duration::from_millis(vad_silence_ms)
                {
                    continue;
                }

                let reason = segment.reason;
                let duration_ms = segment.duration_ms;
                let rms_value = segment.rms;
                let samples = std::mem::take(&mut segment.samples);
                queue.push(samples);
                let _ = app.emit(
                    "continuous-dump:segment",
                    ContinuousDumpEvent {
                        source: "system",
                        reason,
                        duration_ms,
                        rms: rms_value,
                        text_len: 0,
                    },
                );
            }
        }

        if last_backpressure_check.elapsed() >= Duration::from_millis(1_000) {
            let status = queue.status();
            segmenter.set_backpressure_percent(status.percent_used);
            let _ = app.emit(
                "continuous-dump:stats",
                ContinuousDumpStats {
                    source: "system",
                    queued_chunks: status.queued_chunks,
                    dropped_chunks: status.dropped_chunks,
                    percent_used: status.percent_used,
                },
            );
            last_backpressure_check = Instant::now();
        }
    }

    let leftover = buffer.take_all_samples();
    if !leftover.is_empty() {
        for mut segment in segmenter.push_samples(&leftover, 0.0) {
            let samples = std::mem::take(&mut segment.samples);
            if !samples.is_empty() {
                queue.push(samples);
            }
        }
    }
    for mut segment in segmenter.finalize() {
        let samples = std::mem::take(&mut segment.samples);
        if !samples.is_empty() {
            queue.push(samples);
        }
    }

    queue.close();
    let _ = worker_handle.join();
    let _ = audio_client.stop_stream();
    Ok(())
}

fn encode_wav_i16(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + samples.len() * 2);

    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());

    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    wav
}

pub(crate) fn transcribe_audio(
    app: &AppHandle,
    settings: &Settings,
    samples: &[i16],
) -> Result<(String, String), String> {
    let wav_bytes = encode_wav_i16(samples, TARGET_SAMPLE_RATE);

    if settings.cloud_fallback && legacy_cloud_transcription_enabled() {
        match transcribe_cloud(&wav_bytes) {
            Ok(text) => return Ok((text, "cloud-legacy".to_string())),
            Err(err) => {
                warn!(
                    "Legacy cloud transcription failed, falling back to local whisper: {}",
                    err
                );
                let _ = app.emit("transcription:legacy-cloud-failed", err.clone());
            }
        }
    }

    let text = transcribe_local(app, settings, &wav_bytes)?;
    Ok((text, "local".to_string()))
}

fn legacy_cloud_transcription_enabled() -> bool {
    matches!(
      std::env::var("TRISPR_ENABLE_LEGACY_CLOUD_TRANSCRIBE")
        .ok()
        .map(|v| v.trim().to_lowercase()),
      Some(v) if v == "1" || v == "true" || v == "yes" || v == "on"
    )
}

fn parse_env_usize(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        trimmed.parse::<usize>().ok()
    })
}

fn whisper_cli_looks_gpu_capable(cli_path: Option<&Path>) -> bool {
    cli_path
        .map(|path| path.to_string_lossy().to_lowercase())
        .map(|path| {
            path.contains("/cuda/")
                || path.contains("\\cuda\\")
                || path.contains("build-cuda")
                || path.contains("/vulkan/")
                || path.contains("\\vulkan\\")
                || path.contains("build-vulkan")
        })
        .unwrap_or(false)
}

fn resolve_whisper_gpu_layers() -> Option<usize> {
    parse_env_usize("TRISPR_WHISPER_GPU_LAYERS")
}

fn resolve_whisper_threads(gpu_hint: bool) -> usize {
    if let Some(explicit) = parse_env_usize("TRISPR_WHISPER_THREADS") {
        return explicit.max(1);
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    if gpu_hint {
        // GPU mode: keep CPU reserve to avoid UI stalls on Windows.
        let suggested = (cores / 2).max(2);
        return suggested.clamp(2, 8);
    }

    // CPU mode: avoid saturating all cores.
    cores.saturating_sub(1).clamp(2, 12)
}

fn whisper_cli_supports_gpu_layers(cli_path: &Path) -> bool {
    let mut probe = Command::new(cli_path);
    #[cfg(target_os = "windows")]
    probe.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let output = match probe.arg("--help").stdout(Stdio::piped()).stderr(Stdio::piped()).output() {
        Ok(output) => output,
        Err(err) => {
            warn!(
                "Failed to probe whisper-cli args for '{}': {}",
                cli_path.display(),
                err
            );
            return false;
        }
    };

    let help_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_lowercase();
    help_text.contains("-ngl") || help_text.contains("--gpu-layers")
}

fn whisper_runtime_missing_message(detail: &str) -> String {
    format!(
        "Whisper runtime is missing or incomplete ({}). Reinstall Trispr Flow and ensure whisper-cli exists in the installed runtime (bin\\\\cuda or bin\\\\vulkan).",
        detail
    )
}

fn whisper_runtime_dependency_message(cli_path: &Path, err: &std::io::Error) -> String {
    format!(
        "Whisper runtime executable was found at '{}', but Windows could not load required runtime files (possible DLL dependency issue: {}). Reinstall Trispr Flow.",
        cli_path.display(),
        err
    )
}

fn map_whisper_spawn_error(cli_path: &Path, err: std::io::Error) -> String {
    if !cli_path.exists() {
        return whisper_runtime_missing_message(&format!(
            "whisper-cli not found at '{}'",
            cli_path.display()
        ));
    }

    let code = err.raw_os_error();
    if matches!(err.kind(), ErrorKind::NotFound)
        || code == Some(2)
        || code == Some(126)
        || code == Some(193)
    {
        return whisper_runtime_dependency_message(cli_path, &err);
    }

    format!(
        "Failed to start Whisper runtime '{}': {}",
        cli_path.display(),
        err
    )
}

fn whisper_backend_from_cli_path(cli_path: &Path) -> &'static str {
    let lowered = cli_path.to_string_lossy().to_ascii_lowercase();
    if lowered.contains("/cuda/")
        || lowered.contains("\\cuda\\")
        || lowered.contains("build-cuda")
    {
        return "cuda";
    }
    if lowered.contains("/vulkan/")
        || lowered.contains("\\vulkan\\")
        || lowered.contains("build-vulkan")
    {
        return "vulkan";
    }
    "cpu"
}

fn whisper_stderr_indicates_gpu(stderr: &str) -> bool {
    let lowered = stderr.to_ascii_lowercase();
    lowered.contains("ggml_cuda_init")
        || lowered.contains("cuda devices")
        || lowered.contains("ggml_vulkan")
}

fn emit_transcription_gpu_activity(
    app: &AppHandle,
    state: &'static str,
    accelerator: &'static str,
    backend: &str,
    message: Option<String>,
) {
    let accel_code = if accelerator == "gpu" {
        TRANSCRIPTION_ACCEL_GPU
    } else {
        TRANSCRIPTION_ACCEL_CPU
    };
    LAST_TRANSCRIPTION_ACCELERATOR.store(accel_code, Ordering::Relaxed);
    let _ = app.emit(
        "transcription:gpu-activity",
        TranscriptionGpuActivityEvent {
            state,
            accelerator,
            backend: backend.to_string(),
            source: "whisper",
            message,
        },
    );
}

struct WhisperGpuActivityGuard {
    app: AppHandle,
    backend: String,
    accelerator: &'static str,
}

impl WhisperGpuActivityGuard {
    fn new(app: &AppHandle, accelerator: &'static str, backend: &str) -> Self {
        emit_transcription_gpu_activity(
            app,
            if accelerator == "gpu" { "active" } else { "cpu" },
            accelerator,
            backend,
            None,
        );
        Self {
            app: app.clone(),
            backend: backend.to_string(),
            accelerator,
        }
    }

    fn set_accelerator(&mut self, accelerator: &'static str) {
        if self.accelerator == accelerator {
            return;
        }
        self.accelerator = accelerator;
        emit_transcription_gpu_activity(
            &self.app,
            if accelerator == "gpu" { "active" } else { "cpu" },
            accelerator,
            &self.backend,
            None,
        );
    }
}

impl Drop for WhisperGpuActivityGuard {
    fn drop(&mut self) {
        emit_transcription_gpu_activity(
            &self.app,
            "idle",
            self.accelerator,
            &self.backend,
            None,
        );
    }
}

fn transcribe_local(
    app: &AppHandle,
    settings: &Settings,
    wav_bytes: &[u8],
) -> Result<String, String> {
    let temp_dir = std::env::temp_dir();
    let _ = fs::create_dir_all(&temp_dir);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let base = temp_dir.join(format!("trispr_{}_{}", std::process::id(), stamp));
    let wav_path = base.with_extension("wav");
    let output_base = base.clone();

    fs::write(&wav_path, wav_bytes).map_err(|e| {
        format!(
            "Failed to write temporary audio file '{}': {}",
            wav_path.display(),
            e
        )
    })?;

    let model_path = resolve_model_path(app, &settings.model).ok_or_else(|| {
        "Model file not found. Set TRISPR_WHISPER_MODEL_DIR or TRISPR_WHISPER_MODEL.".to_string()
    })?;

    let cli_path = resolve_whisper_cli_path().ok_or_else(|| {
        whisper_runtime_missing_message("whisper-cli executable could not be located")
    })?;
    if !cli_path.exists() {
        return Err(whisper_runtime_missing_message(&format!(
            "whisper-cli not found at '{}'",
            cli_path.display()
        )));
    }

    let mut command = Command::new(&cli_path);

    let gpu_layers = resolve_whisper_gpu_layers();
    let backend_gpu_capable = whisper_cli_looks_gpu_capable(Some(cli_path.as_path()));
    let gpu_hint = gpu_layers
        .map(|layers| layers > 0)
        .unwrap_or(backend_gpu_capable);
    let threads = resolve_whisper_threads(gpu_hint).to_string();

    // Hide console window on Windows
    #[cfg(target_os = "windows")]
    command.creation_flags(0x08000000 | 0x00004000); // CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS

    command
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(&wav_path)
        .arg("-t")
        .arg(threads)
        .arg("-l")
        .arg(if settings.language_pinned {
            &settings.language_mode
        } else {
            "auto"
        })
        .arg("-nt")
        .arg("-otxt")
        .arg("-of")
        .arg(&output_base)
        .arg("-np")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let requested_gpu_layers = gpu_layers.filter(|layers| *layers > 0);
    let mut applied_gpu_layers: Option<usize> = None;
    if let Some(layers) = requested_gpu_layers {
        if whisper_cli_supports_gpu_layers(cli_path.as_path()) {
            command.arg("-ngl").arg(layers.to_string());
            applied_gpu_layers = Some(layers);
        } else {
            warn!(
                "Ignoring TRISPR_WHISPER_GPU_LAYERS={} because whisper-cli '{}' does not support -ngl/--gpu-layers.",
                layers,
                cli_path.display()
            );
        }
    }

    let expected_gpu = if requested_gpu_layers.is_some() {
        applied_gpu_layers.is_some() || backend_gpu_capable
    } else {
        backend_gpu_capable
    };
    let backend = whisper_backend_from_cli_path(cli_path.as_path());
    let mut gpu_activity_guard =
        WhisperGpuActivityGuard::new(app, if expected_gpu { "gpu" } else { "cpu" }, backend);

    let output = command
        .output()
        .map_err(|e| map_whisper_spawn_error(cli_path.as_path(), e))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if whisper_stderr_indicates_gpu(&stderr) {
        gpu_activity_guard.set_accelerator("gpu");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stderr.to_lowercase().contains("unknown argument:") {
        return Err(format!(
            "whisper-cli argument mismatch ('{}'): {}",
            cli_path.display(),
            stderr.trim()
        ));
    }
    if !output.status.success() {
        let details = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(format!(
            "whisper-cli failed ('{}'): {}",
            cli_path.display(),
            details
        ));
    }

    let mut transcript_candidates: Vec<PathBuf> = Vec::new();
    let txt_path = output_base.with_extension("txt");
    push_unique_path(&mut transcript_candidates, txt_path.clone());
    push_unique_path(
        &mut transcript_candidates,
        Path::new(&format!("{}.txt", wav_path.display())).to_path_buf(),
    );
    push_unique_path(&mut transcript_candidates, wav_path.with_extension("txt"));
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = output_base.file_name().and_then(|name| name.to_str()) {
            push_unique_path(&mut transcript_candidates, cwd.join(format!("{name}.txt")));
        }
        if let Some(name) = wav_path.file_name().and_then(|name| name.to_str()) {
            push_unique_path(&mut transcript_candidates, cwd.join(format!("{name}.txt")));
        }
    }

    let mut transcript_path: Option<PathBuf> = None;
    let mut text: Option<String> = None;
    for _ in 0..20 {
        if let Some((path, value)) = read_first_existing_text_file(&transcript_candidates) {
            transcript_path = Some(path);
            text = Some(value);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    let stdout_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let text = if let Some(text) = text {
        text
    } else if !stdout_text.is_empty() {
        stdout_text
    } else {
        let stderr_text = String::from_utf8_lossy(&output.stderr);
        let expected = transcript_candidates
            .iter()
            .map(|path| format!("'{}'", path.display()))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Whisper finished without producing transcript output. Checked: {}. whisper-cli: '{}'. stderr: {}",
            expected,
            cli_path.display(),
            stderr_text.trim()
        ));
    };

    let _ = fs::remove_file(&wav_path);
    for path in &transcript_candidates {
        let _ = fs::remove_file(path);
    }
    if let Some(path) = transcript_path {
        let _ = fs::remove_file(path);
    }

    Ok(text.trim().to_string())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}

fn read_first_existing_text_file(paths: &[PathBuf]) -> Option<(PathBuf, String)> {
    let mut first_non_not_found: Option<(PathBuf, std::io::Error)> = None;
    for path in paths {
        match fs::read_to_string(path) {
            Ok(content) => return Some((path.clone(), content)),
            Err(err) if err.kind() == ErrorKind::NotFound => continue,
            Err(err) => {
                if first_non_not_found.is_none() {
                    first_non_not_found = Some((path.clone(), err));
                }
            }
        }
    }

    if let Some((path, err)) = first_non_not_found {
        warn!(
            "Failed reading whisper transcript candidate '{}': {}",
            path.display(),
            err
        );
    }
    None
}

#[derive(Deserialize)]
struct CloudResponse {
    text: String,
}

fn transcribe_cloud(wav_bytes: &[u8]) -> Result<String, String> {
    let endpoint = std::env::var("TRISPR_CLOUD_ENDPOINT").unwrap_or_default();
    if endpoint.trim().is_empty() {
        return Err("Legacy cloud transcription fallback is not configured".to_string());
    }

    let token = std::env::var("TRISPR_CLOUD_TOKEN").unwrap_or_default();
    let mut req = ureq::post(&endpoint).set("Content-Type", "audio/wav");
    if !token.trim().is_empty() {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }

    let resp = req.send_bytes(wav_bytes).map_err(|e| e.to_string())?;
    let parsed: CloudResponse = resp.into_json().map_err(|e| e.to_string())?;
    Ok(parsed.text)
}

#[cfg(target_os = "windows")]
fn resolve_output_device(device_id: &str) -> Option<wasapi::Device> {
    let enumerator = wasapi::DeviceEnumerator::new().ok()?;
    if device_id == "default" {
        return enumerator
            .get_default_device(&wasapi::Direction::Render)
            .ok();
    }

    if let Some(id) = device_id.strip_prefix("wasapi:") {
        if let Ok(device) = enumerator.get_device(id) {
            return Some(device);
        }
        tracing::warn!(
            "resolve_output_device: WASAPI device '{}' not found in enumerator, falling back to system default.",
            device_id
        );
    }

    enumerator
        .get_default_device(&wasapi::Direction::Render)
        .ok()
}
