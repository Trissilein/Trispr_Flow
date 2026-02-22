# Roadmap - Trispr Flow

Last updated: 2026-02-20 (docs root consolidation + governance sync)

This file is the canonical source for priorities and execution order.

## Documentation Baseline (2026-02-20)

- Repo-root docs were reduced to a minimal set (`README`, `ROADMAP`, `CHANGELOG`, `CONTRIBUTING`, `CLAUDE`, `THIRD_PARTY_NOTICES`).
- Legacy root planning/context docs were moved to `docs/archive/`.
- `STATUS.md` and `SCOPE.md` are now archived files; this roadmap is the canonical planning and status snapshot.
- Canonical documentation ownership map lives in `docs/README.md`.

## Canonical Current State

- Released: `v0.6.0`
- Current phase: `v0.7.0` execution
- Foundation complete: Blocks F + G
- Active execution block: Block H (Tasks 32/33/34/35/38)

## Scope Evolution Summary (from archived `docs/archive/SCOPE.md`)

- Original intent (2025-08): simple Windows dictation app (PTT + Whisper + auto-paste).
- Actual product (v0.6.0): expanded to meeting transcription workflows (system audio, post-processing, export, diarization, quality controls).
- Scope growth was driven by real usage (meeting capture, readability needs, storage constraints), not speculative feature creep.
- Contributor guidance: keep optional features optional and log major tradeoffs in `docs/DECISIONS.md`.
 
## Analysis De-Scope Decision

- Analysis functionality is removed from Trispr Flow mainline.
- `Analyse` remains as a placeholder button in UI.
- Dedicated analysis development moved to `analysis-module-branch`.
- Mainline installer strategy is now CUDA + Vulkan only.
## Active Work Blocks

| Block | Focus | Complexity | Depends on | Status |
| --- | --- | --- | --- | --- |
| D | v0.7 AI fallback implementation (providers + prompt strategy + E2E) | High | F, G | In progress |
| E | UX/UI consistency and settings IA cleanup | Medium | D | Planned |
| F | Reliability hardening and release QA | High | D, E | Planned |

## v0.7 Task Ledger

| Task | Title | State |
| --- | --- | --- |
| 31 | Multi-provider architecture | Done |
| 36 | Settings migration + data model | Done |
| 37 | Provider config UI scaffolding | Done |
| 32 | OpenAI provider integration | Open |
| 33 | Claude provider integration | Open |
| 34 | Gemini provider integration | Open |
| 35 | Prompt strategy polish | Open |
| 38 | End-to-end tests | Open |

## Immediate Next Actions

1. Complete Task 32/33/34 provider integrations.
2. Finish Task 35 prompt strategy polish.
3. Close Task 38 with cross-provider E2E coverage.
4. Continue UX consistency passes for Capture/System settings panels.

## References

- `docs/TASK_SCHEDULE.md`
- `docs/DECISIONS.md`
- `docs/V0.7.0_ARCHITECTURE.md`
- `docs/INSTALLER_VARIANTS.md`
