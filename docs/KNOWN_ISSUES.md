# Known Issues — Trispr Flow

Tracking known build-tooling and environment issues that do **not** block production releases. Each entry includes the impact, a reproducible diagnosis, what was tried, and the workaround in active use.

---

## #001 — `cargo test --lib` fails with `STATUS_ENTRYPOINT_NOT_FOUND` on this Windows build host

**Status:** Triage parked, low priority. Production unaffected.

**First observed:** 2026-04-29 during the `v0.8.0` release-gate run.

**Confirmed pre-existing:** Reproduces identically against commit `b3db990` (pre-vocab changes). `Cargo.lock` between `8ff759f` (v0.7.5 release) and `b3db990` is unchanged, so the bug likely existed silently for some time but went unnoticed because the gate runner (`scripts/assistant-release-gate.mjs`) accepts `--skip-rust-lib-tests`.

### Symptom

```
$ cargo test --lib
   Compiling trispr-flow v0.8.0 ...
    Finished `test` profile [unoptimized + debuginfo] target(s) in 11.69s
     Running unittests src\lib.rs (target\debug\deps\trispr_flow_lib-XXXX.exe)
error: test failed, to rerun pass `--lib`

Caused by:
  process didn't exit successfully: ... (exit code: 0xc0000139, STATUS_ENTRYPOINT_NOT_FOUND)
```

`cargo nextest run --lib` fails identically with the same exit code (the German `os error 127` translation is "Die angegebene Prozedur wurde nicht gefunden" / "The specified procedure could not be found").

### Diagnosis Findings

- **Production binary is healthy.** `cargo build --bin trispr-flow` produces a `trispr-flow.exe` that starts cleanly to normal exit. `npm run build`, `npm test` (269 frontend tests), and `cargo build --lib` all succeed.
- **Bug is in the test binary only.** Custom Win32 debugger (Python + ctypes, `CreateProcess(DEBUG_ONLY_THIS_PROCESS)`) shows 26 system DLLs load successfully, then a single `STATUS_ENTRYPOINT_NOT_FOUND` exception is raised at an address inside `ntdll.dll` (offset `0xdd2f`) with `NumberParameters=0`. No detail info is provided by the loader.
- **Last DLLs loaded before the fault:** `cryptbase.dll` (most recent), `vcruntime140.dll`, `userenv.dll`, `bcrypt.dll`, `dwmapi.dll`, `comctl32.dll`. The actual import resolution that fails happens after `cryptbase.dll` is mapped but before the C-runtime entry-point reaches `main()`.
- **Direct symbol resolution succeeds.** Every directly-imported symbol of the test binary resolves cleanly via `LoadLibraryExW` + `GetProcAddress`, including all `vcruntime140.dll` symbols (`memcpy`, `__current_exception_context`, `_CxxThrowException`, …). The fault is therefore not in the IAT directly visible in the PE; likely a forwarder, delay-load, or transitive `DllMain` call inside one of the loaded modules.
- **Test vs. bin DLL diff:** the test binary additionally imports `userenv.dll` (`GetUserProfileDirectoryW`) but *lacks* `VCRUNTIME140_1.dll`, `dxgi.dll`, `propsys.dll`, `shlwapi.dll`, and several `api-ms-win-crt-*` shims that the production bin pulls in. The presence/absence of these CRT shims correlates with the fault but is not proven causal.

### What was tried (all failed to fix)

- `cargo clean && cargo test --lib` (fresh build artefacts)
- `RUSTFLAGS="-C target-feature=+crt-static"` (static CRT linkage)
- `cargo nextest run --lib` (alternative test harness)
- Reverting the `Win32_System_Threading` feature add in `windows` crate
- Reverting all source changes between `b3db990` and HEAD
- Comparing imports between bin and test binary (no missing direct symbol)

### Why we are not chasing this further right now

- The fault is environment-specific (this Windows 11 26200 build host). It may not reproduce on a fresh Windows runner (e.g. GitHub Actions), so a CI run could re-enable the gate without local debugging.
- Frontend Vitest suite (`npm test`) covers the TypeScript surface that is the user-visible logic.
- Manual smoke-testing of the packaged installer covers the Tauri-runtime integration path.

**Note (2026-05-23):** Rust unit tests now exist in at least 8 source files (`hotkeys.rs`, `audio.rs`, `continuous_dump.rs`, `ai_fallback/provider.rs`, `errors.rs`, `lib.rs`, `models.rs`, `workflow_agent.rs`). The CI smoke job redesign (see `project-spec/decisions/2026-05-23/ci-smoke-job-redesign.md`) moves test execution to CI (GitHub Actions Windows runner) where this crash is not expected to reproduce. The local workaround (`--skip-rust-lib-tests` in the release gate) remains in place.

### Workaround in active use

Release-gate runs use `--skip-rust-lib-tests`. The flag is allowed by `scripts/assistant-release-gate.mjs` and is reflected in the gate report's `options.skip_rust_lib_tests` field. This is acceptable because `--strict-benchmark` plus the green automated checks plus the green frontend tests produce a sufficiently robust gate signal for a non-soak release.

### Re-engagement criteria

Pick this up again when one of:
- The fix is implemented (see "Proposed fix" below) and `--no-run` can be removed.
- A Rust unit test for backend logic becomes load-bearing and compile-check is no longer sufficient.

**Update (2026-05-24) — root cause fully identified:**

**Missing entry point: `TaskDialogIndirect` in `comctl32.dll`**

- The test binary links against `comctl32.dll` (pulled in by Tauri's windowing code).
- Without an application manifest, Windows loads `C:\Windows\System32\comctl32.dll` (v5.82 — the legacy compatibility layer). `comctl32` v5.82 does **not** export `TaskDialogIndirect`; that function is v6 only.
- The production Tauri binary has a manifest embedded by `tauri-winres` via `tauri_build::build()` (see `src-tauri/build.rs`), declaring `Microsoft.Windows.Common-Controls` v6 as a `dependentAssembly`. This causes Windows to load the v6 DLL from WinSxS instead.
- The test binary has no such manifest, so the loader crashes with `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139) before any Rust code runs.
- Verified locally using `llvm-readobj --coff-imports` (PE import table) and `llvm-readobj --coff-exports C:\Windows\System32\comctl32.dll`.

**Proposed fix (one-line in `src-tauri/build.rs`):**

Add a `cargo:rustc-link-arg-tests` directive that tells the MSVC linker to embed the comctl32 v6 dependency in the test binary's manifest, matching what `tauri-winres` does for the production binary. The `rustc-link-arg-tests` instruction only applies to test targets, so it does not affect the production binary:

```rust
// In build.rs, after tauri_build::build():
#[cfg(target_os = "windows")]
println!(
    "cargo:rustc-link-arg-tests=/MANIFESTDEPENDENCY:type='win32' \
     name='Microsoft.Windows.Common-Controls' version='6.0.0.0' \
     processorArchitecture='amd64' publicKeyToken='6595b64144ccf1df' language='*'"
);
```

This embeds the comctl32 v6 activation into the test binary's embedded manifest via the MSVC linker's `/MANIFESTDEPENDENCY` flag. Once tested and confirmed, remove `--no-run` from `test:smoke` and update the CI smoke ADR.

**CI reproduction:** Confirmed on GitHub Actions `windows-latest` (Server 2025 / Windows 11 24H2) — CI run 26372280621 (before `--no-run` restore). Both local Windows 11 26200 and CI are affected. The fault is structural (missing manifest in test binary), not environment-specific.

### Useful diagnostics path for verification

1. Apply the proposed `build.rs` fix.
2. Run `cargo test --manifest-path src-tauri/Cargo.toml --lib` (without `--no-run`).
3. Expect the test binary to launch cleanly and run (0 tests, 0 failures).
4. Remove `--no-run` from `test:smoke` in `package.json` and push to CI.



---

<!--
Add new entries above this line. Keep entries dated; archive entries that are
fixed by moving them to a "Resolved" section once the workaround is removed.
-->
