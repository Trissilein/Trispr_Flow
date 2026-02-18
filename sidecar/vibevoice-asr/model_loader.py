"""
Model loader module.
Handles model initialization, precision selection, and end-to-end transcription.
"""

from __future__ import annotations

import gc
import importlib
import logging
import os
import re
import sys
from importlib import metadata as importlib_metadata
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
VIBEVOICE_ARCHIVE_URL = "https://github.com/microsoft/VibeVoice/archive/1807b858d4f7dffdd286249a01616c243e488c9e.zip"
MANUAL_SETUP_COMMAND = 'powershell -NoProfile -ExecutionPolicy Bypass -File "setup-vibevoice.ps1"'
TRISPR_DEV_BUILD_ENV = "TRISPR_DEV_BUILD"
VIBEVOICE_LOCAL_SOURCE_OVERRIDE_ENV = "VIBEVOICE_ALLOW_LOCAL_SOURCE"
EXACT_DEPENDENCY_VERSIONS = {
    "transformers": "4.51.3",
    "accelerate": "1.6.0",
}
MIN_DEPENDENCY_VERSIONS = {
    "torch": (2, 2, 0),
    "torchaudio": (2, 2, 0),
}
HF_HUB_MIN_VERSION = (0, 30, 0)
HF_HUB_MAX_EXCLUSIVE = (1, 0, 0)


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
        self._last_native_error: Optional[str] = None

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
        candidates: list[Path] = []
        explicit_source = os.getenv("VIBEVOICE_SOURCE_DIR", "").strip()
        if explicit_source:
            candidates.append(Path(explicit_source).expanduser())

        parents = list(sidecar_dir.parents)
        if len(parents) >= 3:
            candidates.append(parents[2] / "VibeVoice")
        if len(parents) >= 4:
            candidates.append(parents[3] / "VibeVoice")

        candidates.append(Path.cwd() / "VibeVoice")
        return candidates

    @staticmethod
    def _parse_bool_env(value: Optional[str]) -> Optional[bool]:
        if value is None:
            return None
        normalized = value.strip().lower()
        if normalized in {"1", "true", "yes", "on"}:
            return True
        if normalized in {"0", "false", "no", "off"}:
            return False
        return None

    def _is_local_source_discovery_enabled(self) -> bool:
        # Explicit source path is always allowed (operator override).
        if os.getenv("VIBEVOICE_SOURCE_DIR", "").strip():
            return True

        # Manual override has priority over build mode defaults.
        override = self._parse_bool_env(os.getenv(VIBEVOICE_LOCAL_SOURCE_OVERRIDE_ENV))
        if override is not None:
            return override

        # Build-mode default: enabled in dev, disabled in release.
        dev_mode = self._parse_bool_env(os.getenv(TRISPR_DEV_BUILD_ENV))
        if dev_mode is not None:
            return dev_mode

        # Safe default for unknown launch contexts.
        return False

    def _inject_local_vibevoice_path(self) -> None:
        if not self._is_local_source_discovery_enabled():
            logger.info(
                "Local VibeVoice source auto-discovery is disabled. "
                "Set %s=1 or VIBEVOICE_SOURCE_DIR to enable override.",
                VIBEVOICE_LOCAL_SOURCE_OVERRIDE_ENV,
            )
            return

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

    @staticmethod
    def _sanitize_native_model_kwargs(model_kwargs: dict[str, Any]) -> dict[str, Any]:
        """
        Normalize kwargs for native VibeVoice runtime loading.

        Native config serialization expects `torch_dtype` and should never receive
        `dtype` directly.
        """
        native_kwargs = dict(model_kwargs)
        if "dtype" in native_kwargs and "torch_dtype" not in native_kwargs:
            native_kwargs["torch_dtype"] = native_kwargs["dtype"]
        native_kwargs.pop("dtype", None)
        return native_kwargs

    @staticmethod
    def _torch_dtype_to_serializable(value: Any) -> str:
        """
        Convert torch dtype-like values to a JSON-serializable representation.
        """
        if isinstance(value, str):
            return value
        name = getattr(value, "name", None)
        if isinstance(name, str) and name:
            return name
        text = str(value)
        if text.startswith("torch."):
            return text.split(".", 1)[1]
        return text

    def _is_vibevoice_asr_model(self) -> bool:
        name = str(self.config.model_name).lower()
        return "vibevoice-asr" in name

    @staticmethod
    def _vibevoice_runtime_modules() -> tuple[str, ...]:
        return (
            "vibevoice.modular.modeling_vibevoice_asr",
            "vibevoice.processor.vibevoice_asr_processor",
        )

    @staticmethod
    def _parse_version_tuple(version: str) -> Optional[tuple[int, int, int]]:
        matches = re.findall(r"\d+", str(version))
        if not matches:
            return None

        parts = [int(item) for item in matches[:3]]
        while len(parts) < 3:
            parts.append(0)
        return tuple(parts[:3])

    def _dependency_runtime_status(self) -> tuple[bool, str]:
        details: list[str] = []
        ok = True

        for package_name, expected in EXACT_DEPENDENCY_VERSIONS.items():
            try:
                installed = importlib_metadata.version(package_name)
            except importlib_metadata.PackageNotFoundError:
                ok = False
                details.append(f"Package '{package_name}' is not installed (expected {expected}).")
                continue

            if installed != expected:
                ok = False
                details.append(
                    f"{package_name} version mismatch: expected {expected}, found {installed}."
                )
            else:
                details.append(f"{package_name} version OK: {installed}")

        for package_name, minimum in MIN_DEPENDENCY_VERSIONS.items():
            try:
                installed = importlib_metadata.version(package_name)
            except importlib_metadata.PackageNotFoundError:
                ok = False
                details.append(
                    f"Package '{package_name}' is not installed (minimum {minimum[0]}.{minimum[1]}.{minimum[2]})."
                )
                continue

            installed_tuple = self._parse_version_tuple(installed)
            if installed_tuple is None:
                ok = False
                details.append(f"Could not parse {package_name} version: {installed}")
                continue

            if installed_tuple < minimum:
                ok = False
                details.append(
                    f"{package_name} version too old: minimum {minimum[0]}.{minimum[1]}.{minimum[2]}, found {installed}."
                )
            else:
                details.append(f"{package_name} version OK: {installed}")

        try:
            hf_hub_version = importlib_metadata.version("huggingface_hub")
        except importlib_metadata.PackageNotFoundError:
            ok = False
            details.append("Package 'huggingface_hub' is not installed (required >=0.30.0,<1.0.0).")
        else:
            parsed_hf_hub = self._parse_version_tuple(hf_hub_version)
            if parsed_hf_hub is None:
                ok = False
                details.append(f"Could not parse huggingface_hub version: {hf_hub_version}")
            elif parsed_hf_hub < HF_HUB_MIN_VERSION or parsed_hf_hub >= HF_HUB_MAX_EXCLUSIVE:
                ok = False
                details.append(
                    "huggingface_hub version out of supported range "
                    f"(>=0.30.0,<1.0.0): found {hf_hub_version}."
                )
            else:
                details.append(f"huggingface_hub version OK: {hf_hub_version}")

        return ok, "\n".join(details)

    def _vibevoice_runtime_status(self) -> tuple[bool, str]:
        details: list[str] = []
        try:
            version = importlib_metadata.version("vibevoice")
            details.append(f"Installed vibevoice package version: {version}")
            if str(version).startswith("0.0."):
                details.append(
                    "Detected legacy vibevoice package 0.0.x, which does not include ASR runtime modules."
                )
        except importlib_metadata.PackageNotFoundError:
            details.append("Package 'vibevoice' is not installed.")
            return False, "\n".join(details)
        except Exception as exc:
            details.append(f"Could not read vibevoice package metadata: {exc}")

        missing_modules: list[str] = []
        for module_name in self._vibevoice_runtime_modules():
            try:
                importlib.import_module(module_name)
            except Exception as exc:
                missing_modules.append(f"{module_name}: {exc}")

        if missing_modules:
            details.append("Required VibeVoice-ASR modules are unavailable:")
            details.extend([f"- {item}" for item in missing_modules])
            return False, "\n".join(details)

        deps_ok, deps_details = self._dependency_runtime_status()
        if deps_details:
            details.append(deps_details)
        if not deps_ok:
            return False, "\n".join(details)

        return True, "\n".join(details)

    def _format_vibevoice_runtime_error(self, runtime_details: str = "") -> str:
        lines = [
            "VibeVoice-ASR runtime is unavailable or incompatible.",
            f"Model: {self.config.model_name}",
        ]
        if runtime_details:
            lines.append(runtime_details)
        if self._last_native_error:
            lines.append(f"Last native loader error: {self._last_native_error}")
        lines.extend(
            [
                "Run setup-vibevoice.ps1 to install or repair dependencies.",
                "Transformers fallback is disabled for microsoft/VibeVoice-ASR until native runtime modules are healthy.",
                "Run manually:",
                MANUAL_SETUP_COMMAND,
                f"Expected pinned runtime source: {VIBEVOICE_ARCHIVE_URL}",
            ]
        )
        return "\n".join(lines)

    def _load_vibevoice_native(self, model_kwargs: dict[str, Any]) -> bool:
        """
        Try loading the official VibeVoice processor/model classes.
        """
        self._last_native_error = None
        try:
            self._inject_local_vibevoice_path()
            modeling_module = importlib.import_module("vibevoice.modular.modeling_vibevoice_asr")
            processor_module = importlib.import_module("vibevoice.processor.vibevoice_asr_processor")
            VibeVoiceASRForConditionalGeneration = getattr(
                modeling_module, "VibeVoiceASRForConditionalGeneration"
            )
            VibeVoiceASRProcessor = getattr(processor_module, "VibeVoiceASRProcessor")
        except Exception as exc:
            self._last_native_error = f"native import failed: {exc}"
            logger.warning("Native VibeVoice imports unavailable: %s", exc)
            return False

        lm_model = os.getenv("VIBEVOICE_LM_MODEL", "Qwen/Qwen2.5-1.5B")
        try:
            processor = VibeVoiceASRProcessor.from_pretrained(
                self.config.model_name,
                cache_dir=self.config.cache_dir,
                trust_remote_code=True,
                language_model_pretrained_name=lm_model,
            )
        except Exception as exc:
            self._last_native_error = f"processor initialization failed: {exc}"
            raise RuntimeError(f"Failed to initialize VibeVoice ASR processor: {exc}") from exc

        native_kwargs = self._sanitize_native_model_kwargs(model_kwargs)

        try:
            model = VibeVoiceASRForConditionalGeneration.from_pretrained(
                self.config.model_name,
                cache_dir=self.config.cache_dir,
                trust_remote_code=True,
                **native_kwargs,
            )
        except Exception as exc:
            error_text = str(exc)
            is_dtype_json_error = (
                "dtype" in error_text.lower() and "not json serializable" in error_text.lower()
            )
            if is_dtype_json_error and "torch_dtype" in native_kwargs:
                retry_kwargs = dict(native_kwargs)
                retry_kwargs["torch_dtype"] = self._torch_dtype_to_serializable(
                    retry_kwargs["torch_dtype"]
                )
                logger.warning(
                    "Native model load hit non-serializable dtype; retrying with torch_dtype=%s",
                    retry_kwargs["torch_dtype"],
                )
                try:
                    model = VibeVoiceASRForConditionalGeneration.from_pretrained(
                        self.config.model_name,
                        cache_dir=self.config.cache_dir,
                        trust_remote_code=True,
                        **retry_kwargs,
                    )
                except Exception as retry_exc:
                    self._last_native_error = (
                        "model initialization failed after dtype retry: "
                        f"{retry_exc}"
                    )
                    raise RuntimeError(
                        f"Failed to initialize VibeVoice ASR model: {retry_exc}"
                    ) from retry_exc
            else:
                self._last_native_error = f"model initialization failed: {exc}"
                raise RuntimeError(f"Failed to initialize VibeVoice ASR model: {exc}") from exc

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
            runtime_details = ""

            if self._is_vibevoice_asr_model():
                runtime_ok, runtime_details = self._vibevoice_runtime_status()
                if not runtime_ok:
                    raise RuntimeError(self._format_vibevoice_runtime_error(runtime_details))

            loaded = self._load_vibevoice_native(model_kwargs)
            if not loaded:
                if self._is_vibevoice_asr_model():
                    raise RuntimeError(self._format_vibevoice_runtime_error(runtime_details))
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
