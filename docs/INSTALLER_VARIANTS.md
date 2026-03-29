# Trispr Flow - Installer Variants

Last updated: 2026-03-27

This document describes the current Windows installer packaging in Trispr Flow mainline.

## Mainline Packaging (Three Variants)

- Config: `src-tauri/tauri.conf.json`
- NSIS hooks: `src-tauri/nsis/hooks.nsh`
- Installer flow: 5-page wizard (Hardware info → Components → First-run config → Capture mode → Summary).
- Output variants:
  - `vulkan-only`: Whisper Vulkan runtime only. FFmpeg + Piper downloaded on-demand during install.
  - `cuda-lite`: Whisper CUDA runtime (no `cublasLt64_13.dll`) + Vulkan. FFmpeg + Piper on-demand.
  - `cuda-complete`: Whisper + FFmpeg + Piper runtime + base voice (`de_DE-thorsten-medium`) bundled offline.

### Variant Size Targets

| Variant | Target | Notes |
| --- | --- | --- |
| `vulkan-only` | ~30-40 MB | Whisper Vulkan + quantize only |
| `cuda-lite` | ~120 MB | + CUDA DLLs (no cublasLt) |
| `cuda-complete` | ~510-560 MB | Offline core payload + base Piper voice |

### Optional Installer Downloads (all variants)

| Component | Size | When | Source |
| --- | --- | --- | --- |
| FFmpeg 7.1.1 essentials | ~84 MB | During NSIS install (if selected) | github.com/GyanD/codexffmpeg |
| Piper TTS Runtime | ~28 MB | During NSIS install (if selected) | github.com/rhasspy/piper |
| Piper curated voice packs | ~53-81 MB each | During NSIS install (optional selection) | huggingface.co/rhasspy/piper-voices |
| Piper extra voice keys | model-dependent | During NSIS install (optional text list) | huggingface.co/rhasspy/piper-voices |

FFmpeg is SHA256-verified after download (`b90225987bdd...`). If FFmpeg is unavailable, OPUS encoding
falls back to WAV mode gracefully (`opus.rs::find_ffmpeg()` handles missing binary).

Installer Components page now contains:
- Curated Piper list (>= medium, no US): `de_DE-thorsten-medium`, `de_DE-thorsten_emotional-medium`, `en_GB-alan-medium`, `en_GB-alba-medium`, `en_GB-cori-high`.
- Extra key input field (`<locale>-<voice>-<quality>`, one key per line, quality in `x_low|low|medium|high`).

Behavior:
- `cuda-complete`: Piper already present; voice downloads are optional.
- `vulkan-only`/`cuda-lite`: voice selection is active only if Piper runtime is selected.
- Download failures never abort setup; installer shows warning list for failed keys.
- Invalid extra keys are skipped and reported.

Fresh install defaults keep `voice_output_settings.piper_model_path = "de_DE-thorsten-medium"` whenever Piper is selected.
Upgrades keep existing `settings.json` unchanged.

Runtime fallback remains available: missing voices can still be downloaded in-app via
`multimodal_io::download_piper_voice()` when `piper_model_path` uses a voice key.

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
