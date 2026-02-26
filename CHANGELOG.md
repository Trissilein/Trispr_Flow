# Changelog

All notable changes to Trispr Flow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

Version 0.7.0 will introduce the **AI Fallback Overhaul** with:

- Multi-provider support (Claude, OpenAI, Gemini)
- User-editable post-processing prompts
- Per-provider model selection and API key management

See [ROADMAP.md](ROADMAP.md) for the full development plan.
