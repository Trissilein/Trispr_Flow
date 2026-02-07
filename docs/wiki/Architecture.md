# Architecture

## Frontend
- Stack: Tauri 2 + vanilla TypeScript + Vite
- Entry: `src/main.ts` bootstraps settings/devices/history/models, then wires listeners and UI rendering
- State model: centralized mutable state in `src/state.ts` with explicit setters
- UI model: direct DOM updates through dedicated modules (`src/settings.ts`, `src/ui-state.ts`, `src/history.ts`, `src/models.ts`)
- Event wiring: `src/event-listeners.ts`

## Backend (`src-tauri/src`)
- `lib.rs`: command registration, startup, tray/window integration
- `audio.rs`: microphone capture, VAD runtime, overlay level emission
- `transcription.rs`: system audio transcription pipeline (WASAPI loopback, queue/chunking)
- `models.rs`: model index, download, checksum and safety validation
- `state.rs`: persisted settings defaults and app state
- `hotkeys.rs`: hotkey normalization, validation, conflict checks
- `overlay.rs`: overlay lifecycle and state updates
- `paths.rs`: app config/data path resolution

## Core data flows
### Input capture (PTT)
1. PTT hotkey down starts mic capture.
2. PCM samples are buffered and level/meter events are emitted.
3. PTT release finalizes the chunk.
4. Whisper backend transcribes.
5. Result is written to mic history and emitted to UI.

### Input capture (Voice Activation)
1. VAD monitor runs continuously while input capture is enabled.
2. Threshold + silence grace gate segment boundaries.
3. Finalized segments are transcribed and written to history.

### Output capture (Windows)
1. Transcribe hotkey toggles output monitoring for the selected device.
2. WASAPI loopback stream feeds chunker/VAD.
3. Chunks are transcribed and appended to output history.
4. UI receives `transcribe:state`, meter, and history update events.
