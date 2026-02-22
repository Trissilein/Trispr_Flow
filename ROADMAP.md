# Roadmap - Trispr Flow

Last updated: 2026-02-22

This file is the canonical source for priorities and execution order.

## Canonical Current State

- Released: `v0.6.0`
- Current phase: `v0.7.0` execution
- Foundation complete: Blocks F + G
- Active execution block: Offline-first AI fallback (Ollama)

## Analysis De-Scope Decision

- Analysis functionality is removed from Trispr Flow mainline.
- `Analyse` remains as a placeholder button in UI.
- Dedicated analysis development moved to `analysis-module-branch`.
- Mainline installer strategy is now CUDA + Vulkan only.

## Active Work Blocks

| Block | Focus | Complexity | Depends on | Status |
| --- | --- | --- | --- | --- |
| D | v0.7.2 Offline-first AI fallback (Ollama provider + pipeline + UX) | High | F, G | In progress |
| E | UX/UI consistency and settings IA cleanup | Medium | D | Planned |
| F | Reliability hardening and release QA | High | D, E | Planned |
| G | Cloud provider rollout (OpenAI/Claude/Gemini) | High | D, F | Deferred to v0.7.3 |

## v0.7 Task Ledger

| Task | Title | State |
| --- | --- | --- |
| 31 | Multi-provider architecture | Done |
| 36 | Settings migration + data model | Done |
| 37 | Provider config UI scaffolding | Done |
| 32 | Ollama provider integration (backend) | Open |
| 33 | Activate AI refinement pipeline stage for local provider | Open |
| 34 | Ollama-only UI (endpoint, model refresh, connection test) | Open |
| 35 | Prompt strategy polish for local models (DE/EN) | Open |
| 38 | Offline E2E + fail-safe regression tests | Open |

## AI Direction (Decision Snapshot)

- Primary fallback mode is now offline-first via locally running Ollama.
- Runtime assumption: external Ollama install, local endpoint, model once downloaded then offline-capable.
- Recommended baseline model track: `qwen3:14b` primary, `qwen3:8b` fast fallback, optional `mistral-small3.1:24b` quality profile.
- Cloud provider UX/activation is intentionally postponed to v0.7.3.

## Immediate Next Actions

1. Implement Task 32 (Ollama backend provider and model discovery).
2. Implement Task 33 (real pipeline activation with safe fallback behavior).
3. Implement Task 34 (Ollama-only settings UX and connection flow).
4. Complete Task 35 and Task 38 for local-model quality and reliability.
5. Continue UX consistency passes for Capture/System settings panels.

## References

- `docs/TASK_SCHEDULE.md`
- `docs/DECISIONS.md`
- `docs/V0.7.0_ARCHITECTURE.md`
- `docs/INSTALLER_VARIANTS.md`
