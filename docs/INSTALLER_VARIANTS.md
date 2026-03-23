# Trispr Flow - Installer Variants

Last updated: 2026-03-23

This document describes the current Windows installer packaging in Trispr Flow mainline.

## Mainline Packaging (Three Variants)

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Installer flow: 4-page wizard (Hardware info → Components → First-run config → Summary).
- Output variants:
  - `vulkan-only`: Whisper Vulkan runtime only. FFmpeg + Piper downloaded on-demand during install.
  - `cuda-lite`: Whisper CUDA runtime (no `cublasLt64_13.dll`) + Vulkan. FFmpeg + Piper on-demand.
  - `cuda-complete`: All payloads bundled offline (Whisper CUDA + Vulkan + FFmpeg + Piper). No downloads required.

### Variant Size Targets

| Variant | Target | Notes |
| --- | --- | --- |
| `vulkan-only` | ~30-40 MB | Whisper Vulkan + quantize only |
| `cuda-lite` | ~120 MB | + CUDA DLLs (no cublasLt) |
| `cuda-complete` | ~560 MB | Full offline installer |

### On-Demand Downloads (vulkan-only + cuda-lite)

| Component | Size | When | Source |
| --- | --- | --- | --- |
| FFmpeg 7.1.1 essentials | ~84 MB | During NSIS install (if selected) | github.com/GyanD/codexffmpeg |
| Piper TTS Runtime | ~28 MB | During NSIS install (if selected) | github.com/rhasspy/piper |
| Piper voice model | ~53-63 MB | First TTS use in-app (lazy) | huggingface.co/rhasspy/piper-voices |

FFmpeg is SHA256-verified after download (`b90225987bdd...`). If FFmpeg is unavailable, OPUS encoding
falls back to WAV mode gracefully (`opus.rs::find_ffmpeg()` handles missing binary).

Piper voice models are downloaded at first TTS use via `multimodal_io::download_piper_voice()`.
Default voice: `de_DE-thorsten-medium`; can be overridden in Voice Output settings.

Backend choice is made at runtime via diagnostics + settings (`local_backend_preference`: `auto|cuda|vulkan`).
With runtime preflight enabled, incomplete CUDA payloads are skipped and app falls back to Vulkan/CPU chain.

## Ollama Runtime + Dependencies

- Ollama runtime is managed by the app and installed per-user on demand (`%LOCALAPPDATA%\TrisprFlow\ollama-runtime\...`).
- Ollama models are intentionally **not** bundled in installers; they are pulled/imported via the in-app model manager.
- The managed runtime includes its own dependency tree (`lib/ollama`, `cuda_v12`, `cuda_v13`, `vulkan`).
- We do **not** mix Whisper CUDA DLLs into Ollama runtime folders and do **not** rely on Ollama DLLs for Whisper.
- CUDA runtime must include `cublasLt64_13.dll` (alongside `cublas64_13.dll` and `cudart64_13.dll`) to avoid loader failures on systems without globally installed CUDA runtime DLLs.

## Build Commands

```bat
scripts\windows\build-installers.bat
```

Compatibility wrapper:
- `build_installers.bat` (repo root)
- `rebuild-installer.bat` (repo root)

Outputs are written to `installers/` with variant suffixes (`vulkan-only`, `cuda-lite`, `cuda-complete`).

Optional release upload (local machine):

```bat
upload_release_assets.bat -Tag vX.Y.Z -CreateReleaseIfMissing -Clobber
```

By default, the uploader selects the newest artifact per variant (`vulkan-only`, `cuda-lite`, `cuda-complete`) for the given tag.  
To upload all files matching a glob, pass `-LatestPerVariant:$false`.

## Notes

- The former CUDA+Analysis variant was removed from Trispr Flow mainline.
- Analysis packaging now lives in the dedicated `analysis-module-branch`.
