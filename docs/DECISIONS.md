# Decisions

Last updated: 2026-02-06

## Implemented Decisions

### DEC-001 Platform and app shell
- Status: `implemented`
- Decision: Use Tauri v2 desktop shell with tray and global hotkeys.
- Why: Strong Rust integration and low runtime overhead for desktop capture tooling.

### DEC-002 Primary transcription backend
- Status: `implemented`
- Decision: Use whisper.cpp as primary ASR backend (GPU-first, CPU fallback).
- Why: Privacy-first local transcription and predictable offline behavior.

### DEC-003 Persistence strategy
- Status: `implemented`
- Decision: Persist settings and histories as JSON in app config/data directories.
- Why: Simple migration path and transparent local state management.

### DEC-004 Capture architecture
- Status: `implemented`
- Decision: Keep input capture and output transcription as separate pipelines, merge views in Conversation tab.
- Why: Clear operational boundaries and easier debugging of source-specific issues.

### DEC-005 Transcribe enablement semantics
- Status: `implemented`
- Decision: Output transcription starts disabled each session and is enabled explicitly by user action/hotkey.
- Why: Safer default and clearer user intent.

### DEC-006 Voice Activation threshold UX
- Status: `implemented`
- Decision: Expose Voice Activation thresholds in dB (-60..0), while storing linear values internally.
- Why: User control maps to perceived loudness; internal DSP remains unchanged.

### DEC-007 Verification baseline
- Status: `implemented`
- Decision: Standard local verification is `npm run test` plus `npm run test:smoke`.
- Why: Fast unit feedback plus integrated frontend+Rust sanity check.

## Accepted Decisions (Planned, Not Fully Implemented)

### DEC-008 Milestone 3 ordering
- Status: `accepted`
- Decision: Implement Capture Enhancements before Post-Processing pipeline.
- Why: Post-processing should target stabilized capture behavior and segmentation patterns.

### DEC-009 AI fallback direction
- Status: `accepted`
- Decision: Replace single-provider cloud fallback with provider-agnostic AI fallback (Claude/OpenAI/Gemini).
- Why: Flexibility, model choice, and future-proof integration.

### DEC-010 Activity feedback expansion
- Status: `accepted`
- Decision: Add tray pulse (recording/transcribing) and backlog capacity management (80% warning + expansion path).
- Why: Better runtime observability and reduced risk of dropped audio during long sessions.

## Open Decisions

### DEC-011 Optional backend scope
- Status: `open`
- Question: Add faster-whisper as optional backend or keep whisper.cpp-only.

### DEC-012 Post-processing scope depth
- Status: `open`
- Question: Final scope of normalization and domain-specific correction rules before AI enhancement.

### DEC-013 CUDA toolchain policy
- Status: `open`
- Question: Enforce preferred VS/CUDA toolchain combo vs supporting broader override-based setups.
