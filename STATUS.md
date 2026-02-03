# Trispr Flow - Status

Last updated: 2026-02-03

## Summary
- Compact dark UI with output tabs (Microphone / System Audio / Conversation).
- Microphone capture supports PTT + VAD; toggle hotkey is available inside PTT mode.
- System audio capture (WASAPI loopback) with transcribe hotkey and separate history.
- Conversation view merges mic + system entries with timestamps.
- Output meters with dB readouts + threshold markers; input gain control for system audio.
- whisper.cpp GPU-first with CPU fallback; cloud fallback toggle wired.
- Overlay settings UI is present (dot color/size/opacity/position) but behavior still WIP.

## Working today
- Settings persistence (JSON in config dir).
- History persistence (mic + system + conversation view).
- Transcribe hotkey toggles system audio monitoring.
- Audio cues with volume control.
- VAD controls for mic + system audio.
- Output device selection for system audio (WASAPI).

## Known gaps
- Overlay dot behavior is unreliable (size/opacity/color/audio coupling needs fixes).
- System audio meter scaling is too low; VAD marker alignment + gain effect need calibration.
- Detachable conversation window stability (content + close behavior).
- Model Manager revamp (sources, install/remove, custom URLs).
- macOS testing still pending.

## Build notes
- whisper.cpp must be built separately (CUDA for NVIDIA; CPU fallback ok).
- Windows CUDA builds may require overriding unsupported compiler versions.
- Tauri dev builds rely on a system WebView runtime.

## Next focus
1. Overlay dot: correct rendering + audio-reactive sizing + opacity control.
2. System audio meter/VAD calibration + input gain effect verification.
3. Model Manager revamp (source selection + install/remove).
4. Conversation detach window reliability.
