# Development setup

## Prerequisites
- Node.js (npm)
- Rust toolchain (stable)
- Tauri v2 prerequisites for your OS (toolchain + webview runtime)

## Run the app
```bash
npm install
npm run tauri dev
```

## First run (Windows)
Use the canonical bootstrap script after cloning:

```bat
scripts\windows\FIRST_RUN.bat
```

Compatibility wrapper:
- `FIRST_RUN.bat` (repo root)

What it does:
- runs `npm install`
- tries to copy runtime files from an existing installed app:
  - `%LOCALAPPDATA%\Programs\Trispr Flow\bin\cuda`
  - `%LOCALAPPDATA%\Programs\Trispr Flow\bin\vulkan`
  - `%LOCALAPPDATA%\Programs\Trispr Flow\bin\quantize.exe`
  - `%LOCALAPPDATA%\Programs\Trispr Flow\bin\ffmpeg\ffmpeg.exe`
- falls nötig zusätzlich aus älteren Layouts unter `resources\bin\...`
- prints runtime readiness summary for transcription, quantization, and FFmpeg

Optional flags:
- `scripts\windows\FIRST_RUN.bat -SkipNpmInstall`
- `scripts\windows\FIRST_RUN.bat -SkipRuntimeHydration`
- `scripts\windows\FIRST_RUN.bat -RequireWhisperRuntime` (fail with exit code 2 if no local Whisper runtime is detected)

If no runtime is found, the script prints actionable instructions and continues by default.

## Windows release pipeline

- The installer workflow lives at `.github/workflows/windows-release-installers.yml`.
- `src-tauri/bin/cuda` and `src-tauri/bin/vulkan` are intentionally ignored and are not available in a clean GitHub checkout.
- Release CI therefore hydrates Whisper runtime payloads from the latest published installer before building:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/hydrate-whisper-runtime-from-release.ps1 -SkipTag vX.Y.Z
```

- The hydration script downloads a published installer asset, runs a silent install into a temp directory, copies `bin/cuda`, `bin/vulkan`, and `bin/quantize.exe` back into `src-tauri/bin`, and terminates the installer if NSIS stays alive after the payload has materialized.
- Tag builds skip the current tag and rehydrate from the previous published release, so `vX.Y.Z` can bootstrap itself from the last stable installer set.

## Release publishing

- `release.bat` now pushes `main` and the version tag before creating the GitHub release.
- `scripts/windows/upload-release-assets.ps1` creates a public release when missing and can mark it as `latest`.
- This avoids the broken state where assets were uploaded against an unpublished draft while the tag was already visible.

Notes:
- `npm run dev` starts the desktop app (`tauri dev`) and reuses an already running Trispr Vite server on `http://localhost:1420`.
- `npm run dev:web` starts only the Vite frontend dev server (web preview).

## Installer rebuild (Windows)
Use the canonical batch script to build the NSIS installer:

```bat
scripts\windows\rebuild-installer.bat
```

Compatibility wrapper:
- `rebuild-installer.bat` (repo root)

Notes:
- The script anchors to the repo root, so it works regardless of the current working directory.
- The script now auto-runs `scripts/setup-ffmpeg.ps1` to fetch a pinned FFmpeg binary with `libopus` encoder support.
- If Vite complains about HTML asset paths, confirm `vite.config.ts` keeps `root` and HTML inputs inside the repo root.

## FFmpeg for OPUS (Windows)
Installer builds require an OPUS-capable FFmpeg binary at `src-tauri/bin/ffmpeg/ffmpeg.exe`.

Use:
```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/setup-ffmpeg.ps1
```

The setup script validates:
- pinned SHA256 of the downloaded `ffmpeg.exe`
- `encoder=libopus` availability (required by the OPUS pipeline)

## Test workflow
### Unit tests
```bash
npm run test
```

### Smoke test (frontend + Rust)
```bash
npm run test:smoke
```

`test:smoke` runs:
1. `npm run build` (TypeScript + Vite production build)
2. `cargo test --manifest-path src-tauri/Cargo.toml`

## Local whisper.cpp (GPU)
1. Build whisper.cpp with CUDA enabled.
2. Place models in the whisper.cpp `models/` directory.
3. Set environment variables (see `.env.example`).

### One-shot setup (Windows)
If your whisper.cpp checkout is in `D:\!GIT\whisper.cpp`, you can run:

```
.\scripts\setup-whisper.ps1
```

This builds whisper.cpp with CUDA and writes `.env.local` with:
`TRISPR_WHISPER_CLI` and `TRISPR_WHISPER_MODEL_DIR`.

If you do not have the CUDA Toolkit installed yet, install it first so `nvcc`
is available on PATH. As a temporary fallback, you can run:

```
.\scripts\setup-whisper.ps1 -CpuFallback
```

If CUDA is installed but CMake still cannot find the CUDA toolset, try:

```
.\scripts\setup-whisper.ps1 -CudaToolset 13.1
```

Notes:
- VS 18/2026 may require CUDA build customizations to be copied into the v180
  BuildCustomizations folder (see STATUS.md for the workaround used).

## Environment variables
See `.env.example` for runtime configuration. You can override the default model download base URL with `TRISPR_WHISPER_MODEL_BASE_URL`.

## Local Ollama runtime (managed)

- Trispr Flow manages Ollama runtime per-user and can install it from the UI (AI Refinement section).
- Model pulls stay in-app and are not bundled with installers.
- Runtime dependency policy:
  - Whisper keeps its own CUDA DLL set in `src-tauri/bin/cuda` (including `cublas64_13.dll`, `cublasLt64_13.dll`, `cudart64_13.dll`).
  - Ollama keeps its own dependency tree under `%LOCALAPPDATA%\Trispr Flow\ollama-runtime\...`.
  - Do not copy/mix DLLs between Whisper and Ollama runtime folders.

## Notes for WSL/Linux builds
Tauri on Linux requires GTK/WebKit and linker dependencies. In WSL/Linux, install:

```bash
sudo apt install -y \
  pkg-config \
  libgtk-3-dev \
  libglib2.0-dev \
  libcairo2-dev \
  libpango1.0-dev \
  libatk1.0-dev \
  libgdk-pixbuf-2.0-dev \
  libwebkit2gtk-4.1-dev \
  libasound2-dev \
  libxdo-dev
```

If these are missing, `cargo test` in smoke runs may fail at link/build steps (`pkg-config`, `alsa`, `xdo`, GTK/WebKit).
