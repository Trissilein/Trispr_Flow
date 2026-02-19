# Architecture

Last updated: 2026-02-19

## Frontend architecture

- Stack: Tauri 2 + vanilla TypeScript + Vite
- Entry: `src/main.ts` bootstraps settings/devices/history/models, then wires listeners and UI rendering.
- State model: centralized mutable state in `src/state.ts` with explicit setter functions.
- UI model: direct DOM updates through dedicated modules (`src/settings.ts`, `src/ui-state.ts`, `src/history.ts`, `src/models.ts`).
- Event wiring: all user interactions are bound in `src/event-listeners.ts`.

### UI Layout (v0.5.0 — Tab-Based)

The application uses a **two-tab layout** to separate daily-use views from configuration:

```
+---------------------------------------------------+
| Hero (status indicators, model info, toggles)     |
+---------------------------------------------------+
| [* Transcription]  [ Settings]                    |
+---------------------------------------------------+
| Tab content area                                  |
+---------------------------------------------------+
```

**Tab 1 — Transcription** (default):

- Full-width transcript panel with sub-tabs: Input | System Audio | Conversation
- Export controls (TXT/MD/JSON format selector + Export button)
- Search bar with text highlighting
- Chapter markers (conversation-only, optional via settings)
- History list (primary interaction surface during recording sessions)

**Tab 2 — Settings**:

- Capture Input (mic device, hotkeys, VAD, language, activation words)
- System Audio Capture (output device, transcribe hotkey, VAD, chunking, quality & encoding, analyse)
- Post-Processing (punctuation, capitalization, numbers, custom vocabulary)
- Model Manager (model sources, storage, download, quantization)
- UX / UI Adjustments (overlay settings, extra hotkeys)

**Design rationale**: Settings are "fire and forget" — configured once, then rarely touched. Separating them from the transcript view reduces cognitive load during active recording sessions. See DEC-016 in DECISIONS.md.

### Frontend module map

| Module | Purpose |
| --- | --- |
| `main.ts` | Bootstrap, backend event listeners, app version display |
| `state.ts` | Centralized mutable state with setters |
| `types.ts` | TypeScript interfaces (Settings, HistoryEntry, ModelInfo, etc.) |
| `dom-refs.ts` | ~140 DOM element references via `getElementById` |
| `event-listeners.ts` | All user interaction handlers (includes Voice Analysis setup/queue orchestration) |
| `settings.ts` | Settings persistence and UI sync (`renderSettings()`) |
| `ui-state.ts` | Runtime UI state (capture/transcribe status, hero rendering) |
| `history.ts` | History rendering, export, chapters, topics, search |
| `chapters.ts` | Chapter UI lifecycle (init, render, scroll-to, toggle visibility) |
| `models.ts` | Model list rendering, download triggers, model dir management |
| `devices.ts` | Audio device list rendering |
| `hotkeys.ts` | Hotkey recorder UI component |
| `toast.ts` | Toast notification system |
| `audio-cues.ts` | Audio cue playback (start/stop beeps) |
| `ui-helpers.ts` | Utility functions (dB conversion, formatting) |
| `accessibility.ts` | ARIA attribute management |
| `feedback-state.ts` | Recording/transcribing indicator state machine |
| `window-state.ts` | Window position/size persistence |

### Naming conventions

| UI Term | Internal Meaning |
| --- | --- |
| Input | Microphone transcription (PTT or VAD) |
| System Audio | System audio transcription via WASAPI loopback |
| Conversation | Combined Input + System Audio, time-sorted |
| Transcription (tab) | The main transcript view area |
| Settings (tab) | All configuration panels |

## Backend architecture (`src-tauri/src`)

- `lib.rs`: command registration, app startup, tray/window integration, module wiring.
- `audio.rs`: microphone capture, VAD runtime, overlay level emission.
- `transcription.rs`: system audio transcription pipeline (WASAPI loopback, queue/chunking, post-capture flow).
- `models.rs`: model index, download, checksum and safety validation, model quantization (q5_0 format).
- `state.rs`: persisted settings defaults/migrations and shared app state. Includes chapter persistence (`chapters.json`).
- `hotkeys.rs`: hotkey normalization, validation, conflict checks.
- `overlay.rs`: overlay window lifecycle and state updates.
- `paths.rs`: app config/data path resolution, quantize.exe resolution.
- `postprocessing.rs`: rule-based text enhancement (punctuation, capitalization, numbers, custom vocabulary).
- `opus.rs`: OPUS encoding via FFmpeg subprocess (WAV → OPUS conversion for smaller file sizes).
- `sidecar.rs`: HTTP client for VibeVoice-ASR sidecar communication (transcription requests, health checks).
- `sidecar_process.rs`: Sidecar lifecycle management (start/stop Python FastAPI process, health monitoring).
- `analysis_jobs.rs`: External analysis worker job manager (queued/running/completed/failed/canceled/timeout).
- `auto_processing.rs`: Post-transcription pipeline (chapter generation, meeting minutes extraction, summary generation).

### Voice Analysis Runtime Architecture

**Purpose**: VibeVoice-ASR 7B model requires Python + Transformers ecosystem and is isolated from the core app runtime.

**Communication Pattern**:

```text
Trispr Flow (Rust)  ←→  Voice Analysis Runtime
     │
     ├─ Legacy mode: HTTP sidecar (FastAPI) `/transcribe`
     └─ External mode: one-shot worker job (CLI process per analysis)
```

**Files**:

- Rust side: `sidecar.rs` (HTTP client), `sidecar_process.rs` (lifecycle)
- Rust job layer: `analysis_jobs.rs` (job registry + worker launch + cancel/timeout)
- Python side: `sidecar/vibevoice-asr/main.py` (FastAPI app), `sidecar/vibevoice-asr/worker_once.py` (one-shot worker), `model_loader.py`, `inference.py`

**Lifecycle**:

Legacy sidecar mode:
1. User triggers analysis → Rust calls `start_sidecar()`.
2. `sidecar_process.rs` spawns Python FastAPI via `Command::new(python).arg(main.py)`.
3. Health check loop waits for `/health` (30s timeout).
4. Rust sends `/transcribe` request with audio path.

External worker mode (feature-flagged, default OFF):
1. UI calls `analysis_create_job`.
2. `analysis_jobs.rs` creates a tracked job and spawns `worker_once.py` as a dedicated process.
3. UI polls `analysis_get_job`/`analysis_get_jobs` and can cancel via `analysis_cancel_job`.
4. Worker exits after one transcription; app remains responsive even on runtime failures/timeouts.

**Packaging**: Runtime supports both bundled sidecar exe and Python fallback. Current installers ship Python sidecar files plus setup script; `sidecar_process.rs` auto-detects bundled exe first, then falls back to Python.

**Parallel Mode**: `parallel_transcribe` command runs Whisper + VibeVoice concurrently for diagnostics. Voice Analysis UI keeps VibeVoice as source of truth.

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
4. For persisted session audio (`session.opus`), overlap prefixes are stripped from follow-up chunks to avoid duplicate audio at chunk boundaries.
5. UI receives `transcribe:state`, meter (`transcribe:level`/`transcribe:db`), and history update events.

### Export flow

1. User selects format (TXT/MD/JSON) from dropdown in Transcription tab.
2. Clicks Export button.
3. Frontend builds formatted text via `buildExportText()`.
4. Invokes Tauri `save_transcript` command with filename, content, format.
5. Backend opens native file save dialog (via `rfd` crate).
6. Content written to selected file path.
7. Success/error toast shown to user.

### Model Manager and Optimization

1. Users browse installed and available models in the Model Manager panel.
2. **Optimize** button quantizes full-size `.bin` models to `q5_0` format (~30% size reduction).
3. Quantized model is created as separate entry (e.g., `model-q5_0.bin`) without restart.
4. Users can apply quantized models immediately for faster inference with minimal accuracy loss.

### Conversation view

- Conversation tab merges mic + output histories into a single time-ordered stream.
- Chapter markers segment long conversations (silence-based, time-based, or hybrid).
- Topic detection identifies content categories via keyword matching.

## Runtime events (selected)

- `capture:state`, `audio:level`, `vad:dynamic-threshold`
- `transcribe:state`, `transcribe:level`, `transcribe:db`
- `history:updated`, `transcribe:history-updated`
- `settings-changed`, `audio:cue`, `app:error`
- `model:download-progress`, `model:download-complete`, `model:download-error`
- `transcribe:backlog-warning`, `transcribe:backlog-expanded`

## Build and Distribution

### Dual Installer System

Two installer variants are built via `build-both-installers.bat`:

- **CUDA Edition** (~93 MB): Includes NVIDIA CUDA runtime (cublas64_13.dll, cudart64_13.dll) + Vulkan backend. For NVIDIA GPU users.
- **Vulkan Edition** (~9 MB): Vulkan backend only. For AMD/Intel GPU users or minimal installs.

Both use NSIS packaging with language selection (English/German).

### Version Management

- Version source of truth: `package.json` version field
- Tauri configs (`tauri.conf.json`, `tauri.conf.vulkan.json`) mirror the version
- App version displayed in UI header via Tauri `getVersion()` API

## Quality status

- Unit tests and smoke test workflow are in place and locally verified.
- Export serialization: 28 unit tests covering TXT/MD/JSON formats, edge cases, and chapter generation.
- Block E E2E: 22 tests covering diarization, analysis, quality presets, parallel mode, and full workflow.
- Optional Tauri E2E coverage remains backlog work.
