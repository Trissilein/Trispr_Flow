# Changelog

All notable changes to Trispr Flow will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

### v0.3.0 Highlights

This release focuses on **Capture Enhancements** to give users better control over transcription quality and workflow:

- **16-language support** with pinning (no more forced auto-detection!)
- **Activation word filtering** to only process relevant speech
- **Hallucination filter** now accessible via UI
- **Bug fix**: Language setting is now properly respected

These features complete **Milestone 3** (Quality of Life & Advanced Features), making Trispr Flow significantly more flexible for diverse use cases.

### What's Next?

Version 0.4.0 will introduce the **Post-Processing Pipeline** with:
- Rule-based punctuation & capitalization
- Number normalization
- Custom vocabulary support
- Optional LLM refinement via Claude API

See [ROADMAP.md](ROADMAP.md) for the full development plan.
