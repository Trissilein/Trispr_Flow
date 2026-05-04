# TODO: Rebuild whisper-server.exe / whisper-cli.exe with multi-arch CUDA support

**Status:** Open. Required for productive work on the notebook (NVIDIA T500, compute 7.5).

## Problem

The current `src-tauri/bin/cuda/whisper-server.exe` and `whisper-cli.exe` were built with `CUDA : ARCHS = 1200` (compute capability 12.0, Blackwell — RTX 50xx series only).

On the notebook (T500, compute 7.5, Turing) the binary loads but crashes on the **first inference call** with:

```
ggml_cuda_compute_forward: IM2COL failed
CUDA error: no kernel image is available for execution on the device
```

Server then silently exits, the trispr-flow app falls back to CLI on every transcription. Diagnosed via stderr capture in `whisper-server.stderr.log` (added 2026-05-04).

The `bin/vulkan/whisper-server.exe` works on both machines but is slower than CUDA — usable as workaround but not production-fast.

## Why this happened

The CUDA build was done locally on the desktop (RTX 50xx) with arch list defaulting to detected device only. The resulting binary contains SASS only for sm_120, no PTX fallback that could be JIT-compiled for sm_75.

## Fix

Rebuild on the desktop with a multi-arch list that covers both target GPUs:

- `75` — Turing (T500, RTX 20xx)
- `80` — Ampere (A100)
- `86` — Ampere (RTX 30xx)
- `89` — Ada Lovelace (RTX 40xx)
- `90` — Hopper (H100)
- `120` — Blackwell (RTX 50xx — the desktop)

Also add PTX (`-real`/`-virtual` form): keeping `120-virtual` as the highest virtual arch lets newer GPUs JIT-compile if needed.

### Steps on the desktop

```powershell
# Use the existing build script — it already handles clone + cmake + copy.
# But edit the CUDA architectures line first (see below).

cd c:\GIT\Trispr_Flow
powershell -ExecutionPolicy Bypass -File build-whisper-server.ps1
```

The script lives at `build-whisper-server.ps1` in the repo root. It currently has:

```powershell
-DCMAKE_CUDA_ARCHITECTURES="75;80;86;89"
```

The Multi-arch update commit will change it to `"75;80;86;89;90;120"` (covers Turing through Blackwell). Verify the change is in place before running.

### What the script does (and what's missing)

The script currently only rebuilds and copies `whisper-server.exe`. It also needs to rebuild and copy `whisper-cli.exe` because that binary has the same arch limitation and is used by the CLI fallback path. After the script runs successfully, manually copy:

```powershell
Copy-Item "C:\temp\whisper.cpp-build\whisper.cpp\build-cuda\bin\Release\whisper-cli.exe" `
          -Destination "c:\GIT\Trispr_Flow\src-tauri\bin\cuda\whisper-cli.exe" -Force
```

(Or extend the script to do both — see ticket below.)

### Verification on the desktop

1. `cd c:\GIT\Trispr_Flow && npm run tauri dev`
2. Settings → set `local_backend_preference` to `"cuda"` (it should still work — Blackwell is in the new arch list)
3. Transcribe a few short clips
4. Verify `%LOCALAPPDATA%\Trispr Flow\logs\whisper-server.stderr.log` shows `CUDA : ARCHS = 75;80;86;89;90;120` and no `no kernel image` errors

### Verification on the notebook

1. Pull/sync the new binaries (they're ~25 MB each, committed in the repo under `src-tauri/bin/cuda/`)
2. `cd c:\GIT\Trispr_Flow && npm run tauri dev`
3. Settings → set `local_backend_preference` to `"cuda"` (currently `"vulkan"` as workaround)
4. Transcribe — `[diagnostics] transcribe_via_server SUCCESS` should now appear
5. `whisper-server.stderr.log` should show the model loading and the inference call running on `CUDA0` without the IM2COL error

## Follow-up improvements (optional)

- Update `build-whisper-server.ps1` to also build and copy `whisper-cli.exe`
- Add `-DCMAKE_CUDA_FLAGS="-Wno-deprecated-gpu-targets"` to suppress noise from sm_75 deprecation warnings (not blocking)
- Document the build prerequisites in the script header (CUDA Toolkit version, Visual Studio version) — current script assumes CUDA 13.0 + VS 2026

## Related context

- The persistent whisper-server (port 8178) is the fast path that pre-loads the model into VRAM, eliminating the ~1.4s per-call model load.
- Settings for backend preference: `settings.json → local_backend_preference` (`cuda` | `vulkan` | `cpu`).
- Diagnostics surface to the UI via `runtime_diagnostics.whisper.{mode, accelerator, last_error}`.

## When marking this done

- [ ] Build script CUDA arch updated to `75;80;86;89;90;120`
- [ ] Both `whisper-server.exe` and `whisper-cli.exe` rebuilt
- [ ] Both copied into `src-tauri/bin/cuda/`
- [ ] Verified on desktop (RTX 50xx) — still works
- [ ] Verified on notebook (T500) — CUDA path works, no fallback to CLI
- [ ] Notebook settings reverted: `local_backend_preference: "cuda"`
- [ ] This file moved to `docs/RESOLVED/` or deleted
