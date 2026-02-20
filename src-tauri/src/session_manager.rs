// Session Manager — Audio chunk consolidation and session lifecycle
//
// Problem: System audio transcription flushes a new OPUS file every 60 seconds,
// producing hundreds of files per day. This module introduces the concept of a
// "session" (transcription mode ON → OFF) that accumulates chunks in a temp
// directory and merges them into a single `session.opus` at session end.
//
// File layout during recording:
//   recordings/tmp_20260217_143022_output/
//       chunk_001_0000s.opus
//       chunk_002_0060s.opus
//       manifest.json          ← status: "recording"
//
// File layout after merge:
//   recordings/2026-02-17_143022_output/
//       session.opus
//       manifest.json          ← status: "merged"

use chrono::Local;
use hound::{SampleFormat, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tracing::{error, info, warn};

// ─────────────────────────────────────────────────────────────────────────────
// Data structures
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMeta {
    pub index: usize,
    pub file: String, // filename relative to session_dir
    pub offset_s: u64,
    pub duration_s: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionManifest {
    pub version: u8,
    pub session_id: String,
    pub session_name: Option<String>,
    pub source: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_s: u64,
    pub status: String, // "recording" | "merging" | "merged" | "merge_failed"
    pub final_file: Option<String>,
    pub chunks: Vec<ChunkMeta>,
}

// ─────────────────────────────────────────────────────────────────────────────
// ActiveSession
// ─────────────────────────────────────────────────────────────────────────────

pub struct ActiveSession {
    pub session_id: String,
    pub session_dir: PathBuf,
    pub source: String,
    pub session_name: Option<String>,
    pub chunks: Vec<ChunkMeta>,
    pub started_at_str: String,
}

impl ActiveSession {
    fn total_duration_s(&self) -> u64 {
        self.chunks.iter().map(|c| c.duration_s).sum()
    }

    fn write_manifest(&self, status: &str, final_file: Option<&str>, ended_at: Option<&str>) {
        let manifest = SessionManifest {
            version: 1,
            session_id: self.session_id.clone(),
            session_name: self.session_name.clone(),
            source: self.source.clone(),
            started_at: self.started_at_str.clone(),
            ended_at: ended_at.map(String::from),
            duration_s: self.total_duration_s(),
            status: status.to_string(),
            final_file: final_file.map(String::from),
            chunks: self.chunks.clone(),
        };
        let path = self.session_dir.join("manifest.json");
        match serde_json::to_string_pretty(&manifest) {
            Ok(json) => {
                if let Err(e) = fs::write(&path, json) {
                    error!("Failed to write session manifest: {}", e);
                }
            }
            Err(e) => error!("Failed to serialize session manifest: {}", e),
        }
    }

    /// Flush a batch of i16 samples as a new OPUS chunk.
    /// Writes temp WAV → FFmpeg encode → deletes WAV, appends ChunkMeta.
    pub fn flush_chunk(&mut self, samples: &[i16]) -> Result<ChunkMeta, String> {
        let duration_s = samples.len() as u64 / 16_000;
        let offset_s = self.total_duration_s();
        let index = self.chunks.len() + 1;
        let chunk_base = format!("chunk_{:03}_{:04}s", index, offset_s);

        let wav_path = self.session_dir.join(format!("{}.wav", chunk_base));
        let opus_path = self.session_dir.join(format!("{}.opus", chunk_base));

        // Write WAV
        write_wav_i16(&wav_path, samples)?;

        // Encode WAV → OPUS
        let ffmpeg = crate::opus::find_ffmpeg()
            .map_err(|e| format!("FFmpeg not found for chunk encoding: {}", e))?;

        let encode_ok = std::process::Command::new(&ffmpeg)
            .args(&[
                "-y",
                "-i",
                wav_path.to_str().unwrap_or(""),
                "-c:a",
                "libopus",
                "-b:a",
                "64k",
                "-ar",
                "16000",
                "-ac",
                "1",
                opus_path.to_str().unwrap_or(""),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        let _ = fs::remove_file(&wav_path);

        if !encode_ok {
            return Err(format!("FFmpeg failed encoding chunk {}", index));
        }

        let meta = ChunkMeta {
            index,
            file: format!("{}.opus", chunk_base),
            offset_s,
            duration_s,
        };
        self.chunks.push(meta.clone());
        self.write_manifest("recording", None, None);
        info!(
            "Session chunk {}/{}: {} s at offset {} s",
            index, self.session_id, duration_s, offset_s
        );
        Ok(meta)
    }

    /// Merge all chunks into a single session.opus via FFmpeg concat.
    /// On success: renames temp dir → final dir, cleans up chunks.
    /// On failure: leaves temp dir intact for crash recovery.
    pub fn finalize(self, recordings_dir: &PathBuf) -> Result<PathBuf, String> {
        if self.chunks.is_empty() {
            warn!(
                "Session {} has no chunks, discarding temp dir",
                self.session_id
            );
            let _ = fs::remove_dir_all(&self.session_dir);
            return Err("No chunks to merge".to_string());
        }

        // Write concat file list (paths relative to session_dir for FFmpeg -safe 0)
        let concat_path = self.session_dir.join("concat.txt");
        let list: String = self
            .chunks
            .iter()
            .map(|c| format!("file '{}'\n", c.file))
            .collect();
        fs::write(&concat_path, &list)
            .map_err(|e| format!("Failed to write concat list: {}", e))?;

        // Build final directory name
        let final_name = if let Some(ref name) = self.session_name {
            let date = Local::now().format("%Y-%m-%d").to_string();
            format!("{}_{}", date, sanitize_name(name))
        } else {
            self.session_id.clone()
        };
        let final_dir = recordings_dir.join(&final_name);
        fs::create_dir_all(&final_dir)
            .map_err(|e| format!("Failed to create final session dir: {}", e))?;
        let final_opus = final_dir.join("session.opus");

        let ffmpeg =
            crate::opus::find_ffmpeg().map_err(|e| format!("FFmpeg not found for merge: {}", e))?;

        let ended_at = Local::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let merge_ok = std::process::Command::new(&ffmpeg)
            .current_dir(&self.session_dir)
            .args(&[
                "-y",
                "-f",
                "concat",
                "-safe",
                "0",
                "-i",
                concat_path.to_str().unwrap_or("concat.txt"),
                "-c",
                "copy",
                final_opus.to_str().unwrap_or("session.opus"),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if !merge_ok {
            // Leave temp dir intact — user can retry or recover manually
            self.write_manifest("merge_failed", None, Some(&ended_at));
            return Err(format!(
                "FFmpeg concat failed for session {}",
                self.session_id
            ));
        }

        // Write final manifest to the permanent directory
        let final_manifest = SessionManifest {
            version: 1,
            session_id: self.session_id.clone(),
            session_name: self.session_name.clone(),
            source: self.source.clone(),
            started_at: self.started_at_str.clone(),
            ended_at: Some(ended_at),
            duration_s: self.total_duration_s(),
            status: "merged".to_string(),
            final_file: Some("session.opus".to_string()),
            chunks: self.chunks.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&final_manifest) {
            let _ = fs::write(final_dir.join("manifest.json"), json);
        }

        // Clean up temp dir after successful merge
        let _ = fs::remove_dir_all(&self.session_dir);

        info!(
            "Session {} merged → {:?} ({} s)",
            self.session_id,
            final_opus,
            self.total_duration_s()
        );
        Ok(final_opus)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SessionManager (global singleton)
// ─────────────────────────────────────────────────────────────────────────────

pub struct SessionManager {
    active: HashMap<String, ActiveSession>,
    recordings_dir: Option<PathBuf>,
}

impl SessionManager {
    fn new() -> Self {
        Self {
            active: HashMap::new(),
            recordings_dir: None,
        }
    }

    pub fn set_recordings_dir(&mut self, dir: PathBuf) {
        self.recordings_dir = Some(dir);
    }

    /// Start a new session for the given audio source.
    /// If a session for the same source is already active, it's a no-op.
    pub fn start_session(
        &mut self,
        source: &str,
        session_name: Option<&str>,
    ) -> Result<(), String> {
        if self.active.contains_key(source) {
            return Ok(());
        }

        let recordings_dir = self
            .recordings_dir
            .clone()
            .ok_or_else(|| "Recordings directory not configured".to_string())?;
        fs::create_dir_all(&recordings_dir)
            .map_err(|e| format!("Cannot create recordings dir: {}", e))?;

        let now = Local::now();
        let session_id = format!("{}_{}", now.format("%Y-%m-%d_%H%M%S"), source);
        let tmp_dir_name = format!("tmp_{}_{}", now.format("%Y%m%d_%H%M%S"), source);
        let session_dir = recordings_dir.join(&tmp_dir_name);

        fs::create_dir_all(&session_dir)
            .map_err(|e| format!("Cannot create session temp dir {:?}: {}", session_dir, e))?;

        let started_at = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let session = ActiveSession {
            session_id: session_id.clone(),
            session_dir,
            source: source.to_string(),
            session_name: session_name.map(String::from),
            chunks: Vec::new(),
            started_at_str: started_at,
        };
        session.write_manifest("recording", None, None);
        info!("Audio session started: {}", session_id);
        self.active.insert(source.to_string(), session);
        Ok(())
    }

    /// Flush samples as a new chunk (auto-starts session if needed).
    pub fn flush_chunk(&mut self, samples: &[i16], source: &str) -> Result<(), String> {
        if !self.active.contains_key(source) {
            self.start_session(source, None)?;
        }
        if let Some(session) = self.active.get_mut(source) {
            session.flush_chunk(samples)?;
        }
        Ok(())
    }

    /// Finalize one source-specific active session: merge → session.opus, cleanup temp dir.
    /// Returns the path to the merged file, or None if no session for this source was active.
    pub fn finalize_session_for(&mut self, source: &str) -> Result<Option<PathBuf>, String> {
        let Some(session) = self.active.remove(source) else {
            return Ok(None);
        };

        let recordings_dir = self
            .recordings_dir
            .clone()
            .ok_or_else(|| "Recordings directory not configured".to_string())?;
        match session.finalize(&recordings_dir) {
            Ok(path) => Ok(Some(path)),
            Err(e) => Err(e),
        }
    }

}

// ─────────────────────────────────────────────────────────────────────────────
// Global API
// ─────────────────────────────────────────────────────────────────────────────

static SESSION_MANAGER: OnceLock<Mutex<SessionManager>> = OnceLock::new();

fn get() -> &'static Mutex<SessionManager> {
    SESSION_MANAGER.get_or_init(|| Mutex::new(SessionManager::new()))
}

/// Call once at app startup (or when transcription mode is activated).
pub fn init(recordings_dir: PathBuf) {
    if let Ok(mut mgr) = get().lock() {
        mgr.set_recordings_dir(recordings_dir);
    }
}

/// Flush audio samples as a new session chunk.
pub fn flush_chunk(samples: &[i16], source: &str) -> Result<(), String> {
    get()
        .lock()
        .map_err(|e| e.to_string())?
        .flush_chunk(samples, source)
}

/// Finalize the active session for a specific source and return the merged file path.
pub fn finalize_for(source: &str) -> Result<Option<PathBuf>, String> {
    get()
        .lock()
        .map_err(|e| e.to_string())?
        .finalize_session_for(source)
}

/// Scan for incomplete (crash-recovered) sessions in the recordings directory.
pub fn scan_incomplete(recordings_dir: &PathBuf) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(recordings_dir) else {
        return vec![];
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("tmp_"))
                    .unwrap_or(false)
                && p.join("manifest.json").exists()
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn write_wav_i16(path: &PathBuf, samples: &[i16]) -> Result<(), String> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)
        .map_err(|e| format!("Cannot create chunk WAV {:?}: {}", path, e))?;
    for &s in samples {
        writer
            .write_sample(s)
            .map_err(|e| format!("WAV write error: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize error: {}", e))?;
    Ok(())
}

fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim_matches('_').to_string();
    if trimmed.len() > 40 {
        trimmed[..40].to_string()
    } else {
        trimmed
    }
}
