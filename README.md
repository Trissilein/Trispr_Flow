# Trispr Flow

Offline dictation tool that runs in the system tray, records the microphone via hotkey, transcribes speech, and pastes the result into the active input field. Designed for fast EN/DE recognition with GPU-first inference on Windows and macOS.

## Status
MVP pipeline works end-to-end on Windows (PTT/toggle, local transcription, paste, history).
GPU build is wired for whisper.cpp; macOS not tested yet.

Last updated: 2026-02-02

## Vision
Build a fast, private, and reliable dictation workflow similar to Wispr Flow, but offline-first and customizable.

## Core user flow
1. User presses PTT or toggles recording on.
2. Audio is captured from a selectable input device.
3. Recording ends on key release (PTT) or toggle-off.
4. Audio is transcribed with GPU-accelerated ASR.
5. Text is post-processed and pasted into the focused input.
6. Transcript is stored in history for review and reuse.

## Key requirements
- **Cross-platform**: Windows and macOS.
- **GPU-first** inference with auto-detect (NVIDIA CUDA, Apple GPU), CPU fallback.
- **Excellent EN/DE recognition** with robust punctuation and auto language detect.
- **System tray app** with hotkey control (PTT and toggle).
- **Device selection** via dropdown.
- **History UI** with settings (hotkeys, model, language, etc.).
- **Offline-first** by default; optional cloud fallback toggle (Claude pipeline).

## Candidate ASR backends (to evaluate)
- whisper.cpp with CUDA/Metal for a native, embeddable path.
- faster-whisper (CTranslate2) as a Windows-only turbo backend (optional).

Decision criteria: latency, accuracy on EN/DE, VRAM usage, cold-start time, licensing, ease of embedding in a tray app.

## High-level architecture
- **Audio capture**: WASAPI (Windows) and CoreAudio (macOS) with ring buffer.
- **Hotkey manager**: global hotkeys for PTT/toggle.
- **VAD/endpointing**: optional voice activity detection to trim silence.
- **ASR engine**: pluggable backend (GPU-first, CPU fallback).
- **Cloud fallback**: optional toggle to route audio to a Claude-based cloud pipeline.
- **Post-processing**: casing, punctuation, number formatting, custom vocab.
- **Text injection**: clipboard-safe paste or simulated typing.
- **UI/Tray**: status, device picker, model settings, history.
- **Storage**: local settings + history (SQLite or lightweight KV).

## Initial tech stack (proposed)
- **App shell**: Tauri v2 (tray, hotkeys, cross-platform).
- **Core**: Rust for capture, hotkey, ASR orchestration.
- **ASR**: whisper.cpp primary backend with GPU auto-detect.

## Cloud fallback toggle (Claude)
When enabled, the app routes finalized audio to a Claude-based cloud pipeline instead of local ASR. This is opt-in, clearly labeled, and disabled by default. The exact transport (API endpoint, auth) will be configurable to keep the desktop app decoupled from any hosted service.

## Non-goals (initially)
- Cloud-only transcription
- Live streaming captions
- Full editor replacement

## Repo structure (planned)
- `docs/` project docs, architecture, and decisions
- `src/` application code
- `tools/` model management scripts
- `assets/` icons, UI assets

## Tech stack (current)
- Tauri v2 + Rust core, Vite + TypeScript frontend
- whisper.cpp as primary ASR backend (GPU auto-detect)
- Optional Claude cloud fallback toggle (opt-in)

## Development
```bash
npm install
npm run tauri dev
```

## Runtime configuration
Environment variables for local and cloud pipelines:
- `TRISPR_WHISPER_CLI` (optional): absolute path to `whisper-cli` binary.
- `TRISPR_WHISPER_MODEL` (optional): absolute path to a ggml model file.
- `TRISPR_WHISPER_MODEL_DIR` (optional): directory containing ggml model files.
- `TRISPR_WHISPER_MODEL_BASE_URL` (optional): base URL for model downloads.
- `TRISPR_CLOUD_ENDPOINT` (optional): HTTP endpoint for cloud fallback.
- `TRISPR_CLOUD_TOKEN` (optional): bearer token for cloud fallback.

If none are set, the app attempts to find `../whisper.cpp` relative to the repo root.

Note: on macOS, paste injection via simulated keystrokes requires Accessibility permissions.

## Local whisper.cpp setup (dev)
- Build `whisper-cli` with CUDA support in `D:\GIT\whisper.cpp` (or any path).
- Place the model file (e.g. `ggml-large-v3.bin`) in `whisper.cpp/models`.
- Export `TRISPR_WHISPER_CLI` / `TRISPR_WHISPER_MODEL_DIR` if you keep them elsewhere.

## Roadmap
See `ROADMAP.md` for milestones and deliverables.

## Project status
Current progress and next steps live in `STATUS.md`.

## Contributing
See `CONTRIBUTING.md` and `docs/DEVELOPMENT.md`.

## Cloud fallback
Design notes for the Claude toggle and cloud pipeline live in `docs/CLOUD_FALLBACK.md`.
