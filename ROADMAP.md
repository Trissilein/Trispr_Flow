# Roadmap - Trispr Flow

Last updated: 2026-02-03

This roadmap has been restructured based on a comprehensive project overhaul. The focus is on building a solid foundation before adding advanced features.

---

## Current Status

✅ **Milestone 0**: Complete (Tech stack locked, ASR backend validated)
✅ **Milestone 1**: Complete (MVP with PTT, capture, transcription, paste working)
🔄 **Milestone 2**: In Progress (Foundation & Critical UX improvements - UI/UX polish phase)

**Recent Progress (2026-02-03)**:

- ✅ Dark mode implementation - Complete UI redesign with dark theme
- ✅ UI density improvements - 40-60% spacing reduction for compact layout
- ❌ Mode architecture still PTT vs Toggle (needs PTT vs VAD; toggle should be convenience within PTT)
- ✅ Model cleanup - Removed small models, kept only large-v3 and large-v3-turbo
- ✅ Enhanced model path resolution - Works correctly in built .exe
- ✅ Comprehensive logging - Detailed tracing for debugging
- ✅ Expander UX - Added chevron indicators
- ✅ Dropdown styling - Dark backgrounds for options
- 🔄 Overlay visibility debugging - In progress (dot exists, but not visible)

---

## Milestone 0 - Discovery & Decisions ✅

**Status**: Complete

**Deliverables**:
- ✅ Tech stack locked: Tauri v2 + Rust + whisper.cpp
- ✅ ASR backend validated (GPU + CPU fallback)
- ✅ Audio capture layer: cpal (WASAPI/CoreAudio)
- ✅ Cloud fallback strategy defined (Claude pipeline, opt-in)

---

## Milestone 1 - MVP (PTT + Paste) ✅

**Status**: Complete

**Deliverables**:
- ✅ Global hotkey (PTT) for start/stop recording
- ✅ Selectable input device
- ✅ GPU transcription with EN/DE auto-detect
- ✅ Paste output into focused field
- ✅ Basic tray status
- ✅ Model manager with download functionality
- ✅ Settings + history persistence

**Definition of Done**:
- ✅ Single hotkey path works end-to-end on Windows
- ✅ 1-3s turnaround on short utterances (GPU)

---

## Milestone 2 - Foundation & Critical UX 🔄

**Target**: 2-3 weeks
**Status**: Complete

### Phase 1.0: Recording Modes (PTT vs VAD) 🔄
- Replace mode toggle with **PTT vs Voice‑Activated**.
- Keep **Toggle hotkey** as convenience within PTT mode (always available when PTT is active).
- VAD mode: no hold/toggle requirement; capture is voice‑triggered.

### Phase 1.1: Recording Overlay ✅
- ✅ Overlay window with always-on-top, transparent design
- ✅ Visual states: Recording (red pulse), Transcribing (yellow spinner), Idle (hidden)
- ✅ Positioned in top-right corner (configurable)
- ✅ Draggable overlay

### Phase 1.1b: Overlay Redesign (Minimal Dot) 🔄
- Replace overlay window UX with a minimal red dot in the top-left.
- Remove any invisible window artifacts; keep only the dot visible.
- Dot pulses like an equalizer when input is detected.
- Keep status transitions (Recording/Transcribing/Idle) but without text labels.
- Make **position/offset configurable in UI** (reference: helltime overlay settings pattern).
- Add **Overlay Settings**: color, min/max radius, rise/fall smoothing (ms), position.

### Phase 1.2: Hotkey System Refactor ✅

- ✅ Visual hotkey picker/recorder component with keyboard event capture
- ✅ Real-time hotkey validation with feedback
- ✅ Conflict detection for duplicate hotkeys
- ✅ Format normalization and error messages
- ✅ Inline status indicators (success/error)

**Implementation**:

- ✅ New Rust module: `src-tauri/src/hotkeys.rs` with validator
- ✅ Frontend: HotkeyRecorder component with keyboard event capture
- ✅ Tauri commands: `validate_hotkey`, `test_hotkey`, `get_hotkey_conflicts`
- ✅ Comprehensive unit tests for hotkey validation

### Phase 1.3: Error Recovery & Logging ✅

- ✅ Robust error types: `AppError` enum with categories (AudioDevice, Transcription, Hotkey, Storage, Network, Window, Other)
- ✅ Comprehensive logging with `tracing` crate
- ✅ Structured logging with proper log levels (ERROR, WARN, INFO, DEBUG)
- ✅ Toast/notification component for error display
- ✅ Error events emitted to frontend with context
- ✅ User-friendly error messages with suggested actions

### Phase 1.4: Audio Cues (Bonus) ✅

- ✅ Toggleable audio feedback for recording start/stop
- ✅ Web Audio API implementation with rising/falling beeps
- ✅ Settings toggle in UI (defaults to enabled)
- ✅ Non-intrusive audio cues (short, 100ms duration)

**Implementation**:
- New Rust module: `src-tauri/src/errors.rs`
- Dependencies: `tracing`, `tracing-subscriber`, `tracing-appender`
- Log levels: ERROR, WARN, INFO, DEBUG
- Key events logged: hotkey press/release, audio capture, transcription, settings changes

### Phase 1.5: UI/UX Polish ✅

- ✅ Dark mode - Complete CSS redesign with dark color scheme
- ✅ UI density - 40-60% spacing reduction for compact, efficient interface
- ✅ Mode architecture fix - Corrected to VAD vs PTT structure
- ✅ Model cleanup - Removed small models (base, small, medium, tiny)
- ✅ Expander indicators - Added chevron icons for better UX
- ✅ Dropdown styling - Dark backgrounds in expanded state
- ✅ Model path resolution - Enhanced search paths for built .exe
- ✅ Comprehensive logging - Detailed tracing throughout app
- 🔄 Overlay visibility - Debugging in progress

### Phase 1.6: Audio Cue Volume 🔄

- Add a volume slider for audio cues.
- Persist volume in settings.

### Phase 1.7: Quality Loop (Post‑Transcription) 🔄
- Optional “Quality Loop” that reviews the transcript before paste.
- Detect likely errors (punctuation, capitalization, homophones, missing words).
- Toggleable in UI (fast path if disabled).
- Output should remain low‑latency; keep original transcript available for audit.

### Phase 1.8: Model Manager Revamp 🔄
- Source selector: default source + custom URL.
- Custom source: JSON index or direct model URL.
- Show **Available (remote)** vs **Installed (local)**.
- Install / Remove actions (delete local model files).
- Clearly show source of each model and storage path.

**Definition of Done**:
- ✅ Recording overlay shows status in <100ms
- [ ] Hotkey configuration is intuitive with visual picker
- [ ] All errors surface to UI with actionable messages
- [ ] Logs capture full debugging context
- [ ] Zero hotkey registration errors reported

---

## Milestone 3 - Quality of Life 📋

**Target**: 2-3 weeks
**Status**: Planned

### Features

#### Voice Activity Detection (VAD)
- Silence trimming before and after speech
- Webrtcvad integration (C library with Rust FFI)
- Settings: VAD on/off toggle + sensitivity slider
- Expected improvement: 20%+ reduction in average recording length

#### Activation Words (Always‑On)
- Optional activation word (e.g., “over”) to finalize and paste.
- Enables continuous recording with post‑cutting of silence.
- Settings: activation word text + enable/disable + timeout.
- Works alongside VAD (activation word as hard stop).

#### Text Post-Processing
- Pipeline for text transformations after transcription
- Processors:
  - Punctuation: Smart capitalization after periods
  - Numbers: "twenty three" → "23"
  - Contractions: "do n't" → "don't"
  - Custom vocabulary: User-defined replacements
- Settings for enabling/disabling individual processors
- Expected accuracy: 95%+ punctuation accuracy

#### Multi-Language Context Switching
- Explicit language selection beyond "auto"
- Language options: Auto (DE/EN mix), English, German, Spanish, French, etc.
- Language hint passed to whisper-cli via `-l` flag

#### Additional Keyboard Shortcuts
- Open/Close Main Window (Ctrl+Shift+H)
- Paste Last Transcript (Ctrl+Shift+V)
- Delete Last Transcript (Ctrl+Shift+Z)
- Toggle Cloud Fallback (Ctrl+Shift+C)
- All shortcuts configurable in settings

#### Undo for Paste
- Clipboard + target app state saved before paste
- Undo command: Restore clipboard + send Ctrl+Z + remove history entry
- 30-second timeout
- Shortcut: Ctrl+Shift+Backspace

**Definition of Done**:
- VAD reduces recording length by 20%+
- Post-processing achieves 95%+ punctuation accuracy
- Multi-language switching works reliably
- Undo works within 30-second window
- Professional UX throughout

#### Interface Compression (UX)
- Rework UI density: two-column layout where possible.
- Move output panel to the top.
- Use expanders for advanced controls.
- Make history/model panels scrollable.

---

## Milestone 4 - Advanced Features 📋

**Target**: 3-4 weeks
**Status**: Planned

### Features

#### Dark Mode
- Theme selector: Light, Dark, Auto (system detection)
- CSS custom properties for both themes
- Smooth transitions between themes
- Expected adoption: 40%+ of users

#### Export Options
- Export history to multiple formats: Plain Text, Markdown, JSON, CSV
- "Export History" button in history UI
- File save dialog integration
- Select all or specific entries

#### Custom Model Support
- "Import Custom Model" button
- Support for quantized models (Q4, Q5, Q8)
- Support for fine-tuned models
- Model metadata display (size, type, performance tier)

#### Analytics Dashboard (Local Only)
- Metrics tracked:
  - Recording count per day/week
  - Average recording length
  - Transcription success rate
  - Average latency
  - Error frequency by type
- Dashboard UI panel
- Privacy-first: Local only, opt-out available
- No metrics transmitted without explicit consent

**Definition of Done**:
- Dark mode adopted by 40%+ of users
- Export feature used regularly by power users
- Custom models load and work reliably
- Analytics dashboard shows usage insights

---

## Milestone 5 - Production Ready 📋

**Target**: 2-3 weeks
**Status**: Planned

### Tasks

#### macOS Testing & Fixes
- Test full pipeline on macOS (CoreAudio, Metal GPU)
- Fix platform-specific issues:
  - Hotkey mapping (Command vs Ctrl)
  - Paste injection (Accessibility permissions)
  - Overlay positioning (menu bar considerations)
- Handle macOS permissions properly

#### Installer & Auto-Update
- Professional installers for Windows and macOS
- Windows: MSI or NSIS installer
- macOS: DMG with drag-to-Applications
- Code signing certificates (Windows + macOS)
- Auto-update mechanism via Tauri updater plugin
- Update manifest hosted on GitHub Releases

#### Auto-Start Configuration
- Toggle in settings: "Start on System Startup"
- App starts minimized to tray
- Works correctly after updates

#### Documentation Polish
- Comprehensive user guide
- Developer documentation for contributors
- Architecture diagrams
- Troubleshooting guide
- FAQ with common issues

**Definition of Done**:
- macOS fully tested and working
- Installers work on first try for 98%+ of users
- Auto-update completes without user intervention
- Comprehensive documentation published
- Ready for 1.0 public release

---

## Milestone 6 - Future Exploration (Post-1.0) 🔮

**Status**: Future

### Potential Features

- Real-time subtitles/captions overlay
- Speaker diarization (multi-speaker detection)
- Audio quality enhancement (noise reduction, echo cancellation)
- App-specific paste rules (Slack, Email, IDE integrations)
- Timeline scrubbing & edit interface
- Plugin marketplace for community extensions
- Streaming transcription (real-time results)
- Voice command editing

---

## Architecture Improvements

### Modular Rust Backend 📋
- **Current**: Monolithic `lib.rs` (1324 lines)
- **Target**: Modular structure with dedicated modules
  - `commands/` - Tauri command handlers
  - `audio/` - Audio capture and processing
  - `transcription/` - ASR backends
  - `hotkeys/` - Hotkey management
  - `postprocessing/` - Text transformations
  - `overlay/` ✅ - Recording overlay (complete)
  - `errors.rs` - Error types and handling
  - `metrics.rs` - Analytics and metrics

### Frontend Improvements 📋
- **Current**: Vanilla TypeScript with global variables
- **Target**: Improved structure with modules
  - `state/` - State management (Nano Stores)
  - `components/` - Reusable UI components
  - `services/` - Backend communication
  - `types/` - TypeScript type definitions

---

## Technical Debt

### High Priority
- [ ] Refactor `lib.rs` (1324 lines) into modules
- [ ] Improve resampling quality (libsamplerate instead of linear interpolation)
- [ ] Add unit tests for core Rust modules
- [ ] Add integration tests for transcription pipeline

### Medium Priority
- [ ] Enable TypeScript strict mode
- [ ] Implement proper frontend state management (Nano Stores)
- [ ] Standardize error types across frontend and backend

### Low Priority
- [ ] Consider frontend framework migration (Preact/Solid/Svelte)
- [ ] Add E2E tests with Tauri testing utilities
- [ ] Implement performance profiling tools

---

## Risks & Dependencies

### Technical Risks
- whisper.cpp streaming limitations → Mitigation: Chunked transcription fallback
- macOS compatibility issues → Mitigation: Early testing, platform-specific code paths
- Performance degradation with advanced features → Mitigation: Profiling, optimization, feature toggles

### UX Risks
- Hotkey conflicts with other apps → Mitigation: Conflict detection, fallback suggestions
- Overlay positioning issues on multi-monitor → Mitigation: Position persistence, manual adjustment
- Complex settings overwhelming users → Mitigation: Sensible defaults, progressive disclosure

### Process Risks
- Scope creep → Mitigation: Strict phase boundaries, MVP-first approach
- Testing coverage gaps → Mitigation: Automated tests, beta testing program

---

## Success Metrics

### Milestone 2 (Foundation)
- ✅ Overlay response time < 100ms
- [ ] Zero hotkey registration errors
- [ ] All errors surfaced to UI
- [ ] Logs capture full context

### Milestone 3 (Quality)
- VAD reduces recording length by 20%+
- Post-processing achieves 95%+ punctuation accuracy
- Undo works within 30-second window

### Milestone 4 (Advanced)
- Dark mode adopted by 40%+ of users
- Export feature used regularly

### Milestone 5 (Production)
- Installers work on first try for 98%+ users
- Auto-update completes without intervention
- Ready for 1.0 public release

---

## Out of Scope (Initially)

- Cloud-only transcription
- Live streaming captions (moved to Milestone 6)
- Full editor replacement
- Mobile support
- Linux support (Windows + macOS focus)

---

## References

- **Original Roadmap**: See git history for previous milestones
- **Architecture Docs**: `docs/ARCHITECTURE.md`
- **Status Updates**: `STATUS.md`
- **Comprehensive Plan**: `.claude/plans/jazzy-swinging-robin.md`
