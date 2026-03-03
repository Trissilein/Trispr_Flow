# Trispr Flow - Installer Variants

Last updated: 2026-02-26

This document describes the current Windows installer variants for Trispr Flow mainline.

## Editions

### 1. CUDA Edition

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Includes: CUDA whisper runtime only (`whisper-cli.exe`, `ggml-*.dll`, `cublas64_13.dll`, `cudart64_13.dll`)
- Installer flow: no GPU backend selector page

### 2. Vulkan Edition

- Config: `src-tauri/tauri.conf.vulkan.json`
- NSIS hooks: `src-tauri/nsis/hooks.vulkan.nsh`
- Includes: Vulkan whisper runtime only
- Installer flow: no GPU backend selector page

## Ollama Runtime + Dependencies

- Ollama runtime is managed by the app and installed per-user on demand (`%LOCALAPPDATA%\TrisprFlow\ollama-runtime\...`).
- Ollama models are intentionally **not** bundled in installers; they are pulled/imported via the in-app model manager.
- The managed runtime includes its own dependency tree (`lib/ollama`, `cuda_v12`, `cuda_v13`, `vulkan`).
- We do **not** mix Whisper CUDA DLLs into Ollama runtime folders and do **not** rely on Ollama DLLs for Whisper.
- CUDA edition keeps the minimal whisper DLL set; `cublasLt64_13.dll` is intentionally excluded as redundant for whisper.cpp.

## Build Command

```bat
build-both-installers.bat
```

Outputs are written to `installers/` for both variants.

## Notes

- The former CUDA+Analysis variant was removed from Trispr Flow mainline.
- Analysis packaging now lives in the dedicated `analysis-module-branch`.
