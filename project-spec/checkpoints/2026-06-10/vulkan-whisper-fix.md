# Vulkan Whisper Fix Checkpoint

Status: active
Created: 2026-06-10
Updated: 2026-06-14
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
- Local field verification on 2026-06-14: installed `v0.8.3` Vulkan-only release payload hash check, direct installed CLI smoke, local repo hotfix CLI smoke, installed hotfix CLI smoke, installed hotfix server startup, and `http://127.0.0.1:8178/` ping.

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
- Field verification on 2026-06-14 showed the published `TrsprFlw.v0.8.3.vulkan-only-12.06.-12.09.exe` installer did not contain the verified hotfix Vulkan payload. The installed folder `C:\Users\hendr\AppData\Local\Programs\Trispr Flow\bin\vulkan` contained February 2026 binaries, missed `whisper-server.exe`, had non-matching hashes for `whisper-cli.exe` and `ggml-vulkan.dll`, and reproduced `whisper-cli.exe` exit `-1073741795` on AMD Radeon RX 6800 XT.
- The repo-local hotfix payload in `src-tauri/bin/vulkan` matched `bench/results/trispr-flow-v0.8.3-vulkan-runtime-20260610.manifest.json` and passed direct CLI transcription on the same machine with the same production model and fixture.
- The installed app was manually patched by backing up the old installed Vulkan folder to `C:\Users\hendr\AppData\Local\Programs\Trispr Flow\bin\vulkan.pre-hotfix-20260614-145021` and copying the verified repo-local hotfix payload into `C:\Users\hendr\AppData\Local\Programs\Trispr Flow\bin\vulkan`.
- After manual patching, the installed-path `whisper-cli.exe` matched the expected hotfix hash and direct fixture transcription exited `0`; installed-path `whisper-server.exe` started with the production model and responded to `http://127.0.0.1:8178/` with HTTP 200.
- The `v0.8.3` installer also surfaced an FFmpeg checksum warning during install. The NSIS hook downloads FFmpeg and verifies a hard-coded SHA256; this is a separate installer issue from the Vulkan Whisper payload mismatch.

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
- Packaged the rebuilt Vulkan runtime payload into ignored local evidence: `bench/results/trispr-flow-v0.8.3-vulkan-runtime-20260610.zip` with manifest `bench/results/trispr-flow-v0.8.3-vulkan-runtime-20260610.manifest.json`.
- Verified on 2026-06-14 that the published `v0.8.3` Vulkan-only installer did not ship that packaged payload; the installed release payload was older and failed direct CLI smoke.
- Manually patched the local installed app by replacing its installed `bin\vulkan` folder with the verified local hotfix payload; this repaired CLI and server smoke tests on the affected machine but is not a release fix.
- Updated strict release gate logic so `--strict-benchmark` requires latency `slo_pass=true`, `classification_pass=true`, and no failed cold server start target when present.
- Recorded the v0.8.3 pinned-language SLO policy in [CONTEXT.md](../../../CONTEXT.md) and [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md).
- Refreshed strict gate reports at [docs/reports/block_u_release_gate.latest.json](../../../docs/reports/block_u_release_gate.latest.json) and [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md). New report shows latency `slo_pass=true`, `classification_pass=true`, `p50=624ms`, `p95=628ms`, `server_warm:30`, and `cold_server_start_target_pass=true`.

## Stale Or Conflicting Context

- [project-spec/checkpoints/2026-06-09/vulkan-whisper-fix.md](../2026-06-09/vulkan-whisper-fix.md) said a fresh local Vulkan build could run direct fixture transcription and a short app benchmark without crashing. That may still describe a prior ignored payload, but it conflicts with the current checked local runtime state and is superseded by this checkpoint.
- Release docs and reports named by the older checkpoint may still describe pre-repair failure evidence. Current local evidence now supports release-doc/report refresh.
- `bench/results/latest.json` is ignored local evidence, not a tracked release artifact. It should be regenerated if runtime payload or benchmark logic changes.
- Published `v0.8.3` being live is not evidence that the Vulkan hotfix payload was included. Field verification showed the live Vulkan-only installer installed the stale/crashing payload instead of the manifest-matching hotfix payload.

## Open Questions

- Should CPU fallback use a true CPU-only binary rather than reusing the Vulkan binary when Vulkan is the forced backend?
- Why did the `v0.8.3` release asset omit the verified local Vulkan payload even though local package evidence existed?
- Should the release workflow gain first-class support for a standalone runtime archive input, or should `v0.8.3` be rebuilt/uploaded from a local workspace where `src-tauri/bin/vulkan` already matches the manifest?
- Should the FFmpeg installer hook update its pinned checksum or vendor/download strategy for `ffmpeg-7.1.1-essentials_build.zip`?

## Next Actions

1. Open the v0.8.4 Vulkan-only installer trust PR and review the CI result before any release upload.
2. Publish a v0.8.4 Vulkan-only release asset only after the installed-installer validation gate passes in CI.
3. Fix or disable the stale FFmpeg checksum path for online FFmpeg installation.
4. Optionally split CPU fallback behavior into a follow-up if a true CPU-only fallback binary is needed.

## Kickoff Prompt

Use checkpoint-kickoff. Continue from `project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md`: review the v0.8.4 Vulkan-only installer trust PR and decide release publication. Acceptance criteria before release upload: CI uses an explicit trusted Vulkan runtime archive input, validates the repo payload against `src-tauri/runtime-manifests/vulkan-v0.8.4-hotfix.json`, builds only the Vulkan installer by default, installs the produced installer, validates installed `bin\vulkan` hashes, runs direct CLI smoke on the short fixture, and confirms installed `whisper-server.exe` responds on port 8178. Use `/grill-with-docs` for release-policy, checkpoint, stale-doc, or cross-file design uncertainty; use `/grill-me` for pure implementation/debug uncertainty. Review but do not merge or publish unless explicitly instructed.

## Verification

Passed:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 252 passed.
- `node scripts/validate-whisper-runtime.mjs --variant vulkan`: passed.
- Direct `src-tauri/bin/vulkan/whisper-cli.exe` fixture transcription with production model: exit 0 and output text produced.
- `scripts/latency-benchmark.ps1 -Warmup 3 -Runs 3 -NoRefinement -TauriVariant vulkan`: `p50=614ms`, `p95=617ms`, `server_warm:3`, `slo_pass=true`.
- `scripts/latency-benchmark.ps1 -Warmup 3 -Runs 30 -NoRefinement -TauriVariant vulkan`: `p50=624ms`, `p95=628ms`, `server_warm:30`, `slo_pass=true`, `classification_pass=true`, `cold_server_start_ms=2293`.
- `node --check scripts/assistant-release-gate.mjs`: passed.
- `git diff --check -- scripts/assistant-release-gate.mjs docs/V0.8.x_BLOCK_U_RELEASE_GATE.md docs/INSTALLER_VARIANTS.md CONTEXT.md project-spec/checkpoints/README.md project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md`: passed.
- `npm run qa:assistant -- --strict-benchmark`: passed. It ran `npm run build`, `npm test` with 638 tests passed, and `cargo test --manifest-path src-tauri/Cargo.toml --lib` with 265 tests passed. The generated strict report shows overall gate pass.
- 2026-06-14 repo-local hotfix payload hashes matched the manifest for `whisper-cli.exe`, `whisper-server.exe`, and `ggml-vulkan.dll`.
- 2026-06-14 repo-local hotfix `whisper-cli.exe` direct fixture transcription with production model: exit 0.
- 2026-06-14 installed-path hotfix `whisper-cli.exe` direct fixture transcription after manual patch: exit 0.
- 2026-06-14 installed-path hotfix `whisper-server.exe` startup with production model: reached `whisper server listening at http://127.0.0.1:8178`.
- 2026-06-14 installed-path hotfix `whisper-server.exe` root ping: HTTP 200.
- 2026-06-14 `scripts\windows\build-installers.bat vulkan`: passed end-to-end for `installers\TrsprFlw.v0.8.4.vulkan-only-14.06.-22.14.exe` after restoring the tracked base Tauri config and adding `bin/vulkan/whisper-server.exe` to its resources.
- 2026-06-14 built v0.8.4 Vulkan-only installer installed payload matched `src-tauri/runtime-manifests/vulkan-v0.8.4-hotfix.json` exactly: 7 files, SHA256 and length checked.
- 2026-06-14 built v0.8.4 Vulkan-only installer direct installed `whisper-cli.exe` smoke: exit 0 on `bench\fixtures\short\short_de_like.wav` with production model.
- 2026-06-14 built v0.8.4 Vulkan-only installer direct installed `whisper-server.exe` smoke: HTTP 200 on `http://127.0.0.1:8178/`.
- Editor diagnostics on touched files: no errors.
- LF scan for edited tracked files: no CRLF detected.

Failed:

- Earlier pre-repair Vulkan payload failed direct CLI and benchmark with exit `-1073741795`.
- Earlier pre-server-fix benchmark produced `cli_gpu:30`, `p50=4828ms`, `p95=4885ms`, `slo_pass=false`.
- 2026-06-14 published `v0.8.3` Vulkan-only installer payload failed direct installed-path CLI smoke with exit `-1073741795` before manual patching.
- 2026-06-14 published `v0.8.3` Vulkan-only installer payload hash check failed: installed `whisper-cli.exe` and `ggml-vulkan.dll` did not match the manifest and `whisper-server.exe` was missing.
- 2026-06-14 installer run showed `FFmpeg-Prüfsumme ungültig`; this was not debugged beyond identifying the hard-coded NSIS checksum path.

Not checked:

- CI workflow execution with a trusted Vulkan runtime archive URL/SHA input.
- GitHub release asset upload for v0.8.4.
