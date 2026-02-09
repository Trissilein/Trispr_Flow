# Roadmap - Trispr Flow

Last updated: 2026-02-09

This roadmap tracks the current focus: getting core capture + transcription stable and tightening UX before expanding features.

---

## Current Status

✅ **Milestone 0**: Complete (tech stack locked, whisper.cpp validated)
✅ **Milestone 1**: Complete (PTT capture, transcription, paste)
✅ **Milestone 2**: Complete (Foundation & Critical UX)
✅ **Milestone 3**: Complete (Window persistence, Activity feedback, Model hot swap, Capture enhancements, Post-processing pipeline)
🔄 **Phase 2**: In Progress (Documentation & Stabilization)

**Recent progress (2026-02-09)**

- ✅ **Post-Processing Pipeline (v0.4.0)**: Rule-based text enhancements, custom vocabulary, settings-driven architecture
- ✅ **Rule-Based Enhancements**: Punctuation, capitalization, number normalization (English/German)
- ✅ **Custom Vocabulary**: HashMap-based replacements with word boundary regex matching
- ✅ **Post-Processing UI**: Master toggle, language selector, rule toggles, dynamic vocabulary table
- ✅ **Capture Enhancements (v0.3.0)**: Language pinning (16 languages), activation word filtering, hallucination filter UI
- ✅ **Language Bug Fix**: Fixed hardcoded "auto" in transcription.rs line 887
- ✅ **Version Bumps**: 0.1.0 → 0.3.0 → 0.4.0

### Next Priorities (v0.5.0+)

1. **Long-Form Features** (v0.5.0)
   - Live transcript dump (TXT, MD, JSON)
   - Chapter segmentation for meetings/lectures
   - Topic detection and marking

2. **VibeVoice-ASR Integration** (v0.6.0)
   - Speaker diarization for recorded meetings
   - Python FastAPI sidecar architecture
   - Post-session analysis workflow

3. **AI Fallback Overhaul** (v0.7.0)
   - Multi-provider support (OpenAI, Anthropic, Groq)
   - User-selectable models per provider
   - Streaming transcription for cloud fallback

**Recent progress (2026-02-08)**

- ✅ **Frontend Modularization**: Split main.ts (~1800 lines) into 14 focused modules (~220 lines)
- ✅ **Overlay Circle Dot Fix**: Audio-reactive size animation now functional
- ✅ **Overlay Lifecycle Stabilization**: Dot/KITT style switching now treated as explicit lifecycle transitions
- ✅ **Overlay Tray Toggle**: Dedicated tray toggle to fully disable/enable overlay runtime
- ✅ **Monitoring Toggles**: Enable/disable microphone tracking and system audio transcription via UI
- ✅ **Tray Menu Sync**: Checkmarks properly sync between UI and system tray
- ✅ **Monitor Re-initialization**: No restart required when toggling monitoring on/off
- ✅ **lib.rs Modularization**: Split backend into focused Rust modules
- ✅ **Security Hardening**: URL safety (no whitelist), checksum verification, download size limits
- ✅ **System Audio Robustness**: WASAPI loopback fixes + transcribe queue/idle meter
- ✅ **Activity Indicators**: Separate recording/transcribing indicators + overlay marker
- ✅ **Automated Testing Baseline**: Unit tests + smoke scripts verified locally
- ✅ **Transcribe Default Disabled**: Session-only enable; always deactivated on startup
- ✅ **Model Manager**: Single-list layout, active-first ordering, 2-column grid
- ✅ **Model Downloads**: German turbo URL fix + filename mapping
- ✅ **Model Removal**: Delete (internal) vs Remove (external) with rescan on Refresh
- ✅ **Model Quantization**: Optimize button for q5_0 compression (~30% size reduction), all variants labeled
- ✅ **Quantizer Bundling**: quantize.exe integrated into NSIS installer, build scripts for updates
- ✅ **Hero UI**: Model display expanded to 2 lines for longer quantized model names

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
- ✅ **SSRF Prevention**: URL safety checks (no whitelist) for model downloads
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

## Milestone 3 — Quality of Life & Advanced Features (Complete ✅)

### Window Behavior ✅

- ✅ Persist main window position + size across sessions
- ✅ Restore on correct monitor
- ✅ Restore on same virtual desktop (Windows) — handled implicitly by OS
- ✅ Conversation window geometry persistence
- ✅ Always-on-top toggle for conversation window

### Activity Feedback ✅

- ✅ **In‑app indicators**: Separate recording/transcribing indicators + overlay marker
- ✅ **Overlay style lifecycle**: Dot/KITT switching and overlay runtime toggle documented as stable
- ✅ **Tray pulse**: turquoise = Recording, yellow = Transcribing; both pulse when both active
- ✅ **Pulse cadence**: ~1.6s loop, ~6 frames
- ⏳ **Transcribe backlog**: target 10 minutes (future enhancement)
- ⏳ **80% warning**: prompt +50% expansion (future enhancement)

### Model Manager QoL ✅

- ✅ Apply model immediately without restart (hot swap with rollback on failure)

### Capture Enhancements ✅

- ✅ Activation words with word boundary matching (case-insensitive)
- ✅ Language pinning (16 languages: EN, DE, FR, ES, IT, PT, NL, PL, RU, JA, KO, ZH, AR, TR, HI)
- ✅ Hallucination filter UI toggle
- ✅ Extra hotkey: Toggle activation words (Ctrl+Shift+A)

### Text Enhancement ✅

- ✅ **Post-Processing Pipeline** (v0.4.0):
  - ✅ Rule-based punctuation & capitalization (English, German)
  - ✅ Number normalization (0-100 + common tens)
  - ✅ Custom vocabulary with word boundary matching
  - ✅ Settings-driven with backward compatibility
  - ✅ Complete UI panel with master toggle, language selector, rule toggles
  - ✅ Dynamic vocabulary table with add/remove functionality
  - ⏳ Optional Claude API integration (planned for future release)

### Speaker-Aware Meeting Transcription (VibeVoice-ASR Integration)

- **Goal**: After a meeting/recording session, analyze the full audio and produce a speaker-diarized transcript
- **Model**: Microsoft VibeVoice-ASR 7B (MIT license, open-source)
  - Up to 60 minutes continuous audio in a single pass
  - Built-in speaker diarization (who spoke when)
  - Timestamps per segment
  - 50+ languages, customizable hotwords
  - Requires ~14-16 GB VRAM (FP16) or ~7-8 GB (INT8 quantized)
- **Architecture**: Python FastAPI sidecar in `sidecar/vibevoice-asr/`
  - Runs as background process on localhost (no user-facing UI)
  - Lazy model loading (first "Analyse" click loads model into VRAM)
  - Tauri manages sidecar lifecycle (start/stop/health-check)
  - Packaged as standalone `.exe` via PyInstaller (no Python required on user machine)
- **Audio Format**: WAV (16-bit, 24kHz mono, ~170 MB/60 min) — simplest to capture, universally compatible. Migration to Opus possible later if storage matters.
- **Workflow**:
  1. Output Capture records meeting audio → saves WAV file in background
  2. User clicks "Analyse" button after meeting
  3. WAV sent to local VibeVoice-ASR server → POST `/transcribe`
  4. Server returns JSON: `{ segments: [{ speaker_id, start_time, end_time, text }] }`
  5. Frontend renders speaker-diarized transcript (color-coded by speaker)
- **Coexistence with Whisper**: Sequential — Whisper handles live transcription during recording, VibeVoice-ASR does post-session analysis with speaker separation
- **Tasks**:
  - ⚪ Set up `sidecar/vibevoice-asr/` project structure (FastAPI + requirements)
  - ⚪ Implement `/transcribe` endpoint with VibeVoice-ASR model
  - ⚪ Add WAV recording to Output Capture pipeline (save alongside live transcription)
  - ⚪ Rust: sidecar process management (start/stop/health)
  - ⚪ Frontend: "Analyse" button + speaker-diarized transcript view
  - ⚪ PyInstaller packaging for standalone sidecar executable

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
