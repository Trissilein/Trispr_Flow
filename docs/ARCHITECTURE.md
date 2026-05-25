# Architecture

Last updated: 2026-05-23

## Frontend architecture

- Stack: Tauri 2 + vanilla TypeScript + Vite
- Entry: `src/main.ts` bootstraps settings/devices/history/models, then wires listeners and UI rendering.
- State model: centralized mutable state in `src/state.ts` with explicit setter functions.
- UI model: direct DOM updates through dedicated modules (`src/settings/index.ts` orchestrator + domain slices, `src/ui-state.ts`, `src/history.ts`, `src/models.ts`).
- Event wiring: user interactions are bound in `src/event-listeners.ts` (thin orchestrator) and domain-specific `src/wiring/*.wire.ts` modules.

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

| Module                               | Purpose                                                                                                     |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `main.ts`                            | Bootstrap, backend event listeners                                                                          |
| `state.ts`                           | Centralized mutable state                                                                                   |
| `types.ts`                           | TypeScript interfaces                                                                                       |
| `dom-refs.ts`                        | DOM element references                                                                                      |
| `event-listeners.ts`                 | Thin orchestrator (~30 lines); domain wiring in `src/wiring/`                                               |
| `wiring/app-chrome.wire.ts`          | Global tab/chrome wiring                                                                                    |
| `wiring/transcription.wire.ts`       | Transcription event and UI wiring                                                                           |
| `wiring/ai-refinement.wire.ts`       | AI Refinement event and UI wiring                                                                           |
| `wiring/overlay.wire.ts`             | Overlay event and UI wiring                                                                                 |
| `wiring/voice-output.wire.ts`        | Voice Output event and UI wiring                                                                            |
| `wiring/history.wire.ts`             | History UI wiring                                                                                           |
| `settings/index.ts`                  | Settings orchestrator; cross-domain render functions and inline Continuous Dump / Post-Processing rendering |
| `settings/vocabulary.settings.ts`    | Custom Vocabulary and Vocabulary Learning UI                                                                |
| `settings/overlay.settings.ts`       | Overlay appearance settings UI                                                                              |
| `settings/transcription.settings.ts` | Transcription language and VAD settings UI                                                                  |
| `settings/voice-output.settings.ts`  | Piper/TTS voice output settings UI                                                                          |
| `settings/ai-refinement.settings.ts` | AI Refinement provider, prompt, and model settings UI                                                       |
| `settings-persist.ts`                | `persistSettings`, `ensureSetupDefaults`, `syncDerivedLanguageSettings`                                     |
| `ui-state.ts`                        | Runtime UI status rendering                                                                                 |
| `history.ts`                         | History rendering, export, chapters, topics, search                                                         |
| `chapters.ts`                        | Chapter UI lifecycle                                                                                        |
| `models.ts`                          | Model list rendering and actions                                                                            |
| `devices.ts`                         | Audio/output device list rendering                                                                          |

## Backend architecture ([src-tauri/src](src-tauri/src))

- [src-tauri/src/lib.rs](src-tauri/src/lib.rs): App startup, tray/window integration, module wiring, and registration of the remaining 21 core commands.
- [src-tauri/src/ai_fallback/commands.rs](src-tauri/src/ai_fallback/commands.rs): Fallback-routing, Ollama connection, and refinement commands (18 commands).
- [src-tauri/src/workflow_agent.rs](src-tauri/src/workflow_agent.rs): Workflow engine handling assistant state, execution plans, prompt parsing, and confirmation commands (8 commands).
- [src-tauri/src/multimodal_io.rs](src-tauri/src/multimodal_io.rs): Vision stream, screen capture, and TTS voice output (speak/stop) commands.
- [src-tauri/src/history_partition.rs](src-tauri/src/history_partition.rs): History partition management and transcript database integrations.
- [src-tauri/src/gdd/confluence.rs](src-tauri/src/gdd/confluence.rs): Speeds publishing of specs directly to Confluence.
- [src-tauri/src/tts_benchmark.rs](src-tauri/src/tts_benchmark.rs): Latency benchmarks for TTS evaluation and models.
- [src-tauri/src/paths.rs](src-tauri/src/paths.rs): Security primitive focusing on sandboxed path traversal ([validate_path_within](src-tauri/src/paths.rs)).
- [src-tauri/src/session_manager.rs](src-tauri/src/session_manager.rs): Crash recovery state management ([save_crash_recovery](src-tauri/src/session_manager.rs)).
- [src-tauri/src/audio.rs](src-tauri/src/audio.rs): microphone capture, VAD runtime, overlay level emission.
- [src-tauri/src/continuous_dump.rs](src-tauri/src/continuous_dump.rs): adaptive segmenter for silence/interval/hard-cut chunking with pre-roll and backpressure handling.
- [src-tauri/src/transcription.rs](src-tauri/src/transcription.rs): system audio transcription pipeline (WASAPI loopback, queue/chunking, post-capture flow).
- [src-tauri/src/models.rs](src-tauri/src/models.rs): model index, download, checksum and validation, model quantization.
- [src-tauri/src/state.rs](src-tauri/src/state.rs): persisted settings defaults/migrations and shared app state.
- [src-tauri/src/hotkeys.rs](src-tauri/src/hotkeys.rs): hotkey normalization, validation, conflict checks.
- [src-tauri/src/overlay.rs](src-tauri/src/overlay.rs): overlay window lifecycle and state updates.
- [src-tauri/src/postprocessing.rs](src-tauri/src/postprocessing.rs): rule-based text enhancement.
- [src-tauri/src/opus.rs](src-tauri/src/opus.rs): OPUS encoding via FFmpeg subprocess, acting as an isolated security boundary ([encode_to_opus](src-tauri/src/opus.rs)).

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

Mainline installer packaging:
- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Bundled runtime folders: `bin/cuda/*` and `bin/vulkan/*` (+ `bin/quantize.exe`)

Backend selection is resolved at runtime (`local_backend_preference`: `auto|cuda|vulkan`).
The previous CUDA+Analysis variant is no longer part of mainline.
