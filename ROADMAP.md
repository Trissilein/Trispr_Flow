# Roadmap - Trispr Flow

Last updated: 2026-02-26 (v0.7.1 stabilization pass)

This file is the canonical source for priorities and execution order.

## Canonical Current State

- Released: `v0.7.0`
- Current phase: `v0.7.1` stabilization execution
- Foundation complete: Blocks F + G + H
- Active execution block: UX/UI consistency (Block E)

## Analysis De-Scope Decision

- Analysis functionality is removed from Trispr Flow mainline.
- `Analyse` remains as a placeholder button in UI.
- Dedicated analysis development moved to `analysis-module-branch`.
- Mainline installer strategy is now CUDA + Vulkan only.

## Active Work Blocks

| Block | Focus | Complexity | Depends on | Status |
| --- | --- | --- | --- | --- |
| D | v0.7.2 Offline-first AI fallback (Ollama provider + pipeline + UX) | High | F, G | Complete ✅ |
| E | UX/UI consistency and settings IA cleanup | Medium | D | Planned |
| F | Reliability hardening and release QA | High | D, E | Planned |
| G | Cloud provider rollout (OpenAI/Claude/Gemini) | High | D, F | Deferred to v0.7.3 |
| J | Adaptive AI refinement intelligence (VRAM indicator + self-learning vocabulary) | Medium | D | Planned |
| K | Expert Mode UX Overhaul (standard/expert toggle, hide technical settings) | Medium | E | Planned |

## v0.7 Task Ledger

| Task | Title | State |
| --- | --- | --- |
| 31 | Multi-provider architecture | Done |
| 36 | Settings migration + data model | Done |
| 37 | Provider config UI scaffolding | Done |
| 32 | Ollama provider integration (backend) | Done |
| 33 | Activate AI refinement pipeline stage for local provider | Done |
| 34 | Ollama-only UI (endpoint, model refresh, connection test) | Done |
| 35 | Prompt strategy polish for local models (DE/EN) | Done |
| 38 | Offline E2E + fail-safe regression tests | Done |
| 43 | GPU VRAM detection (Tauri backend) | Planned |
| 43a | VRAM requirement display in AI Fallback UI | Planned |
| 44 | Word-diff extraction from refinement events | Planned |
| 44a | Persistence and threshold logic for learned vocabulary | Planned |
| 44b | Learned vocabulary settings UI | Planned |
| 44c | Adaptive vocabulary regression tests | Planned |

## AI Direction (Decision Snapshot)

- Primary fallback mode is now offline-first via locally running Ollama.
- Runtime assumption: external Ollama install, local endpoint, model once downloaded then offline-capable.
- Recommended baseline model track: `qwen3:14b` primary, `qwen3:8b` fast fallback, optional `mistral-small3.1:24b` quality profile.
- Cloud provider UX/activation is intentionally postponed to v0.7.3.

## Immediate Next Actions

1. Finalize latency benchmark baseline (`benchmark:latency`) and record p50/p95 trend.
2. Validate runtime-start background behavior on Windows (no permanent "Starting runtime..." state).
3. Complete reliability hardening and release QA for v0.7.1.
4. Continue Block E: UX/UI consistency and settings IA cleanup.
5. Keep Block J and Block G (v0.7.3) out of stabilization scope.

## References

- `docs/TASK_SCHEDULE.md`
- `docs/DECISIONS.md`
- `docs/ARCHITECTURE_REVIEW_0.7.md`
- `docs/V0.7.0_ARCHITECTURE.md`
- `docs/INSTALLER_VARIANTS.md`
