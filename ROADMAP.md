# Roadmap - Trispr Flow

Last updated: 2026-04-07 (Vocabulary learning (44a/44b) complete; Keep-Alive 20m→60m; UI redesign: Custom Vocabulary flat + compact)

This file is the canonical source for priorities and execution order.

## Canonical Current State

- Released: `v0.7.0`, `v0.7.1`, `v0.7.2`
- Current phase: `v0.8.x` assistant hardening with `Block T` + `Block V` completed and `Block U` active.
- **Next version bump**: `v0.8.x` after `Block U` soak/release gates are green.
- Foundation complete: Blocks F + G + H + L + M
- Active execution blocks: Block U (assistant UX + soak + release gate)

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
| L | Module Platform + GDD Automation + Confluence Cloud publishing | Extra High | E, F, K | Complete ✅ |
| M | Workflow-Agent voice automation for GDD (wakeword -> confirm -> execute) | Extra High | L, F | Complete ✅ |
| N | Multimodal I/O modules (screen vision input + TTS voice output) | Extra High | M, L | Foundations complete ✅ (`N1-N12` + benchmark track) |
| Q | Onboarding refinement and startup stability | Medium | D | Complete ✅ |
| R | Local AI provider hardening (Input Truncation + LM Studio integration) | Low | Q, D | Planned / partial ✅ |
| S | Build Recovery + Module Decoupling (`v0.7.3`) | High | N, Q | Complete ✅ (`S1-S13.5`) |
| T | Assistant Pivot Foundation (`v0.8.0`) | Extra High | S, M | Complete ✅ (`T1-T5`) |
| V | GDD Copilot Loop (`v0.8.x`) | Extra High | T, L, M, N | Complete ✅ (`V1-V6`) |
| U | Assistant UX + Soak + Release Gate (`v0.8.x`) | Extra High | T, V | Active ♻️ |

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
| 43a | VRAM requirement display in AI Fallback UI | Partial ✅ (Gemma4 variant cards show size_gb + vram_gb; backend VRAM probe not yet exposed) |
| 44 | Word-diff extraction from refinement events | Done ✅ |
| 44a | Persistence and threshold logic for learned vocabulary | Done ✅ |
| 44b | Learned vocabulary settings UI + suggestion review dialog | Done ✅ |
| 44c | Adaptive vocabulary regression tests | Planned |
| 44d | Screen Recording ground-truth path (capture before Enter → OCR → diff → learn) | Future / depends on Screen Recording module |
| 45 | Refinement quantization profiles + quality/speed recommendation matrix | Partial ✅ (Gemma4 Q4/Q8/BF16 variants with VRAM labels added to model picker) |

## AI Direction (Decision Snapshot)

- Primary fallback mode is now offline-first via locally running Ollama.
- Runtime assumption: external Ollama install, local endpoint, model once downloaded then offline-capable.
- Recommended baseline model track:
  - **Qwen track**: `qwen3.5:4b` primary, `qwen3.5:2b` fast fallback, `qwen3.5:9b` quality profile.
  - **Gemma4 track** (added 2026-04-06): `gemma4:e4b` standard (Q4 ~5 GB VRAM), `gemma4:e4b-it-q8_0` high-quality (~7.5 GB), `gemma4:e4b-it-bf16` maximum (~15 GB). Gemma4 requires explicit anglicism-preservation prompt addons — handled automatically by `resolveEffectiveRefinementPrompt()`.
- Model-family prompt adaptation is live: `detectModelFamily()` prefix heuristic routes Gemma/Qwen/generic to appropriate system-prompt variants.
- Cloud provider UX/activation is intentionally postponed to v0.7.3.
- GDD generation is now treated as core workflow capability; autonomous orchestration is handled by `workflow_agent`.
- Multimodal modules (`input_vision`, `output_voice_tts`) are capability modules consumed by `workflow_agent` when enabled.

## Agent Evolution Guardrails

- Activation model: `Hybrid` (explicit `product_mode` switch + wakeword while in assistant mode).
- Autonomy model: `Plan+Confirm` for all actions with side effects.
- Product priority: `GDD copilot` first, then generalized assistant expansion.
- LLM policy: `Local-first`; cloud remains optional fallback, not required baseline.
- Knowledge baseline: transcript + archive remain primary v1 context source.

## Phased Path to Full Agent

**Phase 0 — Stabiler Unterbau (`S13.5`)**  
Close TTS provider-matrix/device-routing/forced-failure validation and unify runtime diagnostics.  
Exit: deterministic behavior for disabled/unavailable modules.

**Phase 1 — Assistant Pivot Foundation (`Block T`, `v0.8.0`)**  
Finalize assistant state machine + `transcribe|assistant` mode UX + graceful degradation policy.  
Exit: stable assistant mode without transcribe regression.

**Phase 2 — GDD Copilot Loop (`Block V`, `v0.8.x`)**  
Evolve workflow-agent from command trigger to copilot loop (conversation -> clustered session -> archive context -> suggestions -> GDD draft).  
Exit: reproducible E2E `conversation -> draft -> review` path.

**Phase 3 — Voice Confirmation Loop (`Block O`)**  
Implement `awaiting_confirmation` with TTL/token model and wakeword confirm/cancel intents plus TTS prompt timeout.  
Exit: safe voice approvals without accidental execution.

**Phase 4 — Hands-free Actions (`Block P`)**  
Add focus/inject command surface and E2E voice-plan-confirm-window-action path.  
Exit: reliable keyboard-free workflow for defined target apps.

**Phase 5 — Vollwertiger Mitarbeiter-Modus (continuous hardening)**  
Iterate proactive suggestions, role/skill profiles, and policy-bounded autonomy with measurable productivity gains.

## Block Q Task Details

| Task | Title | Depends on | Status |
| --- | --- | --- | --- |
| Q1 | Startup Freeze: `save_settings` sync revert — remove `spawn_blocking`, `refresh_runtime_diagnostics` stays on detached thread | — | Done ✅ |
| Q2 | Ghost Overlay: bootstrap position `(12,12)` → off-screen `(-9999,-9999)`, defensive repositioning in `apply_overlay_state_to_window` | — | Done ✅ |
| Q3 | Refinement Resilience: `catch_unwind` + concurrency gate (max 2 active) + watchdog 90s → 45s | Q1 | Done ✅ (catch_unwind via Crash-Proof Shell P1-P4; concurrency gate via MAX_CONCURRENT_REFINEMENTS=2 in audio.rs:1266; watchdog at 45s in audio.rs:31) |

## Crash-Proof Shell (2026-03-17)

| Phase | Title | Status |
| --- | --- | --- |
| P1 | Global `panic::set_hook` — all panics logged via tracing | Done ✅ |
| P2 | All 31 `thread::spawn` → `spawn_guarded` (catch_unwind wrapper) | Done ✅ |
| P3 | All remaining `.lock().unwrap()` → `.unwrap_or_else(\|p\| p.into_inner())` — zero poison-cascade risk | Done ✅ |
| P4 | 26 module Tauri commands wrapped with `guarded_command!` macro | Done ✅ |
| P5 | Overlay failure guard evolved to bounded supervisor retries + cooldown (no permanent session lockout) | Done ✅ |
| P6 | `register_hotkeys` moved to background thread (cross-thread deadlock with Windows event loop) | Done ✅ |
| P7 | `save_settings` IPC calls: 3 s timeout via `Promise.race` (prevents frontend freeze if backend blocks) | Done ✅ |
| P8 | Event-driven Ollama init: wait for `ollama:runtime-health` instead of immediate ping-storm at startup | Done ✅ |
| P9 | Rust-side cold-start buffer: 10 s OLLAMA_DIAG_NEXT_MS suppresses all Ollama pings during bootstrap | Done ✅ |

## LM Studio Integration (2026-03-17)

| Phase | Title | Status |
| --- | --- | --- |
| LS1 | Daemon lifecycle: `lms daemon up` on switch-to, `lms daemon stop` on switch-away | Done ✅ |
| LS2 | `max_tokens` in OpenAI-compatible request body (LM Studio + Oobabooga) — caps reasoning model output | Done ✅ |

## Block R Task Details

| Task | Title | Depends on | Note |
| --- | --- | --- | --- |
| R1 | Input truncation for local providers (Ollama, LM Studio): max 2000 words, sentence-boundary cutoff, `[truncated]` suffix | Q1 | ✅ Done — both providers truncate at 2000 words with sentence-boundary fallback |
| R2 | LM Studio auto-start: ping endpoint on app start → `lms daemon up` + model load if unreachable | — | ✅ Done — startup ping-check in setup block; mirrors provider-switch lifecycle (LS1) |
| R3 | LM Studio reasoning-model detection: UI warning when model name matches CoT pattern (DeepSeek-R1, QwQ, etc.) — recommend instruct model | R2 | ✅ Done — `isReasoningModel()` in settings.ts; amber warning in model card |
| R4 | **Refinement Model Keep-Alive** (future): Similar to Whisper pre-warm, periodically ping refinement model to prevent cold-start latency. First inference call ~2-5s slower than follow-up calls. Whisper already keeps hot; refinement should too. | Q3 | Low priority; first-call latency acceptable for now. Consider `keep_alive` parameter in Ollama/LM Studio requests. |
| R5 | **Model Picker UX Unification**: Ollama and LM Studio use different UI patterns for model selection — LM Studio models shown inside provider card, Ollama shows them in a separate model section below. Same models, same UX. Both should use the bottom model-category section. | E | Medium priority; UX consistency issue. |
| R6 | **LM Studio Thinking Disable** (future): `chat_template_kwargs` in request body is ignored by llmster (llama.cpp backend). True disable requires `model.yaml` with `enable_thinking: false`. Investigate: automated `model.yaml` provisioning on first `lms load`, or LM Studio Config Preset API. | R2 | Blocked on llmster API limitation. Track LM Studio changelog for per-request thinking control. |

## Block S Task Details (Current Window)

| Task | Title | Depends on | Status |
| --- | --- | --- | --- |
| S6 | AI Refinement as optional module (`ai_refinement`) | S3-S5 | Done ✅ |
| S7 | AI Refinement runtime capability gate + disable side-effects | S6 | Done ✅ |
| S8 | Frontend tab gating + effective refinement state (`module && setting`) | S6, S7 | Done ✅ |
| S9 | Regression/docs closure + handoff to TTS free-config/testing | S6-S8 | Done ✅ |
| S10 | Strict module-UX decoupling + dedicated TTS main tab (`voice-output`) | S9 | Done ✅ |
| S11 | AI-Refinement re-enable speed path (autostart + warmup + runtime-ready defer policy) | S10 | Done ✅ |
| S12 | Overlay deep refactor (bounded recovery supervisor, off-screen fallback, pulse reliability, recovered health signal) | S10 | Done ✅ |
| S13 | Regression + soak/manual gate (`50 cycles + 10 restarts`) and closure handoff to TTS free-config/testing | S10-S12 | Done ✅ (manual soak validated) |

## Immediate Next Actions

1. **Block U U2/U3 Soak Runs (deferred to end)**: attach 8h + 24h soak evidence after U1/U4 prep is complete.
2. **Block U Gate Closure**: run strict assistant gate with benchmark + soak evidence and verify mode-safety/degraded-capability/publish safety.
3. **Release Readiness Bundle**: consolidate evidence (`build`, `tests`, benchmark links, soak logs, manual validation checklist).
4. **Plan O/P follow-up** only after U gates are closed, keeping plan/confirm guarantees intact.

## References

- `docs/TASK_SCHEDULE.md`
- `docs/DECISIONS.md`
- `docs/AGENT_EVOLUTION_ROADMAP.md`
- `docs/ARCHITECTURE_REVIEW_0.7.md`
- `docs/V0.7.0_ARCHITECTURE.md`
- `docs/INSTALLER_VARIANTS.md`
- `docs/GDD_MODULE_WORKFLOW.md`
- `docs/V0.8.0_BLOCK_L_ROLLOUT_PACKET.md`
- `docs/V0.8.1_WORKFLOW_AGENT_PLAN.md`
- `docs/V0.8.2_MULTIMODAL_IO_PLAN.md`
- `docs/N11_TTS_BENCHMARK.md`
