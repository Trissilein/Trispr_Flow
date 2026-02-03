# Roadmap - Trispr Flow

Last updated: 2026-02-03

This roadmap tracks the current focus: getting core capture + transcription stable and tightening UX before expanding features.

---

## Current Status

✅ **Milestone 0**: Complete (tech stack locked, whisper.cpp validated)
✅ **Milestone 1**: Complete (PTT capture, transcription, paste)
🔄 **Milestone 2**: In Progress (Foundation & Critical UX)

**Recent progress**
- ✅ System audio capture via WASAPI (Windows) + transcribe hotkey
- ✅ Output tabs: Microphone / System Audio / Conversation
- ✅ Conversation view combining mic + system transcripts
- ✅ Output meters with dB readouts + threshold markers
- ✅ Input gain for system audio
- ✅ Panel collapse state + compact layout
- ✅ Audio cue volume control

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

### 2.3 Overlay Redesign (Minimal Dot) 🔄
- Visible dot only (no invisible window artifacts)
- Audio-reactive size (min/max radius)
- Color + active/inactive opacity
- Rise/fall smoothing
- Position controls (X/Y)

### 2.4 Conversation View 🔄
- Combined mic/system transcript stream
- Detachable conversation window (stable content + close)
- Font size control

### 2.5 Model Manager Revamp 🔄
- Source selector (default + custom URL)
- Show **available** vs **installed** models
- Install / remove actions
- Per-model storage path display

**Definition of Done**
- Overlay behaves as specified (size/opacity/color tied to input level)
- System audio meter/gain calibrated and VAD threshold accurate
- Conversation detach window fully functional
- Model manager supports install/remove + custom sources

---

## Milestone 3 — Quality of Life (Planned)
- Activation words (“over” / “stop”) for continuous capture
- Text post‑processing (punctuation, numbers, custom vocab)
- Language pinning beyond auto‑detect
- Extra hotkeys (paste last, undo, toggle cloud)

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

