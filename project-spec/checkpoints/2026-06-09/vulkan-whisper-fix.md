# Vulkan Whisper Fix Checkpoint

Status: superseded
Created: 2026-06-09
Updated: 2026-06-10
Slug: vulkan-whisper-fix
Supersedes: none

Superseded by: ../2026-06-10/vulkan-whisper-fix.md

## Scope

This checkpoint covers the next release-quality task: make the Vulkan Whisper backend work on the target AMD machine before the follow-up release. `v0.8.2` is already published, so this work now targets the next hotfix line (`v0.8.3` unless superseded).

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

- Published `v0.8.2` Vulkan `whisper-cli` payload was reproduced on the AMD RX 6800 XT machine: `--help` exits `0`, real inference prints Vulkan device info then exits `-1073741795` (`0xC000001D`).
- The older memory that "Whisper without CLI" was problematic has no current source in this checkpoint. Treat it as unknown until code archaeology or logs prove the reason.

## Decisions

- Treat this work as the `v0.8.3` hotfix line because `v0.8.2` is already published. ADR: none; recorded in [CONTEXT.md](../../../CONTEXT.md).
- CPU fallback is acceptable as fallback behavior, but not accepted as the final release-quality latency path for this hotfix unless the user explicitly waives the latency SLO miss. ADR: none; release constraint is tracked in [ROADMAP.md](../../../ROADMAP.md), [STATUS.md](../../../STATUS.md), and [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md).
- The immediate fix path is to rebuild/package Vulkan runtime from a fresh `whisper.cpp` build, because published `v0.8.2` Vulkan crashes on real inference while fresh local Vulkan does not. ADR: none; this is operational packaging/build evidence rather than a hard-to-reverse architecture decision.
- After the Vulkan release blocker, the next planned cleanup is the Vite mixed static/dynamic import warning for `src/settings/vocabulary.settings.ts`. ADR: none; simple follow-up cleanup.

## Progress

- PR #16 was merged before this checkpoint.
- Canonical release docs were updated during this checkpoint so they no longer instruct tagging immediately after gate-doc merge.
- A checkpoint index was created at [project-spec/checkpoints/README.md](../README.md).
- This checkpoint was created at [project-spec/checkpoints/2026-06-09/vulkan-whisper-fix.md](vulkan-whisper-fix.md).
- Follow-up session reproduced the published `v0.8.2` Vulkan CLI behavior: `--help` exits successfully on AMD RX 6800 XT, but real inference with `ggml-large-v3-turbo.bin` exited `-1073741795` (`0xC000001D`) after Vulkan device detection.
- `scripts/setup-whisper.ps1` now supports `-Backend vulkan` and copies the full Vulkan runtime set, including `whisper-server.exe`, into `src-tauri/bin/vulkan/`. It also supports `-ConservativeCpu` for compatibility diagnosis.
- A fresh default `whisper.cpp` Vulkan build from local `E:\code\ingo\whisper.cpp` runs the same direct fixture transcription successfully on AMD RX 6800 XT.
- `scripts/latency-benchmark.ps1` now accepts `-TauriVariant` and temporarily swaps `src-tauri/tauri.conf.json` with the generated variant config so Vulkan-only benchmark runs do not require missing CUDA/Piper resources. It restores the base config in `finally`.
- Short app benchmark with forced Vulkan now uses GPU and no longer crashes, but still misses latency SLO on this machine: `p50_ms=3152`, `p95_ms=3247`, `slo_pass=false` for `-Warmup 1 -Runs 3 -NoRefinement`.

## Stale Or Conflicting Context

- The latest tracked gate report still says overall gate pass is yes. That is true for current gate semantics, but incomplete for release decision-making because current Vulkan evidence still has `slo_pass=false`. The next agent should refresh reports only after final release policy/runtime decision.
- Some imported local-main docs predated the `v0.8.3` correction and may still mention `v0.8.2` as the pending tag. Treat `CONTEXT.md` and this checkpoint as fresher for the hotfix target.
- `scripts/tts-benchmark.ps1` from the other local `main` checkout was intentionally not imported; do not mix Piper/TTS formatting changes into this Vulkan thread unless explicitly requested.

## Open Questions

- What exact binary/package source should become the final `v0.8.3` Vulkan payload? The working rebuilt `src-tauri/bin/vulkan/` directory is ignored by git.
- Should the hotfix ship with Vulkan crash fixed even if the short benchmark remains above the current p50 SLO, or must the SLO be met before release?
- Should `npm run qa:assistant -- --strict-benchmark` eventually fail when `bench/results/latest.json` has `slo_pass=false`, or is explicit release-policy review enough for the hotfix?
- What was the old issue with "Whisper without CLI"? Unknown until source/log archaeology proves it.

## Next Actions

1. Decide the final runtime payload source for `v0.8.3`: package the rebuilt ignored `src-tauri/bin/vulkan/` payload, rebuild in release workflow, or attach/copy a verified binary artifact.
2. Run a longer Vulkan benchmark with production model defaults after final payload selection. Use `scripts/latency-benchmark.ps1 -Warmup 3 -Runs 30 -NoRefinement -TauriVariant vulkan` with Vulkan env overrides if needed.
3. Decide release policy for the remaining p50 SLO miss if the longer run still misses.
4. If policy requires strict SLO pass, investigate server warm path and timing: current short runs are GPU but around 3.1s p50.
5. Refresh release docs/reports only after runtime payload and SLO policy are settled.
6. After Vulkan/hotfix path is settled, address the Vite mixed import warning for `src/settings/vocabulary.settings.ts`.

## Kickoff Prompt

Use checkpoint-kickoff. Continue from `project-spec/checkpoints/2026-06-09/vulkan-whisper-fix.md`: finish the Vulkan Whisper hotfix by deciding/packaging the final verified Vulkan runtime payload and resolving the remaining latency SLO policy/performance gap. Use `/grill-with-docs` for release-policy or stale-doc uncertainty. Fix and commit; do not mix unrelated Piper/TTS changes.

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

Checked in follow-up session:

- Published `v0.8.2` Vulkan direct CLI `--help`: exit `0`.
- Published `v0.8.2` Vulkan direct CLI fixture transcription: exit `-1073741795`.
- Fresh local Vulkan `whisper.cpp` build: direct fixture transcription exit `0`.
- Short app benchmark after fresh Vulkan build: GPU path, no crash, SLO miss (`p50_ms=3152`, `p95_ms=3247`).
- `npm test`: 36 files, 637 tests passed.
- Focused script validation: `scripts/setup-whisper.ps1` and `scripts/latency-benchmark.ps1` parse; `node scripts/generate-tauri-variant-config.mjs --variant vulkan` succeeds; `node scripts/validate-whisper-runtime.mjs --variant vulkan` succeeds.

Not checked yet:

- Full 30-run production latency rerun after final Vulkan package selection.
- Fresh strict assistant gate after final Vulkan package selection.
