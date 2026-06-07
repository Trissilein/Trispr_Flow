# Settings Decomposition Plan — Trispr Flow

Date: 2026-05-19  
Resolved: 2026-05-23  
Status: **complete** — all slices shipped  
Participants: Hendr (architect), automated challenger review (grill-with-docs session)

---

## Context

`settings.ts` is 2671 lines, zero tests. It is the next largest untested file after the R2 decomposition of `event-listeners.ts`. It handles rendering and persisting settings panels for all domains. The R2 pattern (domain-scoped modules with smoke tests) applies here.

The `settings` global object is defined in `state.ts`, not `settings.ts`. This means domain render modules can import `settings` from `../state` without creating a circular dependency through the orchestrator.

---

## Decisions

### 1 — Orchestrator becomes `src/settings/index.ts`

`settings.ts` moves to `src/settings/index.ts`. No existing imports change — Vite's bundler module resolution resolves `from "./settings"` and `from "../settings"` to `src/settings/index.ts` automatically.

Rejected alternative: keeping `settings.ts` at `src/` root alongside a `src/settings/` directory. Rejected because two things named "settings" at different levels is misleading. Moving the orchestrator into the directory it governs is cleaner and consistent with the `wiring/` pattern.

The orchestrator shrinks to: `persistSettings` (imported from `settings-persist.ts`), the two cross-domain render functions, `renderSettings()` (the top-level dispatcher), `renderAIRefinementTab()`, and `ensureContinuousDumpDefaults()`. Everything else moves to a domain module.

**Amendment (2026-05-20, S5 design):** `ensureSetupDefaults()` and `syncDerivedLanguageSettings()` were initially listed as orchestrator-retained but are moved to `settings-persist.ts` instead. Both are pure data functions (no DOM, no `invoke`). `renderAIFallbackSettingsUi()` calls both at lines 548–549, and the slice cannot import from `index.ts` (invariant). `settings-persist.ts` gains one import: `derivePostprocLanguageFromAsr` from `./settings/transcription.settings`. No circular dependency is introduced (confirmed: `transcription.settings.ts` does not import from `settings-persist.ts`).

`renderAIRefinementTab()` was initially listed as orchestrator-retained but moves to the slice. Rationale: all five functions it composes (`ensureTopicKeywordDefaults`, `syncAIRefinementExpanders`, `renderAIFallbackSettingsUi`, `renderTopicKeywords`, `renderAIRefinementStaticHelp`) are AI-refinement domain only — no cross-domain mutations. It is called from exactly one place: `renderSettings()` in `index.ts`. Moving it gives the orchestrator a single clean import (`renderAIRefinementTab`) instead of four, with zero call-site churn elsewhere. Decision 3's cross-domain rationale does not apply.

### 2 — `persistSettings` moves to `src/settings-persist.ts`

`persistSettings` is imported by 10 files across the codebase. It operates on the `settings` global from `state.ts`, calls `invoke("save_settings")`, and uses two normalization helpers imported from their existing source modules. Moving it to `settings-persist.ts` breaks the circular dependency that would otherwise exist between `settings/index.ts` (which imports domain modules) and domain modules (which call `persistSettings`).

Rejected alternatives:
- **Move to `state.ts`**: Would add a Tauri `invoke()` dependency to what is currently a pure-data module.
- **Callback injection**: Rejected for the same reasons as in OQ-1 — excessive ceremony for what is a straightforward import.

All 10 importers update their import path from `./settings` / `../settings` to `./settings-persist` / `../settings-persist`. This is a mechanical change and is done in the prerequisite commit (Slice 0).

### 3 — Cross-domain render functions stay in the orchestrator

`renderProductModeSettings()` and `renderGlobalOnlineModeSettings()` stay in `src/settings/index.ts`. Both span multiple CONTEXT.md domains:
- Product Mode (Transcription ↔ AI Refinement)
- Global Online Mode (AI Refinement ↔ Voice Output)

Additionally, `renderProductModeSettings()` mutates `settings.product_mode` (normalization on load) — it is not purely a UI render function. Cross-cutting initialization logic belongs in the orchestrator, not in a domain-owned module.

Rejected alternative: `src/settings/app-chrome.settings.ts` mirroring the wire module boundary. Rejected because it would put cross-cutting initialization inside a module that is supposed to own a single domain.

### 4 — Directory structure: `src/settings/<domain>.settings.ts`

Domain render modules live at `src/settings/<domain>.settings.ts`. The double-extension pattern mirrors `src/wiring/<domain>.wire.ts` — same mental model, different suffix. The directory groups all domain settings modules as siblings of the orchestrator.

### 5 — Slice order (risk-ascending)

| Slice | Module                                                                                                            | Rationale                                                                                                                                                                                                                                                                                                                                                                      |
| ----- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| S0    | prerequisite commit: move `settings.ts` → `settings/index.ts`, create `settings-persist.ts`, update all importers | zero-extraction commit; validates bundler resolution and clears the path                                                                                                                                                                                                                                                                                                       |
| S1    | `settings/vocabulary.settings.ts`                                                                                 | Isolated (~120 lines), partially covered by existing `vocab-render.test.ts`, validates the pattern                                                                                                                                                                                                                                                                             |
| S2    | `settings/overlay.settings.ts`                                                                                    | Small (~100 lines), self-contained, Overlay already has its own wire module                                                                                                                                                                                                                                                                                                    |
| S3    | `settings/transcription.settings.ts`                                                                              | Language + VAD helpers, bounded domain (~180 lines). `syncDerivedLanguageSettings` is orchestrator-retained (Decision 1) — not moved here. Secondary exports `resolveEffectiveAsrLanguageHint` and `derivePostprocLanguageFromAsr` move here; `ai-refinement.wire.ts` updates its import to `../settings/transcription.settings` directly (no re-export through orchestrator). |
| S4    | `settings/voice-output.settings.ts`                                                                               | Larger (~450 lines) but all Piper/TTS — cohesive domain. Includes one explicit cross-slice cleanup: move the temporary `ttsStopHotkey` UI write out of `transcription.settings.ts` into this module.                                                                                                                                                                           |
| S5    | `settings/ai-refinement.settings.ts`                                                                              | Largest (~850 lines), most complex — last                                                                                                                                                                                                                                                                                                                                      |

S2 owns the overlay-refining indicator helpers (`normalizeOverlayRefiningPreset`, color/speed/range normalization, TTS stop overlay display state). These settings configure how the Overlay appears during refinement, so they belong to Overlay even when AI Refinement surfaces related state. `renderOverlayHealthNote()` stays with AI Refinement because it renders an AI-refinement panel note rather than overlay controls.

### 6 — Export contract per domain module

Each domain module exports:
- One primary `render<Domain>Settings()` function (the module's entry point from the orchestrator)
- Specific secondary exports that are genuinely called from outside (e.g. `handlePiperVoiceDownloadProgress` in voice-output — called directly from `main.ts`, or provider query helpers called by the wire module)
- All other functions are private (unexported)

Secondary exports are documented in the file header, same as the OQ-3 clause 1 scoped exception for `app-chrome.wire.ts`.

### 7 — Tests

Each slice ships tests at `src/__tests__/<domain>-settings.test.ts`. Same pattern as R2 wire module tests: build DOM via `vi.hoisted` fixtures, call the primary render function, assert observable DOM state. Mock `invoke` (already globally mocked) and any modules that open external surfaces.

**Coverage intent: meaningful branch coverage per user-facing feature.** Every branch Ingo might touch should have at least one test that breaks if the logic changes. Count follows from branches, not the other way around. The "~20 per slice" guideline used in S0–S4 was too conservative — it reflected agent defaults, not the actual goal. S5 targets ~50–60 tests.

**S5 test plan** (`src/__tests__/ai-refinement-settings.test.ts`) — delivered 66 tests:
- Null guard on `settings`
- All three local provider lanes rendered (Ollama, LM Studio, Oobabooga) — including `applyProviderLaneVisibility` effects
- Ollama runtime health states: ready, starting, unreachable — note text and busy indicator
- Cloud provider list rendered (disabled rows), fallback provider selection preserved
- Prompt preset modes: built-in active, built-in modified, user preset, new mode
- Dirty state: textarea matches effective prompt (clean) vs. doesn't (dirty)
- Reasoning model warning shown/hidden in `renderCompatModelCards`
- Low-latency mode toggle → pipeline note text changes
- Topic keyword editor renders with defaults when null
- Expander state: defaults applied, toggle persisted to localStorage, `__resetForTesting()` clears cache
- Overlay health note: null (hidden), failed, recovered
- Pipeline note: 6 text variants (ai+rules, ai-only, rules-only, ai+rules+starting, ai-degraded, none)
- `renderAIRefinementTab` composition: all sub-functions called

### 8 — One commit per slice

Same discipline as R2. Each commit: one settings module + its smoke test + deletions from `settings/index.ts`. Reviewer subagent gate before each commit. Logic is preserved, not improved, during extraction.

### 9 — Security fixes are a permitted exception to Decision 8, handled as commit 2 in the same PR

Where a slice extracts code that contains a known security vulnerability (OWASP classification), the slice commit preserves the code byte-for-equivalent (honoring Decision 8) and a second commit in the same PR fixes the vulnerability. The second commit blocks PR merge — it is not a "follow-up issue." This keeps the extraction diff clean and auditable while eliminating any release window with known-insecure code.

Applied to S5: `renderCompatModelCards` interpolates `modelName` (from external LM Studio / Oobabooga server response) directly into `card.innerHTML` — OWASP A03:2021 (Injection). Fix: replace `innerHTML` template with structured `textContent` assignments. Ships as commit 2 of the S5 PR.

---

## Dependency flow after decomposition

```
src/settings/index.ts
  → src/settings/vocabulary.settings.ts   → state.ts, dom-refs.ts, settings-persist.ts
  → src/settings/overlay.settings.ts      → state.ts, dom-refs.ts, utils.ts
  → src/settings/transcription.settings.ts → state.ts, dom-refs.ts, ui-helpers.ts
  → src/settings/voice-output.settings.ts  → state.ts, dom-refs.ts, settings-persist.ts, ...
  → src/settings/ai-refinement.settings.ts → state.ts, dom-refs.ts, settings-persist.ts, ...
  → src/settings-persist.ts               → state.ts, @tauri-apps/api/core, refinement-prompts.ts
```

All flows are one-way. No module in `src/settings/` imports from `src/settings/index.ts`.

---

## What is not decided here

- Whether `settings/index.ts` is renamed after decomposition (deferred — rename after it shrinks, when the right name is obvious)
- Implementation details of individual slice perimeters (decided during execution via reviewer subagent gate)
- R1 (Rust `lib.rs` decomposition) — separate plan, status unknown
- `(window as any).runtimeInstallProgress` dead read in `renderAIFallbackSettingsUi` (line 722 of `index.ts`): fix deferred to a separate PR after S5 merges. Fix is a one-liner (`getRuntimeInstallProgress()` from `ollama-models.ts`; type shape confirmed compatible). Not a security issue — correctness only. Changes observable behavior (progress bar starts working), so warrants its own focused review.
