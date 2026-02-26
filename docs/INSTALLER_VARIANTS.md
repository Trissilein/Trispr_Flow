# Trispr Flow - Installer Variants

Last updated: 2026-02-26

This document describes the current Windows installer variants for Trispr Flow mainline.

## Editions

### 1. CUDA Edition

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Includes: CUDA whisper runtime only
- Installer flow: no GPU backend selector page

### 2. Vulkan Edition

- Config: `src-tauri/tauri.conf.vulkan.json`
- NSIS hooks: `src-tauri/nsis/hooks.vulkan.nsh`
- Includes: Vulkan whisper runtime only
- Installer flow: no GPU backend selector page

## Build Command

```bat
build-both-installers.bat
```

Outputs are written to `installers/` for both variants.

## Notes

- The former CUDA+Analysis variant was removed from Trispr Flow mainline.
- Analysis packaging now lives in the dedicated `analysis-module-branch`.
