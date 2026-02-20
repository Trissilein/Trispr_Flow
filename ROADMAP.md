# Roadmap - Trispr Flow

Last updated: 2026-02-20

This file is the canonical source for priorities and execution order.

## Canonical Current State

- Released: `v0.6.0`
- Current phase: `v0.7.0` execution
- Foundation complete: Blocks F + G
- Active execution block: Block H (Tasks 32/33/34/35/38)

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
