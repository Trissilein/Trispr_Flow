# Trispr Flow - Status

Last updated: 2026-03-18

## Snapshot

- Current release: `v0.7.0`
- Current planning phase: `v0.7.1 stabilization execution`
- Canonical next steps: `ROADMAP.md`

## Working State

- Contributor process now enforces a pre-push housekeeping gate in `CONTRIBUTING.md` ("Housekeeping (Required Before Push)").
- **Recent**: UI refinement iteration — prompt preset cards replaced with chip selector for improved UX + additional stability fixes.
- `N5+Q Stabilization Packet` is in execution (P1-P3): startup/runtime diagnostics, overlay resilience, and vision runtime hardening are integrated in mainline WIP.
- Ollama runtime IPC paths used by model refresh/verify/info/pull-delete are hardened with non-blocking worker dispatch; UI stays interactive during background runtime activity.
- Mainline installer/runtime packaging now keeps both Whisper backends (`bin/cuda` + `bin/vulkan`) and resolves backend at runtime (`local_backend_preference`).
- First-run developer bootstrap is available via `FIRST_RUN.bat` (`npm install` + runtime hydration from installed app resources).
- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Adaptive continuous dump is unified across mic Toggle mode and system loopback.
- Continuous dump profiles and per-source overrides are available in Settings.
- Session chunk persistence is source-specific (`mic` and `output`) to avoid finalize collisions.
- AI fallback foundation is in place in Post-Processing settings.
- Release QA automation is available via `npm run qa:release` (build/test/rust/audit + strict latency SLO gate).
- **GPU Acceleration Pipeline** (v0.7.0+):
  - NVIDIA CUDA capability detection with runtime backend preference (`auto|cuda|vulkan`).
  - Pre-warming GPU capability cache at startup (eliminates 2.75s cold-start probe).
  - Q5 quantized models for VRAM-constrained GPUs (e.g., T500 mobile GPU).
  - Benchmarked latency (PTT release → paste): **~7.5-8s on NVIDIA T500 + Q5 model** (Whisper + CUDA inference).
    - Baseline: ~55s on CPU alone (not viable for interactive use).
    - GPU speedup: **7x faster than CPU** (6.6s GPU vs 55s CPU for longer audio).
  - FFmpeg bundled for OPUS encoding pipeline.
  - Direct Windows API exit to prevent WebView2 teardown hang on quit.
- Mainline installer bundles both CUDA and Vulkan Whisper runtimes; backend selection happens in-app at runtime.

## Analysis De-Scope

- Analysis launcher flow has been removed from Trispr Flow mainline.
- The `Analyse` button remains as a placeholder and shows a "coming soon" notice.
- Analysis runtime/packaging artifacts were removed from mainline.
- Dedicated analysis work moved to `analysis-module-branch`.

## Known Gaps

- Hands-on desktop/mobile UX QA remains manual and ongoing.
- Some historical planning docs still reference previous analysis experiments.
- Fresh-install startup responsiveness on Optimus systems has improved with non-blocking Ollama IPC hardening; regression monitoring remains active.

## Privacy + Network Notes

- Mainline runtime remains local-first for transcription.
- No analysis installer download path exists in Trispr Flow mainline.

## Next Focus

1. Complete `N5d` regression validation for `N5+Q Stabilization Packet` (startup diagnostics, overlay recovery, vision buffer/snapshot flows, Ollama UI responsiveness under runtime load).
2. Expand multimodal integration tests for Block N (`N12`) on top of the new frame-buffer pipeline.
3. Continue Block N privacy/consent hardening (`N9`) with updated in-app status messaging.
4. Finalize latency benchmark baseline (`benchmark:latency`) with updated GPU timings and track p50/p95 trend.
