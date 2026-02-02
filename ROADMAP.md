# Roadmap - Trispr Flow

Last updated: 2026-02-02

This roadmap is organized by milestones. Dates are intentionally flexible.

## Current status
- Milestone 1 is functionally complete (hotkeys, capture, local transcription, paste wired).
- GPU build path for whisper.cpp validated; CPU fallback available.
- Model manager download flow is implemented.
- VAD, overlays, and packaging are still pending.

## Milestone 0 - Discovery & Decisions
- Confirm ASR backend (whisper.cpp as primary, optional faster-whisper on Windows).
- Lock UI stack (Tauri v2).
- Decide on audio capture layer and VAD implementation.
- Define packaging and auto-start strategy for Windows and macOS.
- Define cloud fallback toggle behavior (Claude pipeline).

Deliverables
- Tech stack decision record.
- Prototype benchmark notes (latency, quality, VRAM).

## Milestone 1 - MVP (PTT + Paste)
- Global hotkey (PTT) to start/stop recording.
- Selectable input device.
- GPU transcription with EN/DE support.
- Auto language detect (DE/EN mix).
- Paste output into focused field.
- Basic tray status (idle/recording/transcribing).

Definition of done
- Single hotkey path works end-to-end on Windows.
- 1-3s turnaround on short utterances (GPU).

## Milestone 2 - Toggle + History UI
- Toggle recording mode.
- Transcript history list with copy/reinsert.
- Settings UI (hotkeys, language mode, model preset).
- Model manager (download/cache/quantized variants).
- Cloud fallback toggle (Claude pipeline) with clear privacy indicator.
- Recording overlay indicator (on-screen while capturing/transcribing).

Definition of done
- Users can switch PTT/toggle and review history.
- Settings persist across restarts.

## Milestone 3 - Quality & UX
- Optional VAD/silence trimming.
- Post-processing rules (punctuation, casing, numbers).
- Safety paste (clipboard restore).
- Error handling + logs.

Definition of done
- Noticeably better transcription quality and UX.
- Stable behavior across multiple apps.

## Milestone 4 - 1.0 Release
- Installer + auto-start (Windows + macOS).
- Update channel (manual or auto).
- Documentation polish.

Definition of done
- Reliable daily-driver for EN/DE dictation on Windows and macOS.

## Risks & Dependencies
- GPU backend selection impacts packaging complexity.
- Hotkey/paste behavior may vary per target app.
- VAD and post-processing can affect latency.

## Out of scope (for now)
- Real-time streaming captions
- Mobile support
