# Architecture

Last updated: 2026-02-20

## Frontend architecture

- Stack: Tauri 2 + vanilla TypeScript + Vite
- Entry: `src/main.ts` bootstraps settings/devices/history/models, then wires listeners and UI rendering.
- State model: centralized mutable state in `src/state.ts` with explicit setter functions.
- UI model: direct DOM updates through dedicated modules (`src/settings.ts`, `src/ui-state.ts`, `src/history.ts`, `src/models.ts`).
- Event wiring: user interactions are bound in `src/event-listeners.ts`.

### UI Layout

- Hero status section (recording/transcribing/model status)
- Main tabs:
  - `Transcription`
  - `Settings`

`Transcription` tab:
- Input/System/Conversation history views
- Search, chapters, export controls

`Settings` tab:
- Capture Input panel
- System Audio Capture panel
- Post-Processing panel
- Model Manager panel
- UX/UI panel

## Frontend module map

| Module | Purpose |
| --- | --- |
| `main.ts` | Bootstrap, backend event listeners |
| `state.ts` | Centralized mutable state |
| `types.ts` | TypeScript interfaces |
| `dom-refs.ts` | DOM element references |
| `event-listeners.ts` | User interaction handlers |
| `settings.ts` | Settings persistence and UI sync |
| `ui-state.ts` | Runtime UI status rendering |
| `history.ts` | History rendering, export, chapters, topics, search |
| `chapters.ts` | Chapter UI lifecycle |
| `models.ts` | Model list rendering and actions |
| `devices.ts` | Audio/output device list rendering |

## Backend architecture (`src-tauri/src`)

- `lib.rs`: command registration, app startup, tray/window integration, module wiring.
- `audio.rs`: microphone capture, VAD runtime, overlay level emission.
- `continuous_dump.rs`: adaptive segmenter for silence/interval/hard-cut chunking with pre-roll and backpressure handling.
- `transcription.rs`: system audio transcription pipeline (WASAPI loopback, queue/chunking, post-capture flow).
- `models.rs`: model index, download, checksum and validation, model quantization.
- `state.rs`: persisted settings defaults/migrations and shared app state.
- `hotkeys.rs`: hotkey normalization, validation, conflict checks.
- `overlay.rs`: overlay window lifecycle and state updates.
- `postprocessing.rs`: rule-based text enhancement.
- `opus.rs`: OPUS encoding via FFmpeg subprocess.

## Core data flows

### Input capture (PTT)
1. PTT hotkey down starts mic capture.
2. PCM samples are buffered and level/meter events are emitted.
3. PTT release finalizes the chunk.
4. Backend transcribes and persists result.

### Input capture (Toggle/VAD)
1. Runtime monitors continuously while enabled.
2. Adaptive segmenter emits chunks by silence/interval/hard-cut rules.
3. Chunks are transcribed and appended to history.

### System audio capture (Windows)
1. Transcribe hotkey toggles monitoring.
2. WASAPI loopback stream is decoded, resampled, and metered.
3. Adaptive segmenter emits chunks for async transcription.
4. Results are appended to system history and conversation history.

### Export flow
1. User selects format (TXT/MD/JSON).
2. Frontend builds export text.
3. Backend `save_transcript` writes file via native dialog path.

## Runtime events (selected)

- `capture:state`, `audio:level`, `vad:dynamic-threshold`
- `transcribe:state`, `transcribe:level`, `transcribe:db`
- `continuous-dump:segment`, `continuous-dump:stats`
- `history:updated`, `transcribe:history-updated`
- `settings-changed`, `model:download-progress`

## Build and distribution

Installer variants:
- CUDA Edition (`src-tauri/tauri.conf.json`)
- Vulkan Edition (`src-tauri/tauri.conf.vulkan.json`)

The previous CUDA+Analysis variant is no longer part of mainline.
