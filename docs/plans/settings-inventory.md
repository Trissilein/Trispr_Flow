# Trispr Flow settings inventory

Status: **inventory complete; final QA pending**. A1, A2, and A3 read-only inventories are complete. B1 static annotation and B2 editor work are complete enough for the current handoff; parent QA still owns the final legacy expectation, type, browser, and full-suite gates.

This inventory covers the settings and settings-adjacent controls that participate in progressive disclosure. It records the user-facing ownership unit, actual DOM surface, TypeScript binding, persisted Rust/JSON owner, default, proposed Basic or Expert surface, runtime gates, and action/status boundaries. It does not turn every wrapper, value label, meter, or button into a setting.

## Reading rules

- A **value** is an editable persisted field or a local UI preference. An **action** changes runtime state, files, models, routes, or history and gets its own row or action note. A **status** reports runtime state and never becomes a profile value.
- A row can group duplicate controls that intentionally edit one setting. The grouped controls must stay synchronized and must receive one logical visibility key. Examples are the two `opus_enabled` toggles and the Whisper/AI language surfaces.
- Dynamic rows are explicit even when child nodes have no stable static id. The renderer or post-render refresh must attach the existing `data-settings-visibility-*` metadata without inventing a second settings registry.
- Basic and Expert are proposed defaults for the view editor. Module availability, capture mode, style, provider, `hidden`, `disabled`, and other runtime gates continue to win. A Basic profile must not revive an unavailable feature.
- The rows below are a cross-domain handoff, not a claim that one row equals one DOM control or one backend key. Detailed evidence remains in the external handoffs `C:\Users\trist\Documents\Codex\2026-09-10\battle-plan-liegt-in-settings-cleanup\work\inventory-a1.md`, `inventory-a2.md`, and `inventory-a3.md`.

## Current B1 static baseline

B1 static audit reports **204 non-dialog controls**, **166 managed ownership units**, **24 Basic** defaults, **142 Expert** defaults, and **39 explicit global/navigation exclusions**. Only hotkey input plus record-button pairs share one logical unit; there are no duplicate profile keys. These are different units: a control, action, logical ownership unit, and persisted setting are not interchangeable counts.

The 142 Expert units exceed the provisional 60-unit review signal deliberately. No control is removed to satisfy a quota. Actions, status, runtime gates, and inherited ancestors are audited separately, and the review list contains no Kill decision. Dynamic renderers still need final browser coverage for attachment timing and lazy entries.

The live-tree snapshot used by the inventories found 417 elements carrying a `field` class token, 60 elements whose entire class attribute is exactly `field`, 160 actual `input`/`select`/`textarea` controls, 44 `data-expert-only` markers, and 16 sections. These counts answer different questions. They are not the number of inventory rows, managed metadata entries, or persisted settings. B1 tests must establish actual annotated coverage after static and dynamic rendering; this document deliberately does not claim those counts match.

The repository already contained an unrelated modification to `src-tauri/src/lib.rs` and an untracked `docs/plans/` directory at inventory start. That file has no ownership in this work. The visibility profile is a separate local browser profile and does not change `settings.json`, Rust settings models, backend defaults, or runtime gates.

## Proposed ownership and profile rows

### A1 — Capture, recording, Whisper, overlay, and text output

| Ownership unit | Actual control ids or dynamic surface | TypeScript binding | Rust path / JSON key / default | Proposed surface | Runtime, action, and dependency boundary |
|---|---|---|---|---|---|
| Capture switch and source | `#capture-enabled-toggle`, `#device-select`, `#mode-select` | `src/wiring/transcription.wire.ts`, `dom.captureEnabledToggle`, `dom.deviceSelect`, `dom.modeSelect` | `Settings.capture_enabled` / `capture_enabled` `true`; `input_device` `"default"`; `mode` `"ptt"` | **Basic** | Starts mic capture, reconciles devices, and chooses PTT/VAD. Mode controls the visibility of VAD and hotkey units. |
| PTT hotkey | `#ptt-hotkey` plus `#ptt-hotkey-record` | `setupHotkeyRecorder('ptt')` | `hotkey_ptt` `Ctrl+Space` | **Basic** | Readonly value plus record action. Keep the action separate from the value. |
| Toggle hotkey | `#toggle-hotkey` plus record action | `setupHotkeyRecorder('toggle')` | `hotkey_toggle` `Ctrl+Shift+Space` | **Expert** | Current expert marker and mode dependency remain. |
| PTT/VAD tuning | `#ptt-use-vad-toggle`, `#vad-threshold`, `#vad-silence` | `dom.pttUseVadToggle`, `dom.vadThreshold`, `dom.vadSilence` | `ptt_use_vad` `false`; `vad_threshold_start` plus legacy `vad_threshold` `0.02`; `vad_silence_ms` `700` | **Expert** | `#vad-block` is mode-dependent. Keep normalized UI units and legacy synchronization. |
| Mic timing and gain | `#ptt-hot-keepalive`, `#mic-gain`, `#continuous-mic-*` | `src/settings/continuous-dump.settings.ts`, transcription wiring | `ptt_hot_keepalive_ms` `600000`; `mic_input_gain_db` `0`; mic override and flush fields `false/10000/1200/45000` | **Expert** | Capture stream and segment timing. The TS missing-field fallback for keepalive differs from the Rust default and needs a later defaults review. |
| Recording archive | `#opus-archive-toggle` and duplicate `#opus-enabled-toggle` | `dom.opusArchiveToggle`, `dom.opusEnabledToggle` | One logical `opus_enabled` `true`; `opus_bitrate_kbps` `64` | **Expert** | Two surfaces edit one setting and must remain synchronized. Do not create two profile keys. |
| Audio storage and cues | `#auto-save-mic-audio-toggle`, `#auto-save-system-audio-toggle`, `#audio-cues-toggle`, `#audio-cues-volume` | `src/settings/continuous-dump.settings.ts`, transcription wiring | `auto_save_mic_audio`/`auto_save_system_audio` `false`; `audio_cues` `true`; `audio_cues_volume` `0.3` persisted, `0..100` UI | **Expert** | Writes recordings or plays cues. Storage and cue actions remain runtime behavior, not removal candidates. |
| System audio source | `#transcribe-enabled-toggle`, `#transcribe-device-select`, `#transcribe-hotkey`, `#transcribe-vad-toggle` | `dom.transcribe*`, transcription wiring | `transcribe_enabled` `true`; `transcribe_output_device` `"default"`; `transcribe_hotkey` `Ctrl+Shift+T`; `transcribe_vad_mode` `false` | **Expert** | Whole System Audio section currently has an expert ancestor. Basic view must not revive it without an explicit product decision and runtime availability. |
| System dump profile and timing | `#continuous-dump-enabled-toggle`, `#continuous-dump-profile`, `#transcribe-vad-*`, `#transcribe-batch-interval`, `#transcribe-chunk-overlap`, `#continuous-*` | `src/settings/continuous-dump.settings.ts`, transcription wiring | `continuous_dump_enabled` `true`; profile `balanced`; thresholds/timing defaults `0.04/900/8000/1000/45000/1000/300/200/60000` plus source overrides | **Expert** | `continuous-dump-profile` is one user-facing preset that writes several derived fields. Keep its save/render semantics intact; split its status and actions. |
| Whisper input language | `#whisper-input-language-select`; grouped with AI `#language-select` and `#language-pinned-toggle` | `dom.whisperInputLanguageSelect`; AI refinement wiring mirrors the same settings | `language_mode` `auto`, `language_pinned` `false` | **Basic** owner | One logical language setting with duplicate visible surfaces. A1 owns the visible input-language row; A2 owns derived post-processing language. Language changes can refresh prompts or ask to discard shared prompt state. |
| Whisper model choice | Dynamic `#model-list` Apply buttons | `src/models.ts`, model renderer | `model` `whisper-large-v3-turbo` | **Basic** value; **Expert** action | Active model is a user job. Download, delete/remove, quantize, and optimize are separate Expert actions with existing confirmation/progress. |
| Whisper source and storage | `#model-source-select`, `#model-custom-url`, `#model-storage-path` | `src/settings/transcription.settings.ts`, transcription wiring | `model_source` `default`; `model_custom_url` `""`; `model_storage_dir` `""` | **Expert** | Refresh, browse, reset, and backend CUDA/Vulkan actions stay separate. |
| Overlay and hotkeys | `#overlay-style`, `#overlay-color`, style-specific `#overlay-*` ranges, product/TTS hotkeys | `src/settings/overlay.settings.ts`, `src/hotkeys.ts`, overlay wiring | `overlay_style` `dot`; color, radius, rise/fall, opacity, position, TTS stop fields use Rust defaults | **Expert** | Dot/KITT and TTS/module gates remain. Apply, GPU purge, health, and telemetry are actions/status, not values. |
| History aliases and local display | `#history-alias-mic-input`, `#history-alias-system-input`, `#conversation-font-size` | `src/wiring/history.wire.ts`, `src/history-preferences.ts` | `history_alias_mic` `Input`; `history_alias_system` `System audio`; font size localStorage `historyFontSize_<tab>` `14` | **Basic** | Aliases persist through settings plus a localStorage shadow. Font size is a local UI preference, not `settings.json`. |
| History toolbar actions | `#history-tab-*`, `#history-copy-conversation`, `#history-delete-conversation`, `#open-recordings-btn`, `#archive-browse-btn`, `#history-export`, search/clear, dynamic row buttons | history and app-chrome wiring | No persisted profile field | **Basic actions / UI-only** | Keep copy, delete confirmation, open-folder, archive, export, search, and per-entry actions. Do not turn tabs or search into settings values. |

### A2 — AI refinement, post-processing, language, and vocabulary

| Ownership unit | Actual control ids or dynamic surface | TypeScript binding | Rust path / JSON key / default | Proposed surface | Runtime, action, and dependency boundary |
|---|---|---|---|---|---|
| Rule processing switch | `#postproc-enabled` | `dom.postprocEnabled`, AI refinement wiring | `postproc_enabled` `false` | **Basic** | Gates `process_transcript` in `src-tauri/src/audio.rs` and `transcription.rs`. |
| AI refinement switch | `#ai-fallback-enabled` | AI refinement wiring; also syncs module membership | `ai_fallback.enabled` / `ai_fallback.enabled` `false`; legacy `postproc_llm_enabled` mirror `false` | **Basic** | Module `ai_refinement` availability wins. Keep the legacy mirror synchronized; do not expose it as a second row. |
| Refinement pipeline status | `#refinement-pipeline-note`, live node, graph nodes/edges | `renderRefinementPipelineNote`, `syncRefinementPipelineGraphFromSettings` | UI/status only | **Basic status** | Keep status separate from the two switches. |
| Prompt intent and preset | Dynamic `#prompt-preset-list`, built-ins and `user:<id>` chips | `renderPromptPresetCards`, `syncActivePromptPresetSelection` | `active_prompt_preset_id` `wording`; `prompt_profile` `wording`; `prompt_presets` `[]`; `prompt_preset_overrides` `{}` | **Basic** | Chip selection is a value; new/delete chips are actions. Built-ins are wording, summary, technical specs, action items, and LLM Prompt. |
| Prompt editor | `#ai-fallback-preset-name-field`, `#ai-fallback-custom-prompt` | AI refinement settings/wiring; Save/Reset/Revert/Discard/Delete handlers | `custom_prompt` wording fallback; `custom_prompt_enabled` `false`; `use_default_prompt` `true` before normalization; user preset names/prompts | **Basic** | Keep name/text values separate from Save, Reset, Revert, Discard, and Delete actions. Empty custom text falls back to the built-in prompt in Rust. |
| Active local model | Dynamic `#ollama-model-manager`, Available/Your Models cards | `src/ollama-models.ts`; Activate updates model and preferred model | `ai_fallback.model` empty until discovered; `providers.ollama.preferred_model` empty; `available_models` `[]` | **Basic** value | Active model selection is Basic. Download, import, delete/uninstall, and model inventory progress are Expert actions; delete remains confirmation-gated. |
| Rule detail tuning | `#postproc-punctuation`, `#postproc-capitalization`, `#postproc-numbers` | AI refinement wiring | `postproc_*_enabled` all `true` | **Expert** | Visible only when rule processing is enabled. `#postproc-language-derived` is derived status (`postproc_language`), not an editable value. |
| Local provider and runtime | `#ai-fallback-local-backend-select`, runtime status/action/import, version/source/endpoints and Verify/Refresh | AI refinement settings/wiring | `ai_fallback.provider` `ollama`; `execution_mode` `local_primary`; `providers.ollama.runtime_target_version` `0.20.2`; source `manual` in Rust; fallback endpoints `[]` | **Expert** values/actions except primary setup action | Provider/module/runtime availability gates all descendants. Split version select from fetch action and status. The Rust `manual` source default has no matching current select option and needs a later compatibility decision. |
| Compatibility server | `#ai-fallback-compat-endpoint`, API key, dynamic model list, Verify/Install actions | AI refinement settings/wiring | `providers.lm_studio.endpoint` `http://127.0.0.1:1234`; `.oobabooga.endpoint` `http://127.0.0.1:5000`; API keys empty; preferred models dynamic | **Expert** | Visible only for the selected compatible provider. API keys stay values in settings and never enter visibility export. |
| Roadmap provider lane | `#ai-fallback-online-lane`, generated cloud rows | AI refinement settings/wiring | `fallback_provider` `null`; provider auth/model records | **Config/Pending** | Rows are currently disabled/read-only roadmap UI. Do not count them as active editable controls or remove the lane without a product decision. |
| Quality tuning | `#ai-fallback-temperature`, `#ai-fallback-preserve-language`, `#ai-fallback-low-latency-mode`, `#ai-fallback-max-tokens` | AI refinement wiring | `temperature` `0.3`; `preserve_source_language` `true`; `low_latency_mode` `false`; `max_tokens` `4000` | **Expert** | Low latency clamps temperature/tokens. Preserve guard applies to built-in and custom profiles; `llm_prompt` remains exempt. Notes are status copy only. |
| Custom vocabulary rows | `#postproc-custom-vocab-enabled`, dynamic `.vocab-row`, `#postproc-vocab-add` | `src/settings/vocabulary.settings.ts`, AI wiring | `postproc_custom_vocab_enabled` `false`; `postproc_custom_vocab` `{}` | **Expert** | Each row splits Original value, Replacement value, and Remove action. Config is shown only when enabled. |
| Learned and topic vocabulary | `#vocab-terms-list`, pending substitutions, dynamic `#topic-keywords-list`, Reset | `vocabulary.settings.ts`, `vocab-auto-learn.ts`, AI refinement settings/wiring | `vocab_terms` `[]`; `edit_substitutions` `[]`; `topic_keywords` default topic map | **Expert** data/action | Learned chips and pending corrections are user state/status; dismiss/reset are actions. Topic inputs are editable tuning values. |

### A3 — Voice, Task Capture, Video, Assistant, and Modules

| Ownership unit | Actual control ids or dynamic surface | TypeScript binding | Rust path / JSON key / default | Proposed surface | Runtime, action, and dependency boundary |
|---|---|---|---|---|---|
| Voice provider and voice choice | Voice Output provider/voice selectors, auto-language, policy, device | `src/settings/voice-output.settings.ts`, `src/wiring/voice-output.wire.ts` | `voice_output_settings.default_provider` `windows_native`; `voice_id_windows` empty; fallback provider/voice; `auto_voice_by_detected_language` `false`; policy `agent_replies_only`; device `default` | **Expert** | Tab/console requires `output_voice_tts`. Provider-dependent voice fields remain grouped with runtime gates. Test-provider is an action. |
| Voice tuning and providers | Rate, volume, Piper gain/path/model/dir, Qwen endpoint/model/voice/key/timeout | Voice output settings/wire | `rate` `1.0`; `volume` `1.0`; Piper gain `-12`; paths empty; Qwen endpoint `http://127.0.0.1:8000/v1/audio/speech`, model `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice`, voice `vivian`, key empty, timeout `45` | **Expert** | API key is secret-bearing and must never appear in visibility export or diagnostics. Piper/Qwen rows are provider-dependent. |
| Task routes | Dynamic route editor with add/remove/save/test | `src/task-capture-config.ts` | `task_capture_settings.routes[]`; normalized default Agenda route; route label/page/endpoint/keywords | **Expert** | `renderTaskCaptureTab()` rebuilds nodes. Route values, Add/Remove/Test/Save actions, and endpoint status are separate. Consumer path is `src-tauri/src/modules/task_capture.rs`. |
| Task matching and AI refinement | Match mode plus dependent AI toggle/prompt | Task Capture settings/wire | `match_mode` `contains`; `ai_refinement_enabled` `true`; `refinement_prompt` nonempty German formatter | **Expert** | Prompt field is visible only while AI refinement is enabled. Module availability gates the whole tab. |
| Video request controls | `#video-style-select`, resolution, FPS, TTS, brief | `src/video-generation.ts` | Session-only `VideoJobRequest`; fallbacks slideshow/1920x1080/30/off/empty | **Expert** | These controls do not load/save `video_generation_settings`; keep session scope until a separate persistence decision. Drop, queue, generate, history, and output-folder controls are actions/status. |
| Assistant configuration | Hands-free, wakewords, aliases, confirm timeout, suggestion level, reply mode, online mode, voice feedback | `src/workflow-agent-console.ts` and app wiring | `workflow_agent.*`; defaults false, standard wakewords, timeout `45`, max candidates `3`, `rule_only`, online false, voice feedback false | **Expert** | Assistant Core plus `workflow_agent.enabled` gates console. Global online mode and assistant online setting share `workflow_agent.online_enabled`; use one managed value. Plan/review/execute controls are actions. |
| Managed modules | Dynamic module rows and health/permission state | `src/modules-hub.ts` and module wiring | `module_settings.enabled_modules`, `consented_permissions`, `module_overrides` | **Expert** | Enable/disable/install/download/update/health/restart are actions, not one generic value. Preserve permission gates and confirmations. Dynamic rows need post-render metadata. |

## Metadata and profile integration snapshot

B1’s current metadata vocabulary is:

| Metadata | Meaning |
|---|---|
| `data-settings-visibility-managed="true"` | Marks the ownership unit controlled by the editor. |
| `data-settings-visibility-key` | Stable logical key; buttons without a value use `action.<id>`. |
| `data-settings-visibility-label` | Optional explicit editor label; otherwise derived from field label/id. |
| `data-settings-visibility-default="basic|expert"` | Proposed default surface. |
| `data-settings-visibility-group` | Editor grouping label. |
| `data-settings-visibility-tab` / `data-settings-visibility-tab-content` | Tab discovery and Basic-tab availability marker. |
| `data-settings-visibility-group-container` | Optional ancestor group reference. |

`src/settings-visibility.ts` currently groups a control with its `.field`, `.hotkey-field`, `.history-alias-field`, or `.font-controls` wrapper and auto-marks otherwise-unannotated inputs/buttons in managed scopes as Expert. It dispatches refresh events for dynamic renderers. Static annotations already exist for the core Capture, Whisper input language, AI prompt units, and A3 static units; dynamic model, vocabulary, Task Capture, and Modules rows require render-time coverage. This is a preliminary snapshot until B1 tests verify the final entry set.

The profile implementation uses localStorage key `trispr-settings-visibility-v1`, profile version `1`, and export filename `trispr-settings-visibility.json`. It stores only `overrides` and `removalCandidates` reasons. Export adds known `defaults` but never includes settings values, prompts, model contents, API keys, or secrets. Valid unknown saved keys are retained in local storage so a lazy or dynamic control can reappear later; they are ignored by the current render until a matching entry exists, and export filters them to currently known definitions.

Mixed groups are expected. A group may contain Basic values, Expert tuning, status, and actions. The editor must let a field or logical group override its profile without merging action/status into a value row. Runtime and ancestor gates remain authoritative; a Basic override cannot reveal a disabled module, unavailable provider, mode-hidden child, or runtime-hidden element.

## Current user workflow

1. Open **Customize view** from the existing Expert settings surface.
2. Set Basic or Expert for a field, or apply a group choice. **Done** saves the local view profile; **Cancel** discards the draft.
3. **Restore defaults** clears visibility overrides while retaining removal-candidate reasons. **Export saved view** exports metadata only.
4. Switching the global mode hides editor-owned controls in Basic mode but retains the open draft. Returning to Expert resumes it. Dynamic controls may arrive later; valid unknown profile keys remain stored until their definition mounts.

## Explicit exclusions and unresolved findings

- Runtime meters, health notes, progress, graph edges, labels, GPU telemetry, and derived `postproc_language` are UI-only/status entries.
- Download, delete/remove, optimize, purge, apply, install, Verify, Refresh, fetch-version, open-folder, history delete, route add/remove/test/save, and module lifecycle controls are actions. Keep their existing confirmation and availability behavior.
- AI legacy mirrors (`postproc_llm_enabled`, `postproc_llm_provider`, `postproc_llm_model`, `postproc_llm_prompt`) have live compatibility consumers and require migration/round-trip evidence before any removal. `postproc_llm_api_key` is `#[serde(skip_serializing)]` and is not a JSON setting.
- `postproc_language` is derived from A1 ASR language and is not an A2 edit row. `strict_local_mode`, adaptive tiers, provider auth metadata, and runtime health/path fields are backend-only or Config candidates until an owner is identified.
- `video_generation_settings.*` exists in Rust while the current Video UI uses session-only `VideoJobRequest` values. Do not silently make Video persistent in the visibility work.
- Rust `providers.ollama.runtime_source="manual"` is not among the current source select options. Preserve the value on load and resolve the UI/default mismatch in a separate compatibility change.
- Input-language duplicates and `opus_enabled` duplicates are grouped logical units; they are not independent removal candidates.
- No current A1/A2/A3 evidence supports a Kill decision. Feature pruning is deferred to the review list, where the owner can choose Keep, Review, Config/Pending, or a later migration batch.

## Validation handoff

Current static/editor evidence:

- B1 static inventory: **204 non-dialog controls**, **166 managed units**, **24 Basic**, **142 Expert**, **39 global/navigation exclusions**; hotkey input/record pairs are shared units and there are no duplicate keys.
- B2 focused visibility tests: **13/13 passed**.
- B3 AI settings wording tests: **72/72 passed**.
- B3 Rust composed provider boundary test: **1 passed**; language-guard predicate tests: **2 passed**.
- Last full TypeScript suite: **663/664 passed**; one shared B1 legacy expert-mode expectation remains pending. Full build still awaits the shared B1 type fix. Do not mark the cleanup fully green until parent QA supplies final numbers.

The focused checks above do not replace the final full repository, browser, dynamic-row, and mode-toggle gates.

## Source reports

- `C:\Users\trist\Documents\Codex\2026-09-10\battle-plan-liegt-in-settings-cleanup\work\inventory-a1.md` — Capture, System Audio, Whisper Runtime, Recording, Overlay & Hotkeys, Text Output.
- `C:\Users\trist\Documents\Codex\2026-09-10\battle-plan-liegt-in-settings-cleanup\work\inventory-a2.md` — AI Refinement, post-processing, language/vocabulary, and P0 guard boundaries.
- `C:\Users\trist\Documents\Codex\2026-09-10\battle-plan-liegt-in-settings-cleanup\work\inventory-a3.md` — Voice Output, Task Capture, Video, Assistant, and Modules.
- [`docs/plans/settings-cleanup-battle-plan.md`](settings-cleanup-battle-plan.md) — approved workflow and implementation gates.
