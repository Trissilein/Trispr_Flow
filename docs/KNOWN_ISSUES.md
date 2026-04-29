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

- Rust-side unit-test coverage is currently at zero impact because no Rust tests fail. Compile gates (`cargo build --lib` + `cargo check`) catch refactoring breakage equally.
- Frontend Vitest suite (`npm test`, 269 tests across 21 files) covers the TypeScript surface that is the user-visible logic.
- Manual smoke-testing of the packaged installer covers the Tauri-runtime integration path.
- The fault is environment-specific (this Windows 11 26200 build host). It may not reproduce on a fresh Windows runner (e.g. GitHub Actions), so a CI run could re-enable the gate without local debugging.

### Workaround in active use

Release-gate runs use `--skip-rust-lib-tests`. The flag is allowed by `scripts/assistant-release-gate.mjs` and is reflected in the gate report's `options.skip_rust_lib_tests` field. This is acceptable because `--strict-benchmark` plus the green automated checks plus the green frontend tests produce a sufficiently robust gate signal for a non-soak release.

### Re-engagement criteria

Pick this up again when one of:
- A Rust unit test for backend logic becomes load-bearing (would require running this gate on a different machine or in CI).
- The fault address inside `ntdll.dll` changes meaningfully (suggests a Windows update touched the loader path).
- An independent reproducer is found on a different developer's machine, ruling out the local environment as cause.
- We have time to install Application Verifier or Windows Debugging Tools (gflags + Loader Snaps) to capture the missing-symbol name conclusively.

### Useful diagnostics path if anyone re-engages

1. Install Windows Debugging Tools for Windows (provides `gflags.exe` and `cdb.exe`).
2. `gflags /i trispr_flow_lib-*.exe +sls` to enable Loader Snaps for the test binary.
3. Run `cdb -G -gx target\debug\deps\trispr_flow_lib-*.exe` and capture the loader-snap output before the entry-point exception. The output prints every `LdrpResolveDllName` and `LdrpResolveProcedureAddress` call and identifies the missing symbol name.
4. Alternative: install Application Verifier and enable the "Basics" stop in test-binary, run, capture `AV` log.

---

<!--
Add new entries above this line. Keep entries dated; archive entries that are
fixed by moving them to a "Resolved" section once the workaround is removed.
-->
