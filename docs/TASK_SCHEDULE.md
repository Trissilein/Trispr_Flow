# Task Schedule - Trispr Flow

Last updated: 2026-03-28 (Agent-Evolution roadmap accepted; Block V inserted after Block T)

## Overview

This file is the detailed execution log and block-level task table.
For current priorities and dependency ordering, use `ROADMAP.md` as source of truth.

Trispr Flow development uses an optimized **batched task schedule strategy** that reduces model switching overhead from 20+ to just **5 strategic model switches**. This approach groups tasks by AI model (Haiku, Sonnet, Opus) to maximize efficiency and context reuse.

### Batched Schedule Strategy

Instead of context-switching between models for each task, tasks are organized into **blocks** where the same model handles multiple related tasks sequentially. This reduces:

- Context initialization overhead
- Model loading/unloading cycles
- Cognitive friction between task transitions
- Overall development time by ~30-40%

**Model Assignment Philosophy**:

- **Haiku**: Quick tasks, unit tests, simple refactoring, documentation fixes
- **Sonnet**: Feature development, complex UI work, integration tasks, testing
- **Opus**: Critical architecture decisions, deep refactoring, complex integrations, review/optimization

---

## v0.5.0 Schedule: Long-Form Features + Tab-Based UI Refactor

**Timeline**: 4 weeks | **Model Switches**: 2

### Block A: Haiku Sprint --- COMPLETE

**Duration**: 1 week | **Model**: Claude Haiku | **Status**: All 7 tasks complete

| Task | Name | Status | Description |
| --- | --- | --- | --- |
| 1 | Research long-form transcription formats | DONE | Investigated TXT, MD, JSON. Validated existing implementation. |
| 2 | Design TXT/MD/JSON export schema | DONE | Schema documented in EXPORT_SCHEMA.md. format_version 1.0. |
| 4 | Add export format selector UI component | DONE | Dropdown in Output panel toolbar (index.html lines 115-122). |
| 7 | Unit tests for export serialization | DONE | 28 tests in history.test.ts covering all formats and edge cases. |
| 8 | Documentation: Export feature guide | DONE | EXPORT_GUIDE.md (8KB) + EXPORT_SCHEMA.md (9KB). |
| 11 | Integrate export toggle with settings state | DONE | Stateless format selection (DEC-014). No persistence needed. |
| 12 | Build export feature (internal commands) | DONE | Tauri save_transcript command (lib.rs:632-659) + event handler. |

**Key findings**: Export functionality was already substantially implemented. Block A validated, documented, and tested all existing code.

### Block B: Sonnet Sprint --- COMPLETE ✅

**Duration**: 3 weeks | **Model**: Claude Sonnet | **Status**: All 8 tasks complete

**Architecture decisions made before Block B** (see DECISIONS.md):

- DEC-016: Tab-Based UI Refactor (Transcription + Settings tabs)
- DEC-017: "Output" renamed to "System Audio"
- DEC-018: Chapters conversation-only by default

| Task | Name | Status | Description |
| --- | --- | --- | --- |
| B1 | Tab-Based UI Refactor: HTML restructuring | DONE | Tab bar added, panels wrapped, renamed Output to System Audio. |
| B2 | Tab-Based UI Refactor: TypeScript + CSS | DONE | Tab switching logic, localStorage persistence, responsive layout. |
| B3 | Naming cleanup (Output to System Audio) | DONE | All references updated across frontend and docs. |
| B4 | Chapter settings integration | DONE | Chapter settings in Settings struct (Rust + TypeScript). |
| B5 | Chapter UI: conversation-only display | DONE | Chapters shown in conversation tab only by default. |
| B6 | Topic detection UI: badges + filter | DONE | Topic badges + keyword filtering + Settings panel. |
| B7 | Live transcript dump (background buffering) | DONE | Crash recovery buffering via live-dump.ts (5-sec interval). |
| B8 | End-to-end test: capture to export to verify | DONE | Block B E2E tests added to history.test.ts. |

**Files affected by Block B**:

- `index.html` — Major restructuring (tab bar, container wrapping, naming)
- `src/dom-refs.ts` — New tab button/container refs
- `src/event-listeners.ts` — Tab switching events, naming updates
- `src/history.ts` — Tab naming, chapter conditional logic
- `src/chapters.ts` — Conditional visibility based on settings
- `src/state.ts` — Optional tab state
- `src/types.ts` — Settings additions (chapter fields), MainTab type
- `src/settings.ts` — Chapter settings rendering
- `src/styles.css` — Tab bar styles, layout grid adjustments
- `src-tauri/src/state.rs` — Chapter settings fields in Settings struct

**Known bugs to fix during Block B**:

- `postprocessing` panel missing from CSS grid-area assignments
- `postprocessing` missing from `initPanelState()` panel ID list (history.ts:455)

---

## v0.6.0 Schedule: VibeVoice-ASR Core + Auto-Processing

**Timeline**: 6-8 weeks | **Model Switches**: 2

### Block C: Haiku Sprint --- COMPLETE ✅

**Duration**: 1 week | **Model**: Claude Haiku | **Status**: All 5 tasks complete

| Task | Name | Status | Description |
| --- | --- | --- | --- |
| C14 | Research VibeVoice-ASR model and architecture | DONE | Documented in VIBEVOICE_RESEARCH.md (18KB). |
| C16 | Design sidecar project structure (FastAPI) | DONE | Created sidecar/vibevoice-asr/ structure. |
| C17 | Set up FastAPI sidecar skeleton + `/transcribe` endpoint | DONE | main.py, model_loader.py, inference.py, config.py. |
| C20 | Design OPUS recording pipeline (FFmpeg integration) | DONE | Documented in OPUS_PIPELINE_DESIGN.md (12KB). |
| C22 | Implement OPUS encoding in Rust (FFmpeg wrapper) | DONE | opus.rs with FFmpeg subprocess wrapper. |

### Block D: Opus Sprint --- COMPLETE ✅

**Duration**: 1.5 weeks | **Model**: Claude Opus | **Status**: All 5 tasks complete

| Task | Name | Status | Description |
| --- | --- | --- | --- |
| D15 | Architect VibeVoice-ASR integration layer | DONE | sidecar.rs with HTTP client, error handling, timeouts. |
| D18 | Implement VibeVoice-ASR model loading + inference | DONE | main.py with /transcribe endpoint, model loading stubs. |
| D21 | Implement FP16/INT8 configuration in Rust | DONE | vibevoice_precision setting in state.rs + types.ts. |
| D23 | Implement sidecar process management (start/stop/health) | DONE | sidecar_process.rs with lifecycle management. |
| D25 | Auto-processing pipeline (chapters, minutes, summary) | DONE | auto_processing.rs with chapter/minutes/summary generation. |

### Block E: Sonnet Sprint 2 (Sonnet tasks) --- COMPLETE ✅

**Duration**: 2.5 weeks | **Model**: Claude Sonnet | **Status**: Sonnet tasks complete

| Task | Name | Complexity | Status | Description |
| --- | --- | --- | --- | --- |
| E19 | Speaker-diarized transcript UI | High | DONE | Color-coded speaker segments, speaker label editing, export. |
| E24 | "Analyse" button + transcript view | High | DONE | UI button to trigger analysis, file picker, auto-save OPUS recordings. |
| E26 | Quality preset controls (OPUS + VibeVoice) | Medium | DONE | OPUS bitrate dropdown (32/64/96/128), VibeVoice precision (FP16/INT8), OPUS toggle. |
| E28 | Model monitoring + notifications | Medium | DONE | Weekly VibeVoice update check, toast notification on startup. |

### Block E: Opus Sprint (remaining tasks) --- COMPLETE ✅

**Duration**: 1.5 weeks | **Model**: Claude Opus | **Status**: All tasks complete

| Task | Name | Complexity | Status | Description |
| --- | --- | --- | --- | --- |
| E27 | Parallel analysis mode toggle | Medium | DONE | Whisper + VibeVoice simultaneous mode. System audio auto-save (60s flush). |
| E29 | PyInstaller packaging for sidecar | Medium | DONE | PyInstaller spec + build script. Sidecar auto-detects bundled exe vs Python. |
| E30 | E2E test: record to analyse to verify | High | DONE | 22 tests covering full workflow: diarization, analysis, quality presets, parallel mode. |

---

## v0.6.1 Stabilization Packet: Adaptive Continuous Dump (Mic + System)

**Timeline**: 2-3 days | **Model Switches**: 1 | **Status**: Complete

| Task | Name | Complexity | Status | Description |
| --- | --- | --- | --- | --- |
| CD1 | Adaptive segmenter module | High | DONE | Added `continuous_dump.rs` with hybrid flush logic (silence + soft interval + hard cut), pre-roll, min-chunk merge, backpressure scaling. |
| CD2 | Settings schema extension + migration | High | DONE | Added continuous dump fields + profile defaults + legacy mapping from transcribe interval/overlap/silence fields. |
| CD3 | System audio pipeline integration | High | DONE | Replaced static chunk slicing with adaptive segmenter in WASAPI loopback path; added runtime telemetry events. |
| CD4 | Mic toggle-mode integration | High | DONE | Added continuous toggle processor for mic with adaptive chunking and per-chunk transcription flow. |
| CD5 | Per-source session finalization | Medium | DONE | Session manager now tracks source-specific active sessions (`mic`, `output`) and finalizes independently. |
| CD6 | UI controls and wiring | Medium | DONE | Added profile + advanced controls + per-source overrides + mic auto-save toggle in Settings panel. |
| CD7 | Validation and regression tests | Medium | DONE | `npm run build`, `npm test`, `cargo test` green; adaptive segmenter unit tests added and fixed. |

---

## v0.7.x Schedule: AI Fallback Overhaul

**Timeline**: Rolling execution in dependency blocks (offline-first before cloud providers)

### Block F: Haiku Quick ✅ COMPLETE

**Duration**: 0.5 weeks | **Model**: Claude Haiku | **Status**: Planning Complete

| Task | Name | Status | Description |
| --- | --- | --- | --- |
| 39 | Requirements and UX decision (AI Fallback rename) | DONE | ✅ Terminology: "AI Fallback" (DEC-023) |
| 39a | Settings location decision | DONE | ✅ Expander in Post-Processing panel (DEC-024) |
| 39b | Execution sequence decision | DONE | ✅ Local rules → AI Fallback pipeline (DEC-025) |
| 39c | V0.7.0 Planning document | DONE | ✅ V0.7.0_PLAN.md with full architecture overview |

**Deliverables**:

- V0.7.0_PLAN.md: 280 lines, full planning doc with provider architecture, settings schema, UI mockups
- DECISIONS.md: 3 new decisions (DEC-023, DEC-024, DEC-025)
- Design decisions documented: terminology, settings layout, execution flow

### Block G: Opus Sprint --- COMPLETE

**Duration**: 1.5 weeks | **Model**: Claude Opus | **Status**: Complete

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 31 | Design multi-provider architecture (Claude, OpenAI, Gemini) | High | Task 39 | DONE | Plan settings schema, API key storage, provider-specific limits. |
| 36 | Implement provider data model and settings migration | High | Task 31 | DONE | Update settings.json schema. Migration from old `cloud_fallback` to new structure. |
| 37 | Implement provider config UI (API keys, model selection) | High | Task 31 | DONE | Create Settings panel for provider/model/key management. |

### Block H: Offline-First Ollama Sprint --- COMPLETE ✅

**Duration**: 2 weeks | **Model**: Claude Sonnet | **Status**: All 5 tasks complete

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 32 | Implement Ollama provider integration (backend) | High | Task 31 | DONE | `OllamaProvider` hardened: `keep_alive: "-1"`, 60s read timeout, 5s connect timeout, `list_ollama_models_with_size` + `fetch_ollama_models_with_size` Tauri command. |
| 33 | Activate AI refinement pipeline stage (local provider) | High | Task 32 | DONE | `maybe_spawn_ai_refinement` helper in audio.rs; wired at all 3 transcription:result emit sites; emits `transcription:refined` / `transcription:refinement-failed`; frontend listeners in main.ts. |
| 34 | Implement Ollama-only provider UX | Medium | Tasks 32, 33 | DONE | UI already complete from Block G: endpoint input, Refresh/Test/Save buttons, ollama-specific section, API-key section hidden for Ollama. |
| 35 | Implement local-model prompt strategy polish | Medium | Task 33 | DONE | EN/DE prompts updated: no-translate guard, output-only instruction, proper-noun preservation, German register (Du/Sie) preservation. |
| 38 | End-to-end test: offline refinement reliability | High | Tasks 32, 33, 34, 35 | DONE | 24 TypeScript tests (block-h-ollama.test.ts: H-S1–H-S5); 8 new Rust unit tests for prompt guards, connection refused → OllamaNotRunning, size-list consistency. |

### Block I: Cloud Provider Rollout --- DEFERRED

**Duration**: 2+ weeks | **Status**: Deferred until Block H stabilization

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 40 | OpenAI provider integration | High | Task 31, Block H | DEFERRED | Add OpenAI API client after local path is release-stable. |
| 41 | Anthropic (Claude) provider integration | High | Task 31, Block H | DEFERRED | Claude API client and model mapping after offline release. |
| 42 | Gemini provider integration | High | Task 31, Block H | DEFERRED | Gemini API client after offline-first milestone. |

### Block J: Adaptive AI Refinement Intelligence --- PLANNED

**Duration**: 2-3 weeks | **Model**: Claude Sonnet | **Status**: Planned (after Block E)

Two features that make the AI refinement pipeline more transparent and self-improving over time.

#### J1 — Hardware Requirements Indicator

When a user enables AI Fallback or selects a model, the UI should display VRAM requirements for the chosen model and warn if the GPU is likely insufficient.

**User-facing behaviour:**

- AI Fallback settings show estimated VRAM per model next to the model name (e.g., `qwen3:8b · ~5.9 GB VRAM`)
- If detected VRAM < model requirement → amber warning banner: *"This model may run on CPU (~1–5 tok/s, 30–120 s per chunk). Consider `qwen3:8b` for faster processing."*
- If no GPU detected → red warning: *"No GPU detected. AI refinement will use CPU and may be slow."*

**Implementation notes:**

- Tauri command `get_gpu_info` → Rust: try `nvml-wrapper` (NVIDIA), fall back to `wgpu` adapter query, fall back to `{ vram_mb: null }`.
- Model VRAM table embedded in frontend (static lookup by quantization tier from `list_ollama_models_with_size` output).
- Warning renders below the model selector in the AI Fallback settings panel.
- No Ollama API call required — size info already available from Block H's `fetch_ollama_models_with_size`.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 43 | GPU VRAM detection (Tauri backend) | Medium | Block H | PLANNED | Rust command `get_gpu_info` returning `{vram_mb: Option<u64>, gpu_name: Option<String>}` via nvml-wrapper → wgpu fallback. |
| 43a | VRAM requirement display in AI Fallback UI | Low | Task 43 | PLANNED | Model selector shows size badge; warning banner renders when VRAM < model threshold. |

#### J2 — Adaptive Vocabulary (Self-Learning from AI Refinement)

When AI refinement consistently replaces the same word or phrase across multiple transcripts, the system should automatically propose (or auto-add) that substitution as a vocabulary rule — so Whisper learns the user's domain vocabulary over time.

**User-facing behaviour:**

- After a correction fires ≥ 3 times (configurable), the system auto-adds it to the custom vocabulary as a substitution rule: `original_word → corrected_word`.
- Optional: surface a *"Learned X new vocabulary rules this session"* toast after a session ends.
- Settings panel: toggle `Auto-learn vocabulary from AI refinement` (default: enabled). Sub-section shows learned rules with ability to accept/reject individually.

**Implementation notes:**

- Frontend (TypeScript): listen to `transcription:refined` events → compute word-level diff between `original` and `refined` (`diffWords` or a simple tokenised comparison).
- Maintain a `Map<string, Map<string, number>>` (`original → refined → count`) in session memory and persisted as `learned_vocabulary` in settings JSON.
- Threshold reached → call `update_settings` to append the rule to the existing vocabulary list.
- Learned rules are indistinguishable at runtime from manually entered ones — same pipeline, same Rust processing.
- Vocabulary rules are applied at Stage 1 (local rule processing), before AI fallback. The self-learning loop therefore improves the base transcript so the AI has less to fix over time.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 44 | Word-diff extraction from refinement events | Medium | Block H | PLANNED | TypeScript: compare `original` vs `refined` payload from `transcription:refined`; extract word-level substitutions; accumulate in session map. |
| 44a | Persistence and threshold logic | Medium | Task 44 | PLANNED | Persist `learned_vocabulary` map to settings JSON. Auto-promote substitution → vocabulary rule after N occurrences (default 3). |
| 44b | Learned vocabulary settings UI | Medium | Task 44a | PLANNED | Settings sub-panel: toggle auto-learn, list of learned rules with accept/reject, session toast notification. |
| 44c | Adaptive vocabulary regression tests | Medium | Tasks 44, 44a | PLANNED | Unit tests: diff extraction correctness, threshold promotion, persistence round-trip, rule deduplication. |

### Block K: Expert Mode UX Overhaul --- PLANNED

**Duration**: 2-3 weeks | **Model**: Claude Sonnet | **Depends on**: Block E | **Status**: Planned

A persistent toggle separates the app into two modes:

- **Standard mode** (default): Only essential settings visible — device, language, hotkeys, enable/disable toggles, AI Refinement tab (full).
- **Expert mode**: Reveals all timing/threshold/buffer controls (VAD thresholds, grace periods, chunk sizes, continuous dump parameters, overlay customization, chapters method, temperature/tokens/custom prompt in AI Refinement).

Implementation uses `data-expert-only` attributes on DOM elements — CSS hides them in standard mode, shows them in expert mode. No settings schema change needed; purely a display filter.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| K1 | Expert-mode toggle (header/settings, localStorage persistence) | Low | Block E | DONE | Toggle implemented in Settings tab, persisted via localStorage `trispr-expert-mode`, adds `expert-mode`/`standard-mode` root classes for follow-up K3. |
| K2 | Audit & classify all settings (agent-assisted) | Medium | Block E | PLANNED | Agent reviews every settings field; outputs two lists: standard vs expert. Decision document added to DECISIONS.md. |
| K3 | Apply `data-expert-only` attributes + CSS hide/show | Medium | K1, K2 | PLANNED | Add attribute to expert-only elements; CSS rule `.expert-mode [data-expert-only]` toggles visibility. |
| K4 | Settings re-ordering within panels (expert items sink to bottom) | Medium | K3 | PLANNED | Visual grouping: essential controls at top, expert controls below a subtle divider. |
| K5 | Regression tests (mode toggle shows correct subsets) | Low | K3, K4 | PLANNED | Unit tests verify standard mode hides expert elements; expert mode shows all. |

---

### Block L: Module Platform + GDD Automation --- COMPLETE

**Duration**: 4-6 weeks | **Model**: Claude Opus + Sonnet | **Depends on**: v0.7.1 stabilization (Blocks E/F) | **Status**: Complete

Goal: Introduce a managed module platform and deliver a production-ready first module that turns transcripts into strict GDD drafts and publishes to Confluence Cloud.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| L1 | Module registry + lifecycle core (Rust) | High | E, F | DONE | Managed module states, dependency checks, lifecycle orchestration and command surface are implemented. |
| L2 | Settings schema migration for modules | High | L1 | DONE | `module_settings`, `gdd_module_settings`, `confluence_settings` with safe normalization/migration are live. |
| L3 | Module health/update commands | Medium | L1 | DONE | `get_module_health` and `check_module_updates` implemented and emitted to UI. |
| L4 | Modules tab UI shell | Medium | L1 | DONE | Modules tab with cards, status badges, dependencies and actions implemented. |
| L5 | Permission consent UX | Medium | L4 | DONE | Consent gating before first enable is implemented and persisted. |
| L6 | Analyse button -> module launcher migration | Low | L4 | DONE | Analyse now routes to Modules tab and focuses module launcher path. |
| L7 | Universal strict GDD preset schema | High | L2 | DONE | Universal strict preset and section schema implemented. |
| L8 | Clone-preset persistence and editor API | Medium | L7 | DONE | Clone preset list/save API implemented with validation. |
| L9 | Preset recognition engine | Medium | L7, L8 | DONE | Heuristic scoring with confidence/candidates/reasoning implemented. |
| L10 | Token-safe extraction pipeline | High | L7 | DONE | Chunked extraction + synthesis pipeline implemented. |
| L11 | GDD synthesis + validation | High | L10 | DONE | Strict draft generation with `TBD` fallback + validation command implemented. |
| L12 | Draft rendering (Markdown + Confluence storage) | Medium | L11 | DONE | Markdown and Confluence storage rendering implemented and wired. |
| L13 | Confluence auth and secret handling | High | L2 | DONE | OAuth exchange/refresh + API-token mode + keyring/file fallback implemented. |
| L14 | Confluence discovery and routing suggestion | High | L13 | DONE | Space listing + target suggestion implemented and integrated in GDD flow. |
| L15 | Confluence publish create/update lifecycle | High | L14, L12 | DONE | Create/update publish lifecycle implemented with version bump handling. |
| L16 | Review flow + one-click mode policy | Medium | L11, L15 | DONE | One-click publish now enforces confidence threshold + explicit confirmation fallback; policy covered in `src/tests/gdd-policy.test.ts`. |
| L17 | E2E + resilience tests | High | L1-L16 | DONE | Build/test/check gate is green, plus dedicated Rust queue/retry/conflict suites (`gdd::publish_queue`, `gdd::confluence`). |
| L18 | Documentation and rollout packet | Medium | L17 | DONE | Workflow docs + rollout packet completed (`docs/V0.8.0_BLOCK_L_ROLLOUT_PACKET.md`) and roadmap pointers updated. |

---

### Block M: Workflow-Agent Voice Automation --- COMPLETE ✅

**Duration**: 5-6 weeks | **Model**: Claude Opus + Sonnet | **Depends on**: Block L hardening + Block F | **Status**: Complete ✅ (closed 2026-03-08)

Goal: Ship an optional `workflow_agent` module that converts wakeword-triggered transcript commands into safe plan+confirm GDD execution.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| M1 | Core/module semantics cleanup (GDD core always-on) | High | L1, L2 | DONE | Module descriptor now supports `core` + `toggleable`; `gdd` and `integrations_confluence` are core-always-on. |
| M2 | Workflow-agent settings schema + migration | High | M1 | DONE | Added `workflow_agent` settings defaults/normalization in Rust + frontend types. |
| M3 | Raw command channel | High | M1 | DONE | New backend event `transcription:raw-result` emitted before activation-word drop filters. |
| M4 | Agent parse + session search commands | High | M2, M3 | DONE | Added `agent_parse_command` and `search_transcript_sessions` with gap-based session grouping and scoring. |
| M5 | Plan builder + execute commands | High | M4 | DONE | Added `agent_build_execution_plan` and `agent_execute_gdd_plan` (draft + publish/queue path). |
| M6 | Agent event bus wiring | Medium | M5 | DONE | Added `agent:*` progress/finish/fail events and frontend listeners. |
| M7 | Agent Console UI (Modules tab) | Medium | M4, M6 | DONE | Added Workflow Agent Console with parse, candidate select, language target, plan, and execute controls. |
| M8 | Wakeword runtime hookup | Medium | M3, M7 | DONE | Frontend listens to `transcription:raw-result`, detects wakeword, and triggers parser pipeline. |
| M9 | Candidate confirm hardening | Medium | M7 | DONE | Removed auto-select; disambiguation warning when top-2 score diff < 0.1; topic/temporal hint feedback in log. |
| M10 | Language target enforcement UX | Medium | M7 | DONE | `languageExplicitlySet` flag resets per parse; backend validates `target_language` against ALLOWED_LANGUAGES. |
| M11 | Workflow-agent regression tests | High | M8, M9, M10 | DONE | 16 Rust unit tests (parse_command/score_sessions/build_sessions) + 14 TS tests (WA-S1/S2/S3). All 154 tests green. |
| M12 | v0.8.1 release hardening | High | M11 | DONE | ROADMAP + CHANGELOG + TASK_SCHEDULE updated; .claude/ROADMAP.md redirect stub created. |

---

### Block Q: Onboarding Refinement & Stability --- IN PROGRESS

**Duration**: 1 week | **Model**: Gemini (Sonnet/Opus equivalent) | **Status**: In Progress

Goal: Ensure a smooth, robust first-time user experience and fix critical startup bugs.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| Q1 | Fix Onboarding Import Bug | Low | - | DONE | Resolved SyntaxError by adding `updateSettings` to state.ts and fixing imports. |
| Q2 | Functional Hotkey Setup | Medium | Q1 | DONE | Enabled real hotkey recording in Step 3 of the wizard. |
| Q3 | Deep Merge Settings Update | Medium | Q1 | DONE | Refactored `updateSettings` to handle nested objects (setup, ai_fallback) robustly. |
| Q4 | Hardware Detection Timeout | Low | Q1 | DONE | Added 8s timeout to `get_hardware_info` in wizard to prevent UI hang on slow systems. |
| Q4b | Hardware Detection Spinner | Low | Q4 | DONE | Added visual loading spinner and state management during GPU detection. |
| Q5 | UI Polishing (Wizard) | Low | Q2 | DONE | Added styling for hotkey box and finish animation in styles-modern.css. |
| Q6 | Robustness Audit (Backend) | Medium | - | DONE | Verified atomic settings save and non-blocking GPU detection in Rust. |
| Q7 | Startup diagnostics + staged bootstrap hardening | High | Q6 | DONE | Added startup/runtime diagnostics model + frontend staged bootstrap with readiness gating and runtime drift handling. |
| Q8 | Overlay + refinement lifecycle resilience | High | Q7 | DONE | Added overlay health signaling and refinement activity reconciliation to prevent stuck UI states after timeout/watchdog paths. |

---


### Block N: Multimodal I/O Modules --- FOUNDATION COMPLETE

**Duration**: 5-6 weeks | **Model**: Claude Opus + Sonnet | **Depends on**: Block M + Block L | **Status**: Foundations complete (`N1-N12`), tail work rescheduled

Goal: Add optional capability modules `input_vision` and `output_voice_tts` and bridge them to `workflow_agent`.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| N1 | Multimodal settings schema + migration | High | M2 | DONE | Added `vision_input_settings` + `voice_output_settings` defaults/normalization. |
| N2 | Module registry + permissions for Vision/TTS | Medium | N1 | DONE | Added `input_vision` and `output_voice_tts` manifests and permissions (`screen_capture`, `audio_output`). |
| N3 | Vision command surface | High | N2 | DONE | Added `list_screen_sources`, `start_vision_stream`, `stop_vision_stream`, `get_vision_stream_health`, `capture_vision_snapshot`. |
| N4 | TTS command surface | High | N2 | DONE | Added `list_tts_providers`, `list_tts_voices`, `speak_tts`, `stop_tts`, `test_tts_provider`. |
| N5 | Vision runtime hardening | High | N3 | IN PROGRESS | Executed via `N5+Q Stabilization Packet` in three delivery packets (P1-P3). |
| N5a | Runtime diagnostics + startup status event surface | High | N3 | DONE | Added `get_startup_status`, `get_runtime_diagnostics`, `startup:status`, `runtime:diagnostics`, and frontend/runtime integration. |
| N5b | Overlay health + resilient state replay | High | N5a | DONE | Added overlay controller recovery path and `overlay:health` signaling with frontend diagnostics hints. |
| N5c | Vision frame pipeline + bounded RAM buffer | High | N5a | DONE | Replaced metadata-only stream with capture pipeline (`capture_vision_frame`) and in-memory ring buffer stats/telemetry. |
| N5d | Vision/diagnostics regression validation packet | Medium | N5b, N5c | DONE | Regression packet finalized in `src/tests/n5d-regression.test.ts` with startup/overlay/module-health/runtime/partial-availability coverage plus vision buffer/snapshot contract checks (S1-S6). |
| N6 | Local custom TTS backend hardening | High | N4 | DONE | Piper TTS integrated: `speak_piper()` + `play_wav_blocking()` in `multimodal_io.rs`; 4-level binary/model auto-discovery; `VoiceOutputSettings` extended; NSIS installer bundles `piper.exe`, DLLs, `espeak-ng-data/`, `de_DE-thorsten-medium` + `en_US-amy-medium` models; `scripts/setup-piper.ps1` downloads assets. |
| N7 | Agent capability bridge | Medium | M8, N3, N4 | DONE | `agent_execute_gdd_plan` now consumes multimodal capabilities via workflow-agent: optional vision snapshot context injection (`vision_context*` progress stages) and agent-driven TTS feedback when capability modules are active. |
| N8 | Voice output policy enforcement | Medium | N4 | DONE | `speak_tts` now enforces `voice_output_settings.output_policy` using request context (`agent_reply`, `agent_event`, `manual_user`, `manual_test`); `test_tts_provider` uses explicit `manual_test` context. |
| N9 | Privacy + consent UX hardening | Medium | N5 | DONE | Modules UX now uses module-specific Vision/TTS consent messaging, richer enable-confirmation details with privacy scope notes, and a pending-consent summary in Modules header status. |
| N10 | TTS provider fallback matrix | Medium | N6 | DONE | Added deterministic fallback executor (`primary -> fallback`) with explicit matrix error codes, structured `tts:speech-*` diagnostics payloads, and fallback-visible `test_tts_provider` results in UI. |
| N11 | Benchmark track (>=3 runs/provider/scenario) | Medium | N6, N10 | DONE | Benchmark harness delivered (`run_tts_benchmark` + `scripts/tts-benchmark.ps1`) and validated with Windows evidence run (`measure_runs=3`): `windows_native` recommended (`success_rate=100%`, `p50=245ms`, `p95=282ms`); `local_custom` failed in run due missing Piper binary. |
| N12 | Vision/TTS integration tests | High | N7, N8 | DONE | Added `src/tests/n12-multimodal-integration.test.ts` covering vision command contracts, TTS fallback behavior, policy matrix, multimodal module health scenarios, and voice-output UI integration (16 tests). |
| N13 | E2E agent automation with multimodal IO | High | N11, N12 | MOVED -> S | Rescheduled into `Block S` as part of stabilization + decoupling acceptance. |
| N14 | v0.8.2 release hardening | High | N13 | MOVED -> S | Rescheduled into `Block S` release gating path. |

---

### Block S: Build Recovery + Module Decoupling (`v0.7.3`) --- COMPLETE ✅

**Duration**: 1-2 weeks | **Model**: Claude Opus + Sonnet | **Depends on**: Block N + Block Q | **Status**: COMPLETE (2026-03-28) — S13 manual soak validated; automated gates green. Handoff closed to Block T.

Goal: Restore hard-green build baseline, then enforce strict module decoupling semantics (module disabled = capability disabled).

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| S1 | Build compile fix (Windows/TTS path) | High | N | DONE | Fixed Windows TTS compile/runtime mismatch in `multimodal_io` and restored green build gate. |
| S2 | Rust lib test fix (language guard) | Medium | S1 | DONE | Fixed deterministic language-guard behavior in `ai_fallback` and restored `cargo test --lib` green. |
| S3 | Hard capability gates | High | S1, S2 | DONE | Centralized module+settings capability gates and enforced them across vision/TTS/workflow-agent command paths. |
| S4 | Disable lifecycle effects | High | S3 | DONE | Module disable now enforces immediate runtime side-effects (`vision` stream stop, `tts` stop) with no lingering active path. |
| S5 | Module UI state consistency | Medium | S3, S4 | DONE | UI consoles now reflect effective capability state (module + setting), avoiding pseudo-active behavior when modules are disabled. |
| S6 | AI Refinement als optionales Modul (`ai_refinement`) | High | S3-S5 | DONE | Added module manifest + lifecycle wiring (`enable` => `ai_fallback.enabled=true`, `disable` => hard-off) with migration-safe binding to legacy settings. |
| S7 | AI Refinement capability hard-gates + disable side-effects | High | S6 | DONE | Added runtime capability gate (`module + setting`) for auto-refinement/manual refine paths; disable now resets refinement activity and stops managed local runtime/daemon paths. |
| S8 | Frontend tab gating + effective refinement state | Medium | S6, S7 | DONE | AI Refinement tab is hidden unless module active; active-tab fallback to `transcription`; effective refinement checks now require `module_enabled && ai_fallback.enabled`; onboarding Step 4 now toggles module state. |
| S9 | Regression + docs + handoff to TTS free-config/testing | Medium | S6-S8 | DONE | Automated gates confirmed green (`cargo test --lib`, `npm test`, `npm run build`); roadmap/status/schedule synchronized; next execution focus set to `TTS freikonfigurierbar + testbar`. |
| S10 | Strict module-UX decoupling + own TTS main tab (`voice-output`) | High | S9 | DONE | `output_voice_tts` moved to dedicated main-tab with hard module-gating, localStorage fallback, active-tab fallback, and Configure routing from Modules Hub into the tab. |
| S11 | AI-Refinement re-enable speed path (`autostart + warmup + no false defer`) | High | S10 | DONE | Re-enable now autostarts managed Ollama in `local_primary`, performs warmup, and defer policy only activates when runtime is truly ready; runtime-not-ready emits stable refinement-failed reason. |
| S12 | Overlay deep refactor (supervisor/recovery/pulse/off-screen) | High | S10 | DONE | Replaced permanent create-fail lockout with bounded retry/cooldown supervisor, added explicit `recovered` health signal, heartbeat sync channel, off-screen fallback anchor, and deterministic replay hardening. |
| S13 | Regression + soak gate for S10-S12 (`50 cycles + 10 restarts`) | Medium | S10-S12 | DONE | Automated gates confirmed green (2026-03-24): `cargo test --lib` 169/169, `npm test` 211/211, `npm run build` OK. Manual soak (`50 cycles + 10 restarts`) validated without overlay/pulse regressions (2026-03-28). |

#### Block S Acceptance Criteria (`S10-S13`)

- `S10` accepted when TTS configuration is isolated to `voice-output` main-tab, the tab is shown only when module-enabled, and Configure action routes directly there.
- `S11` accepted when Ollama `local_primary` defer is runtime-ready only, non-ready path emits deterministic `transcription:refinement-failed` reason code, and re-enable path autostarts + warms runtime.
- `S12` accepted when overlay creation no longer permanently locks out after first failure, recovery emits `recovering/failed/recovered`, and refinement pulse/off-screen recovery are deterministic.
- `S13` accepted when automated gates (`cargo test --lib`, `npm test`, `npm run build`) stay green and manual soak gate (`50 cycles + 10 restarts`) passes without visibility/pulse regressions.

---

### Block S13.5: TTS Free-Config Verification (`v0.7.3`) --- COMPLETE ✅

**Duration**: 1-2 days | **Model**: Haiku + Sonnet | **Depends on**: S13 | **Status**: DONE (2026-03-28)

Goal: Validate TTS provider matrix, device routing, and hard-fail diagnostics before Assistant Pivot.

| Task | Name | Complexity | Status | Description |
| --- | --- | --- | --- | --- |
| S13.5.A | Provider matrix test | Medium | DONE | Verified deterministic fallback matrix and explicit reason-code paths (`tts_fallback_*`, `tts_audio_device_unavailable`) across provider-chain logic and test lane. |
| S13.5.B | Device-routing test | Medium | DONE | Verified explicit output-device routing and hard-fail semantics for invalid targets (`tts_output_device_unavailable`) without OS default mutation. |
| S13.5.C | Forced test-path verification | Low | DONE | `test_tts_provider` emits deterministic `tts:speech-finished`/`tts:speech-error` payloads with `manual_test` context and clear diagnostics text. |

**Pass Criteria**: Deterministic provider behavior; clear error text for failures; configurable routing works. ✅

---

### Block T: Assistant Pivot Foundation (`v0.8.0`) --- PLANNED

**Duration**: 2-4 weeks | **Model**: Claude Opus + Sonnet | **Depends on**: Block S + Block M | **Status**: Planned

Goal: Introduce explicit product-mode split and assistant orchestration baseline without regressing transcription-first flow.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| T1 | Product mode types/settings (`transcribe` vs `assistant`) | High | S | DONE | Added persistent `product_mode` schema (`transcribe`/`assistant`) in Rust + TypeScript settings with migration-safe normalization defaults for legacy configs. |
| T2 | Backend assistant orchestrator state | High | T1 | DONE | Added deterministic assistant orchestrator states (`idle/listening/parsing/planning/awaiting_confirm/executing/recovering`) with transition tracking + reconciliation hooks on settings/module lifecycle paths. |
| T3 | Frontend mode switch UX | Medium | T1 | DONE | Added explicit `product_mode` selector (`transcribe` vs `assistant`) in Capture settings and mode-aware workflow-agent console behavior. |
| T4 | Graceful degradation policy | High | T2, T3 | DONE | Assistant now degrades soft for missing TTS/Vision and hard-blocks only when product mode or workflow-agent capability is unavailable; wakeword handling is assistant-mode gated. |
| T5 | Assistant state events | Medium | T2, T3 | DONE | Added `assistant:state-changed`, `assistant:plan-ready`, and `assistant:action-result` events with capability snapshot payloads; frontend listener/log sync wired. |

---

### Block V: GDD Copilot Loop (`v0.8.x`) --- PLANNED

**Duration**: 2-3 weeks | **Model**: Claude Sonnet + Opus | **Depends on**: Block T + Block L/M/N | **Status**: Planned

Goal: Evolve workflow-agent from wakeword command trigger to a transparent GDD copilot loop (`conversation -> clustered session -> archive context -> suggestions -> draft`) with strict plan/execute separation.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| V1 | Session intelligence for copilot | High | T2, M4 | DONE | Added adaptive session clustering for slight gap-overruns when conversation continuity is high, plus enriched scoring signals (`continuity`, `archive_context`) for mixed-source/archive-aware candidate ranking. |
| V2 | Suggestion engine (transparent reasoning) | High | V1, T5 | PLANNED | Generate explicit suggestions with recognized signals, assumptions, and next-step proposals. |
| V3 | Copilot plan model | High | V2, M5 | PLANNED | Extend plan shape to separate `analysis/suggestion` from `side-effectful` actions. |
| V4 | Draft-only execution lane | Medium | V3, L11 | PLANNED | Ensure draft generation is side-effect free; no publish without confirmation path. |
| V5 | Review UX for copilot suggestions | Medium | V2, T3 | PLANNED | Add review-friendly presentation before plan execution/publish decisions. |
| V6 | E2E Copilot flow tests | High | V1-V5 | PLANNED | Validate `conversation -> suggestions -> draft -> optional publish` reproducibility. |

---

### Block U: Assistant UX + Soak + Release Gate (`v0.8.x`) --- PLANNED

**Duration**: 2-3 weeks | **Model**: Claude Opus + Sonnet | **Depends on**: Block T + Block V | **Status**: Planned

Goal: Stabilize assistant UX and enforce long-run release gates before assistant-focused rollout.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| U1 | Assistant UX hardening | High | T | PLANNED | Tighten mode-specific UX copy, feedback, and error surfaces for daily usage. |
| U2 | Soak: 8h stability run | High | U1 | PLANNED | Validate no manual restart requirement under continuous tray/assistant operation. |
| U3 | Soak: 24h release soak | High | U2 | PLANNED | Validate bounded-recovery behavior and no restart loops over a full-day run. |
| U4 | Release gate + evidence packet | High | U2, U3 | PLANNED | Gate release on soak evidence + smoke pass + benchmark-linked diagnostics. |

---

### Block O: Voice Confirmation Loop --- POST-T/U BACKLOG

**Duration**: 3-4 weeks | **Model**: Claude Sonnet | **Depends on**: Block T + Block V + Block U | **Status**: Planned (deferred)

Goal: Enable a voice-driven confirmation dialog — Agent speaks a question, user responds with "bestätigen"/"abbrechen" via activation word, Agent executes or cancels.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| O1 | `awaiting_confirmation` State im Workflow-Agent | High | T2 | PLANNED | New backend state machine entry; pending action stored with TTL and unique token. |
| O2 | Activation-Word-Matching für confirm/cancel | Medium | O1, T5 | PLANNED | Recognize "bestätigen" / "abbrechen" (+ EN synonyms) as confirmation tokens in `transcription:raw-result` handler. |
| O3 | `confirm_pending_action` / `cancel_pending_action` Commands | High | O1 | PLANNED | Tauri commands to resolve pending action; emit `agent:confirmed` / `agent:cancelled` events. |
| O4 | TTS Confirmation Prompt + Timeout | Medium | O1, T4 | PLANNED | Agent speaks confirmation request via TTS; auto-cancels pending action after configurable timeout. |
| O5 | KITT-Overlay: "Awaiting confirmation" Visual | Low | O4, T3 | PLANNED | Overlay shows distinct "waiting" state while confirmation is pending. |
| O6 | Integration Tests für Confirmation Loop | High | O1–O5 | PLANNED | Unit + integration tests for state machine transitions, token matching, and timeout behavior. |

---

### Block P: Hands-Free Screen Interaction --- POST-T/U BACKLOG

**Duration**: 4-5 weeks | **Model**: Claude Opus | **Depends on**: Block O (O3) + Block T/V/U stabilization | **Status**: Planned (deferred)

Goal: Agent detects the active window, injects text into focused input fields via `enigo` (already in Cargo.toml), and confirms via TTS — fully keyboard-free workflow.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| P1 | `enigo`-Command-Surface: `type_text`, `key_combo` | High | T2 | PLANNED | Expose `enigo::Enigo` as Tauri commands `inject_text` and `send_key_combo`. |
| P2 | Active Window Detection (WinAPI) | Medium | P1 | PLANNED | Detect foreground window title and class via WinAPI or `tauri-plugin-os`; return to agent as context. |
| P3 | Agent-Step-Type: `inject_text` in Execution Plan | High | P1, T2 | PLANNED | New step variant in `AgentExecutionPlan`; runner delegates to `inject_text` command. |
| P4 | Window-Switch + Focus: `focus_window_by_title` | Medium | P2, T3 | PLANNED | Raise and focus a window by title match before text injection. |
| P5 | E2E Test: Voice → Screen-Insert | High | P1–P4 | PLANNED | Validate full path: voice command → agent plan → window focus → text inject → TTS confirmation. |

---

## Key Scheduling Principles

1. **Offline First**: Local refinement path is shipped before online provider integrations.
2. **Dependency Sequencing**: Tasks with dependencies are scheduled in blocks that can reference prior blocks.
3. **Complexity Distribution**: Backend provider work lands before UX and E2E stabilization.
4. **Team Efficiency**: Planning/architecture first, then contained execution packets with low drift.
5. **Risk Mitigation**: End-to-end tests are scheduled at block end to catch integration failures early.

---

## How to Use This Schedule

1. **Per Block**: Assign block to the designated model
2. **Model Context**: Load the block description + task list + relevant code files
3. **Task Execution**: Work through tasks in order, maintaining model focus
4. **Transition**: When block completes, switch to next block (and next model if different)
5. **Checkpoints**: Run integration tests at block boundaries to verify correctness

---

## Notes for Developers

- **Task Dependencies**: Explicitly track in task table. Block order respects critical path
- **Estimation**: Task complexity (Low/Medium/High) guides effort expectations
- **Flexibility**: If a task reveals unexpected complexity, add sub-tasks or escalate to higher model
- **Documentation**: Each block's final task should include documentation updates
- **Testing**: Integration tests should validate cumulative task outputs, not just individual task correctness
- **Context Loading**: Read ARCHITECTURE.md, DECISIONS.md, and V0.5.0_PLAN.md before starting any block

---

## Version Release Criteria

Each version is considered **release-ready** when:

- All tasks in final block completed
- End-to-end integration test passes
- Code reviewed and merged to main
- Changelog updated
- Release notes drafted

See [ROADMAP.md](../ROADMAP.md) for full project timeline.
