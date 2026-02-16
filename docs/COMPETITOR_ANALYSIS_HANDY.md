# Competitor Analysis: Handy (handy.computer)

Last updated: 2026-02-15

## Overview

**Handy** is an open-source, offline speech-to-text desktop app built in Rust. It positions itself as "the most forkable speech-to-text app" rather than the most feature-rich. Relevant comparison point for Trispr Flow as both share the same core tech stack and philosophy.

---

## Feature Comparison

| Feature | Handy | Trispr Flow | Gap |
|---------|-------|-------------|-----|
| Offline Whisper transcription | Yes | Yes | Parity |
| **Parakeet model support** | **Yes** | No | **New opportunity** |
| Push-to-talk | Yes | Yes | Parity |
| Toggle mode | Unknown | Yes | We lead |
| VAD / silence detection | Yes | Yes (advanced) | We lead |
| GPU acceleration | Yes | Yes (CUDA + Vulkan) | We lead |
| System audio capture | No | Yes (WASAPI) | We lead |
| Speaker diarization | No | Planned (v0.6.0) | We lead |
| Post-processing pipeline | No | Yes | We lead |
| Export (TXT/MD/JSON) | No | Yes | We lead |
| Chapter detection | No | Yes | We lead |
| Topic detection | No | Yes | We lead |
| Full-text search | No | Yes | We lead |
| Overlay feedback | No | Yes (dot + KITT) | We lead |
| Cross-platform | Win/Mac/Linux | Windows only | Handy leads |
| Plugin/extension system | Forkable design | No | Handy leads |
| Minimal first-run UX | Yes (1 shortcut) | Complex settings | Handy leads |

---

## Key Findings

### 1. Parakeet ASR Model Support (High Priority)

Handy supports **NVIDIA Parakeet** models alongside Whisper. Parakeet is NVIDIAs own ASR model family optimized for their GPUs.

**Why this matters for Trispr Flow:**
- Parakeet-RNNT and Parakeet-CTC are significantly faster than Whisper on NVIDIA hardware
- Our target hardware (RTX 5070 Ti, 16GB VRAM) is ideal for Parakeet
- NVIDIA actively maintains and updates Parakeet (better long-term support than community Whisper)
- Could run Parakeet for real-time transcription + Whisper for batch correction

**Integration approach:**
- Parakeet uses ONNX Runtime or TensorRT (not whisper.cpp)
- Needs separate inference backend alongside existing whisper.cpp
- Could be offered as alternative engine in model settings
- v0.6.0 already plans model architecture changes (VibeVoice-ASR) - natural fit

**Roadmap impact:** Add to v0.6.0 as optional ASR backend

---

### 2. Simplified First-Run Experience (Medium Priority)

Handys approach: install, press one shortcut, speak, done. No configuration required.

**Current Trispr Flow problem:**
- New users see a dense settings-heavy UI on first launch
- Multiple panels, toggles, and options before first transcription
- Tab-based refactor (v0.5.0 Block B) already improves this

**Proposed improvement: "Quick Start" mode**
- First launch shows a minimal view: big record button + shortcut hint
- Settings hidden behind "Advanced" or the Settings tab
- Auto-detect best model + GPU configuration
- One-click setup: "Download recommended model and start"

**Roadmap impact:** Fits into v0.5.0 Block B (Tab-Based UI Refactor)

---

### 3. Cross-Platform Potential (Low Priority, Long-term)

Handy runs on Windows, macOS, and Linux. Trispr Flow is Windows-only due to WASAPI dependency.

**Current blockers:**
- WASAPI capture is Windows-specific
- macOS would need CoreAudio, Linux would need PulseAudio/PipeWire
- Tauri itself is cross-platform, so UI layer is portable

**Assessment:** Not a v0.5/v0.6 priority. Revisit when core feature set stabilizes. The capture abstraction layer could be designed now for future portability.

---

### 4. Extensibility / Plugin Architecture (Low Priority)

Handys "forkable" philosophy means users modify source code directly. An alternative for Trispr Flow would be a proper plugin system.

**Possible plugin hooks:**
- Post-processing plugins (custom text transforms)
- Export format plugins (custom output formats)
- ASR engine plugins (swap Whisper for Parakeet or others)
- Analysis plugins (custom meeting analysis)

**Assessment:** Over-engineering risk. Defer until v0.7+ when core features stabilize.

---

## Recommended Roadmap Adjustments

### v0.5.0 (Current)
- [ ] Add "Quick Start" first-run experience to Tab-Based UI Refactor
- [ ] Auto-detect GPU capabilities on first launch
- [ ] Simplify default view for new users

### v0.6.0 (Next)
- [ ] **Add Parakeet ASR as alternative engine** (alongside Whisper + VibeVoice)
- [ ] Abstract ASR backend interface for multiple engines
- [ ] Benchmark Parakeet vs Whisper on RTX 5070 Ti
- [ ] Add engine selection in model settings ("Whisper" / "Parakeet" / "Auto")

### v0.7.0+ (Future)
- [ ] Evaluate cross-platform capture abstraction
- [ ] Consider plugin architecture for post-processing and export
- [ ] macOS support investigation

---

## Sources

- [Handy - About](https://handy.computer/about)
- [NVIDIA Parakeet Models](https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/models.html)
