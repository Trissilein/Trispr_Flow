# Roadmap - Trispr Flow

Last updated: 2026-03-05 (v0.8.0 hardening + v0.8.1/0.8.2 agent and multimodal track)

This file is the canonical source for priorities and execution order.

## Canonical Current State

- Released: `v0.7.0`
- Current phase: `v0.7.1` stabilization execution
- Foundation complete: Blocks F + G + H
- Active execution blocks: Module Platform hardening (Block L) + Workflow-Agent (Block M foundation)

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
| L | Module Platform + GDD Automation + Confluence Cloud publishing | Extra High | E, F, K | In progress |
| M | Workflow-Agent voice automation for GDD (wakeword -> confirm -> execute) | Extra High | L, F | In progress |
| N | Multimodal I/O modules (screen vision input + TTS voice output) | Extra High | M, L | Planned |

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
| 45 | Refinement quantization profiles + quality/speed recommendation matrix | Deferred (future iteration) |

## AI Direction (Decision Snapshot)

- Primary fallback mode is now offline-first via locally running Ollama.
- Runtime assumption: external Ollama install, local endpoint, model once downloaded then offline-capable.
- Recommended baseline model track: `qwen3.5:4b` primary, `qwen3.5:2b` fast fallback, `qwen3.5:9b` quality profile.
- Cloud provider UX/activation is intentionally postponed to v0.7.3.
- GDD generation is now treated as core workflow capability; autonomous orchestration is handled by `workflow_agent`.
- Multimodal modules (`input_vision`, `output_voice_tts`) are capability modules consumed by `workflow_agent` when enabled.

## Immediate Next Actions

1. Close Block L hardening gate: one-click policy gate + publish conflict/retry coverage + rollout docs.
2. Complete Block M phase M1-M4: core/module semantic split, workflow-agent settings migration, raw command channel, parser/session search.
3. Complete Block M phase M5-M12: plan/confirm/execute, agent console, release hardening for v0.8.1.
4. Start Block N after M execution stability gate (N1-N4): screen vision module + TTS module + agent capability bridge.
5. Run N5 benchmark track (>=3 runs/provider/scenario) and select default TTS provider based on data.
6. Keep Block J and Block G as lower-priority backlog until M/N delivery gates are met.
7. Keep quantization configurability (Task 45) deferred until dedicated benchmark iteration.

## References

- `docs/TASK_SCHEDULE.md`
- `docs/DECISIONS.md`
- `docs/ARCHITECTURE_REVIEW_0.7.md`
- `docs/V0.7.0_ARCHITECTURE.md`
- `docs/INSTALLER_VARIANTS.md`
- `docs/GDD_MODULE_WORKFLOW.md`
- `docs/V0.8.1_WORKFLOW_AGENT_PLAN.md`
- `docs/V0.8.2_MULTIMODAL_IO_PLAN.md`
