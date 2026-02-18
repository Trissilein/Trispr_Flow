# Decisions

Last updated: 2026-02-18

## Implemented Decisions

### DEC-001 Platform and app shell

- Status: `implemented`
- Decision: Use Tauri v2 desktop shell with tray and global hotkeys.
- Why: Strong Rust integration and low runtime overhead for desktop capture tooling.

### DEC-002 Primary transcription backend

- Status: `implemented`
- Decision: Use whisper.cpp as primary ASR backend (GPU-first, CPU fallback).
- Why: Privacy-first local transcription and predictable offline behavior.

### DEC-003 Persistence strategy

- Status: `implemented`
- Decision: Persist settings and histories as JSON in app config/data directories.
- Why: Simple migration path and transparent local state management.

### DEC-004 Capture architecture

- Status: `implemented`
- Decision: Keep input capture and output transcription as separate pipelines, merge views in Conversation tab.
- Why: Clear operational boundaries and easier debugging of source-specific issues.

### DEC-005 Transcribe enablement semantics

- Status: `implemented`
- Decision: Output transcription starts disabled each session and is enabled explicitly by user action/hotkey.
- Why: Safer default and clearer user intent.

### DEC-006 Voice Activation threshold UX

- Status: `implemented`
- Decision: Expose Voice Activation thresholds in dB (-60..0), while storing linear values internally.
- Why: User control maps to perceived loudness; internal DSP remains unchanged.

### DEC-007 Verification baseline

- Status: `implemented`
- Decision: Standard local verification is `npm run test` plus `npm run test:smoke`.
- Why: Fast unit feedback plus integrated frontend+Rust sanity check.

### DEC-014 Export format strategy (v0.5.0)

- Status: `implemented`
- Decision: Support TXT, Markdown, and JSON export formats with stateless format selection (no settings persistence for export format).
- Why: Export format is contextual — users choose based on immediate need (TXT for sharing, MD for documentation, JSON for data). No single "default" format serves all use cases. Stateless selection avoids unnecessary settings complexity.

### DEC-015 CUDA installer optimization (v0.4.1)

- Status: `implemented`
- Decision: Remove cublasLt64_13.dll from CUDA bundle, keep only cublas64_13.dll + cudart64_13.dll.
- Why: cublasLt64_13.dll (459 MB) is not used by whisper.cpp. Removing it reduced CUDA installer from ~560 MB to ~93 MB (81% reduction) with zero functional impact. Verified by testing whisper-cli.exe GPU detection (RTX 5070 Ti) without the DLL.

## Accepted Decisions (Planned, Not Fully Implemented)

### DEC-008 Milestone 3 ordering

- Status: `accepted`
- Decision: Implement Capture Enhancements before Post-Processing pipeline.
- Why: Post-processing should target stabilized capture behavior and segmentation patterns.

### DEC-009 AI fallback direction

- Status: `accepted`
- Decision: Replace single-provider cloud fallback with provider-agnostic AI fallback (Claude/OpenAI/Gemini).
- Why: Flexibility, model choice, and future-proof integration.

### DEC-010 Activity feedback expansion

- Status: `accepted`
- Decision: Add tray pulse (recording/transcribing) and backlog capacity management (80% warning + expansion path).
- Why: Better runtime observability and reduced risk of dropped audio during long sessions.

### DEC-016 Tab-Based UI Refactor (v0.5.0)

- Status: `accepted`
- Decision: Restructure the single-page layout into two top-level tabs: **Transcription** (transcript history, export, chapters) and **Settings** (all configuration panels).
- Context: User feedback identified the current layout as visually overloaded. All 6 panels are shown simultaneously, but settings panels (Capture Input, System Audio Capture, Model Manager, Post-Processing, UX/UI) are "fire and forget" — configured once, then rarely touched. The transcript output is the primary interaction surface during active sessions.
- Design: Hero section stays visible on both tabs. Transcription tab shows full-width transcript area. Settings tab uses 2-column grid for configuration panels. Tab state persists via localStorage.
- Why: Separating daily-use views (transcription) from configuration views (settings) reduces cognitive load by ~70% and focuses user attention on the primary task.

### DEC-017 "Output" to "System Audio" naming (v0.5.0)

- Status: `accepted`
- Decision: Rename the "Output" transcript tab to **"System Audio"** and the "Capture Output" settings panel to **"System Audio Capture"**.
- Context: "Output" is overloaded in the interface — used for the transcript tab, the settings panel, the device selector, and the hero card. This creates ambiguity. User preference for technically precise naming over consumer-friendly simplification, given Trispr Flow's power-user audience.
- Why: "System Audio" is technically precise (WASAPI loopback capture of system audio output), removes ambiguity, and aligns with the user's expectations of a technical tool.

### DEC-018 Chapters: conversation-only by default (v0.5.0)

- Status: `accepted`
- Decision: Chapter markers are shown **only in the Conversation tab** by default, with an optional setting to enable in System Audio tab. Input-only tab does not show chapters.
- Context: Chapters segment long transcripts into navigable sections. In Input-only mode (short PTT dictations), chapters add no value and waste space. In Conversation mode (meetings, lectures), chapters provide meaningful structure. System Audio mode may benefit for long monitoring sessions, so it's optionally available.
- Settings: `chapters_enabled` (master toggle), `chapters_show_in` ("conversation" or "all"), `chapters_method` ("silence" or "time" or "hybrid").
- Why: Reduces UI noise for common use cases while keeping flexibility for power users.

### DEC-019 Window visibility state persistence (v0.5.0+)

- Status: `implemented`
- Decision: Persist window visibility state (normal/minimized/tray) across sessions. If user closes app while minimized, next launch starts minimized. If closed while in system tray, next launch stays in tray.
- Context: Users expect window state to persist across restarts. Previous implementation only saved geometry (position, size, monitor) but not visibility state. This led to minimized/tray windows always restoring to normal visible state.
- Implementation:
  - New setting: `main_window_start_state` ("normal" | "minimized" | "tray")
  - Frontend tracks minimize via `window.onFocusChanged` + `window.isMinimized()`
  - Backend tracks tray state in `hide_main_window()` / `show_main_window()`
  - Startup code in `.setup()` handler checks setting and conditionally shows/hides/minimizes window
- Why: Improves UX consistency, especially for users who run Trispr Flow in background (tray-only mode).

### DEC-020 VibeVoice-ASR Sidecar Architecture (v0.6.0)

- Status: `implemented`
- Decision: Run VibeVoice-ASR as a Python FastAPI sidecar process, communicated via HTTP, rather than embedding Python in Rust.
- Context: VibeVoice-ASR 7B requires Python + Transformers + PyTorch. Embedding via PyO3 would create complex build dependencies and potential ABI issues. A sidecar process keeps the Rust binary clean and allows independent Python updates.
- Implementation: `sidecar_process.rs` manages lifecycle (start/stop/health), `sidecar.rs` provides HTTP client. Runtime auto-detects bundled sidecar exe vs Python fallback.
- Why: Clean separation of concerns. Python ecosystem evolves independently. Supports both distribution modes without Rust build complexity from Python bindings.

### DEC-021 OPUS as Default Recording Format (v0.6.0)

- Status: `implemented`
- Decision: Use OPUS (via FFmpeg) as the default format for saved recordings instead of WAV.
- Context: WAV files for meeting-length recordings are enormous (1 hour = ~600 MB). OPUS at 64 kbps reduces this by ~75% with negligible quality loss for speech.
- Settings: Configurable bitrate (32/64/96/128 kbps), OPUS encoding toggle, VBR enabled by default.
- Why: Practical storage requirements for users who auto-save recordings. FFmpeg is already required for audio processing.

### DEC-022 Parallel Mode as Opt-In (v0.6.0)

- Status: `implemented`
- Decision: Parallel mode (Whisper + VibeVoice simultaneously) is disabled by default, requiring explicit user opt-in.
- Context: Running both models simultaneously requires significant VRAM (up to 20 GB with FP16). Most users have 8-16 GB VRAM. Sequential mode is safer and sufficient for most workflows.
- Why: Prevents OOM crashes on hardware with limited VRAM. Power users with 16+ GB can opt in.

### DEC-023 AI Fallback Terminology (v0.7.0 - Block F)

- Status: `accepted`
- Decision: Use "AI Fallback" as terminology for the multi-provider AI refinement system (not "AI Enhancement" or "Cloud Fallback")
- Context: Replaces single-provider "Cloud Fallback" with support for Claude, OpenAI, Gemini. Terminology should be neutral and accurately describe the behavior.
- Why: "Fallback" accurately describes behavior (optional refinement layer), is neutral to provider, aligns with "Post-Processing" terminology, and is a proven pattern in transcription tools.

### DEC-024 AI Fallback Settings Location (v0.7.0 - Block F)

- Status: `accepted`
- Decision: Place AI Fallback configuration in an expander within the Post-Processing panel (not a separate tab).
- Context: Post-Processing (local rules) runs first, AI Fallback runs second. Logical grouping reduces settings clutter.
- Why: Clear data flow (rules → refinement), reduces tab complexity, improves discoverability vs. buried in new tab, single panel for text enhancement workflow.

### DEC-025 AI Fallback Execution Sequence (v0.7.0 - Block F)

- Status: `accepted`
- Decision: Pipeline execution order: Raw Transcript → Local Post-Processing → AI Fallback (optional) → Final Output
- Context: Local rules are fast/offline, AI refinement uses polished base text, both can be toggled independently.
- Why: Respects local-first philosophy (offline fallback works), AI sees better quality input (higher output quality), clear mental model for users.

### DEC-026 Voice Analysis Terminology (v0.6.0 post-release)

- Status: `implemented`
- Decision: Rename "Speaker Diarization" to "Voice Analysis" in all user-facing UI, installer text, and export headers.
- Context: "Speaker diarization" is a technical NLP term that non-technical users don't recognise. Early testers asked "what is speaker diarization?" — the feature name was blocking adoption.
- Why: "Voice Analysis" immediately communicates the value ("identifies who said what") without prior knowledge. Technical name (VibeVoice-ASR, diarization) is kept in developer docs and internal code. Rule: feature names should describe the *outcome*, not the *algorithm*.

### DEC-027 Voice Analysis — Dedicated Dialog vs Inline Results (v0.6.0 post-release)

- Status: `implemented`
- Decision: Open a dedicated full-screen modal dialog for Voice Analysis instead of injecting results inline into the history list.
- Context: Previous approach showed results directly in the transcript history, with no feedback during the (potentially 30s) engine startup phase. Users were left with a spinning button and no indication of what was happening.
- Why: Analysis is a multi-step async operation (file pick → engine start → speaker identification). A dedicated dialog allows showing step-by-step progress (pending / active / done / error per stage), surfacing errors with actionable messages, and presenting results without cluttering the transcript history. Inline toast notifications are insufficient for operations with multiple stages and multi-minute runtimes.

### DEC-028 Voice Analysis distribution strategy (2026-02-18)

- Status: `accepted`
- Decision: Keep base installer slim and deliver Voice Analysis as optional setup/download path (no mandatory large bundle in base installer).
- Context: Voice Analysis runtime and model dependencies can be very large; bundling everything directly would significantly inflate installer footprint.
- Why: Better default download size and faster base install for users who do not need Voice Analysis immediately.

### DEC-029 Voice Analysis prefetch policy (2026-02-18)

- Status: `accepted`
- Decision: Keep model prefetch `default OFF`; trigger guided setup on first Analyse click with size/storage hint, disk check, and progress feedback.
- Context: Automatic prefetch can surprise users with very large downloads and disk growth.
- Why: Balances first-run UX with resource control and transparency.

### DEC-030 Local VibeVoice source discovery policy (2026-02-18)

- Status: `accepted`
- Decision: Disable local `VibeVoice` source auto-discovery in release builds; allow in dev builds (or explicit override only).
- Context: Auto-discovery is useful for development but can cause non-deterministic runtime behavior in production.
- Why: Improves reproducibility and supportability in shipped builds while retaining developer flexibility.

### DEC-031 Voice Analysis installer failure policy (2026-02-18)

- Status: `accepted`
- Decision: Use soft-fail policy for Voice Analysis setup. App installation must complete; user receives actionable remediation and in-app setup retry path.
- Context: Voice Analysis is optional; hard-failing full app install because of optional dependency setup blocks core use cases.
- Why: Maximizes successful installs while still guiding users to feature readiness.

## Open Decisions

### DEC-011 Optional backend scope

- Status: `open`
- Question: Add faster-whisper as optional backend or keep whisper.cpp-only.

### DEC-012 Post-processing scope depth

- Status: `open`
- Question: Final scope of normalization and domain-specific correction rules before AI enhancement.

### DEC-013 CUDA toolchain policy

- Status: `open`
- Question: Enforce preferred VS/CUDA toolchain combo vs supporting broader override-based setups.
