# Roadmap (Sketch)

This is a compact view of priorities. The canonical detailed plan is `ROADMAP.md` and `docs/TASK_SCHEDULE.md`.

## Now
- Execute `Block T` (`T2-T5`: assistant state machine, mode UX, graceful degradation, assistant events).
- Keep regression baseline green (`npm run build`, `npm test`, `cargo test --lib`) while integrating `T`.

## Next
- `Block T`: Assistant pivot foundation (`transcribe` vs `assistant`, state machine, graceful degradation).
- `Block V`: GDD copilot loop (`conversation -> suggestions -> draft`) with strict plan/execute separation.

## Then
- `Block O`: Voice confirmation loop (`awaiting_confirmation`, token+TTL, confirm/cancel intents).
- `Block P`: Hands-free execution (`focus/inject`, voice-plan-confirm-action E2E).

## Guardrails
- Hybrid activation model (mode switch + wakeword in assistant mode).
- Plan+Confirm for side-effect actions.
- Local-first LLM strategy.
- GDD copilot before generalized assistant expansion.
