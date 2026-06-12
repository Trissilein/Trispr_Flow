# Architecture & Code Audit Brief — Trispr Flow

> **Status:** Ready to execute. Not yet run.
> **Purpose:** A self-contained brief for a full architecture/code audit of Trispr Flow.
> Designed to be run **identically** by two different agents (Codex/Opus and Fable) so their
> outputs can be diffed for a quality comparison. No external context (Knowledge Graph,
> Mem Palace, session history) is required — everything needed is in this repo.

---

## 1. Mission

Trispr Flow is a Tauri v2 desktop app (Rust backend in `src-tauri/src/`, TS/Vite frontend in `src/`).
It has a **long development history** with many reworks: JIT-bypass for OLLAMA refinement,
"always-ready v2", GPU-load hysteresis gate, multiple installer-variant migrations, and several
Codex-ported feature branches merged into `main`.

The concern: **historical cruft has accumulated.** Dead state fields, unreachable code paths from
superseded approaches, duplicated logic, and orphaned modules. Your job is to find it, classify it,
and propose concrete cleanup — without breaking working behavior.

This is an **analysis-only** audit. **Do not change code.** Produce a findings report only.

---

## 2. Ground-truth inputs (read these first)

These documents describe the *intended* architecture. Compare them against the *actual* code:

| Doc | What it claims |
|-----|----------------|
| `docs/ARCHITECTURE.md` | Overall architecture, module layout |
| `docs/V0.7.0_ARCHITECTURE.md` | Current-gen architecture target |
| `docs/ARCHITECTURE_REVIEW_0.7.md` | Prior review — note what it already flagged |
| `docs/STATE_MANAGEMENT.md` | How `AppState` is meant to be used |
| `docs/APP_FLOW.md` | End-to-end runtime flow (PTT → transcribe → refine → paste) |
| `docs/DECISIONS.md` | Architectural decisions and their rationale |
| `docs/KNOWN_ISSUES.md` | Already-known problems — don't re-report these as new |

Treat divergence between these docs and the code as a finding in **both directions**: code that
drifted from the doc, *and* docs that no longer describe reality.

---

## 3. The four audit dimensions

Cover all four. For each finding, assign exactly one primary dimension.

### D1 — Dead State
- `AppState` fields (`src-tauri/src/state.rs`) that are never read, or never written, after init.
- `Settings` fields that are normalized in `save_settings_inner` (`lib.rs`) but never consumed downstream.
- Atomics/locks that are maintained but whose value never influences behavior.

### D2 — Dead / unreachable code paths
- Branches made unreachable by reworks. Specifically scrutinize:
  - The OLLAMA **bypass decision** — JIT `/api/ps` is now the source of truth (`handle_transcription_ok`).
    Find any *older* flag-based bypass logic (`ollama_model_warm`, `ollama_warmup_in_progress`) still
    acting as a decision input rather than pure UI state.
  - The **GPU-load gate** (`gpu_busy` atomic, `update_gpu_busy_gate`) vs. any earlier
    settle/cooldown bypass attempts (e.g. remnants of `feat/gpu-settle-bypass`).
  - Whisper-server lifecycle: warmup, keepalive watchdog, idle-retire — are all three reachable and
    non-overlapping?
- `#[allow(dead_code)]` annotations — each is a TODO marker; list them and judge whether the code
  should be wired up or removed.

### D3 — Duplication & inconsistency
- The settings-normalization chain in `save_settings_inner` (`normalize_*` calls) — overlaps, ordering
  hazards, fields normalized twice.
- State updates that happen in multiple places for the same logical event (e.g. overlay state set from
  several call sites with possible races).
- Repeated helper logic that should be a shared function (e.g. `ollama_runner_defining_options` is
  *required* to be byte-identical between warmup and refinement — verify it actually is the single source).

### D4 — Orphaned modules & features
- Modules in `src-tauri/src/modules/` registered but with no active consumer.
- Health-checks (`build_dependency_preflight_report`) that point at modules/features no longer present.
- Frontend wiring (`src/wiring/`) referencing backend commands that no longer exist, or vice versa
  (Tauri `invoke_handler!` commands with no frontend caller).
- Feature flags / cargo features that gate code nobody builds.

---

## 4. Cross-cutting checks (apply within every dimension)

- **Performance:** blocking I/O on the Tauri event-loop thread; redundant locks; polling that could be
  event-driven; hot-path allocations.
- **Security:** secret handling (API keys via keyring vs. plaintext); command injection surface in
  `Command::new` calls; path traversal in model/file resolution; the `strict_local_mode` guard — is it
  enforced on *every* OLLAMA path, or bypassable?
- **Correctness:** lock poisoning handled consistently? `unwrap()` on locks vs. `into_inner()`?
  Error swallowing (`let _ =`) that hides real failures?

---

## 5. Output format (MANDATORY — this enables the comparison)

Write your report to `docs/audit-results-<agent>.md` where `<agent>` is `opus` or `fable`.

Begin with a **summary table**, then **one section per finding**. Every finding uses this exact schema:

```
### F<NN> — <short title>

- **Dimension:** D1 | D2 | D3 | D4
- **Location:** `path/to/file.rs:LINE` (or range)
- **Severity:** critical | high | medium | low
- **Confidence:** certain | probable | speculative
- **Category:** performance | security | correctness | dead-code | duplication | doc-drift
- **Finding:** <what is wrong, factually>
- **Evidence:** <the specific code/grep that proves it — quote it>
- **Recommendation:** <concrete fix; "remove", "wire up", "merge with X", "verify then delete">
- **Risk if changed:** <what could break>
```

End with:
- **Coverage statement:** which files/dirs you actually read vs. sampled vs. skipped, and why.
- **Open questions:** things you couldn't resolve without running the app or asking the maintainer.

### Rules that make the two runs comparable
1. Number findings `F01, F02, …` in **severity order** (critical first).
2. Be **honest about confidence** — a `speculative` finding flagged as such is better than a false `certain`.
3. **Do not pad.** A short report of real findings beats a long one with filler. Quality over count.
4. If you find **zero** issues in a dimension, say so explicitly — don't invent findings to fill it.
5. Quote real code as evidence. No finding without evidence.

---

## 6. Scope boundaries

- **In scope:** `src-tauri/src/**`, `src/**`, `docs/**` (for drift), `Cargo.toml`, `package.json`,
  feature flags, CI workflow files.
- **Out of scope:** `vendor/` (except note the `global-hotkey` patch exists and why), generated
  artifacts, `node_modules`, `target/`.
- **Don't re-report** anything already in `docs/KNOWN_ISSUES.md` as new — reference it instead.

---

## 7. Comparison methodology (for the maintainer, after both runs)

1. Run this brief with agent A (Codex/Opus) → `docs/audit-results-opus.md`.
2. Run the **same brief unchanged** with agent B (Fable) → `docs/audit-results-fable.md`.
3. Diff: which findings overlap, which are unique to each, which were false positives.
4. Score each agent on: true findings, depth of evidence, false-positive rate, actionability of
   recommendations, coverage honesty.

The brief is deliberately identical input for both — that is what makes the comparison fair.
