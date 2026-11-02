# CONTEXT.md — Trispr Flow Domain Glossary

This file is the canonical source for domain language in Trispr Flow.
It is written for domain experts (users, designers, contributors), not for implementation details.
Update this file inline as terms are resolved during design sessions.

Last updated: 2026-11-02

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

### Release Gate
The evidence-backed check that decides whether the current release line can move forward. Trispr Core and the active Whisper path (CUDA or Vulkan) must pass, and a fresh/default profile must not show false red errors for optional parts. The assistant and TTS release-gate scripts were removed with those features.

Confirmed 2026-06-09: latency evidence must use the production default Whisper model class, `ggml-large-v3-turbo.bin`, or an explicit `TRISPR_WHISPER_MODEL` pointing to equivalent evidence. Smaller smoke models are not accepted as final release evidence.

→ `docs/RELEASE_QA_CHECKLIST.md`, `scripts/latency-benchmark.ps1`

### AI Refinement
An optional post-transcription pass that sends raw transcript text to a local Ollama model to correct, reformat, or summarize. A managed Module; disabled by default.

User-facing name: **AI Refinement** (UI tab, ModuleId `"ai_refinement"`).
Internal code name: `ai_fallback` (Rust module `src-tauri/src/ai_fallback/`, settings key `settings.ai_fallback`). The name is historical: the old provider fallback chain (Ollama, then cloud) was removed, Ollama is the only provider. Same system, two names; "AI Refinement" is canonical for user-facing contexts.

AI Refinement is an optional Feature Module, not part of Trispr Core. Its boundary includes prompt/profile logic, LLM calls, and the managed local Ollama runtime (detect, install, start, verify, version switching) and the three-model catalog (Gemma 4 E4B recommended, Gemma 4 E2B, Qwen 3.5 4B). Whisper model management, rule-based Post-Processing, and the Core transcription pipeline do not belong here.
→ `src-tauri/src/ai_fallback/`, `src-tauri/src/ollama_runtime.rs`, `src/refinement-prompts.ts`, `src/refinement-inspector.ts`

### System Audio
The user-facing name for capturing and transcribing what's playing through the system speakers (WASAPI loopback). Formerly called "Output" — renamed per DEC-017.

The rename is complete in user-facing UI (HTML labels, history display, hero card). Internally, `source: "output"` persists as the data field value in history entries and settings keys (`outputDevices`, `transcribe_output_device`). This is intentional to avoid data migration.

System Audio is part of Trispr Core. It is a baseline local capture/transcription path alongside mic capture, even though its current implementation is platform-specific. Platform specificity does not make it a Feature Module.
→ `src-tauri/src/transcription.rs`, `src/history.ts`

### Overlay
An always-on-top transparent floating WebView window (`overlay.html`) that shows audio level and refinement status during recording. Controlled from Rust via `window.eval()`, not via Tauri events.

**Note:** "Overlay" is overloaded, it also refers to CSS overlay dialogs in HTML.

The capture overlay is part of Trispr Core because it is direct capture UX for PTT, VAD, and system-audio recording state. Generic CSS modal overlays are a UI mechanism, not Core.
→ `src-tauri/src/overlay.rs`, `overlay.html`, `public/overlay.js`

### Trispr Core
The non-toggleable product baseline: local capture, local transcription, rule-based post-processing, history, and the app shell needed to operate those flows. Trispr Core is not a Feature Module and does not appear as something users can install, consent to, enable, or disable in the Modules Hub.

Trispr Core includes PTT, VAD, the Continuous Dump Pipeline, the Whisper Backend, Post-Processing, Partitioned History, core settings, hotkeys, and the capture overlay. AI Refinement (Ollama) and the Opus export sidecar sit outside Core as Feature Modules.

Use **Trispr Core** for the product baseline. Use **Feature Module** for optional capabilities with consent, lifecycle, and permission gating.
→ `src-tauri/src/lib.rs`, `src-tauri/src/audio.rs`, `src-tauri/src/transcription.rs`, `src-tauri/src/postprocessing.rs`, `src-tauri/src/history_partition.rs`

### Module System
A registry of opt-in feature modules (today `ai_refinement` and `opus`) each with enable/disable lifecycle and health reporting. Users manage modules via the **Modules Hub** UI.

**Note:** "Module" is an overloaded term in this codebase. It also refers to Rust crate modules (`mod audio` in `lib.rs`) and occasionally to UI panels ("Model Manager module"). In domain/user-facing contexts, "Module" always means a Feature Module from this registry.

Setup and health reporting must separate lifecycle state from user intent. Disabled, uninstalled, or unused optional Feature Modules are not global app errors. Missing runtime, model, or dependency setup is user-facing setup work only when the user has enabled the feature or is actively using that feature surface. Trispr Core failures remain release blockers.

→ `src-tauri/src/modules/`, `src/modules-hub.ts`, `src/types.ts:ModuleId`

### Core Settings
The settings required for Trispr Core to operate: capture mode, hotkeys, transcription devices, Whisper backend/model selection, rule-based Post-Processing toggles, history/storage basics, and capture overlay basics.

Core Settings are distinct from Module Settings. The current persisted `Settings` shape still stores both Core and Module settings together, but architecturally the boundary is: Core owns baseline capture/transcription behavior; Feature Modules own their own provider, prompt, permission, runtime, and surface settings.

→ `src-tauri/src/state.rs`, `src/settings/`

### Module Settings
Settings owned by Feature Modules rather than by Trispr Core. Today these are the AI Refinement model/prompt/runtime settings (`settings.ai_fallback`, `settings.providers.ollama`).

Module Settings may be persisted inside the same `Settings` struct as Core Settings. Long-term modularization should not treat that shared persistence shape as the domain boundary.

→ `src-tauri/src/state.rs`, `src-tauri/src/modules/`, `src/settings/`

### Bundled
A build/installer property: the code, binary, sidecar, or resource is included in the delivered app package. Bundled does not mean the user wants the capability active, and it does not guarantee the capability can run right now.

### Installed
A local setup property: the capability's required local assets, sidecars, models, secrets, or runtime dependencies are present and configured on this machine. Installed does not mean enabled.

Target module installability: Feature Modules should eventually be installable by unpacking a module package into a module directory. This is the product goal for module installation, not the current implementation state of every module.

Confirmed 2026-06-08: the first target model for installable modules is package-based, not arbitrary runtime code loading. A module package may contain a manifest, presets/templates/assets, configuration defaults, and sidecar binaries or resources for host-known capabilities. The host app still owns the executable Rust/TypeScript capability surface through compiled code and feature gates.

Confirmed 2026-06-08: an unpacked module is `Installed` only when its manifest is valid, its module ID and host capability are known, its version/schema constraints are compatible with the host, and all manifest-declared required assets are present and schema-valid. `Installed` is therefore a validation result, not a mere directory-exists check.

### Enabled
A user-intent property: the user has turned the capability on in the Modules Hub or settings. Enabled does not guarantee availability; a capability can be enabled but degraded or unavailable.

Confirmed 2026-06-08: `Enabled` stays user intent for installable modules. Enabling a module may launch consent or setup flows, but permissions, secrets, and external service configuration are not part of `Installed`. Missing setup means the module is enabled but degraded or not available.

### Available
A runtime-health property: the capability can be used now. Availability depends on enabled state plus local runtime health, required assets, permissions, credentials, and external service reachability where applicable.

For release and setup reporting, unavailable optional features should be classified by intent. If the feature is disabled or unused, report it neutrally in feature-local surfaces. If the feature is enabled or selected, report the missing prerequisite as setup-needed or fallback-active. Reserve global error language for Trispr Core failures, active release-gate failures, and optional capabilities the current default or selected flow depends on.

Feature Module lifecycle should distinguish these states: **Bundled → Installable/Installed → Enabled → Available/Healthy**. Trispr Core does not follow this module lifecycle; Core is always the baseline, though parts of Core can still be degraded (for example, if no Whisper model is available).

Module installation should not imply that the app can load arbitrary third-party code into the process. If runtime plugin code is ever introduced, it needs a separate security and compatibility decision.

### Removed features (lean-down, 2026-11-02)
Trispr Flow was cut back to its core on 2026-11-02 (Ingo's decision, binding). Removed completely: cloud providers, LM Studio and Oobabooga, TTS / Voice Output / Piper, Video Generation, Vision Input, benchmarks, Assistant Core and Assistant Presence (including Product Mode), GDD and Confluence, Task Capture. Old `settings.json` files still load: unknown fields are ignored and old provider values fall back to `ollama`. Historical decisions about these features stay in `docs/DECISIONS.md` and `docs/archive/` as history only.

### Export
Basic transcript persistence and simple user export are Core-adjacent baseline behavior because users must be able to keep or move their transcript text. Advanced export workflows (batch export, analytics export, schema-heavy archive formats, Opus recordings) are optional tooling surfaces.

→ `docs/EXPORT_GUIDE.md`, `docs/EXPORT_SCHEMA.md`

### Partitioned History
The separation of mic dictation results (stored as `history`) from system-audio results (stored as `transcribeHistory`). Both are merged into the **Conversation** view in the UI.

Partitioned History is part of Trispr Core: transcript storage, mic/system separation, partition loading/saving, active history lists, and basic crash-safe persistence are baseline product behavior.
→ `src-tauri/src/history_partition.rs`, `src/history.ts`

### Session
Intentionally overloaded term (confirmed by Ingo, 2026-05-15). Always means "something that starts and stops later." Three distinct usages in the codebase:
1. **Recording Session** — a bounded system-audio recording period (system-audio ON → OFF) that accumulates OPUS chunks and merges them into `session.opus`. Managed by `session_manager.rs`.
2. **App Session** — the app lifecycle from launch to quit. History is scoped to an app session and persisted on exit.
3. **Capture Session** — a single PTT press-and-release or VAD-detected utterance cycle.

No rename planned. Context determines which meaning applies.

### Product Mode
Removed with the assistant (2026-11-02). The app has one operating mode, dictation. A stored `product_mode` in an old `settings.json` is ignored.

### Chapters
Time- or silence-based segmentation markers applied to long transcripts for navigation and export organization.
→ `src/history.ts`

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
