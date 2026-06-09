# Piper Local Custom Followup Checkpoint

Status: active
Created: 2026-06-09
Updated: 2026-06-09
Slug: piper-local-custom-followup
Supersedes: none

## Scope

This checkpoint covers the Piper `local_custom` TTS provider follow-up after the v0.8.2 release-gate provider-role change.

It does not cover Vulkan Whisper latency work, the Vite warning cleanup, or general Voice Output redesign. `local_custom` is not a `v0.8.2` release blocker while `windows_native` remains healthy.

## Sources Read

- [CONTEXT.md](../../../CONTEXT.md)
- [STATUS.md](../../../STATUS.md)
- [ROADMAP.md](../../../ROADMAP.md)
- [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md)
- [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1)
- Local ignored benchmark evidence from `bench/results/tts.latest.json`
- [project-spec/checkpoints/README.md](../README.md)

## Current Facts

- Voice Output TTS is an optional Feature Module. Piper provider/voice selection and runtime behavior belong to Voice Output, not Trispr Core. Source: [CONTEXT.md](../../../CONTEXT.md).
- For the v0.8.2 release gate, `windows_native` is the baseline TTS provider, `local_custom` is supported optional, and `qwen3_tts` is experimental. Source: [CONTEXT.md](../../../CONTEXT.md) and [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- A degraded supported-optional provider passes the release gate with a warning if the baseline provider passes. Source: [CONTEXT.md](../../../CONTEXT.md) and [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- The latest gate report marks TTS release gate pass as yes, with degraded optional provider `local_custom`, unavailable experimental provider `qwen3_tts`, and zero uncategorized TTS failures. Source: [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md).
- Local ignored TTS evidence says `release_gate_pass=true`, `release_gate_reason="Baseline providers passed; supported optional providers degraded: local_custom"`, baseline provider `windows_native`, degraded supported optional provider `local_custom`, unavailable experimental provider `qwen3_tts`, and recommended default provider `windows_native`. Source: local `bench/results/tts.latest.json` summary read during this checkpoint.
- Local ignored TTS evidence says the `local_custom` provider has `release_role=supported_optional`, `evaluated_for_release=false`, `blocks_release_gate=false`, `passes_release_gate=false`, `preflight_ok=false`, `runtime_smoke_ok=false`, and `success_rate=0`. Source: local `bench/results/tts.latest.json` provider evaluation summary.
- Historical status notes say a prior Block N `N11` run recommended `windows_native` and `local_custom` failed in that environment because the Piper binary was missing. Source: [STATUS.md](../../../STATUS.md).
- The current TTS benchmark script resolves Piper by optional configured paths first, then `piper.exe` / `piper` on `PATH`, then repo/runtime candidates including `src-tauri/bin/piper/piper.exe`, `bin/piper/piper.exe`, `D:\GIT\piper\piper.exe`, `D:\GIT\piper\build\piper.exe`, and `%LOCALAPPDATA%\trispr-flow\piper\piper.exe`. Source: [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- The current TTS benchmark script resolves Piper models from an explicit configured file, `src-tauri/bin/piper/voices`, `piper/voices`, `piper/models`, `D:\GIT\piper\voices`, `D:\GIT\piper\models`, and `%LOCALAPPDATA%\trispr-flow\piper\voices`. Source: [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- The current Piper synthesis command uses `piper --model "<model>" --output_file "<wav>" --length_scale <scale>`, sends text on stdin, waits for exit, and treats nonzero exit as failure with stderr/stdout detail. Source: [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).

## Decisions

- `local_custom` Piper remains supported and should be fixed, but it is not release-critical for `v0.8.2` while `windows_native` passes. ADR: none; this release policy is recorded in [CONTEXT.md](../../../CONTEXT.md), [STATUS.md](../../../STATUS.md), [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md), and [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- Piper `local_custom` should be tracked as follow-up/backlog work, not treated as a release blocker. ADR: none; user confirmed this direction on 2026-06-09.

## Progress

- TTS release-gate provider roles are implemented in [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- Gate reporting surfaces degraded supported-optional providers instead of hiding them.
- `local_custom` is currently visible as degraded optional evidence in the latest gate report and local ignored TTS report.
- No Piper runtime fix has been implemented in this checkpoint.
- No GitHub issue or tracker item was created in this checkpoint.

## Stale Or Conflicting Context

- No material stale canonical doc was found in the audited surface. `CONTEXT.md`, `STATUS.md`, the latest gate report, and the benchmark script all agree that `local_custom` is degraded supported optional and non-blocking for v0.8.2.
- The exact current Piper failure category from `bench/results/tts.latest.json` was not expanded beyond preflight/runtime-smoke failure summary during this checkpoint. The next agent should inspect provider preflight details before choosing a fix.

## Open Questions

- Is the current `local_custom` failure still missing Piper binary, missing model, synthesis process failure, or a packaging/path mismatch?
- Should the follow-up be tracked as a GitHub issue, roadmap item, or dedicated Block after the Vulkan/tag path is settled?
- Should Piper runtime assets be bundled, hydrated from release artifacts, user-installed, or managed through the Voice Output module setup flow?
- Should `local_custom` remain named `local_custom`, or should the user-facing provider name be made Piper-specific once the runtime is real?

## Next Actions

1. Inspect `bench/results/tts.latest.json` provider preflight details for `local_custom`, especially `binary_available`, `model_available`, and `synthesis_probe`.
2. Reproduce directly with `scripts/tts-benchmark.ps1 -Providers local_custom` and, if needed, explicit `-PiperBinaryPath` and `-PiperModelPath`.
3. Verify whether expected runtime assets exist under `src-tauri/bin/piper/` and whether they are gitignored, hydrated, bundled, or absent.
4. Run `piper.exe --help` and a direct one-line stdin synthesis command outside the benchmark script to separate process/runtime failure from benchmark harness failure.
5. Decide packaging/setup path: bundle Piper assets, hydrate them from release artifacts, or expose a setup/install action in the Voice Output module.
6. Create the actual backlog/issue item once failure category and desired packaging path are known.
7. Rerun `npm run benchmark:tts` after any fix and confirm `local_custom` moves out of `degraded_supported_optional_providers`.

## Verification

- Read current provider-role policy in [CONTEXT.md](../../../CONTEXT.md).
- Read current release status and historical Piper note in [STATUS.md](../../../STATUS.md).
- Read latest gate report in [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md).
- Read relevant provider role, Piper binary/model resolution, synthesis invocation, preflight, and release-gate evaluation logic in [scripts/tts-benchmark.ps1](../../../scripts/tts-benchmark.ps1).
- Read local ignored `bench/results/tts.latest.json` summary for release gate and `local_custom` provider evaluation state.

Not checked in this checkpoint:

- Full local TTS benchmark rerun.
- Direct Piper command reproduction.
- Actual Piper binary/model file existence.
- GitHub issue creation.