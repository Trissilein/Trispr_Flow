# Architecture

## High-Level
1. UI receives audio files.
2. Analysis API creates jobs.
3. Worker executes VibeVoice pipeline.
4. Results are normalized and persisted.
5. UI renders timeline + segment list and supports export.

## Components
- `web-ui-template`: frontend shell and interaction model
- `analysis-api` (planned): job orchestration and result serving
- `analysis-worker` (planned): VibeVoice execution and result normalization
- `storage` (planned): artifacts + analysis metadata

## API Contract (Draft)

### POST `/api/v1/analyze`
Request:
```json
{
  "audio_path": "C:/recordings/session.opus",
  "options": {
    "language": "auto",
    "speaker_diarization": true
  }
}
```

Response:
```json
{
  "job_id": "job_123",
  "status": "queued"
}
```

### GET `/api/v1/analyze/{job_id}`
Response:
```json
{
  "job_id": "job_123",
  "status": "completed",
  "analysis": {
    "duration_s": 135,
    "total_speakers": 3,
    "segments": [
      {
        "speaker_id": "SPEAKER_00",
        "speaker_label": "Speaker 1",
        "start_time": 0.0,
        "end_time": 8.4,
        "text": "Welcome everyone."
      }
    ]
  }
}
```

## Compatibility Notes
- Schema mirrors Trispr Flow historical analysis payload to simplify future import/export.
- Runtime remains independently deployable.
