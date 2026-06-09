# Vulkan Whisper Fix Checkpoint

Status: active
Created: 2026-06-09
Updated: 2026-06-09
Slug: vulkan-whisper-fix
Supersedes: none

## Scope

This checkpoint covers the next release-quality task: make the Vulkan Whisper backend work on the target AMD machine before tagging `v0.8.2`.

It does not cover the later Vite warning cleanup, Piper `local_custom` follow-up work, general refactoring, or Block B UX/UI work except as sequencing context.

## Sources Read

- `gh pr view 16 --json number,state,mergedAt,headRefName,baseRefName,url,title`
- `git fetch --prune`, `git status --short --branch`, `git log --oneline --decorate`
- [CONTEXT.md](../../../CONTEXT.md)
- [ROADMAP.md](../../../ROADMAP.md)
- [STATUS.md](../../../STATUS.md)
- [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md)
- [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md)
- [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs)
- [src-tauri/src/paths.rs](../../../src-tauri/src/paths.rs)
- [scripts/latency-benchmark.ps1](../../../scripts/latency-benchmark.ps1)
- Local ignored benchmark evidence: `bench/results/latest.json`

## Current Facts

- PR #16, `docs(release): integrate Block A gate closure`, is merged into `main` and its remote branch was deleted. Source: `gh pr view 16` returned `state=MERGED`, `mergedAt=2026-06-09T19:11:24Z`, and `headRefName=integration/block-a-release-gate-closure`.
- Updated `origin/main` points at `61727bd docs(release): integrate Block A gate closure (#16)`. Source: `git log --oneline --decorate origin/main -5`.
- The local working branch at checkpoint time was `integration/block-a-release-gate-closure`, whose upstream is gone. It also had a local commit `7c1c1a5 fix(tts): clean benchmark script diagnostics` and local `scripts/tts-benchmark.ps1` formatting changes unrelated to Vulkan. Do not mix those changes into the Vulkan fix unless intentionally picked up.
- Trispr Core owns the Whisper Backend. The backend ships CUDA and Vulkan builds and selects them at runtime through `local_backend_preference`; `whisper-server` is preferred and `whisper-cli` is fallback. Source: [CONTEXT.md](../../../CONTEXT.md).
- Runtime path resolution honors `TRISPR_WHISPER_CLI` before normal backend search. `TRISPR_LOCAL_BACKEND` can override backend preference when no explicit preference is passed. Source: [src-tauri/src/paths.rs](../../../src-tauri/src/paths.rs).
- Backend search prefers Vulkan before CUDA when the normalized backend preference is `vulkan`; otherwise it tries CUDA then Vulkan. Source: [src-tauri/src/paths.rs](../../../src-tauri/src/paths.rs).
- Vulkan runtime preflight expects `whisper-cli.exe`, `whisper.dll`, `ggml.dll`, `ggml-base.dll`, `ggml-cpu.dll`, and `ggml-vulkan.dll`. Source: [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
- `whisper_backend_from_cli_path` classifies a CLI path as `vulkan` when the path contains `/vulkan/`, `\vulkan\`, or `build-vulkan`. Source: [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
- Explicit `local_backend_preference = "vulkan"` means the GPU attempt order is only `vulkan`, with no hidden switch back to CUDA in that GPU phase. Source: [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
- After all GPU CLI attempts fail, transcription tries CLI CPU fallback. The current code passes `-ng` when CPU fallback uses a CLI that supports `-ng` or `--no-gpu`. Source: [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
- `scripts/latency-benchmark.ps1` writes `bench/results/latest.json` and warns rather than exits nonzero on SLO miss unless `-FailOnSloMiss` is passed. Source: [scripts/latency-benchmark.ps1](../../../scripts/latency-benchmark.ps1).
- Local ignored latency evidence exists, but it used CPU fallback: `p50_ms=22968`, `p95_ms=23763`, `slo_p50_ms=2500`, `slo_p95_ms=4000`, `slo_pass=false`, samples show `accelerator=cpu`. Source: local `bench/results/latest.json` summary read during this checkpoint.
- The latest tracked gate report says automated checks pass, benchmark linkage passes, latency and TTS reports are present, and TTS release gate passes. Source: [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md).
- The tracked gate report does not prove latency SLO success. It proves benchmark linkage. The raw ignored latency report is the source for the SLO miss.

Session observations to reproduce, not yet checkpoint-verified in repo files:

- Published CUDA/Vulkan `whisper-cli` payloads were previously observed to crash on the AMD RX 6800 XT machine; Vulkan printed device info before exiting with a Windows crash code. This must be reproduced in the fresh session before fixing.
- The older memory that "Whisper without CLI" was problematic has no current source in this checkpoint. Treat it as unknown until code archaeology or logs prove the reason.

## Decisions

- Do not tag `v0.8.2` until Vulkan is fixed, or until the user explicitly waives the CPU-fallback latency SLO miss. ADR: none; this is a release constraint recorded in [ROADMAP.md](../../../ROADMAP.md), [STATUS.md](../../../STATUS.md), and [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md).
- CPU fallback is acceptable as fallback behavior, but not accepted as the final release-quality latency path for this tag. ADR: none; same release constraint as above.
- After the Vulkan release blocker, the next planned cleanup is the Vite mixed static/dynamic import warning for `src/settings/vocabulary.settings.ts`. ADR: none; simple follow-up cleanup.

## Progress

- PR #16 was merged before this checkpoint.
- Canonical release docs were updated during this checkpoint so they no longer instruct tagging immediately after gate-doc merge.
- A checkpoint index was created at [project-spec/checkpoints/README.md](../README.md).
- This checkpoint was created at [project-spec/checkpoints/2026-06-09/vulkan-whisper-fix.md](vulkan-whisper-fix.md).
- No Vulkan code fix has been attempted in this checkpoint.

## Stale Or Conflicting Context

- The latest tracked gate report still says overall gate pass is yes. That is true for current gate semantics, but incomplete for release decision-making because the raw ignored latency report has `slo_pass=false` on CPU fallback. The release docs now state the extra Vulkan-before-tag constraint.
- `STATUS.md` still contains older historical performance notes from prior NVIDIA/Q5 evidence. Those notes are historical, not the current `v0.8.2` target evidence. No edit needed for this Vulkan checkpoint.
- Current local Git state is not a clean `origin/main` checkout. The receiving agent should start from `origin/main` or a fresh branch and should not inherit unrelated local `scripts/tts-benchmark.ps1` formatting changes by accident.

## Open Questions

- Does bundled `src-tauri/bin/vulkan/whisper-cli.exe --help` crash on the target AMD machine, or only real transcription calls?
- Is the Vulkan failure caused by missing or mismatched DLLs, Vulkan driver/runtime compatibility, build CPU instruction flags, bad invocation args, model size/memory pressure, or app fallback orchestration?
- Does a fresh local `whisper.cpp` Vulkan build work on the same machine with the same `ggml-large-v3-turbo.bin` model and fixtures?
- Should `npm run qa:assistant -- --strict-benchmark` eventually fail when `bench/results/latest.json` has `slo_pass=false`, or is the Vulkan-before-tag policy enough for `v0.8.2`?
- What was the old issue with "Whisper without CLI"? Unknown until source/log archaeology proves it.

## Next Actions

1. Start from fresh `origin/main` after PR #16, not from the deleted `integration/block-a-release-gate-closure` branch.
2. Preserve or deliberately discard the unrelated local `scripts/tts-benchmark.ps1` formatting changes before beginning Vulkan work.
3. Verify bundled Vulkan runtime files exist under `src-tauri/bin/vulkan/` and include the required files listed in [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs).
4. Run the bundled Vulkan CLI directly with `--help` and capture exit code, stdout, and stderr.
5. Run a minimal fixture transcription through the bundled Vulkan CLI directly, using the production model at `%LOCALAPPDATA%\Trispr Flow\models\ggml-large-v3-turbo.bin` if available.
6. Run the app benchmark with explicit Vulkan preference and a tiny fixture set. Keep `-Warmup 1 -Runs 3 -NoRefinement` for diagnosis before the full 30-run evidence pass.
7. If bundled Vulkan crashes, build or obtain a fresh local `whisper.cpp` Vulkan runtime and compare direct CLI behavior on the same model and fixtures.
8. Fix the confirmed root cause: runtime hydration/package mismatch, bundled DLL set, app invocation args, backend detection, or build flags.
9. Rerun production-default latency evidence with `ggml-large-v3-turbo.bin`, then rerun `npm run qa:assistant -- --strict-benchmark`.
10. Refresh release docs/reports and tag `v0.8.2` only after Vulkan evidence satisfies the release constraint or the user explicitly waives it.
11. After Vulkan/tag path is settled, address the Vite mixed import warning for `src/settings/vocabulary.settings.ts`.

## Verification

Checked before checkpoint:

- PR #16 merge state via `gh pr view 16`.
- Current branch and origin state via `git fetch --prune`, `git status --short --branch`, and `git log --oneline --decorate`.
- Relevant source behavior in [src-tauri/src/transcription.rs](../../../src-tauri/src/transcription.rs) and [src-tauri/src/paths.rs](../../../src-tauri/src/paths.rs).
- Gate report and release gate docs in [docs/reports/block_u_release_gate.latest.md](../../../docs/reports/block_u_release_gate.latest.md) and [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md).
- Local ignored latency summary from `bench/results/latest.json`.

Passed before checkpoint, from PR #16 validation:

- `npm test`: 36 files, 637 tests.
- `npm run build`: passed, with Vite warning about mixed static/dynamic import of `src/settings/vocabulary.settings.ts`.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 252 tests.
- `git diff --check`: clean at PR #16 merge validation time.

Not checked in this checkpoint:

- Vulkan direct CLI reproduction.
- Local Vulkan `whisper.cpp` build.
- Full production latency rerun after Vulkan fix.
- Fresh strict assistant gate after Vulkan fix.