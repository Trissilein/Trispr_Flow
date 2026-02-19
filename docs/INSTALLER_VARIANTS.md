# Trispr Flow - Installer Variants

Last updated: 2026-02-19

This document describes the current Windows installer variants and how the external Analysis Tool is delivered.

## Editions

### 1. CUDA Edition

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Includes: CUDA + Vulkan whisper runtimes
- Does not bundle Trispr Analysis installer

### 2. Vulkan Edition

- Config: `src-tauri/tauri.conf.vulkan.json`
- NSIS hooks: `src-tauri/nsis/hooks.vulkan.nsh`
- Includes: Vulkan whisper runtime only
- Does not bundle Trispr Analysis installer

### 3. CUDA+Analysis Edition

- Config: `src-tauri/tauri.conf.cuda.analysis.json`
- NSIS hooks: `src-tauri/nsis/hooks.cuda.analysis.nsh`
- Includes: CUDA + Vulkan whisper runtimes
- Bundles local Analysis installer:
  - `resources/analysis-installer/Trispr-Analysis-Setup.exe`
- Optional local chain-install in NSIS, no network download

## Analysis Install Policy

- Trispr Flow runtime never auto-downloads Analysis installers.
- Analyse action uses local executable detection and optional local `.exe` selection.
- Dev builds may use Python fallback (`analysis-tool/main.py`) when no local `trispr-analysis.exe` is installed.

## Build Gate

`build-both-installers.bat` enforces a hard gate before building CUDA+Analysis:

- Required local file: `installers/Trispr-Analysis-Setup.exe`
- If missing, CUDA+Analysis build fails with a clear error.

## Build Command

```bat
build-both-installers.bat
```

Outputs are written to `installers/` for all three variants.
