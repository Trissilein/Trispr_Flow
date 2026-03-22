#!/usr/bin/env python3
"""OpenAI-compatible /v1/audio/speech bridge backed by qwen-tts."""

from __future__ import annotations

import io
import os
import threading
import wave
from typing import Optional

import numpy as np
from fastapi import FastAPI, Header, HTTPException
from fastapi.responses import JSONResponse, Response
from pydantic import BaseModel
from qwen_tts import Qwen3TTSModel

DEFAULT_MODEL = os.getenv(
    "TRISPR_QWEN3_TTS_MODEL",
    "Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice",
)
DEFAULT_DEVICE = os.getenv("TRISPR_QWEN3_TTS_DEVICE", "cpu")
DEFAULT_VOICE = os.getenv("TRISPR_QWEN3_TTS_VOICE", "vivian")
API_KEY = os.getenv("TRISPR_QWEN3_TTS_API_KEY", "").strip()

LANGUAGE_ALIASES = {
    "auto": "auto",
    "de": "german",
    "en": "english",
    "fr": "french",
    "it": "italian",
    "ja": "japanese",
    "ko": "korean",
    "pt": "portuguese",
    "ru": "russian",
    "es": "spanish",
}

GEN_LOCK = threading.Lock()
MODEL = None
SUPPORTED_SPEAKERS: set[str] = set()
SUPPORTED_LANGUAGES: set[str] = set()


class SpeechRequest(BaseModel):
    model: Optional[str] = None
    input: str
    voice: Optional[str] = None
    response_format: Optional[str] = "wav"
    speed: Optional[float] = 1.0
    language: Optional[str] = "auto"
    instruct: Optional[str] = None


def _normalize_language(raw: Optional[str]) -> str:
    value = (raw or "auto").strip().lower()
    value = LANGUAGE_ALIASES.get(value, value)
    if value in SUPPORTED_LANGUAGES:
        return value
    return "auto" if "auto" in SUPPORTED_LANGUAGES else (next(iter(SUPPORTED_LANGUAGES)) if SUPPORTED_LANGUAGES else "auto")


def _normalize_voice(raw: Optional[str]) -> str:
    candidate = (raw or DEFAULT_VOICE).strip().lower()
    if candidate in SUPPORTED_SPEAKERS:
        return candidate
    if DEFAULT_VOICE.lower() in SUPPORTED_SPEAKERS:
        return DEFAULT_VOICE.lower()
    return next(iter(SUPPORTED_SPEAKERS)) if SUPPORTED_SPEAKERS else candidate


def _encode_wav(wav: np.ndarray, sample_rate: int) -> bytes:
    pcm = np.asarray(wav, dtype=np.float32)
    pcm = np.clip(pcm, -1.0, 1.0)
    pcm_i16 = (pcm * 32767.0).astype(np.int16)
    buffer = io.BytesIO()
    with wave.open(buffer, "wb") as writer:
        writer.setnchannels(1)
        writer.setsampwidth(2)
        writer.setframerate(sample_rate)
        writer.writeframes(pcm_i16.tobytes())
    return buffer.getvalue()


def _load_model() -> None:
    global MODEL, SUPPORTED_SPEAKERS, SUPPORTED_LANGUAGES
    MODEL = Qwen3TTSModel.from_pretrained(DEFAULT_MODEL, device_map=DEFAULT_DEVICE)
    SUPPORTED_SPEAKERS = set((MODEL.get_supported_speakers() or []))
    SUPPORTED_LANGUAGES = set((MODEL.get_supported_languages() or []))


app = FastAPI(title="Trispr Qwen3 TTS Bridge", version="0.1.0")


@app.on_event("startup")
def _startup() -> None:
    _load_model()


@app.get("/health")
def health() -> dict:
    return {
        "ok": MODEL is not None,
        "model": DEFAULT_MODEL,
        "device": DEFAULT_DEVICE,
        "voices": sorted(SUPPORTED_SPEAKERS),
        "languages": sorted(SUPPORTED_LANGUAGES),
    }


@app.post("/v1/audio/speech")
def create_speech(
    payload: SpeechRequest,
    authorization: Optional[str] = Header(default=None),
) -> Response:
    if API_KEY:
        expected = f"Bearer {API_KEY}"
        if authorization != expected:
            raise HTTPException(status_code=401, detail="Missing or invalid bearer token.")

    text = payload.input.strip()
    if not text:
        raise HTTPException(status_code=400, detail="input is required.")
    if payload.response_format and payload.response_format.lower() not in {"wav", "pcm16"}:
        raise HTTPException(status_code=400, detail="Only response_format=wav is supported.")

    if MODEL is None:
        raise HTTPException(status_code=503, detail="Model not initialized.")

    voice = _normalize_voice(payload.voice)
    language = _normalize_language(payload.language)
    instruct = (payload.instruct or "").strip() or None

    try:
        with GEN_LOCK:
            wavs, sample_rate = MODEL.generate_custom_voice(
                text=text,
                speaker=voice,
                language=language,
                instruct=instruct,
                non_streaming_mode=True,
            )
    except Exception as err:  # pragma: no cover - runtime safety
        raise HTTPException(status_code=500, detail=f"qwen-tts generation failed: {err}") from err

    if not wavs:
        raise HTTPException(status_code=500, detail="qwen-tts returned no audio.")
    wav_bytes = _encode_wav(wavs[0], int(sample_rate))
    return Response(content=wav_bytes, media_type="audio/wav")


@app.exception_handler(Exception)
async def _unexpected_error_handler(_, exc: Exception):  # pragma: no cover - runtime safety
    return JSONResponse(status_code=500, content={"error": str(exc)})


if __name__ == "__main__":
    import argparse
    import uvicorn

    parser = argparse.ArgumentParser(description="Run Trispr Qwen3-TTS bridge server.")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8000)
    args = parser.parse_args()

    uvicorn.run(app, host=args.host, port=args.port, workers=1, log_level="info")
