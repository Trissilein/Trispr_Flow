# Task Schedule - Trispr Flow

Last updated: 2026-02-19

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
