"""
VibeVoice-ASR FastAPI Sidecar
Provides speaker-diarized transcription API for Trispr Flow
"""

import logging
import os
import time
from pathlib import Path
from typing import Optional

from fastapi import FastAPI, File, Form, HTTPException, UploadFile
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel

from config import PrecisionMode, model_config, server_config
from model_loader import ModelLoader, get_gpu_info

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Initialize FastAPI app
app = FastAPI(
    title="VibeVoice-ASR Sidecar",
    description="Speaker-diarized transcription API",
    version="0.6.0"
)

# CORS (development only)
if server_config.cors_enabled:
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

# Global model loader instance
model_loader: Optional[ModelLoader] = None


@app.on_event("startup")
async def startup_event():
    """Initialize model on startup"""
    global model_loader
    logger.info("Starting VibeVoice-ASR sidecar...")
    logger.info(f"Configuration: {model_config}")

    try:
        model_loader = ModelLoader(model_config)
        logger.info("Model loader initialized (lazy loading on first request)")
    except Exception as e:
        logger.error(f"Failed to initialize model loader: {e}")
        raise


@app.on_event("shutdown")
async def shutdown_event():
    """Cleanup on shutdown"""
    global model_loader
    logger.info("Shutting down VibeVoice-ASR sidecar...")
    if model_loader:
        model_loader.unload_model()


# ============================================================================
# API Models
# ============================================================================

class TranscriptionSegment(BaseModel):
    """Single transcription segment with speaker label"""
    speaker: str
    start_time: float
    end_time: float
    text: str


class TranscriptionMetadata(BaseModel):
    """Transcription metadata"""
    duration: float
    language: str
    processing_time: float
    model_precision: str
    num_speakers: int


class TranscriptionResponse(BaseModel):
    """Transcription API response"""
    status: str
    segments: list[TranscriptionSegment]
    metadata: TranscriptionMetadata


class TranscribeByPathRequest(BaseModel):
    """Transcription request using local file path"""
    audio_path: str
    precision: str = "fp16"
    language: str = "auto"


class HealthResponse(BaseModel):
    """Health check response"""
    status: str
    model_loaded: bool
    gpu_available: bool
    vram_used_mb: Optional[int] = None
    vram_total_mb: Optional[int] = None


class ReloadRequest(BaseModel):
    """Model reload request"""
    precision: PrecisionMode


class ReloadResponse(BaseModel):
    """Model reload response"""
    status: str
    message: str


# ============================================================================
# API Endpoints
# ============================================================================

@app.get("/")
async def root():
    """Root endpoint"""
    return {
        "service": "VibeVoice-ASR Sidecar",
        "version": "0.6.0",
        "status": "running"
    }


@app.get("/health", response_model=HealthResponse)
async def health_check():
    """Health check endpoint"""
    gpu_info = get_gpu_info()

    return HealthResponse(
        status="ok",
        model_loaded=model_loader.is_loaded() if model_loader else False,
        gpu_available=gpu_info["available"],
        vram_used_mb=gpu_info.get("vram_used_mb"),
        vram_total_mb=gpu_info.get("vram_total_mb"),
    )


@app.post("/transcribe", response_model=TranscriptionResponse)
async def transcribe_audio(request: TranscribeByPathRequest):
    """
    Transcribe audio from local file path with speaker diarization.

    The Rust host and this sidecar run on the same machine,
    so the sidecar reads the audio file directly from disk.

    **Parameters:**
    - `audio_path`: Local path to audio file (OPUS or WAV)
    - `precision`: Model precision mode ("fp16" or "int8")
    - `language`: Language code (e.g., "en", "de") or "auto"

    **Returns:**
    - Speaker-diarized transcript with timestamps
    """
    start_time = time.time()

    if not model_loader:
        raise HTTPException(status_code=500, detail="Model loader not initialized")

    # Validate precision
    if request.precision not in ["fp16", "int8"]:
        raise HTTPException(
            status_code=400,
            detail=f"Invalid precision: {request.precision}. Must be 'fp16' or 'int8'"
        )

    # Validate audio file exists
    audio_path = Path(request.audio_path)
    if not audio_path.exists():
        raise HTTPException(
            status_code=400,
            detail=f"Audio file not found: {request.audio_path}"
        )

    try:
        # Read audio file
        audio_data = audio_path.read_bytes()
        file_size = len(audio_data)
        logger.info(f"Processing audio: {audio_path.name} ({file_size} bytes)")

        # Ensure model is loaded with requested precision
        if model_loader.config.precision != request.precision:
            logger.info(f"Switching precision to {request.precision}")
            model_loader.reload_model(request.precision)

        if not model_loader.is_loaded():
            logger.info("Loading model for first transcription...")
            model_loader.load_model()

        # Run transcription
        result = model_loader.transcribe(audio_data, request.language)

        processing_time = time.time() - start_time

        # Build response segments
        segments = [
            TranscriptionSegment(
                speaker=seg["speaker"],
                start_time=seg["start_time"],
                end_time=seg["end_time"],
                text=seg["text"],
            )
            for seg in result.get("segments", [])
        ]

        # Count unique speakers
        speakers = set(seg.speaker for seg in segments)

        # Get audio duration from metadata or estimate
        duration = result.get("metadata", {}).get("duration", 0.0)
        if duration == 0.0 and segments:
            duration = max(seg.end_time for seg in segments)

        return TranscriptionResponse(
            status="success",
            segments=segments,
            metadata=TranscriptionMetadata(
                duration=duration,
                language=result.get("metadata", {}).get("language", request.language),
                processing_time=processing_time,
                model_precision=request.precision,
                num_speakers=len(speakers),
            )
        )

    except MemoryError:
        raise HTTPException(
            status_code=507,
            detail="CUDA out of memory. Try INT8 precision or close GPU applications."
        )

    except Exception as e:
        logger.error(f"Transcription failed: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@app.post("/reload-model", response_model=ReloadResponse)
async def reload_model(request: ReloadRequest):
    """Reload model with different precision mode"""
    if not model_loader:
        raise HTTPException(status_code=500, detail="Model loader not initialized")

    try:
        logger.info(f"Reloading model with precision: {request.precision}")
        model_loader.reload_model(request.precision)

        return ReloadResponse(
            status="success",
            message=f"Model reloaded with precision: {request.precision}"
        )

    except Exception as e:
        logger.error(f"Model reload failed: {e}")
        raise HTTPException(status_code=500, detail=str(e))


# ============================================================================
# Main Entry Point
# ============================================================================

if __name__ == "__main__":
    import uvicorn

    uvicorn.run(
        "main:app",
        host=server_config.host,
        port=server_config.port,
        log_level=server_config.log_level,
        reload=False,
    )
