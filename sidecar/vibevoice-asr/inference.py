"""
Inference module
Audio preprocessing and transcription logic
"""

import io
import logging
from typing import Optional, Tuple

import numpy as np

logger = logging.getLogger(__name__)


def load_audio_from_bytes(
    audio_bytes: bytes,
    target_sr: int = 16000
) -> Tuple[np.ndarray, int]:
    """
    Load audio from bytes and resample

    Args:
        audio_bytes: Raw audio file bytes (OPUS or WAV)
        target_sr: Target sample rate (default: 16000 Hz)

    Returns:
        Tuple of (audio_array, sample_rate)
    """
    try:
        import soundfile as sf
        import librosa

        # Load audio from bytes
        audio_io = io.BytesIO(audio_bytes)
        audio, sr = sf.read(audio_io)

        # Convert stereo to mono if needed
        if len(audio.shape) > 1:
            audio = np.mean(audio, axis=1)

        # Resample if needed
        if sr != target_sr:
            logger.info(f"Resampling audio from {sr} Hz to {target_sr} Hz")
            audio = librosa.resample(audio, orig_sr=sr, target_sr=target_sr)
            sr = target_sr

        return audio, sr

    except Exception as e:
        logger.error(f"Failed to load audio: {e}")
        raise


def chunk_audio(
    audio: np.ndarray,
    chunk_length_s: int,
    sample_rate: int
) -> list[np.ndarray]:
    """
    Split audio into chunks

    Args:
        audio: Audio array
        chunk_length_s: Chunk length in seconds
        sample_rate: Audio sample rate

    Returns:
        List of audio chunks
    """
    chunk_samples = chunk_length_s * sample_rate
    num_chunks = int(np.ceil(len(audio) / chunk_samples))

    chunks = []
    for i in range(num_chunks):
        start = i * chunk_samples
        end = min((i + 1) * chunk_samples, len(audio))
        chunk = audio[start:end]

        # Pad last chunk if needed
        if len(chunk) < chunk_samples:
            chunk = np.pad(chunk, (0, chunk_samples - len(chunk)))

        chunks.append(chunk)

    logger.info(f"Split audio into {num_chunks} chunks")
    return chunks


def preprocess_audio(
    audio: np.ndarray,
    processor: any,
    sample_rate: int = 16000
) -> dict:
    """
    Preprocess audio for model input

    Args:
        audio: Audio array (numpy)
        processor: HuggingFace processor
        sample_rate: Audio sample rate

    Returns:
        Preprocessed inputs dict
    """
    # TODO: Implement in Block D
    # inputs = processor(
    #     audio,
    #     sampling_rate=sample_rate,
    #     return_tensors="pt"
    # )
    # return inputs

    # Placeholder
    return {"input_features": audio}


def run_inference(
    model: any,
    inputs: dict,
    language: Optional[str] = None
) -> dict:
    """
    Run model inference

    Args:
        model: Loaded model
        inputs: Preprocessed inputs
        language: Optional language code

    Returns:
        Raw model outputs
    """
    # TODO: Implement in Block D
    # import torch
    #
    # with torch.no_grad():
    #     if language and language != "auto":
    #         outputs = model.generate(
    #             inputs["input_features"],
    #             language=language
    #         )
    #     else:
    #         outputs = model.generate(inputs["input_features"])
    #
    # return outputs

    # Placeholder
    return {"transcription": "[Not implemented]"}


def run_speaker_diarization(
    audio: np.ndarray,
    sample_rate: int,
    min_speakers: int = 1,
    max_speakers: int = 10
) -> list[dict]:
    """
    Run speaker diarization on audio

    Args:
        audio: Audio array
        sample_rate: Audio sample rate
        min_speakers: Minimum number of speakers
        max_speakers: Maximum number of speakers

    Returns:
        List of speaker segments with timestamps
    """
    # TODO: Implement in Block D
    # Options:
    # 1. Use pyannote.audio for diarization
    # 2. Use built-in VibeVoice diarization (if available)
    # 3. Simple energy-based VAD for speaker change detection

    # Placeholder: Mock speaker segments
    duration = len(audio) / sample_rate
    segments = [
        {"speaker": "Speaker_0", "start": 0.0, "end": duration / 2},
        {"speaker": "Speaker_1", "start": duration / 2, "end": duration},
    ]

    return segments


def align_transcription_with_speakers(
    transcription_segments: list[dict],
    speaker_segments: list[dict]
) -> list[dict]:
    """
    Align transcription with speaker segments

    Args:
        transcription_segments: Timestamped transcription
        speaker_segments: Speaker diarization results

    Returns:
        Combined segments with speaker labels
    """
    # TODO: Implement proper alignment in Block D
    # For each transcription segment, find overlapping speaker segment
    # Assign speaker label to transcription

    # Placeholder: Simple merge
    aligned = []
    for i, trans_seg in enumerate(transcription_segments):
        # Find speaker at this timestamp
        speaker = "Speaker_0"  # Default
        for speaker_seg in speaker_segments:
            if (
                speaker_seg["start"] <= trans_seg.get("start", 0)
                and speaker_seg["end"] >= trans_seg.get("end", 0)
            ):
                speaker = speaker_seg["speaker"]
                break

        aligned.append({
            "speaker": speaker,
            "start_time": trans_seg.get("start", i * 3.0),
            "end_time": trans_seg.get("end", (i + 1) * 3.0),
            "text": trans_seg.get("text", "[Mock text]")
        })

    return aligned
