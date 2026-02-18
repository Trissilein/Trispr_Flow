# Trispr Flow - Installer Variants

Last updated: 2026-02-18

This document describes the current installer editions and the Voice Analysis dependency flow.

## Editions

### 1. CUDA Edition

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Includes CUDA + Vulkan whisper runtimes (installer lets user choose backend)
- Recommended for NVIDIA users

### 2. Vulkan Edition

- Config: `src-tauri/tauri.conf.vulkan.json`
- NSIS hooks: `src-tauri/nsis/hooks.vulkan.nsh`
- Includes Vulkan whisper runtime only
- Recommended for AMD/Intel users and minimal GPU runtime footprint

## Voice Analysis Packaging Strategy (Current)

To avoid shipping a much larger installer, the following is true by design:

- The installer does **not** bundle full VibeVoice model weights.
- The installer bundles sidecar source/runtime files:
  - `sidecar/vibevoice-asr/main.py`
  - `sidecar/vibevoice-asr/model_loader.py`
  - `sidecar/vibevoice-asr/inference.py`
  - `sidecar/vibevoice-asr/config.py`
  - `sidecar/vibevoice-asr/requirements.txt`
  - `sidecar/vibevoice-asr/setup-vibevoice.ps1`
- Python dependencies and model assets are installed/downloaded only when user opts into Voice Analysis.

## Policy Baseline (Locked Decisions)

- Base installer stays slim; Voice Analysis remains optional.
- Prefetch is default OFF.
- Voice Analysis setup failures are soft-fail (installer completes, remediation is shown).
- Guided first-use setup flow on Analyse click is planned and tracked in roadmap blocks (not fully shipped yet).

## Installer User Flow (Voice Analysis)

1. User runs setup (`CUDA` or `Vulkan` edition).
2. NSIS asks: enable Voice Analysis (`yes/no`).
3. If `yes`:
   - installer checks whether Python is available,
   - if missing, installer offers to open python.org download page,
   - post-install, setup script is executed automatically when available.
4. If auto-setup fails, user gets a remediation command:
   - `powershell -ExecutionPolicy Bypass -File "<install_path>\resources\sidecar\vibevoice-asr\setup-vibevoice.ps1"`

## What Gets Downloaded at Runtime

When Voice Analysis dependencies are installed and model cache is cold:

- Python wheels (torch/transformers/etc.) into local venv
- Hugging Face model/tokenizer files into HF cache

Important:
- This can consume **significant disk space** (often many GB, depending on cache state and model revisions).
- Trispr Flow does **not** upload user audio to Hugging Face.
- Hugging Face calls are for model/tokenizer retrieval and cache checks.

## Build Commands

### Build both installers

```bat
build-both-installers.bat
```

### Build single variant

```bash
npm run build
cd src-tauri
cargo tauri build --config tauri.conf.json
# or
cargo tauri build --config tauri.conf.vulkan.json
```

## Notes

- Canonical priority/planning is in `ROADMAP.md`.
- This file is distribution-focused only.
