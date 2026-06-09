# Refactoring Plan — Trispr Flow Quality Foundation

Date: 2026-05-15  
Last verified: 2026-06-09
Status: **closed (2026-06-09) · Phase 0 complete · Phase 1 complete · R1 complete · R2 complete · R3 cancelled/folded into R2**
Participants: Hendr (architect), automated challenger review

Follow-on architectural work (Trispr Flow modularization) is tracked separately in [`../2026-05-25/trispr-flow-modularization.md`](../2026-05-25/trispr-flow-modularization.md).

Closure note (2026-06-09): This plan is closed as an implementation plan. Its quality-foundation goal was to reduce the highest-risk architecture hotspots before new feature work: backend command implementations were moved to domain modules where the boundary was clean, frontend event wiring was split into domain wire modules, and the standalone state-tier refactor was intentionally cancelled in favour of per-slice accessors. Remaining cleanup items are residual architecture backlog, not blockers for closing this plan. OQ-4 was resolved after closure by PR #12 / `c496baf refactor(settings): split settings render slices`.

Residual backlog after closure:

- Consider a future settings/core/startup split for commands intentionally left in `lib.rs`.
- Rename `workflow_agent.rs` to `assistant_core.rs` only if the naming mismatch starts causing navigation or domain-language confusion.
- Continue GDD/module source decoupling under the module-installability decision, not under this refactoring plan.

---

## Claim

Refactoring proceeds in three phases: safety-net tests first, then isolated extractions, then structural splits. No big-bang moves. Each item is independently committable.

---

## Context (facts from code scan)

### Critical files by line count

**Rust (`src-tauri/src/`):**

| File                      |  Lines | Problem                                                                                  |
| ------------------------- | -----: | ---------------------------------------------------------------------------------------- |
| `lib.rs`                  | 11,642 | 108 `#[tauri::command]` functions inline; business logic not delegated to domain modules |
| `multimodal_io.rs`        |  3,208 | —                                                                                        |
| `transcription.rs`        |  2,620 | —                                                                                        |
| `audio.rs`                |  2,388 | —                                                                                        |
| `state.rs`                |  2,098 | —                                                                                        |
| `ollama_runtime.rs`       |  2,046 | —                                                                                        |
| `ai_fallback/provider.rs` |  1,994 | —                                                                                        |
| `models.rs`               |  1,446 | —                                                                                        |
| `workflow_agent.rs`       |  1,290 | —                                                                                        |

~~Zero `#[cfg(test)]` blocks found across all Rust source files.~~ **Superseded (2026-05-23):** `#[cfg(test)] mod tests` blocks now exist in `postprocessing.rs` (line 402) and `ai_fallback/provider.rs` (line 1873) — T0b and T0c are done.

**TypeScript (`src/`):**

| File                        | Lines | Problem                                                                  |
| --------------------------- | ----: | ------------------------------------------------------------------------ |
| `event-listeners.ts`        | 3,303 | God module, 21 imports, zero tests, bidirectional dep with `settings.ts` |
| `settings.ts`               | 2,463 | Zero tests, bidirectional dep with `event-listeners.ts`                  |
| `ollama-models.ts`          | 1,640 | —                                                                        |
| `main.ts`                   | 1,501 | —                                                                        |
| `workflow-agent-console.ts` | 1,450 | —                                                                        |
| `types.ts`                  | 1,035 | —                                                                        |

Test suite: 21 test files, 269 tests pass (2026-05-15). The two largest files (`event-listeners.ts`, `settings.ts`) have zero coverage.

**Superseded (2026-05-23):** `event-listeners.ts` is now a ~30-line orchestrator (R2 complete). `settings.ts` is gone — decomposed into `src/settings/index.ts` + 5 domain slices (settings decomposition ADR 2026-05-19, complete). Test suite is 626 tests / 33 files (was 269/21).

### Confirmed circular dependency

`settings.ts` imports `renderVocabulary`, `renderLearnedVocabChips` from `event-listeners.ts`.  
`event-listeners.ts` imports `persistSettings`, `renderSettings`, and 12+ other symbols from `settings.ts`.  
`vocab-auto-learn.ts` already works around this via dynamic import (`void import("./event-listeners").then(...)`).

**Superseded (2026-05-23):** The cycle is broken. `settings/vocabulary.settings.ts` now owns `renderVocabulary`/`renderLearnedVocabChips` — the `settings → event-listeners` direction is gone. `event-listeners.ts` retains one import from `./settings` (`ensureContinuousDumpDefaults`); no `src/settings/**` module imports from `event-listeners`. The original bidirectional cycle is now unidirectional. OQ-1 confirmed closed (2026-05-23): the cycle is broken. Implementation path differed from the 2026-05-15 in-place move — vocabulary functions landed in `settings/vocabulary.settings.ts` via ADR 2026-05-19 (settings decomposition) rather than `settings.ts` directly. Outcome is identical.

### `addVocabRow` constraint (confirmed by challenger)

`addVocabRow` in `event-listeners.ts` calls `persistSettings()`. A naïve extraction of `renderVocabulary` into `vocabulary-ui.ts` recreates the circular dependency as `settings.ts → vocabulary-ui.ts → settings.ts`. This is unresolved. See OQ-1.

**Superseded (2026-05-23):** Resolved via the settings decomposition (ADR 2026-05-19). `addVocabRow` now lives in `settings/vocabulary.settings.ts` alongside `renderVocabulary`/`renderLearnedVocabChips`. The constraint no longer applies.

### `lib.rs` structure (confirmed by challenger)

`lib.rs` already declares 20+ `mod` statements. The domain modules exist. The problem is that command implementations live in `lib.rs` rather than being delegated to those modules. The correct R1 task is to move implementations, not to create modules from scratch.

---

## Decided plan

### Phase 0 — Safety net (parallel, always-on, no merge risk with Ingo's work)

| ID  | What                                                                    | Where                         | Effort         | Status (2026-05-23)                                             |
| --- | ----------------------------------------------------------------------- | ----------------------------- | -------------- | --------------------------------------------------------------- |
| T0a | Tests for `normalizeAssistantSettings()`, `normalizeEnabledModuleIds()` | new `state.test.ts` or inline | trivial        | **done** — `src/__tests__/state.normalizers.test.ts` (24 tests) |
| T0b | Rust tests for pure text functions in `postprocessing.rs`               | inline `#[cfg(test)]`         | trivial–simple | **done** — `#[cfg(test)] mod tests` at line 402                 |
| T0c | Rust tests for prompt template logic in `ai_fallback/provider.rs`       | inline `#[cfg(test)]`         | simple         | **done** — `#[cfg(test)] mod tests` at line 1873                |

Rationale: tests before structural moves create the safety net. Phase 0 has zero dependencies on Phase 1 or 2.

### Phase 1 — Isolated extractions (sequential, after Phase 0, each item independently committable)

| ID   | What                                                                               | Where                                                          | Effort       | Depends on    | Status (2026-05-23)                                                                                       |
| ---- | ---------------------------------------------------------------------------------- | -------------------------------------------------------------- | ------------ | ------------- | --------------------------------------------------------------------------------------------------------- |
| QW4  | Extract weather logic                                                              | `lib.rs` ~L5987–6340 → `weather.rs`                            | trivial      | —             | **done** — `src-tauri/src/weather.rs` exists                                                              |
| QW3  | Extract TTS benchmark                                                              | `lib.rs` ~L2084–3997 + ~L10445 (two cuts) → `tts_benchmark.rs` | **moderate** | QW4 (pattern) | **done** — `src-tauri/src/tts_benchmark.rs` extracted; startup path rewired                               |
| QW3b | Rename `Qwen3TtsBenchmarkConfig` → `Qwen3TtsConfig`; migrate to `multimodal_io.rs` | `lib.rs` + `tts_benchmark.rs` → `multimodal_io.rs`             | trivial      | QW3           | **done** — config + runtime/request/speak helpers moved; call sites rewired                               |
| QW5  | Extract `MODEL_DESCRIPTIONS`                                                       | `state.ts` → `model-descriptions.ts`                           | trivial      | —             | **done** — `src/model-descriptions.ts` exists                                                             |
| QW2  | Break circular dep                                                                 | depends on OQ-1 resolution                                     | moderate     | OQ-1          | **resolved** — cycle is now unidirectional only; OQ-1 confirmed closed 2026-05-23 (see Context amendment) |

QW2 depends on OQ-1, which is confirmed closed (2026-05-23). QW3, QW3b, QW4, and QW5 are done.

**Design (2026-05-23) — QW3:** Two-cut extraction — the physical block is not fully contiguous.

- **Cut 1 (~L2084–3997):** benchmark type definitions, constants, and all benchmark functions through `run_tts_benchmark_inner` and `write_tts_benchmark_report`.
- **Cut 2 (~L10445):** `tts_benchmark_request_from_env` — env-driven constructor for `TtsBenchmarkRequest`; physically outside the main block but belongs with the type. Decision (2026-05-23 grill): classified as benchmark domain logic (reads env vars, constructs `TtsBenchmarkRequest` fields), not startup wiring — moves with the type. Two-cut shape approved.

What moves to `tts_benchmark.rs`: all `TtsBenchmark*` structs; all `TTS_FAILURE_*` and `TTS_PROVIDER_SURFACE_*` constants; functions `classify_tts_failure`, `default_tts_benchmark_gates`, `tts_provider_profile`, `is_runtime_stable_provider`, `default_tts_benchmark_scenarios`, `normalize_tts_benchmark_providers`, `resolve_qwen3_tts_benchmark_config`, `benchmark_qwen3_tts_synthesis`, `normalize_tts_benchmark_scenarios`, `run_tts_provider_once`, `run_tts_runtime_smoke_once`, `summarize_tts_provider`, `build_tts_fallback_order`, `scenario_success_counts_for_provider`, `provider_consistency_from_runtime_surface`; `pub(crate) fn run_tts_benchmark_inner`; `pub(crate) fn write_tts_benchmark_report`; `tts_benchmark_request_from_env`; `#[tauri::command] pub(crate) fn run_tts_benchmark`; existing `#[cfg(test)] mod tts_benchmark_tests`.

Anchor functions that stay in `lib.rs`:
- `format_ureq_status_error` (pub(crate)) — called by `task_capture.rs` via `crate::format_ureq_status_error`

**Superseded by QW3b (2026-05-23):** `Qwen3TtsConfig` (renamed from `Qwen3TtsBenchmarkConfig`), `request_qwen3_tts_audio_bytes`, `speak_qwen3_tts`, and `resolve_qwen3_tts_runtime_config` moved from `lib.rs` to `multimodal_io.rs`.

Cross-module access after QW3b: `tts_benchmark.rs` uses `crate::multimodal_io::Qwen3TtsConfig` and calls `crate::multimodal_io::request_qwen3_tts_audio_bytes(...)` for benchmark probes.

`lib.rs` changes: add `mod tts_benchmark;` near existing domain `mod` declarations; delete both cut regions; add `pub(crate) use tts_benchmark::run_tts_benchmark;` (OQ-2 pattern — `generate_handler!` list unchanged by name); update startup block (current `TRISPR_RUN_TTS_BENCHMARK` branch) to call `tts_benchmark::tts_benchmark_request_from_env()`, `tts_benchmark::run_tts_benchmark_inner(...)`, `tts_benchmark::write_tts_benchmark_report(...)`.

**QW3b (done, 2026-05-23):** Renamed `Qwen3TtsBenchmarkConfig` → `Qwen3TtsConfig`; moved the config struct, `request_qwen3_tts_audio_bytes`, `speak_qwen3_tts`, and `resolve_qwen3_tts_runtime_config` from `lib.rs` to `multimodal_io.rs`. `tts_benchmark.rs` now uses `crate::multimodal_io::Qwen3TtsConfig` and no longer uses `use super::` for Qwen3 request/config access.

Decision (2026-05-23 grill): timing is "after QW3" (not part of QW3) to keep each commit bounded to a single concern and independently bisectable. Executed: QW3b landed as the immediate follow-on.

### Phase 2 — Structural refactoring (after Phase 1; each item requires an ADR before execution)

| ID  | What                                                             | Depends on               | Status                                                                                                                                                                                                        |
| --- | ---------------------------------------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1  | Move command implementations out of `lib.rs` into domain modules | OQ-2, QW3+QW4 as pattern | **execution complete (2026-05-25)** — `lib.rs` now has 21 `#[tauri::command]` functions (bootstrap/diagnostics only). Commands distributed: `ai_fallback/commands.rs` (16), `gdd/confluence.rs` (14), `multimodal_io.rs` (12), `models.rs` (9), `ollama_runtime.rs` (9), `history_partition.rs` (9), `workflow_agent.rs` (8), `audio.rs` (7), `gdd/mod.rs` (7), plus smaller. Completion commit: `b397260 refactor(overlay): complete R1 command extraction and B8 deferral`. Design notes below retained for traceability. |
| R2  | Split `event-listeners.ts` by domain                             | QW2 resolved, T0a as net | **complete** — all 6 slices shipped; `event-listeners.ts` is a 30-line orchestrator                                                                                                                           |
| R3  | ~~Separate state management tier~~                               | —                        | **cancelled as standalone**                                                                                                                                                                                   |

R3 decision: State modernization is folded into R2. Each domain slice extracted in R2 gets explicit accessor functions instead of direct module-level variable access. No big-bang pub-sub refactor. This is a one-way door of medium reversibility — see Trade-offs.

**Design (2026-05-24) — R1:** Grill + challenger review (2026-05-24) produced the binding plan below. Approved by Hendr.

#### R1 scope

Hybrid: clean-boundary commands move; commands coupled to cross-cutting orchestrators stay in `lib.rs` and are documented as intentional. "No big-bang moves" rule is preserved. R1 proceeds cluster-by-cluster, one commit per cluster.

#### R1 prerequisites (must land before their dependents)

| Step | Action                                                                                                                                       | Unlocks                                                                                      |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| A0.1 | Add `#[macro_export]` to `macro_rules! guarded_command` in `lib.rs` — one line, zero behaviour change                                        | GDD (A1.10), Confluence (B4), modules (B8), workflow_agent (B1), vision (B2), TTS (B3)       |
| A0.2 | Move `validate_path_within` (path-traversal guard with UNC rejection) from `lib.rs` to `paths.rs` as `pub(crate)` — approved as one-way door | `encode_to_opus` → `opus.rs` (A1.9); `run_latency_benchmark_inner` → `tts_benchmark.rs` (B7) |

Decision A0.1: `#[macro_export]` in `lib.rs` (not move to `util.rs`). Moving the macro costs ~21 callsite updates in `lib.rs` for a marginal organisational gain. Accepted as cost-justified; domain modules call `crate::guarded_command!(...)`. One-way door not triggered — trivially reversible.

Decision A0.2: `validate_path_within` is a security primitive. Correct home is `paths.rs` alongside `resolve_base_dir`. Both `encode_to_opus` and `run_latency_benchmark_inner` import from `crate::paths::validate_path_within` after this move. One-way door — approved 2026-05-24.

#### R1 Cluster A — Pure-delegation moves

A1.1–A1.10 may ship in any order; A0 must land first. A1.9 additionally requires A0.2; A1.10 requires A0.1.

| Commit | Target module          | Commands                                                                                                                                                                                                                                                                                                                                            |
| ------ | ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A1.1   | `hotkeys.rs`           | `validate_hotkey`, `test_hotkey`, `get_hotkey_conflicts`                                                                                                                                                                                                                                                                                            |
| A1.2   | `util.rs`              | `log_frontend_event`, `frontend_heartbeat`                                                                                                                                                                                                                                                                                                          |
| A1.3   | `paths.rs`             | `open_log_directory`                                                                                                                                                                                                                                                                                                                                |
| A1.4   | `audio.rs`             | `get_last_recording_path`, `get_recordings_directory`, `open_recordings_directory`                                                                                                                                                                                                                                                                  |
| A1.5   | `session_manager.rs`   | `save_crash_recovery`, `clear_crash_recovery`                                                                                                                                                                                                                                                                                                       |
| A1.6   | `history_partition.rs` | `save_transcript`                                                                                                                                                                                                                                                                                                                                   |
| A1.7   | `video_ingest.rs`      | `video_ingest_sources`, `video_ingest_history_entry` (delegate to `crate::video_ingest::*` — ingest commands belong in the ingest module, not generation)                                                                                                                                                                                           |
| A1.8   | `video_generation.rs`  | `video_generate`, `video_get_output_dir`, `video_open_output_dir`                                                                                                                                                                                                                                                                                   |
| A1.9   | `opus.rs`              | `check_ffmpeg`, `get_ffmpeg_version_info`, `encode_to_opus`; uses `crate::paths::validate_path_within` (needs A0.2)                                                                                                                                                                                                                                 |
| A1.10  | `gdd/mod.rs`           | `list_gdd_presets`, `save_gdd_preset_clone`, `detect_gdd_preset`, `generate_gdd_draft`, `validate_gdd_draft`, `render_gdd_for_confluence`, `render_gdd_markdown`; needs A0.1. `save_gdd_preset_clone` retains its own inline persistence path — refactoring to `update_and_persist_settings` is out of R1 scope (documented intentional divergence) |

#### R1 Cluster B — Helper-promotion moves

All B commits require A0.1 (macro export). Each row promotes its helpers before moving commands.

| Commit | Promote first                                                                                                                                                                | Commands → Target                                                                                                                                                                                                                                           |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| B1     | `emit_assistant_*` helper family + `ASSISTANT_PENDING_CONFIRMATION` / `ASSISTANT_CONFIRM_TOKEN_SEQ` statics → `workflow_agent.rs`                                            | 8 agent commands → `workflow_agent.rs`                                                                                                                                                                                                                      |
| B2     | `stop_vision_stream_internal` → `multimodal_io.rs`                                                                                                                           | `list_screen_sources`, `start_vision_stream`, `stop_vision_stream`, `get_vision_stream_health`, `capture_vision_snapshot` → `multimodal_io.rs`                                                                                                              |
| B3     | `speak_tts_internal`, `stop_tts_internal` → `multimodal_io.rs`                                                                                                               | `list_tts_providers`, `list_tts_voices`, `list_piper_voice_catalog`, `download_piper_voice_key`, `speak_tts`, `stop_tts`, `test_tts_provider` → `multimodal_io.rs`                                                                                          |
| B4     | (macro only — A0.1)                                                                                                                                                          | 14 Confluence commands → `gdd/confluence.rs` (existing 924-line file; `confluence/mod.rs` is a 1-line stub and is not the target)                                                                                                                           |
| B5     | (state fields only, no new lib.rs helpers)                                                                                                                                   | `get_history`, `get_transcribe_history`, `clear_active_transcript_history`, `delete_active_transcript_entry`, `list_history_partitions`, `load_history_partition`, `add_history_entry`, `add_transcribe_entry` → `history_partition.rs`                     |
| B6     | `prepare_refinement`, `check_strict_local_mode`, `update_and_persist_settings` → `ai_fallback/` — one-way door, approved 2026-05-24                                          | 18 AI fallback commands → `ai_fallback/commands.rs`                                                                                                                                                                                                         |
| B7     | `run_latency_benchmark_inner` + `LatencyBenchmarkRequest`/`LatencyBenchmarkResult` types → `tts_benchmark.rs`; imports `crate::ai_fallback::prepare_refinement` post-B6      | `run_latency_benchmark` → `tts_benchmark.rs` (must follow B6 — moving in Cluster A would create a module→root dependency on `prepare_refinement` that B6 would then force to re-migrate)                                                                    |
| B8     | `schedule_piper_daemon_reconcile`, `schedule_ai_refinement_reenable_bootstrap`, `start_transcribe_monitor`/`stop_transcribe_monitor` — assess entanglement before committing | `list_modules`, `enable_module`, `disable_module`, `get_module_health`, `check_module_updates`, `show_assistant_presence_window` → `modules/commands.rs` — **stretch goal**: if helpers are too entangled, R1 ships without B8 and a follow-up ADR is filed |
| B9     | (no promotion needed)                                                                                                                                                        | `get_task_capture_settings`, `save_task_capture_settings`, `test_task_capture_endpoint` → `modules/task_capture.rs`                                                                                                                                         |

Build result (2026-05-25): A0, A1, and B1-B9 are implemented. B8 shipped as a lifecycle coordinator extraction rather than a full command relocation: `enable_module` and `disable_module` now delegate to `modules/lifecycle_coordinator.rs`, while `list_modules`, `get_module_health`, `check_module_updates`, and `show_assistant_presence_window` remain in `lib.rs` as core registration/orchestration commands. Validation: `npm test` passed 626/626; `cargo test --manifest-path src-tauri/Cargo.toml --lib` passed 221/221; `cargo check --manifest-path src-tauri/Cargo.toml` passed with warnings only under the local Vulkan Tauri config.

Note — `workflow_agent.rs` rename: `ASSISTANT_PENDING_CONFIRMATION` static moves as-is to `workflow_agent.rs`. Renaming the file to `assistant_core.rs` (CONTEXT.md preferred term) is deferred — independent of command extraction and out of R1 scope.

#### R1 intentional stays (~24 commands)

These commands remain in `lib.rs`. They are not abandoned work — they are candidates for a future "settings/core/startup split" pass.

| Command(s)                                                                                          | Reason stays                                                                                                                                                                        |
| --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `get_settings`, `save_settings`, `save_window_state`, `save_window_visibility_state`, `apply_model` | Coupled to `save_settings_inner` + full settings normalisation chain                                                                                                                |
| `get_startup_status`, `get_runtime_diagnostics`                                                     | Coupled to `refresh_startup_status`, `refresh_runtime_diagnostics` — startup infrastructure                                                                                         |
| `get_dependency_preflight_status`                                                                   | `build_dependency_preflight_report` is a ~120-line cross-cutting orchestrator calling `ping_ollama_quick`, `check_powershell_available`, `capability_enabled`, `module_registry::*` |
| `get_runtime_metrics_snapshot`, `record_runtime_metric`                                             | Helpers entangled with AI refinement state machine; separate architecture pass                                                                                                      |
| `toggle_transcribe`, `expand_transcribe_backlog`, `paste_transcript_text`                           | Coupled to `toggle_transcribe_state`, `cancel_backlog_auto_expand`, `expand_transcribe_backlog_inner`, `paste_text` — transcription orchestration                                   |

---

## Trade-offs and lock-in

| Decision                                                                   | What is given up                                   | Lock-in risk                                                                                   |
| -------------------------------------------------------------------------- | -------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| Command impls move to domain modules (R1)                                  | All commands visible in one file                   | Low — standard Tauri pattern                                                                   |
| `guarded_command!` stays in `lib.rs` with `#[macro_export]`                | Macro in a more semantically specific home         | Low — trivially moved to `util.rs` later; no callsite impact on domain modules                 |
| `validate_path_within` → `paths.rs` (A0.2)                                 | Path-security helper no longer in lib.rs           | Low — correct home; no reversal friction                                                       |
| `prepare_refinement` + `update_and_persist_settings` → `ai_fallback/` (B6) | `lib.rs` no longer owns AI settings persistence    | **Medium** — future changes to AI settings persistence must understand `ai_fallback/` boundary |
| `save_gdd_preset_clone` retains inline persistence                         | Diverges from `update_and_persist_settings` helper | Low — documented intentional; consolidation is future cleanup                                  |
| `workflow_agent.rs` not renamed to `assistant_core.rs` in R1               | CONTEXT.md preferred name not reflected in file    | Low — independent rename is a `git mv` + import pass                                           |
| B8 coordinator extraction                                                  | Full module command relocation did not land in R1  | Low — lifecycle side effects are isolated; remaining command wrappers can move later           |
| Per-slice accessors instead of pub-sub (R3 → R2)                           | Reactive updates, subscription pattern             | **Medium** — later adoption of signals requires touching all extracted slices again            |
| Callback injection for `persistSettings` (if chosen for OQ-1)              | Simplicity of direct import                        | Low — established pattern                                                                      |

The per-slice accessor approach is a one-way door with medium reversal cost. Acceptable given two-person team and active delivery constraint.

---

## Open questions (block specific items if unresolved)

### OQ-1 — How is `persistSettings` decoupled to fix the circular dependency?

**RESOLVED 2026-05-15**

Decision: Move `renderVocabulary`, `renderLearnedVocabChips`, `addVocabRow`, and their five private helpers (`renderLearnedVocabChipsInternal`, `renderObservingCandidateChips`, `updateVocabCountBadge`, `buildLearnedChip`, `buildPendingSubstitutionChip`) from `event-listeners.ts` into `settings.ts`.

Rationale: These are settings-panel render functions that were misplaced in `event-listeners.ts`. Moving them to `settings.ts` removes the only import `settings.ts` has from `event-listeners.ts`, breaking the cycle. No new files, no callback injection. `event-listeners.ts` imports them from `settings.ts` instead (already imports 14+ symbols from there). The ~150-line growth of `settings.ts` is acceptable; a full architecture pass via `/improve-codebase-architecture` will split files by domain later.

The two options previously on the table (settings-core.ts extraction, callback injection) were more complex than necessary because they didn't identify that the vocabulary render functions had simply landed in the wrong file.

**Implementation note (2026-05-23):** The vocabulary render functions were moved to `settings/vocabulary.settings.ts` via the settings decomposition (ADR 2026-05-19), not to `settings.ts` directly as this decision specified. The cycle-breaking outcome is identical. OQ-1 confirmed closed.

### OQ-2 — How are Tauri commands registered after R1 moves implementations?

**RESOLVED 2026-05-15**

Decision: Option (b) — `lib.rs` stays the registration hub using the `pub use` pattern.

Concretely:
- `#[tauri::command]` attribute moves **with** the implementation into the domain module
- `lib.rs` re-exports the function name via `pub use domain::command_fn`
- `generate_handler![command_fn, ...]` list stays in `lib.rs`

This makes domain modules self-contained (command + implementation co-located) while keeping `lib.rs` as a pure registration manifest. The 108-name list is acceptable as a manifest — it contains names, not implementations.

Rationale: Option (a) (`Vec<Box<dyn Command>>`) is incompatible with `generate_handler!`, which is a compile-time macro. Option (c) (Tauri plugin per domain) pays plugin ceremony now for a benefit only realised during the full architecture pass. Option (b) with `pub use` is the natural Tauri 2 community pattern, fully reversible, and leaves a clean foundation for per-domain plugin promotion later.

### OQ-3 — What is the wire-module contract for R2?

**RESOLVED 2026-05-15** (slice taxonomy amended 2026-05-18 per Ingo's "ignore current state, we refactor" direction)

The contract is fixed for all R2 slices so the pattern, once set on slice 1, is mechanically applied to the rest.

1. **Signature.** Each module exports a single `export function wire<Domain>(): void`. No return value, no cleanup. Listeners persist for app lifetime. (DOM listeners, not Tauri events; `CLAUDE.md`'s `unlisten` rule applies to `tauri::event::listen`, not `addEventListener`.) Slice 6 has a scoped exception: `app-chrome.wire.ts` also exports runtime navigation helpers used by bootstrap and module-hub code (`initMainTab`, `openMainTab`, `reconcileMainTabVisibility`).
2. **Location and imports.** Wire modules live in `src/wiring/<domain>.wire.ts` — a dedicated directory that groups the sibling files and keeps `src/` root readable. Each file is a *role-level peer* of `event-listeners.ts` (co-equal, not subordinate) and imports `dom` from `../dom-refs` and helper functions from their existing modules (`../history`, `../history-preferences`, `../archive-browser`, etc.) directly. No injection, no helper bag. Snippets shared across multiple wire modules (e.g. `onChangePersist`) live in `src/wiring/wire-helpers.ts`; wire modules and `event-listeners.ts` both import from there. The flow stays one-way: `event-listeners.ts → wiring/*.wire.ts → wiring/wire-helpers.ts`. Wire modules never import from `event-listeners.ts`.
3. **Local closures.** Inline closures defined in the current `wireEvents()` body (e.g. `commitAlias` in the history cluster) are lifted to module-scope `function` declarations inside the wire module. Not exported. Kept private to the slice.
4. **Smoke tests.** Each slice ships ~25 integration-style tests at `src/__tests__/<domain>-wire.test.ts`. Tests build the DOM via `vi.hoisted` fixtures, call `wire<Domain>()`, dispatch a DOM event, and assert observable state without mocking the helper modules. Mocks limited to Tauri `invoke` (already globally mocked via `src/__tests__/tauri.setup.ts`) and dispatch-target modules that open modals or jump to external surfaces (e.g. `archive-browser`, `export-dialog`).
5. **`wireEvents()` shrinks to an orchestrator.** After all slices are extracted, `wireEvents()` consists only of bootstrap calls plus the `wire*()` call sequence — no residual listener junk drawer. The file reads like a table of contents.
6. **One commit per slice.** Each commit lands one wire module, its smoke-test file, and the matching deletion(s) in `event-listeners.ts`. Reviewer subagent gate before each commit. Listeners are moved byte-equivalent where possible; logic is preserved, not improved, during a slice. Reviewer-flagged cleanups (e.g. inline-`change`-handler normalisation to `onChangePersist`) are scoped to the slice that touches them.
7. **Slice taxonomy is domain-capability, not UI-panel.** Wire modules are named after CONTEXT.md terms (or their user-facing equivalents), not after the settings-panel sections that happen to host their controls today. A listener belongs to the wire module whose CONTEXT.md term it mutates. The settings panel can be redesigned freely without reshuffling wire modules.

   The slices, in this taxonomy:

   | Slice | Wire module                        | CONTEXT.md term(s) covered                                                                                                                                   | Status                                                                                                                                                                                                                                                                                                   |
   | ----- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
   | 1     | `src/wiring/history.wire.ts`       | Partitioned History                                                                                                                                          | shipped (`5b91f6f`)                                                                                                                                                                                                                                                                                      |
   | 2     | `src/wiring/overlay.wire.ts`       | Overlay (settings panel for the always-on-top WebView)                                                                                                       | shipped (`b1c61c1`)                                                                                                                                                                                                                                                                                      |
   | 3     | `src/wiring/voice-output.wire.ts`  | Output Voice TTS (UI tab "Voice Output", DOM prefix `voiceOutput*`)                                                                                          | shipped (`87c2157`)                                                                                                                                                                                                                                                                                      |
   | 4     | `src/wiring/transcription.wire.ts` | Transcription + Whisper Backend + PTT + VAD + Continuous Dump + Quality/Encoding                                                                             | shipped — non-contiguous extraction                                                                                                                                                                                                                                                                      |
   | 5     | `src/wiring/ai-refinement.wire.ts` | AI Refinement + Post-Processing + topic-keywords + provider-chain + prompt-editor + local-runtime                                                            | shipped (`6b210f1`, CI fix `b91238d`) — 1766 lines, 28 smoke tests. Naming kept as `ai-refinement.wire.ts` to match CONTEXT.md canonical user-facing term; Post-Processing UI is folded in because it is mechanically inseparable from the refinement panel and the file header documents the inclusion. |
   | 6     | `src/wiring/app-chrome.wire.ts`    | Global app appearance + navigation (accent color, main-tab switching, modules-hub / recordings / analyse buttons) — items that are *not* a domain capability | shipped (`b9ad036`) — 417 lines, 30 smoke tests. Exports `wireAppChrome` plus runtime navigation helpers (`initMainTab`, `openMainTab`, `reconcileMainTabVisibility`); scoped OQ-3 export exception documented in clause 1.                                                                              |

   Listeners that don't fit any of the six are a signal that either CONTEXT.md is missing a term or the listener is bootstrap (e.g. one-shot default initialisation). They are surfaced during the relevant slice's reviewer pass; CONTEXT.md is updated rather than silently extending a wire module.

8. **Slice 5 perimeter (decided 2026-05-18).** The Slice 5 row is large and non-contiguous, so the boundary against Slice 6 was grilled and pinned before extraction:

   | Item                                                                                                                                                                                        | Slice 5 includes?                                                                                                 |
   | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
   | AI Fallback enable/lanes/auth modal/temperature/max-tokens/preserve-language                                                                                                                | yes                                                                                                               |
   | Local runtime controls (version, source, backend, install, refresh, verify, import, fallback endpoints)                                                                                     | yes                                                                                                               |
   | OpenAI-compat backends (LM Studio / Oobabooga endpoint, key, fetch, verify, install)                                                                                                        | yes                                                                                                               |
   | Prompt presets + custom-prompt editor (list, save, reset, revert, discard, delete, input, keydown, change)                                                                                  | yes                                                                                                               |
   | Post-processing toggles + custom-vocab toggle + vocab-add                                                                                                                                   | yes                                                                                                               |
   | Topic-keywords reset                                                                                                                                                                        | yes                                                                                                               |
   | Language controls (`languageSelect`, `languagePinnedToggle`, `whisperInputLanguageSelect`) — deferred by Slice 4 because they mutate AI-Refinement prompt state                             | yes                                                                                                               |
   | `ensureAIFallbackSettingsDefaults` — moves into `ai-refinement.wire.ts` as private; first statement of `wireAiRefinement()`; the explicit call at the top of `wireEvents()` is removed      | yes                                                                                                               |
   | Auth-modal Escape `window.keydown` listener — moves to wire module **without** `_windowCleanups` registration, per OQ-3 clause 1; behaviour drift noted in commit body                      | yes                                                                                                               |
   | Main-tab switching block (`switchMainTab`, `initMainTab`, `reconcileMainTabVisibility`, `openMainTab`, `syncMainTabAvailability`, `getActiveMainTabFromDom`, all `tabBtn*` click listeners) | **no** — Slice 6                                                                                                  |
   | AI-refinement-specific tab helpers (`refreshAiRefinementTabState`, `aiRefinementTabAvailable`, `aiRefinementTabRefreshInFlight`)                                                            | **no** — stay with the tab block in `event-listeners.ts` for Slice 6 (preserves OQ-3 clause 1 single-export rule) |
   | Product Mode toggle (`setProductMode`, `productModeTranscribeBtn`, `productModeAssistantBtn`)                                                                                               | **no** — Slice 6 (Product Mode is its own CONTEXT.md term, not AI Refinement)                                     |
   | Global Online/Offline (`setGlobalOnlineMode`, `globalOnlineOfflineBtn`, `globalOnlineEnabledBtn`)                                                                                           | **no** — Slice 6 (Workflow Agent state)                                                                           |
   | Whisper model source/storage (`modelSourceSelect`, `modelCustomUrl`, `modelRefresh`, `modelStorageBrowse`, `modelStorageReset`, `modelStoragePath`)                                         | **no** — separate Slice 4 backfill commit, scheduled **after** Slice 5 and **before** Slice 6                     |
   | Generic panel-collapse listeners (`.panel-collapse-btn`, `.panel-header`)                                                                                                                   | **no** — Slice 6                                                                                                  |
   | `_windowCleanups` + `cleanupWindowListeners()` export                                                                                                                                       | **no** — stay in `event-listeners.ts` to serve the residual `_onResize` listener                                  |

   Smoke-test scope: ~25 state-machine tests covering behaviours, not one-per-listener (Slice 5 has ~54 listeners; listener-parity testing inflates count without adding signal). Plus 2–4 input-handler edge-case tests where the input handler computes a derived value (`onChangePersist` cases).

   Bootstrap order after Slice 5 in `wireEvents()`:

   ```
   ensureContinuousDumpDefaults();
   if (syncHistoryAliasesIntoSettings()) void persistSettings();
   wireTranscription();
   wireAiRefinement();   // runs ensureAIFallbackSettingsDefaults() internally
   wireHistory();
   wireOverlay();
   wireVoiceOutput();
   // residual: tab switching + AI-refinement tab helpers, product mode, global online,
   // model source (slice-4 backfill target), panel collapse, productModeToggle/ttsStop hotkeys
   ```

### OQ-4 — `settings-persist.ts` → `transcription.settings.ts` coupling risk

**RESOLVED 2026-06-09.**

Resolved by PR #12 / `c496baf refactor(settings): split settings render slices`. Pure ASR/Post-Processing language derivation moved to dependency-free `src/language-utils.ts`, and `settings-persist.ts` now imports `derivePostprocLanguageFromAsr` from that module instead of from `settings/transcription.settings.ts`. `settings/index.ts` remains the settings orchestrator, while Continuous Dump, Recording Quality, and Post-Processing rendering live in dedicated settings slices. The cycle risk is closed.

---

## Constraints (binding)

- This refactoring plan is closed and no longer controls active release work. Current release-gate closure is tracked in `ROADMAP.md` as Block A for v0.8.2.
- Follow-up architecture cleanup from this plan is residual backlog only. New cleanup work should be tracked under the relevant roadmap or modularization decision instead of reopening this plan.
- No framework additions to the TypeScript frontend (no React, Vue, Svelte, or signals library).
- Git operations in native Windows shell only (per CLAUDE.md).
- No retroactive ADRs for items already in `docs/DECISIONS.md`. New decisions go here.

---

## Items explicitly excluded from this plan

| Item                                           | Reason                                                                               |
| ---------------------------------------------- | ------------------------------------------------------------------------------------ |
| `ollama-models.ts` (1,640 lines)               | Large but no active risk signal (no circular dep, no zero-coverage with active bugs) |
| `workflow-agent-console.ts` (1,450 lines)      | Same — observe, no action without trigger                                            |
| `main.ts` (1,501 lines)                        | Bootstrap orchestrator — high fan-out is expected                                    |
| Frontend design and UX changes                 | Ingo's domain                                                                        |
| `docs/DECISIONS.md` retroactive ADR conversion | No value over existing format                                                        |
