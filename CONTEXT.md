# CONTEXT.md — Trispr Flow Domain Glossary

This file is the canonical source for domain language in Trispr Flow.
It is written for domain experts (users, designers, contributors), not for implementation details.
Update this file inline as terms are resolved during design sessions.

Last updated: 2026-05-14

---

## Terms

### PTT (Push-to-Talk)
A recording mode where the user holds a global hotkey to capture microphone audio; releasing the hotkey finalizes the audio chunk and triggers transcription. One of the two mic capture modes (the other is VAD).
→ `src-tauri/src/audio.rs`, `src-tauri/src/hotkeys.rs`

### VAD (Voice Activity Detection)
A recording mode where the app monitors the microphone continuously and automatically starts/stops recording based on a configurable silence threshold. No hotkey required.
→ `src-tauri/src/audio.rs`, `src-tauri/src/continuous_dump.rs`

### Continuous Dump Pipeline
The adaptive segmenter that slices long audio streams (VAD or system-audio) into transcribable chunks using silence-, interval-, and hard-cut rules. Includes a pre-roll buffer to avoid clipping utterance starts. Provides crash-safe transcript continuity.
→ `src-tauri/src/continuous_dump.rs`

### Transcription
The conversion of audio to text via the local Whisper runtime. Always happens locally; never sent to cloud.
→ `src-tauri/src/transcription.rs`, `src-tauri/src/whisper_server.rs`

### Whisper Backend
The local ASR (Automatic Speech Recognition) runtime. Two binaries are shipped: `whisper-server` (preferred, server mode with warm model) and `whisper-cli` (fallback). CUDA and Vulkan builds exist; resolved at runtime via the `local_backend_preference` setting.
→ `src-tauri/src/whisper_server.rs`, `src-tauri/src/models.rs`

### AI Refinement
An optional post-transcription pass that sends raw transcript text to a local LLM (Ollama or LM Studio) or a cloud provider to correct, reformat, or summarize. A managed Module; disabled by default.

User-facing name: **AI Refinement** (UI tab, ModuleId `"ai_refinement"`).
Internal code name: `ai_fallback` (Rust module `src-tauri/src/ai_fallback/`, settings key `settings.ai_fallback`).

The "Fallback" in the code name refers to the **provider fallback chain** (Ollama local → cloud provider as fallback), per DEC-009/DEC-023. The module handles both provider selection and text processing (prompt construction, LLM call, result) in one place.

Same system, two names. "AI Refinement" is canonical for user-facing contexts.
→ `src-tauri/src/ai_fallback/`, `src/refinement-prompts.ts`, `src/refinement-inspector.ts`

### System Audio
The user-facing name for capturing and transcribing what's playing through the system speakers (WASAPI loopback). Formerly called "Output" — renamed per DEC-017.

The rename is complete in user-facing UI (HTML labels, history display, hero card). Internally, `source: "output"` persists as the data field value in history entries and settings keys (`outputDevices`, `transcribe_output_device`). This is intentional to avoid data migration.
→ `src-tauri/src/transcription.rs`, `src/history.ts`

### Overlay
An always-on-top transparent floating WebView window (`overlay.html`) that shows audio level and refinement status during recording. Controlled from Rust via `window.eval()`, not via Tauri events.

**Note:** "Overlay" is overloaded — also refers to CSS overlay dialogs in HTML. In ROADMAP Block F, a third window (Assistant Presence) is introduced alongside the main window and overlay.
→ `src-tauri/src/overlay.rs`, `overlay.html`, `public/overlay.js`

### Module System
A registry of opt-in feature modules (e.g. `gdd`, `ai_refinement`, `assistant_core`, `output_voice_tts`) each with consent flow, enable/disable lifecycle, and permission gating. Users enable modules via the **Modules Hub** UI.

**Note:** "Module" is an overloaded term in this codebase. It also refers to Rust crate modules (`mod audio` in `lib.rs`) and occasionally to UI panels ("Model Manager module"). In domain/user-facing contexts, "Module" always means a Feature Module from this registry.

→ `src-tauri/src/modules/`, `src/modules-hub.ts`, `src/types.ts:ModuleId`

### Assistant Core (`assistant_core`)
Wake-word listening, intent detection, and agent orchestration. Canonical ModuleId: `assistant_core`.

Legacy ID `workflow_agent` still exists in persisted user settings and as the Rust filename (`workflow_agent.rs`). Migration in `state.ts` normalizes `workflow_agent` → `assistant_core` on load. Rust filename rename is a cleanup candidate, not urgent.
→ `src-tauri/src/workflow_agent.rs`, `src/workflow-agent-console.ts`

### Partitioned History
The separation of mic dictation results (stored as `history`) from system-audio results (stored as `transcribeHistory`). Both are merged into the **Conversation** view in the UI.
→ `src-tauri/src/history_partition.rs`, `src/history.ts`

### Session
Intentionally overloaded term (confirmed by Ingo, 2026-05-15). Always means "something that starts and stops later." Three distinct usages in the codebase:
1. **Recording Session** — a bounded system-audio recording period (system-audio ON → OFF) that accumulates OPUS chunks and merges them into `session.opus`. Managed by `session_manager.rs`.
2. **App Session** — the app lifecycle from launch to quit. History is scoped to an app session and persisted on exit.
3. **Capture Session** — a single PTT press-and-release or VAD-detected utterance cycle.

No rename planned. Context determines which meaning applies.

### Product Mode
The top-level operating mode of the app: `"transcribe"` (dictation-first UX) or `"assistant"` (agent-first UX). Toggled via hotkey or UI button.
→ `src/types.ts:ProductMode`, `src/event-listeners.ts`

### Chapters
Time- or silence-based segmentation markers applied to long transcripts for navigation and export organization.
→ `src/chapters.ts`, `src/history.ts`

### Custom Vocabulary
User-defined find-replace pairs applied post-transcription by the rule-based Post-Processing pipeline. Separate from Vocabulary Learning (which auto-promotes candidates).
→ `src-tauri/src/postprocessing.rs`, `src/settings.ts`

### Vocabulary Learning
A diff-based system that tracks recurring AI corrections of user text (via LCS word-diff) and builds a candidate list for auto-promotion to Custom Vocabulary.
→ `src/vocab-auto-learn.ts`, `src-tauri/src/uiautomation_capture.rs`

### Post-Processing
Rule-based text enhancement applied after transcription (punctuation normalization, capitalization, custom vocabulary substitution, number formatting). Distinct from AI Refinement, which is LLM-based.
→ `src-tauri/src/postprocessing.rs`

---

## Open Questions

_Terms flagged as fuzzy — to be resolved with Ingo (repo owner)._

- **"Session"**: intentionally overloaded (see Term entry above). No rename planned — documented as-is.

