# Refactoring Plan — Trispr Flow Quality Foundation

Date: 2026-05-15  
Status: **decided** — plan approved, execution pending OQ resolution  
Participants: Hendr (architect), automated challenger review

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

Zero `#[cfg(test)]` blocks found across all Rust source files.

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

### Confirmed circular dependency

`settings.ts` imports `renderVocabulary`, `renderLearnedVocabChips` from `event-listeners.ts`.  
`event-listeners.ts` imports `persistSettings`, `renderSettings`, and 12+ other symbols from `settings.ts`.  
`vocab-auto-learn.ts` already works around this via dynamic import (`void import("./event-listeners").then(...)`).

### `addVocabRow` constraint (confirmed by challenger)

`addVocabRow` in `event-listeners.ts` calls `persistSettings()`. A naïve extraction of `renderVocabulary` into `vocabulary-ui.ts` recreates the circular dependency as `settings.ts → vocabulary-ui.ts → settings.ts`. This is unresolved. See OQ-1.

### `lib.rs` structure (confirmed by challenger)

`lib.rs` already declares 20+ `mod` statements. The domain modules exist. The problem is that command implementations live in `lib.rs` rather than being delegated to those modules. The correct R1 task is to move implementations, not to create modules from scratch.

---

## Decided plan

### Phase 0 — Safety net (parallel, always-on, no merge risk with Ingo's work)

| ID  | What                                                                    | Where                         | Effort         |
| --- | ----------------------------------------------------------------------- | ----------------------------- | -------------- |
| T0a | Tests for `normalizeAssistantSettings()`, `normalizeEnabledModuleIds()` | new `state.test.ts` or inline | trivial        |
| T0b | Rust tests for pure text functions in `postprocessing.rs`               | inline `#[cfg(test)]`         | trivial–simple |
| T0c | Rust tests for prompt template logic in `ai_fallback/provider.rs`       | inline `#[cfg(test)]`         | simple         |

Rationale: tests before structural moves create the safety net. Phase 0 has zero dependencies on Phase 1 or 2.

### Phase 1 — Isolated extractions (sequential, after Phase 0, each item independently committable)

| ID  | What                         | Where                                     | Effort       | Depends on    |
| --- | ---------------------------- | ----------------------------------------- | ------------ | ------------- |
| QW4 | Extract weather logic        | `lib.rs` ~L5987–6340 → `weather.rs`       | trivial      | —             |
| QW3 | Extract TTS benchmark        | `lib.rs` ~L2200–3900 → `tts_benchmark.rs` | **moderate** | QW4 (pattern) |
| QW5 | Extract `MODEL_DESCRIPTIONS` | `state.ts` → `model-descriptions.ts`      | trivial      | —             |
| QW2 | Break circular dep           | depends on OQ-1 resolution                | moderate     | OQ-1          |

QW2 is blocked pending OQ-1. QW4 and QW5 have no blockers and can start immediately after Phase 0.

**Note on QW3:** Challenger confirmed this is moderate, not simple. The block contains embedded `#[cfg(test)]` code and depends on `AppState`/`AppHandle`. Proof-of-pattern from QW4 first.

### Phase 2 — Structural refactoring (after Phase 1; each item requires an ADR before execution)

| ID  | What                                                             | Depends on               | Status                      |
| --- | ---------------------------------------------------------------- | ------------------------ | --------------------------- |
| R1  | Move command implementations out of `lib.rs` into domain modules | OQ-2, QW3+QW4 as pattern | blocked on OQ-2             |
| R2  | Split `event-listeners.ts` by domain                             | QW2 resolved, T0a as net | blocked on OQ-1             |
| R3  | ~~Separate state management tier~~                               | —                        | **cancelled as standalone** |

R3 decision: State modernization is folded into R2. Each domain slice extracted in R2 gets explicit accessor functions instead of direct module-level variable access. No big-bang pub-sub refactor. This is a one-way door of medium reversibility — see Trade-offs.

---

## Trade-offs and lock-in

| Decision                                                      | What is given up                       | Lock-in risk                                                                        |
| ------------------------------------------------------------- | -------------------------------------- | ----------------------------------------------------------------------------------- |
| Command impls move to domain modules (R1)                     | All commands visible in one file       | Low — standard Tauri pattern                                                        |
| Per-slice accessors instead of pub-sub (R3 → R2)              | Reactive updates, subscription pattern | **Medium** — later adoption of signals requires touching all extracted slices again |
| Callback injection for `persistSettings` (if chosen for OQ-1) | Simplicity of direct import            | Low — established pattern                                                           |

The per-slice accessor approach is a one-way door with medium reversal cost. Acceptable given two-person team and active delivery constraint.

---

## Open questions (block specific items if unresolved)

### OQ-1 — How is `persistSettings` decoupled to fix the circular dependency?

**RESOLVED 2026-05-15**

Decision: Move `renderVocabulary`, `renderLearnedVocabChips`, `addVocabRow`, and their five private helpers (`renderLearnedVocabChipsInternal`, `renderObservingCandidateChips`, `updateVocabCountBadge`, `buildLearnedChip`, `buildPendingSubstitutionChip`) from `event-listeners.ts` into `settings.ts`.

Rationale: These are settings-panel render functions that were misplaced in `event-listeners.ts`. Moving them to `settings.ts` removes the only import `settings.ts` has from `event-listeners.ts`, breaking the cycle. No new files, no callback injection. `event-listeners.ts` imports them from `settings.ts` instead (already imports 14+ symbols from there). The ~150-line growth of `settings.ts` is acceptable; a full architecture pass via `/improve-codebase-architecture` will split files by domain later.

The two options previously on the table (settings-core.ts extraction, callback injection) were more complex than necessary because they didn't identify that the vocabulary render functions had simply landed in the wrong file.

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

1. **Signature.** Each module exports a single `export function wire<Domain>(): void`. No return value, no cleanup. Listeners persist for app lifetime. (DOM listeners, not Tauri events; `CLAUDE.md`'s `unlisten` rule applies to `tauri::event::listen`, not `addEventListener`.)
2. **Location and imports.** Wire modules live in `src/wiring/<domain>.wire.ts` — a dedicated directory that groups the sibling files and keeps `src/` root readable. Each file is a *role-level peer* of `event-listeners.ts` (co-equal, not subordinate) and imports `dom` from `../dom-refs` and helper functions from their existing modules (`../history`, `../history-preferences`, `../archive-browser`, etc.) directly. No injection, no helper bag. Snippets shared across multiple wire modules (e.g. `onChangePersist`) live in `src/wiring/wire-helpers.ts`; wire modules and `event-listeners.ts` both import from there. The flow stays one-way: `event-listeners.ts → wiring/*.wire.ts → wiring/wire-helpers.ts`. Wire modules never import from `event-listeners.ts`.
3. **Local closures.** Inline closures defined in the current `wireEvents()` body (e.g. `commitAlias` in the history cluster) are lifted to module-scope `function` declarations inside the wire module. Not exported. Kept private to the slice.
4. **Smoke tests.** Each slice ships ~25 integration-style tests at `src/__tests__/<domain>-wire.test.ts`. Tests build the DOM via `vi.hoisted` fixtures, call `wire<Domain>()`, dispatch a DOM event, and assert observable state without mocking the helper modules. Mocks limited to Tauri `invoke` (already globally mocked via `src/__tests__/tauri.setup.ts`) and dispatch-target modules that open modals or jump to external surfaces (e.g. `archive-browser`, `export-dialog`).
5. **`wireEvents()` shrinks to an orchestrator.** After all slices are extracted, `wireEvents()` consists only of bootstrap calls plus the `wire*()` call sequence — no residual listener junk drawer. The file reads like a table of contents.
6. **One commit per slice.** Each commit lands one wire module, its smoke-test file, and the matching deletion(s) in `event-listeners.ts`. Reviewer subagent gate before each commit. Listeners are moved byte-equivalent where possible; logic is preserved, not improved, during a slice. Reviewer-flagged cleanups (e.g. inline-`change`-handler normalisation to `onChangePersist`) are scoped to the slice that touches them.
7. **Slice taxonomy is domain-capability, not UI-panel.** Wire modules are named after CONTEXT.md terms (or their user-facing equivalents), not after the settings-panel sections that happen to host their controls today. A listener belongs to the wire module whose CONTEXT.md term it mutates. The settings panel can be redesigned freely without reshuffling wire modules.

   The slices, in this taxonomy:

   | Slice | Wire module                    | CONTEXT.md term(s) covered                                                          | Status                |
   | ----- | ------------------------------ | ----------------------------------------------------------------------------------- | --------------------- |
   | 1     | `src/wiring/history.wire.ts`   | Partitioned History                                                                 | shipped (`5b91f6f`)   |
   | 2     | `src/wiring/overlay.wire.ts`   | Overlay (settings panel for the always-on-top WebView)                              | shipped (`b1c61c1`)   |
   | 3     | `src/wiring/voice-output.wire.ts` | Output Voice TTS (UI tab "Voice Output", DOM prefix `voiceOutput*`)              | shipped (`87c2157`)   |
   | 4     | `src/wiring/transcription.wire.ts` | Transcription + Whisper Backend + PTT + VAD + Continuous Dump + Quality/Encoding | shipped — non-contiguous extraction |
   | 5     | `src/wiring/ai-refinement.wire.ts` | AI Refinement + Post-Processing + topic-keywords + provider-chain + prompt-editor + local-runtime | pending — large, non-contiguous (provider setup ~L200–500, post-processing ~L1686, AI fallback ~L1726, local runtime ~L1914–2060, topic-keywords ~L2719). Naming kept as `ai-refinement.wire.ts` to match CONTEXT.md canonical user-facing term; Post-Processing UI is folded in because it is mechanically inseparable from the refinement panel and the file header documents the inclusion. |
   | 6     | `src/wiring/app-chrome.wire.ts` | Global app appearance + navigation (accent color, main-tab switching, modules-hub / recordings / analyse buttons) — items that are *not* a domain capability | pending — added under Option A to eliminate the `event-listeners.ts` junk drawer. |

   Listeners that don't fit any of the six are a signal that either CONTEXT.md is missing a term or the listener is bootstrap (e.g. one-shot default initialisation). They are surfaced during the relevant slice's reviewer pass; CONTEXT.md is updated rather than silently extending a wire module.

---

## Constraints (binding)

- Active block (Block U, v0.8.x) remaining work is soak tests (U2/U3) and a release-gate doc (U4). No code changes to `event-listeners.ts`, `settings.ts`, or `lib.rs` are in progress. Merge conflict risk for Phase 1/2 refactoring is currently zero. (Updated 2026-05-15 — previous constraint named Block B, which is complete.)
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
