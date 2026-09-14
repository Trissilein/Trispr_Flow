# Settings Cleanup — Battle Plan

Status: **IN PROGRESS**. Reviewed 2026-09-10. A1–A3 inventories, B1 static annotation, B2 editor work, and bounded B3 wording/boundary QA are complete for the current handoff. Final browser, legacy-expectation, type, and full-suite gates remain pending. No feature deletion or backend settings migration is approved.

Owner: Ingo. Product decisions remain human-owned. Codex can prepare inventories, bounded documentation changes, and low-risk implementation batches after approval.

## Scope and outcome

The review targets task-first discoverability and progressive disclosure. Users should find settings by the job they are trying to complete, while advanced controls remain available in the context where they matter.

The following are provisional guardrails for the future inventory:

- Basic view: at most 40 user-editable controls.
- Expert view: at most 60 additional user-editable controls.
- No setting is deleted solely to satisfy either number. A control can remain Config or Pending when evidence does not support removal.

Runtime and status information, action buttons, and editable values must remain separate in the inventory and in the resulting layout. Triage may be split into small batches; no one-sitting session is required. The plan does not assume a new search system or a speculative visual redesign.

## Approved final editor plan

The approved implementation is a stable per-field and per-group Expert visibility editor. It changes discoverability and local view preferences while preserving the existing setting values, Rust defaults, runtime gates, and module behavior.

- Mark logical ownership units with `data-settings-visibility-managed="true"`, `data-settings-visibility-key`, `data-settings-visibility-label`, `data-settings-visibility-default="basic|expert"`, and `data-settings-visibility-group`. Tab content uses `data-settings-visibility-tab-content`; optional group containers use `data-settings-visibility-group-container`.
- Split editable values, status, and actions into meaningful units. A wrapper containing a select and Fetch/Verify action must expose separate rows. Dynamic model, vocabulary, Task Capture, and Modules rows receive metadata after rendering.
- Store local overrides and removal proposals under `trispr-settings-visibility-v1`. A profile contains versioned `overrides` and bounded `removalCandidates` reasons. Defaults and known keys may be included in the visibility export; settings values, prompts, model content, API keys, and other secrets never enter it. Do not add a `settings.json` key or Rust settings field.
- Keep default Basic/Expert assignments in metadata. Support local override, reset-to-default, and reason retention. Unknown structurally valid saved keys remain available for lazy/dynamic entries but are ignored until a matching current entry exists; export filters to currently known definitions.
- Mixed groups are valid. A group can contain Basic values, Expert tuning, status, and actions. Per-field or logical-group choices must not turn action/status nodes into editable values. Ancestor, module, provider, mode, `hidden`, `disabled`, and other runtime gates remain authoritative.
- Feature pruning is deferred. The removal-candidate list records evidence-backed Review, Keep, or Config/Pending decisions; no Kill is implemented merely to meet Basic/Expert count signals.

### Current user flow

1. Use the existing Expert toggle to expose advanced settings, then open **Customize view** in the settings visibility editor.
2. Choose Basic or Expert for a field or group. Use **Restore defaults** to clear visibility overrides while retaining removal-candidate reasons.
3. Press **Done** to save the local view profile or **Cancel** to discard the draft. Use **Export saved view** to share visibility metadata JSON; it contains no setting values or secrets.
4. A global mode switch hides editor-owned controls in Basic mode but retains the draft. Returning to Expert resumes it. Lazy dynamic entries keep valid unknown profile keys until their definitions mount.

This flow is local to the view profile. It does not change transcription, AI, voice, module, or backend settings.

## Evidence and counting rules

The 2026-09-02 measurements are historical context: 413 reported `class="field"` elements, 44 expert-only markers, about 20 sections across six tabs, and 1,207 lines in the largest settings module. They are not a current baseline.

The current 2026-09-10 audit reports:

- 417 elements in [`index.html`](../../index.html) carrying a `field` class token.
- 60 elements with the exact attribute `class="field"`; this is not a useful inventory unit by itself.
- 160 actual `<input>`, `<select>`, and `<textarea>` controls.
- 44 `data-expert-only` attributes; this does not count inherited visibility from parent sections or dependencies.
- 16 `<section>` elements.
- 1,112 lines in [`src/settings/ai-refinement.settings.ts`](../../src/settings/ai-refinement.settings.ts).

These numbers answer different questions. The inventory counts actual controls, wrappers, actions, persisted keys, and visibility dependencies separately. B1 tests still need to establish final annotated coverage after static and dynamic rendering; this plan must never claim that the counts are equivalent.

The final B1 static ownership baseline is **166 managed units** across **204 non-dialog controls**, with **24 Basic**, **142 Expert**, and **39 explicit global/navigation exclusions**. Hotkey input/record pairs share logical units and profile keys remain unique. The Expert total deliberately exceeds the provisional 60-unit review signal; no control is removed to satisfy a quota, and no Kill decision is implied. Controls, actions, status, logical units, and persisted keys remain separate dimensions.

## Optional P0: language semantics clarification

This bounded clarification is tracked alongside B3. It is not a prerequisite for the inventory and is not a broad prompt redesign.

Confirmed evidence:

- [`src-tauri/src/ai_fallback/provider.rs`](../../src-tauri/src/ai_fallback/provider.rs#L542-L552) leaves non-empty custom prompt text untouched.
- [`src-tauri/src/ai_fallback/provider.rs`](../../src-tauri/src/ai_fallback/provider.rs#L1263-L1272) can reject output language drift.
- [`src-tauri/src/ai_fallback/mod.rs`](../../src-tauri/src/ai_fallback/mod.rs#L313-L367) selects the guard through a pure predicate with `preserve && profile != "llm_prompt"`.

The hint that custom prompt text is not changed does not promise that a custom profile bypasses the language guard. Keep the preserve flag authoritative for built-in and custom profiles, while retaining the established `llm_prompt` exception. Approved wording should say that LLM Prompt is exempt and **requests English output**, without promising model compliance. Do not add a blanket custom bypass, an intent classifier, or a new setting.

Turning the guard off can permit a translation request, subject to the model and other checks; it is not a guarantee. Empty custom text falls back to the built-in Wording prompt. Keep dynamic and static hint copy aligned during final QA.

Boundary tests cover built-in prompts, non-empty custom prompts, empty custom fallback, preserve on and off, a DE-to-EN request, and `llm_prompt`. Assert both guard selection and output behavior. Current targeted results are **1 composed Rust provider boundary test**, **2 Rust guard-predicate tests**, **72 TS AI settings tests**, and **13 B2 visibility tests** passed. The last full TypeScript run was **663/664**; one shared B1 legacy expectation and the shared B1 type/build fix remain pending.

## Phase 1 — inventory and evidence

Deliverables: [`docs/plans/settings-inventory.md`](settings-inventory.md) and [`docs/plans/settings-removal-candidates.md`](settings-removal-candidates.md). Start from actual DOM inputs, toggles, selects, textareas, and action buttons. Record actions separately from editable values. Map each candidate through TypeScript bindings and persisted Rust paths.

Each inventory row must include:

`task | current tab/section | label | HTML id(s) | TypeScript binding and type | Rust nested path | JSON key and type | default | visibility, including ancestors and dependencies | consumer and side effect | evidence | proposed verdict and reason | decision owner | decision status`

UI-only controls must be explicit. Unmatched bindings and unmatched persisted consumers must be reported and resolved; acceptance is not “within ±5 rows.” Do not require a complete backend-only settings inventory before starting. For each UI candidate, identify backend-only consumers that could be affected. Map nested module structs and `Settings` types beyond [`src-tauri/src/ai_fallback/models.rs`](../../src-tauri/src/ai_fallback/models.rs) and [`src-tauri/src/state.rs`](../../src-tauri/src/state.rs).

Inventory exit gate: every actual control has a row or an explicit exclusion, every persisted key referenced by an inventoried control has an owner or an unresolved finding, visibility dependencies are recorded, and the report states which counts were used.

## Phase 2 — triage and product decisions

Use five statuses: **Basic**, **Expert**, **Config**, **Kill**, and **Pending**. Pending is a valid outcome while evidence or product direction is missing; it is not a forced verdict.

For each candidate, answer four questions:

1. Which user job does it serve, and how often does that job occur?
2. What happens when it is absent, including device recovery, accessibility, and infrequent critical workflows, and is there a safe default?
3. Is it supported advanced behavior or internal tuning?
4. Is it provably unused, or is it a live feature requiring an explicit product decision?

The agent proposes status and rationale. The owner reviews uncertain, destructive, and cross-feature decisions in small batches. Treat the Basic and Expert limits as review signals: if proposed counts exceed them, record the reason and obtain product review. Do not force a quota by deleting controls.

Config means the setting stays in `settings.json` with documented instructions for how to edit it, what validation does, and whether a restart is required. The path must survive UI updates and save operations. Kill requires demonstrated obsolescence or approved feature removal, plus an explicit migration and round-trip decision. Live feature changes are separate from UI reorganization; a Kill decision changes behavior and must not be described as behavior-neutral.

## Phase 3 — implementation sequence

Use this sequence after the inventory and triage gates:

1. Discover and document control, binding, persistence, and consumer relationships.
2. Select one small pilot with proven low coupling. Modules may be a candidate, but selection is evidence-based and not automatic.
3. Review pilot usability, persistence, visibility dependencies, and runtime effects.
4. Deliver remaining bounded PRs by dependency and coupling, rather than treating seven PRs as mandatory.

The original seven groups remain hypotheses: Recording; Text Output; AI Refinement; Language & Vocabulary; Voice Output; Modules; and Advanced. Advanced must not become a dumping ground; expert controls stay with their task context whenever one exists. Treat Recording as last only when dependency evidence shows the largest blast radius, and record that heuristic for review.

Keep overlay-coupled state isolated. Separate layout-only edits, config hiding, and deletion or migration changes into distinct reviewable changes. Do not mix them in one PR merely for convenience.

## Persistence and validation

Persistence review must use the actual code paths:

- [`src-tauri/src/state.rs`](../../src-tauri/src/state.rs#L300) defines the root `Settings` state.
- [`src-tauri/src/state.rs`](../../src-tauri/src/state.rs#L1043-L1047) loads with `from_str(...).unwrap_or_default()`; malformed input can silently fall back.
- [`src-tauri/src/state.rs`](../../src-tauri/src/state.rs#L1824-L1847) saves a normalized full clone through an atomic rewrite.

`serde(default)` covers missing keys. A removed key is ignored during load and disappears on the next save. Audit every candidate for TypeScript defaults and normalizers, migrations, types, binding and render wiring, commands and events, tests, and documentation. Search import, export, and reset entry points for each candidate; do not claim absence without evidence.

Future fixtures must cover an old config, a custom config with unrelated values, load/save/reload, invalid-load behavior, retained Config values, and deliberate Kill disappearance. Verify rollback or migration before any data-loss boundary.

Run existing settings-domain tests, targeted backend tests, and build/type-check scripts for the relevant package after inspecting their package definitions. Run the complete repository suite before merge. UI validation must cover Basic and Expert visibility, dependent visibility, keyboard navigation, default and non-default config, dynamic rows, export redaction, and relevant runtime smoke. Targeted B3 checks currently pass as recorded above; final annotated-entry counts and the blocked wire suite remain open.

## Estimates and routing

These estimates are provisional:

- Discovery and inventory: 2–4 hours.
- Triage: 2–4 hours, split across decision batches.
- Pilot: 0.5–1 day.
- Remaining work: estimate only after pilot coupling and persistence results.
- Optional P0: depends on clarified UI wording and boundary tests; no five-minute estimate.

| Role | Use |
|---|---|
| Astra | Parent orchestration, plan review, integration, and final risk review. |
| Terra, high | Deep audit, persistence analysis, and risky migration diagnosis. |
| Luna, max | Inventory, documentation, and mechanical bounded implementation. |
| Astra, medium | Only if Terra is insufficient; record the reason before escalation. |
| Sol | Do not use unless explicitly requested. |

Human product decisions remain required for Basic versus Expert, Config versus Kill, wording, and any live feature change.

## Exit gates and per-PR acceptance

The work is ready to leave discovery when the inventory has resolved matches, explicit UI-only rows, persisted-key ownership, visibility dependencies, and evidence-backed proposed statuses. Triage is complete when the owner has reviewed all non-Pending rows and recorded decisions for every Pending item that blocks implementation.

Each implementation PR must state its scope as layout-only, config hiding, or deletion/migration; link affected inventory rows; show the persistence and consumer audit; preserve or document overlay boundaries; include relevant tests and runtime smoke results; and provide migration, rollback, or explicit no-data-loss evidence where applicable.

The cleanup is complete only when Basic and Expert views are usable by task, settings round-trip safely, Config entries retain a documented workflow, approved Kill entries have deliberate migration coverage, and the required test gates are green.
