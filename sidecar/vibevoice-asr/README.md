# VibeVoice-ASR Sidecar

Last updated: 2026-02-18

FastAPI sidecar used by Trispr Flow for local Voice Analysis (speaker-aware transcription).

## Architecture

```text
Trispr Flow (Rust/Tauri) -> HTTP localhost -> FastAPI sidecar -> VibeVoice runtime
```

- Sidecar runs locally on the same machine.
- Rust sends an `audio_path` to sidecar.
- Sidecar reads the file from local disk and processes it locally.

## Requirements

- Python 3.11 / 3.12 / 3.13
- Windows GPU recommended for practical speed
- Disk headroom for Python deps + HF model caches (can be many GB)

## Setup

### Recommended (Windows)

```powershell
powershell -ExecutionPolicy Bypass -File .\setup-vibevoice.ps1
```

Optional model prefetch:

```powershell
powershell -ExecutionPolicy Bypass -File .\setup-vibevoice.ps1 -PrefetchModel
```

### Manual

```bash
python -m venv .venv
.venv\Scripts\python -m pip install -r requirements.txt
```

## API

### `POST /transcribe`

Request body is JSON (not multipart upload):

```json
{
  "audio_path": "C:/path/to/file.opus",
  "precision": "fp16",
  "language": "auto"
}
```

Response:

```json
{
  "status": "success",
  "segments": [
    {
      "speaker": "Speaker_0",
      "start_time": 0.0,
      "end_time": 1.2,
      "text": "Hello"
    }
  ],
  "metadata": {
    "duration": 1.2,
    "language": "en",
    "processing_time": 0.7,
    "model_precision": "fp16",
    "num_speakers": 1
  }
}
```

### `GET /health`

Returns model/gpu availability and VRAM stats.

### `POST /reload-model`

Switches precision (`fp16`/`int8`) by reloading model.

## Hugging Face Behavior

- HF is used for model/tokenizer download/cache access.
- Warning about missing `HF_TOKEN` only affects rate limits/download speed.
- User audio is not uploaded to HF by this sidecar.

## Local VibeVoice Runtime Source

`model_loader.py` attempts native VibeVoice imports and auto-discovers local `VibeVoice` source folders.

Planned policy: keep this auto-discovery in dev workflows, but disable it for release builds unless explicitly overridden.

If needed, set explicitly:

```powershell
$env:VIBEVOICE_SOURCE_DIR = "D:\GIT\VibeVoice"
```

This resolves errors like:

- `Unrecognized processing class in microsoft/VibeVoice-ASR`

## Troubleshooting

### `Unrecognized processing class ...`

Cause: generic HF auto-processor path is insufficient for this model without native VibeVoice runtime.

Fix:
- ensure local VibeVoice source is available and importable,
- rerun setup script,
- restart sidecar/app.

### HF unauthenticated warning

Set token if desired:

```powershell
$env:HF_TOKEN = "hf_xxx"
```

Optional. Not required for local processing.

## Notes

- `requirements.txt` currently pins `transformers` to `<5.0.0` for compatibility.
- This README is sidecar-focused; product roadmap lives in `../../ROADMAP.md`.
