# Trispr Flow - Status

Last updated: 2026-02-02

## Summary
- UI scaffold with settings + history is in place.
- Global hotkeys are registered (PTT + toggle).
- Audio capture runs via cpal, resampled to 16 kHz mono.
- Local transcription runs via `whisper-cli` (GPU build validated; CPU fallback works).
- Transcripts are stored in history and pasted into the focused input.
- Claude cloud fallback toggle is wired (HTTP endpoint required).
- Model manager downloads ggml models into the app data cache.

## Working today
- Settings persistence (JSON in config dir)
- History persistence (JSON in data dir)
- Tray menu with Claude fallback toggle
- Capture -> transcribe -> paste pipeline (local + cloud)
- Model manager with download UI (ggml models)
- GPU whisper.cpp build on Windows (CUDA), using override for unsupported compiler

## Known gaps
- VAD/silence trimming
- Better resampling quality (linear currently)
- Mac testing (not available yet)
- UX layout rework (output panel prioritized, two-column layout)
- Recording overlay indicator

## Build notes
- WSL/Linux builds require GTK/WebKit system deps for Tauri.
- Windows host build avoids GTK dependency.
- whisper.cpp must be built separately (CUDA for NVIDIA).

## Next focus
1. UX layout rework (output on top, two-column layout, expanders/scrollable panels).
2. Speed-mode toggles (beam/best-of/no-fallback/flash-attn/VAD).
3. Recording overlay indicator while capturing/transcribing.
