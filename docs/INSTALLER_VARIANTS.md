# Trispr Flow - Installer Variants

Windows x64 only (NVIDIA CUDA or any Vulkan GPU). The installers are **unsigned**.

## Variants

| Variant | File | Size | Contents |
| --- | --- | --- | --- |
| `mini` | `TrsprFlw.v<ver>.mini.exe` | ~4.5 MB | App + `quantize.exe`. The setup wizard downloads the speech engine (CUDA or Vulkan runtime) and the models on first start. |
| `full` | `TrsprFlw.v<ver>.full.exe` | ~477 MB | App + `quantize.exe` + both speech engines (`bin\cuda`, `bin\vulkan`). The setup wizard only downloads models. |

Both variants offer the same wizard pages: system info, startup options (AI refinement is on by default), recording mode, summary.
AI refinement uses the app-managed Ollama runtime and `gemma4:e4b`; the setup wizard downloads both on first start and the user can skip that step.

## Installed Layout

- `<install-dir>\bin\quantize.exe` (both variants)
- `<install-dir>\bin\cuda`, `<install-dir>\bin\vulkan` (`full` only)

Backend choice happens at runtime (`local_backend_preference`: `auto|cuda|vulkan`); an incomplete or failing CUDA runtime falls back to Vulkan/CPU.

## SmartScreen

The installers are not code-signed, so Windows SmartScreen shows "Windows protected your PC" on first run.
Click **More info**, then **Run anyway**.

## Whisper Runtime (pinned)

- Manifest: `src-tauri/runtime-manifests/whisper-runtime-1.9.5.json` (whisper.cpp v1.9.5, CUDA 13 + Vulkan).
- Per variant it lists the zip `url` + `zip_sha256` (GitHub release `whisper-runtime-v1.9.5`) and the exact file list `files[]` (length + SHA-256).
- `node scripts/fetch-whisper-runtime.mjs [--variants cuda,vulkan] [--bin-dir <dir>] [--verify-only] [--force]` downloads the zip, checks its hash, unpacks only manifest-listed files into `src-tauri/bin/{cuda,vulkan}`, and validates every file. A failed run never replaces an existing directory. Tests: `npm run test:scripts`.
- `src-tauri/bin/cuda` and `src-tauri/bin/vulkan` are gitignored; only `bin/quantize.exe` is tracked.

## Build

```bat
scripts\windows\build-installers.bat            rem mini + full
scripts\windows\build-installers.bat mini       rem one variant
```

- `full` fetches the runtimes from the manifest first (skipped when `src-tauri/bin/{cuda,vulkan}` already match).
- `scripts/generate-tauri-variant-config.mjs --variant <mini|full|ci>` writes the per-variant Tauri config (resource list) and `src-tauri/nsis/variant-define.nsh` (compile-time variant for `hooks.nsh`). `ci` strips all runtime payload for `cargo check/test` on runners.
- Output: `installers\TrsprFlw.v<ver>.mini.exe` and `installers\TrsprFlw.v<ver>.full.exe`.
- After each build the script silently installs the result into a temp folder and checks the payload (`scripts\windows\validate-installed-installer.ps1`). Set `TRISPR_SKIP_INSTALL_VALIDATION=1` to skip.
- `CARGO_TARGET_DIR` is honored when locating the NSIS bundle.

## Release

- Tag `vX.Y.Z` triggers `.github/workflows/windows-release-installers.yml`: it checks the tag against `package.json`, fetches the runtimes from the manifest (SHA-256 checked), builds `mini` and `full`, uploads both as workflow artifact and attaches them to the release. No manual inputs.
- Local release: `release.bat` (bump, build, tag, push, upload via `scripts\windows\upload-release-assets.ps1`).
- Bumping the runtime: publish the new zips as a new `whisper-runtime-v<ver>` release, add a new manifest file and point the manifest defaults in `scripts/fetch-whisper-runtime.mjs` and `scripts/windows/validate-installed-installer.ps1` at it.

## Ollama

- Ollama is managed by the app and installed per user on demand; models are never bundled in installers.
- Whisper CUDA DLLs and Ollama DLLs are never mixed.
