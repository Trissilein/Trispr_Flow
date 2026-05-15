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

Blocks: QW2, R2.

Options on the table:
- (a) Extract `persistSettings` into `settings-core.ts`, which neither `event-listeners.ts` nor `vocabulary-ui.ts` depends on circularly.
- (b) Pass `persistSettings` as a callback into `addVocabRow` instead of importing it.

No decision recorded. Requires explicit resolution before QW2 begins.

### OQ-2 — How are Tauri commands registered after R1 moves implementations?

Blocks: R1.

Options on the table:
- (a) Each domain module exports `pub fn commands() -> Vec<Box<dyn Command>>`, `lib.rs` merges.
- (b) `lib.rs` stays the registration hub; domain modules expose `pub fn handle_x()` delegates.
- (c) Tauri 2 plugin pattern per domain.

No decision recorded. Requires explicit resolution before R1 begins. Without it, each R1 PR will solve this independently, producing incoherent patterns.

---

## Constraints (binding)

- Ingo (repo owner) is actively shipping Block B (UX/UI). Refactoring must not produce merge conflicts with his work.
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
