from __future__ import annotations

import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from fastapi import FastAPI
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel

from .analysis_runtime import AnalysisRuntimeError, transcribe_file


@dataclass
class StartupContext:
    audio_path: str | None = None
    source: str | None = None


class _State:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._startup = StartupContext()

    def set_startup(self, audio_path: str | None, source: str | None) -> None:
        with self._lock:
            self._startup = StartupContext(audio_path=audio_path, source=source)

    def get_startup(self) -> StartupContext:
        with self._lock:
            return StartupContext(
                audio_path=self._startup.audio_path,
                source=self._startup.source,
            )


STATE = _State()


class OpenFileRequest(BaseModel):
    audio_path: str
    source: str | None = None


class TranscribeRequest(BaseModel):
    audio_path: str
    context_info: str = ""
    sampling_enabled: bool = False
    temperature: float = 0.0
    top_p: float = 1.0


def create_app() -> FastAPI:
    app = FastAPI(title="Trispr Analysis Tool")

    static_dir = Path(__file__).resolve().parent / "static"
    app.mount("/static", StaticFiles(directory=str(static_dir)), name="static")

    @app.get("/")
    async def index() -> FileResponse:
        return FileResponse(static_dir / "index.html")

    @app.get("/api/ping")
    async def ping() -> dict[str, Any]:
        return {"ok": True}

    @app.get("/api/startup-context")
    async def startup_context() -> dict[str, Any]:
        data = STATE.get_startup()
        return {
            "audio_path": data.audio_path,
            "source": data.source,
        }

    @app.post("/api/open-file")
    async def open_file(payload: OpenFileRequest) -> dict[str, Any]:
        STATE.set_startup(payload.audio_path, payload.source)
        return {"status": "ok"}

    @app.post("/api/clear-startup-context")
    async def clear_startup_context() -> dict[str, Any]:
        STATE.set_startup(None, None)
        return {"status": "ok"}

    @app.post("/api/transcribe")
    async def transcribe(payload: TranscribeRequest) -> dict[str, Any]:
        start = time.time()
        try:
            result = transcribe_file(payload.audio_path, payload.context_info)
        except AnalysisRuntimeError as exc:
            return {
                "status": "error",
                "error": str(exc),
            }

        metadata = dict(result.get("metadata") or {})
        metadata.setdefault("processing_time", max(0.0, time.time() - start))
        return {
            "status": "success",
            "segments": result.get("segments") or [],
            "metadata": metadata,
            "diagnostics": result.get("diagnostics") or {},
        }

    return app
