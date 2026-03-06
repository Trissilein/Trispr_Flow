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

Notes:
- `npm run dev` starts the desktop app (`tauri dev`) and reuses an already running Trispr Vite server on `http://localhost:1420`.
- `npm run dev:web` starts only the Vite frontend dev server (web preview).

## Installer rebuild (Windows)
Use the repo-root batch script to build the NSIS installer:

```bat
rebuild-installer.bat
```

Notes:
- The script anchors to the repo root, so it works regardless of the current working directory.
- If Vite complains about HTML asset paths, confirm `vite.config.ts` keeps `root` and HTML inputs inside the repo root.

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
  - Whisper keeps its own CUDA DLL set in `src-tauri/bin/cuda`.
  - Ollama keeps its own dependency tree under `%LOCALAPPDATA%\TrisprFlow\ollama-runtime\...`.
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
