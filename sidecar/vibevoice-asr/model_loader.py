"""
Model loader module.
Handles model initialization, precision selection, and end-to-end transcription.
"""

from __future__ import annotations

import gc
import logging
import os
import sys
from pathlib import Path
from typing import Any, Optional

from config import ModelConfig, PrecisionMode
from inference import (
    align_transcription_with_speakers,
    load_audio_from_bytes,
    normalize_transcription_segments,
    preprocess_audio,
    run_inference,
    run_speaker_diarization,
)

logger = logging.getLogger(__name__)


def get_gpu_info() -> dict[str, Any]:
    """
    Get GPU information (VRAM usage, availability).
    """
    try:
        import torch

        if not torch.cuda.is_available():
            return {"available": False}

        device_name = torch.cuda.get_device_name(0)
        vram_total = torch.cuda.get_device_properties(0).total_memory / (1024**2)
        vram_allocated = torch.cuda.memory_allocated(0) / (1024**2)

        return {
            "available": True,
            "vram_used_mb": int(vram_allocated),
            "vram_total_mb": int(vram_total),
            "device_name": device_name,
        }
    except ImportError:
        logger.warning("PyTorch not installed, GPU info unavailable")
        return {"available": False}
    except Exception as exc:
        logger.error("Failed to get GPU info: %s", exc)
        return {"available": False}


class ModelLoader:
    """
    Lazy model loader for VibeVoice-ASR and compatible HF checkpoints.
    """

    def __init__(self, config: ModelConfig):
        self.config = config
        self.model: Optional[Any] = None
        self.processor: Optional[Any] = None
        self._loaded = False
        self._backend: str = "unknown"
        self._actual_precision: str = config.precision
        self._target_sample_rate: int = config.sample_rate

        logger.info("ModelLoader initialized with config: %s", config)

        gpu_info = get_gpu_info()
        if not gpu_info.get("available", False):
            logger.warning("No GPU detected; forcing CPU mode (inference will be slow)")
            self.config.device = "cpu"

    def is_loaded(self) -> bool:
        return self._loaded

    def _torch_dtype_for_precision(self, precision: str) -> Any:
        import torch

        if precision == "fp16" and self.config.device == "cuda":
            return torch.float16
        return torch.float32

    def _candidate_local_vibevoice_dirs(self) -> list[Path]:
        sidecar_dir = Path(__file__).resolve().parent
        candidates = [Path(os.getenv("VIBEVOICE_SOURCE_DIR", "")).expanduser()]

        parents = list(sidecar_dir.parents)
        if len(parents) >= 3:
            candidates.append(parents[2] / "VibeVoice")
        if len(parents) >= 4:
            candidates.append(parents[3] / "VibeVoice")

        candidates.append(Path.cwd() / "VibeVoice")
        return candidates

    def _inject_local_vibevoice_path(self) -> None:
        for candidate in self._candidate_local_vibevoice_dirs():
            if not candidate or str(candidate) == ".":
                continue
            package_init = candidate / "vibevoice" / "__init__.py"
            if package_init.exists():
                candidate_str = str(candidate)
                if candidate_str not in sys.path:
                    sys.path.insert(0, candidate_str)
                    logger.info("Added local VibeVoice source to PYTHONPATH: %s", candidate_str)
                return

    def _build_model_load_kwargs(self) -> tuple[dict[str, Any], str]:
        """
        Build kwargs for from_pretrained and return (kwargs, actual_precision).
        """
        kwargs: dict[str, Any] = {}
        requested = self.config.precision
        actual = requested

        if requested == "int8":
            if self.config.device != "cuda":
                logger.warning("INT8 requested without CUDA; falling back to float32 on CPU")
                actual = "fp32"
            else:
                try:
                    from transformers import BitsAndBytesConfig

                    kwargs["quantization_config"] = BitsAndBytesConfig(load_in_8bit=True)
                    kwargs["device_map"] = "auto"
                    actual = "int8"
                except Exception as exc:
                    logger.warning("INT8 unavailable (%s). Falling back to fp16", exc)
                    actual = "fp16"

        if actual != "int8":
            kwargs["torch_dtype"] = self._torch_dtype_for_precision(actual)

        return kwargs, actual

    def _is_vibevoice_asr_model(self) -> bool:
        name = str(self.config.model_name).lower()
        return "vibevoice-asr" in name

    def _load_vibevoice_native(self, model_kwargs: dict[str, Any]) -> bool:
        """
        Try loading the official VibeVoice processor/model classes.
        """
        try:
            self._inject_local_vibevoice_path()
            from vibevoice.modular.modeling_vibevoice_asr import (
                VibeVoiceASRForConditionalGeneration,
            )
            from vibevoice.processor.vibevoice_asr_processor import VibeVoiceASRProcessor
        except Exception as exc:
            logger.warning("Native VibeVoice imports unavailable: %s", exc)
            return False

        lm_model = os.getenv("VIBEVOICE_LM_MODEL", "Qwen/Qwen2.5-1.5B")
        processor = VibeVoiceASRProcessor.from_pretrained(
            self.config.model_name,
            cache_dir=self.config.cache_dir,
            trust_remote_code=True,
            language_model_pretrained_name=lm_model,
        )

        native_kwargs = dict(model_kwargs)
        if "torch_dtype" in native_kwargs:
            native_kwargs["dtype"] = native_kwargs.pop("torch_dtype")

        model = VibeVoiceASRForConditionalGeneration.from_pretrained(
            self.config.model_name,
            cache_dir=self.config.cache_dir,
            trust_remote_code=True,
            **native_kwargs,
        )

        if self._actual_precision != "int8":
            model = model.to(self.config.device)
        model.eval()

        self.processor = processor
        self.model = model
        self._backend = "vibevoice-native"
        return True

    def _load_transformers_fallback(self, model_kwargs: dict[str, Any]) -> None:
        """
        Fallback path using HuggingFace Auto classes + trust_remote_code.
        """
        from transformers import AutoModel, AutoModelForCausalLM, AutoModelForSpeechSeq2Seq, AutoProcessor

        processor = AutoProcessor.from_pretrained(
            self.config.model_name,
            cache_dir=self.config.cache_dir,
            trust_remote_code=True,
        )

        model_loaders = [
            ("AutoModelForCausalLM", AutoModelForCausalLM.from_pretrained),
            ("AutoModelForSpeechSeq2Seq", AutoModelForSpeechSeq2Seq.from_pretrained),
            ("AutoModel", AutoModel.from_pretrained),
        ]

        model: Optional[Any] = None
        last_error: Optional[Exception] = None
        for loader_name, loader_fn in model_loaders:
            try:
                model = loader_fn(
                    self.config.model_name,
                    cache_dir=self.config.cache_dir,
                    trust_remote_code=True,
                    **model_kwargs,
                )
                self._backend = f"transformers-{loader_name}"
                break
            except Exception as exc:
                last_error = exc
                logger.info("%s failed: %s", loader_name, exc)

        if model is None:
            raise RuntimeError(
                "Failed to load model with all fallback loaders"
                + (f": {last_error}" if last_error else "")
            )

        if self._actual_precision != "int8":
            model = model.to(self.config.device)
        model.eval()

        self.processor = processor
        self.model = model

    def _resolve_processor_sample_rate(self) -> int:
        if not self.processor:
            return self.config.sample_rate

        for attr in ("target_sample_rate", "sampling_rate"):
            value = getattr(self.processor, attr, None)
            if isinstance(value, (int, float)) and int(value) > 0:
                return int(value)
        return self.config.sample_rate

    def load_model(self) -> None:
        if self._loaded:
            logger.info("Model already loaded")
            return

        logger.info("Loading model: %s", self.config.model_name)
        logger.info("Requested precision=%s device=%s", self.config.precision, self.config.device)

        try:
            model_kwargs, actual_precision = self._build_model_load_kwargs()
            self._actual_precision = actual_precision

            loaded = self._load_vibevoice_native(model_kwargs)
            if not loaded:
                if self._is_vibevoice_asr_model():
                    raise RuntimeError(
                        "Native VibeVoice runtime is unavailable. "
                        "microsoft/VibeVoice-ASR does not provide a generic AutoProcessor fallback. "
                        "Run setup-vibevoice.ps1 to install sidecar dependencies and ensure local "
                        "VibeVoice source is available."
                    )
                self._load_transformers_fallback(model_kwargs)

            if not self.model or not self.processor:
                raise RuntimeError("Model or processor failed to initialize")

            self._target_sample_rate = self._resolve_processor_sample_rate()
            self._loaded = True

            logger.info(
                "Model loaded successfully via %s (actual precision: %s, sample_rate: %s)",
                self._backend,
                self._actual_precision,
                self._target_sample_rate,
            )
        except Exception as exc:
            logger.error("Failed to load model: %s", exc)
            self.model = None
            self.processor = None
            self._loaded = False
            raise

    def unload_model(self) -> None:
        if not self._loaded:
            return

        logger.info("Unloading model...")
        try:
            self.model = None
            self.processor = None
            self._loaded = False
            self._backend = "unknown"
            gc.collect()

            try:
                import torch

                if torch.cuda.is_available():
                    torch.cuda.empty_cache()
            except Exception:
                pass

            logger.info("Model unloaded successfully")
        except Exception as exc:
            logger.error("Failed to unload model: %s", exc)

    def reload_model(self, precision: PrecisionMode) -> None:
        logger.info("Reloading model with precision: %s", precision)
        self.unload_model()
        self.config.precision = precision
        self.load_model()

    @staticmethod
    def _segments_need_external_diarization(segments: list[dict[str, Any]]) -> bool:
        if len(segments) <= 1:
            return False

        diarization_fallback = os.getenv("VIBEVOICE_ENABLE_DIARIZATION_FALLBACK", "1").lower()
        if diarization_fallback not in {"1", "true", "yes"}:
            return False

        speakers = {str(seg.get("speaker", "")).strip() for seg in segments}
        speakers.discard("")

        if len(speakers) > 1:
            return False

        if not speakers:
            return True

        only_speaker = next(iter(speakers))
        return only_speaker.lower() in {"speaker_0", "speaker0", "speaker_00", "unknown"}

    @staticmethod
    def _finalize_segments(
        segments: list[dict[str, Any]],
        duration: float,
    ) -> list[dict[str, Any]]:
        if not segments:
            return []

        normalized = normalize_transcription_segments(segments, fallback_duration=duration)
        if not normalized:
            return []

        for idx, seg in enumerate(normalized):
            seg["start_time"] = max(0.0, float(seg["start_time"]))
            seg["end_time"] = max(seg["start_time"] + 0.05, float(seg["end_time"]))
            if duration > 0:
                seg["start_time"] = min(seg["start_time"], duration)
                seg["end_time"] = min(seg["end_time"], duration)
            if idx > 0 and seg["start_time"] < normalized[idx - 1]["end_time"]:
                seg["start_time"] = normalized[idx - 1]["end_time"]
                if seg["end_time"] <= seg["start_time"]:
                    seg["end_time"] = min(seg["start_time"] + 0.1, duration or seg["start_time"] + 0.1)

        return normalized

    def transcribe(
        self,
        audio_data: bytes,
        language: str = "auto",
    ) -> dict[str, Any]:
        if not self._loaded:
            logger.info("Model not loaded yet; loading lazily")
            self.load_model()

        if not self.model or not self.processor:
            raise RuntimeError("Model or processor is not available")

        logger.info("Transcribing %s bytes of audio", len(audio_data))

        audio, sample_rate = load_audio_from_bytes(
            audio_data,
            target_sr=self._target_sample_rate,
        )
        duration = len(audio) / sample_rate if sample_rate > 0 else 0.0

        inputs = preprocess_audio(
            audio,
            self.processor,
            sample_rate=sample_rate,
            language=language,
        )
        inference_result = run_inference(
            self.model,
            self.processor,
            inputs,
            language=language,
        )

        segments = normalize_transcription_segments(
            inference_result.get("segments", []),
            fallback_duration=duration,
        )

        if not segments:
            raw_text = str(inference_result.get("raw_text") or "").strip()
            if raw_text:
                segments = [
                    {
                        "speaker": "Speaker_0",
                        "start_time": 0.0,
                        "end_time": max(duration, 0.1),
                        "text": raw_text,
                    }
                ]

        if self._segments_need_external_diarization(segments):
            diarization_segments = run_speaker_diarization(
                audio,
                sample_rate,
                min_speakers=self.config.min_speakers,
                max_speakers=self.config.max_speakers,
            )
            segments = align_transcription_with_speakers(segments, diarization_segments)

        segments = self._finalize_segments(segments, duration)
        speaker_count = len({seg["speaker"] for seg in segments}) if segments else 0

        detected_language = str(inference_result.get("language") or "").strip()
        if not detected_language or detected_language == "auto":
            detected_language = language if language and language != "auto" else "auto"

        return {
            "segments": segments,
            "metadata": {
                "duration": duration,
                "language": detected_language,
                "num_speakers": speaker_count,
                "model_precision": self._actual_precision,
                "backend": self._backend,
            },
        }
