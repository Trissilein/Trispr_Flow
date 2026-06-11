# Analysis Module Specification

Last updated: 2026-02-20
Status: Source-of-truth draft for implementation

## 1. Purpose

Build a standalone analysis product line (separate from Trispr Flow mainline) for speaker-aware audio analysis and structured transcript export.

Trispr Flow mainline only keeps a placeholder `Analyse` button. All real analysis behavior lives here.

## 2. Product Goals

1. Analyze uploaded/local audio files with speaker-aware output.
2. Render an editable speaker timeline and segment list.
3. Export cleaned results in TXT, Markdown, and JSON.
4. Support queue-based batch processing.
5. Keep runtime decoupled from Trispr Flow.

## 3. Non-Goals

1. No live microphone capture.
2. No system-audio monitoring controls.
3. No shared runtime process with Trispr Flow.
4. No hard dependency on cloud inference.

## 4. Target Users

1. Power users transcribing meetings/interviews.
2. Teams reviewing speaker-attributed transcripts.
3. Internal devs building separate analysis releases.

## 5. End-to-End User Flow

1. User opens Analysis Module UI.
2. User uploads/selects one or more audio files.
3. System creates analysis jobs and queues them.
4. Worker processes each file with speaker-aware runtime.
5. UI shows live job progress and per-stage status.
6. User reviews timeline + segments.
7. User optionally edits speaker labels and text.
8. User exports TXT/MD/JSON.

## 6. Functional Requirements

### 6.1 Input
- Accept `.wav`, `.mp3`, `.m4a`, `.opus`.
- Single-file and batch submission.
- Validate file availability and basic format constraints.

### 6.2 Processing
- Queue model with states: `queued`, `running`, `completed`, `failed`, `canceled`.
- Speaker-aware inference enabled by default.
- Deterministic normalization to shared JSON output schema.

### 6.3 Review UI
- Timeline view with speaker blocks and timestamps.
- Segment list with speaker ID/label, start/end, transcript text.
- Inline rename of speaker labels.
- Basic text correction per segment.

### 6.4 Export
- TXT: readable, linear transcript.
- Markdown: structured with speaker/time headings.
- JSON: full machine-readable payload.

## 7. UX/UI Requirements

## 7.1 Information Architecture
- Header: project status + quick actions.
- Left/Top: upload + queue.
- Center: speaker timeline.
- Right/Bottom: segment detail and edit panel.
- Footer/toolbar: export actions.

### 7.2 Visual Direction
- Keep current template language from `web-ui-template/` (dark, high-contrast, compact, technical).
- Desktop-first layout; responsive single-column fallback for mobile widths.
- Clear state colors:
  - queued: neutral
  - running: accent
  - completed: success
  - failed: error

### 7.3 Interaction Rules
- One primary action per step (`Start Analysis`, `Export`).
- Job cards must expose state + error reason.
- Editing speaker labels should update all matching segments in current result.
- No hidden auto-export; export is explicit user action.

## 8. Web Interface Contract (MVP)

Required screens/areas:
1. Upload/Job Queue screen area.
2. Result viewer area (timeline + segments).
3. Export action area.

Required controls:
1. File input/select.
2. Start/cancel job.
3. Segment text edit.
4. Speaker label rename.
5. Export buttons (TXT/MD/JSON).

Reference scaffold:
- `analysis-module/web-ui-template/index.html`
- `analysis-module/web-ui-template/styles.css`
- `analysis-module/web-ui-template/app.js`

## 9. API Contract (Draft)

### POST `/api/v1/analyze`
Request
```json
{
  "audio_path": "C:/recordings/session.opus",
  "options": {
    "language": "auto",
    "speaker_diarization": true
  }
}
```

Response
```json
{
  "job_id": "job_123",
  "status": "queued"
}
```

### GET `/api/v1/analyze/{job_id}`
Response
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

### Error model
- `400`: invalid input
- `404`: unknown job/file
- `409`: invalid job state transition
- `500`: runtime/inference/storage failure

## 10. Data Schema (Result)

```json
{
  "analysis_id": "analysis_001",
  "source_file": "session.opus",
  "duration_s": 135.2,
  "total_speakers": 3,
  "segments": [
    {
      "id": "seg_0001",
      "speaker_id": "SPEAKER_00",
      "speaker_label": "Speaker 1",
      "start_time": 0.0,
      "end_time": 8.4,
      "text": "Welcome everyone.",
      "confidence": null
    }
  ],
  "metadata": {
    "runtime": "vibevoice",
    "created_at": "2026-02-20T12:00:00Z",
    "version": "1.0"
  }
}
```

## 11. Runtime Components

1. `analysis-api` (planned): job lifecycle + result serving.
2. `analysis-worker` (planned): inference + normalization.
3. `storage` (planned): artifacts and metadata.
4. `web-ui-template`: UI shell until production frontend is wired.

## 12. State Machines

Job state:
- `queued -> running -> completed`
- `queued -> running -> failed`
- `queued -> canceled`
- `running -> canceled`

UI state:
- `idle`
- `upload-ready`
- `processing`
- `result-ready`
- `error`

## 13. Reliability and Safety

1. Timeouts for long-running inference jobs.
2. Cancel support for queued/running jobs.
3. Clear user-facing errors with action hints.
4. Local-first handling of audio artifacts.

## 14. Performance Targets (Initial)

1. UI remains responsive during active jobs.
2. Queue operations are non-blocking.
3. Result rendering supports long sessions without freezing.

## 15. Testing and Acceptance

### 15.1 Automated
1. API contract tests (request/response and state transitions).
2. Worker normalization tests.
3. UI tests for queue state rendering and export actions.

### 15.2 Manual QA
1. Single-file happy path.
2. Batch with mixed success/failure.
3. Cancel and retry behavior.
4. Speaker label edit propagation.
5. Export validity across formats.

### 15.3 MVP Acceptance Criteria
1. At least one supported audio file can be analyzed end-to-end.
2. Timeline and segments are visible and editable.
3. JSON export matches schema in this document.
4. No runtime coupling to Trispr Flow mainline.

## 16. Branch and Ownership

- Branch: `analysis-module-branch`
- Mainline branch (`main`) is intentionally analysis-runtime free.
- Integration with Trispr Flow is contract-based only.

## 17. Document Relationships

- `README.md`: project entrypoint
- `PRODUCT_SCOPE.md`: boundaries
- `ARCHITECTURE.md`: component design
- `ROADMAP.md`: phased rollout
- `ANALYSIS_MODULE_SPEC.md` (this file): execution-level consolidated spec
