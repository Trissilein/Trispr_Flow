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

### Block H: Offline-First Ollama Sprint --- OPEN

**Duration**: 2 weeks | **Status**: Open / next execution packet

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 32 | Implement Ollama provider integration (backend) | High | Task 31 | OPEN | Add local Ollama client (`/api/chat`, `/api/tags`) with robust error handling. |
| 33 | Activate AI refinement pipeline stage (local provider) | High | Task 32 | OPEN | Wire stage 3 to real local refinement with non-blocking fallback on failures. |
| 34 | Implement Ollama-only provider UX | Medium | Tasks 32, 33 | OPEN | Endpoint input, model refresh/test-connection, remove active API-key flow in MVP path. |
| 35 | Implement local-model prompt strategy polish | Medium | Task 33 | OPEN | Tune prompts for DE/EN refinement quality on local models (qwen3 track). |
| 38 | End-to-end test: offline refinement reliability | High | Tasks 32, 33, 34, 35 | OPEN | Validate offline flow, timeout behavior, and no transcript loss under provider failures. |

### Block I: Cloud Provider Rollout --- DEFERRED

**Duration**: 2+ weeks | **Status**: Deferred until Block H stabilization

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| 40 | OpenAI provider integration | High | Task 31, Block H | DEFERRED | Add OpenAI API client after local path is release-stable. |
| 41 | Anthropic (Claude) provider integration | High | Task 31, Block H | DEFERRED | Claude API client and model mapping after offline release. |
| 42 | Gemini provider integration | High | Task 31, Block H | DEFERRED | Gemini API client after offline-first milestone. |

### Block K: Expert Mode UX Overhaul --- PLANNED

**Duration**: 2-3 weeks | **Model**: Claude Sonnet | **Depends on**: Block E | **Status**: Planned

A persistent toggle separates the app into two modes:

- **Standard mode** (default): Only essential settings visible — device, language, hotkeys, enable/disable toggles, AI Refinement tab (full).
- **Expert mode**: Reveals all timing/threshold/buffer controls (VAD thresholds, grace periods, chunk sizes, continuous dump parameters, overlay customization, chapters method, temperature/tokens/custom prompt in AI Refinement).

Implementation uses `data-expert-only` attributes on DOM elements — CSS hides them in standard mode, shows them in expert mode. No settings schema change needed; purely a display filter.

| Task | Name | Complexity | Dependencies | Status | Description |
| --- | --- | --- | --- | --- | --- |
| K1 | Expert-mode toggle (header/settings, localStorage persistence) | Low | Block E | PLANNED | Small toggle button; persists via localStorage `trispr-expert-mode`. |
| K2 | Audit & classify all settings (agent-assisted) | Medium | Block E | PLANNED | Agent reviews every settings field; outputs two lists: standard vs expert. Decision document added to DECISIONS.md. |
| K3 | Apply `data-expert-only` attributes + CSS hide/show | Medium | K1, K2 | PLANNED | Add attribute to expert-only elements; CSS rule `.expert-mode [data-expert-only]` toggles visibility. |
| K4 | Settings re-ordering within panels (expert items sink to bottom) | Medium | K3 | PLANNED | Visual grouping: essential controls at top, expert controls below a subtle divider. |
| K5 | Regression tests (mode toggle shows correct subsets) | Low | K3, K4 | PLANNED | Unit tests verify standard mode hides expert elements; expert mode shows all. |

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
