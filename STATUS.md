# Trispr Flow - Status

Last updated: 2026-04-29

## Snapshot

- Current release: `v0.8.0`
- Current planning phase: Block B (UX/UI consistency + R5 picker-unification) — see `ROADMAP.md`
- Canonical next steps: `ROADMAP.md` (A → F active zone, Z1-Z4 deferred)
- Current readiness: v0.8.0 cut, gate green via `--strict-benchmark` (soak runs intentionally skipped)

## Current Blockers

1. No hard compile/test blockers as of 2026-04-07 (`cargo test --lib`, `npm test`, `npm run build` all green).
2. Block U release gate: soak evidence (8h + 24h runs) still pending before `v0.8.x` cut.
3. Vocabulary learning (Block J / Tasks 44a–44b) now complete; Task 44c (regression tests) planned.

## Next Focus (Execution Order)

1. Complete Block U U2/U3 soak runs and close release gate.
2. Task 44c: vocabulary learning regression tests (validate learning + threshold + auto-add flows).
3. Screen Recording Module (future): vocabulary ground-truth via active-window capture + OCR diff.
4. Keep baseline gates green while iterating (`cargo test --lib`, `npm test`, `npm run build`).

## Working State

- **Recent (2026-04-08)**:
  - **v0.7.3 Release**: UI audit and redesigns complete.
    - **Models section redesign**: Installed models moved to top (management-first UX), converted to `.qwen-model-card` layout; curated catalog below with "Available Models" section header. Installed/active models filtered from catalog.
    - **Custom Vocabulary Auto-Learn redesign**: Settings restructured into inset `vocab-learn-block` sub-card with visual separator from table; threshold + auto-add stacked vertically; Reset button demoted to link-style and hidden when no candidates.
    - **Vocabulary learning casing bug fixed**: Corrections differing only in capitalisation (e.g., "trispr" → "Trispr") now tracked correctly; was silently discarded by overly strict lowercase equality check.
    - **Polish**: Removed `vocab-input` hover lift animation (wrong pattern for text inputs); removed invisible gradient overlay on vocab table; simplified "Installed locally" header to "Your Models".
    - **Build + Tests**: `npm run build` green, 219 tests pass. Ready for release candidate.
  - Block U soak testing: Delayed to v0.8.x release cycle; observed data collection continues parallel to development.

- **Recent (2026-04-07)**:
  - **Vocabulary Learning (Tasks 44a–44b) complete**: LLM-diff-based word correction tracking now live. Users can collect recurring AI corrections with configurable threshold (1–10 repetitions, default 3), optional auto-add, and review dialog. Settings: enable learning toggle + threshold + auto-add controls + reset button. Banner + modal UI for suggesting newly-detected corrections. Persistence via localStorage with max 200 candidates.
  - **Custom Vocabulary UI Redesign**: Flattened nesting (now 1 level max, previously 3 deep). Moved from inside "Rule-Based Details" to peer section before "Topic Detection". Compact vocab table (reduced padding 7→4px, smaller fonts 13→12px) with arrow separators. Vocab Learning section is sub-toggle within Custom Vocabulary expander.
  - **Keep-Alive duration**: Ollama refinement keep-alive increased from 20m to 60m default (env var `TRISPR_OLLAMA_KEEP_ALIVE` still configurable). Reduces cold-start latency for refinement requests after long idle periods.
  - **Frontend build**: `npm run build` green, all TypeScript compiles clean, Vite bundle optimized.
- **Recent (2026-04-06)**:
  - **Hotkey system overhaul**: ISO `<`/`>` key (`IntlBackslash`) now supported via local vendor patch for `global-hotkey v0.7.0`. Hotkey recorder uses hybrid `event.code`/`event.key` strategy fixing DE keyboard Y ↔ Z swap. Extended recordable surface to Numpad, Media, Volume, and lock keys. Human-readable display labels via `formatHotkeyForDisplay()` in `ui-helpers.ts`.
  - **TTS-Stop default hotkey**: Changed from `CommandOrControl+Shift+Escape` (conflicted with Windows Task Manager) to `CommandOrControl+Shift+F12`.
  - **Gemma 4 model variant matrix**: Model picker now shows all 5 quantization variants (E2B Q4/Q8, E4B Q4/Q8/BF16) with explicit VRAM annotations per card.
  - **Model-family-specific refinement prompts**: `resolveEffectiveRefinementPrompt()` now receives the active model name and appends Gemma-specific anglicism/brand-name preservation instructions for `gemma*` models.
  - **Ollama target runtime**: Updated to v0.20.2.
  - **Ollama download progress popup**: Model pull progress now shown in modal with MB counter.
  - **Block J (vocabulary learning) designed**: LLM-diff-based suggestion flow planned (Tasks 44–44c); Screen Recording module identified as future ground-truth path. Implementation not yet started.
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

1. Complete Block U soak runs (U2/U3: 8h + 24h) and close `v0.8.x` release gate.
2. Begin Block J: Task 44 word-diff extraction from refinement events → persistence threshold → vocabulary suggestion UI.
3. Screen Recording Module (future): active-window capture before Enter/send-click → OCR diff → vocabulary ground-truth learning.
4. Keep Block R follow-up (`R4`–`R6`) non-blocking and regression-safe.
