# CONTEXT.md — Trispr Flow Domain Glossary

This file is the canonical source for domain language in Trispr Flow.
It is written for domain experts (users, designers, contributors), not for implementation details.
Update this file inline as terms are resolved during design sessions.

Last updated: 2026-06-07

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

Whisper Backend is part of Trispr Core. Core owns the selected Whisper model, local model/storage paths needed to transcribe, backend preference (`auto`/CUDA/Vulkan), runtime warmup, server/CLI fallback, and minimal model-availability checks.

Model Management is only partially Core: the minimum ability to locate and use a model is Core, but larger management surfaces such as download/import/quantize/remove flows, online model discovery, benchmarking, and evaluation are optional tooling surfaces rather than the Core baseline.
→ `src-tauri/src/whisper_server.rs`, `src-tauri/src/models.rs`

### AI Refinement
An optional post-transcription pass that sends raw transcript text to a local LLM (Ollama or LM Studio) or a cloud provider to correct, reformat, or summarize. A managed Module; disabled by default.

User-facing name: **AI Refinement** (UI tab, ModuleId `"ai_refinement"`).
Internal code name: `ai_fallback` (Rust module `src-tauri/src/ai_fallback/`, settings key `settings.ai_fallback`).

The "Fallback" in the code name refers to the **provider fallback chain** (Ollama local → cloud provider as fallback), per DEC-009/DEC-023. The module handles both provider selection and text processing (prompt construction, LLM call, result) in one place.

Same system, two names. "AI Refinement" is canonical for user-facing contexts.

AI Refinement is an optional Feature Module, not part of Trispr Core. Its boundary includes provider selection, prompt/profile logic, LLM calls, fallback-chain behavior, and local AI runtime management such as Ollama or LM Studio when those runtimes serve refinement. AI model discovery/download/repair for refinement belongs here. Whisper model management, rule-based Post-Processing, and the Core transcription pipeline do not.
→ `src-tauri/src/ai_fallback/`, `src/refinement-prompts.ts`, `src/refinement-inspector.ts`

### System Audio
The user-facing name for capturing and transcribing what's playing through the system speakers (WASAPI loopback). Formerly called "Output" — renamed per DEC-017.

The rename is complete in user-facing UI (HTML labels, history display, hero card). Internally, `source: "output"` persists as the data field value in history entries and settings keys (`outputDevices`, `transcribe_output_device`). This is intentional to avoid data migration.

System Audio is part of Trispr Core. It is a baseline local capture/transcription path alongside mic capture, even though its current implementation is platform-specific. Platform specificity does not make it a Feature Module.
→ `src-tauri/src/transcription.rs`, `src/history.ts`

### Overlay
An always-on-top transparent floating WebView window (`overlay.html`) that shows audio level and refinement status during recording. Controlled from Rust via `window.eval()`, not via Tauri events.

**Note:** "Overlay" is overloaded — also refers to CSS overlay dialogs in HTML. In ROADMAP Block F, a third window (Assistant Presence) is introduced alongside the main window and overlay.

The capture overlay is part of Trispr Core because it is direct capture UX for PTT, VAD, and system-audio recording state. Assistant Presence, agent UI, GDD/Confluence surfaces, and generic CSS modal overlays are not Core; they are separate optional surfaces or UI mechanisms.
→ `src-tauri/src/overlay.rs`, `overlay.html`, `public/overlay.js`

### Trispr Core
The non-toggleable product baseline: local capture, local transcription, rule-based post-processing, history, and the app shell needed to operate those flows. Trispr Core is not a Feature Module and does not appear as something users can install, consent to, enable, or disable in the Modules Hub.

Trispr Core includes PTT, VAD, the Continuous Dump Pipeline, the Whisper Backend, Post-Processing, Partitioned History, core settings, hotkeys, and the capture overlay. Optional capabilities such as AI Refinement, Assistant Core, Voice Output TTS, Vision Input, GDD, Confluence, Task Capture, and Video Generation sit outside Core as Feature Modules.

Use **Trispr Core** for the product baseline. Use **Feature Module** for optional capabilities with consent, lifecycle, and permission gating.
→ `src-tauri/src/lib.rs`, `src-tauri/src/audio.rs`, `src-tauri/src/transcription.rs`, `src-tauri/src/postprocessing.rs`, `src-tauri/src/history_partition.rs`

### Module System
A registry of opt-in feature modules (e.g. `gdd`, `ai_refinement`, `assistant_core`, `output_voice_tts`) each with consent flow, enable/disable lifecycle, and permission gating. Users enable modules via the **Modules Hub** UI.

**Note:** "Module" is an overloaded term in this codebase. It also refers to Rust crate modules (`mod audio` in `lib.rs`) and occasionally to UI panels ("Model Manager module"). In domain/user-facing contexts, "Module" always means a Feature Module from this registry.

→ `src-tauri/src/modules/`, `src/modules-hub.ts`, `src/types.ts:ModuleId`

### Core Settings
The settings required for Trispr Core to operate: capture mode, hotkeys, transcription devices, Whisper backend/model selection, rule-based Post-Processing toggles, history/storage basics, and capture overlay basics.

Core Settings are distinct from Module Settings. The current persisted `Settings` shape still stores both Core and Module settings together, but architecturally the boundary is: Core owns baseline capture/transcription behavior; Feature Modules own their own provider, prompt, permission, runtime, and surface settings.

→ `src-tauri/src/state.rs`, `src/settings/`

### Module Settings
Settings owned by Feature Modules rather than by Trispr Core. Examples include AI Refinement provider/model/prompt/auth settings, Voice Output TTS provider/voice settings, Vision Input settings, GDD and Confluence settings, Assistant Core settings, Task Capture settings, and Video Generation settings.

Module Settings may currently be persisted inside the same `Settings` struct as Core Settings. Long-term modularization should not treat that shared persistence shape as the domain boundary.

→ `src-tauri/src/state.rs`, `src-tauri/src/modules/`, `src/settings/`

### Bundled
A build/installer property: the code, binary, sidecar, or resource is included in the delivered app package. Bundled does not mean the user wants the capability active, and it does not guarantee the capability can run right now.

### Installed
A local setup property: the capability's required local assets, sidecars, models, secrets, or runtime dependencies are present and configured on this machine. Installed does not mean enabled.

### Enabled
A user-intent property: the user has turned the capability on in the Modules Hub or settings. Enabled does not guarantee availability; a capability can be enabled but degraded or unavailable.

### Available
A runtime-health property: the capability can be used now. Availability depends on enabled state plus local runtime health, required assets, permissions, credentials, and external service reachability where applicable.

Feature Module lifecycle should distinguish these states: **Bundled → Installable/Installed → Enabled → Available/Healthy**. Trispr Core does not follow this module lifecycle; Core is always the baseline, though parts of Core can still be degraded (for example, if no Whisper model is available).

### Assistant Core (`assistant_core`)
Wake-word listening, intent detection, and agent orchestration. Canonical ModuleId: `assistant_core`.

Assistant Core is an optional Feature Module, not part of Trispr Core. Trispr Core can transcribe without Assistant Core. Product Mode `"assistant"` activates an agent-first UX that depends on Assistant Core; it does not redefine Core itself.

Legacy ID `workflow_agent` still exists in persisted user settings and as the Rust filename (`workflow_agent.rs`). Migration in `state.ts` normalizes `workflow_agent` → `assistant_core` on load. Rust filename rename is a cleanup candidate, not urgent.
→ `src-tauri/src/workflow_agent.rs`, `src/workflow-agent-console.ts`

### Voice Output TTS (`output_voice_tts`)
Optional Feature Module that speaks text aloud. Voice Output TTS is not part of Trispr Core: Core can emit events or text that Voice Output consumes, but speech playback, provider/voice selection, Piper/Qwen/Windows runtime behavior, playback control, output policy, and voice settings belong to the Feature Module.

→ `src-tauri/src/multimodal_io.rs`, `src/settings/voice-output.settings.ts`

### Assistant Presence (`assistant_presence`)
Optional assistant-facing surface/window. Assistant Presence is not part of Trispr Core and is distinct from the capture overlay. It can be bundled with or enabled alongside Assistant Core, but it remains a Feature Module surface rather than baseline capture/transcription UX.

→ `src-tauri/src/assistant_presence.rs`, `assistant-presence.html`, `public/assistant-presence.js`

### Vision Input (`input_vision`)
Optional Feature Module for screen capture, visual context, and vision snapshots. Vision Input is not part of Trispr Core because Core transcription does not require visual context. It owns its own privacy/consent boundary, settings, health checks, and capture lifecycle.

→ `src-tauri/src/multimodal_io.rs`, `src/settings/ai-refinement.settings.ts`

### Task Capture (`task_capture`)
Optional Feature Module that interprets transcript text as tasks, reminders, or agenda items and can post them to configured task endpoints. Task Capture is not part of Trispr Core: Core produces transcript text, while Task Capture is an optional automation/action layer on top of that text.

Task Capture owns its own route settings, endpoint checks, task-formatting prompt, and delivery behavior.

→ `src-tauri/src/modules/task_capture.rs`, `src/task-capture-config.ts`

### GDD Automation (`gdd`)
Optional Feature Module that turns transcript/history material into Game Design Document drafts and publishing workflows. GDD is not part of Trispr Core: Core provides transcript text and history; GDD is an optional document-generation capability on top of that material.

### Confluence Integration (`integrations_confluence`)
A GDD-owned integration surface. From a product perspective, Confluence belongs to the GDD publishing workflow rather than being a generic integration platform. Architecturally it keeps a separate integration boundary for auth, secrets, network access, spaces/pages, target routing, and publish queue behavior.

Confluence may remain a separate ModuleId for lifecycle, permission, and security tracking. That implementation detail is reversible: it can later become an internal GDD sub-capability or remain separately toggleable without changing the Core boundary.

→ `src-tauri/src/gdd/`, `src-tauri/src/gdd/confluence.rs`

### Video Generation (`output_video_generation`)
Optional Feature Module that generates video output artifacts from transcripts, source items, and assets. Video Generation is not part of Trispr Core: Core can capture, transcribe, post-process, and persist text without rendering video.

Video Generation owns its own sidecar/runtime concerns, jobs, work directories, asset materialization, export paths, and render lifecycle.

→ `src-tauri/src/video_generation.rs`, `src-tauri/src/video_ingest.rs`

### Analysis (`analysis`)
Optional Feature Module for inspecting, comparing, or deriving insights from transcript/history data. Analysis is not part of Trispr Core: Core provides transcript and history data, while Analysis consumes it through optional views, exports, or insight surfaces.

→ `src/modules-hub.ts`, `src/types.ts:ModuleId`

### Export
Basic transcript persistence and simple user export are Core-adjacent baseline behavior because users must be able to keep or move their transcript text. Advanced export workflows are optional tooling or Feature Module surfaces.

Advanced exports include batch export, analytics export, schema-heavy archive formats, GDD/Confluence publishing, and Video Generation output.

→ `docs/EXPORT_GUIDE.md`, `docs/EXPORT_SCHEMA.md`

### Partitioned History
The separation of mic dictation results (stored as `history`) from system-audio results (stored as `transcribeHistory`). Both are merged into the **Conversation** view in the UI.

Partitioned History is part of Trispr Core: transcript storage, mic/system separation, partition loading/saving, active history lists, and basic crash-safe persistence are baseline product behavior. Optional consumers of history, such as GDD source selection, Assistant recaps, semantic search, analytics, or advanced exports, are Feature Module surfaces that depend on Core history rather than belonging to it.
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

Custom Vocabulary is part of Trispr Core despite being user-defined data: it is deterministic, local, synchronous, and has no runtime lifecycle, permission surface, or consent flow of its own.
→ `src-tauri/src/postprocessing.rs`, `src/settings/vocabulary.settings.ts`

### Vocabulary Learning
A diff-based system that tracks recurring AI corrections of user text (via LCS word-diff) and builds a candidate list for auto-promotion to Custom Vocabulary.

Vocabulary Learning is not part of the Core baseline while it depends on AI correction signals. Treat it as optional or Core-adjacent until its lifecycle and dependency on AI Refinement are resolved.
→ `src/vocab-auto-learn.ts`, `src-tauri/src/uiautomation_capture.rs`

### Post-Processing
Rule-based text enhancement applied after transcription (punctuation normalization, capitalization, custom vocabulary substitution, number formatting). This deterministic rule-based path is part of Trispr Core.

Individual rules can be configurable. Alternative rule sets or rule versions are a possible future internal extension point for reproducibility or language-specific behavior, but they are not Feature Modules. Distinct from AI Refinement, which is LLM-based and optional.
→ `src-tauri/src/postprocessing.rs`

---

## Open Questions

_Terms flagged as fuzzy — to be resolved with Ingo (repo owner)._

- **"Session"**: intentionally overloaded (see Term entry above). No rename planned — documented as-is.

