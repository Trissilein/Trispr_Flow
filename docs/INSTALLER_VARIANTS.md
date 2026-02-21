# Trispr Flow - Installer Variants

Last updated: 2026-02-20

This document describes the canonical Windows release installer variants for Trispr Flow mainline.

## Editions

### 1. CUDA Edition

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Includes: CUDA + Vulkan whisper runtimes

### 2. Vulkan Edition

- Config: `src-tauri/tauri.conf.vulkan.json`
- NSIS hooks: `src-tauri/nsis/hooks.vulkan.nsh`
- Includes: Vulkan whisper runtime only

## Analysis Module Policy

- Analysis is developed and distributed as a separate module/project line.
- Trispr Flow mainline installers do not bundle analysis payloads.
- Trispr Flow runtime does not auto-download analysis installers.

## Build Command

```bat
build-both-installers.bat
```

Canonical release outputs are written to `installers/` for the two supported variants: CUDA and Vulkan.
