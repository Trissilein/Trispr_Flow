# Vulkan Whisper Fix Checkpoint

Status: active
Created: 2026-06-10
Updated: 2026-06-10
Slug: vulkan-whisper-fix
Supersedes: ../2026-06-09/vulkan-whisper-fix.md

## Scope

This checkpoint covers the current v0.8.3 Vulkan Whisper hotfix state after adding latency instrumentation, replacing the crashing Vulkan runtime payload, and producing classified warm-server benchmark evidence.

It does not cover Piper/TTS follow-up work, the Vite vocabulary import warning, unrelated release docs, or performance optimization beyond identifying the next runtime blocker.

## Sources Read

- [project-spec/checkpoints/README.md](../README.md)
- [project-spec/checkpoints/2026-06-09/vulkan-whisper-fix.md](../2026-06-09/vulkan-whisper-fix.md)
- [CONTEXT.md](../../../CONTEXT.md)
- [scripts/latency-benchmark.ps1](../../../scripts/latency-benchmark.ps1)
- [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs)
- [src-tauri/src/tts_benchmark.rs](../../../src-tauri/src/tts_benchmark.rs)
- [src-tauri/src/whisper_server.rs](../../../src-tauri/src/whisper_server.rs)
- [src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs)
- Local ignored benchmark evidence: `bench/results/latest.json`
- Local session command evidence: Rust lib tests, direct CLI smoke, warm-server smoke, 30-run latency benchmark, LF scan.

## Current Facts

- The active release gate remains the v0.8.3 Vulkan Whisper hotfix. Source: [CONTEXT.md](../../../CONTEXT.md).
- The Whisper Backend is local ASR, with `whisper-server` preferred and `whisper-cli` fallback; CUDA and Vulkan builds are selected through backend preference. Source: [CONTEXT.md](../../../CONTEXT.md).
- Latency benchmark samples now carry explicit execution-path fields: `whisper_path`, `backend`, pinned language state, model/runtime paths and drives, ping timing, server/CLI timing, and pipeline overhead. Source: [src-tauri/src/tts_benchmark.rs](../../../src-tauri/src/tts_benchmark.rs).
- Latency benchmark results now include `classification_pass`, cold server startup target fields, and `whisper_path_summary`; `slo_pass` now requires both latency thresholds and classification success. Source: [src-tauri/src/tts_benchmark.rs](../../../src-tauri/src/tts_benchmark.rs).
- Transcription now records a last-run timing summary for successful `server_warm`, `cli_gpu`, and `cli_cpu` paths. Source: [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
- `whisper-server` cold startup duration is recorded when startup succeeds or times out. Source: [src-tauri/src/whisper_server.rs](../../../src-tauri/src/whisper_server.rs).
- The latency benchmark app hook writes `bench/results/latest-error.txt` when benchmark execution fails before `latest.json` can be written. Source: [src-tauri/src/lib.rs](../../../src-tauri/src/lib.rs) and [src-tauri/src/tts_benchmark.rs](../../../src-tauri/src/tts_benchmark.rs).
- `scripts/latency-benchmark.ps1 -TauriVariant vulkan` now forces `TRISPR_LOCAL_BACKEND=vulkan`, prints the selected local backend, restores any previous value in `finally`, and surfaces `latest-error.txt` when no report exists. Source: [scripts/latency-benchmark.ps1](../../../scripts/latency-benchmark.ps1).
- Transcription backend ordering now honors `TRISPR_LOCAL_BACKEND` for CLI attempts. Source: [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
- The crashing March 2026 Vulkan payload was replaced from the local `E:\code\ingo\whisper.cpp\build-vulkan\bin\Release` build output. The copied payload includes `whisper-cli.exe`, `whisper-server.exe`, `whisper.dll`, `ggml.dll`, `ggml-base.dll`, `ggml-cpu.dll`, and `ggml-vulkan.dll`. Source: local session command evidence.
- Direct CLI fixture transcription now succeeds against `bench/fixtures/short/short_en_like.wav` with `ggml-large-v3-turbo.bin`. Source: local session command evidence.
- `scripts/validate-whisper-runtime.mjs --variant vulkan` now requires `whisper-server.exe` for the Vulkan variant, and `src-tauri/tauri.conf.json` bundles it. Source: [scripts/validate-whisper-runtime.mjs](../../../scripts/validate-whisper-runtime.mjs) and [src-tauri/tauri.conf.json](../../../src-tauri/tauri.conf.json).
- Shared runtime path resolution now lets `TRISPR_LOCAL_BACKEND` override persisted `auto`, so benchmark-forced Vulkan applies to both CLI and server paths. Source: [src-tauri/src/paths.rs](../../../src-tauri/src/paths.rs).
- `whisper-server` root ping treats any HTTP status response as reachable, so a non-2xx root route no longer forces a false startup timeout. Source: [src-tauri/src/whisper_server.rs](../../../src-tauri/src/whisper_server.rs).
- Final benchmark evidence is warm-server Vulkan: `Warmup=3`, `Runs=30`, `NoRefinement`, `TauriVariant=vulkan`, `p50=624ms`, `p95=628ms`, `slo_pass=true`, `classification_pass=true`, `cold_server_start_ms=2293`, `cold_server_start_target_pass=true`, `whisper_path_summary=server_warm:30`. Source: local ignored `bench/results/latest.json`.
- The persisted production model path on this machine is `C:\Users\hendr\AppData\Local\Trispr Flow\models\ggml-large-v3-turbo.bin`. Source: local settings inspection command evidence.

## Decisions

- Keep warm and cold latency evidence distinct in benchmark output. ADR: none; encoded in [src-tauri/src/tts_benchmark.rs](../../../src-tauri/src/tts_benchmark.rs).
- Treat unknown or missing Whisper execution-path classification as release-gate failure for latency evidence. ADR: none; encoded in [src-tauri/src/tts_benchmark.rs](../../../src-tauri/src/tts_benchmark.rs).
- Treat `TRISPR_LOCAL_BACKEND` as the authoritative benchmark/runtime override over persisted `auto`. ADR: none; this is a test and runtime-selection correctness fix.

## Progress

- Added benchmark JSON fields for Whisper path classification, language mode, model/runtime path and drive, warm/cold server timings, CLI timings, and overhead.
- Added benchmark-level classification pass/fail and path summary.
- Added cold-start timing storage for `whisper-server`.
- Added benchmark failure sidecar output and script surfacing for failures before `latest.json` exists.
- Fixed benchmark variant forcing so `-TauriVariant vulkan` actually drives Vulkan runtime selection instead of stored `auto` trying CUDA first.
- Fixed shared runtime path resolution so that forced backend selection applies to `whisper-server` as well as CLI.
- Replaced the crashing Vulkan runtime payload with a fresh local `whisper.cpp` Vulkan build.
- Added `whisper-server.exe` to Vulkan runtime validation and Tauri bundle resources.
- Fixed `whisper-server` ping readiness to accept HTTP status responses from the dedicated server port.
- Improved CLI failure diagnostics with exit code and both output streams.
- Ran `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 252 tests passed.
- Ran direct Vulkan CLI fixture transcription: exit 0 and output text produced.
- Ran warm-server benchmark smoke: `Warmup=3`, `Runs=3`, `p50=614ms`, `p95=617ms`, `slo_pass=true`, `server_warm:3`.
- Ran final 30-run benchmark: `p50=624ms`, `p95=628ms`, `slo_pass=true`, `classification_pass=true`, `server_warm:30`.
- Verified edited tracked files have LF line endings.

## Stale Or Conflicting Context

- [project-spec/checkpoints/2026-06-09/vulkan-whisper-fix.md](../2026-06-09/vulkan-whisper-fix.md) said a fresh local Vulkan build could run direct fixture transcription and a short app benchmark without crashing. That may still describe a prior ignored payload, but it conflicts with the current checked local runtime state and is superseded by this checkpoint.
- Release docs and reports named by the older checkpoint may still describe pre-repair failure evidence. Current local evidence now supports release-doc/report refresh.
- `bench/results/latest.json` is ignored local evidence, not a tracked release artifact. It should be regenerated if runtime payload or benchmark logic changes.

## Open Questions

- Should CPU fallback use a true CPU-only binary rather than reusing the Vulkan binary when Vulkan is the forced backend?
- Should release automation rebuild the Vulkan payload locally, hydrate from a release artifact, or store a verified runtime payload through another controlled mechanism?

## Next Actions

1. Refresh release docs/reports with the final 30-run Vulkan warm-server evidence.
2. Decide how release automation should source the verified Vulkan runtime payload.
3. Run the strict release gate after docs/report refresh.
4. Optionally split CPU fallback behavior into a follow-up if a true CPU-only fallback binary is needed.

## Kickoff Prompt

Use checkpoint-kickoff. Continue from `project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md`: refresh release docs/reports from the passing Vulkan warm-server benchmark evidence and decide the durable release-automation source for the verified Vulkan runtime payload. Use `/grill-with-docs` for release-policy, checkpoint, stale-doc, or cross-file design uncertainty; use `/grill-me` for pure implementation/debug uncertainty. Fix but do not commit.

## Verification

Passed:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 252 passed.
- `node scripts/validate-whisper-runtime.mjs --variant vulkan`: passed.
- Direct `src-tauri/bin/vulkan/whisper-cli.exe` fixture transcription with production model: exit 0 and output text produced.
- `scripts/latency-benchmark.ps1 -Warmup 3 -Runs 3 -NoRefinement -TauriVariant vulkan`: `p50=614ms`, `p95=617ms`, `server_warm:3`, `slo_pass=true`.
- `scripts/latency-benchmark.ps1 -Warmup 3 -Runs 30 -NoRefinement -TauriVariant vulkan`: `p50=624ms`, `p95=628ms`, `server_warm:30`, `slo_pass=true`, `classification_pass=true`, `cold_server_start_ms=2293`.
- Editor diagnostics on touched files: no errors.
- LF scan for edited tracked files: no CRLF detected.

Failed:

- Earlier pre-repair Vulkan payload failed direct CLI and benchmark with exit `-1073741795`.
- Earlier pre-server-fix benchmark produced `cli_gpu:30`, `p50=4828ms`, `p95=4885ms`, `slo_pass=false`.

Not checked:

- Strict release gate after release docs/report refresh.
