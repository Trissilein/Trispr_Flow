# Architecture

Last updated: 2026-02-02

## Data flow (PTT)
1. Hotkey pressed
2. Audio capture starts (selected device)
3. Ring buffer collects PCM frames
4. Hotkey released
5. Audio chunk finalized
6. ASR backend transcribes
7. Post-processing rules applied
8. Text injected into focused field
9. Transcript stored in history

## Data flow (Toggle)
1. Hotkey toggles recording on
2. Audio capture starts
3. Hotkey toggles recording off
4. Steps 5-9 same as PTT

## Components
- **HotkeyService**: global hotkey registration via tauri-plugin-global-shortcut.
- **AudioCapture**: cpal-backed capture (WASAPI on Windows, CoreAudio on macOS), selectable device, resampling.
- **AudioBuffer**: ring buffer + chunk finalization.
- **Endpointing**: optional VAD or silence trimming.
- **BackendSelector**: decides local vs cloud path based on toggle and availability.
- **AsrBackend**: interface for whisper.cpp (GPU-first, CPU fallback) or faster-whisper.
- **CloudBackend**: Claude-based cloud pipeline for fallback (opt-in).
- **ModelManager**: downloads and caches ggml models for local use.
- **PostProcessor**: punctuation, casing, numbers, custom vocab.
- **PasteService**: clipboard-safe paste with OS-level key simulation.
- **HistoryStore**: persistent transcripts and metadata.
- **TrayUI**: status + settings + history access.

## Backend interface (concept)
- init(model_path, device, options)
- transcribe(audio_pcm, language_hint)
- shutdown()

## Local backend (current)
- `whisper-cli` is invoked as an external process.
- The app writes a temporary WAV file and reads the generated `.txt` output.
- Model and binary paths are resolved via env vars or `../whisper.cpp`.
- GPU builds validated via CUDA (Windows); CPU fallback available.

## Cloud backend contract (concept)
- init(endpoint, auth, options)
- transcribe(audio_pcm, language_hint)
- shutdown()

## Error handling
- Clear error states in tray UI.
- Retry on transient backend failures.
- Log minimal diagnostics (opt-in).
