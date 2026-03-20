# Trispr Flow - Installer Variants

Last updated: 2026-03-13

This document describes the current Windows installer packaging in Trispr Flow mainline.

## Mainline Packaging

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Includes: both Whisper runtime folders (`bin/cuda/*` and `bin/vulkan/*`) plus `bin/quantize.exe`.
- Installer flow: no in-installer backend selector page.

Backend choice is made at runtime via diagnostics + settings (`local_backend_preference`: `auto|cuda|vulkan`).
The installer no longer prunes CUDA/Vulkan folders post-install.

## Ollama Runtime + Dependencies

- Ollama runtime is managed by the app and installed per-user on demand (`%LOCALAPPDATA%\TrisprFlow\ollama-runtime\...`).
- Ollama models are intentionally **not** bundled in installers; they are pulled/imported via the in-app model manager.
- The managed runtime includes its own dependency tree (`lib/ollama`, `cuda_v12`, `cuda_v13`, `vulkan`).
- We do **not** mix Whisper CUDA DLLs into Ollama runtime folders and do **not** rely on Ollama DLLs for Whisper.
- CUDA runtime keeps the minimal whisper DLL set; `cublasLt64_13.dll` is intentionally excluded as redundant for whisper.cpp.

## Build Command

```bat
scripts\windows\rebuild-installer.bat
```

Compatibility wrapper:
- `rebuild-installer.bat` (repo root)

Output is written to the NSIS bundle output folder (and copied to `installers/` by helper scripts where configured).

## Notes

- The former CUDA+Analysis variant was removed from Trispr Flow mainline.
- Analysis packaging now lives in the dedicated `analysis-module-branch`.
