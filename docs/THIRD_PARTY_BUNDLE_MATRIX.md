# Third-Party Bundle Matrix

Last updated: 2026-02-20

This matrix maps installer resource manifests to third-party binary payloads.
Source of truth for bundle inputs:

- `src-tauri/tauri.conf.json`
- `src-tauri/tauri.conf.vulkan.json`
- `src-tauri/tauri.conf.cuda.analysis.json`

Use this file together with `THIRD_PARTY_NOTICES.md`.

## CUDA Edition (`src-tauri/tauri.conf.json`)

| Resource path | Binary | Category | License reference |
| --- | --- | --- | --- |
| `bin/cuda/whisper-cli.exe` | `whisper-cli.exe` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/cuda/whisper.dll` | `whisper.dll` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/cuda/ggml.dll` | `ggml.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/cuda/ggml-base.dll` | `ggml-base.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/cuda/ggml-cpu.dll` | `ggml-cpu.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/cuda/ggml-cuda.dll` | `ggml-cuda.dll` | ggml CUDA backend | MIT (whisper.cpp/ggml) |
| `bin/cuda/cublas64_13.dll` | `cublas64_13.dll` | NVIDIA CUDA redistributable | NVIDIA CUDA EULA |
| `bin/cuda/cudart64_13.dll` | `cudart64_13.dll` | NVIDIA CUDA redistributable | NVIDIA CUDA EULA |
| `bin/vulkan/whisper-cli.exe` | `whisper-cli.exe` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/vulkan/whisper.dll` | `whisper.dll` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/vulkan/ggml.dll` | `ggml.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-base.dll` | `ggml-base.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-cpu.dll` | `ggml-cpu.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-vulkan.dll` | `ggml-vulkan.dll` | ggml Vulkan backend | MIT (whisper.cpp/ggml) |
| `bin/quantize.exe` | `quantize.exe` | whisper.cpp utility | MIT (whisper.cpp) |

## Vulkan Edition (`src-tauri/tauri.conf.vulkan.json`)

| Resource path | Binary | Category | License reference |
| --- | --- | --- | --- |
| `bin/vulkan/whisper-cli.exe` | `whisper-cli.exe` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/vulkan/whisper.dll` | `whisper.dll` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/vulkan/ggml.dll` | `ggml.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-base.dll` | `ggml-base.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-cpu.dll` | `ggml-cpu.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-vulkan.dll` | `ggml-vulkan.dll` | ggml Vulkan backend | MIT (whisper.cpp/ggml) |
| `bin/quantize.exe` | `quantize.exe` | whisper.cpp utility | MIT (whisper.cpp) |

## CUDA+Analysis Edition (`src-tauri/tauri.conf.cuda.analysis.json`)

| Resource path | Binary | Category | License reference |
| --- | --- | --- | --- |
| `bin/cuda/whisper-cli.exe` | `whisper-cli.exe` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/cuda/whisper.dll` | `whisper.dll` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/cuda/ggml.dll` | `ggml.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/cuda/ggml-base.dll` | `ggml-base.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/cuda/ggml-cpu.dll` | `ggml-cpu.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/cuda/ggml-cuda.dll` | `ggml-cuda.dll` | ggml CUDA backend | MIT (whisper.cpp/ggml) |
| `bin/cuda/cublas64_13.dll` | `cublas64_13.dll` | NVIDIA CUDA redistributable | NVIDIA CUDA EULA |
| `bin/cuda/cudart64_13.dll` | `cudart64_13.dll` | NVIDIA CUDA redistributable | NVIDIA CUDA EULA |
| `bin/vulkan/whisper-cli.exe` | `whisper-cli.exe` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/vulkan/whisper.dll` | `whisper.dll` | whisper.cpp runtime | MIT (whisper.cpp) |
| `bin/vulkan/ggml.dll` | `ggml.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-base.dll` | `ggml-base.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-cpu.dll` | `ggml-cpu.dll` | ggml runtime | MIT (whisper.cpp/ggml) |
| `bin/vulkan/ggml-vulkan.dll` | `ggml-vulkan.dll` | ggml Vulkan backend | MIT (whisper.cpp/ggml) |
| `bin/quantize.exe` | `quantize.exe` | whisper.cpp utility | MIT (whisper.cpp) |
| `analysis-installer/Trispr-Analysis-Setup.exe` | `Trispr-Analysis-Setup.exe` | external analysis installer payload | separate package notices |

## Notes

- The matrix is release-oriented (bundled payloads only).
- Optional runtime tools not bundled by default (for example FFmpeg from PATH) are tracked in `THIRD_PARTY_NOTICES.md` under optional components.
