# Decisions

Last updated: 2026-02-02

## Confirmed
- OS targets: Windows + macOS (mac currently not testable).
- UI stack: Tauri v2 (tray + global hotkeys).
- Primary ASR backend: whisper.cpp with GPU auto-detect.
- Language mode: auto-detect (EN/DE mix).
- Cloud fallback: optional toggle to route audio to a Claude-based pipeline.

## Implemented (scaffold)
- Tauri v2 project skeleton with tray menu and settings UI.
- Local settings + history persisted as JSON (config/data dirs).
- Global hotkeys registered via tauri-plugin-global-shortcut (events emitted to UI).
- Audio device list pulled via cpal (currently inputs only).
- Audio capture wired via cpal with resampling to 16 kHz mono.
- Local transcription executed via `whisper-cli` (auto GPU, `--no-gpu` not used).
- Clipboard-safe paste via arboard + enigo.
- Model manager can download ggml models into the app data directory.
- GPU build validated on Windows with CUDA; VS 18/2026 requires override flags.

## Housekeeping
- Added STATUS.md, CONTRIBUTING.md, .env.example, and docs/DEVELOPMENT.md.

## Open
- Whether to ship faster-whisper as a Windows-only optional backend.
- VAD approach (webrtcvad vs built-in vs custom).
- Post-processing ruleset scope (numbers, abbreviations, punctuation styles).
- Decide on supported VS toolchain for CUDA (prefer VS 2022 vs override on VS 18/2026).
