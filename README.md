# Trispr Flow

> Offline-first dictation and system-audio transcription with local AI refinement.

[![Version](https://img.shields.io/badge/version-0.7.3-blue.svg)](CHANGELOG.md)
[![Windows](https://img.shields.io/badge/Windows-0078D6?style=flat&logo=windows&logoColor=white)](https://github.com/Trissilein/Trispr_Flow/releases)
[![macOS](https://img.shields.io/badge/macOS-planned-lightgrey?style=flat&logo=apple)](ROADMAP.md)
[![Notices](https://img.shields.io/badge/license-notices-blue.svg)](THIRD_PARTY_NOTICES.md)

## What Is Trispr Flow

Trispr Flow is a desktop dictation app built with Tauri + Rust + TypeScript.
It is designed for local-first transcription and editing workflows:

- microphone capture (PTT / VAD)
- system-audio transcription (WASAPI loopback)
- local post-processing
- optional local AI refinement via Ollama
- export + session history for recovery and auditability

## Current Release

- Current app line: `v0.7.2`
- Packaging: unified Windows installer flow
- Delivery focus: stabilization and reliability hardening for local runtime + UX

Release details:
- [CHANGELOG.md](CHANGELOG.md)
- [STATUS.md](STATUS.md)
- [ROADMAP.md](ROADMAP.md)

## Capabilities

### Transcription
- GPU-accelerated local Whisper runtime with CPU fallback
- Microphone and system-audio capture
- Continuous dump pipeline for crash-safe transcript continuity
- Multi-language workflows with language hint/pinning

### Refinement
- Local-first refinement pipeline
- Managed Ollama runtime flow (detect/install/start/verify)
- Curated local model cards and activation flow
- Prompt presets + custom prompt support

### Output and Organization
- Session history and search
- Chapter/topic support
- Export formats: TXT / Markdown / JSON
- Live dump + restore workflows

### Modules (Managed)
- GDD generation/publish flow
- Confluence integration lane
- Workflow agent and multimodal modules (status per roadmap)

## Quick Start

### Users
1. Download the latest release from [GitHub Releases](https://github.com/Trissilein/Trispr_Flow/releases).
2. Choose an installer variant:
   - `*.vulkan-only-*.exe` (smallest, no CUDA payload)
   - `*.cuda-lite-*.exe` (CUDA without `cublasLt64_13.dll`, Vulkan fallback included)
   - `*.cuda-complete-*.exe` (full CUDA payload including `cublasLt64_13.dll`)
3. Open **AI Refinement** in-app to install/start local Ollama runtime and download models.

### Developers (Windows)
```bat
git clone https://github.com/Trissilein/Trispr_Flow.git
cd Trispr_Flow
scripts\windows\FIRST_RUN.bat
```

Compatibility wrappers remain in root (`FIRST_RUN.bat`, `build_unified.bat`, `rebuild-installer.bat`, `build-quantize.bat`).

Installer build/upload shortcuts:
- `build_installers.bat` (build all variants)
- `upload_release_assets.bat -Tag vX.Y.Z -CreateReleaseIfMissing -Clobber` (upload latest `vulkan-only`/`cuda-lite`/`cuda-complete` assets for that tag via GitHub CLI)

## Status and Roadmap

| Version | Status | Focus |
| --- | --- | --- |
| `v0.7.0` | Released | Stable baseline for local-first transcription |
| `v0.7.1` | Released | Stabilization, UX consistency, reliability hardening |
| `v0.7.2` | Released | Repo cleanup, Ollama hardening, WebView recovery watchdog |
| `v0.7.3` | Released | UI redesigns, vocabulary learning, casing bug fix |
| `v0.8.x` | Planned | Block U (assistant UX) + cloud lane activation |

Canonical planning docs:
- [ROADMAP.md](ROADMAP.md)
- [STATUS.md](STATUS.md)
- [docs/TASK_SCHEDULE.md](docs/TASK_SCHEDULE.md)

## Dependencies

Core runtime dependencies:
- [Tauri 2](https://tauri.app/)
- [Rust](https://www.rust-lang.org/)
- [TypeScript](https://www.typescriptlang.org/)
- [whisper.cpp](https://github.com/ggerganov/whisper.cpp)
- [Ollama](https://ollama.com/) (optional for local refinement)
- FFmpeg with `libopus` support (for OPUS pipeline)

Dependency and policy docs:
- [docs/DEPENDENCY_POLICY.md](docs/DEPENDENCY_POLICY.md)
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)

## Documentation Entry Points

- [READ_ME_FIRST.md](READ_ME_FIRST.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- [docs/APP_FLOW.md](docs/APP_FLOW.md)
- [docs/FRONTEND_GUIDELINES.md](docs/FRONTEND_GUIDELINES.md)
- [docs/SECURITY.md](docs/SECURITY.md)
- [docs/SCOPE.md](docs/SCOPE.md)

## Acknowledgements

Trispr Flow builds on the work of the open-source ecosystem, especially:
- whisper.cpp contributors
- Ollama contributors
- Tauri and Rust communities
- FFmpeg project maintainers

See [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for attribution details.

## Contributing

Contribution process and housekeeping requirements:
- [CONTRIBUTING.md](CONTRIBUTING.md)
