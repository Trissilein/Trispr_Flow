from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


class AnalysisRuntimeError(RuntimeError):
    pass


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[2]


def _worker_script() -> Path:
    return _repo_root() / "sidecar" / "vibevoice-asr" / "worker_once.py"


def _python_executable() -> str:
    return sys.executable


def _mock_result(audio_path: str, context_info: str = "") -> dict[str, Any]:
    speaker = "SPEAKER_00"
    snippet = Path(audio_path).name
    text = f"Mock transcription for {snippet}. {context_info}".strip()
    return {
        "status": "success",
        "segments": [
            {
                "speaker": speaker,
                "start_time": 0.0,
                "end_time": 3.8,
                "text": text,
            }
        ],
        "metadata": {
            "duration": 3.8,
            "num_speakers": 1,
            "processing_time": 0.1,
            "language": "auto",
            "model_precision": "fp16",
        },
        "diagnostics": {
            "backend": "mock",
            "worker": "mock",
        },
    }


def transcribe_file(audio_path: str, context_info: str = "") -> dict[str, Any]:
    if os.getenv("TRISPR_ANALYSIS_USE_MOCK", "0") == "1":
        return _mock_result(audio_path, context_info)

    worker = _worker_script()
    if not worker.exists():
        raise AnalysisRuntimeError(
            f"worker_once.py not found at {worker}. Set TRISPR_ANALYSIS_USE_MOCK=1 for UI-only mode."
        )

    payload = {
        "audio_path": audio_path,
        "precision": "fp16",
        "language": "auto",
        "analysis_backend": "vibevoice",
    }

    process = subprocess.run(
        [_python_executable(), str(worker)],
        cwd=str(worker.parent),
        input=json.dumps(payload),
        text=True,
        capture_output=True,
    )

    if process.returncode != 0:
        err = process.stderr.strip() or process.stdout.strip() or "Unknown worker error"
        raise AnalysisRuntimeError(err)

    stdout = process.stdout.strip()
    if not stdout:
        raise AnalysisRuntimeError("Worker returned empty output")

    try:
        result = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise AnalysisRuntimeError(f"Failed to parse worker output: {exc}") from exc

    if result.get("status") != "success":
        raise AnalysisRuntimeError(str(result.get("error") or "Worker returned non-success status"))

    return result
