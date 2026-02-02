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

## Notes for WSL/Linux builds
Tauri on Linux requires GTK/WebKit development packages. If you build in WSL, ensure those system dependencies are available; otherwise build from Windows directly.
