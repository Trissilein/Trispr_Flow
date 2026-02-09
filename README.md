# Trispr Flow

> GPU-first offline dictation + system audio transcription, privacy-first by default

[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-000000?style=flat&logo=apple&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Trispr Flow is a modern desktop dictation app built with Tauri + Rust + TypeScript. It combines local GPU-accelerated transcription (whisper.cpp) with a compact, responsive UI and optional cloud fallback.

## Read Me First
Before changing documentation, read `READ_ME_FIRST.md`.

## Key Capabilities
- Microphone capture with global hotkeys (PTT + Voice Activation)
- GPU-accelerated transcription (whisper.cpp) with CPU fallback
- System audio capture (Windows / WASAPI loopback) with dedicated transcribe hotkey
- Output tabs for Microphone, System Audio, and Conversation
- Output meters with dB readout, thresholds, and gain control
- Minimal overlay (Dot) plus KITT-style bar mode
- Model manager (install/remove, sources, storage path)
- Privacy-first by default, AI-enhanced mode optional

## Status
**Current phase:** Documentation + stabilization

**Recent highlights**
- Frontend modularization (main.ts split into focused modules)
- Overlay lifecycle stabilization (Dot/KITT treated as explicit lifecycle transitions)
- System audio robustness (WASAPI fixes + transcribe queue/idle meter)
- Automated testing baseline (unit + smoke verified)
- Transcribe defaults to disabled per session

## Roadmap (At a Glance)
**Now**
- Finalize documentation sync across roadmap and architecture/state docs

**Next**
- Activity feedback: tray pulse + backlog warning/expansion flow

**Then**
- Capture enhancements: activation words, language pinning, extra hotkeys

**Later**
- Post-processing pipeline and AI fallback overhaul
- Long-form features (export, chapters, topic detection)
- Conversation window configurability

Full roadmap: `ROADMAP.md`

## Quick Start (Dev)
```bash
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
npm install
npm run tauri dev
```

## Usage
### Input transcription
1. Select Capture Input mode (PTT or Voice Activation).
2. Configure PTT hotkey and optional toggle hotkey.
3. Hold PTT to record; release to transcribe and paste.

### Output transcription (Windows)
1. Select your Output device in Capture Output.
2. Press the Transcribe hotkey to start or stop monitoring.
3. System audio transcripts appear in the System Audio tab and the Conversation tab.

## Documentation
- `READ_ME_FIRST.md`
- `ROADMAP.md`
- `docs/ARCHITECTURE.md`
- `docs/DEVELOPMENT.md`
- `docs/CLOUD_FALLBACK.md`
- `docs/STATE_MANAGEMENT.md`
- `docs/wiki/` (GitHub Wiki source files)

## Testing
### Unit tests
```bash
npm run test
```

### Smoke test (frontend build + Rust tests)
```bash
npm run test:smoke
```

If you run in WSL/Linux, install the system dependencies listed in `docs/DEVELOPMENT.md` first.

## Contributing
PRs are welcome. See `CONTRIBUTING.md`.
