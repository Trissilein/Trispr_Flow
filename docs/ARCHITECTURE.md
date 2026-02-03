# Architecture

Last updated: 2026-02-03

## Data flow (Microphone - PTT)
1. Hotkey pressed
2. Audio capture starts (selected input device)
3. Ring buffer collects PCM frames
4. Hotkey released
5. Audio chunk finalized
6. ASR backend transcribes
7. Transcript pasted into active field
8. Transcript stored in history (mic)

## Data flow (Microphone - VAD)
1. VAD monitor starts
2. Audio capture runs continuously
3. VAD gates chunk start/stop with silence grace
4. ASR backend transcribes each chunk
5. Transcript pasted + stored

## Data flow (System Audio - WASAPI loopback)
1. Transcribe hotkey toggles monitoring
2. WASAPI loopback capture reads system output
3. Chunker segments audio (interval + overlap)
4. Optional VAD gate for system audio
5. ASR backend transcribes per chunk
6. Transcript stored in system history

## Conversation view
- Combines mic + system histories into a single time‑ordered stream.
- Rendered in the Output panel (Conversation tab).
- Optional detachable window (WIP).

## Core components
- **HotkeyService**: global hotkey registration (PTT / Toggle / Transcribe)
- **AudioCapture (mic)**: cpal input capture, resampling to 16 kHz mono
- **AudioCapture (system)**: WASAPI loopback (Windows)
- **Chunker**: splits audio for system transcription (interval + overlap)
- **VAD**: threshold + silence grace gating for mic/system
- **BackendSelector**: local whisper.cpp or cloud fallback
- **AsrBackend**: whisper-cli process runner
- **PasteService**: clipboard-safe paste to focused field
- **HistoryStore**: mic + system histories + conversation rendering
- **Overlay**: separate always-on-top window (WIP)

## Local backend
- `whisper-cli` invoked with a temporary WAV file.
- Model path resolved via env vars or `whisper.cpp/models`.
- GPU builds validated (CUDA on Windows); CPU fallback supported.

## Cloud backend (optional)
- HTTP endpoint + token
- Used only when cloud fallback is enabled

