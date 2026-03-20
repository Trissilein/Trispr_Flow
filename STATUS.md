# Trispr Flow - Status

Last updated: 2026-03-20

## Snapshot

- Current release: `v0.7.1`
- Current planning phase: `v0.7.1 stabilization execution`
- Canonical next steps: `ROADMAP.md`

## Working State

- Contributor process now enforces a pre-push housekeeping gate in `CONTRIBUTING.md` ("Housekeeping (Required Before Push)").
- **Recent (2026-03-20)**:
  - Overlay startup hardening: transparent WebView create now retries once with safe non-transparent fallback before disabling overlay for the session.
  - New startup kill-switch `TRISPR_DISABLE_OVERLAY=1` skips overlay initialization for crash triage and recovery runs.
  - FFmpeg packaging cleanup: committed `src-tauri/bin/ffmpeg/ffmpeg.exe` removed from git tracking; installer build now auto-fetches pinned FFmpeg 7.1.1 (`scripts/setup-ffmpeg.ps1`) with SHA256 + OPUS capability validation.
  - Runtime FFmpeg checks now require `libopus` support for OPUS encode paths and dependency preflight reporting.
- **Recent (2026-03-18)**:
  - UI: Prompt Style section promoted to top of AI Refinement tab (was buried 3 levels deep in collapsed expander).
  - Logging: daily log files now use `.txt` extension (`trispr-flow.YYYY-MM-DD.txt`); shutdown log entry added before `ExitProcess(0)` — normal exits now distinguishable from crashes.
  - Housekeeping: removed all tracked `.tmp.*` crash artefacts from repository; `.gitignore` hardened for editor temp files and rust-analyzer artefacts; legacy `%LOCALAPPDATA%\TrisprFlow` folder cleaned up.
  - Chip selector for prompt presets (from previous session) is stable.
- `N5+Q Stabilization Packet` is closed for regression scope: `N5d` validation packet is complete and green.
- Block N bridge milestones are integrated in mainline:
  - `N7` Agent capability bridge is live (`workflow_agent` now consumes optional vision snapshot context and optional TTS feedback through module gates).
  - `N8` Voice output policy enforcement is live (`agent_replies_only`, `replies_and_events`, `explicit_only` with request context gating).
  - `N12` multimodal integration test packet is present (`src/tests/n12-multimodal-integration.test.ts`, 16 tests).
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
  - FFmpeg for OPUS encoding is now provisioned at build-time (pinned download) and bundled into installer resources.
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

1. Continue Block N privacy/consent hardening (`N9`) with updated in-app status messaging.
2. Implement deterministic TTS fallback/error matrix (`N10`) and align UX diagnostics surface.
3. Execute benchmark track (`N11`) with >=3 runs/provider/scenario and lock default TTS provider by evidence.
4. Prepare `N13` end-to-end agent automation validation (voice command -> resolve -> publish/queue -> spoken response).
