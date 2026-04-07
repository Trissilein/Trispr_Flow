# Changelog

All notable changes to Trispr Flow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Vocabulary Learning (Block J, Tasks 44a–44b)**:
  - Automatic detection of recurring AI corrections via LCS word-diff algorithm.
  - Configurable threshold (1–10 repetitions, default 3) for when corrections become suggestions.
  - Optional auto-add mode: corrections are automatically added to custom vocabulary without review.
  - Review dialog with accept/dismiss controls for each suggestion pair.
  - Settings UI with Enable toggle, Auto-add checkbox, Threshold input, candidate status line, and Reset button.
  - Persistent candidate tracking via localStorage (max 200 entries).
  - Suggestion banner in history panel when corrections reach threshold.
- **Custom Vocabulary UI redesign**:
  - Flattened nesting structure: moved from "Rule-Based Details" to peer section before "Topic Detection".
  - Compact vocab table with reduced padding (7px→4px) and smaller fonts (13px→12px).
  - Arrow separators between input columns for visual clarity.
  - Vocabulary Learning integrated as sub-toggle within Custom Vocabulary expander (progressive disclosure).
- **Keep-Alive duration increase**:
  - Ollama refinement keep-alive duration increased from 20 minutes to 60 minutes default.
  - Configurable via `TRISPR_OLLAMA_KEEP_ALIVE` environment variable.
  - Reduces cold-start latency for refinement requests after idle periods.
- **Hotkey system overhaul**:
  - ISO `<`/`>` key (`IntlBackslash` / `VK_OEM_102`) is now a valid hotkey on DE/EU keyboard layouts.
    - Implemented via a local Cargo vendor patch for `global-hotkey v0.7.0` (`vendor/global-hotkey-0.7.0/`) with no upstream fork required.
    - Added `IntlBackslash`, `IntlRo`, `IntlYen` to `parse_key()` and `key_to_vk()` in the patched crate.
  - Hotkey recorder now uses a hybrid `event.code` / `event.key` strategy: letter keys use `event.key` (fixes Y ↔ Z swap on DE layout), special/symbol keys use `event.code` via an explicit `CODE_TO_KEY` lookup (layout-independent).
  - Extended recordable key surface: Numpad keys (Num0–9, NumAdd/Sub/Mul/Div/Decimal/Enter), Media keys (Play/Pause/Stop/Next/Prev), Volume keys (Up/Down/Mute), and lock/system keys (CapsLock, NumLock, ScrollLock, Pause, PrintScreen).
  - Media/Volume keys can be bound as standalone hotkeys (no modifier required) — `validate_hotkey_format()` now permits modifier-free bindings for this key class.
  - Hotkey inputs now display human-readable labels via `formatHotkeyForDisplay()` (e.g. `IntlBackslash` → `< >`, `ArrowUp` → `↑`, `MediaPlayPause` → `⏯`).
  - `formatHotkeyForDisplay()` extracted to `src/ui-helpers.ts` to avoid circular imports between `hotkeys.ts` and `settings.ts`.
- **Gemma 4 model variant matrix**:
  - Ollama model picker now lists all useful Gemma 4 quantization variants with explicit VRAM requirements:
    - `gemma4:e2b` — Fast Q4, ~3.2 GB VRAM
    - `gemma4:e2b-it-q8_0` — Balanced Q8, ~4.6 GB VRAM
    - `gemma4:e4b` — Standard Q4, ~5 GB VRAM (default recommendation)
    - `gemma4:e4b-it-q8_0` — High Quality Q8, ~7.5 GB VRAM
    - `gemma4:e4b-it-bf16` — Maximum BF16, ~15 GB VRAM
  - Model cards show `size_gb · ~vram_gb VRAM` label when `vram_gb` metadata is present.
- **Model-family-specific refinement prompts**:
  - `resolveEffectiveRefinementPrompt()` accepts an optional `model` parameter and adapts the prompt for the detected model family.
  - Gemma 4 (`gemma*` prefix): `wording` preset receives an explicit anglicism/brand-name preservation instruction appended after the base prompt, preventing Gemma's tendency to silently remove foreign-language terms.
  - `detectModelFamily()` uses a simple lowercase prefix heuristic (`gemma`, `qwen`, `generic`).
  - `llm_prompt` preset continues to bypass both language guard and model-family addons (always outputs English).
- **Ollama download progress popup**:
  - Download progress is now shown in a modal popup during Ollama model pulls with MB counter and percentage.

### Fixed

- **TTS-Stop default hotkey conflict**: Changed default from `CommandOrControl+Shift+Escape` to `CommandOrControl+Shift+F12` to avoid conflict with Windows Task Manager shortcut.
- **Hotkey UI badge strings**: Registration status badges now use English labels ("Conflict", "Hotkey registered") consistent with the rest of the UI.

### Changed

- **Ollama runtime**: Updated target/tested runtime to v0.20.2.

- **Installer variant matrix (Windows)**:
  - New multi-variant build pipeline (`scripts/windows/build-installers.bat`) for:
    - `vulkan-only`
    - `cuda-lite`
    - `cuda-complete`
  - New Tauri variant config generator (`scripts/generate-tauri-variant-config.mjs`).
  - New manual release asset uploader (`scripts/windows/upload-release-assets.ps1`, wrapper: `upload_release_assets.bat`).
  - New GitHub Actions workflow for Windows installer build/upload: `.github/workflows/windows-release-installers.yml`.
- **Installer voice packs v1.1**:
  - Components page now includes curated Piper voice selection (`de_DE-thorsten-medium`, `de_DE-thorsten_emotional-medium`, `en_GB-alan-medium`, `en_GB-alba-medium`, `en_GB-cori-high`) plus optional extra voice-key input.
  - Voice pack download flow supports all installer variants (including optional online add-ons for `cuda-complete`).
  - Fresh installs now bind `voice_output_settings.piper_model_path` deterministically to `de_DE-thorsten-medium` when Piper is enabled.
  - `cuda-complete` packaging now bundles only Piper runtime + base voice; additional voices remain optional installer downloads.

### Fixed

- **CUDA runtime dependency hardening**:
  - Added explicit CUDA runtime preflight for Whisper CLI (`cublas64_13.dll`, `cublasLt64_13.dll`, `cudart64_13.dll`) before probe/spawn.
  - Prevents Windows loader popups from incomplete CUDA payloads by skipping broken CUDA runtime and falling back to Vulkan/CPU chain.
  - Installer build scripts now validate Whisper runtime payload completeness before `tauri build`.

## [0.7.2] - 2026-03-20

### Added

- **Frontend/Renderer watchdog**:
  - New frontend heartbeat lane (`frontend_heartbeat`) with automatic reload escalation when IPC heartbeat repeatedly fails.
  - New backend stale-heartbeat watchdog that attempts layered WebView recovery (reload/reload/destroy+recreate).
- **Windows crash triage hardening**:
  - Automatic WER LocalDumps setup at startup for `trispr-flow.exe`, `Trispr Flow.exe`, `com.trispr.flow.exe`, and `msedgewebview2.exe`.
  - Crash dumps now target `%LOCALAPPDATA%\\Trispr Flow\\crashdumps`.

### Changed

- **Repository housekeeping pass**:
  - Root docs reduced to the canonical set; planning/archive material moved under `docs/`.
  - Windows utility `.bat` scripts moved to `scripts/windows/` with root compatibility wrappers retained.
- **Ollama hardening + UX**:
  - Curated model pull flow now pre-validates tags against installable registry entries.
  - Runtime version surfacing is filtered for installable targets and normalized around runtime `0.17.7`.
  - Updated manager wording and pull-complete CTA for direct model activation.
- **Whisper/Quant policy defaults**:
  - Default transcription model is now `whisper-large-v3-turbo`.
  - Quantization UI default now prefers `q8_0` with explicit `q5_0` guidance for low-VRAM systems.

### Fixed

- **White-window/no-response resilience**:
  - App no longer relies on manual restarts when the renderer becomes stale; watchdog recovery path now self-heals in-session.

## [0.7.1] - 2026-03-20

### Added

- **First-Run Bootstrap (Windows)**:
  - Added `FIRST_RUN.bat` + `scripts/first-run.ps1` for post-clone onboarding.
  - Bootstrap now installs npm dependencies and reports local runtime readiness for transcription/quantization.
  - Includes best-effort runtime hydration from an installed app (`resources/bin` -> `src-tauri/bin`).
- **GPU VRAM Monitoring & Management**:
  - Real-time VRAM usage display in header status bar (updated every 2s via nvidia-smi).
  - Format: "2.1 GB / 8.0 GB" showing used/total VRAM.
  - Click GPU status item to purge VRAM (unload Ollama models, kill Whisper server).
  - Automatic VRAM cleanup on model switches (Whisper server restart, Ollama unload via API).
  - Hidden CMD window for nvidia-smi queries on Windows (CREATE_NO_WINDOW flag).
- **UX Improvements - Header Reorganization**:
  - Status indicators reorganized into 2-row layout:
    - Row 1: Recording + Transcribing status
    - Row 2: Refining + GPU status (clickable) + no separate button needed
  - Visual feedback: VRAM purge shows "Purging..." → "Purged ✓" (2s) in status display
  - Keyboard accessible: Tab + Enter/Space to trigger VRAM purge
- **N5+Q Stabilization Packet (P1-P3)**:
  - Startup/runtime diagnostics surface:
    - New backend commands `get_startup_status`, `get_runtime_diagnostics`, and `log_frontend_event`.
    - New event channels `startup:status`, `runtime:diagnostics`, and `overlay:health`.
    - Frontend staged bootstrap now loads heavy data after UI readiness to reduce startup stalls.
  - Local backend preference flow:
    - Added persistent `local_backend_preference` (`auto|cuda|vulkan`) in settings model/migration path.
    - Onboarding now stores detected backend recommendation into settings.
  - Overlay and refinement resilience:
    - Overlay state machine migrated to `Hidden/Armed/Recording/Transcribing` with recovery replay.
    - Added overlay health note/toast wiring and refinement activity reconciliation to prevent stuck indicators.
  - Vision runtime hardening:
    - Replaced metadata-only vision stream path with captured frame pipeline and bounded in-memory buffer stats.
    - `capture_vision_snapshot` now prefers buffered frames and falls back to live capture without disk persistence.
- **Block N Bridge + Policy Delivery (N7/N8/N12)**:
  - `agent_execute_gdd_plan` now bridges optional multimodal capabilities through module gates:
    - Vision context injection via `capture_vision_snapshot_internal` with progress stages `vision_context`, `vision_context_ready`, `vision_context_unavailable`.
    - Optional agent speech lane via internal `speak_tts_internal` calls with explicit context routing.
  - Voice output policy enforcement now gates speech by request context:
    - `agent_replies_only`, `replies_and_events`, `explicit_only` implemented via `is_tts_policy_allowed`.
    - TTS events now include `context`; provider tests use `manual_test`.
  - New multimodal integration validation packet:
    - `src/tests/n12-multimodal-integration.test.ts` added (16 tests).
    - `src/tests/n5d-regression.test.ts` extended to S1-S6 regression coverage for startup/overlay/module-health/runtime and vision buffer/snapshot contracts.

### Changed

- **Contributor Workflow**:
  - Added a mandatory pre-push housekeeping gate in `CONTRIBUTING.md`.
  - Housekeeping now standardizes context check, cleanup audit, doc sync, and a separate `chore(housekeeping): <scope>` commit before push.
- **Installer Runtime Packaging**:
  - NSIS post-install no longer prunes `bin/cuda` or `bin/vulkan`.
  - Both Whisper runtime folders now remain installed, with backend chosen at runtime.
- **FFmpeg Packaging Workflow**:
  - Replaced committed `src-tauri/bin/ffmpeg/ffmpeg.exe` with build-time provisioning via `scripts/setup-ffmpeg.ps1`.
  - Installer build scripts now auto-fetch pinned FFmpeg 7.1.1, validate SHA256, and enforce `libopus` encoder availability.
  - Added `bin/ffmpeg/ffmpeg.exe` to Tauri bundle resources so installed apps resolve FFmpeg from `resources/ffmpeg/ffmpeg.exe`.

### Fixed

- **Optimus/Hybrid Runtime Reliability**:
  - Prevented post-install backend folder deletion that could leave missing DLL/runtime states for quantization and backend fallback paths.
- **Three UX Fixes**:
  1. Voice Input Enabled by Default: `transcribe_enabled: true` in Settings::default()
     - Users can use PTT (Push-to-Talk) immediately after fresh install
  2. AI Refinement Without Ollama - Guided Onboarding:
     - Modal dialog when user enables AI Refinement without Ollama installed
     - Prominent download progress bar (MB counter) during Ollama installation
     - "Jetzt installieren" vs. "Später" options for user control
  3. AppData Path Unification:
     - All writes to canonical `%LOCALAPPDATA%\Trispr Flow\`
     - Legacy migration fallback for `%LOCALAPPDATA%\TrisprFlow\` (old Ollama paths)
     - Removed fragmentation across `%APPDATA%\com.trispr.flow\` and `%LOCALAPPDATA%\TrisprFlow\`
- **Ollama Process Lifecycle Management**:
  - Stop Ollama runtime when AI Refinement is disabled (prevents orphaned processes)
  - Enhanced app exit handler with logging for process cleanup verification
  - New `stop_ollama_runtime()` command for explicit process termination
- **Ollama UI Responsiveness Hardening (Non-blocking IPC)**:
  - Moved blocking Ollama command paths (`fetch_available_models`, `fetch_ollama_models_with_size`, `test_provider_connection`, `delete_ollama_model`, `get_ollama_model_info`, `unload_ollama_model`) to async worker dispatch.
  - Added timeout-protected `get_ollama_model_info` refresh path with soft-fail behavior to avoid runtime-state hangs in UI.
  - Reduced refresh-loop info logging to state transitions to lower frontend->backend IPC pressure while Ollama is starting or busy.
  - Guarded settings-driven runtime refresh while background startup polling is active to avoid refresh storms.
- **GPU Acceleration Hardening**:
  - NVIDIA GPU layer auto-detection and configuration during installer setup (nsDialogs custom page).
  - Registry environment variable `TRISPR_WHISPER_GPU_LAYERS` for persistent GPU settings.
  - Explicit CUDA device selection (`-dev 0`) for multi-GPU systems.
  - GPU capability pre-warming at app startup (background thread) to eliminate 2.75s cold-start probe on first transcription.
- **Q5 Quantized Model Variants**:
  - Added friendly labels for whisper Q5/Q8 German and English models for VRAM-constrained GPUs.
  - Local model scan recognizes ggml-large-v3-turbo-german-q5_0.bin automatically.
- **FFmpeg Binary Bundling**:
  - Runtime FFmpeg lookup now resolves bundled, repo-local, and PATH variants in a stable order.
  - OPUS paths now require `libopus` support explicitly (`find_ffmpeg_for_opus`) before encoding.
- **Performance Instrumentation**:
  - [TIMING] logs added to transcription pipeline (wav_write, whisper_spawn, whisper_process, handle_transcription_ok, segment_total) for latency diagnosis.
  - File-based logging to `%APPDATA%\com.trispr.flow\logs\trispr-flow.log.YYYY-MM-DD` (daily rotation, tracing-appender).
- **Ollama Fallback Timeout Optimization**:
  - Added `ping_ollama_quick()` (300ms timeout) in `prepare_refinement()` to fail fast if Ollama is unreachable.
  - Prevents blocking transcription paste for 5-10s when AI fallback is misconfigured but Ollama is not running.
- **Release Build Default**:
  - Changed `npm run dev` to use `tauri dev --release` for optimal local development performance (eliminates debug build overhead).
- **Windows Exit Optimization**:
  - Direct Windows API `ExitProcess(0)` on quit to bypass WebView2 teardown and eliminate 5-10s hang on exit.

### Fixed

- **GPU Capability Probe Cache**: OnceLock-backed cache prevents repeated whisper-cli invocations during transcription.
- **LLM Prompt Engineer Preset** (Block M / M-extra):
  - New refinement prompt preset `llm_prompt` that converts spoken dictation into high-quality, ready-to-use LLM prompts.
  - Output is always English regardless of input language — language guard is explicitly excluded for this preset.
  - Bilingual meta-prompts (EN + DE) on both TypeScript and Rust sides.
- **Workflow-Agent: Candidate Confirm Hardening** (Block M / M9):
  - Removed automatic first-candidate selection; user must explicitly click a session row.
  - Disambiguation warning shown when top-2 candidates score within 0.1 of each other.
  - Topic/temporal hint feedback displayed in agent console log.
- **Workflow-Agent: Target Language Enforcement** (Block M / M10):
  - `languageExplicitlySet` flag: user must actively choose a language per parse before building a plan.
  - Language select resets to disabled placeholder on every new parse.
  - Backend validates `target_language` against `ALLOWED_LANGUAGES` list and returns a structured error for unknown values.
- **Workflow-Agent Policy Module** (`src/workflow-agent-policy.ts`) (Block M / M11):
  - Pure, side-effect-free functions `isAmbiguousSelection` and `isValidTargetLanguage` extracted for testability.
  - Exported constants `DISAMBIGUATION_SCORE_THRESHOLD` and `ALLOWED_TARGET_LANGUAGES`.
- **Workflow-Agent Test Coverage** (Block M / M11):
  - 16 Rust unit tests for `parse_command`, `build_sessions`, and `score_sessions` in `workflow_agent.rs`.
  - 14 TypeScript tests (WA-S1/S2/S3) covering disambiguation, language validation, and score edge cases.

### Fixed

- **Hotkey toasts deduplication**: Repeated hotkey-conflict toasts (same message) are now suppressed after the first appearance per session (`src/toast.ts`).

## [0.7.0] - 2026-02-26

### Added

- **Refinement Insert Flow**: Deferred paste with `Refined-Only` strategy and automatic raw fallback after timeout.
- **Refinement Inspector + History View**:
  - Original + refined text are shown together.
  - Final refined text is displayed first, original text below.
  - Word-level diff summary for quick change review.
  - Refinement metadata is persisted in backend history entries (survives app restart).
- **Local Ollama Runtime Autostart**:
  - Autostart on app bootstrap when local refinement is enabled.
  - Autostart on enabling AI refinement (no auto-install).
- **Refinement Prompt Presets**:
  - Presets: `wording`, `summary`, `technical_specs`, `action_items`, plus `custom`.
  - Effective prompt preview is always visible.
- **Low Latency Refinement Mode**:
  - Faster local refinement profile via reduced token/context budgets.
- **GPU Activity Indicator**:
  - Runtime CPU/GPU activity indicator in UI.
  - Last known accelerator/backend snapshot persisted in local storage.
- **Dedicated Refinement Overlay Controls**:
  - Refinement animation settings separated from base overlay settings.
  - Color, speed, range and preset controls for refinement animation.

### Changed

- **Windows Installer Strategy**:
  - Two explicit installers (`CUDA` and `Vulkan`) as primary distribution model.
  - Removed in-installer GPU backend selector flow.
- **Whisper Runtime Resolution**:
  - No silent `Command::new("whisper-cli")` fallback.
  - Clear runtime-missing and dependency-error messaging for `NotFound`/DLL cases.
- **Runtime UI Behavior**:
  - Reduced stale "Starting runtime..." states when runtime is already reachable.

### Fixed

- Fixed `os error 2` transcription failures caused by missing/incomplete runtime binary paths.
- Fixed `-ngl` argument incompatibility for whisper-cli variants that do not support GPU layer flags.
- Fixed history text-size regression:
  - Slider affects refined/final transcript body text.
  - Original text stays intentionally compact/italic.

## [0.6.0] - 2026-02-16

### Added

- **VibeVoice-ASR Integration**: Speaker-diarized transcription via Python FastAPI sidecar
  - Microsoft VibeVoice-ASR 7B model support (MIT license)
  - Python sidecar with `/transcribe`, `/health`, `/reload-model` endpoints
  - HTTP client in Rust (`sidecar.rs`) with timeout and retry handling
  - Sidecar process management (start/stop/health/restart)
  - Auto-processing pipeline (chapters, minutes, summary)
- **Speaker Diarization UI** (E19): Color-coded speaker segments in transcript view
  - 8-color palette for distinct speakers
  - Editable speaker labels (click to rename)
  - Speaker-attributed export support
- **Analyse Button** (E24): Manual trigger for VibeVoice analysis
  - File picker with recordings directory default path
  - Auto-save mic recordings as OPUS (>10s threshold)
  - Progress indicator with loading spinner
  - Speaker diarization result rendering
- **Quality Preset Controls** (E26): OPUS + VibeVoice configuration UI
  - OPUS bitrate dropdown (32/64/96/128 kbps)
  - VibeVoice precision selector (FP16/INT8)
  - OPUS encoding toggle
  - System audio auto-save toggle
- **Parallel Mode** (E27): Run Whisper + VibeVoice simultaneously
  - `parallel_transcribe` command runs both engines concurrently
  - Side-by-side results display
  - System audio auto-save with 60s flush intervals
- **Model Monitoring** (E28): Weekly VibeVoice update check on startup
  - Toast notification when new version available
  - localStorage-based check interval (7 days)
- **PyInstaller Packaging** (E29): Standalone sidecar executable
  - PyInstaller spec file with hidden imports
  - Build script with `--onedir` and `--clean` flags
  - Sidecar auto-detects bundled exe vs Python fallback
- **OPUS Audio Encoding**: FFmpeg-based WAV→OPUS pipeline
  - Configurable bitrate, VBR, compression level
  - Smart filenames with session name or timestamp fallback
  - Bundled FFmpeg support + system PATH fallback
- **Recording Auto-Save**: Automatic OPUS saving for later analysis
  - Mic recordings >10s auto-saved on stop
  - System audio chunks accumulated and flushed every 60s
  - Recordings stored in `~/.local/share/trispr-flow/recordings/`

### Technical

- New Rust modules: `opus.rs`, `sidecar.rs`, `sidecar_process.rs`, `auto_processing.rs`
- New Cargo dependencies: `hound`, `chrono`, `tauri-plugin-dialog`
- New npm dependency: `@tauri-apps/plugin-dialog`
- Tauri v2 dialog plugin with capability permissions
- Python sidecar: FastAPI + uvicorn + transformers + torch + librosa
- 22 new E2E tests (block-e-e2e.test.ts)

## [0.5.0] - 2026-02-15

### Added

- **Tab-Based UI**: Transcription + Settings tabs (DEC-016)
  - Two-tab layout replacing single-page design
  - localStorage tab persistence
- **System Audio Rename**: "Output" renamed to "System Audio" (DEC-017)
- **Chapter Segmentation**: Automatic chapter markers in long transcripts
  - Silence-based, time-based (5min), and hybrid detection methods
  - Chapter settings in Settings tab
  - Conversation-only display by default (DEC-018)
- **Topic Detection**: Automatic topic badges on conversation entries
  - Keyword-based detection (tech, business, personal, creative, health)
  - Customizable keywords in Settings
  - Topic filter buttons in transcript view
- **Live Transcript Dump**: Crash recovery buffering via 5-sec intervals
- **Test Coverage**: Toast system (35+) and accessibility (25+) test suites

### Changed

- Conversation window detach functionality removed
- Import optimization: dynamic → static imports (bundle size reduced)

### Fixed

- Window state bug: invisible/minimized window on startup
  - Validates window position and dimensions before saving
  - Prevents saving minimized state (position ~-32000)

## [0.4.0] - 2026-02-09

### Added

- **Post-Processing Pipeline**: Intelligent transcript enhancement system
  - Rule-based text processing (punctuation, capitalization, numbers)
  - English/German language-specific rules
  - Custom vocabulary with word boundary matching
  - Settings-driven with backward compatibility
- **Punctuation Enhancement**: Automatic punctuation rules
  - Adds periods, commas before conjunctions (and, but, or)
  - Question mark detection (what, how, why, when, where, who, etc.)
  - Language-specific rules for English and German
- **Capitalization**: Sentence and proper word capitalization
  - First letter and after sentence-ending punctuation
  - English "I" always capitalized
  - German noun capitalization support
- **Number Normalization**: Convert spoken numbers to digits
  - Numbers 0-100 plus common tens
  - Word boundary matching ("one" → "1" but "someone" unchanged)
  - English and German number words
- **Custom Vocabulary**: User-defined word replacements
  - HashMap-based string replacement
  - Word boundary regex matching (prevents partial replacements)
  - Dynamic UI table for managing entries
  - Add/remove functionality with instant persistence
- **Post-Processing UI**: Complete settings panel
  - Master toggle to enable/disable
  - Language selector for rule customization
  - Individual toggles for each enhancement type
  - Custom Vocabulary expander with styled table
  - Clean, responsive design with grid layout

### Technical

- New `postprocessing.rs` module with 3-stage pipeline architecture
- Integration at 3 transcription emission points (mic PTT, mic VAD, system audio)
- Error handling with fallback to original text (never lose data)
- Comprehensive unit tests (24 tests covering all functions)
- Settings backward compatibility via #[serde(default)]
- Added regex crate dependency for vocabulary matching
- Non-invasive integration (post-filtering, pre-history)

### Changed

- HTML bundle size: 39.90 kB → 44.70 kB (+12%)
- CSS bundle size: 25.08 kB → 26.10 kB (+4%)
- Main.js bundle size: 59.92 kB → 62.33 kB (+4%)

## [0.3.0] - 2026-02-09

### Added

- **Language Pinning**: Pin transcription to specific language instead of auto-detection
  - Support for 16 languages: EN, DE, FR, ES, IT, PT, NL, PL, RU, JA, KO, ZH, AR, TR, HI
  - UI dropdown for language selection with pin toggle
- **Activation Words**: Filter transcripts to only process those containing trigger words
  - Word boundary matching algorithm (case-insensitive)
  - Default word list: ["computer", "hey assistant"]
  - Configurable via textarea in Capture Input panel
- **Hallucination Filter UI**: Toggle for existing hallucination filter
  - Previously only accessible via settings.json
  - Now user-facing checkbox in Capture Input panel
- **Extra Hotkey**: Toggle activation words on/off (Ctrl+Shift+A)
  - Integrated into UX/UI-Adjustments panel
  - Audio cue feedback on toggle
  - Settings persistence across sessions

### Fixed

- **Critical Bug**: Fixed hardcoded language parameter in transcription.rs (line 887)
  - Language setting was ignored, always using "auto"
  - Now respects user's language_mode setting when pinned

### Changed

- Hotkey configuration moved from standalone panel to UX/UI-Adjustments section
- HTML bundle size reduced from 41.06 kB to 39.90 kB

## [0.2.0] - 2026-02-08

### Added

- **Window Behavior Persistence**: Main and conversation window geometry restored across sessions
  - Position, size, and monitor tracking
  - 500ms debounce to prevent excessive I/O
  - Monitor awareness with fallback to primary if unplugged
- **Activity Feedback**: Tray icon pulsing for recording/transcribing states
  - Turquoise pulse = Recording, Yellow = Transcribing
  - ~1.6s loop with 6 frames
  - Thread-safe state management with atomic variables
- **Conversation Window Configuration**: Always-on-top toggle with persistence
- **Model Hot Swap**: Switch models without app restart
  - Seamless transcription restart if active
  - Rollback to previous model on failure
  - Success/error toast feedback

### Changed

- Model Manager UI: Hero model display expanded to 2 lines for longer names

## [0.1.0] - 2026-02-07

### Added

- **Core Platform Stability**
  - Frontend modularized into 14 TypeScript modules
  - Backend modularized (audio, transcription, models, state, paths)
  - Security hardening (URL safety, checksum verification, download limits)
  - Unit + smoke baseline established
- **Model Manager**
  - Single-list layout with active-first sorting
  - 2-column grid display
  - Optimize button for q5_0 quantization (~30% size reduction)
  - Quantizer bundled in NSIS installer
- **Capture & Transcription**
  - Input/Output capture flows with dedicated panels
  - Recording/transcribing activity indicators
  - Transcribe defaults to disabled on startup (session-only)
- **Overlay System**
  - Dot and KITT overlay styles
  - Runtime toggle via tray menu
  - Style switching stabilized

### Fixed

- German turbo model download URL and filename mapping
- Model delete vs remove semantics for internal/external models

---

## Release Notes

### v0.6.0 Highlights

This release delivers **VibeVoice-ASR Integration** for speaker-aware meeting transcription:

- **Speaker diarization** with color-coded segments and editable labels
- **Analyse button** for manual VibeVoice analysis with file picker
- **OPUS audio encoding** (75% size reduction vs WAV)
- **Parallel mode** for simultaneous Whisper + VibeVoice transcription
- **Quality presets** for OPUS bitrate and VibeVoice precision
- **PyInstaller packaging** for standalone sidecar deployment

### What's Next?

Current focus is **v0.7.1 stabilization** with:

- UX/UI consistency and settings IA cleanup (Block E)
- Reliability hardening and release QA (Block F)
- Latency benchmark baseline + runtime-start validation

Cloud provider rollout (OpenAI/Claude/Gemini) remains planned for **v0.7.3**.

See [ROADMAP.md](ROADMAP.md) for the full development plan.
