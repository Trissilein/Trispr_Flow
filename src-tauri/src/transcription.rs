use crate::audio::CaptureBuffer;
use crate::constants::{MIN_AUDIO_MS, TARGET_SAMPLE_RATE};
#[cfg(target_os = "windows")]
use crate::constants::{TRANSCRIBE_IDLE_METER_MS, TRANSCRIBE_QUEUE_MAX_CHUNKS};
use crate::errors::AppError;
use crate::models::resolve_model_path;
use crate::overlay::{update_overlay_state, OverlayState};
use crate::paths::resolve_whisper_cli_path;
use crate::state::{push_transcribe_entry_inner, AppState, Settings};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TranscriptionResult {
  pub(crate) text: String,
  pub(crate) source: String,
}

#[derive(Default)]
pub(crate) struct TranscribeRecorder {
  pub(crate) active: bool,
  pub(crate) stop_tx: Option<std::sync::mpsc::Sender<()>>,
  pub(crate) join_handle: Option<thread::JoinHandle<()>>,
}

struct AudioQueue {
  inner: Mutex<VecDeque<Vec<i16>>>,
  cond: Condvar,
  max_chunks: usize,
  closed: AtomicBool,
}

impl AudioQueue {
  fn new(max_chunks: usize) -> Arc<Self> {
    Arc::new(Self {
      inner: Mutex::new(VecDeque::new()),
      cond: Condvar::new(),
      max_chunks: max_chunks.max(1),
      closed: AtomicBool::new(false),
    })
  }

  fn push(&self, chunk: Vec<i16>) {
    let mut queue = self.inner.lock().unwrap();
    if queue.len() >= self.max_chunks {
      queue.pop_front();
    }
    queue.push_back(chunk);
    self.cond.notify_one();
  }

  fn pop(&self) -> Option<Vec<i16>> {
    let mut queue = self.inner.lock().unwrap();
    loop {
      if let Some(chunk) = queue.pop_front() {
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
}

#[cfg(test)]
mod tests {
  use super::AudioQueue;

  #[test]
  fn audio_queue_drops_oldest_when_full() {
    let queue = AudioQueue::new(2);
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
    let queue = AudioQueue::new(1);
    queue.close();
    assert!(queue.pop().is_none());
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
    }
  }
}

pub(crate) fn start_transcribe_monitor(
  app: &AppHandle,
  state: &State<'_, AppState>,
  settings: &Settings,
) -> Result<(), String> {
  let mut recorder = state.transcribe.lock().unwrap();
  if recorder.active {
    return Ok(());
  }

  let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
  let app_handle = app.clone();
  let settings = settings.clone();

  let join_handle = thread::spawn(move || {
    #[cfg(target_os = "windows")]
    {
      if let Err(err) = run_transcribe_loopback(app_handle.clone(), settings, stop_rx) {
        crate::emit_error(&app_handle, AppError::AudioDevice(err), Some("System Audio"));
        let state = app_handle.state::<AppState>();
        state.transcribe_active.store(false, Ordering::Relaxed);
        if let Ok(mut transcribe) = state.transcribe.lock() {
          transcribe.active = false;
          transcribe.stop_tx = None;
          transcribe.join_handle = None;
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
        AppError::AudioDevice("System audio capture is not supported on this OS yet.".to_string()),
        Some("System Audio"),
      );
      let state = app_handle.state::<AppState>();
      state.transcribe_active.store(false, Ordering::Relaxed);
      if let Ok(mut transcribe) = state.transcribe.lock() {
        transcribe.active = false;
        transcribe.stop_tx = None;
        transcribe.join_handle = None;
      }
      let _ = app_handle.emit("transcribe:state", "idle");
      emit_transcribe_idle(&app_handle);
      update_transcribe_overlay(&app_handle, false);
    }
  });

  recorder.active = true;
  recorder.stop_tx = Some(stop_tx);
  recorder.join_handle = Some(join_handle);
  state.transcribe_active.store(true, Ordering::Relaxed);

  emit_transcribe_idle(app);
  let _ = app.emit("transcribe:state", "recording");
  Ok(())
}

pub(crate) fn stop_transcribe_monitor(app: &AppHandle, state: &State<'_, AppState>) {
  let (stop_tx, join_handle) = {
    let mut recorder = state.transcribe.lock().unwrap();
    recorder.active = false;
    (recorder.stop_tx.take(), recorder.join_handle.take())
  };

  state.transcribe_active.store(false, Ordering::Relaxed);
  let _ = app.emit("transcribe:state", "idle");
  update_transcribe_overlay(app, false);
  emit_transcribe_idle(app);

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
  text
    .chars()
    .filter(|c| c.is_alphanumeric() || c.is_whitespace())
    .collect::<String>()
    .to_lowercase()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

pub(crate) fn should_drop_transcript(text: &str, rms: f32, duration_ms: u64) -> bool {
  let normalized = normalize_transcript(text);
  if normalized.is_empty() {
    return true;
  }
  let word_count = normalized.split_whitespace().count();
  let is_short = word_count <= crate::constants::HALLUCINATION_MAX_WORDS
    || normalized.len() <= crate::constants::HALLUCINATION_MAX_CHARS;
  let is_low_energy = rms < crate::constants::HALLUCINATION_RMS_THRESHOLD;
  let is_short_audio = duration_ms <= crate::constants::HALLUCINATION_MAX_DURATION_MS;
  let matches_common = HALLUCINATION_PHRASES.iter().any(|p| *p == normalized);

  if matches_common && is_short_audio {
    return true;
  }

  is_low_energy && is_short_audio && is_short
}

const HALLUCINATION_PHRASES: &[&str] = &[
  "you",
  "thank you",
  "thanks",
  "okay",
  "ok",
  "yeah",
  "yes",
  "no",
  "uh",
  "um",
  "hmm",
  "huh",
];

fn transcribe_worker(app: AppHandle, settings: Settings, queue: Arc<AudioQueue>) {
  let min_samples = (TARGET_SAMPLE_RATE as u64 * MIN_AUDIO_MS / 1000) as usize;
  while let Some(chunk) = queue.pop() {
    if chunk.len() < min_samples {
      continue;
    }
    let level = rms_i16(&chunk);
    let duration_ms = chunk.len() as u64 * 1000 / TARGET_SAMPLE_RATE as u64;

    if settings.transcribe_vad_mode {
      if level < settings.transcribe_vad_threshold {
        continue;
      }
    }

    let _ = app.emit("transcribe:state", "transcribing");
    update_transcribe_overlay(&app, true);
    let result = transcribe_audio(&app, &settings, &chunk);

    if app.state::<AppState>().transcribe_active.load(Ordering::Relaxed) {
      let _ = app.emit("transcribe:state", "recording");
    }
    update_transcribe_overlay(&app, false);

    match result {
      Ok((text, _source)) => {
        if !text.trim().is_empty() && !should_drop_transcript(&text, level, duration_ms) {
          let state = app.state::<AppState>();
          let _ = push_transcribe_entry_inner(&app, &state.history_transcribe, text);
        }
      }
      Err(err) => {
        let _ = app.emit("transcription:error", err);
      }
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
            let value = i16::from_le_bytes([sample[0], sample[1]]) as f32 / i16::MAX as f32;
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
            let value = ((sample[2] as i32) << 24 | (sample[1] as i32) << 16 | (sample[0] as i32) << 8) >> 8;
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
            let value = i32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]) as f32
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
) -> Result<(), String> {
  let hr = wasapi::initialize_mta();
  if hr.0 < 0 {
    return Err(format!("WASAPI init error: 0x{:X}", hr.0));
  }
  let device = resolve_output_device(&settings.transcribe_output_device)
    .ok_or_else(|| "Output device not found".to_string())?;
  let mut audio_client = device
    .get_iaudioclient()
    .map_err(|e| format!("WASAPI error: {e}"))?;

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

  let chunk_samples =
    (TARGET_SAMPLE_RATE as u64 * settings.transcribe_batch_interval_ms / 1000) as usize;
  let overlap_samples =
    (TARGET_SAMPLE_RATE as u64 * settings.transcribe_chunk_overlap_ms / 1000) as usize;
  let mut gain = (10.0f32).powf(settings.transcribe_input_gain_db / 20.0);
  let mut vad_enabled = settings.transcribe_vad_mode;
  let mut vad_threshold = settings.transcribe_vad_threshold;
  let mut vad_silence_ms = settings.transcribe_vad_silence_ms;
  let mut last_settings_check = Instant::now();
  let mut vad_last_hit_ms = Instant::now();

  let queue = AudioQueue::new(TRANSCRIBE_QUEUE_MAX_CHUNKS);
  let worker_app = app.clone();
  let worker_settings = settings.clone();
  let worker_queue = queue.clone();
  let worker_handle = thread::spawn(move || {
    transcribe_worker(worker_app, worker_settings, worker_queue);
  });

  let mut buffer = CaptureBuffer::default();
  let mut smooth_level = 0.0f32;
  let mut last_emit = Instant::now();
  let mut last_idle_emit = Instant::now();

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

    buffer.push_samples(&mono, sample_rate);
    while buffer.len() >= chunk_samples {
      let chunk = buffer.take_chunk(chunk_samples, overlap_samples);
      if chunk.is_empty() {
        break;
      }
      if vad_enabled {
        if vad_last_hit_ms.elapsed() <= Duration::from_millis(vad_silence_ms) {
          queue.push(chunk);
        }
      } else {
        queue.push(chunk);
      }
    }
  }

  let leftover = buffer.drain();
  if !leftover.is_empty() {
    queue.push(leftover);
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

  if settings.cloud_fallback {
    let text = transcribe_cloud(&wav_bytes)?;
    return Ok((text, "cloud".to_string()));
  }

  let text = transcribe_local(app, settings, &wav_bytes)?;
  Ok((text, "local".to_string()))
}

fn transcribe_local(
  app: &AppHandle,
  settings: &Settings,
  wav_bytes: &[u8],
) -> Result<String, String> {
  let temp_dir = std::env::temp_dir();
  let stamp = crate::util::now_ms();
  let base = temp_dir.join(format!("trispr_{}", stamp));
  let wav_path = base.with_extension("wav");
  let output_base = base.clone();

  fs::write(&wav_path, wav_bytes).map_err(|e| e.to_string())?;

  let model_path = resolve_model_path(app, &settings.model).ok_or_else(|| {
    "Model file not found. Set TRISPR_WHISPER_MODEL_DIR or TRISPR_WHISPER_MODEL.".to_string()
  })?;

  let cli_path = resolve_whisper_cli_path();

  let mut command = if let Some(path) = cli_path {
    Command::new(path)
  } else {
    Command::new("whisper-cli")
  };

  let threads = std::thread::available_parallelism()
    .map(|n| n.get().to_string())
    .unwrap_or_else(|_| "4".to_string());

  command
    .arg("-m")
    .arg(model_path)
    .arg("-f")
    .arg(&wav_path)
    .arg("-t")
    .arg(threads)
    .arg("-l")
    .arg("auto")
    .arg("-nt")
    .arg("-otxt")
    .arg("-of")
    .arg(&output_base)
    .arg("-np")
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

  let output = command.output().map_err(|e| e.to_string())?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(format!("whisper-cli failed: {stderr}"));
  }

  let txt_path = output_base.with_extension("txt");
  let text = fs::read_to_string(&txt_path).map_err(|e| e.to_string())?;

  let _ = fs::remove_file(&wav_path);
  let _ = fs::remove_file(&txt_path);

  Ok(text.trim().to_string())
}

#[derive(Deserialize)]
struct CloudResponse {
  text: String,
}

fn transcribe_cloud(wav_bytes: &[u8]) -> Result<String, String> {
  let endpoint = std::env::var("TRISPR_CLOUD_ENDPOINT").unwrap_or_default();
  if endpoint.trim().is_empty() {
    return Err("Cloud fallback not configured".to_string());
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
    return enumerator.get_default_device(&wasapi::Direction::Render).ok();
  }

  if let Some(id) = device_id.strip_prefix("wasapi:") {
    if let Ok(device) = enumerator.get_device(id) {
      return Some(device);
    }
  }

  enumerator.get_default_device(&wasapi::Direction::Render).ok()
}

#[cfg(not(target_os = "windows"))]
fn resolve_output_device(_device_id: &str) -> Option<cpal::Device> {
  None
}
