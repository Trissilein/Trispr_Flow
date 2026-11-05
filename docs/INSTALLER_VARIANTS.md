# Trispr Flow - Installer Variants

Last updated: 2026-03-27

This document describes the current Windows installer packaging in Trispr Flow mainline.

## Installed Layout

- The NSIS installer materializes bundled runtime payloads under `<install-dir>\bin\...`.
- Current important paths:
  - `<install-dir>\bin\cuda`
  - `<install-dir>\bin\vulkan`
  - `<install-dir>\bin\quantize.exe`
- Older tooling sometimes assumed `<install-dir>\resources\bin\...`; that is now treated only as a legacy fallback for hydration/bootstrap logic.

## Mainline Packaging (Three Variants)

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Installer flow: 5-page wizard (Hardware info → Components → First-run config → Capture mode → Summary).
- Output variants:
  - `vulkan-only`: Whisper Vulkan runtime only.
  - `cuda-lite`: Whisper CUDA runtime (no `cublasLt64_13.dll`) + Vulkan.
  - `cuda-complete`: Whisper CUDA + Vulkan runtime bundled offline.

### Variant Size Targets

| Variant | Target | Notes |
| --- | --- | --- |
| `vulkan-only` | ~30-40 MB | Whisper Vulkan + quantize only |
| `cuda-lite` | ~120 MB | + CUDA DLLs (no cublasLt) |
| `cuda-complete` | ~510-560 MB | Offline core payload |

Backend choice is made at runtime via diagnostics + settings (`local_backend_preference`: `auto|cuda|vulkan`).
With runtime preflight enabled, incomplete CUDA payloads are skipped and app falls back to Vulkan/CPU chain.

## Ollama Runtime + Dependencies

- Ollama runtime is managed by the app and installed per-user on demand (`%LOCALAPPDATA%\TrisprFlow\ollama-runtime\...`).
- Ollama models are intentionally **not** bundled in installers; they are pulled/imported via the in-app model manager.
- The managed runtime includes its own dependency tree (`lib/ollama`, `cuda_v12`, `cuda_v13`, `vulkan`).
- We do **not** mix Whisper CUDA DLLs into Ollama runtime folders and do **not** rely on Ollama DLLs for Whisper.
- CUDA runtime must include `cublasLt64_13.dll` (alongside `cublas64_13.dll` and `cudart64_13.dll`) to avoid loader failures on systems without globally installed CUDA runtime DLLs.

## Build Commands

```bat
scripts\windows\build-installers.bat
```

Compatibility wrapper:
- `build_installers.bat` (repo root)
- `rebuild-installer.bat` (repo root)

Outputs are written to `installers/` with variant suffixes (`vulkan-only`, `cuda-lite`, `cuda-complete`).

Optional release upload (local machine):

```bat
upload_release_assets.bat -Tag vX.Y.Z -CreateReleaseIfMissing -Clobber
```

By default, the uploader selects the newest artifact per variant (`vulkan-only`, `cuda-lite`, `cuda-complete`) for the given tag.  
To upload all files matching a glob, pass `-LatestPerVariant:$false`.

`release.bat` now pushes the branch and tag before it creates/uploads the GitHub release. That order matters because `gh release create --verify-tag` expects the tag to exist remotely.

## GitHub Actions Release Build

- Workflow: `.github/workflows/windows-release-installers.yml`
- Purpose: build all Windows installer variants on tag push and upload them to the matching GitHub release.
- Constraint: Whisper runtime payloads are ignored in git (`src-tauri/bin/cuda`, `src-tauri/bin/vulkan`) and are therefore missing on GitHub runners.
- Solution: the workflow now runs `scripts/hydrate-whisper-runtime-from-release.ps1` before `build-installers.bat`.
  - It downloads a published installer asset from the latest non-draft/non-prerelease release.
  - It skips the current tag when the workflow is building `vX.Y.Z`.
  - It performs a silent install into a temp directory, copies `bin/cuda`, `bin/vulkan`, and `bin/quantize.exe` into `src-tauri/bin`, and validates the payload before the actual build starts.
- Result: the runner no longer depends on local untracked binaries and can reproduce the installer build from a clean checkout.

### v0.8.3 Vulkan Hotfix Payload

The v0.8.3 Vulkan hotfix uses a verified local Vulkan payload package instead of the previous published installer payload. The previous v0.8.2 Vulkan runtime reproduces the AMD inference crash, so hydrating Vulkan from that installer would rebuild the failure into the hotfix.

Local package evidence lives under ignored benchmark output:

- `bench/results/trispr-flow-v0.8.3-vulkan-runtime-20260610.zip`
- `bench/results/trispr-flow-v0.8.3-vulkan-runtime-20260610.manifest.json`

For the v0.8.3 build, hydrate or copy `src-tauri/bin/vulkan` from that package, then run `node scripts/validate-whisper-runtime.mjs --variant vulkan` before building installers.

Field verification on 2026-06-14 showed the published `TrsprFlw.v0.8.3.vulkan-only-12.06.-12.09.exe` asset did not contain this payload. The installed `bin/vulkan` folder had stale February 2026 binaries, did not include `whisper-server.exe`, and reproduced the AMD Vulkan crash. The local manifest-matching payload passed direct CLI and server smoke after being copied manually into the installed app. Treat the current published `v0.8.3` Vulkan-only installer as not fixed until the release asset is rebuilt or replaced and its installed payload is hash-validated.

Once a fixed installer is published and validated, the normal published-installer hydration chain becomes valid again for later releases.
