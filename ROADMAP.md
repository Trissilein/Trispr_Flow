# Roadmap - Trispr Flow

Last updated: 2026-02-16

This roadmap tracks the current focus: getting core capture + transcription stable and tightening UX before expanding features.

---

## Current Status

### Completed Versions

- **Milestone 0**: ✅ Complete (tech stack locked, whisper.cpp validated)
- **Milestone 1**: ✅ Complete (PTT capture, transcription, paste)
- **Milestone 2**: ✅ Complete (Foundation & Critical UX)
- **Milestone 3**: ✅ Complete (Window persistence, Activity feedback, Model hot swap, Capture enhancements, Post-processing pipeline)
- **v0.4.0**: ✅ Complete (Post-processing pipeline, custom vocabulary, rule-based enhancements)
- **v0.4.1**: ✅ Complete (Security hardening, CUDA distribution, Hotkey UX)
- **v0.5.0 Block A**: ✅ Complete (Export features, schema design, documentation)
- **v0.5.0 Block B**: ✅ Complete (Tab-Based UI Refactor, Chapter UI, Topic detection, live dump, naming cleanup)
- **v0.6.0 Block C**: ✅ Complete (VibeVoice research, sidecar structure, OPUS design)
- **v0.6.0 Block D**: ✅ Complete (Integration layer, model loading, sidecar process management, auto-processing)
- **v0.6.0 Block E**: ✅ Complete (Speaker UI, Analyse button, quality presets, parallel mode, PyInstaller, E2E tests)

### Latest Release: v0.6.0 (2026-02-16)

- ✅ VibeVoice-ASR 7B speaker diarization with color-coded segments
- ✅ Parallel transcription mode (Whisper + VibeVoice simultaneously, opt-in for 16GB+ VRAM)
- ✅ OPUS audio encoding with configurable bitrate (32/64/96/128 kbps)
- ✅ Quality presets (VibeVoice precision: FP16/INT8)
- ✅ Analyse button with file picker and auto-save recordings
- ✅ PyInstaller packaging (standalone sidecar exe, no Python dependencies for users)
- ✅ System audio auto-save with 60-second flush intervals
- ✅ Model monitoring with weekly VibeVoice update checks
- ✅ 22 E2E tests covering full diarization workflow
- ✅ Professional app icon (Cyan/Gold Yin-Yang branding)
- 📦 Dual installers: CUDA Edition (92MB) + Vulkan Edition (9.4MB)

### Next: v0.7.0 (AI Fallback Overhaul) — Planning Complete ✅

**Block F (Haiku) — COMPLETE**:
1. ✅ **Task 39**: UX decisions finalized
   - Terminology: "AI Fallback" (neutral, describes optional refinement)
   - Location: Post-Processing panel expander
   - Sequence: Raw → Local Rules → AI Fallback → Final
2. ✅ **Task 39a**: Architecture documented (V0.7.0_ARCHITECTURE.md — 400+ lines)
   - Provider trait pattern + factory
   - Tauri command design
   - Error handling strategy
3. ✅ **Task 39b**: Design decisions documented (DEC-023, DEC-024, DEC-025 in DECISIONS.md)
4. ✅ **Task 39c**: Implementation plan for Blocks G & H

**Block G (Opus) — Ready to Start**:
1. **Task 31**: Multi-provider architecture (provider trait, factory, models)
2. **Task 36**: Settings migration (v0.6.0 cloud_fallback → v0.7.0 ai_fallback)
3. **Task 37**: Configuration UI (API key setup, provider selector, model dropdown)

**Block H (Sonnet) — Queued**:
1. **Task 32**: OpenAI client (ChatGPT/GPT-4o integration)
2. **Task 33**: Claude client (Anthropic integration)
3. **Task 34**: Gemini client (Google integration)
4. **Task 35**: Custom prompt strategy (user-editable with defaults)
5. **Task 38**: E2E tests (all providers, error scenarios)

---

## Competitor Analysis Integration (Handy.computer)

**Key Findings from COMPETITOR_ANALYSIS_HANDY.md:**

### ✅ Areas Where Trispr Flow Leads

- System audio capture (WASAPI)
- Speaker diarization (v0.6.0)
- Post-processing pipeline
- Export formats (TXT/MD/JSON)
- Chapter detection + topic detection
- Full-text search
- Overlay feedback (dot + KITT)

### 🔍 Opportunities from Competitor Analysis

#### 1. Parakeet ASR Engine (v0.6.0+) — **HIGH PRIORITY**

**Why:** NVIDIA Parakeet models are significantly faster than Whisper on NVIDIA hardware

**Integration:**
- Add as alternative ASR engine alongside Whisper + VibeVoice
- ONNX Runtime or TensorRT backend (separate from whisper.cpp)
- Engine selection in model settings: "Whisper" / "Parakeet" / "VibeVoice" / "Auto"
- Benchmark Parakeet vs Whisper on RTX 5070 Ti

**Roadmap Impact:** Add to v0.6.0 Block E as E31-E34 tasks

**Hardware Fit:** RTX 5070 Ti (16GB VRAM) is ideal for Parakeet models

#### 2. Quick Start Mode (v0.5.0 Complete) — **MEDIUM PRIORITY**

**Status:** Partially addressed by tab-based UI refactor in Block B

**Remaining Work:**
- First-run wizard: minimal view with big record button
- Auto-detect best model + GPU configuration
- One-click setup flow
- Settings hidden behind "Advanced" tab

**Roadmap Impact:** Add to v0.7.0 as UX polish tasks

#### 3. Cross-Platform Support (v1.0+) — **LOW PRIORITY**

**Current Blocker:** WASAPI capture is Windows-only

**Long-term Plan:**
- macOS: CoreAudio capture abstraction
- Linux: PulseAudio/PipeWire abstraction
- Tauri UI layer is already cross-platform

**Roadmap Impact:** Defer until v1.0+ when core features stabilize

#### 4. Plugin Architecture (v0.7+) — **LOW PRIORITY**

**Possible Hooks:**
- Post-processing plugins (custom text transforms)
- Export format plugins
- ASR engine plugins
- Analysis plugins (custom meeting analysis)

**Assessment:** Over-engineering risk. Defer until v0.7+ when core features stabilize.

---

## Implementation Schedule

Trispr Flow uses a **batched task schedule strategy** that groups related tasks by AI model (Haiku, Sonnet, Opus) to reduce context-switching overhead.

For detailed task breakdowns, timelines, dependencies, and complexity levels, see:
**[docs/TASK_SCHEDULE.md](docs/TASK_SCHEDULE.md)**

**Quick Summary**:

- **v0.5.0 Block A** (Haiku): 7 tasks COMPLETE — Export features, documentation, testing
- **v0.5.0 Block B** (Sonnet): 8 tasks COMPLETE — Tab UI refactor, chapter UI, topic UI, live dump, E2E tests
- **v0.6.0 Block C** (Haiku): 5 tasks COMPLETE — VibeVoice research, sidecar structure, OPUS design
- **v0.6.0 Block D** (Opus): 5 tasks COMPLETE — Integration layer, model loading, sidecar mgmt, auto-processing
- **v0.6.0 Block E** (Sonnet+Opus): 7 tasks COMPLETE — Speaker UI, Analyse button, quality presets, parallel mode, monitoring, packaging, E2E tests
- **v0.6.0+ Parakeet Integration** (Sonnet): 4 tasks — Research, engine abstraction, integration, benchmarking
- **v0.7.0**: AI Fallback Overhaul (1 Haiku + 3 Opus + 5 Sonnet tasks = 1 switch)
- **v0.7.0+**: Quick Start UX (3 Sonnet tasks)

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

### 2.3 Overlay Redesign (Minimal Dot)

- Audio-reactive size (min/max radius)
- Color + active/inactive opacity
- Rise/fall smoothing, Position controls (X/Y)
- **KITT bar mode** (alternative overlay style)

### 2.4 Conversation View

- Combined mic/system transcript stream
- Font size control

### 2.5 Model Manager Revamp

- Source selector (default + custom URL)
- Show available vs installed models
- Install / remove actions
- Per-model storage path display

### 2.6 Code Quality & Maintainability

- Frontend modularization (14 specialized modules)
- TypeScript type safety improvements

---

## Phase 2 — Security Hardening & Code Quality (Complete)

- **SSRF Prevention**: URL safety checks (no whitelist) for model downloads
- **Model Integrity**: SHA256 checksum verification
- **Download Limits**: Size caps and timeout protection
- **lib.rs Modularization**: Split 3700+ line file into focused modules
- **Automated Testing**: Unit + smoke baseline verified locally

---

## Milestone 3 — Quality of Life & Advanced Features (Complete)

### Window Behavior

- Persist main window position + size across sessions
- Restore on correct monitor
- **Persist minimized/tray state** (v0.5.0+) — NEW

### Activity Feedback

- In-app indicators: Separate recording/transcribing indicators + overlay marker
- Tray pulse: turquoise = Recording, yellow = Transcribing

### Model Manager QoL

- Apply model immediately without restart (hot swap with rollback on failure)

### Capture Enhancements

- Activation words with word boundary matching (case-insensitive)
- Language pinning (16 languages)
- Hallucination filter UI toggle

### Text Enhancement (v0.4.0)

- Post-Processing Pipeline:
  - Rule-based punctuation & capitalization (English, German)
  - Number normalization (0-100 + common tens)
  - Custom vocabulary with word boundary matching
  - Settings-driven with backward compatibility
  - Complete UI panel with master toggle, language selector, rule toggles

### Export Features (v0.5.0 Block A)

- Export formats: TXT, Markdown, JSON
- Export schema with format versioning (v1.0)
- Tauri command integration with native file dialog
- 28 unit tests covering all formats and edge cases
- Comprehensive documentation (EXPORT_GUIDE.md + EXPORT_SCHEMA.md)

### Speaker-Aware Meeting Transcription (v0.6.0 - Complete)

- **Model**: Microsoft VibeVoice-ASR 7B (MIT license, open-source)
- **Architecture**: Python FastAPI sidecar in `sidecar/vibevoice-asr/`
- **Audio Format**: OPUS (FFmpeg-based, 75% size reduction vs WAV)
- **Precision Options**: Configurable FP16 (~14-16 GB VRAM) or INT8 (~7-8 GB VRAM)
- **Speaker Diarization UI**: Color-coded segments, editable labels, speaker-attributed export
- **Analyse Button**: File picker, auto-save recordings, progress indicator
- **Quality Controls**: OPUS bitrate (32/64/96/128 kbps), VibeVoice precision (FP16/INT8)
- **Parallel Mode**: Whisper + VibeVoice simultaneous with auto-save (60s flush)
- **Model Monitoring**: Weekly update checks with toast notifications
- **PyInstaller Packaging**: Standalone sidecar exe (no Python required)
- **E2E Tests**: 22 tests covering full workflow

### Long-Form Transcription (v0.5.0)

- **Live Transcript Dump**: Export ongoing transcripts (TXT, MD, JSON) — COMPLETE
- **Chapter Segmentation**: Automatic segmentation — COMPLETE
- **Topic Detection**: Identify and mark topic shifts — COMPLETE

---

## Planning Queue — Parakeet ASR Engine Integration (v0.6.0+)

**Goal**: Add NVIDIA Parakeet as alternative ASR engine for faster inference on NVIDIA hardware

**Why Parakeet?**
- Significantly faster than Whisper on NVIDIA GPUs
- NVIDIA actively maintains and updates Parakeet
- Could run Parakeet for real-time transcription + Whisper for batch correction
- RTX 5070 Ti (16GB VRAM) is ideal hardware for Parakeet

**Technical Approach:**
- Parakeet uses ONNX Runtime or TensorRT (not whisper.cpp)
- Needs separate inference backend alongside existing whisper.cpp
- Offered as alternative engine in model settings
- Abstract ASR backend interface for multiple engines

**Tasks (v0.6.0 Block E+):**

1. **E31: Research Parakeet Integration** (Sonnet)
   - Investigate ONNX Runtime vs TensorRT
   - Evaluate Parakeet-RNNT vs Parakeet-CTC
   - Benchmark memory requirements on RTX 5070 Ti
   - Document integration approach

2. **E32: ASR Backend Abstraction** (Sonnet)
   - Create trait/interface for ASR engines
   - Refactor existing Whisper backend to use interface
   - Design engine selection mechanism
   - Update settings schema

3. **E33: Parakeet Backend Implementation** (Sonnet)
   - Implement ONNX Runtime backend
   - Add Parakeet model loading
   - Integrate with audio pipeline
   - Add engine selection UI

4. **E34: Parakeet Benchmarking** (Sonnet)
   - Compare Parakeet vs Whisper speed/quality
   - Test on RTX 5070 Ti
   - Document performance metrics
   - Create user guidance (when to use which engine)

---

## Planning Queue — AI Fallback Overhaul (v0.7.0)

Goal: replace "Claude fallback" with **AI Fallback** that supports multiple providers and user-selectable models.

**Providers**: Claude, OpenAI (ChatGPT), Gemini

**Planning steps**:

1. Requirements & UX — Rename UI to AI Fallback, config location, toggle behavior
2. Provider Config Design — Per-provider model selection, API key flow, limits
3. Data Model & Settings — Schema for provider/model/key storage, migration from `cloud_fallback`
4. Prompt Strategy — Default post-process prompt, user-editable with reset

---

## Planning Queue — Quick Start Mode (v0.7.0+)

**Goal**: Simplify first-run experience with minimal UI and auto-configuration

**Current Problem:**
- New users see dense settings-heavy UI on first launch
- Multiple panels, toggles, and options before first transcription
- Tab-based refactor (v0.5.0 Block B) already improves this

**Proposed Improvements:**

1. **First-Run Wizard**
   - Minimal view: big record button + shortcut hint
   - Settings hidden behind "Advanced" or Settings tab
   - One-click setup: "Download recommended model and start"

2. **Auto-Detection**
   - Auto-detect best model for user's GPU
   - Auto-detect CUDA vs Vulkan capability
   - Auto-select optimal quality preset

3. **Progressive Disclosure**
   - Start with minimal UI
   - Reveal advanced features as user explores
   - Onboarding tips for key features

**Tasks (v0.7.0+):**

- **Q1**: Design first-run wizard flow
- **Q2**: Implement GPU auto-detection
- **Q3**: Create minimal "Quick Start" view

---

## Milestone 4 — Production Ready (Planned)

- macOS testing + fixes
- Professional installers + updater
- Autostart
- Documentation polish

---

## Technical Debt / Risks

- Improve resampling quality (libsamplerate)
- Add tests for audio + transcription pipeline
- `postprocessing` panel missing from CSS grid-area assignments and `initPanelState()` list
- `wireEvents()` at 810 lines could be split per-panel
- `renderSettings()` at 160 lines could be split per-panel

---

## Version Release Criteria

Each version is considered **release-ready** when:

- All tasks in final block completed
- End-to-end integration test passes
- Code reviewed and merged to main
- Changelog updated
- Release notes drafted

For detailed technical roadmap, see [.claude/ROADMAP.md](.claude/ROADMAP.md)
