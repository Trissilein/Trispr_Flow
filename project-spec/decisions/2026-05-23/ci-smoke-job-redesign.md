# CI Smoke Job Redesign

Date: 2026-05-23  
Status: **decided — implemented**  
Participants: Hendr (architect), automated grill-with-docs session

---

## Context

The `smoke` job in `.github/workflows/ci.yml` was failing on `refactor/maintainability-foundation` with:

```
resource path 'bin\ffmpeg\ffmpeg.exe' doesn't exist
```

Investigation found the failure was a symptom of a deeper design problem in `test:smoke`. The job as written could not pass on a fresh CI checkout without significant binary hydration.

### `test:smoke` before this change (package.json)

```
npm run build &&
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --lib --no-run &&
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --bins &&
cargo build --manifest-path src-tauri/Cargo.toml
```

### Facts from code inspection

- `src-tauri/Cargo.toml` defines **no `[features]` section**. `--no-default-features` is a no-op.
- `src-tauri/src/main.rs` is a 5-line shim: `trispr_flow_lib::run()`. All logic lives in the library.
- `[[bin]] test = false` means `cargo test --bins` skips the main binary.
- `cargo build` (final step) triggers `tauri_build::build()` in `build.rs`, which validates that resource paths declared in `tauri.conf.json` exist on disk.
- `tauri.conf.json` declares `bin/ffmpeg/ffmpeg.exe` as a bundle resource. `src-tauri/bin/ffmpeg/` is gitignored. On a fresh CI checkout, the file does not exist.
- The same applies to `bin/cuda/`, `bin/vulkan/`, and `bin/piper/` — all gitignored, all declared as bundle resources. Adding only FFmpeg would expose the next missing resource.
- `scripts/generate-tauri-variant-config.mjs` already handles this for installer builds: it generates variant-specific configs that filter out gitignored resources. The `ci` variant does not yet exist.
- Rust tests (`#[cfg(test)]`) exist in at least 8 files: `hotkeys.rs`, `audio.rs`, `continuous_dump.rs`, `ai_fallback/provider.rs`, `errors.rs`, `lib.rs`, `models.rs`, `workflow_agent.rs`. All are pure unit tests with no external dependencies.
- `--no-run` caused **zero Rust tests to execute** in CI. The flag's origin is _unknown_: the user reported it was set by a previous AI agent without documented rationale. `docs/KNOWN_ISSUES.md` records a separate local crash (`STATUS_ENTRYPOINT_NOT_FOUND`) when running `cargo test --lib` on Hendrik's Windows 11 machine. That document explicitly states the fault is environment-specific and "may not reproduce on a fresh Windows runner (e.g. GitHub Actions)." The two facts may be related or coincidental.
- FFmpeg is used solely for archiving transcriptions as OPUS files (a non-critical path, GDD module integration). It is not on the core capture/transcription/refinement path.

---

## Decision: Smoke job intent is code correctness only

The smoke job verifies that:
1. The TypeScript frontend compiles and bundles without errors (`npm run build`)
2. The Rust library compiles without errors
3. Rust unit tests pass

The smoke job does **not** verify bundle completeness (that all installer binary assets are present). That concern belongs to the release workflow (`windows-release-installers.yml`), which already handles binary hydration via `hydrate-whisper-runtime-from-release.ps1` and `build-installers.bat`.

Rejected alternative: hydrate all gitignored binaries in the smoke job. Rejected because it would require downloading 200–500 MB of runtime assets (Whisper, Piper, FFmpeg, CUDA DLLs) on every PR CI run for a non-blocking gate. The release workflow already covers bundle completeness.

---

## Changes (three files)

### 1. `package.json` — `test:smoke` rewritten

```
npm run build &&
cargo test --manifest-path src-tauri/Cargo.toml --lib &&
cargo test --manifest-path src-tauri/Cargo.toml --bins
```

Removals:
- `--no-default-features` removed (was a no-op; no features defined in Cargo.toml)
- `--no-run` removed (was preventing any Rust tests from executing; tests should run in CI)
- final `cargo build` removed (compiled a 5-line shim with no safety value; was the source of resource validation failure)

Risk: if `cargo test --lib` crashes on the CI runner due to the same DLL fault described in `docs/KNOWN_ISSUES.md`, the smoke job will fail on that step instead. Expected: the fault does not reproduce on a fresh GitHub Actions Windows runner per the KNOWN_ISSUES assessment. If it does, the smoke job will need `--no-run` restored and a separate fix.

### 2. `scripts/generate-tauri-variant-config.mjs` — `ci` variant added

New variant `ci` filters out all gitignored resource directories:
- `bin/cuda/` (gitignored)
- `bin/vulkan/` (gitignored)
- `bin/ffmpeg/` (gitignored)
- `bin/piper/` (gitignored)

Keeps only `bin/quantize.exe` (the one committed binary).

This ensures `tauri_build::build()` does not fail on missing files regardless of which `cargo` step triggers it.

### 3. `.github/workflows/ci.yml` — pre-step added to smoke job

A step is inserted before "Smoke test":

```yaml
- name: Generate CI tauri config
  run: node scripts/generate-tauri-variant-config.mjs --variant ci --out src-tauri/tauri.conf.json
```

This overwrites `tauri.conf.json` in the transient CI checkout with the CI-appropriate config before any `cargo` step runs.

---

## What is not changed

- `continue-on-error: true` on the smoke job remains. The job stays informational (non-blocking). Promotion to required check is a separate decision, contingent on CI proving stable green.
- `docs/DECISIONS.md` DEC-007 ("Standard local verification is `npm run test` plus `npm run test:smoke`") remains accurate and is not updated.
- The release installer workflow is not touched.

---

## Follow-up (2026-05-23, same day)

Running the redesigned smoke job locally surfaced two pre-existing issues in the TTS benchmark code path that were previously hidden by `--no-run` and the resource-validation failure. Both were fixed so the redesigned smoke job can actually go green:

- **rustfmt drift in `src-tauri/src/tts_benchmark.rs` and `src-tauri/src/lib.rs`.** Resolved by `cargo fmt`. No semantic change.
- **Visibility boundary on `TtsBenchmarkResult`.** `src-tauri/src/lib.rs` read five private fields (`recommended_default_provider`, `release_gate_pass`, `release_gate_reason`, `recommendation_reason`, `uncategorized_failure_count`) to emit a benchmark summary log. Rejected: making the fields `pub(crate)` (leaks report shape across the crate). Chosen: a `pub(crate) fn log_summary(&self, report_path: &Path)` method on `TtsBenchmarkResult` that owns the summary logging. `lib.rs` now calls `report.log_summary(&path)`; report shape stays encapsulated in `tts_benchmark.rs`.

Verification: `cargo fmt --check` clean; `cargo check` clean (only pre-existing unrelated warnings) under the generated CI Tauri config.

Stale documentation cleaned up in the same pass:

---

## Follow-up (2026-05-24)

The assumption that `STATUS_ENTRYPOINT_NOT_FOUND` would not reproduce on a fresh GitHub Actions Windows runner proved false. CI run [26371709951](https://github.com/Trissilein/Trispr_Flow/actions/runs/26371709951) produced the identical crash on `windows-latest` (Server 2025 / Windows 11 24H2).

**Root cause understanding:** The crash is not a missing DLL (`STATUS_DLL_NOT_FOUND` = 0xc0000135). It is `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139): every DLL the test binary references is present, but a function one of them tries to call from another DLL is not in that DLL's export table on this Windows version. The production binary has `VCRUNTIME140_1.dll`, `dxgi.dll`, `propsys.dll`, and `api-ms-win-crt-*` shims that the test binary lacks, suggesting the test binary's loader chain reaches a version-mismatched path those shims would otherwise redirect.

**Decision:** Restore `--no-run` to `cargo test --lib` (the ADR's stated fallback). Rust test *execution* in CI is now a separate tracked work item. The compile-check (`--no-run`) still catches type errors, missing symbols, and refactoring breakage — the primary safety value for a parallel-agent development workflow.

**Next diagnostic step (planned):** Run `dumpbin /imports` on the locally built test binary (`src-tauri/target/debug/deps/trispr_flow_lib-*.exe`) and compare each imported function against the export tables of the corresponding DLLs on this machine. This will identify the specific missing entry point and determine whether static CRT linkage or a different fix is appropriate.

Stale documentation cleaned up in the same pass:

- `GEMINI.md`: removed "Full build + Rust tests + Cargo build check" description of `test:smoke`.
- `docs/DEVELOPMENT.md`: replaced the single-line `cargo test` step with the actual `--lib` then `--bins` invocations and a pointer to this ADR.
