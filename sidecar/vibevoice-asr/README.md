# VibeVoice-ASR Sidecar

Last updated: 2026-02-19

FastAPI sidecar used by Trispr Flow for local Voice Analysis (speaker-aware transcription).

## Architecture

```text
Legacy mode:   Trispr Flow (Rust/Tauri) -> HTTP localhost -> FastAPI sidecar -> VibeVoice runtime
External mode: Trispr Flow (Rust/Tauri) -> one-shot worker_once.py process -> VibeVoice runtime
```

- Sidecar runs locally on the same machine.
- Rust sends an `audio_path` to sidecar.
- Sidecar reads the file from local disk and processes it locally.
- In external-worker mode, Rust sends one JSON payload via stdin to `worker_once.py` and receives one JSON response on stdout.

## Requirements

- Python 3.11 / 3.12 / 3.13
- Windows GPU recommended for practical speed
- Disk headroom for Python deps + HF model caches (can be many GB)
- VibeVoice ASR runtime (`vibevoice` from Microsoft GitHub archive, installed via `requirements.txt`)

## Setup

### Recommended (Windows)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\setup-vibevoice.ps1
```

Optional model prefetch:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\setup-vibevoice.ps1 -PrefetchModel
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

## External Worker Contract (`worker_once.py`)

- Input: JSON via stdin (`audio_path`, `precision`, `language`, `analysis_backend`)
- Output: JSON via stdout (same segment/metadata contract as sidecar response)
- Exit codes:
  - `0` success
  - `10` runtime/import missing
  - `11` model init/load failure
  - `14` request/input I/O failure
  - `20` generic worker failure

## Hugging Face Behavior

- HF is used for model/tokenizer download/cache access.
- Warning about missing `HF_TOKEN` only affects rate limits/download speed.
- User audio is not uploaded to HF by this sidecar.

## Local VibeVoice Runtime Source

`setup-vibevoice.ps1` installs a pinned VibeVoice runtime from Microsoft GitHub archive.
`model_loader.py` still supports local source auto-discovery for dev workflows as an override.

Policy:
- Dev builds: local `VibeVoice` source auto-discovery is enabled by default.
- Release builds: local source auto-discovery is disabled by default.
- Explicit override: set `VIBEVOICE_ALLOW_LOCAL_SOURCE=1` (or provide `VIBEVOICE_SOURCE_DIR`).

If needed, set explicitly:

```powershell
$env:VIBEVOICE_SOURCE_DIR = "D:\GIT\VibeVoice"
# Optional toggle override:
$env:VIBEVOICE_ALLOW_LOCAL_SOURCE = "1"
```

Primary fix path remains rerunning setup script; local source override is optional for development.

## Troubleshooting

### `Unrecognized processing class ...`

Cause: native VibeVoice ASR runtime is missing or incompatible.

Fix:
- rerun setup script,
- ensure the installed runtime is not legacy `vibevoice==0.0.x`,
- verify runtime dependency versions (notably `transformers==4.51.3`, `accelerate==1.6.0`, `huggingface_hub<1.0.0`),
- restart sidecar/app.

### `Object of type dtype is not JSON serializable`

Cause: model-load kwargs reached native VibeVoice init with a non-serializable dtype object.
This happens during model initialization, before audio decoding, so it is not WAV/OPUS specific.

Fix:
- update to the latest sidecar hotfix (native kwargs sanitizing),
- rerun setup script if runtime files are stale,
- restart sidecar/app.

### HF unauthenticated warning

Set token if desired:

```powershell
$env:HF_TOKEN = "hf_xxx"
```

Optional. Not required for local processing.

## Notes

- `requirements.txt` pins `transformers==4.51.3` and a commit-pinned VibeVoice runtime source for compatibility.
- This README is sidecar-focused; product roadmap lives in `../../ROADMAP.md`.
