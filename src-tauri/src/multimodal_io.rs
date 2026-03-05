use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionSourceInfo {
    pub id: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionStreamHealth {
    pub running: bool,
    pub fps: u8,
    pub source_scope: String,
    pub started_at_ms: Option<u64>,
    pub frame_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionSnapshotResult {
    pub captured: bool,
    pub timestamp_ms: u64,
    pub source_count: usize,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsProviderInfo {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsVoiceInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSpeakRequest {
    pub provider: String,
    pub text: String,
    pub rate: Option<f32>,
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsSpeakResult {
    pub provider_used: String,
    pub accepted: bool,
    pub message: String,
}

pub fn list_tts_providers() -> Vec<TtsProviderInfo> {
    vec![
        TtsProviderInfo {
            id: "windows_native".to_string(),
            label: "Windows Native TTS".to_string(),
            available: cfg!(target_os = "windows"),
        },
        TtsProviderInfo {
            id: "local_custom".to_string(),
            label: "Local Custom TTS".to_string(),
            available: true,
        },
    ]
}

#[cfg(target_os = "windows")]
fn list_windows_voices() -> Vec<TtsVoiceInfo> {
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.GetInstalledVoices() | ForEach-Object { $_.VoiceInfo.Name }",
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let mut voices = Vec::new();
            for line in text.lines() {
                let label = line.trim();
                if label.is_empty() {
                    continue;
                }
                voices.push(TtsVoiceInfo {
                    id: label.to_string(),
                    label: label.to_string(),
                    provider: "windows_native".to_string(),
                });
            }
            if voices.is_empty() {
                voices.push(TtsVoiceInfo {
                    id: "windows_default".to_string(),
                    label: "Windows Default Voice".to_string(),
                    provider: "windows_native".to_string(),
                });
            }
            voices
        }
        _ => vec![TtsVoiceInfo {
            id: "windows_default".to_string(),
            label: "Windows Default Voice".to_string(),
            provider: "windows_native".to_string(),
        }],
    }
}

#[cfg(not(target_os = "windows"))]
fn list_windows_voices() -> Vec<TtsVoiceInfo> {
    Vec::new()
}

pub fn list_tts_voices(provider: &str) -> Vec<TtsVoiceInfo> {
    match provider {
        "windows_native" => list_windows_voices(),
        "local_custom" => vec![
            TtsVoiceInfo {
                id: "local_custom_default".to_string(),
                label: "Local Custom Voice (default)".to_string(),
                provider: "local_custom".to_string(),
            },
            TtsVoiceInfo {
                id: "local_custom_neutral".to_string(),
                label: "Local Custom Voice (neutral)".to_string(),
                provider: "local_custom".to_string(),
            },
        ],
        _ => Vec::new(),
    }
}

#[cfg(target_os = "windows")]
pub fn speak_windows_native(text: &str, rate: f32, volume: f32) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("TTS text is empty.".to_string());
    }

    let rate = rate.clamp(0.5, 2.0);
    let volume = volume.clamp(0.0, 1.0);
    let sapi_rate = (((rate - 1.0) * 10.0).round() as i32).clamp(-10, 10);
    let sapi_volume = ((volume * 100.0).round() as i32).clamp(0, 100);
    let escaped_text = text.replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName System.Speech; $s = New-Object System.Speech.Synthesis.SpeechSynthesizer; $s.Rate = {sapi_rate}; $s.Volume = {sapi_volume}; $s.Speak('{escaped_text}')"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .map_err(|error| format!("Failed to start Windows TTS: {}", error))?;
    if !output.status.success() {
        return Err(format!(
            "Windows TTS failed with status {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn speak_windows_native(_text: &str, _rate: f32, _volume: f32) -> Result<(), String> {
    Err("Windows native TTS is only available on Windows.".to_string())
}
