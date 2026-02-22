# Trispr Flow

> GPU-first offline dictation + system audio transcription with optional AI refinement, privacy-first by default

[![Version](https://img.shields.io/badge/version-0.6.0-blue.svg)](CHANGELOG.md)
[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Trispr Flow** is a professional-grade desktop dictation application for Windows (macOS planned) built with Tauri + Rust + TypeScript. It combines GPU-accelerated local transcription, post-processing refinement, and optional multi-provider AI enhancement.

**Perfect for**: Meeting transcription, research notes, technical documentation, dictation workflows.

## What's New in v0.6.0 ✨

- **Adaptive Continuous Dump**: Silence-aware + interval + hard-cut chunking for system audio and mic Toggle mode
- **Professional Icon**: Cyan/Gold Yin-Yang branding

**v0.7.0 In Execution**: Foundation complete (Block F + G), provider integrations (Block H) are next.

## Core Capabilities

### Transcription

- ✅ Microphone capture (PTT + Voice Activation modes)
- ✅ System audio capture (Windows WASAPI loopback)
- ✅ Adaptive continuous dump controls (profile + advanced per-source overrides)
- ✅ GPU-accelerated inference (whisper.cpp) with CPU fallback
- 🔄 Parakeet ASR engine (planned v0.6.0+)

### Processing & Refinement

- ✅ Local post-processing (punctuation, capitalization, numbers, custom vocabulary)
- 🔄 Multi-provider AI Fallback (Claude, OpenAI, Gemini) — planned v0.7.0
- ✅ Custom prompt support (user-editable with defaults)

### Output & Organization

- ✅ Chapter segmentation (silence-based, time-based, hybrid)
- ✅ Topic detection (keyword-based with filters)
- ✅ Full-text search across transcripts
- ✅ Export formats (TXT, Markdown, JSON)
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
| **v0.6.0** | 🟢 LIVE | Complete | Core capture/transcription, OPUS, chapters, topics |
| **v0.7.0** | 📋 Planning | Block F Complete | AI Fallback architecture (Claude/OpenAI/Gemini) |
| **v0.7.0** | ✅ Complete | Block G (Opus) | Multi-provider architecture, settings migration, config UI |
| **v0.7.0** | 🔵 In Progress | Block H (Sonnet) | Provider integrations (OpenAI/Claude/Gemini), prompt polish, E2E |

👉 **[Full Roadmap](ROADMAP.md)** — See milestones, implementation schedule, and competitor analysis

## Quick Start

### For Users
Download the latest installer from [Releases](https://github.com/Trissilein/Trispr_Flow/releases):
- **Trispr_Flow_0.6.0_CUDA_Edition.exe** — For NVIDIA GPU systems (RTX 4000+ series recommended)
- **Trispr_Flow_0.6.0_Vulkan_Edition.exe** — For systems without CUDA support

### For Developers
```bash
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
npm install
npm run tauri dev
```

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for system requirements and build configuration.

## Active Branches

- `main`: Trispr Flow mainline (capture/transcription product)
- `analysis-module-branch`: standalone analysis-module project line

See `docs/BRANCHING.md` for branch responsibilities.

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

### Analyse Placeholder
1. The `Analyse` button is currently a placeholder in Trispr Flow.
2. It shows an in-app notice that the dedicated analysis module is coming soon.
3. Analysis development now lives in the separate `analysis-module-branch`.

### Processing Pipeline
1. **Raw Transcription**: Whisper-generated text
2. **Post-Processing**: Local rules (punctuation, numbers, vocabulary)
3. **AI Refinement** (optional v0.7.0+): Multi-provider AI enhancement via Claude/OpenAI/Gemini

## Documentation

- 📖 [docs/README.md](docs/README.md) — Documentation map + governance
- 🗺️ [ROADMAP.md](ROADMAP.md) — Project status and milestones
- 🏗️ [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Technical overview
- 🧭 [docs/APP_FLOW.md](docs/APP_FLOW.md) — App flow and panel behavior
- 🎨 [docs/frontend/DESIGN_SYSTEM.md](docs/frontend/DESIGN_SYSTEM.md) — Visual tokens and UI patterns
- 🧱 [docs/frontend/FRONTEND_GUIDELINES.md](docs/frontend/FRONTEND_GUIDELINES.md) — Frontend engineering conventions
- 🌿 [docs/BRANCHING.md](docs/BRANCHING.md) — Branch responsibilities and workflow
- 🔊 [docs/CONTINUOUS_DUMP_PLAN.md](docs/CONTINUOUS_DUMP_PLAN.md) — Adaptive continuous dump design + rollout
- 🛠️ [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) — Build setup
- 🔄 [docs/STATE_MANAGEMENT.md](docs/STATE_MANAGEMENT.md) — Internal state flow
- 📤 [docs/EXPORT_SCHEMA.md](docs/EXPORT_SCHEMA.md) — Export format spec
- 📋 [docs/TASK_SCHEDULE.md](docs/TASK_SCHEDULE.md) — Implementation blocks and tasks
- 🗃️ [docs/archive/SCOPE.md](docs/archive/SCOPE.md) — Project scope evolution (historical)

## Testing

### Run tests
```bash
npm run test          # Unit tests
npm run test:docs     # Documentation governance checks
npm run test:smoke    # Smoke test (build + Rust tests)
```

For WSL/Linux development, install dependencies listed in [DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Contributing

👥 **Contributing Guidelines**

- 📝 See [CONTRIBUTING.md](CONTRIBUTING.md) for PR process
- 🎯 For feature scope and priority, see [ROADMAP.md](ROADMAP.md) + [docs/DECISIONS.md](docs/DECISIONS.md)
- 🚀 **Next tasks available**: See [ROADMAP.md](ROADMAP.md) for the live Done/Open task ledger (v0.7 Block H)
- 💬 Discussions welcome in Issues
