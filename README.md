# Trispr Flow

> GPU-first offline dictation + system audio transcription, privacy-first by default

[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Trispr Flow is a modern desktop dictation app built with Tauri + Rust + TypeScript. It combines local GPU-accelerated transcription (whisper.cpp) with a compact, responsive UI and optional cloud fallback.

## ✨ Features (Current)
- **🎙️ Microphone capture (PTT + VAD)** with global hotkeys
- **⚡ GPU-accelerated transcription** (whisper.cpp) + CPU fallback
- **🔊 System audio capture (Windows / WASAPI loopback)** with a dedicated transcribe hotkey
- **🧾 Output tabs** for Microphone, System Audio, and a combined Conversation view
- **📈 Output meters** with dB readout + adjustable thresholds + system input gain
- **🔒 Privacy-first** (offline by default; cloud fallback is opt-in)

## 🚧 Work in progress
- Overlay dot (audio-reactive size, opacity, color reliability)
- Model Manager revamp (sources, install/remove, custom URLs)
- Conversation detach window stability

## 🚀 Quick Start (Dev)
```bash
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
npm install
npm run tauri dev
```

## 🎮 Usage
### Microphone dictation
1. Select **Capture Microphone** mode (PTT or VAD).
2. Configure **PTT hotkey** and optional **Toggle hotkey**.
3. Hold PTT to record; release to transcribe + paste.

### System audio transcription (Windows)
1. Select your **Output device** in **Capture System Audio**.
2. Press the **Transcribe hotkey** to start/stop monitoring.
3. System audio transcripts appear in the **System Audio** tab, and the combined **Conversation** tab.

## ⚙️ Configuration
### Environment variables
- `TRISPR_WHISPER_CLI`: Path to `whisper-cli.exe`
- `TRISPR_WHISPER_MODEL`: Path to a ggml model file (optional)
- `TRISPR_WHISPER_MODEL_DIR`: Directory containing models
- `TRISPR_WHISPER_MODEL_BASE_URL`: Base URL for model downloads
- `TRISPR_CLOUD_ENDPOINT`: HTTP endpoint for cloud fallback
- `TRISPR_CLOUD_TOKEN`: Bearer token for cloud auth

### Local whisper.cpp setup (Windows)
```bash
# Example one-shot setup
.\scripts\setup-whisper.ps1

# CPU fallback
.\scripts\setup-whisper.ps1 -CpuFallback
```

## 🧱 Project structure
```
Trispr_Flow/
├── src/                 # Frontend TypeScript
│   ├── main.ts         # Main app logic
│   ├── overlay.ts      # Overlay state + animation
│   └── styles.css      # App styling
├── src-tauri/          # Rust backend
│   └── src/lib.rs      # Core backend logic
├── index.html          # Main window UI
├── overlay.html        # Overlay UI
├── ROADMAP.md          # Roadmap
└── STATUS.md           # Current status
```

## 🗺️ Roadmap
See [ROADMAP.md](ROADMAP.md) for milestones and next steps.

## 🤝 Contributing
PRs are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).
