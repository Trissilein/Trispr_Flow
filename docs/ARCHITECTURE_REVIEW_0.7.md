# Architecture Review - v0.7.0 Baseline

Last updated: 2026-02-26  
Status: Kickoff (active)

## Goal

Validate the `v0.7.0` architecture for:

- Runtime reliability on Windows (no UI hangs during runtime start/install/refinement)
- Deterministic transcript pipeline (`raw -> refine -> paste`)
- Persistency guarantees (history + refinement state survive restart)
- Performance headroom (short utterances target significantly below current latency)

## Scope

- `src-tauri/src/audio.rs`
- `src-tauri/src/transcription.rs`
- `src-tauri/src/ollama_runtime.rs`
- `src-tauri/src/state.rs`
- `src/main.ts`
- `src/ollama-models.ts`
- `src/history.ts`
- `src/refinement-inspector.ts`

## Review Checklist

1. Pipeline ownership and boundaries
2. Concurrency model (threads, locks, event fan-out, watchdogs)
3. Failure semantics (timeouts, fallback, retry, idempotency)
4. Persistency model (single source of truth vs UI cache)
5. Startup/autostart behavior and stale-state handling
6. Installer/runtime compatibility assumptions (CUDA/Vulkan, DLLs, PATH)
7. Observability (logs, metrics, user-visible status)
8. Performance bottlenecks and tuning levers

## Initial Findings (Seed)

1. Refinement state previously lived mostly in UI localStorage; now persisted in backend history entries.
2. Runtime start UI could remain in stale "Starting runtime..." state; mitigated via health-aware busy-state handling.
3. Clipboard/paste flow is serialized in frontend queue, but throughput under rapid bursts still needs benchmark data.
4. End-to-end latency budget is not yet enforced by measurable SLO in CI.

## Decisions for v0.7.0 Stabilization

1. Freeze scope: no major feature additions before hardening sign-off.
2. Use backend history persistence as authoritative refinement state.
3. Keep cloud provider rollout out of stabilization scope (`v0.7.3`).

## Open Questions

1. What is the accepted p50/p95 latency target for short utterances (ms)?
2. Should refined text replace `HistoryEntry.text`, or remain in dedicated refinement fields only?
3. Do we require structured telemetry counters for runtime start/refinement timeout/fallback events?

## Action Items

1. Add latency benchmark script for mic short-utterance path (p50/p95).
2. Add integration test for restart persistence of raw+refined+error states.
3. Audit lock-hold durations in history update paths under burst load.
4. Define release SLO gate for `v0.7.0` sign-off.
