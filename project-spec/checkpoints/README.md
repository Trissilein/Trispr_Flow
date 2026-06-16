# Project Checkpoints

Durable handoff notes for work that spans multiple files, docs, or sessions. This index is a map only; each checkpoint file is the source for its own thread.

## Active

- [2026-06-10/ai-refinement-runtime-observations.md](2026-06-10/ai-refinement-runtime-observations.md) - status: active; slug: `ai-refinement-runtime-observations`; date: 2026-06-10; scope: AI Refinement loaded-model, CPU/GPU runtime, and debug-surface issues for local Ollama refinement; next action: switch prompt profile to `LLM Prompt Engineer` and run normal dictation/UI repro with the new gate telemetry visible in the Refinement Inspector.
- [2026-06-10/release-clean-setup-health.md](2026-06-10/release-clean-setup-health.md) - status: active; slug: `release-clean-setup-health`; date: 2026-06-10; scope: v0.8.3 setup-health taxonomy and first module-health classifier slice; next action: decide whether an aggregate release-health command/script is needed beyond `get_module_health`.
- [2026-06-10/vulkan-whisper-fix.md](2026-06-10/vulkan-whisper-fix.md) - status: active; slug: `vulkan-whisper-fix`; date: 2026-06-10; scope: Vulkan Whisper hotfix after latency instrumentation and 2026-06-14 field verification that the live v0.8.3 Vulkan-only installer omitted the hotfix payload; next action: rebuild/replace the release asset or cut v0.8.4 with installer payload validation.
- [2026-06-09/piper-local-custom-followup.md](2026-06-09/piper-local-custom-followup.md) - status: active; slug: `piper-local-custom-followup`; date: 2026-06-09; scope: Piper `local_custom` TTS provider degraded supported-optional follow-up; next action: reproduce the Piper preflight failure and decide packaging/runtime fix.

## Superseded

- [2026-06-09/vulkan-whisper-fix.md](2026-06-09/vulkan-whisper-fix.md) - status: superseded; slug: `vulkan-whisper-fix`; date: 2026-06-09; scope: earlier Vulkan Whisper hotfix state; next action: use [2026-06-10/vulkan-whisper-fix.md](2026-06-10/vulkan-whisper-fix.md).

## Closed

None.

## Blocked

None.
