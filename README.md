# Trispr Flow

> GPU-first offline dictation + system audio transcription with optional AI refinement, privacy-first by default

[![Version](https://img.shields.io/badge/version-0.6.0-blue.svg)](CHANGELOG.md)
[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Trispr Flow** is a professional-grade desktop dictation application for Windows (macOS planned) built with Tauri + Rust + TypeScript. It combines GPU-accelerated local transcription with speaker diarization, post-processing refinement, and optional multi-provider AI enhancement.

**Perfect for**: Meeting transcription, research notes, technical documentation, dictation workflows.

## What's New in v0.6.0 ✨

- **Speaker Diarization**: Microsoft VibeVoice-ASR 7B speaker-aware transcription
- **Quality Controls**: Configurable OPUS bitrate + VibeVoice precision (FP16/INT8)
- **Parallel Transcription**: Run Whisper + VibeVoice simultaneously (opt-in for 16GB+ VRAM)
- **Voice Analysis Bootstrap**: Optional installer/setup flow for VibeVoice dependencies (no Git required)
- **System Audio Auto-Save**: 60-second flush intervals for OPUS recordings
- **Professional Icon**: Cyan/Gold Yin-Yang branding

**v0.7.0 Planning Complete**: Multi-provider AI Fallback (Claude, OpenAI, Gemini) — Ready for implementation

## Core Capabilities

### Transcription

- ✅ Microphone capture (PTT + Voice Activation modes)
- ✅ System audio capture (Windows WASAPI loopback)
- ✅ GPU-accelerated inference (whisper.cpp) with CPU fallback
- ✅ Speaker diarization (VibeVoice-ASR 7B with color-coded segments)
- ✅ Parallel mode (Whisper + VibeVoice simultaneously)
- 🔄 Parakeet ASR engine (planned v0.6.0+)

### Processing & Refinement

- ✅ Local post-processing (punctuation, capitalization, numbers, custom vocabulary)
- 🔄 Multi-provider AI Fallback (Claude, OpenAI, Gemini) — planned v0.7.0
- ✅ Custom prompt support (user-editable with defaults)

### Output & Organization

- ✅ Chapter segmentation (silence-based, time-based, hybrid)
- ✅ Topic detection (keyword-based with filters)
- ✅ Full-text search across transcripts
- ✅ Export formats (TXT, Markdown, JSON with speaker attribution)
- ✅ Live transcript dump (crash recovery)

### User Experience

- ✅ Dual overlays (minimal Dot + KITT bar modes)
- ✅ Activity feedback (tray pulse: turquoise=recording, yellow=transcribing)
- ✅ Window state persistence (geometry + minimized/tray state)
- ✅ Model hot-swap (no restart required)
- ✅ 16 language support with language pinning
- 🔄 First-run wizard (planned v0.7.0+)

## Status & Roadmap

| Version | Phase | Status | Highlights |
| --- | --- | --- | --- |
| **v0.6.0** | 🟢 LIVE | Complete | VibeVoice-ASR, diarization, OPUS, parallel mode |
| **v0.7.0** | 📋 Planning | Block F Complete | AI Fallback architecture (Claude/OpenAI/Gemini) |
| **v0.7.0** | 🔵 Ready | Block G (Opus) | Multi-provider architecture, settings migration, config UI |
| **v0.7.0** | 🔵 Queued | Block H (Sonnet) | Provider implementations, E2E tests |

👉 **[Full Roadmap](ROADMAP.md)** — See milestones, implementation schedule, and competitor analysis

## Quick Start

### For Users
Download the latest installer from [Releases](https://github.com/Trissilein/Trispr_Flow/releases):
- **Trispr_Flow_0.6.0_CUDA_Edition.exe** — For NVIDIA GPU systems (RTX 4000+ series recommended)
- **Trispr_Flow_0.6.0_Vulkan_Edition.exe** — For systems without CUDA support

Voice Analysis (VibeVoice) is optional. If enabled, the installer/app guides users through dependency setup via bundled PowerShell script.

### For Developers
```bash
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
npm install
npm run tauri dev
```

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for system requirements and build configuration.

## Usage

### Microphone Transcription
1. **Capture Input**: Select PTT (Push-to-Talk) or Voice Activation mode
2. **Configure Hotkey**: Set your preferred hotkey (default: `Ctrl+Shift+R`)
3. **Record**: Hold hotkey to record, release to transcribe
4. **Auto-Paste**: Refined transcript auto-pastes to active window

### System Audio Transcription
1. **System Audio Capture**: Select output device (Windows WASAPI loopback)
2. **Transcribe Toggle**: Press dedicated hotkey to start/stop monitoring
3. **View**: Transcripts appear in System Audio tab and merged Conversation view

### Speaker Diarization (v0.6.0+)
1. **Enable VibeVoice**: In Model Manager, install VibeVoice-ASR 7B model
2. **Analyse Button**: Upload audio file for speaker-aware transcription
3. **Export**: Color-coded speaker segments in TXT/MD/JSON export

### Processing Pipeline
1. **Raw Transcription**: Whisper-generated text
2. **Post-Processing**: Local rules (punctuation, numbers, vocabulary)
3. **AI Refinement** (optional v0.7.0+): Multi-provider AI enhancement via Claude/OpenAI/Gemini

## Documentation

- 📖 [READ_ME_FIRST.md](READ_ME_FIRST.md) — Start here
- 🗺️ [ROADMAP.md](ROADMAP.md) — Project status and milestones
- 🏗️ [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Technical overview
- 🛠️ [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) — Build setup
- 🔄 [docs/STATE_MANAGEMENT.md](docs/STATE_MANAGEMENT.md) — Internal state flow
- 📤 [docs/EXPORT_SCHEMA.md](docs/EXPORT_SCHEMA.md) — Export format spec
- 📋 [docs/TASK_SCHEDULE.md](docs/TASK_SCHEDULE.md) — Implementation blocks and tasks
- ⚖️ [docs/DOC_SYNC_CONFLICTS.md](docs/DOC_SYNC_CONFLICTS.md) — Contradictions found + discussion points
- 🔀 [SCOPE.md](SCOPE.md) — How the project evolved vs original plan

## Testing

### Run tests
```bash
npm run test          # Unit tests
npm run test:smoke    # Smoke test (build + Rust tests)
```

For WSL/Linux development, install dependencies listed in [DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Contributing

👥 **Contributing Guidelines**

- 📝 See [CONTRIBUTING.md](CONTRIBUTING.md) for PR process
- 🎯 For large features, see [SCOPE.md](SCOPE.md) to understand project direction
- 🚀 **Next tasks available**: See [NEXT_BLOCK_G.md](docs/NEXT_BLOCK_G.md) for v0.7.0 implementation (Block G — Opus)
- 💬 Discussions welcome in Issues
