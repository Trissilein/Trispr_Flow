"""
One-shot analysis worker for process-isolated Voice Analysis jobs.

Reads a JSON request from stdin, runs a single VibeVoice transcription, and
prints the JSON response to stdout.

Exit codes:
- 0: success
- 10: runtime missing / import failure
- 11: model load or model init failure
- 12: timeout (reserved for launcher)
- 13: canceled (reserved for launcher)
- 14: invalid request / I/O failure
- 20: generic worker failure
"""

from __future__ import annotations

import copy
import json
import logging
import sys
import time
from pathlib import Path
from typing import Any


logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger("worker_once")


def _fail(exit_code: int, message: str, details: str | None = None) -> None:
    payload: dict[str, Any] = {
        "status": "error",
        "error": message,
    }
    if details:
        payload["details"] = details
    try:
        sys.stderr.write(json.dumps(payload, ensure_ascii=False) + "\n")
    except Exception:
        sys.stderr.write(f"{message}\n")
    sys.exit(exit_code)


def _load_request() -> dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        _fail(14, "Worker request is empty")
    try:
        data = json.loads(raw)
    except json.JSONDecodeError as exc:
        _fail(14, f"Invalid worker request JSON: {exc}")
    if not isinstance(data, dict):
        _fail(14, "Worker request must be a JSON object")
    return data


def _build_config(base_config: Any, precision: str) -> Any:
    try:
        cfg = base_config.model_copy(deep=True)  # pydantic v2
    except Exception:
        cfg = copy.deepcopy(base_config)
    cfg.precision = precision
    return cfg


def main() -> None:
    request = _load_request()

    audio_path = str(request.get("audio_path") or "").strip()
    if not audio_path:
        _fail(14, "audio_path is required")

    precision = str(request.get("precision") or "fp16").strip().lower()
    if precision not in {"fp16", "int8"}:
        _fail(14, f"Unsupported precision: {precision}")

    language = str(request.get("language") or "auto").strip() or "auto"
    analysis_backend = str(request.get("analysis_backend") or "vibevoice").strip().lower()
    if analysis_backend != "vibevoice":
        _fail(14, f"Unsupported analysis backend: {analysis_backend}")

    audio_file = Path(audio_path)
    if not audio_file.exists():
        _fail(14, f"Audio file not found: {audio_file}")

    try:
        from config import model_config
        from model_loader import ModelLoader
    except Exception as exc:
        _fail(10, f"Failed to import VibeVoice runtime: {exc}")

    try:
        audio_data = audio_file.read_bytes()
    except Exception as exc:
        _fail(14, f"Failed to read audio file: {exc}")

    start = time.time()
    try:
        cfg = _build_config(model_config, precision)
        loader = ModelLoader(cfg)
        if not loader.is_loaded():
            loader.load_model()
        result = loader.transcribe(audio_data, language=language)
    except Exception as exc:
        message = str(exc)
        if "initialize VibeVoice ASR model" in message or "Failed to load model" in message:
            _fail(11, f"Failed to initialize VibeVoice ASR model: {message}")
        _fail(20, f"Worker transcription failed: {message}")

    metadata = dict(result.get("metadata", {}))
    metadata.setdefault("processing_time", max(0.0, time.time() - start))
    metadata.setdefault("language", language)
    metadata.setdefault("model_precision", precision)
    metadata.setdefault("num_speakers", len({seg.get("speaker", "Speaker_0") for seg in result.get("segments", [])}))

    payload = {
        "status": "success",
        "segments": result.get("segments", []),
        "metadata": metadata,
        "diagnostics": {
            "backend": "vibevoice",
            "worker": "worker_once",
        },
    }
    sys.stdout.write(json.dumps(payload, ensure_ascii=False))
    sys.stdout.flush()


if __name__ == "__main__":
    main()

