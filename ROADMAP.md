# Roadmap - Trispr Flow

Last updated: 2026-02-19 (Roadmap sync after analysis split + launcher/installer hardening)

This file is the canonical source for priorities, dependencies, and "what is next."

---

## Canonical Current State

- **Released**: `v0.6.0` (2026-02-16) with post-release fixes.
- **Current phase**: `v0.7.0` implementation execution.
  - Foundation complete: Block F + Block G
  - Execution packet open: Block H (Tasks 32/33/34/35/38)
- **Voice Analysis architecture**:
  - Trispr Flow no longer auto-downloads analysis installers at runtime.
  - Analyse action launches an external tool via local executable detection.
  - When no local EXE is found, users select a local `trispr-analysis.exe`.
  - Dev builds support Python fallback (`analysis-tool/main.py`) for local testing.
- **Installer strategy**:
  - `CUDA` and `Vulkan` stay slim (no analysis payload).
  - `CUDA+Analysis` adds optional bundled local chain-install (`Trispr-Analysis-Setup.exe`).
  - No network download in NSIS analysis path.

## Next Work (4 Dependency Blocks)

| Block | Focus | Complexity | Depends on | Status |
| --- | --- | --- | --- | --- |
| **A** | Installer + setup automation hardening (NSIS page flow, setup script reliability, clear recovery paths) | **High** | - | Complete |
| **B** | VibeVoice runtime hardening (model loading, dependency pinning, cache/prefetch guardrails, HF warning UX) | **High** | A | Complete |
| **C** | Voice Analysis UX resilience (retry/reset after failure, actionable dialog states, no stale error replay) | **High** | A, B | Complete |
| **D** | v0.7.0 AI Fallback implementation (Tasks 31/36/37 then 32/33/34/35/38) | **High** | C | In progress (foundation done, integrations open) |

## Locked Decisions (2026-02-19)

1. Runtime analysis installer download path is removed.
2. Analyse flow uses local EXE selection and remembered override path.
3. A third installer variant is introduced: `CUDA+Analysis`.
4. `CUDA+Analysis` build has a hard gate on local file `installers/Trispr-Analysis-Setup.exe`.

## v0.7.0 Implementation Status

- ✅ **Block F complete**: terminology, UX location, execution sequence, architecture docs.
- ✅ **Block G complete**: Task 31 (provider architecture), Task 36 (settings migration), Task 37 (config UI scaffolding).
- 🔵 **Block H open**: Task 32/33/34 provider API integrations, Task 35 prompt strategy polish, Task 38 E2E.

### v0.7.0 Task Ledger (Done/Open)

| Task | Title | State |
| --- | --- | --- |
| 31 | Multi-provider architecture | ✅ Done |
| 36 | Settings migration + data model | ✅ Done |
| 37 | Provider config UI scaffolding | ✅ Done |
| 32 | OpenAI provider integration | 🔵 Open |
| 33 | Claude provider integration | 🔵 Open |
| 34 | Gemini provider integration | 🔵 Open |
| 35 | Prompt strategy polish | 🔵 Open |
| 38 | End-to-end tests | 🔵 Open |

## Analysis Launcher Status

- ✅ Runtime download URL path removed (`download.trispr.dev` no longer used by app launcher).
- ✅ `analysis_tool_status` now returns local candidate paths/directories for picker UX.
- ✅ Dev fallback path to Python CLI is available when EXE is absent.
- ✅ NSIS analysis install path is local-only in the CUDA+Analysis variant.

## Immediate Next Actions

1. Finish launcher/installer E2E validation for all three variants (including CUDA+Analysis chain-install).
2. Start Block H Task 32 (OpenAI provider integration) and proceed through Task 33/34/35.
3. Close Block H with Task 38 cross-provider E2E tests.

## Detailed References

- Detailed execution table: `docs/TASK_SCHEDULE.md`
- Block G implementation handover (archive): `docs/NEXT_BLOCK_G.md`
- v0.7 architecture: `docs/V0.7.0_ARCHITECTURE.md`
- Decision log: `docs/DECISIONS.md`
- Installer variants and chain-install details: `docs/INSTALLER_VARIANTS.md`
- Voice Analysis runtime QA matrix: `docs/VOICE_ANALYSIS_RUNTIME_QA_MATRIX.md`
- Voice Analysis UX QA matrix: `docs/VOICE_ANALYSIS_UX_QA_MATRIX.md`

---

## Historical Context (Pre-Sync Notes)

The following sections are preserved for implementation history and research context.
When content conflicts with the sections above, treat the top section (`Canonical Current State`, `Next Work`) as source of truth.
This historical section may reference the earlier in-app sidecar architecture and should be treated as archive.

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
- **Sidecar Runtime Packaging**: Supports bundled exe and Python fallback; installer currently ships Python sidecar setup path
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

## Planning Queue — Capture UI Overhaul (v0.7.0+)

**Priority: HIGH** — Identified 2026-02-17 from real usage

### Problem Statement

The Capture panels (Input + System Audio) are inconsistent in how they handle modes, toggles, and state. Two concrete UX failures:

#### 1. Redundant "Enable System Audio Transcription" Toggle

**Current**: A global toggle at the top of System Audio Capture does the same thing as the transcription hotkey. Both start/stop transcription. Users don't know which one is "in charge."

**Impact**: Confusing. Users toggle it on, then press the hotkey, now they're unsure of the state.

**Fix**: Remove the toggle. The hotkey is the canonical control. A clear status indicator (IDLE / MONITORING / TRANSCRIBING) replaces the toggle. A "Click to activate" hint teaches new users about the hotkey.

---

#### 2. Capture Mode Inconsistency (Input vs. System Audio)

**Current — Capture Input:**
- Dropdown: "Push to Talk" / "Toggle to Transcribe"
- Separate "Voice Activation" subsection with threshold slider

**Current — System Audio:**
- Separate checkboxes: "Use Voice Activation" + "Transcribe Hotkey"
- Completely different UX pattern for the same concepts

**Impact**: Users switching between tabs get confused. No clear mental model.

**Fix**: Align both panels to the same structure:
- **Mode selector** (Radio or Dropdown): "Hotkey / Toggle" vs "Always On (VAD-gated)"
- **VAD threshold** appears only when "Always On" is selected (progressive disclosure)
- Same visual language, same component patterns

---

#### 3. System Audio Recording Cut-Off Under 8 Seconds

**Current**: Only saves audio chunks ≥ 8s. If a speaker pauses frequently (typical meeting), most audio is silently discarded. Users press "Analyse" and get nothing.

**Impact**: Core VibeVoice feature is essentially broken for real meetings.

**Fix** (Backend + UX):
- **Continuous buffer**: Always accumulate system audio into a rolling buffer regardless of silence
- **VAD as gate only**: Use VAD threshold to decide what to *transcribe* (real-time display), but always keep raw audio
- **Silence stripping on flush**: When saving for analysis, strip silence (< threshold) before writing OPUS — so 10min of audio with 50% silence becomes a tight 5min OPUS file
- **Analysis-ready indicator**: Visual badge on the Analyse button showing "~4m recorded" when buffer exceeds minimum useful duration (e.g., 30s of non-silence)
- **Auto-flush on meaningful content**: Flush to file when accumulated non-silence exceeds 30s (not 60s of total time)

---

### Tasks

**UX-1: Unify Capture Mode Controls** (Sonnet)
- Redesign both capture panels to use consistent mode selector + VAD subsection
- Remove redundant System Audio global toggle
- Add status indicator (IDLE / MONITORING / TRANSCRIBING) to System Audio panel
- Keep settings backward compatible

**UX-2: Continuous System Audio Buffer** (Opus)
- Decouple transcription buffer (short chunks for Whisper) from recording buffer (continuous accumulation)
- Implement silence stripping using existing VAD threshold before OPUS flush
- Lower flush trigger: 30s of non-silence content (not 60s of total audio)
- Preserve existing mic recording behavior (unchanged)

**UX-3: Analysis-Ready Indicator** (Sonnet)
- Track accumulated non-silence duration in recording buffer
- Show badge on Analyse button: "~2m ready" / "~8m ready"
- Update in real time as audio accumulates
- Clear when recording is flushed to file

**UX-4: Settings Cleanup** (Haiku)
- Audit all Capture settings for dead/redundant toggles
- Consolidate System Audio settings: remove `enable_transcription` toggle, keep VAD threshold, keep hotkey config
- Update settings migration if fields are removed

---

### Success Criteria

- User can open System Audio panel and immediately understand how to start monitoring (one hotkey, clear status)
- Input and System Audio panels follow the same control pattern
- Meeting recordings (speakers with natural pauses) consistently produce analysable OPUS files
- Analyse button shows readiness without user having to guess

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
