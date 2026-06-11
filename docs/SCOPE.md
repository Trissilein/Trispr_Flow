# Project Scope Evolution (Archived)

> Archive note: this file moved out of repo root during docs housekeeping on 2026-02-20.
> Use `ROADMAP.md` for current priorities and `docs/DECISIONS.md` for active decision rationale.

**Last updated**: 2026-02-16

## Original Scope (2025-08)

Initial plan when Trispr Flow started:

> "Simple GPU-accelerated offline dictation for Windows. Record with hotkey, transcribe with Whisper, auto-paste result."

### Original MVP
- ✅ PTT (Push-to-Talk) recording with global hotkey
- ✅ GPU-accelerated transcription (whisper.cpp)
- ✅ Auto-paste to active application
- ✅ Model manager (download/remove models)

**Estimated scope**: ~2 months (Haiku/Sonnet sprints)

---

## What Actually Happened: Scope Expansion

The project grew significantly beyond MVP as use cases became clearer:

### Phase 1: Foundation (v0.1-v0.3)
**Original scope maintained**
- ✅ PTT capture working
- ✅ Whisper transcription working
- ✅ Model manager functional

### Phase 2: Real-World Requirements (v0.4-v0.5)
**New features discovered from user needs:**

#### System Audio Capture (Major Addition)
- Problem: "I need to transcribe Zoom calls, not just my dictation"
- Solution: Windows WASAPI loopback capture
- Impact: +60% development time vs original plan

#### Post-Processing Pipeline (Major Addition)
- Problem: "Whisper is great but punctuation/capitalization sucks"
- Solution: Rule-based local post-processing (punctuation, numbers, vocabulary)
- Impact: +40% development time, new Settings panel complexity

#### Long-Form Features (Major Addition)
- Problem: "Need to handle 2-hour recordings, not just quick dictations"
- Solution: Chapter segmentation, topic detection, export formats (TXT/MD/JSON)
- Impact: +50% development time, new Data Model design

#### Tab-Based UI Refactor (Major Addition)
- Problem: "Settings panel is overwhelming with 6 panels on-screen"
- Solution: Separate Transcription tab (active use) from Settings tab (configuration)
- Impact: +30% frontend development time

### Phase 3: Production Requirements (v0.6.0)
**Enterprise features discovered:**

#### Speaker Diarization (Major Addition)
- Problem: "I need to know WHO said WHAT in meeting recordings"
- Solution: Integrate Microsoft VibeVoice-ASR 7B (MIT license)
- Impact: +4 weeks development, new Python sidecar architecture, PyInstaller packaging

#### OPUS Audio Encoding (Major Addition)
- Problem: "Meeting recordings are 600MB WAV files, storage is too expensive"
- Solution: FFmpeg-based OPUS encoding (75% size reduction)
- Impact: +1 week development, audio processing complexity

#### Parallel Transcription (Major Addition)
- Problem: "I want real-time speed + speaker identification simultaneously"
- Solution: Run Whisper + VibeVoice in parallel (opt-in for 16GB+ VRAM)
- Impact: +1 week development, queue management complexity

---

## Scope Comparison: Original vs Reality

| Dimension | Original Plan | Actual (v0.6.0) | Growth |
| --- | --- | --- | --- |
| **Core ASR engines** | Whisper only | Whisper + VibeVoice | +1 |
| **Audio sources** | Microphone only | Mic + System audio | +1 |
| **Features** | Basic transcription | Diarization, chapters, topics, export | +8 |
| **UI panels** | 1 simple panel | 6 settings panels + tabbed layout | +5 |
| **Export formats** | Copy to clipboard | TXT/MD/JSON with metadata | +3 |
| **Quality controls** | Model selection | OPUS bitrate + precision presets | +2 |
| **Development time** | ~8 weeks estimated | ~20 weeks actual | 2.5x |
| **Code size** | ~5k LOC estimated | ~25k LOC actual | 5x |
| **Architecture complexity** | Simple Tauri app | Multi-component (sidecar, async, state) | Significant |

---

## Why Scope Grew

### 1. **Competitive Pressure**
Initial research of competitors (Handy.computer, Otter.ai) showed:
- System audio capture is table-stakes, not optional
- Speaker diarization is a major differentiator
- Export flexibility is expected

### 2. **User Feedback Loop**
Early testers revealed:
- Zoom/Teams/Discord meeting transcription is primary use case (not voice dictation)
- Punctuation and capitalization matter more than expected
- Storage/cost concerns are real for power users

### 3. **Technical Feasibility**
- VibeVoice-ASR became available with MIT license (timing was lucky)
- OPUS encoding via FFmpeg was straightforward to integrate
- Tauri's sidecar support made Python integration cleanable

### 4. **Product Vision Shift**
**Original**: "Fast dictation app for Windows"
**Actual**: "Professional meeting transcription with advanced features"

This is a **different product category** with higher complexity and different market.

---

## Current Architecture (v0.6.0)

The project now has:
- **2 transcription engines** (Whisper + VibeVoice)
- **2 audio sources** (microphone + system audio)
- **2 UI paradigms** (Dot overlay + KITT bar mode)
- **2 encoding formats** (WAV for Whisper, OPUS for storage)
- **Python sidecar** (FastAPI server for VibeVoice)
- **Async state machine** (recording → transcribing → idle states)
- **Settings persistence** (JSON with migration logic)
- **Export system** (3 formats with speaker attribution)
- **E2E testing** (22 tests for full workflow)

---

## v0.7.0 Scope: Further Expansion (Planned)

**Original scope** → **v0.6.0** → **v0.7.0**

### Multi-Provider AI Fallback (Planned)

**Why?**
- Users want choice: "I prefer OpenAI's cost", "I trust Anthropic", "Google's models are faster"
- Lock-in risk: Single provider (Claude) was limiting
- Market reality: Serious users evaluate multiple providers

**New Complexity:**
- Provider abstraction layer (trait pattern)
- API key management (system keyring integration)
- Settings migration (cloud_fallback → ai_fallback)
- Error handling per provider (rate limits, quotas, auth)
- Cost estimation across providers
- Custom prompt management

**Estimated**: +2 weeks development (Blocks G & H)

---

## Lessons: Balancing Scope vs Ship Date

### ✅ What Worked
1. **MVP-first approach**: Core transcription was solid before adding system audio
2. **Modular architecture**: Easy to add VibeVoice without breaking Whisper
3. **User feedback**: Real problems (punctuation, meeting transcription) drove priorities
4. **Documented decisions**: DECISIONS.md tracks why each feature was added

### ⚠️ What Was Challenging
1. **Scope creep**: Each new feature seemed "just one more thing"
2. **Storage concerns**: CUDA bundle became 560MB initially (fixed with DLL optimization)
3. **Python sidecar complexity**: Added debugging surface area
4. **Settings migration**: Schema changes require migration logic (will happen v0.6.0 → v0.7.0)

### 🎯 Current Policy
- **Core features**: Whisper transcription (always included)
- **Optional features**: VibeVoice, OPUS encoding (can be disabled)
- **Advanced features**: Parallel mode, custom prompts (opt-in)
- **New providers** (v0.7.0): All optional, only enabled if API key provided

---

## What This Means for Contributors

When working on v0.7.0 and beyond:

1. **Document your decisions** → Add to DECISIONS.md with "Why?" not just "What?"
2. **Consider impact** → Will this feature require settings migration? Increase build size?
3. **Keep optional features optional** → Don't force users into cloud providers or complex UX
4. **Test the scope** → Unit test + E2E test before saying "done"
5. **Write for 2x expected time** → Scope tends to grow; plan conservatively

---

## References

- **DECISIONS.md** — All architectural decisions with rationale
- **ROADMAP.md** — Current priorities and next milestones
- **V0.7.0_ARCHITECTURE.md** — Detailed design for v0.7.0
- **TASK_SCHEDULE.md** — Implementation blocks and timelines
