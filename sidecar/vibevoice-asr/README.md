# VibeVoice-ASR Sidecar

**Purpose**: FastAPI service providing speaker-diarized transcription via VibeVoice-ASR 7B model.

## Architecture

```
Trispr Flow (Tauri/Rust) ──HTTP──> FastAPI Sidecar ──GPU──> VibeVoice-ASR
                                    (Python)
```

## Directory Structure

```
sidecar/vibevoice-asr/
├── main.py              # FastAPI application entry point
├── config.py            # Configuration (precision mode, model path)
├── model_loader.py      # Model initialization and VRAM management
├── inference.py         # Transcription inference logic
├── requirements.txt     # Python dependencies
├── Dockerfile           # Container build (optional)
├── .gitignore           # Git ignore patterns
└── README.md            # This file
```

## Requirements

### Hardware
- **GPU**: 8 GB VRAM minimum (INT8), 16 GB recommended (FP16)
- **RAM**: 16 GB system RAM minimum
- **Disk**: 20 GB free space for model files

### Software
- **Python**: 3.10 or 3.11
- **CUDA**: 12.1 or higher (for GPU acceleration)
- **FFmpeg**: For audio preprocessing

## Installation

### 1. Install Python Dependencies

```bash
cd sidecar/vibevoice-asr
pip install -r requirements.txt
```

### 2. Download Model (First Run)

The model will be automatically downloaded on first use via HuggingFace Hub.
Alternatively, pre-download:

```bash
python -c "from transformers import AutoModelForSpeechSeq2Seq; AutoModelForSpeechSeq2Seq.from_pretrained('microsoft/vibevoice-asr-7b')"
```

### 3. Start Sidecar

```bash
uvicorn main:app --host 127.0.0.1 --port 8765
```

Or use the development server:

```bash
python main.py
```

## API Endpoints

### `POST /transcribe`

Transcribe audio with speaker diarization.

**Request**:
```http
POST /transcribe
Content-Type: multipart/form-data

audio: <OPUS or WAV file binary>
precision: "fp16" | "int8"  (optional, default: "fp16")
language: "auto" | "en" | "de" | ... (optional, default: "auto")
```

**Response**:
```json
{
  "status": "success",
  "segments": [
    {
      "speaker": "Speaker_0",
      "start_time": 0.0,
      "end_time": 3.5,
      "text": "Hello, welcome to the meeting."
    },
    {
      "speaker": "Speaker_1",
      "start_time": 3.8,
      "end_time": 7.2,
      "text": "Thanks for having me."
    }
  ],
  "metadata": {
    "duration": 45.3,
    "language": "en",
    "processing_time": 2.1,
    "model_precision": "fp16",
    "num_speakers": 2
  }
}
```

### `GET /health`

Health check endpoint.

**Response**:
```json
{
  "status": "ok",
  "model_loaded": true,
  "gpu_available": true,
  "vram_used_mb": 14234,
  "vram_total_mb": 16384
}
```

### `POST /reload-model`

Reload model with different precision mode.

**Request**:
```json
{
  "precision": "fp16" | "int8"
}
```

**Response**:
```json
{
  "status": "success",
  "message": "Model reloaded with precision: fp16"
}
```

## Configuration

Environment variables (optional):

- `VIBEVOICE_PRECISION`: Default precision mode (`fp16` or `int8`)
- `VIBEVOICE_MODEL_PATH`: Custom model cache directory
- `VIBEVOICE_PORT`: Server port (default: 8765)
- `VIBEVOICE_HOST`: Server host (default: 127.0.0.1)

## Development

### Running Tests

```bash
pytest tests/
```

### Code Formatting

```bash
black .
isort .
```

### Type Checking

```bash
mypy main.py
```

## Troubleshooting

### Out of VRAM

**Symptom**: `RuntimeError: CUDA out of memory`

**Solutions**:
1. Switch to INT8 mode (halves VRAM usage)
2. Close other GPU applications
3. Use smaller batch sizes (in config.py)

### Model Download Fails

**Symptom**: `ConnectionError` or `TimeoutError`

**Solutions**:
1. Check internet connection
2. Use VPN if HuggingFace is blocked
3. Manually download model and set `VIBEVOICE_MODEL_PATH`

### Slow Inference

**Symptom**: Transcription takes >10 seconds for 30s audio

**Possible Causes**:
1. Running on CPU instead of GPU
2. Other GPU processes consuming resources
3. Model not fully loaded into VRAM

**Solutions**:
1. Verify CUDA availability: `python -c "import torch; print(torch.cuda.is_available())"`
2. Check GPU usage: `nvidia-smi`
3. Restart sidecar

## Production Deployment

### PyInstaller (Standalone .exe)

```bash
pyinstaller --onefile --add-data "config.py:." main.py
```

### Docker

```bash
docker build -t vibevoice-asr .
docker run -p 8765:8765 --gpus all vibevoice-asr
```

## License

This sidecar code is MIT licensed. VibeVoice-ASR model is also MIT licensed.

## See Also

- [VIBEVOICE_RESEARCH.md](../../docs/VIBEVOICE_RESEARCH.md) - Integration architecture
- [Trispr Flow Main Repo](../../README.md)
