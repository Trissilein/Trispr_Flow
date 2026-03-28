# Decisions

Last updated: 2026-03-28

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

### DEC-015 CUDA installer optimization (v0.4.1, superseded)

- Status: `superseded`
- Decision: Keep `cublasLt64_13.dll` in CUDA runtime bundle again.
- Why: Field installs on 2026-03-23 showed `whisper-cli.exe` startup failure (`cublasLt64_13.dll` missing). Current CUDA runtime chain is not reliably loadable without that DLL on all target systems.

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

### DEC-032 Managed module platform baseline (2026-03-04)

- Status: `accepted`
- Decision: Introduce a managed internal module platform (`not_installed | installed | enabled | active | error`) with explicit dependencies, lifecycle hooks, and permission consent.
- Context: Feature scope now includes larger optional capabilities (GDD automation, analysis relaunch, integrations) that should not increase baseline complexity for all users.
- Why: Controlled modular growth without introducing external plugin execution risk. Keeps core transcription stable while optional modules can fail independently.

### DEC-033 GDD automation as standalone module (2026-03-04)

- Status: `accepted`
- Decision: Implement GDD generation as a dedicated module, not as part of AI refinement.
- Context: GDD creation requires multi-pass chunking, structured synthesis, schema validation, and publish routing—very different runtime profile than per-entry refinement.
- Why: Better token-budget control, clearer UX boundaries, simpler reliability guarantees, and lower regression risk for transcription/refinement paths.

### DEC-034 Module permission model (2026-03-04)

- Status: `accepted`
- Decision: Require explicit per-module permission consent before first enable (`network_confluence`, `filesystem_history`, `filesystem_exports`, `keyring_access`).
- Context: Optional modules touch sensitive resources (network publishing, credential storage).
- Why: Transparent trust model and auditability with minimal friction.

### DEC-035 Confluence target platform and auth strategy (2026-03-04)

- Status: `accepted`
- Decision: Scope first integration to **Confluence Cloud** with dual auth tracks: OAuth 3LO and API token.
- Context: Cloud has stable public APIs and aligns with current target workflows.
- Why: Fastest path to production utility while preserving enterprise-compatible auth options.

### DEC-036 Confluence publish policy (2026-03-04)

- Status: `accepted`
- Decision: Default flow is `Draft + Review + Publish`; optional one-click mode is supported but must fall back to confirmation when routing confidence is low.
- Context: Automatic document placement has unavoidable ambiguity.
- Why: Balances speed and safety; protects data integrity while still enabling automation.

### DEC-037 GDD template source strategy (2026-03-04)

- Status: `implemented`
- Decision: GDD generation accepts optional template guidance loaded from Confluence URLs or uploaded files (`pdf`, `docx`, `txt`, `md`).
- Context: Teams often already maintain structure/style templates. Reusing these reduces setup friction and improves output consistency.
- Why: Keeps baseline preset system intact while allowing low-friction adaptation without building a full template DSL.

### DEC-038 GDD module UX baseline (2026-03-04)

- Status: `implemented`
- Decision: Deliver a single modal-based GDD workflow in Modules with explicit stages: detect preset -> generate draft -> validate -> suggest target -> publish.
- Context: Early module integration needed a concrete end-to-end path before introducing complex automation modes.
- Why: Gives users an immediately useful manual flow with clear checkpoints and minimal hidden automation.

### DEC-039 Confluence OAuth runtime handling (2026-03-04)

- Status: `implemented`
- Decision: Use OAuth code exchange + refresh-token retry on 401 and persist selected cloud site context (`site_base_url`, `oauth_cloud_id`) in settings.
- Context: Confluence Cloud 3LO tokens are short-lived and workspace selection is required for API gateway routes.
- Why: Prevents brittle session failures and avoids repeated manual reconfiguration between restarts.

### DEC-040 GDD as core capability (2026-03-05)

- Status: `implemented`
- Decision: Treat `gdd` and `integrations_confluence` as core-always-on modules in the module registry and lifecycle.
- Context: Manual GDD flow should be always available; module toggles are reserved for autonomous orchestration capabilities.
- Why: Removes misleading enablement friction and aligns module semantics with real product boundaries.

### DEC-041 Workflow-agent as optional automation module (2026-03-05)

- Status: `implemented`
- Decision: Introduce `workflow_agent` as an optional managed module that orchestrates GDD generation/publish via speech commands.
- Context: Users need automation without coupling orchestration to baseline transcription/GDD usage.
- Why: Preserves core stability while enabling opt-in automation.

### DEC-042 Wakeword + confirm command model (2026-03-05)

- Status: `implemented`
- Decision: Agent flow uses wakeword detection plus plan/confirm execution path.
- Context: External side effects (Confluence publish) need explicit user control.
- Why: Safety-first automation with lower accidental-execution risk.

### DEC-043 Plan+confirm always for external actions (2026-03-05)

- Status: `accepted`
- Decision: Execute path keeps a strict confirm gate before publish-capable actions.
- Context: Fully autonomous publish behavior is intentionally out-of-scope in this iteration.
- Why: Limits risk while still reducing operator workload.

### DEC-044 Separate agent command channel (2026-03-05)

- Status: `implemented`
- Decision: Add `transcription:raw-result` event channel before activation-word drop filtering; keep legacy activation words as transcript filter only.
- Context: Existing activation words are not a command channel and would suppress valid agent intents.
- Why: Clean separation of transcript filtering and command orchestration.

### DEC-045 Session resolution by temporal/topic scoring (2026-03-05)

- Status: `implemented`
- Decision: Build candidate sessions via gap grouping and score by temporal hint, topic overlap, and recency.
- Context: Natural references like “gestern/vorhin … über X” require probabilistic matching with disambiguation.
- Why: Robust candidate shortlist with transparent reasoning and confirm-before-execute.

### DEC-046 Always ask target language in agent flow (2026-03-05)

- Status: `implemented`
- Decision: Agent Console includes explicit target-language selection before plan build/execute.
- Context: Output language can differ from source and must be user-directed.
- Why: Prevents silent language assumptions and keeps intent explicit.

### DEC-047 Vision module policy baseline (2026-03-05)

- Status: `accepted`
- Decision: Vision module defaults to low-fps, all-monitors scope, and no disk persistence.
- Context: Screen context is useful for agent workflows but privacy and storage must remain bounded.
- Why: Balances capability and privacy with strict retention constraints.

### DEC-048 Dual TTS provider strategy (2026-03-05)

- Status: `implemented`
- Decision: Expose both `windows_native` and `local_custom` provider lanes with deterministic fallback behavior.
- Context: We need immediate voice-output coverage while evaluating final provider defaults.
- Why: Keeps product flexible and benchmark-driven without blocking delivery.

### DEC-049 Windows-first multimodal rollout (2026-03-05)

- Status: `accepted`
- Decision: Deliver multimodal runtime first on Windows, while keeping command/settings interfaces provider-platform agnostic.
- Context: Current deployment target and credentials/tooling are Windows-centric.
- Why: Fastest path to production value with lower integration risk.

### DEC-050 Block L hardening gate closure policy (2026-03-06)

- Status: `implemented`
- Decision: Treat Block L as closed only when all three are true: one-click confidence gate with confirmation fallback is active, publish retry/queue resilience tests are green, and rollout packet docs are updated.
- Context: Block L had functional delivery completed but hardening tasks remained partially tracked.
- Why: Prevents premature milestone closure and keeps publish safety/recovery behavior auditable.

### DEC-051 TTS natural voice engine selection (2026-03-08)

- Status: `accepted`
- Decision: Use **Piper TTS** as the `local_custom` provider for the mainline app. Reserve **Kokoro TTS** for the separate `analysis-module-branch` / VibeVoice integration path.
- Context: Evaluated four local neural TTS options (Piper, Kokoro, Coqui XTTS-v2, Edge TTS). The `local_custom` lane needs an offline, natural-sounding, low-latency engine. Kokoro has better audio quality but requires a Python sidecar and GPU. Piper is a standalone binary (~25 MB), supports German voices out of the box, achieves < 200 ms latency on CPU, and integrates as a Tauri sidecar/PATH binary with zero new runtime dependencies.
- Why: Piper aligns with offline-first principle; no Python sidecar needed; ~1.5 days of integration work. Kokoro's quality advantage is better suited to the VibeVoice analysis workflow where Python is already present and GPU time is available.
- Implementation notes:
  - Piper binary resolved via `piper_binary_path` setting → PATH → `%LOCALAPPDATA%\trispr-flow\piper\piper.exe`
  - Active model path stored in `piper_model_path` (`VoiceOutputSettings`)
  - Voice model directory scanned for `.onnx` files stored in `piper_model_dir`
  - Synthesis: `piper.exe --model <model.onnx> --output_file <tmp.wav>` then cpal WAV playback
  - Fallback: `local_custom` → `windows_native` (existing chain)

### DEC-052 Voice Interaction Model — Stufen-Architektur (2026-03-09)

- Status: `implemented` (Stufe 1 abgeschlossen, Stufe 2/3 als Folgeblöcke offen)
- Decision: Dreistufige Voice-Interaktionsarchitektur:
  - **Stufe 1 — Passives Feedback** (sofort, Block N6/N8): TTS spricht Agent-Antworten; policy `agent_replies_only`. Keine Architekturänderungen.
  - **Stufe 2 — Confirmation Loop** (Block O): Agent stellt Bestätigungsfrage via TTS; User antwortet via Aktivierungswort. Erfordert `awaiting_confirmation`-State, Confirmation-Token-Matching und `confirm/cancel_pending_action`-Commands.
  - **Stufe 3 — Hands-Free Screen Interaction** (Block P): Agent erkennt aktives Fenster (N5 Vision), injiziert Text via `enigo` (bereits in `Cargo.toml`), meldet Ergebnis via TTS.
- Context: Trispr Flow ist derzeit "interaction-less" (Mikrofon → Transkription → Clipboard). Mit TTS (N6/N8) und Vision (N5) wird ein echter Voice-Loop ohne Tastatur möglich.
- Why: Stufe 1 liefert sofort Mehrwert mit vorhandenen Komponenten. Stufe 2 + 3 sind eigenständige Blöcke, die keine Stufe-1-Architektur ändern. `enigo` ist bereits als Dependency vorhanden → Block P erfordert keine neuen Runtime-Dependencies.
- Implementation notes:
  - Stufe 1 ist im Workflow-Agent umgesetzt (`agent_reply`/`agent_event` Kontexte, policy-gated `speak_tts`).
  - Policy-Matrix ist aktiv (`agent_replies_only`, `replies_and_events`, `explicit_only`) und durch Integrationstests abgedeckt (`N12`).

### DEC-053 Voxtral TTS Lab-Evaluierung (2026-03-27)

- Status: `accepted` (lab-only, conditional go)
- Decision: Für Trispr wird eine **lokale, experimentelle** Voxtral-TTS-Evaluierung freigegeben (`voxtral_local_experimental`), strikt opt-in und ohne Änderung des Produktionsdefaults.
- Context: Das bestehende TTS-Setup (`windows_native`/`windows_natural`/`local_custom`=Piper) ist stabil. Voxtral zeigt hohes Qualitäts-Potenzial, aber die Open-Weights-TTS-Lane ist derzeit NC-lizenziert und damit nicht automatisch produktions-/kommerztauglich.
- Why: Wir wollen Qualitätsgewinn (Natürlichkeit) unter realen lokalen Bedingungen messen, ohne die aktuelle Runtime-Stabilität oder Release-Policy zu gefährden.
- Guardrails:
  - Nur intern/lab.
  - Kein Default-Switch weg von Piper.
  - Deterministischer Fallback auf bestehende Provider bleibt verpflichtend.
  - Vor jeder kommerziellen Nutzung ist ein separates Lizenz-/Alternativen-Gate Pflicht.
- Reference:
  - `docs/VOXTRAL_TTS_DECISION_MEMO.md`

### DEC-054 Agent-Evolution Guardrails und Phasenpfad (2026-03-28)

- Status: `accepted`
- Decision: Die Agent-Entwicklung folgt verbindlich einem Phasenpfad `S13.5 -> T -> V -> O -> P` mit den Produktleitplanken **Hybrid activation**, **Plan+Confirm**, **GDD copilot first**, **Local-first**.
- Context: Kernbausteine (Transkript, Archiv, TTS, Workflow-Agent, GDD) sind vorhanden, aber der Weg zum vollwertigen Assistant benötigte eine klare Priorisierungs- und Sicherheitslinie.
- Why: Verhindert Scope-Drift, schützt den stabilen Transcribe-Flow und schafft abarbeitbare Exit-Kriterien je Ausbauphase.
- Guardrails:
  - Wakeword-Auswertung nur im `assistant`-Modus.
  - Side-effect Aktionen bleiben confirm-pflichtig.
  - Kein Breaking Change für den bestehenden Transcribe-Produktpfad.
  - Transcript/Archive bleiben v1-Primärwissensbasis.
- Reference:
  - `docs/AGENT_EVOLUTION_ROADMAP.md`
  - `ROADMAP.md`
  - `docs/TASK_SCHEDULE.md`

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
