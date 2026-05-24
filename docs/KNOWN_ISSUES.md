# Known Issues — Trispr Flow

Tracking known build-tooling and environment issues that do **not** block production releases. Each entry includes the impact, a reproducible diagnosis, what was tried, and the workaround in active use.

---

## #001 — `cargo test --lib` fails with `STATUS_ENTRYPOINT_NOT_FOUND` on this Windows build host

**Status:** RESOLVED 2026-05-24. Fix in `src-tauri/build.rs` (delay-load comctl32). Production unaffected throughout.

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

### Workaround in active use (updated 2026-05-24)

Release-gate runs still use `--skip-rust-lib-tests`. The reason has changed: the loader crash is fixed, but `cargo test --lib` now reveals 3 pre-existing logic test failures in `ai_fallback/provider` (`custom_profile_prompt_is_not_modified_by_language_lock`, `ssrf_target_blocks_ipv4_mapped_ipv6_link_local`, `ssrf_target_blocks_ipv6_link_local`). Until those are addressed, the gate flag remains appropriate.

### Resolution (2026-05-24)

Root cause was confirmed as a missing `TaskDialogIndirect` export in `C:\Windows\System32\comctl32.dll` v5.82 when loading the Rust lib test binary.

The fix is now implemented in `src-tauri/build.rs`:
- Emit `cargo:rustc-link-arg=/DELAYLOAD:comctl32.dll`
- Emit `cargo:rustc-link-lib=delayimp`

This moves `comctl32.dll` from hard imports to delay imports for the test binary path, preventing `STATUS_ENTRYPOINT_NOT_FOUND` during process load. Verification now reaches normal Rust test execution (assertion failures, if any, are regular test failures and no longer loader crashes).

Re-open this issue only if loader-level entry-point faults return on `cargo test --lib`.



---

<!--
Add new entries above this line. Keep entries dated; archive entries that are
fixed by moving them to a "Resolved" section once the workaround is removed.
-->
