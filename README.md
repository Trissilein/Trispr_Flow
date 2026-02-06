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

## ✅ Recently Completed
- **Frontend Modularization**: Split monolithic main.ts into 14 focused modules for better maintainability
- **Overlay Circle Dot Fix**: Audio-reactive size animation now works correctly
- **Monitoring Toggles**: UI controls to enable/disable microphone tracking and system audio transcription
- **Tray Menu Sync**: Checkmarks properly sync between UI toggles and system tray menu

## 🚧 Work in Progress
- **Documentation Updates**: Architecture docs + development/test workflow sync
- **Activity Feedback**: tray pulse + backlog handling
- **Capture Enhancements**: activation words, language pinning, extra hotkeys

## 🚀 Quick Start (Dev)
```bash
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
npm install
npm run tauri dev
```

## 🎮 Usage
### Input transcription
1. Select **Capture Input** mode (PTT or Voice Activation).
2. Configure **PTT hotkey** and optional **Toggle hotkey**.
3. Hold PTT to record; release to transcribe + paste.

### Output transcription (Windows)
1. Select your **Output device** in **Capture Output**.
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

## 🧱 Project Structure
```
Trispr_Flow/
├── src/                      # Frontend TypeScript (Modular Architecture)
│   ├── main.ts              # App initialization (~220 lines, down from ~1800)
│   ├── state.ts             # Global application state
│   ├── types.ts             # TypeScript type definitions
│   ├── settings.ts          # Settings persistence & UI rendering
│   ├── devices.ts           # Audio device management
│   ├── hotkeys.ts           # Hotkey configuration
│   ├── models.ts            # Model management
│   ├── history.ts           # Transcript history logic
│   ├── dom-refs.ts          # Centralized DOM references
│   ├── event-listeners.ts   # Event handler setup
│   ├── ui-state.ts          # UI state management
│   ├── ui-helpers.ts        # UI utility functions
│   ├── toast.ts             # Toast notifications
│   ├── accessibility.ts     # Accessibility helpers
│   ├── audio-cues.ts        # Audio feedback system
│   ├── overlay.ts           # Overlay state + animation
│   └── styles.css           # App styling
├── src-tauri/               # Rust backend
│   ├── src/lib.rs           # App wiring + Tauri commands
│   ├── src/audio.rs         # Mic capture + VAD runtime
│   ├── src/transcription.rs # System audio transcription pipeline
│   ├── src/models.rs        # Model download/install/validation
│   ├── src/state.rs         # Settings + app state
│   ├── src/hotkeys.rs       # Hotkey parsing/validation
│   ├── src/overlay.rs       # Overlay control
│   └── src/paths.rs         # Config/data paths
├── index.html               # Main window UI
├── overlay.html             # Overlay UI
├── .claude/
│   └── ROADMAP.md          # Development roadmap
└── docs/                    # Documentation
    ├── ARCHITECTURE.md
    ├── CLOUD_FALLBACK.md
    └── DEVELOPMENT.md
```

## 🗺️ Roadmap
See [.claude/ROADMAP.md](.claude/ROADMAP.md) for detailed milestones and next steps.

**Current Phase:** Documentation Updates
- Block 8: Architecture/docs refresh
- Keep local test workflow documented and reproducible

**Next Phase:** Code Refactoring & Testing
- Block 6: lib.rs Modularization
- Block 7: Automated Testing
- Block 9: Tauri E2E (optional)

**Future Features:**
- Capture Enhancements (activation words, language pinning, hotkeys)
- Post-Processing Pipeline (punctuation, formatting, normalization)
- Live Transcript Dump & Chapter Summarization

## 🧪 Testing
### Unit tests
```bash
npm run test
```

### Smoke test (frontend build + Rust tests)
```bash
npm run test:smoke
```

If you run in WSL/Linux, install the system dependencies listed in `docs/DEVELOPMENT.md` first.

## 🤝 Contributing
PRs are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).
