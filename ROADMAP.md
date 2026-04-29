# Roadmap — Trispr Flow

Last updated: 2026-04-29 — restructured to A-Z priority scheme with complexity / Codex-delegation tags.

This file is the canonical source for priorities and execution order. Block IDs (A, B, C, …) reflect **current** execution priority, not historical phase. The Legacy-ID-Mapping at the bottom maps old letter codes (U, V, W, …) used in commit history.

## Snapshot

- **Released:** `v0.7.0`, `v0.7.1`, `v0.7.2`, `v0.7.3`, `v0.7.4`, `v0.7.5`
- **Current phase:** `v0.8.x` assistant hardening; Block A (former U) gate-closure pending; Soak-runs intentionally skipped per 2026-04-29 decision.
- **Foundation complete:** Blocks D, F, G, H, L, M, N, Q, S, T, V (legacy IDs).
- **Active block:** A (gate-closure of v0.8.x release).

## Complexity Buckets (used in tables below)

Aligned with `~/.claude/skills/codex-delegate/SKILL.md`:

| Bucket | Heuristic | Codex-default? |
|---|---|---|
| trivial | One file, no logic, replace/rename/typo | ✅ gpt-5.4-mini / minimal |
| simple | Repeating known pattern, boilerplate | ✅ gpt-5.3-codex / medium |
| moderate | Standard implementation from clear spec | ✅ gpt-5.4 / medium |
| complex | Multi-file integration, non-trivial state | ⚠️ gpt-5.4 / high — split first |
| architecturally-tricky | Design judgement needed, cross-cutting | ❌ Claude does it |

---

## Active Zone (A → F)

| ID | Title | Complexity | Codex-able | Effort | Depends on | Status |
|---|---|---|---|---|---|---|
| **A** | v0.8.x Release Gate Closure (no soak) | moderate | ✅ mostly | ~1 day | — | 🟡 active |
| **B** | UX/UI consistency + Picker-Unification | complex | ⚠️ mixed | 3–5 days | A | 📋 next |
| **C** | Adaptive AI Refinement (VRAM probe + quant matrix + keep-alive + vocab-ground-truth) | complex | ⚠️ mixed | 5–7 days | B | 📋 planned |
| **D** | Reliability Hardening + Release-QA polish | complex | ⚠️ mixed | 5 days | C | 📋 planned |
| **E** | Expert-Mode UX Toggle (standard/expert) | moderate | ✅ mostly | 2–3 days | B | 📋 planned |
| **F** | Assistant Presence Window (3D dot-cloud + TTS panel) | architecturally-tricky | ❌ Claude | 1–2 weeks | A, D | 📋 later |

### A — v0.8.x Release Gate Closure

- **A1** Generate `bench/results/tts.latest.json` (run `npm run tts:benchmark`) — *trivial, Codex* — runs harness, captures result, attaches to gate. Fallback: I run it inline if Codex unavailable.
- **A2** Run `npm run qa:assistant -- --strict-benchmark` (no `--require-soak`) — *trivial, Codex* — exit-criteria check. Sign-off doc in `docs/reports/`.
- **A3** Update `STATUS.md` + `CHANGELOG.md` with v0.8.x release notes — *simple, Codex* — boilerplate writes from gate-output JSON.
- **A4** Cut tag `v0.8.0` and push — *trivial, manual* — Claude orchestrates, user runs `git tag` + `git push --tags` himself.

### B — UX/UI consistency + Picker-Unification

- **B1** Audit settings panel IA: capture inconsistencies (spacing, label-style, expander depth) → checklist — *moderate, Claude* — needs design judgement.
- **B2** Picker-Unification: Ollama vs LM Studio model lists → single bottom section (was R5) — *moderate, Codex* — pattern is clear after B1.
- **B3** Apply settings refactors per B1 checklist — *simple-to-moderate, Codex* — repetitive once decisions made.
- **B4** Modernization pass: refresh icon system, animation easing, hover-states — *moderate, Claude* — design-heavy, hard to delegate cleanly.

### C — Adaptive AI Refinement

- **C1** Backend GPU-VRAM probe (was 43) — *complex, Claude* — Tauri-Rust + nvml/vulkan-info + Tauri-command surface. Probably needs platform-specific paths.
- **C2** Wire VRAM probe into Fallback-UI (was 43a finish) — *moderate, Codex* — once C1 surface is stable.
- **C3** Refinement Keep-Alive ping for Ollama / LM Studio (was R4) — *moderate, Codex* — mirror Whisper pre-warm pattern.
- **C4** Quantization profile matrix UI polish (was 45 finish) — *simple, Codex* — labels + tooltips on existing model picker.
- **C5** Pre-Paste-Diff ground-truth (frontend learn from refinement diffs, complement to UIA capture) — *moderate, Claude* — touches existing vocab-auto-learn architecture, judgment on signal weighting.
- **C6** Asymmetric vocab promotion: lower threshold for `vocab_terms` (Whisper-bias) than for `postproc_custom_vocab` (find-replace) — *moderate, Codex* — clear spec, mechanical.

### D — Reliability Hardening + Release-QA polish

- **D1** Soak-test automation harness (since manual soak is skipped, build automated 4h regression run) — *complex, Codex* — clear spec but multi-file.
- **D2** Latency-budget assertions in benchmark gate (e.g. p95 ≤ X for refinement) — *moderate, Codex* — extends existing gate.
- **D3** First-run developer-bootstrap polish (FIRST_RUN.bat audit) — *simple, Codex* — script-only.
- **D4** Crash-report auto-collection on quit (panic logs + last-N tracing lines packaged) — *moderate, Claude* — needs Win32 path judgement.

### E — Expert-Mode UX Toggle

- **E1** Add `expert_mode` setting + persistence — *trivial, Codex* — boilerplate.
- **E2** Hide technical settings groups when expert_mode=false — *simple, Codex* — pattern from existing module-gating.
- **E3** UX writing for "what's in expert mode" tooltip + onboarding hint — *simple, Claude* — copy needs voice/tone judgement.

### F — Assistant Presence Window

- **F1** Architecture: how does the Presence-Window relate to overlay + assistant-presence (3 windows now)? — *architecturally-tricky, Claude*
- **F2** 3D dot-cloud renderer (WebGL inside Tauri webview) — *complex, Codex* — given clear visual spec.
- **F3** TTS text panel (real-time word streaming) — *moderate, Codex* — once F1 plumbing is set.
- **F4** Window-mgmt integration (always-on-top, click-through, multi-monitor) — *complex, Claude* — overlay-style edge cases.

---

## Deferred Zone (Z)

| ID | Title | Reason | Unblock condition |
|---|---|---|---|
| **Z1** | Cloud provider rollout (OpenAI/Claude/Gemini for refinement) | Local-first is sufficient for now | when an explicit user need or business case appears |
| **Z2** | LM Studio per-request thinking disable (was R6) | `chat_template_kwargs` ignored by llmster backend | when LM Studio adds API for per-request thinking control |
| **Z3** | Screen-Recording ground-truth path (was 44d) | Depends on Screen-Recording module which doesn't exist yet | when Screen-Recording module lands (separate roadmap) |
| **Z4** | UIA `RuntimeId`-based identity (additional to HWND/PID) | HWND/PID match has been sufficient in testing | only if false-positive learning re-appears across same-window controls |

---

## Codex Bridge Health

The `codex-delegate` skill provides automatic failure handling. Claude triggers the bridge per task, with these outcomes:

| `failure.kind` | Bridge response | What gets done |
|---|---|---|
| `ok` | accept Codex result | task closes |
| `quota_exhausted` | **circuit-breaker trips** (2 consecutive → no further delegation that session) | Claude takes over remaining Codex-tagged tasks inline |
| `rate_limit` | wait 60 s, retry once | if 2nd attempt fails → take over |
| `auth_error` | surface to user (`codex login`) | take over current task; halt delegation until user re-auths |
| `model_unavailable` | retry with `gpt-5.4` (next-tier fallback) | continue if recovery succeeds |
| `timeout` | split task or take over | timeout typically means prompt was too ambitious |

**Robustness rules for this roadmap:**
1. Every task in the Active Zone with "Codex-able: ✅" or "⚠️" has an implicit Claude fallback. If the bridge dies mid-block, Claude finishes the block.
2. "Codex-able: ❌" tasks are never delegated — Claude does them by hand.
3. Architecturally-tricky decisions (B1, C1, C5, F1, F4, D4, E3) are explicitly Claude-owned even when sub-implementation is Codex-delegated.
4. After a `quota_exhausted` event, Claude documents the unfinished sub-tasks in `docs/reports/codex-bridge-paused.md` so the user can see what's queued for the next quota window.

**Quota visibility limitation:** ChatGPT-OAuth quota is server-side; the CLI does not expose remaining budget. The bridge learns of quota exhaustion only at the next call. This is acceptable because the circuit-breaker prevents repeat hits.

---

## Legacy Block-ID Mapping

| Old ID | New ID | Notes |
|---|---|---|
| U | A | Assistant UX + Soak Gate; soak runs skipped per 2026-04-29 decision |
| E | B | UX/UI consistency; absorbs R5 (Picker-Unification) |
| J | C | Adaptive AI refinement; absorbs 43, 43a, 45, R4 |
| F | D | Reliability hardening |
| K | E | Expert-Mode toggle |
| W | F | Assistant Presence Window |
| G | Z1 | Cloud provider — deferred |
| R6 | Z2 | LM Studio thinking-disable — externally blocked |
| 44d | Z3 | Screen-Recording ground-truth — depends on new module |
| D, H, L, M, N, Q, R1-R5, S, T, V | history | Completed in earlier phases — see History below |

---

## Phased Path to Full Agent (unchanged from prior roadmap)

**Phase 0 — Stabiler Unterbau (`S13.5`)** — done.
**Phase 1 — Assistant Pivot Foundation (Block T → v0.8.0)** — done.
**Phase 2 — GDD Copilot Loop (Block V → v0.8.x)** — done.
**Phase 3 — Voice Confirmation Loop (Block O)** — planned post-A.
**Phase 4 — Hands-free Actions (Block P)** — planned post-Phase 3.
**Phase 5 — Vollwertiger Mitarbeiter-Modus** — continuous.

(Phase IDs O/P remain in their original notation since they map to long-term agent evolution, not the current sprint queue.)

---

## History (Done) — compressed

### 2026-04-29
- Identity-Tracking + Caret-Range vocab capture (HWND/PID validation, `selection-line` pattern, Self-Filter)
- Window-Shrinking heuristic in `wordDiff` for VS-Code/Monaco compatibility
- Overlay topmost hardening (toggle re-promote + heartbeat re-assertion)
- Task 44c — Adaptive Vocabulary Regression Tests (43 tests green)
- CSS branding: CUDA→Nvidia-green, Vulkan→AMD-red on backend-switch buttons
- ROADMAP.md restructured to A-Z priority scheme

### 2026-04-08
- v0.7.3 release: Models section + Custom Vocabulary UI redesigns; vocabulary casing bug fixed
- 219 tests green at release time

### 2026-04-07
- Block J / Tasks 44a-44b: vocabulary learning live (LLM-diff, threshold, auto-add, suggestion review)
- Custom Vocabulary UI flattened (3 levels → 1)
- Ollama refinement keep-alive 20m → 60m

### 2026-04-06
- Hotkey overhaul: ISO `</>` key support, DE keyboard Y↔Z fix, hybrid event.code/event.key strategy
- TTS-Stop default hotkey: Ctrl+Shift+Esc → Ctrl+Shift+F12 (Task Manager conflict)
- Gemma 4 model variant matrix (5 quantizations with VRAM annotations)
- Model-family-specific refinement prompts (Gemma anglicism preservation)
- Ollama runtime updated to v0.20.2 + download progress popup
- Block J designed (LLM-diff suggestion flow → 44, 44a, 44b, 44c, 44d)

### 2026-03-22
- Block S6-S13 landed: AI Refinement as toggleable module, strict module-UX decoupling, dedicated voice-output tab, autostart + warmup speed path, overlay deep refactor

### 2026-03-20
- Block N9 (privacy/consent UX), N10 (TTS fallback matrix), N11 (TTS benchmark harness)
- N11 evidence run: `windows_native` recommended (success_rate=100%, p50=245ms, p95=282ms)
- Overlay startup hardening (transparent fallback to non-transparent)
- FFmpeg packaging cleanup (build-time fetch, SHA256 + OPUS validation)

### 2026-03-18
- Prompt-style chip selector UI
- Logging: daily `.txt` files; shutdown log entry distinguishes normal exits from crashes
- Crash-Proof Shell P1-P9 (panic hooks, spawn_guarded, lock_unwrap_or_else, guarded_command, …)

### 2026-03-17
- LM Studio integration LS1 (daemon lifecycle), LS2 (max_tokens for OpenAI-compat)
- Block R1 (input truncation 2000-word cap), R2 (LM Studio auto-start), R3 (reasoning-model UI warning)

### Earlier
- v0.7.0 GPU acceleration: CUDA detection, runtime backend preference, Q5 quantized models for VRAM-constrained GPUs
- Block T (Assistant Pivot Foundation, v0.8.0) and Block V (GDD Copilot Loop) — see git log for details
- Block L (Module Platform), Block M (Workflow-Agent voice automation), Block Q (Onboarding refinement)
- Crash-Proof Shell + LM Studio integration phases

---

## References

- `~/.claude/skills/codex-delegate/SKILL.md` — Codex bridge protocol and circuit-breaker
- `~/.claude/skills/orchestrator/SKILL.md` — multi-agent plan/task coordination
- `docs/V0.8.x_BLOCK_U_RELEASE_GATE.md` — A-block gate procedure
- `docs/AGENT_EVOLUTION_ROADMAP.md` — long-term Phase 3-5 detail
- `docs/ARCHITECTURE_REVIEW_0.7.md`, `docs/V0.7.0_ARCHITECTURE.md`
- `docs/INSTALLER_VARIANTS.md`, `docs/N11_TTS_BENCHMARK.md`
- `docs/V0.8.0_BLOCK_L_ROLLOUT_PACKET.md`, `docs/V0.8.1_WORKFLOW_AGENT_PLAN.md`, `docs/V0.8.2_MULTIMODAL_IO_PLAN.md`
