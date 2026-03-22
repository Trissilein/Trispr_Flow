# Trispr Flow - Status

Last updated: 2026-03-22

## Snapshot

- Current release: `v0.7.2`
- Current planning phase: `v0.7.3 stabilization follow-through (Block S reopened: S10-S13 active, then TTS free-config/testing)`
- Canonical next steps: `ROADMAP.md`
- Current readiness: development-ready, not release-ready

## Current Blockers

1. No hard compile/test blockers in current S10-S12 baseline as of 2026-03-22 (`cargo test --lib`, `npm test`, `npm run build` green).
2. Remaining stabilization blocker: `S13` manual acceptance gate (overlay `50 cycles + 10 restarts` + module toggle soak) is still pending.
3. Remaining milestone blocker after S13: `TTS freikonfigurierbar + testbar` completion (forced verification flow finalization).

## Tomorrow's Objective (Execution Order)

1. Close `S13` acceptance with overlay soak gate and re-enable performance validation.
2. Continue directly with `TTS freikonfigurierbar + testbar` (provider-agnostic configuration + explicit forced-test path).
3. Keep baseline gates green while iterating (`cargo test --lib`, `npm test`, `npm run build`).
4. Hold Block T start until S13 + TTS acceptance pass.

## Working State

- Contributor process now enforces a pre-push housekeeping gate in `CONTRIBUTING.md` ("Housekeeping (Required Before Push)").
- **Recent (2026-03-20)**:
  - Overlay startup hardening: transparent WebView create now retries once with safe non-transparent fallback before disabling overlay for the session.
  - New startup kill-switch `TRISPR_DISABLE_OVERLAY=1` skips overlay initialization for crash triage and recovery runs.
  - FFmpeg packaging cleanup: committed `src-tauri/bin/ffmpeg/ffmpeg.exe` removed from git tracking; installer build now auto-fetches pinned FFmpeg 7.1.1 (`scripts/setup-ffmpeg.ps1`) with SHA256 + OPUS capability validation.
  - Runtime FFmpeg checks now require `libopus` support for OPUS encode paths and dependency preflight reporting.
  - Block N `N9` privacy/consent UX hardening landed in `modules-hub`: module-specific consent copy for Vision/TTS, richer enable-confirmation details, and pending-consent status summary in Modules header.
  - Block N `N10` fallback/error matrix landed: deterministic TTS fallback executor (`primary -> fallback`), explicit fallback error codes, richer `tts:speech-*` diagnostics payloads, and synchronous `test_tts_provider` status with fallback visibility.
  - Block N `N11` benchmark harness landed: new `run_tts_benchmark` backend command, `scripts/tts-benchmark.ps1` runner, and provider recommendation policy/reporting (`bench/results/tts.latest.json`).
  - Block N `N11` evidence run completed on 2026-03-20/21: `windows_native` recommended (`success_rate=100%`, `p50=245ms`, `p95=282ms`); `local_custom` failed in this environment due missing Piper binary.
- **Recent (2026-03-22)**:
  - Block S `S6-S8` landed: AI Refinement is now a toggleable `ai_refinement` module with hard runtime gating (`module && setting`) and lifecycle side-effects on disable.
  - Frontend tab-gating is active: AI Refinement tab/panel are hidden when module is disabled; active-tab fallback and localStorage guard are in place.
  - Legacy compatibility migration is active: existing configs with `ai_fallback.enabled=true` are preserved by auto-enabling `ai_refinement` when module status was previously missing.
  - Block S `S9` closure done: `ROADMAP.md`, `STATUS.md`, `docs/TASK_SCHEDULE.md` synchronized and regression gates reconfirmed green.
  - Block S `S10` landed: strict module-UX decoupling for TTS (`output_voice_tts`) with dedicated `voice-output` main tab, hard module gating, active-tab fallback, and Configure routing from Modules Hub to tab.
  - Block S `S11` landed: AI refinement re-enable path now autostarts managed Ollama (`local_primary`) and performs model warmup; defer policy now requires runtime-ready state and emits deterministic runtime-not-ready fallback reasons.
  - Block S `S12` landed: overlay lifecycle moved to bounded retry/cooldown supervisor (no permanent lockout), explicit `overlay:health` recovered status, heartbeat sync path, off-screen safe-anchor fallback, and refinement pulse restart hardening.
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
- First-run developer bootstrap is available via `scripts/windows/FIRST_RUN.bat` (root `FIRST_RUN.bat` wrapper remains compatible).
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

1. Close `S13` manual acceptance (`50 cycles + 10 restarts` overlay/module soak gate).
2. Prioritize `TTS freikonfigurierbar + testbar` (configuration flexibility + forced end-to-end verification).
3. Use `docs/N11_TTS_BENCHMARK.md` as supporting evidence for TTS provider/runtime defaults while implementing free-config mode.
4. Start Block T (`v0.8.0` assistant foundation) only after S13 + TTS acceptance gates are green.
5. Keep Block R follow-up (`R4`-`R6`) non-blocking and regression-safe.
