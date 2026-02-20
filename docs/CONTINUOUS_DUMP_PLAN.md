# Adaptive Continuous Dump Plan

Last updated: 2026-02-19

## Goal

Unify long-running audio segmentation behavior for:

- Mic Toggle mode (continuous input monitoring until toggle-off)
- System Audio loopback transcription

And replace fixed chunk-only behavior with a smarter hybrid strategy:

- Silence-aware flush
- Soft interval target
- Hard-cut safety bound
- Optional pre/post context
- Backpressure-aware scaling

## Design

## 1. Segmenter Runtime

- New backend module: `src-tauri/src/continuous_dump.rs`
- Core config:
  - `soft_flush_ms`
  - `silence_flush_ms`
  - `hard_cut_ms`
  - `min_chunk_ms`
  - `pre_roll_ms`
  - `post_roll_ms`
  - `idle_keepalive_ms`
  - `threshold_start` / `threshold_sustain`
- Flush reasons:
  - `silence`
  - `soft_interval`
  - `hard_cut`
  - `stop`
  - `backpressure`

## 2. Settings and Compatibility

- Extend persisted settings with continuous dump fields and profile presets:
  - `balanced`
  - `low_latency`
  - `high_quality`
- Keep legacy fields (`transcribe_batch_interval_ms`, `transcribe_chunk_overlap_ms`, `transcribe_vad_silence_ms`) synchronized.
- Clamp all timing fields to safe limits on load/save.

## 3. System Audio Integration

- Replace static interval/overlap chunk slicing in loopback path with adaptive segmenter.
- Preserve VAD filtering semantics for transcript visibility.
- Emit telemetry:
  - `continuous-dump:segment`
  - `continuous-dump:stats`

## 4. Mic Toggle Integration

- Start a dedicated toggle processor thread while capture stream runs.
- Feed mic samples through adaptive segmenter continuously.
- Transcribe each finalized segment and emit transcript/history updates.
- Optional `auto_save_mic_audio` chunk persistence through session manager.

## 5. Session Persistence Safety

- Session manager upgraded to source-specific active sessions (`mic`, `output`).
- Finalization now uses source-specific API to avoid cross-source collisions.

## UI/UX Rollout

- Add master switch + profile selector.
- Rename legacy chunk controls to adaptive wording:
  - chunk interval -> soft flush target
  - chunk overlap -> context pre-roll
- Add advanced controls:
  - hard cut
  - min chunk
  - pre-roll/post-roll
  - idle keepalive
- Add per-source overrides:
  - system soft/silence/hard
  - mic soft/silence/hard

## Validation

- Frontend:
  - `npm run build`
  - `npm test`
- Backend:
  - `cargo test --manifest-path src-tauri/Cargo.toml`
- Unit tests added for adaptive segmenter behavior:
  - silence flush
  - hard cut
  - short chunk merge

## Follow-up Improvements

- Add live UI badge for queue pressure and active profile.
- Consider silence-stripped OPUS export for analysis-focused recordings.
- Add source-level integration tests for adaptive dumping behavior under mixed speech/silence traces.
