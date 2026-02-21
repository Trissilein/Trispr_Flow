# Third-Party Notices

Last updated: 2026-02-20

This document lists third-party components relevant to Trispr Flow release installers.
It is not legal advice and not a full transitive dependency SBOM.
For full dependency inventories, see `package.json` and `src-tauri/Cargo.lock`.

## Canonical License Sources

- whisper.cpp / ggml license: <https://github.com/ggml-org/whisper.cpp/blob/master/LICENSE>
- Tauri license model (MIT or Apache-2.0): <https://github.com/tauri-apps/tauri>
- NVIDIA CUDA Toolkit EULA / redistribution terms: <https://docs.nvidia.com/cuda/eula/>
- FFmpeg licensing overview: <https://ffmpeg.org/legal.html>

## 1) Bundled in Release Installers

### 1.1 whisper.cpp / ggml runtime binaries

- Source project: `ggml-org/whisper.cpp`
- License: MIT
- Bundled files (depending on installer variant):
  - `whisper-cli.exe`
  - `whisper.dll`
  - `ggml.dll`
  - `ggml-base.dll`
  - `ggml-cpu.dll`
  - `ggml-cuda.dll`
  - `ggml-vulkan.dll`
  - `quantize.exe`
- Bundled in:
  - CUDA Edition (`src-tauri/tauri.conf.json`)
  - Vulkan Edition (`src-tauri/tauri.conf.vulkan.json`)

### 1.2 NVIDIA CUDA runtime redistributables (CUDA variants)

- Source vendor: NVIDIA
- License terms: NVIDIA CUDA Toolkit EULA and redistribution terms
- Bundled files:
  - `cublas64_13.dll`
  - `cudart64_13.dll`
- Bundled in:
  - CUDA Edition (`src-tauri/tauri.conf.json`)

### 1.3 Tauri runtime/framework (application binary dependency)

- Source project: `tauri-apps/tauri`
- License model: MIT or Apache-2.0
- Applies to all release variants because Trispr Flow is built on Tauri.

## 2) Optional / Not Bundled by Default

### FFmpeg

- Trispr Flow can use FFmpeg for OPUS conversion if available at runtime.
- Default release installers in this repository do not bundle FFmpeg by default.
- If a distribution bundles FFmpeg, that distribution must comply with FFmpeg license terms for the exact build configuration used.

## 3) Downloaded Model Assets (Not Bundled)

- Model files (for example Whisper model binaries downloaded from model hosts) are not bundled with the installer by default.
- Each downloaded model remains under its own upstream license and terms.
- Users/distributors are responsible for reviewing and complying with each model's license before redistribution.

## Maintenance Rule

When installer resources change (new DLL/EXE or variant changes), update both:

1. `THIRD_PARTY_NOTICES.md`
2. `docs/THIRD_PARTY_BUNDLE_MATRIX.md`
