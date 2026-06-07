# Trispr Flow Modularization — Candidate List

Date: 2026-05-25
Status: **candidates surfaced, not yet decided**
Participants: Hendr (architect, on behalf of Ingo who raised the question)

---

## Context

Ingo's framing (paraphrased from his message):

> Falls du mal wirklich zu viel Zeit hast: die Modularisierung des Trisper-Flows wäre tatsächlich was, was man angehen könnte. Also die Architektur wieder verschärft darauf umbauen, dass Trisper-Flow an sich wirklich nur Whisper mit dem Rule-based-Refinement ist. Das AI-Refinement ist ein komplett eigenständiges Modul (wo dann auch ollama und die modelle zugehören), und auch alle anderen Module sollen viel strikter getrennt, und von mir aus auch nachinstalliert werden können. Aktuell ist es so, dass sehr viel irgendwie immer noch halb mitgeladen wird, wenn das "pure" Trispa-Flow geladen wurde.

The intuition is mechanically correct. The Module System in [`src-tauri/src/modules/`](../../../src-tauri/src/modules/) is a **registry of descriptors plus an enable flag in `ModuleSettings`** — not a real seam. Every Module's Rust code is unconditionally `mod`-declared in [`src-tauri/src/lib.rs`](../../../src-tauri/src/lib.rs) and unconditionally `use`d from there. "Module disabled" only suppresses runtime behavior via `capability_enabled()`; the code is in the binary either way. That is exactly what Ingo means by "wird halb mitgeladen."

### Concrete signals (scan, 2026-05-25)

`lib.rs` references per top-level module:

| Module               | refs in `lib.rs` |
| -------------------- | ---------------: |
| `gdd`                |              153 |
| `confluence`         |               85 |
| `workflow_agent`     |               84 |
| `ai_fallback`        |               66 |
| `multimodal_io`      |               56 |
| `ollama_runtime`     |               22 |
| `assistant_presence` |               19 |
| `weather`            |               14 |
| `video_ingest`       |               10 |

`[features]` block in [`src-tauri/Cargo.toml`](../../../src-tauri/Cargo.toml): empty. Every `ModuleManifest` with `bundled = true` also has `installed_by_default = true` except `output_video_generation`. "Install/uninstall" is a UI state — it cannot exclude code.

Deletion test: deleting `ai_fallback/` breaks `lib.rs` compilation in 66 places. Deleting `gdd/` breaks it in 153 places. A real Module would let you delete its directory and the core would still build.

### R1 status correction (was "not started" in the 2026-05-15 plan)

R1 is done in practice as of 2026-05-25. `lib.rs` now has 21 `#[tauri::command]` functions (mostly bootstrap/diagnostics). Commands are distributed: `ai_fallback/commands.rs` (16), `gdd/confluence.rs` (14), `multimodal_io.rs` (12), `models.rs` (9), `ollama_runtime.rs` (9), `history_partition.rs` (9), `workflow_agent.rs` (8), `audio.rs` (7), `gdd/mod.rs` (7), plus smaller files. The 2026-05-15 plan is amended separately.

---

## Candidates

Not yet decided. Each candidate's interface is intentionally not designed yet — candidate selection comes first.

### 1 — Name the core. Make "Trispr Core" an explicit Module, not a residue.

**Files**: new `src-tauri/src/core/` (or `trispr_core/`); large parts of `lib.rs` move into it; CONTEXT.md gains the term "Trispr Core."

**Problem**: "Core" is implicit — whatever is left after subtracting opt-ins. No module, file, or test surface says *this and only this is pure Trispr-Flow* (PTT/VAD capture → Continuous Dump → Whisper Backend → Post-Processing → emit). You cannot reason about what should *stay loaded* when AI Refinement, GDD, Confluence, Workflow Agent, Voice Output, Vision, Video Generation are all off, because no boundary describes it.

**Solution**: Define Trispr Core as a first-class domain. Everything currently listed in the manifest as non-core (`ai_refinement`, `gdd`, `task_capture`, `assistant_core`, `assistant_presence`, `input_vision`, `output_voice_tts`, `output_video_generation`, `integrations_confluence`) sits behind the Module System seam. Core depends only on the registry, never on a specific Module's types.

**Benefits**: Locality of the most-touched code path. The deletion test becomes meaningful — deleting `gdd/` would not break the core build. Tests gain a clear surface ("does Trispr Core work with zero Modules enabled?").

---

### 2 — Per-Module Cargo feature flags

**Files**: [`src-tauri/Cargo.toml`](../../../src-tauri/Cargo.toml), every `mod foo;` in `lib.rs`, every cross-module `use crate::ai_fallback::…` site.

**Problem**: There is no build configuration in which `ai_fallback`, `gdd`, `confluence`, `multimodal_io`, etc. are absent. "Module disabled" only suppresses runtime behavior. This is the literal "halb mitgeladen" Ingo named.

**Solution**: Add a `[features]` block — one flag per opt-in Module, plus `default = ["ai_refinement", "gdd", …]` for the current full build. Wrap each `mod` declaration and each cross-module `use` site with `#[cfg(feature = "…")]`. Variant installers (`run_build.ps1` / `generate-tauri-variant-config.mjs`) pick the feature set; a "pure" variant excludes everything optional.

**Benefits**: Real load-time separation, smaller binary for stripped variants, compile-time enforcement that the core does not reach across the seam. `cargo build --no-default-features` either works or names every coupling violation.

**Conflicts**: Contradicts the implicit assumption in OQ-2 of [`2026-05-15/refactoring-plan.md`](../2026-05-15/refactoring-plan.md) (single `generate_handler!` list in `lib.rs`). Worth reopening because Ingo's question is the full architecture pass that OQ-2 deferred to.

---

### 3 — Promote each Module to its own Tauri plugin with its own handler set

**Files**: `lib.rs` builder block and `generate_handler![…]`, each module's `mod.rs`.

**Problem**: `lib.rs` is the registration funnel for every command of every Module. Even after R1 moved implementations, the `pub use` re-exports and the giant `generate_handler!` list keep `lib.rs` as the file that must change whenever any Module gains a command. The Module System cannot truly own its command surface.

**Solution**: Each Module exposes `pub fn plugin() -> TauriPlugin<R>` that registers its own commands and managed state. `lib.rs`'s builder becomes `.plugin(core::plugin()).plugin(ai_refinement::plugin())…`, gated by feature flags from #2. "AI Refinement uninstalled in this build" = "no plugin registered" = the commands literally don't exist.

**Benefits**: Each Module is its own deep unit — one place owns its commands, state, settings, and lifecycle hooks. Leverage at the seam is the Tauri plugin trait. Locality is a Module's entire surface in its directory.

**Conflicts**: Explicitly contradicts the resolved OQ-2 in [`2026-05-15/refactoring-plan.md`](../2026-05-15/refactoring-plan.md), which chose `pub use` and rejected per-domain plugins as "ceremony now for later benefit." Worth reopening for the same reason as #2 — the later benefit is now in scope.

---

### 4 — Fold `ollama_runtime` into the AI Refinement Module

**Files**: [`src-tauri/src/ollama_runtime.rs`](../../../src-tauri/src/ollama_runtime.rs), [`src-tauri/src/ai_fallback/`](../../../src-tauri/src/ai_fallback/).

**Problem**: Ingo's wording is exact: *"AI-Refinement ist ein komplett eigenständiges Modul (wo dann auch ollama und die modelle zugehören)."* Today `ollama_runtime.rs` is a top-level peer of `ai_fallback/`, called 22× directly from `lib.rs`. Conceptually one feature, split into two unrelated paths at the top level.

**Solution**: Move `ollama_runtime.rs` into `ai_fallback/runtime.rs` (and rename the directory `ai_refinement/` while we're there — the canonical user-facing name per [CONTEXT.md](../../../CONTEXT.md)). External callers go through one entry point. The model registry (already in `ai_fallback/models.rs`) joins it.

**Benefits**: Smallest, cheapest concrete win for Ingo's specific complaint. The AI Refinement Module becomes a single deep box: providers + local runtime + models + prompts behind one interface. Precondition for #2 and #3 on this Module.

**Blocked by parallel work (2026-05-25)**: The `origin/codex/runtime-resume-doc` branch and the recently merged PR #3 on `origin/main` both rewrite exactly these files. Divergence stats from `refactor/maintainability-foundation`:

| Branch                            | `ollama_runtime.rs` | `ai_fallback/provider.rs` | `lib.rs` |
| --------------------------------- | ------------------: | ------------------------: | -------: |
| `origin/main` (PR #3 merged)      |                +537 |                      ±300 |     +621 |
| `origin/codex/runtime-resume-doc` |                +537 |                      ±300 |     +735 |

#4 cannot proceed before `main` is merged into the branch and codex's branch either lands or is abandoned. Attempting it now creates a guaranteed merge collision and invalidates work already on `main`.

---

### 5 — Stop the lifecycle dispatcher from reaching into per-Module settings

**Files**: [`src-tauri/src/lib.rs`](../../../src-tauri/src/lib.rs) `enable_module` invocation site (~L3285), [`src-tauri/src/modules/lifecycle.rs`](../../../src-tauri/src/modules/lifecycle.rs).

**Problem**: The site that enables a Module hard-codes string-matches for every Module ID and mutates that Module's settings field directly (`settings.ai_fallback.enabled = true`, `settings.voice_output_settings.enabled = true`, etc.). Adding a new Module = editing this site. The registry knows every Module's settings shape. That's reverse direction.

**Solution**: Each Module owns `on_enable(&mut Settings)` / `on_disable(&mut Settings)` (or owns its own settings slice entirely — see #6). The lifecycle dispatcher calls it polymorphically. No per-ID arms.

**Benefits**: Locality — a Module's enable side-effects live in its directory. Leverage — the dispatcher is genuinely generic.

---

### 6 — Decompose the `Settings` god-struct into per-Module slices

**Files**: [`src-tauri/src/state.rs`](../../../src-tauri/src/state.rs) `Settings`, every persistence path.

**Problem**: `Settings` holds `ai_fallback`, `gdd_module_settings`, `confluence_settings`, `workflow_agent`, `vision_input_settings`, `voice_output_settings`, `video_generation_settings`, `task_capture_settings` directly. Adding a Module = editing the core struct. Removing one = compile errors everywhere.

**Solution**: `Settings { core: CoreSettings, modules: ModuleSettingsRegistry }` where `ModuleSettingsRegistry` stores per-Module settings keyed by ModuleId. Strongly-typed variants are possible (each Module registers its settings type at startup) but the load/save format stays JSON-compatible via per-Module serializers.

**Benefits**: Hard prerequisite for #2 (feature-gated removal of `ai_fallback` requires the `Settings` struct to not name it). Locality of settings shape. Same shape `ModuleSettings.module_overrides` already gestures at, made first-class.

---

### 7 — Finish R1 first

**Status (2026-05-25)**: Done. Superseded by the R1 amendment in [`2026-05-15/refactoring-plan.md`](../2026-05-15/refactoring-plan.md). Kept here for traceability — the original walkthrough listed it as a gate.

---

### 8 — Multimodal/Video drift mirrors the AI Refinement problem

**Files**: [`src-tauri/src/multimodal_io.rs`](../../../src-tauri/src/multimodal_io.rs) (3208 LOC, 56× refs), [`src-tauri/src/video_generation.rs`](../../../src-tauri/src/video_generation.rs), [`src-tauri/src/video_ingest.rs`](../../../src-tauri/src/video_ingest.rs), [`src-tauri/src/tts_benchmark.rs`](../../../src-tauri/src/tts_benchmark.rs).

**Problem**: The "Output Voice TTS" Module and "Output Video Generation" Module each have their code scattered across three or four top-level files, plus large `lib.rs` integration. Same shape as #4 but for the multimodal side.

**Solution**: A `voice_output/` directory absorbing the TTS halves of `multimodal_io.rs` and the live-path code already discussed in QW3b; a `video/` directory absorbing both `video_generation.rs` and `video_ingest.rs`.

**Benefits**: Each Module's deletion test starts to pass. Precondition for feature-gating these Modules under #2.

---

## Recommended sequencing (architect's read, not yet ratified)

- **#4** is the cheapest concrete answer to the literal AI-Refinement complaint, but is currently blocked by parallel work on `ollama_runtime.rs` and `ai_fallback/provider.rs` (codex branch + recently merged `main`).
- The shape Ingo is asking for is **#1 → #6 → #2** as a sequence (name the core, give Modules their own settings, then feature-gate). **#5** is a natural precursor to #6.
- **#3** is the strongest seam but the largest reversal of an existing ADR.
- **#7** is already done.

Each candidate that is picked up needs its own ADR with interface design before execution.

---

## Open questions

### OQ-1 — Wait for codex, or merge and rebase?

`origin/main` is ahead of `refactor/maintainability-foundation` on `ollama_runtime.rs`, `ai_fallback/provider.rs`, `lib.rs`, and `ollama-models.ts`. `origin/codex/runtime-resume-doc` adds more on top. Before #4 can be attempted: merge `main` into our branch, then decide whether to wait for codex to land or coordinate explicitly. This is for Ingo to decide.

### OQ-2 — Are #2 and #3 worth reopening the 2026-05-15 OQ-2 decision?

The 2026-05-15 plan deferred the per-domain plugin discussion. Ingo's question raises it again. If #2/#3 are accepted, the prior OQ-2 resolution is superseded; if rejected, it stands. To be decided when a candidate is selected.
