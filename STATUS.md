# Trispr Flow - Status

Last updated: 2026-02-06

## Summary
- Milestones 0-2 are complete; current phase is documentation updates.
- Input capture supports PTT + Voice Activation with separate status/activity indicators.
- Output transcription uses WASAPI loopback with transcribe hotkey and dedicated history.
- Conversation view merges input/output transcripts into a time-ordered stream.
- Voice Activation thresholds are controlled in dB (-60..0) and persisted as linear values.
- Transcribe enablement is session-only and starts deactivated on each app launch.
- whisper.cpp remains primary (GPU-first, CPU fallback); cloud/AI fallback remains optional.
- Model manager supports default/custom sources, storage path picker, install/remove.

## Working today
- Settings and histories persist to app config/data directories.
- Unit tests pass (`npm run test`).
- Smoke test passes (`npm run test:smoke`).
- Documentation refreshed for architecture, development, and contribution workflow.

## Known gaps
- Tray pulse feedback and backlog handling are still pending.
- Capture enhancements (activation words, language pinning, extra hotkeys) are pending.
- Post-processing pipeline is pending and intentionally sequenced after capture enhancements.
- Conversation window configurability (size/position/font/always-on-top) is pending.
- Optional Tauri E2E coverage is pending.
- macOS testing still pending.

## Build notes
- whisper.cpp must be built separately (CUDA for NVIDIA; CPU fallback ok).
- Windows CUDA builds may require overriding unsupported compiler versions.
- Tauri dev builds rely on a system WebView runtime.
- WSL/Linux smoke builds require GTK/WebKit/pkg-config/linker dependencies (see `docs/DEVELOPMENT.md`).

## Next focus
1. Complete documentation pass (`STATUS.md` + `docs/DECISIONS.md` kept current).
2. Implement Milestone 3 Activity Feedback (tray pulse + backlog handling).
3. Implement Capture Enhancements.
4. Implement Post-Processing pipeline on top of stabilized capture behavior.
