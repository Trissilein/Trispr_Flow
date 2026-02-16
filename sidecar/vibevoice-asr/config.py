"""
Configuration module for VibeVoice-ASR sidecar
"""

import os
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, Field

# Precision modes
PrecisionMode = Literal["fp16", "int8"]

# Default configuration
DEFAULT_PRECISION: PrecisionMode = "fp16"
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 8765
DEFAULT_MODEL_NAME = "microsoft/vibevoice-asr-7b"

# Environment variable overrides
PRECISION = os.getenv("VIBEVOICE_PRECISION", DEFAULT_PRECISION)
HOST = os.getenv("VIBEVOICE_HOST", DEFAULT_HOST)
PORT = int(os.getenv("VIBEVOICE_PORT", str(DEFAULT_PORT)))
MODEL_NAME = os.getenv("VIBEVOICE_MODEL_NAME", DEFAULT_MODEL_NAME)

# Model cache directory
MODEL_CACHE_DIR = os.getenv(
    "VIBEVOICE_MODEL_PATH",
    str(Path.home() / ".cache" / "huggingface" / "hub")
)


class ModelConfig(BaseModel):
    """Model configuration"""

    model_name: str = Field(
        default=MODEL_NAME,
        description="HuggingFace model identifier"
    )

    precision: PrecisionMode = Field(
        default=DEFAULT_PRECISION,
        description="Model precision mode (fp16 or int8)"
    )

    cache_dir: str = Field(
        default=MODEL_CACHE_DIR,
        description="Model cache directory"
    )

    device: str = Field(
        default="cuda",
        description="Device for inference (cuda or cpu)"
    )

    # Audio preprocessing
    sample_rate: int = Field(
        default=16000,
        description="Audio sample rate for model input"
    )

    chunk_length_s: int = Field(
        default=30,
        description="Audio chunk length in seconds"
    )

    # Inference parameters
    batch_size: int = Field(
        default=1,
        description="Inference batch size"
    )

    # Speaker diarization
    min_speakers: int = Field(
        default=1,
        description="Minimum number of speakers"
    )

    max_speakers: int = Field(
        default=10,
        description="Maximum number of speakers"
    )


class ServerConfig(BaseModel):
    """Server configuration"""

    host: str = Field(
        default=HOST,
        description="Server host address"
    )

    port: int = Field(
        default=PORT,
        description="Server port"
    )

    workers: int = Field(
        default=1,
        description="Number of worker processes"
    )

    log_level: str = Field(
        default="info",
        description="Logging level"
    )

    cors_enabled: bool = Field(
        default=False,
        description="Enable CORS (only for development)"
    )


# Global config instances
model_config = ModelConfig()
server_config = ServerConfig()


def update_precision(new_precision: PrecisionMode) -> None:
    """Update model precision mode"""
    global model_config
    model_config.precision = new_precision


def get_vram_requirements_mb() -> int:
    """Get estimated VRAM requirements in MB"""
    if model_config.precision == "fp16":
        return 14_000  # ~14 GB for FP16
    else:  # int8
        return 7_000   # ~7 GB for INT8
