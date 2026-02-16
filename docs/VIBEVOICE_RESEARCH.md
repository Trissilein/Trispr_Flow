# VibeVoice-ASR Research & Integration Plan

**Status**: Research Phase (Task C14)
**Last Updated**: 2026-02-15
**Model**: Microsoft VibeVoice-ASR 7B

---

## Model Overview

### Key Specifications
- **Model Size**: 7 Billion parameters
- **License**: MIT (open-source, commercially viable)
- **Core Capability**: Automatic Speech Recognition with Speaker Diarization
- **Language Support**: 50+ languages
- **Architecture**: Transformer-based (likely similar to Whisper/Conformer)

### Feature Set
1. **Speaker Diarization**
   - Separates multiple speakers in audio
   - Assigns speaker labels (Speaker 1, Speaker 2, etc.)
   - Timestamp-aligned segments per speaker

2. **Multi-language Support**
   - 50+ languages supported
   - Language auto-detection
   - Code-switching handling (mixed languages)

3. **Precision Modes**
   - **FP16 (Float16)**: ~14-16 GB VRAM required
   - **INT8 (8-bit quantization)**: ~7-8 GB VRAM required
   - Trade-off: INT8 = faster inference, slightly lower accuracy

---

## Typical ASR Model Architecture (Based on SOTA Models)

### Input Pipeline
```
Audio (WAV/OPUS) → Preprocessing → Feature Extraction → Model Inference → Output
```

### Preprocessing Steps
1. **Resampling**: Convert to 16kHz (standard for ASR)
2. **Normalization**: Audio level normalization
3. **Feature Extraction**: Mel-spectrogram or log-mel filterbank
4. **Chunking**: Split long audio into manageable segments (30s typical)

### Output Format (Expected)
```json
{
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
  "language": "en",
  "processing_time": 1.234
}
```

---

## Integration Requirements for Trispr Flow

### 1. Python Sidecar (FastAPI)
**Why Python?**
- VibeVoice-ASR likely uses PyTorch/TensorFlow
- FastAPI provides async HTTP API for Rust ↔ Python communication
- Easier GPU management with Python ML libraries

**Directory Structure**:
```
sidecar/
└── vibevoice-asr/
    ├── main.py              # FastAPI app entry point
    ├── requirements.txt     # Python dependencies
    ├── config.py            # Model config (FP16/INT8 selection)
    ├── model_loader.py      # Model initialization and VRAM management
    ├── inference.py         # Transcription logic
    ├── Dockerfile           # Container build (optional)
    └── README.md            # Setup instructions
```

### 2. Audio Format: OPUS
**Why OPUS over WAV?**
- **Size**: 10-20x smaller than WAV (crucial for long recordings)
- **Quality**: Perceptually lossless at 64-128 kbps
- **Streaming**: Designed for low-latency streaming
- **Compatibility**: Supported by FFmpeg, widely used in VoIP

**Encoding Settings**:
- Sample Rate: 16kHz (ASR standard) or 48kHz (high quality)
- Bitrate: 64-128 kbps (balances quality vs size)
- Channels: Mono (ASR doesn't need stereo)
- Complexity: 10 (highest quality)

### 3. Rust ↔ Python Communication

**Flow**:
```
[Tauri Rust] ──HTTP POST──> [FastAPI Python] ──GPU──> [VibeVoice Model]
                              /transcribe
     │                              │
     │                              ▼
     │                        {segments, speakers}
     │◄────────────────────────────┘
     │
     ▼
[Parse & Display Speaker-Diarized Transcript]
```

**API Design**:
```http
POST /transcribe
Content-Type: multipart/form-data

audio: <OPUS file binary>
precision: "fp16" | "int8"
language: "auto" | "en" | "de" | ...
```

**Response**:
```json
{
  "status": "success",
  "segments": [...],
  "metadata": {
    "duration": 45.3,
    "language": "en",
    "processing_time": 2.1,
    "model_precision": "fp16"
  }
}
```

---

## Hardware Requirements

### Minimum (INT8 Mode)
- **GPU**: 8 GB VRAM (RTX 3060 Ti, RTX 4060)
- **RAM**: 16 GB system RAM
- **Disk**: 20 GB for model files

### Recommended (FP16 Mode)
- **GPU**: 16 GB VRAM (RTX 4070 Ti Super, RTX 5070 Ti, RTX 3090)
- **RAM**: 32 GB system RAM
- **Disk**: 40 GB for model files + cache

### CPU Fallback (Optional Future Enhancement)
- Pure CPU inference: Very slow (~10x slower than GPU)
- Not recommended for real-time use

---

## Implementation Phases (Block C)

### Phase 1: Research & Design (This Document)
- ✅ Understand VibeVoice-ASR capabilities
- ✅ Define integration architecture
- ✅ Plan sidecar structure

### Phase 2: Sidecar Skeleton (Task C16-C17)
- Create `sidecar/vibevoice-asr/` directory
- Set up FastAPI with `/transcribe` endpoint
- Mock response for testing Rust integration

### Phase 3: OPUS Pipeline (Task C20-C22)
- Design FFmpeg-based OPUS encoding
- Implement Rust wrapper for audio conversion
- Test WAV → OPUS conversion

### Phase 4: Model Loading (Block D - Opus)
- Download/cache VibeVoice-ASR model
- Implement FP16/INT8 loader
- Test inference on sample audio

### Phase 5: Process Management (Block D - Opus)
- Rust: spawn FastAPI sidecar subprocess
- Health-check endpoint (`/health`)
- Graceful shutdown on app exit

### Phase 6: UI Integration (Block E - Sonnet)
- Speaker-diarized transcript view
- Color-coded speakers
- Export formats with speaker labels

---

## Expected Model Files

Typical structure for a 7B parameter model:

```
models/
└── vibevoice-asr-7b/
    ├── config.json                # Model architecture config
    ├── pytorch_model.bin          # FP16 weights (~14 GB)
    ├── pytorch_model_int8.bin     # INT8 weights (~7 GB) [if available]
    ├── tokenizer.json             # Text tokenizer
    ├── preprocessor_config.json   # Audio preprocessing params
    └── README.md                  # Model card
```

**Download Source**:
- HuggingFace Model Hub (likely): `huggingface.co/microsoft/vibevoice-asr-7b`
- Direct download via `transformers` library or `huggingface_hub`

---

## Key Dependencies

### Python (sidecar/vibevoice-asr/requirements.txt)
```txt
fastapi==0.109.0
uvicorn[standard]==0.27.0
transformers==4.37.0
torch==2.2.0+cu121
torchaudio==2.2.0+cu121
librosa==0.10.1
soundfile==0.12.1
numpy==1.26.3
pydantic==2.5.3
```

### Rust (Cargo.toml additions)
```toml
[dependencies]
# Existing...
reqwest = { version = "0.11", features = ["json", "multipart"] }  # HTTP client for sidecar
tokio = { version = "1", features = ["process"] }  # Async subprocess
```

### System (FFmpeg)
- **Windows**: Bundle `ffmpeg.exe` in `resources/ffmpeg/`
- **macOS**: Use Homebrew FFmpeg or bundle
- **Linux**: Expect system FFmpeg

---

## Error Handling & Edge Cases

### Sidecar Not Running
- **Detection**: HTTP request to `/health` fails
- **Action**: Auto-start sidecar process, retry
- **UI Feedback**: "Starting AI analysis engine..."

### Out of VRAM
- **Detection**: GPU OOM error in Python
- **Fallback 1**: Retry with INT8 mode
- **Fallback 2**: Show error, suggest closing GPU apps
- **UI Feedback**: "Insufficient GPU memory. Try INT8 mode or close other GPU apps."

### Unsupported Language
- **Detection**: Model doesn't support language code
- **Fallback**: Use "auto" detection
- **UI Feedback**: Warning message with supported languages

### Long Audio Files
- **Chunking**: Split into 30-second segments
- **Stitching**: Merge segments with speaker continuity
- **Progress**: Show progress bar (0-100%)

---

## Testing Strategy

### Unit Tests
1. **OPUS Encoding**: WAV → OPUS conversion correctness
2. **API Mocking**: Test Rust HTTP client with mock FastAPI
3. **Segment Parsing**: Parse JSON response into UI-ready format

### Integration Tests
1. **Sidecar Lifecycle**: Start/stop/health-check
2. **End-to-End**: Record → OPUS → FastAPI → Speaker-diarized output
3. **Precision Switching**: FP16 ↔ INT8 mode toggle

### Performance Benchmarks
- **Latency**: Measure time from audio upload to transcript
- **VRAM Usage**: Monitor GPU memory consumption
- **CPU Usage**: Ensure Rust doesn't bottleneck

---

## Open Questions (To Resolve in Implementation)

1. **Model Download**:
   - Where to cache model files? (User's models directory or separate?)
   - Auto-download on first use or require manual install?

2. **Sidecar Distribution**:
   - Bundle Python + dependencies as `.exe` (PyInstaller)?
   - Require user to install Python + pip dependencies?
   - Docker container (requires Docker Desktop)?

3. **GPU Detection**:
   - How to detect available VRAM?
   - Automatic FP16 vs INT8 selection based on GPU?

4. **Speaker Labeling**:
   - Allow user to rename "Speaker 0" → "John"?
   - Persist speaker names across sessions?

---

## Next Steps

1. ✅ **Task C14 Complete**: Research documented
2. 🔄 **Task C16**: Design sidecar directory structure (create files)
3. 🔄 **Task C17**: Implement FastAPI skeleton with mock `/transcribe`
4. 🔄 **Task C20**: Design OPUS encoding pipeline
5. 🔄 **Task C22**: Implement Rust FFmpeg wrapper

---

## References

- **Whisper (OpenAI)**: Similar ASR model architecture
- **Pyannote.audio**: Speaker diarization library (possible underlying tech)
- **OPUS Codec**: RFC 6716, Opus Interactive Audio Codec
- **FFmpeg**: Audio conversion and processing
- **FastAPI**: Modern Python web framework for ML APIs
