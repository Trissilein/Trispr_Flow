# Trispr Flow

> GPU-first offline dictation + system audio transcription with optional AI refinement, privacy-first by default

[![Version](https://img.shields.io/badge/version-0.7.0-blue.svg)](CHANGELOG.md)
[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Trispr Flow** is a professional-grade desktop dictation application for Windows (macOS planned) built with Tauri + Rust + TypeScript. It combines GPU-accelerated local transcription, post-processing refinement, and optional multi-provider AI enhancement.

**Perfect for**: Meeting transcription, research notes, technical documentation, dictation workflows.

## What's New in v0.7.0 ✨

- **Refined-Only Insert Flow**: Deferred paste with timeout fallback to raw text
- **AI Refinement UX Upgrade**: Prompt presets + custom prompt preview + dedicated refinement inspector
- **Local Ollama Runtime Reliability**: Background autostart + improved runtime state handling
- **GPU Activity Visibility**: CPU/GPU activity indicator with persisted status snapshot
- **Overlay Controls Split**: Separate refinement animation controls under overlay settings
- **Installer Hardening**: Clear CUDA/Vulkan variant strategy and safer runtime path handling
- **Managed Module Platform (initial)**: Module states, permissions, health, and updates
- **GDD + Confluence Flow**: Draft, review, publish, and queue fallback path
- **Workflow Agent Console (V1)**: Voice-command parsing + session candidate confirm flow
- **Multimodal Foundations (V1)**: Vision/TTS module surfaces with safe defaults

**v0.7.0 Baseline Locked**: Stabilization + architecture review before next feature wave.

## Core Capabilities

### Transcription

- ✅ Microphone capture (PTT + Voice Activation modes)
- ✅ System audio capture (Windows WASAPI loopback)
- ✅ Adaptive continuous dump controls (profile + advanced per-source overrides)
- ✅ GPU-accelerated inference (whisper.cpp) with CPU fallback
- 🔄 Parakeet ASR engine (planned v0.6.0+)

### Processing & Refinement

- ✅ Local post-processing (punctuation, capitalization, numbers, custom vocabulary)
- ✅ Local AI refinement (Ollama runtime management, presets, low-latency mode)
- ✅ Qwen local model presets in UI (Qwen3 + Qwen3.5 lineup)
- 🔄 Cloud provider rollout (Claude, OpenAI, Gemini) — planned v0.7.3
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

### Modules (Managed)

- ✅ **GDD Automation (core)**: Transcript -> structured GDD draft generation
- ✅ **Confluence Integration (core)**: Cloud auth + target suggestion + page publish
- ✅ **Workflow Agent (optional)**: Wakeword command parsing + plan/confirm execution
- 🧪 **Screen Vision Input (optional)**: low-fps monitor source pipeline (RAM-only policy)
- 🧪 **Voice Output TTS (optional)**: Windows native output path + local custom placeholder
- ✅ Module lifecycle: `not_installed | installed | enabled | active | error`
- ✅ Permission consent model + dependency-aware enable checks

## Status & Roadmap

| Version | Phase | Status | Highlights |
| --- | --- | --- | --- |
| **v0.7.0** | 🟢 LIVE | Complete | Stable offline-first baseline |
| **v0.7.1** | 🔵 In Progress | Stabilization | Block E (UX/UI consistency) + Block F (reliability/QA) |
| **v0.7.3** | 📋 Planned | Cloud rollout | Claude/OpenAI/Gemini provider activation |

👉 **[Full Roadmap](ROADMAP.md)** — See milestones, implementation schedule, and competitor analysis

## Quick Start

### For Users
Download the latest installer from [Releases](https://github.com/Trissilein/Trispr_Flow/releases):
- **Trispr_Flow_0.7.0_CUDA_Edition.exe** — For NVIDIA GPU systems (RTX 4000+ series recommended)
- **Trispr_Flow_0.7.0_Vulkan_Edition.exe** — For systems without CUDA support

Ollama models are downloaded separately in-app (AI Refinement -> Models).

### For Developers
```bash
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
FIRST_RUN.bat
```

`FIRST_RUN.bat` installs npm dependencies and tries to hydrate missing local runtime files
from an installed Trispr Flow app (`resources/bin` -> `src-tauri/bin`).

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for detailed build configuration and fallbacks.

## Active Branches

- `main`: Trispr Flow mainline (capture/transcription product)
- `vibe-voice-branch`: module/voice experimentation line

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
3. **AI Refinement** (optional): Local-first refinement via Ollama with deferred paste + fallback

## Documentation

- 📖 [READ_ME_FIRST.md](READ_ME_FIRST.md) — Start here
- 🗺️ [ROADMAP.md](ROADMAP.md) — Project status and milestones
- 🧱 [docs/ARCHITECTURE_REVIEW_0.7.md](docs/ARCHITECTURE_REVIEW_0.7.md) — v0.7.0 architecture review checklist + findings
- 🏗️ [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) — Technical overview
- 🌿 [docs/BRANCHING.md](docs/BRANCHING.md) — Branch responsibilities and workflow
- 🔊 [docs/CONTINUOUS_DUMP_PLAN.md](docs/CONTINUOUS_DUMP_PLAN.md) — Adaptive continuous dump design + rollout
- 🛠️ [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) — Build setup
- 🧩 [docs/DEPENDENCY_POLICY.md](docs/DEPENDENCY_POLICY.md) — Installer/runtime dependency policy + startup preflight matrix
- 🔄 [docs/STATE_MANAGEMENT.md](docs/STATE_MANAGEMENT.md) — Internal state flow
- 📤 [docs/EXPORT_SCHEMA.md](docs/EXPORT_SCHEMA.md) — Export format spec
- 📋 [docs/TASK_SCHEDULE.md](docs/TASK_SCHEDULE.md) — Implementation blocks and tasks
- 🤖 [docs/V0.8.1_WORKFLOW_AGENT_PLAN.md](docs/V0.8.1_WORKFLOW_AGENT_PLAN.md) — Voice workflow-agent implementation plan
- 👁️ [docs/V0.8.2_MULTIMODAL_IO_PLAN.md](docs/V0.8.2_MULTIMODAL_IO_PLAN.md) — Vision/TTS module implementation plan
- ⚖️ [docs/DOC_SYNC_CONFLICTS.md](docs/DOC_SYNC_CONFLICTS.md) — Contradictions found + discussion points
- 🔀 [SCOPE.md](SCOPE.md) — How the project evolved vs original plan

## Testing

### Run tests
```bash
npm run test          # Unit tests
npm run test:smoke    # Smoke test (build + Rust tests)
npm run benchmark:latency       # Fixture benchmark (p50/p95, warn-gate)
npm run benchmark:latency:live  # Optional live profile (manual)
```

For WSL/Linux development, install dependencies listed in [DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Contributing

👥 **Contributing Guidelines**

- 📝 See [CONTRIBUTING.md](CONTRIBUTING.md) for PR process
- 🎯 For large features, see [SCOPE.md](SCOPE.md) to understand project direction
- 🚀 **Next tasks available**: See [ROADMAP.md](ROADMAP.md) for the live Done/Open task ledger (active: Block E/F, planned: Block J/K)
- 💬 Discussions welcome in Issues
