# Trispr Flow - Status

Last updated: 2026-02-04

## Summary
- Compact dark UI with output tabs (Microphone / System Audio / Conversation).
- Microphone capture supports PTT + VAD; toggle hotkey remains available inside PTT mode.
- System audio capture (WASAPI loopback) with transcribe hotkey and separate history.
- Conversation view merges mic + system entries with timestamps.
- Output meters with dB readouts + threshold markers; input gain control for mic + system audio (±30 dB).
- whisper.cpp GPU-first with CPU fallback; cloud fallback toggle + status badge wired.
- Model Manager revamped with sources, storage path picker, and install/remove actions.
- Overlay settings UI restructured (style selector visible, config in expander).

## Working today
- Settings persistence (JSON in config dir).
- History persistence (mic + system + conversation view).
- Transcribe hotkey toggles system audio monitoring.
- Audio cues with volume control.
- VAD controls for mic + system audio.
- Output device selection for system audio (WASAPI).
- Model sources (default + custom URL) and model storage picker.

## Known gaps
- Hallucination filter still needs tightening (occasional “you/thanks” on noise).
- Detachable conversation window stability (content + close behavior).
- VAD + meter calibration still needs verification with real input.
- macOS testing still pending.

## Build notes
- whisper.cpp must be built separately (CUDA for NVIDIA; CPU fallback ok).
- Windows CUDA builds may require overriding unsupported compiler versions.
- Tauri dev builds rely on a system WebView runtime.

## Next focus
1. Conversation detach window reliability.
2. Hallucination filtering improvements.
3. Verify VAD + meter calibration after gain changes.
4. Plan AI fallback overhaul (Claude/OpenAI/Gemini).
