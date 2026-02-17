"""
Inference module.
Audio preprocessing, model generation, diarization fallback, and alignment logic.
"""

from __future__ import annotations

import inspect
import io
import json
import logging
import math
import os
import re
import tempfile
from pathlib import Path
from typing import Any, Optional, Tuple

import numpy as np

logger = logging.getLogger(__name__)

_FLOAT_TIME_REGEX = re.compile(r"[-+]?\d*\.?\d+")


def _supports_argument(fn: Any, arg_name: str) -> bool:
    try:
        return arg_name in inspect.signature(fn).parameters
    except (TypeError, ValueError):
        return False


def _to_mono_float32(audio: np.ndarray) -> np.ndarray:
    arr = np.asarray(audio)
    if arr.ndim > 1:
        arr = arr.mean(axis=1)
    arr = arr.astype(np.float32, copy=False)
    if arr.size == 0:
        return arr
    max_abs = float(np.max(np.abs(arr)))
    if max_abs > 1.0:
        arr = arr / max_abs
    return np.clip(arr, -1.0, 1.0)


def _load_audio_with_pydub(audio_bytes: bytes) -> Tuple[np.ndarray, int]:
    from pydub import AudioSegment

    segment = AudioSegment.from_file(io.BytesIO(audio_bytes))
    sample_rate = int(segment.frame_rate)
    channels = int(segment.channels)
    sample_width = int(segment.sample_width)

    raw = np.array(segment.get_array_of_samples())
    if channels > 1:
        raw = raw.reshape((-1, channels)).mean(axis=1)

    scale = float(1 << (8 * sample_width - 1))
    audio = (raw.astype(np.float32) / max(scale, 1.0)).astype(np.float32)
    return audio, sample_rate


def load_audio_from_bytes(
    audio_bytes: bytes,
    target_sr: int = 16000,
) -> Tuple[np.ndarray, int]:
    """
    Decode bytes (WAV/OPUS/MP3/...) to mono float32 and resample.
    """
    if not audio_bytes:
        raise ValueError("Audio payload is empty")

    audio: Optional[np.ndarray] = None
    sr: Optional[int] = None
    decode_errors: list[str] = []

    try:
        import soundfile as sf

        audio_io = io.BytesIO(audio_bytes)
        decoded, decoded_sr = sf.read(audio_io, always_2d=False)
        audio = _to_mono_float32(np.asarray(decoded))
        sr = int(decoded_sr)
    except Exception as exc:
        decode_errors.append(f"soundfile: {exc}")

    if audio is None or sr is None:
        try:
            audio, sr = _load_audio_with_pydub(audio_bytes)
            audio = _to_mono_float32(audio)
        except Exception as exc:
            decode_errors.append(f"pydub: {exc}")

    if audio is None or sr is None:
        detail = "; ".join(decode_errors) if decode_errors else "unknown decode error"
        raise RuntimeError(f"Failed to decode audio bytes ({detail})")

    if target_sr > 0 and sr != target_sr:
        import librosa

        logger.info("Resampling audio from %s Hz to %s Hz", sr, target_sr)
        audio = librosa.resample(audio, orig_sr=sr, target_sr=target_sr)
        audio = _to_mono_float32(audio)
        sr = target_sr

    if audio.size == 0:
        raise RuntimeError("Decoded audio is empty")

    return audio, sr


def chunk_audio(
    audio: np.ndarray,
    chunk_length_s: int,
    sample_rate: int,
) -> list[np.ndarray]:
    chunk_samples = max(int(chunk_length_s * sample_rate), 1)
    num_chunks = int(np.ceil(len(audio) / chunk_samples))

    chunks: list[np.ndarray] = []
    for i in range(num_chunks):
        start = i * chunk_samples
        end = min((i + 1) * chunk_samples, len(audio))
        chunk = audio[start:end]
        if len(chunk) < chunk_samples:
            chunk = np.pad(chunk, (0, chunk_samples - len(chunk)))
        chunks.append(chunk)

    logger.info("Split audio into %s chunk(s)", num_chunks)
    return chunks


def preprocess_audio(
    audio: np.ndarray,
    processor: Any,
    sample_rate: int = 16000,
    language: Optional[str] = None,
) -> dict[str, Any]:
    """
    Build model inputs through the processor with broad compatibility
    across VibeVoice custom processors and HF AutoProcessor variants.
    """
    kwargs: dict[str, Any] = {"return_tensors": "pt", "padding": True}
    if _supports_argument(processor.__call__, "add_generation_prompt"):
        kwargs["add_generation_prompt"] = True
    if language and language != "auto" and _supports_argument(processor.__call__, "language"):
        kwargs["language"] = language

    call_attempts = [
        lambda: processor(audio=audio, sampling_rate=sample_rate, **kwargs),
        lambda: processor(audio, sampling_rate=sample_rate, **kwargs),
        lambda: processor(audio=audio, **kwargs),
        lambda: processor(audio, **kwargs),
    ]

    last_error: Optional[Exception] = None
    for attempt in call_attempts:
        try:
            processed = attempt()
            if isinstance(processed, dict):
                return processed
            if hasattr(processed, "items"):
                return dict(processed.items())
            return dict(processed)
        except TypeError as exc:
            last_error = exc
            continue

    raise RuntimeError(
        "Processor preprocessing failed for all call signatures"
        + (f": {last_error}" if last_error else "")
    )


def _safe_parse_time(value: Any) -> Optional[float]:
    if value is None:
        return None

    if isinstance(value, (int, float)):
        return float(value)

    text = str(value).strip()
    if not text:
        return None

    if ":" in text:
        parts = text.split(":")
        try:
            if len(parts) == 3:
                h, m, s = parts
                return float(h) * 3600 + float(m) * 60 + float(s)
            if len(parts) == 2:
                m, s = parts
                return float(m) * 60 + float(s)
        except ValueError:
            pass

    match = _FLOAT_TIME_REGEX.search(text)
    if match:
        try:
            return float(match.group(0))
        except ValueError:
            return None
    return None


def _extract_json_candidate(text: str) -> Optional[str]:
    if not text:
        return None

    fence_idx = text.find("```json")
    if fence_idx >= 0:
        start = fence_idx + len("```json")
        end = text.find("```", start)
        if end > start:
            return text[start:end].strip()

    for opener, closer in (("[", "]"), ("{", "}")):
        start = text.find(opener)
        if start < 0:
            continue
        depth = 0
        for idx in range(start, len(text)):
            char = text[idx]
            if char == opener:
                depth += 1
            elif char == closer:
                depth -= 1
                if depth == 0:
                    return text[start : idx + 1].strip()
    return None


def _parse_structured_segments_from_text(text: str) -> list[dict[str, Any]]:
    candidate = _extract_json_candidate(text)
    if not candidate:
        return []

    try:
        parsed = json.loads(candidate)
    except json.JSONDecodeError:
        return []

    if isinstance(parsed, dict):
        if isinstance(parsed.get("segments"), list):
            parsed = parsed["segments"]
        else:
            parsed = [parsed]

    if not isinstance(parsed, list):
        return []

    return [entry for entry in parsed if isinstance(entry, dict)]


def normalize_transcription_segments(
    transcription_segments: list[dict[str, Any]],
    fallback_duration: Optional[float] = None,
) -> list[dict[str, Any]]:
    normalized: list[dict[str, Any]] = []

    for idx, seg in enumerate(transcription_segments):
        text = str(
            seg.get("text")
            or seg.get("Content")
            or seg.get("content")
            or seg.get("transcript")
            or ""
        ).strip()
        if not text:
            continue

        speaker = str(
            seg.get("speaker")
            or seg.get("speaker_id")
            or seg.get("Speaker ID")
            or seg.get("Speaker")
            or "Speaker_0"
        ).strip()

        start = (
            _safe_parse_time(seg.get("start_time"))
            or _safe_parse_time(seg.get("start"))
            or _safe_parse_time(seg.get("Start time"))
            or _safe_parse_time(seg.get("Start"))
        )
        end = (
            _safe_parse_time(seg.get("end_time"))
            or _safe_parse_time(seg.get("end"))
            or _safe_parse_time(seg.get("End time"))
            or _safe_parse_time(seg.get("End"))
        )

        if start is None:
            if normalized:
                start = float(normalized[-1]["end_time"])
            else:
                start = float(idx) * 2.0

        if end is None:
            token_estimate = max(len(text.split()), 1)
            end = start + max(0.6, token_estimate * 0.35)

        if end <= start:
            end = start + 0.5

        normalized.append(
            {
                "speaker": speaker or "Speaker_0",
                "start_time": float(start),
                "end_time": float(end),
                "text": text,
            }
        )

    normalized.sort(key=lambda seg: seg["start_time"])
    for idx, seg in enumerate(normalized):
        if idx > 0 and seg["start_time"] < normalized[idx - 1]["end_time"]:
            seg["start_time"] = normalized[idx - 1]["end_time"]
        if seg["end_time"] <= seg["start_time"]:
            seg["end_time"] = seg["start_time"] + 0.5

    if fallback_duration and fallback_duration > 0 and normalized:
        max_end = max(seg["end_time"] for seg in normalized)
        if max_end > fallback_duration and max_end > 0:
            ratio = fallback_duration / max_end
            for seg in normalized:
                seg["start_time"] *= ratio
                seg["end_time"] *= ratio

    return normalized


def _resolve_tensor_device(model: Any) -> Optional[Any]:
    try:
        import torch

        if isinstance(model, torch.nn.Module):
            return next(model.parameters()).device
    except (ImportError, StopIteration, AttributeError):
        return None
    return None


def _move_inputs_to_device(inputs: dict[str, Any], device: Any) -> dict[str, Any]:
    if device is None:
        return inputs

    try:
        import torch
    except ImportError:
        return inputs

    moved: dict[str, Any] = {}
    for key, value in inputs.items():
        if isinstance(value, torch.Tensor):
            moved[key] = value.to(device)
        else:
            moved[key] = value
    return moved


def _resolve_pad_token_id(processor: Any, model: Any) -> Optional[int]:
    candidates = [
        getattr(processor, "pad_id", None),
        getattr(getattr(processor, "tokenizer", None), "pad_token_id", None),
        getattr(getattr(model, "config", None), "pad_token_id", None),
    ]
    for value in candidates:
        if value is not None:
            try:
                return int(value)
            except (TypeError, ValueError):
                continue
    return None


def _resolve_eos_token_id(processor: Any, model: Any) -> Optional[int]:
    candidates = [
        getattr(getattr(processor, "tokenizer", None), "eos_token_id", None),
        getattr(getattr(model, "config", None), "eos_token_id", None),
    ]
    for value in candidates:
        if value is not None:
            try:
                return int(value)
            except (TypeError, ValueError):
                continue
    return None


def _decode_generated_text(processor: Any, generated_ids: Any) -> str:
    decode_fn = None
    if hasattr(processor, "decode"):
        decode_fn = processor.decode
    elif hasattr(processor, "tokenizer") and hasattr(processor.tokenizer, "decode"):
        decode_fn = processor.tokenizer.decode
    if decode_fn is None:
        raise RuntimeError("Processor does not provide decode()")

    try:
        import torch

        if isinstance(generated_ids, torch.Tensor) and generated_ids.ndim > 1:
            generated_ids = generated_ids[0]
    except ImportError:
        pass

    return str(decode_fn(generated_ids, skip_special_tokens=True)).strip()


def run_inference(
    model: Any,
    processor: Any,
    inputs: dict[str, Any],
    language: Optional[str] = None,
) -> dict[str, Any]:
    """
    Run generation on the loaded model and parse diarized transcription segments.
    """
    try:
        import torch
    except ImportError as exc:
        raise RuntimeError("PyTorch is required for inference") from exc

    if not hasattr(model, "generate"):
        raise RuntimeError("Loaded model does not support generate()")

    model_inputs = _move_inputs_to_device(inputs, _resolve_tensor_device(model))

    generation_kwargs: dict[str, Any] = {
        "max_new_tokens": int(os.getenv("VIBEVOICE_MAX_NEW_TOKENS", "1024")),
        "do_sample": os.getenv("VIBEVOICE_DO_SAMPLE", "0").lower() in {"1", "true", "yes"},
    }

    if generation_kwargs["do_sample"]:
        generation_kwargs["top_p"] = float(os.getenv("VIBEVOICE_TOP_P", "0.9"))
        generation_kwargs["temperature"] = float(os.getenv("VIBEVOICE_TEMPERATURE", "0.2"))

    pad_id = _resolve_pad_token_id(processor, model)
    eos_id = _resolve_eos_token_id(processor, model)
    if pad_id is not None:
        generation_kwargs["pad_token_id"] = pad_id
    if eos_id is not None:
        generation_kwargs["eos_token_id"] = eos_id

    if language and language != "auto":
        if _supports_argument(model.generate, "language"):
            generation_kwargs["language"] = language
        elif hasattr(processor, "get_decoder_prompt_ids"):
            try:
                forced_ids = processor.get_decoder_prompt_ids(language=language, task="transcribe")
                if forced_ids:
                    generation_kwargs["forced_decoder_ids"] = forced_ids
            except Exception as exc:
                logger.debug("Language prompt setup failed: %s", exc)

    with torch.no_grad():
        output_ids = model.generate(**model_inputs, **generation_kwargs)

    generated_ids = output_ids
    if "input_ids" in model_inputs and hasattr(model_inputs["input_ids"], "shape"):
        prompt_len = int(model_inputs["input_ids"].shape[1])
        if hasattr(output_ids, "shape") and len(output_ids.shape) == 2 and output_ids.shape[1] > prompt_len:
            generated_ids = output_ids[:, prompt_len:]

    generated_text = _decode_generated_text(processor, generated_ids)

    raw_segments: list[dict[str, Any]] = []
    if hasattr(processor, "post_process_transcription"):
        try:
            parsed = processor.post_process_transcription(generated_text)
            if isinstance(parsed, list):
                raw_segments = [seg for seg in parsed if isinstance(seg, dict)]
        except Exception as exc:
            logger.warning("Processor post_process_transcription failed: %s", exc)

    if not raw_segments:
        raw_segments = _parse_structured_segments_from_text(generated_text)

    normalized_segments = normalize_transcription_segments(raw_segments)
    return {
        "raw_text": generated_text,
        "segments": normalized_segments,
    }


def _single_speaker_segments(duration: float) -> list[dict[str, Any]]:
    safe_duration = max(duration, 0.1)
    return [{"speaker": "Speaker_0", "start": 0.0, "end": safe_duration}]


def _normalize_speaker_segments(speaker_segments: list[dict[str, Any]]) -> list[dict[str, Any]]:
    normalized: list[dict[str, Any]] = []
    for seg in speaker_segments:
        speaker = str(seg.get("speaker") or "Speaker_0").strip() or "Speaker_0"
        start = _safe_parse_time(seg.get("start"))
        end = _safe_parse_time(seg.get("end"))
        if start is None or end is None:
            continue
        if end <= start:
            continue
        normalized.append({"speaker": speaker, "start": float(start), "end": float(end)})

    normalized.sort(key=lambda seg: seg["start"])
    merged: list[dict[str, Any]] = []
    for seg in normalized:
        if (
            merged
            and merged[-1]["speaker"] == seg["speaker"]
            and abs(merged[-1]["end"] - seg["start"]) < 0.05
        ):
            merged[-1]["end"] = seg["end"]
        else:
            merged.append(seg)
    return merged


def run_speaker_diarization(
    audio: np.ndarray,
    sample_rate: int,
    min_speakers: int = 1,
    max_speakers: int = 10,
) -> list[dict[str, Any]]:
    """
    Run diarization via pyannote when available.
    Falls back to a single-speaker segment when pyannote is unavailable.
    """
    duration = len(audio) / sample_rate if sample_rate > 0 else 0.0
    fallback = _single_speaker_segments(duration)

    diarization_enabled = os.getenv("VIBEVOICE_ENABLE_PYANNOTE", "1").lower() in {"1", "true", "yes"}
    if not diarization_enabled:
        return fallback

    token = os.getenv("HF_TOKEN") or os.getenv("HUGGINGFACE_TOKEN")
    if not token:
        logger.info("Skipping pyannote diarization: missing HF token")
        return fallback

    model_id = os.getenv("VIBEVOICE_DIARIZATION_MODEL", "pyannote/speaker-diarization-3.1")
    temp_path: Optional[Path] = None

    try:
        import soundfile as sf
        import torch
        from pyannote.audio import Pipeline

        pipeline = Pipeline.from_pretrained(model_id, use_auth_token=token)
        if torch.cuda.is_available():
            pipeline.to(torch.device("cuda"))

        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as temp_file:
            temp_path = Path(temp_file.name)
        sf.write(str(temp_path), audio, sample_rate)

        diarization = pipeline(
            str(temp_path),
            min_speakers=min_speakers,
            max_speakers=max_speakers,
        )

        segments: list[dict[str, Any]] = []
        for turn, _, speaker in diarization.itertracks(yield_label=True):
            segments.append(
                {
                    "speaker": str(speaker),
                    "start": float(turn.start),
                    "end": float(turn.end),
                }
            )

        normalized = _normalize_speaker_segments(segments)
        if normalized:
            return normalized

        return fallback
    except ImportError:
        logger.info("pyannote.audio not installed; using single-speaker fallback")
        return fallback
    except Exception as exc:
        logger.warning("Diarization failed, using single-speaker fallback: %s", exc)
        return fallback
    finally:
        if temp_path and temp_path.exists():
            try:
                temp_path.unlink()
            except OSError:
                pass


def _overlap(a_start: float, a_end: float, b_start: float, b_end: float) -> float:
    return max(0.0, min(a_end, b_end) - max(a_start, b_start))


def _pick_best_speaker(
    seg_start: float,
    seg_end: float,
    speaker_segments: list[dict[str, Any]],
) -> str:
    best_speaker = "Speaker_0"
    best_overlap = -1.0
    seg_center = (seg_start + seg_end) / 2.0
    best_distance = float("inf")

    for speaker_seg in speaker_segments:
        ov = _overlap(seg_start, seg_end, speaker_seg["start"], speaker_seg["end"])
        center = (speaker_seg["start"] + speaker_seg["end"]) / 2.0
        dist = abs(seg_center - center)
        if ov > best_overlap or (math.isclose(ov, best_overlap) and dist < best_distance):
            best_overlap = ov
            best_distance = dist
            best_speaker = speaker_seg["speaker"]

    return best_speaker


def align_transcription_with_speakers(
    transcription_segments: list[dict[str, Any]],
    speaker_segments: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """
    Align timestamped transcription with diarization segments by max-overlap.
    """
    normalized_transcription = normalize_transcription_segments(transcription_segments)
    if not normalized_transcription:
        return []

    normalized_speakers = _normalize_speaker_segments(speaker_segments)
    if not normalized_speakers:
        duration = max(seg["end_time"] for seg in normalized_transcription)
        normalized_speakers = _single_speaker_segments(duration)

    aligned: list[dict[str, Any]] = []
    for seg in normalized_transcription:
        speaker = _pick_best_speaker(seg["start_time"], seg["end_time"], normalized_speakers)
        aligned.append(
            {
                "speaker": speaker,
                "start_time": float(seg["start_time"]),
                "end_time": float(seg["end_time"]),
                "text": seg["text"],
            }
        )
    return aligned
