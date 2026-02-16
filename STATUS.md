# Trispr Flow - Status

Last updated: 2026-02-16

## Summary

- **Current version**: 0.6.0
- **Current phase**: VibeVoice-ASR Integration Complete
- **Milestones 0-3**: Complete
- **v0.4.0**: Complete (Post-processing pipeline)
- **v0.4.1**: Complete (Security hardening, CUDA distribution)
- **v0.5.0**: Complete (Tab UI, chapters, topics, export, live dump)
- **v0.6.0**: Complete (VibeVoice-ASR, speaker diarization, OPUS encoding, parallel mode)
- Input capture supports PTT + Voice Activation with separate status/activity indicators.
- Output transcription uses WASAPI loopback with transcribe hotkey and dedicated history.
- Conversation view merges input/output transcripts into a time-ordered stream.
- Voice Activation thresholds are controlled in dB (-60..0) and persisted as linear values.
- Transcribe enablement is session-only and starts deactivated on each app launch.
- whisper.cpp remains primary (GPU-first, CPU fallback); VibeVoice-ASR available for speaker diarization.
- Model manager uses a single list with active-first ordering and internal Delete vs external Remove.

## Working today

- Settings and histories persist to app config/data directories.
- Unit + smoke baseline is available (`npm run test`, `npm run test:smoke`).
- Window state persistence with validation (prevents minimized window bug).
- Tray pulse feedback for recording/transcribing states (turquoise/yellow).
- Activity indicators for capture and transcribe states.
- Model hot-swap without restart (with rollback on failure).
- Capture enhancements: activation words, language pinning (16 languages), hallucination filter UI.
- Post-processing pipeline: punctuation, capitalization, numbers, custom vocabulary.
- Conversation window: always-on-top toggle, geometry persistence.
- Tab-based UI: Transcription + Settings tabs with localStorage persistence.
- Chapter segmentation: silence-based, time-based, hybrid detection methods.
- Topic detection: keyword-based with customizable keywords and filter buttons.
- Live transcript dump: crash recovery buffering via 5-sec intervals.
- Export: TXT/MD/JSON with format versioning, speaker attribution support.
- VibeVoice-ASR sidecar: speaker-diarized transcription via FastAPI.
- Speaker diarization UI: color-coded segments, editable labels.
- Analyse button: file picker, auto-save OPUS recordings, progress indicator.
- Quality presets: OPUS bitrate (32/64/96/128 kbps), VibeVoice precision (FP16/INT8).
- Parallel mode: Whisper + VibeVoice simultaneous transcription.
- System audio auto-save: 60s flush intervals for OPUS recordings.
- Model monitoring: weekly VibeVoice update checks with toast notifications.
- PyInstaller packaging: standalone sidecar exe (auto-detect bundled vs Python).

## Known gaps

- Optional Tauri E2E coverage is pending.
- macOS testing still pending.
- Sidecar exe not yet bundled in NSIS installer (manual setup for now).

## Build notes

- whisper.cpp must be built separately (CUDA for NVIDIA; CPU fallback ok).
- Windows CUDA builds may require overriding unsupported compiler versions.
- Tauri dev builds rely on a system WebView runtime.
- WSL/Linux smoke builds require GTK/WebKit/pkg-config/linker dependencies (see `docs/DEVELOPMENT.md`).
- VibeVoice sidecar requires Python 3.10+ or bundled PyInstaller exe.

## Next focus

1. Plan v0.7.0 (AI Fallback Overhaul: multi-provider Claude/OpenAI/Gemini).
2. Optional: Parakeet ASR engine integration (NVIDIA hardware acceleration).
3. Optional: Improve E2E test coverage with Tauri WebDriver.
