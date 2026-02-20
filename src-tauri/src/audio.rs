use crate::constants::{
    TARGET_SAMPLE_RATE, VAD_MIN_CONSECUTIVE_CHUNKS, VAD_MIN_VOICE_MS,
};
use crate::continuous_dump::{AdaptiveSegmenter, AdaptiveSegmenterConfig, SegmentFlushReason};
use crate::overlay::{update_overlay_state, OverlayState};
use crate::postprocessing::process_transcript;
use crate::state::{push_history_entry_inner, AppState, Settings};
use crate::transcription::{
    rms_i16, should_drop_transcript, transcribe_audio, TranscriptionResult,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tracing::{error, info};

const MIC_MIN_AUDIO_MS: u64 = 120;

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct AudioDevice {
    pub(crate) id: String,
    pub(crate) label: String,
}

#[derive(Default)]
pub(crate) struct CaptureBuffer {
    samples: Vec<i16>,
    resample_pos: f64,
}

impl CaptureBuffer {
    pub(crate) fn reset(&mut self) {
        self.samples.clear();
        self.resample_pos = 0.0;
    }

    pub(crate) fn take_all_samples(&mut self) -> Vec<i16> {
        let mut out = Vec::new();
        std::mem::swap(&mut out, &mut self.samples);
        out
    }

    pub(crate) fn drain(&mut self) -> Vec<i16> {
        let mut out = Vec::new();
        std::mem::swap(&mut out, &mut self.samples);
        self.resample_pos = 0.0;
        out
    }

    pub(crate) fn push_samples(&mut self, input: &[f32], in_rate: u32) {
        if input.is_empty() {
            return;
        }

        if in_rate == TARGET_SAMPLE_RATE {
            for &sample in input {
                self.samples.push(float_to_i16(sample));
            }
            return;
        }

        let ratio = in_rate as f64 / TARGET_SAMPLE_RATE as f64;
        let mut pos = self.resample_pos;

        while pos + 1.0 < input.len() as f64 {
            let idx = pos.floor() as usize;
            let frac = pos - idx as f64;
            let a = input[idx] as f64;
            let b = input[idx + 1] as f64;
            let sample = (a * (1.0 - frac) + b * frac) as f32;
            self.samples.push(float_to_i16(sample));
            pos += ratio;
        }

        self.resample_pos = pos - input.len() as f64;
    }
}

fn float_to_i16(sample: f32) -> i16 {
    let clamped = sample.clamp(-1.0, 1.0);
    (clamped * i16::MAX as f32) as i16
}

fn mic_min_samples() -> usize {
    (TARGET_SAMPLE_RATE as u64 * MIC_MIN_AUDIO_MS / 1000) as usize
}

#[derive(Debug, Clone, serde::Serialize)]
struct ContinuousDumpEvent {
    source: &'static str,
    reason: SegmentFlushReason,
    duration_ms: u64,
    rms: f32,
    text_len: usize,
}

fn mic_segmenter_config(settings: &Settings) -> AdaptiveSegmenterConfig {
    let mut cfg = AdaptiveSegmenterConfig::from_profile(&settings.continuous_dump_profile);
    cfg.soft_flush_ms = if settings.continuous_mic_override_enabled {
        settings.continuous_mic_soft_flush_ms
    } else {
        settings.continuous_soft_flush_ms
    };
    cfg.silence_flush_ms = if settings.continuous_mic_override_enabled {
        settings.continuous_mic_silence_flush_ms
    } else {
        settings.continuous_silence_flush_ms
    };
    cfg.hard_cut_ms = if settings.continuous_mic_override_enabled {
        settings.continuous_mic_hard_cut_ms
    } else {
        settings.continuous_hard_cut_ms
    };
    cfg.min_chunk_ms = settings.continuous_min_chunk_ms;
    cfg.pre_roll_ms = settings.continuous_pre_roll_ms;
    cfg.post_roll_ms = settings.continuous_post_roll_ms;
    cfg.idle_keepalive_ms = settings.continuous_idle_keepalive_ms;
    cfg.threshold_start = settings.vad_threshold_start.max(0.001);
    cfg.threshold_sustain = settings
        .vad_threshold_sustain
        .clamp(0.001, settings.vad_threshold_start.max(0.001));
    cfg.clamp();
    cfg
}

pub(crate) struct Recorder {
    pub(crate) buffer: Arc<Mutex<CaptureBuffer>>,
    pub(crate) active: bool,
    pub(crate) transcribing: bool,
    pub(crate) stop_tx: Option<std::sync::mpsc::Sender<()>>,
    pub(crate) join_handle: Option<thread::JoinHandle<()>>,
    pub(crate) continuous_toggle_mode: bool,
    continuous_processor_stop_tx: Option<std::sync::mpsc::Sender<()>>,
    continuous_processor_join_handle: Option<thread::JoinHandle<()>>,
    vad_tx: Option<std::sync::mpsc::Sender<VadEvent>>,
    vad_runtime: Option<Arc<VadRuntime>>,
    pub(crate) input_gain_db: Arc<AtomicI64>,
}

impl Recorder {
    pub(crate) fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(CaptureBuffer::default())),
            active: false,
            transcribing: false,
            stop_tx: None,
            join_handle: None,
            continuous_toggle_mode: false,
            continuous_processor_stop_tx: None,
            continuous_processor_join_handle: None,
            vad_tx: None,
            vad_runtime: None,
            input_gain_db: Arc::new(AtomicI64::new(0)),
        }
    }

    pub(crate) fn update_vad_settings(
        &self,
        threshold_start: f32,
        threshold_sustain: f32,
        silence_ms: u64,
    ) {
        if let Some(runtime) = self.vad_runtime.as_ref() {
            runtime.update_thresholds(threshold_start, threshold_sustain);
            runtime.update_silence_ms(silence_ms);
        }
    }
}

struct DynamicThreshold {
    ambient_level: std::sync::atomic::AtomicU64,
    dynamic_threshold: std::sync::atomic::AtomicU64,
    min_threshold: f32,
    max_threshold: f32,
    ambient_multiplier: f32,
    rise_tau_ms: f32,
    fall_tau_ms: f32,
    last_update_ms: AtomicU64,
}

impl DynamicThreshold {
    fn new(min_threshold: f32, max_threshold: f32) -> Self {
        let initial_ambient = (min_threshold * 0.3 * 1_000_000.0) as u64;
        let initial_threshold = (min_threshold * 1_000_000.0) as u64;
        Self {
            ambient_level: std::sync::atomic::AtomicU64::new(initial_ambient),
            dynamic_threshold: std::sync::atomic::AtomicU64::new(initial_threshold),
            min_threshold,
            max_threshold: max_threshold.max(min_threshold),
            ambient_multiplier: 1.5,
            rise_tau_ms: 1000.0,
            fall_tau_ms: 300.0,
            last_update_ms: AtomicU64::new(0),
        }
    }

    fn update(&self, level: f32, now_ms: u64) -> f32 {
        let last = self.last_update_ms.swap(now_ms, Ordering::Relaxed);
        let dt_ms = now_ms.saturating_sub(last) as f32;
        if dt_ms <= 0.0 {
            return self.get_threshold();
        }

        let current_ambient = self.ambient_level.load(Ordering::Relaxed) as f32 / 1_000_000.0;

        let ambient_tau_ms = 1500.0;
        let ambient_alpha = 1.0 - (-dt_ms / ambient_tau_ms).exp();
        let new_ambient = current_ambient + (level - current_ambient) * ambient_alpha;
        self.ambient_level
            .store((new_ambient * 1_000_000.0) as u64, Ordering::Relaxed);

        let target_threshold = (new_ambient * self.ambient_multiplier).max(self.min_threshold);

        let current_threshold = self.dynamic_threshold.load(Ordering::Relaxed) as f32 / 1_000_000.0;

        let tau = if target_threshold > current_threshold {
            self.rise_tau_ms
        } else {
            self.fall_tau_ms
        };
        let alpha = 1.0 - (-dt_ms / tau).exp();
        let new_threshold = current_threshold + (target_threshold - current_threshold) * alpha;
        let clamped_threshold = new_threshold.clamp(self.min_threshold, self.max_threshold);

        self.dynamic_threshold
            .store((clamped_threshold * 1_000_000.0) as u64, Ordering::Relaxed);

        clamped_threshold
    }

    fn get_threshold(&self) -> f32 {
        self.dynamic_threshold.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }
}

struct OverlayLevelEmitter {
    app: AppHandle,
    start: Instant,
    last_emit_ms: AtomicU64,
    dynamic_threshold: DynamicThreshold,
    last_threshold_emit_ms: AtomicU64,
    smooth_level: AtomicU64,
    last_smooth_ms: AtomicU64,
}

impl OverlayLevelEmitter {
    fn new(app: AppHandle, min_sustain_threshold: f32, start_threshold: f32) -> Self {
        let max_threshold = start_threshold * 0.9;
        Self {
            app,
            start: Instant::now(),
            last_emit_ms: AtomicU64::new(0),
            dynamic_threshold: DynamicThreshold::new(min_sustain_threshold, max_threshold),
            last_threshold_emit_ms: AtomicU64::new(0),
            smooth_level: AtomicU64::new(0),
            last_smooth_ms: AtomicU64::new(0),
        }
    }

    fn emit_level(&self, level: f32) {
        let now_ms = self.start.elapsed().as_millis() as u64;
        let last = self.last_emit_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last) < 50 {
            return;
        }
        self.last_emit_ms.store(now_ms, Ordering::Relaxed);

        let level_clamped = level.clamp(0.0, 1.0);

        let dynamic_thresh = self.dynamic_threshold.update(level_clamped, now_ms);

        let _ = self.app.emit("audio:level", level_clamped);

        let last_thresh_emit = self.last_threshold_emit_ms.load(Ordering::Relaxed);
        if now_ms.saturating_sub(last_thresh_emit) >= 200 {
            self.last_threshold_emit_ms.store(now_ms, Ordering::Relaxed);
            let _ = self.app.emit("vad:dynamic-threshold", dynamic_thresh);
        }

        if let Some(window) = self.app.get_webview_window("overlay") {
            if let Ok(state) = self.app.state::<AppState>().settings.lock() {
                let (rise_ms, fall_ms) = if state.overlay_style == "kitt" {
                    (state.overlay_kitt_rise_ms, state.overlay_kitt_fall_ms)
                } else {
                    (state.overlay_rise_ms, state.overlay_fall_ms)
                };

                let last_smooth = self.last_smooth_ms.load(Ordering::Relaxed);
                let mut current = self.smooth_level.load(Ordering::Relaxed) as f32 / 1_000_000.0;
                if last_smooth == 0 {
                    current = level_clamped;
                } else {
                    let dt = now_ms.saturating_sub(last_smooth).max(1) as f32;
                    let tau: u64 = if level_clamped > current {
                        rise_ms
                    } else {
                        fall_ms
                    };
                    let denom = tau.max(1) as f32;
                    let max_step = (dt / denom).min(1.0);
                    let delta = level_clamped - current;
                    if delta.abs() <= max_step {
                        current = level_clamped;
                    } else {
                        current += max_step * delta.signum();
                    }
                }
                self.last_smooth_ms.store(now_ms, Ordering::Relaxed);
                let clamped = current.clamp(0.0, 1.0);
                self.smooth_level
                    .store((clamped * 1_000_000.0) as u64, Ordering::Relaxed);

                // Use window.setOverlayLevel() for both KITT and Dot modes
                // This delegates the rendering logic to the frontend, which is cleaner
                let js = format!(
                    "if(window.setOverlayLevel){{window.setOverlayLevel({});}}",
                    clamped
                );
                let _ = window.eval(&js);
            }
        }
    }
}

#[derive(Debug)]
struct VadRuntime {
    recording: std::sync::atomic::AtomicBool,
    pending_flush: std::sync::atomic::AtomicBool,
    processing: std::sync::atomic::AtomicBool,
    flush_on_silence: bool,
    last_voice_ms: AtomicU64,
    start_ms: AtomicU64,
    audio_cues: bool,
    threshold_start_scaled: AtomicU64,
    threshold_sustain_scaled: AtomicU64,
    silence_ms: AtomicU64,
    consecutive_above: AtomicU64,
}

impl VadRuntime {
    fn new(
        audio_cues: bool,
        threshold_start: f32,
        threshold_sustain: f32,
        silence_ms: u64,
        flush_on_silence: bool,
    ) -> Self {
        let start_scaled = (threshold_start.clamp(0.001, 0.5) * 1_000_000.0) as u64;
        let sustain_scaled = (threshold_sustain.clamp(0.001, 0.5) * 1_000_000.0) as u64;
        Self {
            recording: std::sync::atomic::AtomicBool::new(false),
            pending_flush: std::sync::atomic::AtomicBool::new(false),
            processing: std::sync::atomic::AtomicBool::new(false),
            flush_on_silence,
            last_voice_ms: AtomicU64::new(0),
            start_ms: AtomicU64::new(0),
            audio_cues,
            threshold_start_scaled: AtomicU64::new(start_scaled),
            threshold_sustain_scaled: AtomicU64::new(sustain_scaled),
            silence_ms: AtomicU64::new(silence_ms.max(100)),
            consecutive_above: AtomicU64::new(0),
        }
    }

    fn threshold_start(&self) -> f32 {
        self.threshold_start_scaled.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }

    fn threshold_sustain(&self) -> f32 {
        self.threshold_sustain_scaled.load(Ordering::Relaxed) as f32 / 1_000_000.0
    }

    fn update_thresholds(&self, threshold_start: f32, threshold_sustain: f32) {
        let start_scaled = (threshold_start.clamp(0.001, 0.5) * 1_000_000.0) as u64;
        let sustain_scaled = (threshold_sustain.clamp(0.001, 0.5) * 1_000_000.0) as u64;
        self.threshold_start_scaled
            .store(start_scaled, Ordering::Relaxed);
        self.threshold_sustain_scaled
            .store(sustain_scaled, Ordering::Relaxed);
    }

    fn silence_ms(&self) -> u64 {
        self.silence_ms.load(Ordering::Relaxed)
    }

    fn update_silence_ms(&self, silence_ms: u64) {
        self.silence_ms
            .store(silence_ms.max(100), Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
enum VadEvent {
    Finalize(Vec<i16>),
}

#[derive(Clone)]
struct VadHandle {
    runtime: Arc<VadRuntime>,
    tx: std::sync::mpsc::Sender<VadEvent>,
    app: AppHandle,
}

#[tauri::command]
pub(crate) fn list_audio_devices() -> Vec<AudioDevice> {
    let mut devices = vec![AudioDevice {
        id: "default".to_string(),
        label: "Default (System)".to_string(),
    }];

    let host = cpal::default_host();
    if let Ok(inputs) = host.input_devices() {
        for (index, device) in inputs.enumerate() {
            let name = device
                .name()
                .unwrap_or_else(|_| format!("Input {}", index + 1));
            let id = format!("input-{}-{}", index, name);
            devices.push(AudioDevice { id, label: name });
        }
    }

    devices
}

#[tauri::command]
pub(crate) fn list_output_devices() -> Vec<AudioDevice> {
    let mut devices = vec![AudioDevice {
        id: "default".to_string(),
        label: "System Default Output".to_string(),
    }];

    #[cfg(target_os = "windows")]
    {
        if let Ok(enumerator) = wasapi::DeviceEnumerator::new() {
            if let Ok(collection) = enumerator.get_device_collection(&wasapi::Direction::Render) {
                if let Ok(count) = collection.get_nbr_devices() {
                    for index in 0..count {
                        if let Ok(device) = collection.get_device_at_index(index) {
                            let name = device
                                .get_friendlyname()
                                .unwrap_or_else(|_| format!("Output {}", index + 1));
                            let id = device.get_id().unwrap_or_else(|_| format!("idx-{index}"));
                            devices.push(AudioDevice {
                                id: format!("wasapi:{id}"),
                                label: name,
                            });
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let host = cpal::default_host();
        if let Ok(outputs) = host.output_devices() {
            for (index, device) in outputs.enumerate() {
                let name = device
                    .name()
                    .unwrap_or_else(|_| format!("Output {}", index + 1));
                let id = format!("output-{}-{}", index, name);
                devices.push(AudioDevice { id, label: name });
            }
        }
    }

    devices
}

fn resolve_input_device(device_id: &str) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if device_id == "default" {
        return host.default_input_device();
    }

    if let Ok(inputs) = host.input_devices() {
        for (index, device) in inputs.enumerate() {
            let name = device
                .name()
                .unwrap_or_else(|_| format!("Input {}", index + 1));
            let id = format!("input-{}-{}", index, name);
            if id == device_id {
                return Some(device);
            }
        }
    }

    host.default_input_device()
}

fn push_mono_samples(buffer: &Arc<Mutex<CaptureBuffer>>, mono: &[f32], sample_rate: u32) {
    if let Ok(mut guard) = buffer.lock() {
        guard.push_samples(mono, sample_rate);
    }
}

fn handle_vad_audio(
    vad_handle: &VadHandle,
    buffer: &Arc<Mutex<CaptureBuffer>>,
    mono: Vec<f32>,
    level: f32,
    sample_rate: u32,
) {
    let runtime = &vad_handle.runtime;
    let now = crate::util::now_ms();
    let is_recording = runtime.recording.load(Ordering::Relaxed);

    let threshold = if is_recording {
        runtime.threshold_sustain()
    } else {
        runtime.threshold_start()
    };

    if level >= threshold {
        let consecutive = runtime.consecutive_above.fetch_add(1, Ordering::Relaxed) + 1;
        runtime.last_voice_ms.store(now, Ordering::Relaxed);

        if !is_recording && consecutive >= VAD_MIN_CONSECUTIVE_CHUNKS {
            runtime.recording.store(true, Ordering::Relaxed);
            runtime.start_ms.store(now, Ordering::Relaxed);
            runtime.pending_flush.store(false, Ordering::Relaxed);
            if let Ok(mut buf) = buffer.lock() {
                buf.reset();
            }
            let _ = vad_handle.app.emit("capture:state", "recording");
            let _ = update_overlay_state(&vad_handle.app, OverlayState::Recording);
            if runtime.audio_cues {
                let _ = vad_handle.app.emit("audio:cue", "start");
            }
        }
    } else if !is_recording {
        runtime.consecutive_above.store(0, Ordering::Relaxed);
    }

    if runtime.recording.load(Ordering::Relaxed) {
        push_mono_samples(buffer, &mono, sample_rate);

        let last = runtime.last_voice_ms.load(Ordering::Relaxed);
        let start = runtime.start_ms.load(Ordering::Relaxed);
        let silence_ms = runtime.silence_ms();
        if runtime.flush_on_silence
            && now.saturating_sub(last) > silence_ms
            && now.saturating_sub(start) > VAD_MIN_VOICE_MS
        {
            if !runtime.pending_flush.swap(true, Ordering::Relaxed) {
                runtime.recording.store(false, Ordering::Relaxed);
                let samples = {
                    let mut buf = buffer.lock().unwrap();
                    buf.drain()
                };
                let _ = vad_handle.tx.send(VadEvent::Finalize(samples));
            }
        }
    }
}

fn build_input_stream_f32(
    device: &cpal::Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<CaptureBuffer>>,
    overlay: Option<Arc<OverlayLevelEmitter>>,
    vad: Option<VadHandle>,
    gain_db: Arc<AtomicI64>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0;
    let err_fn = |err| eprintln!("audio stream error: {}", err);

    device
        .build_input_stream(
            config,
            move |data: &[f32], _| {
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                let mut sum_squared = 0.0f32;
                let gain_db = gain_db.load(Ordering::Relaxed) as f32 / 1000.0;
                let gain = (10.0f32).powf(gain_db / 20.0);
                for frame in data.chunks(channels.max(1)) {
                    let mut sum = 0.0f32;
                    for &sample in frame {
                        sum += sample;
                    }
                    let sample = (sum / channels.max(1) as f32 * gain).clamp(-1.0, 1.0);
                    mono.push(sample);
                    sum_squared += sample * sample;
                }
                let level = if mono.is_empty() {
                    0.0
                } else {
                    let rms = (sum_squared / mono.len() as f32).sqrt();
                    (rms * 2.5).min(1.0)
                };
                if let Some(emitter) = overlay.as_ref() {
                    emitter.emit_level(level);
                }
                if let Some(vad_handle) = vad.as_ref() {
                    handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
                } else {
                    push_mono_samples(&buffer, &mono, sample_rate);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| e.to_string())
}

fn build_input_stream_i16(
    device: &cpal::Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<CaptureBuffer>>,
    overlay: Option<Arc<OverlayLevelEmitter>>,
    vad: Option<VadHandle>,
    gain_db: Arc<AtomicI64>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0;
    let err_fn = |err| eprintln!("audio stream error: {}", err);

    device
        .build_input_stream(
            config,
            move |data: &[i16], _| {
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                let mut sum_squared = 0.0f32;
                let gain_db = gain_db.load(Ordering::Relaxed) as f32 / 1000.0;
                let gain = (10.0f32).powf(gain_db / 20.0);
                for frame in data.chunks(channels.max(1)) {
                    let mut sum = 0.0f32;
                    for &sample in frame {
                        sum += sample as f32 / i16::MAX as f32;
                    }
                    let sample = (sum / channels.max(1) as f32 * gain).clamp(-1.0, 1.0);
                    mono.push(sample);
                    sum_squared += sample * sample;
                }
                let level = if mono.is_empty() {
                    0.0
                } else {
                    let rms = (sum_squared / mono.len() as f32).sqrt();
                    (rms * 2.5).min(1.0)
                };
                if let Some(emitter) = overlay.as_ref() {
                    emitter.emit_level(level);
                }
                if let Some(vad_handle) = vad.as_ref() {
                    handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
                } else {
                    push_mono_samples(&buffer, &mono, sample_rate);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| e.to_string())
}

fn build_input_stream_u16(
    device: &cpal::Device,
    config: &StreamConfig,
    buffer: Arc<Mutex<CaptureBuffer>>,
    overlay: Option<Arc<OverlayLevelEmitter>>,
    vad: Option<VadHandle>,
    gain_db: Arc<AtomicI64>,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate.0;
    let err_fn = |err| eprintln!("audio stream error: {}", err);

    device
        .build_input_stream(
            config,
            move |data: &[u16], _| {
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                let mut sum_squared = 0.0f32;
                let gain_db = gain_db.load(Ordering::Relaxed) as f32 / 1000.0;
                let gain = (10.0f32).powf(gain_db / 20.0);
                for frame in data.chunks(channels.max(1)) {
                    let mut sum = 0.0f32;
                    for &sample in frame {
                        let centered = sample as f32 - 32768.0;
                        sum += centered / 32768.0;
                    }
                    let sample = (sum / channels.max(1) as f32 * gain).clamp(-1.0, 1.0);
                    mono.push(sample);
                    sum_squared += sample * sample;
                }
                let level = if mono.is_empty() {
                    0.0
                } else {
                    let rms = (sum_squared / mono.len() as f32).sqrt();
                    (rms * 2.5).min(1.0)
                };
                if let Some(emitter) = overlay.as_ref() {
                    emitter.emit_level(level);
                }
                if let Some(vad_handle) = vad.as_ref() {
                    handle_vad_audio(vad_handle, &buffer, mono, level, sample_rate);
                } else {
                    push_mono_samples(&buffer, &mono, sample_rate);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| e.to_string())
}

pub(crate) fn start_recording_with_settings(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &Settings,
) -> Result<(), String> {
    info!("start_recording_with_settings called");
    if !settings.capture_enabled {
        return Ok(());
    }
    let mut recorder = state.recorder.lock().unwrap();
    if recorder.active || recorder.transcribing {
        info!("Recording already active or transcribing, skipping");
        return Ok(());
    }

    if let Ok(mut buf) = recorder.buffer.lock() {
        buf.reset();
    }

    recorder.input_gain_db.store(
        (settings.mic_input_gain_db * 1000.0) as i64,
        Ordering::Relaxed,
    );
    let gain_db = recorder.input_gain_db.clone();
    let buffer = recorder.buffer.clone();
    let overlay_emitter = Arc::new(OverlayLevelEmitter::new(
        app.clone(),
        settings.vad_threshold_sustain,
        settings.vad_threshold_start,
    ));
    let device_id = settings.input_device.clone();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let join_handle = thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let device = resolve_input_device(&device_id)
                .ok_or_else(|| "No input device available".to_string())?;
            let config = device.default_input_config().map_err(|e| e.to_string())?;
            let stream_config: StreamConfig = config.clone().into();

            let overlay = Some(overlay_emitter);
            let vad = None;
            let stream = match config.sample_format() {
                SampleFormat::F32 => build_input_stream_f32(
                    &device,
                    &stream_config,
                    buffer,
                    overlay.clone(),
                    vad.clone(),
                    gain_db.clone(),
                )?,
                SampleFormat::I16 => build_input_stream_i16(
                    &device,
                    &stream_config,
                    buffer,
                    overlay.clone(),
                    vad.clone(),
                    gain_db.clone(),
                )?,
                SampleFormat::U16 => build_input_stream_u16(
                    &device,
                    &stream_config,
                    buffer,
                    overlay.clone(),
                    vad.clone(),
                    gain_db.clone(),
                )?,
                _ => return Err("Unsupported sample format".to_string()),
            };

            stream.play().map_err(|e| e.to_string())?;
            let _ = ready_tx.send(Ok(()));

            let _ = stop_rx.recv();
            drop(stream);
            Ok(())
        })();

        if let Err(err) = result {
            let _ = ready_tx.send(Err(err));
        }
    });

    let start_result = match ready_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("Failed to start audio stream".to_string()),
    };

    if let Err(err) = start_result {
        error!("Failed to start recording: {}", err);
        let _ = stop_tx.send(());
        let _ = join_handle.join();
        return Err(err);
    }

    recorder.stop_tx = Some(stop_tx);
    recorder.join_handle = Some(join_handle);
    recorder.active = true;
    recorder.continuous_toggle_mode = false;
    recorder.continuous_processor_stop_tx = None;
    recorder.continuous_processor_join_handle = None;

    info!("Recording started successfully, updating overlay");
    let _ = app.emit("capture:state", "recording");
    let _ = update_overlay_state(app, OverlayState::Recording);

    if settings.audio_cues {
        let _ = app.emit("audio:cue", "start");
    }

    Ok(())
}

fn flush_mic_audio_to_session(buffer: &mut Vec<i16>) {
    if buffer.is_empty() {
        return;
    }
    let duration_ms = buffer.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;
    if duration_ms < 10_000 {
        buffer.clear();
        return;
    }
    if let Err(err) = crate::session_manager::flush_chunk(buffer, "mic") {
        error!("Failed to flush mic chunk: {}", err);
    }
    buffer.clear();
}

fn process_toggle_segment(
    app_handle: &AppHandle,
    runtime_settings: &Settings,
    chunk: Vec<i16>,
    reason: SegmentFlushReason,
    segment_rms: f32,
    duration_ms: u64,
) {
    if chunk.is_empty() {
        return;
    }

    let _ = app_handle.emit("capture:state", "transcribing");
    let _ = update_overlay_state(app_handle, OverlayState::Transcribing);

    if let Ok(mut recorder) = app_handle.state::<AppState>().recorder.lock() {
        recorder.transcribing = true;
    }

    let result = transcribe_audio(app_handle, runtime_settings, &chunk);

    if let Ok(mut recorder) = app_handle.state::<AppState>().recorder.lock() {
        recorder.transcribing = false;
    }

    match result {
        Ok((text, source)) => {
            if !text.trim().is_empty()
                && !should_drop_transcript(&text, segment_rms, duration_ms)
                && !crate::transcription::should_drop_by_activation_words(
                    &text,
                    &runtime_settings.activation_words,
                    runtime_settings.activation_words_enabled,
                )
            {
                let processed_text = if runtime_settings.postproc_enabled {
                    match process_transcript(&text, runtime_settings, app_handle) {
                        Ok(processed) => processed,
                        Err(err) => {
                            error!("Post-processing failed: {}", err);
                            text.clone()
                        }
                    }
                } else {
                    text.clone()
                };

                let state = app_handle.state::<AppState>();
                if let Ok(updated) = push_history_entry_inner(
                    app_handle,
                    &state.history,
                    processed_text.clone(),
                    source.clone(),
                ) {
                    let _ = app_handle.emit("history:updated", updated);
                }

                let _ = app_handle.emit(
                    "transcription:result",
                    TranscriptionResult {
                        text: processed_text.clone(),
                        source: source.clone(),
                    },
                );

                let _ = app_handle.emit(
                    "continuous-dump:segment",
                    ContinuousDumpEvent {
                        source: "mic",
                        reason,
                        duration_ms,
                        rms: segment_rms,
                        text_len: processed_text.len(),
                    },
                );

                if let Err(err) = crate::paste_text(&processed_text) {
                    let _ = app_handle.emit("transcription:error", err);
                }
            }
        }
        Err(err) => {
            let _ = app_handle.emit("transcription:error", err);
        }
    }

    let is_active = app_handle
        .state::<AppState>()
        .recorder
        .lock()
        .map(|r| r.active)
        .unwrap_or(false);
    if is_active {
        let _ = app_handle.emit("capture:state", "recording");
        let _ = update_overlay_state(app_handle, OverlayState::Recording);
    } else {
        let _ = app_handle.emit("capture:state", "idle");
        let _ = update_overlay_state(app_handle, OverlayState::Idle);
    }
}

fn run_toggle_processor(
    app_handle: AppHandle,
    initial_settings: Settings,
    buffer: Arc<Mutex<CaptureBuffer>>,
    stop_rx: std::sync::mpsc::Receiver<()>,
) {
    let min_samples = mic_min_samples();
    let mut segmenter = AdaptiveSegmenter::new(mic_segmenter_config(&initial_settings));
    let mut last_settings_check = Instant::now();
    let mut runtime_settings = initial_settings;

    let auto_save = runtime_settings.auto_save_mic_audio && runtime_settings.opus_enabled;
    let mut save_buffer: Vec<i16> = Vec::new();
    let flush_threshold = TARGET_SAMPLE_RATE as usize * 60;

    if auto_save {
        let recordings_dir = crate::paths::resolve_recordings_dir(&app_handle);
        crate::session_manager::init(recordings_dir);
    }

    loop {
        match stop_rx.try_recv() {
            Ok(_) | Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }

        if last_settings_check.elapsed() >= Duration::from_millis(200) {
            if let Ok(settings) = app_handle.state::<AppState>().settings.lock() {
                runtime_settings = settings.clone();
                segmenter.update_config(mic_segmenter_config(&runtime_settings));
            }
            last_settings_check = Instant::now();
        }

        let samples = {
            if let Ok(mut guard) = buffer.lock() {
                guard.take_all_samples()
            } else {
                Vec::new()
            }
        };

        if samples.is_empty() {
            thread::sleep(Duration::from_millis(30));
            continue;
        }

        let level = rms_i16(&samples);
        let segments = segmenter.push_samples(&samples, level);
        for mut segment in segments {
            if auto_save {
                save_buffer.extend_from_slice(&segment.samples);
                if save_buffer.len() >= flush_threshold {
                    flush_mic_audio_to_session(&mut save_buffer);
                }
            }

            if segment.samples.len() < min_samples {
                continue;
            }

            let duration_ms = segment.duration_ms;
            let segment_rms = segment.rms;
            let reason = segment.reason;
            let chunk = std::mem::take(&mut segment.samples);
            process_toggle_segment(
                &app_handle,
                &runtime_settings,
                chunk,
                reason,
                segment_rms,
                duration_ms,
            );
        }
    }

    let leftover = {
        if let Ok(mut guard) = buffer.lock() {
            guard.take_all_samples()
        } else {
            Vec::new()
        }
    };
    if !leftover.is_empty() {
        for mut segment in segmenter.push_samples(&leftover, 0.0) {
            if auto_save {
                save_buffer.extend_from_slice(&segment.samples);
            }
            let chunk = std::mem::take(&mut segment.samples);
            if chunk.len() < min_samples {
                continue;
            }
            process_toggle_segment(
                &app_handle,
                &runtime_settings,
                chunk,
                segment.reason,
                segment.rms,
                segment.duration_ms,
            );
        }
    }
    for mut segment in segmenter.finalize() {
        if auto_save {
            save_buffer.extend_from_slice(&segment.samples);
        }
        let chunk = std::mem::take(&mut segment.samples);
        if chunk.len() < min_samples {
            continue;
        }
        process_toggle_segment(
            &app_handle,
            &runtime_settings,
            chunk,
            segment.reason,
            segment.rms,
            segment.duration_ms,
        );
    }

    if auto_save {
        flush_mic_audio_to_session(&mut save_buffer);
        match crate::session_manager::finalize_for("mic") {
            Ok(Some(path)) => {
                let state = app_handle.state::<AppState>();
                *state.last_mic_recording_path.lock().unwrap() =
                    Some(path.to_string_lossy().to_string());
            }
            Ok(None) => {}
            Err(err) => error!("Failed to finalize mic audio session: {}", err),
        }
    }
}

fn start_toggle_recording_with_settings(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &Settings,
) -> Result<(), String> {
    start_recording_with_settings(app, state, settings)?;

    let (buffer, stop_rx) = {
        let mut recorder = state.recorder.lock().unwrap();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        recorder.continuous_toggle_mode = true;
        recorder.continuous_processor_stop_tx = Some(tx);
        (recorder.buffer.clone(), rx)
    };

    let app_handle = app.clone();
    let settings_clone = settings.clone();
    let handle = thread::spawn(move || {
        run_toggle_processor(app_handle, settings_clone, buffer, stop_rx);
    });

    let mut recorder = state.recorder.lock().unwrap();
    recorder.continuous_processor_join_handle = Some(handle);
    Ok(())
}

pub(crate) fn stop_toggle_recording_async(app: AppHandle, state: &State<'_, AppState>) {
    let app_handle = app.clone();
    let settings = state.settings.lock().unwrap().clone();

    thread::spawn(move || {
        let state = app_handle.state::<AppState>();
        let (capture_stop_tx, capture_join_handle, proc_stop_tx, proc_join_handle) = {
            let mut recorder = state.recorder.lock().unwrap();
            if !recorder.active {
                return;
            }
            recorder.active = false;
            recorder.transcribing = false;
            recorder.continuous_toggle_mode = false;
            (
                recorder.stop_tx.take(),
                recorder.join_handle.take(),
                recorder.continuous_processor_stop_tx.take(),
                recorder.continuous_processor_join_handle.take(),
            )
        };

        if let Some(tx) = capture_stop_tx {
            let _ = tx.send(());
        }
        if let Some(tx) = proc_stop_tx {
            let _ = tx.send(());
        }
        if let Some(handle) = capture_join_handle {
            let _ = handle.join();
        }
        if let Some(handle) = proc_join_handle {
            let _ = handle.join();
        }

        let _ = app_handle.emit("capture:state", "idle");
        let _ = update_overlay_state(&app_handle, OverlayState::Idle);
        if settings.audio_cues {
            let _ = app_handle.emit("audio:cue", "stop");
        }
    });
}

pub(crate) fn start_vad_monitor(
    app: &AppHandle,
    state: &State<'_, AppState>,
    settings: &Settings,
) -> Result<(), String> {
    info!("start_vad_monitor called");
    if !settings.capture_enabled {
        return Ok(());
    }
    let mut recorder = state.recorder.lock().unwrap();
    if recorder.active || recorder.transcribing {
        info!("VAD already active or transcribing, skipping");
        return Ok(());
    }

    if let Ok(mut buf) = recorder.buffer.lock() {
        buf.reset();
    }

    recorder.input_gain_db.store(
        (settings.mic_input_gain_db * 1000.0) as i64,
        Ordering::Relaxed,
    );
    let gain_db = recorder.input_gain_db.clone();
    let buffer = recorder.buffer.clone();
    let overlay_emitter = Arc::new(OverlayLevelEmitter::new(
        app.clone(),
        settings.vad_threshold_sustain,
        settings.vad_threshold_start,
    ));
    let device_id = settings.input_device.clone();
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let (vad_tx, vad_rx) = std::sync::mpsc::channel::<VadEvent>();

    let flush_on_silence = settings.mode == "vad";
    let vad_runtime = Arc::new(VadRuntime::new(
        settings.audio_cues,
        settings.vad_threshold_start,
        settings.vad_threshold_sustain,
        settings.vad_silence_ms,
        flush_on_silence,
    ));
    let vad_handle = VadHandle {
        runtime: vad_runtime.clone(),
        tx: vad_tx.clone(),
        app: app.clone(),
    };

    let app_handle = app.clone();
    let settings_clone = settings.clone();
    let vad_runtime_clone = vad_runtime.clone();
    thread::spawn(move || {
        for event in vad_rx {
            match event {
                VadEvent::Finalize(samples) => {
                    process_vad_segment(
                        app_handle.clone(),
                        settings_clone.clone(),
                        samples,
                        vad_runtime_clone.clone(),
                    );
                }
            }
        }
    });

    let join_handle = thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let device = resolve_input_device(&device_id)
                .ok_or_else(|| "No input device available".to_string())?;
            let config = device.default_input_config().map_err(|e| e.to_string())?;
            let stream_config: StreamConfig = config.clone().into();

            let overlay = Some(overlay_emitter);
            let vad = Some(vad_handle);
            let gain_db = gain_db.clone();
            let stream = match config.sample_format() {
                SampleFormat::F32 => build_input_stream_f32(
                    &device,
                    &stream_config,
                    buffer,
                    overlay.clone(),
                    vad.clone(),
                    gain_db.clone(),
                )?,
                SampleFormat::I16 => build_input_stream_i16(
                    &device,
                    &stream_config,
                    buffer,
                    overlay.clone(),
                    vad.clone(),
                    gain_db.clone(),
                )?,
                SampleFormat::U16 => build_input_stream_u16(
                    &device,
                    &stream_config,
                    buffer,
                    overlay.clone(),
                    vad.clone(),
                    gain_db.clone(),
                )?,
                _ => return Err("Unsupported sample format".to_string()),
            };

            stream.play().map_err(|e| e.to_string())?;
            let _ = ready_tx.send(Ok(()));

            let _ = stop_rx.recv();
            drop(stream);
            Ok(())
        })();

        if let Err(err) = result {
            let _ = ready_tx.send(Err(err));
        }
    });

    let start_result = match ready_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err("Failed to start audio stream".to_string()),
    };

    if let Err(err) = start_result {
        error!("Failed to start VAD monitor: {}", err);
        let _ = stop_tx.send(());
        let _ = join_handle.join();
        return Err(err);
    }

    recorder.stop_tx = Some(stop_tx);
    recorder.join_handle = Some(join_handle);
    recorder.active = true;
    recorder.vad_tx = Some(vad_tx);
    recorder.vad_runtime = Some(vad_runtime);

    let _ = app.emit("capture:state", "idle");
    let _ = update_overlay_state(app, OverlayState::Idle);
    Ok(())
}

pub(crate) fn stop_vad_monitor(app: &AppHandle, state: &State<'_, AppState>) {
    let (buffer, stop_tx, join_handle, vad_tx, vad_runtime) = {
        let mut recorder = state.recorder.lock().unwrap();
        if !recorder.active {
            return;
        }
        recorder.active = false;
        (
            recorder.buffer.clone(),
            recorder.stop_tx.take(),
            recorder.join_handle.take(),
            recorder.vad_tx.take(),
            recorder.vad_runtime.take(),
        )
    };

    let should_flush_on_stop = vad_runtime
        .as_ref()
        .map(|runtime| {
            runtime.recording.load(Ordering::Relaxed)
                && !runtime.pending_flush.load(Ordering::Relaxed)
        })
        .unwrap_or(false);

    if let Some(runtime) = vad_runtime.as_ref() {
        runtime.recording.store(false, Ordering::Relaxed);
        runtime.processing.store(false, Ordering::Relaxed);
    }

    if let Some(tx) = stop_tx {
        let _ = tx.send(());
    }
    if let Some(join_handle) = join_handle {
        let _ = join_handle.join();
    }

    if should_flush_on_stop {
        if let (Some(tx), Some(runtime)) = (vad_tx.as_ref(), vad_runtime.as_ref()) {
            let samples = {
                let mut buf = buffer.lock().unwrap();
                buf.drain()
            };
            if !samples.is_empty() {
                runtime.pending_flush.store(true, Ordering::Relaxed);
                let _ = tx.send(VadEvent::Finalize(samples));
            }
        }
    }

    if let Some(runtime) = vad_runtime.as_ref() {
        if !should_flush_on_stop {
            runtime.pending_flush.store(false, Ordering::Relaxed);
        }
    }

    drop(vad_tx);

    let _ = app.emit("capture:state", "idle");
    let _ = update_overlay_state(app, OverlayState::Idle);
}

fn process_vad_segment(
    app_handle: AppHandle,
    settings: Settings,
    samples: Vec<i16>,
    runtime: Arc<VadRuntime>,
) {
    let state = app_handle.state::<AppState>();
    if samples.is_empty() {
        runtime.pending_flush.store(false, Ordering::Relaxed);
        return;
    }
    if let Ok(mut recorder) = state.recorder.lock() {
        recorder.transcribing = true;
    }

    let min_samples = mic_min_samples();
    if samples.len() < min_samples {
        let _ = app_handle.emit("capture:state", "idle");
        let _ = update_overlay_state(&app_handle, OverlayState::Idle);
        let _ = app_handle.emit(
            "transcription:error",
            format!(
                "Audio too short ({} ms). Speak a bit longer.",
                (samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64)
            ),
        );
        runtime.processing.store(false, Ordering::Relaxed);
        runtime.pending_flush.store(false, Ordering::Relaxed);
        if let Ok(mut recorder) = state.recorder.lock() {
            recorder.transcribing = false;
        }
        return;
    }

    runtime.processing.store(true, Ordering::Relaxed);
    let _ = app_handle.emit("capture:state", "transcribing");
    let _ = update_overlay_state(&app_handle, OverlayState::Transcribing);

    let result = transcribe_audio(&app_handle, &settings, &samples);
    let level = rms_i16(&samples);
    let duration_ms = samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

    if let Ok(mut recorder) = state.recorder.lock() {
        recorder.transcribing = false;
    }

    runtime.processing.store(false, Ordering::Relaxed);
    runtime.pending_flush.store(false, Ordering::Relaxed);

    if runtime.recording.load(Ordering::Relaxed) {
        let _ = app_handle.emit("capture:state", "recording");
        let _ = update_overlay_state(&app_handle, OverlayState::Recording);
    } else {
        let _ = app_handle.emit("capture:state", "idle");
        let _ = update_overlay_state(&app_handle, OverlayState::Idle);
    }

    if settings.audio_cues {
        let _ = app_handle.emit("audio:cue", "stop");
    }

    match result {
        Ok((text, source)) => {
            let settings = state.settings.lock().unwrap().clone();
            if !text.trim().is_empty()
                && !should_drop_transcript(&text, level, duration_ms)
                && !crate::transcription::should_drop_by_activation_words(
                    &text,
                    &settings.activation_words,
                    settings.activation_words_enabled,
                )
            {
                // Apply post-processing if enabled
                let processed_text = if settings.postproc_enabled {
                    match process_transcript(&text, &settings, &app_handle) {
                        Ok(processed) => processed,
                        Err(e) => {
                            error!("Post-processing failed: {}", e);
                            text.clone() // Fallback to original
                        }
                    }
                } else {
                    text.clone()
                };

                if let Ok(updated) = push_history_entry_inner(
                    &app_handle,
                    &state.history,
                    processed_text.clone(),
                    source.clone(),
                ) {
                    let _ = app_handle.emit("history:updated", updated);
                }
                let _ = app_handle.emit(
                    "transcription:result",
                    TranscriptionResult {
                        text: processed_text.clone(),
                        source: source.clone(),
                    },
                );
                if let Err(err) = crate::paste_text(&processed_text) {
                    let _ = app_handle.emit("transcription:error", err);
                }
            }
        }
        Err(err) => {
            let _ = app_handle.emit("transcription:error", err);
        }
    }
}

pub(crate) fn stop_recording_async(app: AppHandle, state: &State<'_, AppState>) {
    let app_handle = app.clone();
    let settings = state.settings.lock().unwrap().clone();

    thread::spawn(move || {
        info!("stop_recording_async called");
        let state = app_handle.state::<AppState>();
        let (buffer, stop_tx, join_handle, proc_stop_tx, proc_join_handle) = {
            let mut recorder = state.recorder.lock().unwrap();
            if !recorder.active {
                info!("Recording not active, skipping stop");
                return;
            }
            recorder.active = false;
            recorder.transcribing = true;
            recorder.continuous_toggle_mode = false;
            let stop_tx = recorder.stop_tx.take();
            let join_handle = recorder.join_handle.take();
            let proc_stop_tx = recorder.continuous_processor_stop_tx.take();
            let proc_join_handle = recorder.continuous_processor_join_handle.take();
            (
                recorder.buffer.clone(),
                stop_tx,
                join_handle,
                proc_stop_tx,
                proc_join_handle,
            )
        };

        if let Some(tx) = stop_tx {
            let _ = tx.send(());
        }
        if let Some(tx) = proc_stop_tx {
            let _ = tx.send(());
        }
        if let Some(join_handle) = join_handle {
            let _ = join_handle.join();
        }
        if let Some(join_handle) = proc_join_handle {
            let _ = join_handle.join();
            let _ = app_handle.emit("capture:state", "idle");
            let _ = update_overlay_state(&app_handle, OverlayState::Idle);
            if settings.audio_cues {
                let _ = app_handle.emit("audio:cue", "stop");
            }
            return;
        }

        let samples = {
            let mut buf = buffer.lock().unwrap();
            buf.drain()
        };

        let min_samples = mic_min_samples();
        if samples.len() < min_samples {
            let _ = app_handle.emit("capture:state", "idle");
            let _ = update_overlay_state(&app_handle, OverlayState::Idle);
            let _ = app_handle.emit(
                "transcription:error",
                format!(
                    "Audio too short ({} ms). Hold PTT a bit longer.",
                    (samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64)
                ),
            );
            let mut recorder = state.recorder.lock().unwrap();
            recorder.transcribing = false;
            return;
        }

        let _ = app_handle.emit("capture:state", "transcribing");
        let _ = update_overlay_state(&app_handle, OverlayState::Transcribing);

        let result = transcribe_audio(&app_handle, &settings, &samples);
        let level = rms_i16(&samples);
        let duration_ms = samples.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

        // Save recording as OPUS for later VibeVoice analysis
        // Only save if duration > 10 seconds (avoid short dictations)
        if duration_ms >= 10_000 {
            if let Ok(opus_path) = crate::save_recording_opus(&app_handle, &samples, "mic", None) {
                let state_ref = app_handle.state::<crate::state::AppState>();
                *state_ref.last_mic_recording_path.lock().unwrap() = Some(opus_path);
            }
        }

        let mut recorder = state.recorder.lock().unwrap();
        recorder.transcribing = false;
        drop(recorder);

        let _ = app_handle.emit("capture:state", "idle");
        let _ = update_overlay_state(&app_handle, OverlayState::Idle);

        if settings.audio_cues {
            let _ = app_handle.emit("audio:cue", "stop");
        }

        match result {
            Ok((text, source)) => {
                let settings = state.settings.lock().unwrap().clone();
                if !text.trim().is_empty()
                    && !should_drop_transcript(&text, level, duration_ms)
                    && !crate::transcription::should_drop_by_activation_words(
                        &text,
                        &settings.activation_words,
                        settings.activation_words_enabled,
                    )
                {
                    // Apply post-processing if enabled
                    let processed_text = if settings.postproc_enabled {
                        match process_transcript(&text, &settings, &app_handle) {
                            Ok(processed) => processed,
                            Err(e) => {
                                error!("Post-processing failed: {}", e);
                                text.clone() // Fallback to original
                            }
                        }
                    } else {
                        text.clone()
                    };

                    if let Ok(updated) = push_history_entry_inner(
                        &app_handle,
                        &state.history,
                        processed_text.clone(),
                        source.clone(),
                    ) {
                        let _ = app_handle.emit("history:updated", updated);
                    }
                    let _ = app_handle.emit(
                        "transcription:result",
                        TranscriptionResult {
                            text: processed_text.clone(),
                            source: source.clone(),
                        },
                    );
                    if let Err(err) = crate::paste_text(&processed_text) {
                        let _ = app_handle.emit("transcription:error", err);
                    }
                }
            }
            Err(err) => {
                let _ = app_handle.emit("transcription:error", err);
            }
        }
    });
}

pub(crate) fn handle_ptt_press(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    if !settings.capture_enabled {
        return Ok(());
    }
    if settings.mode != "ptt" {
        return Ok(());
    }

    if settings.ptt_use_vad {
        start_vad_monitor(app, &state, &settings)
    } else {
        start_recording_with_settings(app, &state, &settings)
    }
}

pub(crate) fn handle_ptt_release_async(app: AppHandle) {
    let app_handle = app.clone();
    let state = app_handle.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    if settings.mode != "ptt" {
        return;
    }

    if settings.ptt_use_vad {
        stop_vad_monitor(&app, &state);
    } else {
        stop_recording_async(app, &state);
    }
}

pub(crate) fn handle_toggle_async(app: AppHandle) {
    let app_handle = app.clone();
    let state = app_handle.state::<AppState>();
    let settings = state.settings.lock().unwrap().clone();
    if !settings.capture_enabled {
        return;
    }
    if settings.mode != "ptt" {
        return;
    }

    let (active, continuous_toggle_mode) = {
        let recorder = state.recorder.lock().unwrap();
        (recorder.active, recorder.continuous_toggle_mode)
    };
    if active {
        if continuous_toggle_mode {
            stop_toggle_recording_async(app, &state);
        } else {
            stop_recording_async(app, &state);
        }
    } else {
        if settings.continuous_dump_enabled {
            let _ = start_toggle_recording_with_settings(&app, &state, &settings);
        } else {
            let _ = start_recording_with_settings(&app, &state, &settings);
        }
    }
}

#[tauri::command]
pub(crate) fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.settings.lock().unwrap().clone();
    start_recording_with_settings(&app, &state, &settings)
}

#[tauri::command]
pub(crate) fn stop_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    stop_recording_async(app, &state);
    Ok(())
}
