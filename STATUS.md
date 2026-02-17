# Trispr Flow - Status

Last updated: 2026-02-16

## Executive Summary

- **Current version**: 0.6.0 (RELEASED) + post-release fixes
- **Current phase**: v0.7.0 Planning Complete (Block F ✅)
- **Next phase**: Block G Implementation (Opus) — Ready to start
- **Project scope**: Multi-provider AI-enhanced desktop dictation with Voice Analysis (speaker identification)
- **Status**: Production-ready. v0.7.0 planning complete with full architecture documentation

## Version Highlights

### v0.6.0 (RELEASED 2026-02-16)

- ✅ VibeVoice-ASR 7B Voice Analysis (speaker identification — who said what)
- ✅ Parallel transcription mode (Whisper + VibeVoice simultaneously)
- ✅ OPUS audio encoding (75% size reduction vs WAV)
- ✅ Quality presets (OPUS bitrate + VibeVoice precision)
- ✅ PyInstaller packaging (standalone sidecar exe)
- ✅ Professional app icon (Cyan/Gold Yin-Yang design)
- ✅ 22 E2E tests with full workflow coverage
- Dual installers: CUDA Edition (92MB) + Vulkan Edition (9.4MB)

### v0.6.0 Post-Release Fixes (2026-02-16)

- ✅ **Voice Analysis dialog**: Dedicated full-screen modal with step-by-step progress
  (File selected → Engine starting → Identifying speakers → Results with speaker segments)
- ✅ **Terminology**: "Speaker Diarization" renamed to "Voice Analysis" throughout UI and installer
- ✅ **Error visibility**: Engine failures shown directly in dialog with pip install hint
- ✅ **Apply Model bug fix**: Active state now updates immediately in UI after model switch
- ✅ **CREATE_NO_WINDOW**: All subprocesses (FFmpeg, whisper-cli, sidecar) no longer steal focus
- ✅ **Sidecar path resolution**: Fixed dev vs installed path detection
- ✅ **Sidecar stderr logging**: Engine errors now surfaced in app logs and error messages
- ✅ **Analyse button**: Always opens file picker — never silently reuses last file

### v0.7.0 (Planning Complete — Ready for Implementation)
- 📋 **Block F (Haiku)**: UX decisions + architecture design ✅ COMPLETE
  - Decision: "AI Fallback" terminology
  - Decision: Post-Processing panel expander location
  - Decision: Execution sequence (Local Rules → AI Fallback)
  - Full architecture documentation (400+ lines)
  - Implementation plan for Blocks G & H
- 🔵 **Block G (Opus)**: Multi-provider architecture ready to start
  - Task 31: Provider trait + factory design
  - Task 36: Settings migration (v0.6.0 → v0.7.0)
  - Task 37: Configuration UI components
- 🔵 **Block H (Sonnet)**: Provider implementations queued
  - Task 32: OpenAI client (GPT-4o, GPT-3.5-turbo)
  - Task 33: Claude client (Claude 3.5 Sonnet, Opus)
  - Task 34: Gemini client (Gemini 2.0 Pro)
  - Task 35: Custom prompt strategy
  - Task 38: E2E tests (all providers)

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
- VibeVoice-ASR sidecar: Voice Analysis (who said what) via FastAPI sidecar.
- Voice Analysis dialog: dedicated modal with step-by-step progress, results, copy transcript.
- Voice Analysis UI: color-coded speaker segments, editable speaker labels.
- Analyse button: always opens file picker, no silent reuse of last file.
- Quality presets: OPUS bitrate (32/64/96/128 kbps), VibeVoice precision (FP16/INT8).
- Parallel mode: Whisper + VibeVoice simultaneous transcription.
- System audio auto-save: 60s flush intervals for OPUS recordings.
- Model monitoring: weekly VibeVoice update checks with toast notifications.
- PyInstaller packaging: standalone sidecar exe (auto-detect bundled vs Python).

## Known gaps

- Optional Tauri E2E coverage is pending.
- macOS testing still pending.
- Sidecar exe not yet bundled in NSIS installer (manual setup for now).
- Voice Analysis Python deps require manual install for dev:
  `pip install -r sidecar/vibevoice-asr/requirements.txt`
- Capture UI inconsistencies planned for overhaul (UX-1 to UX-4, see ROADMAP.md).
- System audio cut-off under 8s means short-silence meetings may not produce analysable files.

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
