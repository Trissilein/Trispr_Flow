# Architecture

Last updated: 2026-02-06

## Frontend architecture
- Stack: Tauri 2 + vanilla TypeScript + Vite
- Entry: `src/main.ts` bootstraps settings/devices/history/models, then wires listeners and UI rendering.
- State model: centralized mutable state in `src/state.ts` with explicit setter functions.
- UI model: direct DOM updates through dedicated modules (`src/settings.ts`, `src/ui-state.ts`, `src/history.ts`, `src/models.ts`).
- Event wiring: all user interactions are bound in `src/event-listeners.ts`.

## Backend architecture (`src-tauri/src`)
- `lib.rs`: command registration, app startup, tray/window integration, module wiring.
- `audio.rs`: microphone capture, VAD runtime, overlay level emission.
- `transcription.rs`: system audio transcription pipeline (WASAPI loopback, queue/chunking, post-capture flow).
- `models.rs`: model index, download, checksum and safety validation.
- `state.rs`: persisted settings defaults/migrations and shared app state.
- `hotkeys.rs`: hotkey normalization, validation, conflict checks.
- `overlay.rs`: overlay window lifecycle and state updates.
- `paths.rs`: app config/data path resolution.

## Core data flows
### Input capture (PTT)
1. PTT hotkey down starts mic capture.
2. PCM samples are buffered and level/meter events are emitted.
3. PTT release finalizes the chunk.
4. Whisper backend transcribes.
5. Result is persisted to mic history and emitted to UI for display/paste.

### Input capture (Voice Activation)
1. VAD monitor runs continuously while input capture is enabled.
2. Threshold + silence grace gate segment boundaries.
3. Finalized segments are transcribed and written to history.

### Output capture/transcription (Windows)
1. Transcribe hotkey toggles output monitoring for the selected device.
2. WASAPI loopback stream feeds chunker/VAD.
3. Chunks are transcribed and appended to output history.
4. UI receives `transcribe:state`, meter (`transcribe:level`/`transcribe:db`), and history update events.

### Conversation view
- Conversation tab merges mic + output histories into a single time-ordered stream.
- Detachable conversation window is supported and configurable work is planned on roadmap.

## Runtime events (selected)
- `capture:state`, `audio:level`, `vad:dynamic-threshold`
- `transcribe:state`, `transcribe:level`, `transcribe:db`
- `history:updated`, `transcribe:history-updated`
- `settings-changed`, `audio:cue`, `app:error`

## Quality status
- Unit tests and smoke test workflow are in place and locally verified.
- Optional Tauri E2E coverage remains backlog work.
