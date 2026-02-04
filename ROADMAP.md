# Roadmap - Trispr Flow

Last updated: 2026-02-04

This roadmap tracks the current focus: getting core capture + transcription stable and tightening UX before expanding features.

---

## Current Status

✅ **Milestone 0**: Complete (tech stack locked, whisper.cpp validated)
✅ **Milestone 1**: Complete (PTT capture, transcription, paste)
✅ **Milestone 2**: Complete (Foundation & Critical UX)
🔄 **Phase 2**: In Progress (Security Hardening & Code Quality)

**Recent progress (2026-02-04)**
- ✅ **Frontend Modularization**: Split main.ts (~1800 lines) into 14 focused modules (~220 lines)
- ✅ **Overlay Circle Dot Fix**: Audio-reactive size animation now functional
- ✅ **Monitoring Toggles**: Enable/disable microphone tracking and system audio transcription via UI
- ✅ **Tray Menu Sync**: Checkmarks properly sync between UI and system tray
- ✅ **Monitor Re-initialization**: No restart required when toggling monitoring on/off

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

## Milestone 2 — Foundation & Critical UX (In Progress)

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
- 🔴 **SSRF Prevention**: URL whitelist for model downloads
- 🔴 **Model Integrity**: SHA256 checksum verification
- 🔴 **Download Limits**: Size caps and timeout protection

### Code Refactoring (Next Sprint)
- 🟡 **lib.rs Modularization**: Split 3700+ line file into focused modules
  - Audio module (device management, CPAL)
  - Transcription module (whisper.cpp integration)
  - Models module (download, management)
  - State/Settings module
  - Paths/Utilities module
- 🟡 **Automated Testing**: Unit tests for critical paths
- 🟡 **Documentation**: Architecture docs, code comments

For detailed technical roadmap, see [.claude/ROADMAP.md](.claude/ROADMAP.md)

---

## Milestone 3 — Quality of Life & Advanced Features (Planned)

### Text Enhancement
- **Post-Processing Pipeline**:
  - Punctuation & capitalization (rule-based + AI-powered)
  - Number normalization (digits, dates, currency)
  - Custom vocabulary (technical terms, proper nouns)
  - Domain-aware corrections
  - Optional Claude API integration for advanced processing
- **Language-specific rules** (English, German)

### Capture Enhancements
- Activation words ("over" / "stop") for continuous capture
- Language pinning beyond auto-detect
- Extra hotkeys (paste last, undo, toggle cloud)

### Long-Form Transcription
- **Live Transcript Dump**: Export ongoing transcripts (TXT, MD, JSON)
- **Chapter Summarization**: Automatic segmentation for meetings, lectures
- **Topic Detection**: Identify and mark topic shifts

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
