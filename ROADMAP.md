# Roadmap - Trispr Flow

Last updated: 2026-02-07

This roadmap tracks the current focus: getting core capture + transcription stable and tightening UX before expanding features.

---

## Current Status

✅ **Milestone 0**: Complete (tech stack locked, whisper.cpp validated)
✅ **Milestone 1**: Complete (PTT capture, transcription, paste)
✅ **Milestone 2**: Complete (Foundation & Critical UX)
🔄 **Phase 2**: In Progress (Documentation & Stabilization)

**Recent progress (2026-02-07)**
- ✅ **Frontend Modularization**: Split main.ts (~1800 lines) into 14 focused modules (~220 lines)
- ✅ **Overlay Circle Dot Fix**: Audio-reactive size animation now functional
- ✅ **Overlay Lifecycle Stabilization**: Dot/KITT style switching now treated as explicit lifecycle transitions
- ✅ **Overlay Tray Toggle**: Dedicated tray toggle to fully disable/enable overlay runtime
- ✅ **Monitoring Toggles**: Enable/disable microphone tracking and system audio transcription via UI
- ✅ **Tray Menu Sync**: Checkmarks properly sync between UI and system tray
- ✅ **Monitor Re-initialization**: No restart required when toggling monitoring on/off
- ✅ **lib.rs Modularization**: Split backend into focused Rust modules
- ✅ **Security Hardening**: SSRF prevention, checksum verification, download size limits
- ✅ **System Audio Robustness**: WASAPI loopback fixes + transcribe queue/idle meter
- ✅ **Activity Indicators**: Separate recording/transcribing indicators + overlay marker
- ✅ **Automated Testing Baseline**: Unit tests + smoke scripts verified locally
- ✅ **Transcribe Default Disabled**: Session-only enable; always deactivated on startup

**Previous milestones**
- ✅ System audio capture via WASAPI (Windows) + transcribe hotkey
- ✅ Output tabs: Microphone / System Audio / Conversation
- ✅ Conversation view combining mic + system transcripts
- ✅ Output meters with dB readouts + threshold markers
- ✅ Input gain for mic + system audio (±30 dB)
- ✅ Panel collapse state + compact layout
- ✅ Audio cue volume control
- ✅ Model Manager revamp (sources, storage picker, install/remove)

---

## Milestone 2 — Foundation & Critical UX (Complete)

### 2.1 Recording Modes (Mic)
- **PTT vs VAD** modes (toggle hotkey remains inside PTT)
- VAD thresholds + silence grace

### 2.2 System Audio Transcription (Windows)
- WASAPI loopback capture
- Transcribe hotkey toggle
- VAD option + chunking controls
- Output meter + dB display

### 2.3 Overlay Redesign (Minimal Dot) ✅
- Visible dot only (no invisible window artifacts)
- Audio-reactive size (min/max radius) ✅
- Color + active/inactive opacity ✅
- Rise/fall smoothing ✅
- Position controls (X/Y) ✅
- **KITT bar mode** (alternative overlay style) ✅

### 2.4 Conversation View ✅
- Combined mic/system transcript stream ✅
- Detachable conversation window (stable content + close) ✅
- Font size control ✅

### 2.5 Model Manager Revamp ✅
- Source selector (default + custom URL) ✅
- Show **available** vs **installed** models ✅
- Install / remove actions ✅
- Per-model storage path display ✅

### 2.6 Code Quality & Maintainability ✅
- Frontend modularization (14 specialized modules) ✅
- TypeScript type safety improvements ✅
- DOM reference centralization ✅
- Event listener organization ✅

**Definition of Done** ✅
- System audio meter/gain calibrated and VAD threshold accurate ✅
- Conversation detach window fully functional ✅
- Frontend codebase maintainable and modular ✅

---

## Phase 2 — Security Hardening & Code Quality (In Progress)

### Critical Security Tasks (This Week)
- ✅ **SSRF Prevention**: URL whitelist for model downloads
- ✅ **Model Integrity**: SHA256 checksum verification
- ✅ **Download Limits**: Size caps and timeout protection

### Code Refactoring (Next Sprint)
- ✅ **lib.rs Modularization**: Split 3700+ line file into focused modules
  - Audio module (device management, CPAL)
  - Transcription module (whisper.cpp integration)
  - Models module (download, management)
  - State/Settings module
  - Paths/Utilities module
- ✅ **Automated Testing**: Unit + smoke baseline verified locally
- 🟡 **Documentation**: Architecture docs, code comments

### Testing (Ongoing)
- ✅ **Automated Testing**: Unit tests + smoke command verified (`npm run test` + `npm run test:smoke`)
- ⚪ **Tauri E2E Tests (Block 9)**: Optional end-to-end coverage once unit + smoke are stable

### Documentation Sprint (Current)
- 🔄 Sync `ROADMAP.md` and `.claude/ROADMAP.md` after each major feature decision
- 🔄 Keep architecture/state docs aligned with current overlay and transcription behavior
- 🔄 Consolidate completed items from working notes into stable docs (`progress.txt`, `APP_FLOW.md`)

For detailed technical roadmap, see [.claude/ROADMAP.md](.claude/ROADMAP.md)

---

## Milestone 3 — Quality of Life & Advanced Features (Planned)

### Window Behavior
- Persist main window position + size across sessions
- Restore on correct monitor
- Restore on same virtual desktop (Windows), if possible

### Activity Feedback
- ✅ **In‑app indicators**: Separate recording/transcribing indicators + overlay marker
- ✅ **Overlay style lifecycle**: Dot/KITT switching and overlay runtime toggle documented as stable
- ⏳ **Tray pulse**: turquoise = Recording, yellow = Transcribing; both pulse when both active
- ⏳ **Pulse cadence**: ~1.6s loop, ~6 frames
- ⏳ **Transcribe backlog**: target 10 minutes
- ⏳ **80% warning**: prompt +50% expansion (repeatable)

### Capture Enhancements
- Activation words ("over" / "stop") for continuous capture
- Language pinning beyond auto-detect
- Extra hotkeys (paste last, undo, toggle cloud)

### Text Enhancement
- **Post-Processing Pipeline** (after Capture Enhancements):
  - Punctuation & capitalization (rule-based + AI-powered)
  - Number normalization (digits, dates, currency)
  - Custom vocabulary (technical terms, proper nouns)
  - Domain-aware corrections
  - Optional Claude API integration for advanced processing
- **Language-specific rules** (English, German)

### Long-Form Transcription
- **Live Transcript Dump**: Export ongoing transcripts (TXT, MD, JSON)
- **Chapter Summarization**: Automatic segmentation for meetings, lectures
- **Topic Detection**: Identify and mark topic shifts

### Conversation Window (Later)
- Make the conversation window configurable (size, position, font size, always-on-top)

---

## Planning Queue — AI Fallback Overhaul (Next 3–4 steps, planning only)
Goal: replace “Claude fallback” with **AI Fallback** that supports multiple providers and user‑selectable models.

**Providers**
- Claude
- OpenAI (ChatGPT)
- Gemini

**Planning steps**
1. **Requirements & UX**  
   - Rename UI to **AI Fallback** (global status + settings section).  
   - Decide where config lives (Model panel or dedicated AI section).  
   - Toggle behavior and when post‑processing runs.
2. **Provider Config Design**  
   - Per‑provider model selection.  
   - API key / account linking flow.  
   - Provider‑specific limits and validation.
3. **Data Model & Settings**  
   - Settings schema for provider, model, key storage, enabled state.  
   - Migration from existing `cloud_fallback`.
4. **Prompt Strategy**  
   - Default post‑process prompt.  
   - User‑editable prompt with reset.

---

## Milestone 4 — Production Ready (Planned)
- macOS testing + fixes
- Professional installers + updater
- Autostart
- Documentation polish

---

## Technical Debt / Risks
- Split monolithic `lib.rs` into modules
- Improve resampling quality (libsamplerate)
- Add tests for audio + transcription pipeline
