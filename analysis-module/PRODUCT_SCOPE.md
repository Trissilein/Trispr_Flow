# Product Scope

## Goal
Provide a dedicated, production-ready analysis app that focuses on speaker-aware transcription and post-hoc audio analysis.

## In Scope
- Audio upload/selection workflow
- Speaker segmentation and diarization view
- Segment editing (speaker labels, text corrections)
- Export pipeline (TXT/MD/JSON)
- Batch analysis queue and progress status
- Local-first execution path

## Out of Scope
- Real-time microphone capture
- System audio capture controls
- Trispr Flow settings/state persistence
- Tauri shell integration in this phase

## Non-Goals
- Replace Trispr Flow transcription UI
- Share runtime processes with Trispr Flow
- Introduce cloud-only hard dependency

## Success Criteria
- Users can analyze one or more files end-to-end
- Output schema is stable and versioned
- UI supports desktop-first workflow with accessible controls
