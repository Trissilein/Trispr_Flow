# Trispr Flow - Status

Last updated: 2026-03-13

## Snapshot

- Current release: `v0.7.0`
- Current planning phase: `v0.7.1 stabilization execution`
- Canonical next steps: `ROADMAP.md`

## Working State

- Contributor process now enforces a pre-push housekeeping gate in `CONTRIBUTING.md` ("Housekeeping (Required Before Push)").
- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Adaptive continuous dump is unified across mic Toggle mode and system loopback.
- Continuous dump profiles and per-source overrides are available in Settings.
- Session chunk persistence is source-specific (`mic` and `output`) to avoid finalize collisions.
- AI fallback foundation is in place in Post-Processing settings.
- Release QA automation is available via `npm run qa:release` (build/test/rust/audit + strict latency SLO gate).
- **GPU Acceleration Pipeline** (v0.7.0+):
  - NVIDIA CUDA auto-detection and configuration during installer setup.
  - Pre-warming GPU capability cache at startup (eliminates 2.75s cold-start probe).
  - Q5 quantized models for VRAM-constrained GPUs (e.g., T500 mobile GPU).
  - Benchmarked latency (PTT release → paste): **~7.5-8s on NVIDIA T500 + Q5 model** (Whisper + CUDA inference).
    - Baseline: ~55s on CPU alone (not viable for interactive use).
    - GPU speedup: **7x faster than CPU** (6.6s GPU vs 55s CPU for longer audio).
  - FFmpeg bundled for OPUS encoding pipeline.
  - Direct Windows API exit to prevent WebView2 teardown hang on quit.
- Installer variants available:
  - CUDA (recommended for NVIDIA GPUs)
  - Vulkan (experimental, cross-platform)

## Analysis De-Scope

- Analysis launcher flow has been removed from Trispr Flow mainline.
- The `Analyse` button remains as a placeholder and shows a "coming soon" notice.
- Analysis runtime/packaging artifacts were removed from mainline.
- Dedicated analysis work moved to `analysis-module-branch`.

## Known Gaps

- Hands-on desktop/mobile UX QA remains manual and ongoing.
- Some historical planning docs still reference previous analysis experiments.

## Privacy + Network Notes

- Mainline runtime remains local-first for transcription.
- No analysis installer download path exists in Trispr Flow mainline.

## Next Focus

1. **Whisper-Server Mode** (Optional latency optimization):
   - Keep Whisper model pre-loaded in GPU VRAM across transcriptions.
   - Eliminates ~1.4s model-load overhead per transcription (7.5s → ~6.2s on T500; 4-5s → ~3s on RTX 5700 XT).
   - Requires `whisper-server.exe` CUDA binary (either compile from whisper.cpp or pre-built).
   - Medium complexity: ~3-4h implementation + binary sourcing.
2. Finalize latency benchmark baseline (`benchmark:latency`) with updated GPU timings and track p50/p95 trend.
3. Validate runtime-start background behavior on Windows (no stuck "Starting runtime..." state).
4. Continue Block E (UX/UI consistency + settings IA cleanup).
5. Complete Block F reliability hardening + release QA.
