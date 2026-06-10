# AI Refinement Runtime Observations Checkpoint

Status: active
Created: 2026-06-10
Updated: 2026-06-10
Slug: ai-refinement-runtime-observations
Supersedes: none

## Scope

This checkpoint captures observed and verified AI Refinement runtime, model-selection, debug-surface, and UX/terminology issues from a hands-on v0.8.2/v0.8.3 local session.

It does not decide the final AI Refinement architecture, redesign the model picker, change Ollama packaging, or create an ADR. It records the loaded-model availability fix, the first debug-surface patch, and the version-specific Ollama CPU/GPU diagnosis.

## Sources Read

- [CONTEXT.md](../../../CONTEXT.md)
- [ROADMAP.md](../../../ROADMAP.md)
- [STATUS.md](../../../STATUS.md)
- [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md)
- [docs/MODEL_MANAGER_BACKEND_CARDS.md](../../../docs/MODEL_MANAGER_BACKEND_CARDS.md)
- [project-spec/README.md](../../README.md)
- [project-spec/checkpoints/README.md](../README.md)
- [project-spec/checkpoints/2026-06-10/release-clean-setup-health.md](release-clean-setup-health.md)
- [project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md](vulkan-whisper-fix.md)
- [project-spec/decisions/2026-05-25/trispr-flow-modularization.md](../../decisions/2026-05-25/trispr-flow-modularization.md)
- [project-spec/decisions/2026-06-08/installable-module-package-model.md](../../decisions/2026-06-08/installable-module-package-model.md)
- [project-spec/decisions/2026-06-07/cargo-feature-module-boundary.md](../../decisions/2026-06-07/cargo-feature-module-boundary.md)
- [project-spec/decisions/2026-05-19/settings-decomposition.md](../../decisions/2026-05-19/settings-decomposition.md)
- Session observations from the user during a live Trispr Flow AI Refinement session, including screenshots of the AI Refinement tab, AI Runtime panel, Models section, and Assistant Debug tab.

## Current Facts

- AI Refinement is an optional Feature Module and user-facing name for module ID `ai_refinement`; the internal code name is `ai_fallback`. Source: [CONTEXT.md](../../../CONTEXT.md).
- AI Refinement owns provider selection, prompt/profile logic, LLM calls, fallback-chain behavior, and local AI runtime management such as Ollama or LM Studio when those runtimes serve refinement. Source: [CONTEXT.md](../../../CONTEXT.md).
- The active roadmap already has Block C, `Adaptive AI Refinement`, covering VRAM probing, quant matrix polish, keep-alive, and vocabulary ground-truth. Source: [ROADMAP.md](../../../ROADMAP.md).
- Recent historical status says AI Refinement became a toggleable `ai_refinement` module, re-enable autostarts managed Ollama, and the defer policy requires runtime-ready state with deterministic runtime-not-ready fallback reasons. Source: [STATUS.md](../../../STATUS.md).
- Release-health taxonomy distinguishes release blockers from enabled optional modules that need setup or run through fallback. Source: [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md).
- Backend-aware model-card design already names GPU type, VRAM, CPU fallback, and hardware recommendation as model-card concerns, but it is a design reference rather than implemented runtime behavior. Source: [docs/MODEL_MANAGER_BACKEND_CARDS.md](../../../docs/MODEL_MANAGER_BACKEND_CARDS.md).
- The modularization candidate list records that AI Refinement should eventually include Ollama runtime and model ownership, but this is a candidate/history item, not an accepted refactor decision. Source: [project-spec/decisions/2026-05-25/trispr-flow-modularization.md](../../decisions/2026-05-25/trispr-flow-modularization.md).
- Accepted module-package and cargo-feature decisions are relevant background for longer-term module boundaries, but they do not decide this runtime gate bug. Sources: [project-spec/decisions/2026-06-08/installable-module-package-model.md](../../decisions/2026-06-08/installable-module-package-model.md), [project-spec/decisions/2026-06-07/cargo-feature-module-boundary.md](../../decisions/2026-06-07/cargo-feature-module-boundary.md).
- Frontend AI Refinement settings belong to the `ai-refinement` settings slice after the settings decomposition work. Source: [project-spec/decisions/2026-05-19/settings-decomposition.md](../../decisions/2026-05-19/settings-decomposition.md).
- User-observed UI state showed Ollama local runtime running from `per_user_zip` with version `0.20.2` and managed PID, while the install-version control initially exposed `0.30.7` with confusing mixed badges such as active/not installable/online. Source: session observation; not yet verified in code.
- User later observed the install-version control showing `0.20.2` as installed/recommended/installable/pinned while runtime remained ready. Source: session observation; not yet verified in code.
- User installed or reinstalled `Qwen 3.5 4B Standard` from the model card; download progress was visible and perceived as working well. Source: session observation; not yet verified in code.
- User-observed AI Refinement pipeline later showed `AI Refiner` active with active model `qwen3.5:4b` and `Primary output: Ollama AI refinement`; rule-based refinement remained active as fallback. Source: session observation.
- With prompt style `LLM Prompt Engineer`, the prompt preview instructed English output, but both hold-to-record and toggle recording still inserted German output. Runtime/code evidence below explains this as no normal-dictation AI generation when the gate treats CPU-loaded Ollama as not resident.
- The user observed no meaningful output difference after enabling AI Refinement, despite pipeline animation through `Rec -> Whisper -> Postproc -> Agent -> Paste`. Runtime/code evidence below explains this as the same loaded-model gate issue unless later UI reproduction disproves it.
- The Assistant Debug tab showed assistant command state such as last heard, last intent, latest reply, and last gate reason, but did not expose raw Whisper output, rule-based output, AI prompt, AI refined output, final inserted text, or Deferred Insert Gate decision for normal dictation. Source: session observation; not yet verified in code.
- The `Import model` button opened a file picker, but the user had no clear path to a Qwen model file and likely did not need manual import for a model installed through model cards. Source: session observation; not yet verified in code.
- The model management UI used terminology like activating another model to uninstall this one. The user found this confusing because uninstall implies deleting local files, while the immediate intent was deactivation or selection change. Source: session observation; not yet verified in code.
- The user still saw roughly 4 GB VRAM available/unchanged and questioned whether the model was actually loaded into GPU memory. Source: session observation; not yet verified with Ollama, GPU telemetry, or code.
- Hold-to-record appeared to produce better transcription than toggle mode, where intermediate output seemed poorer or less refined. Source: session observation; not yet verified in code.
- The vocabulary learning claim is uncertain. User reported that Ingo said Trispr Flow tracks how inserted text is edited to learn vocabulary, but the user questioned whether edits after focus changes or delayed cursor activity can be attributed reliably. Source: session observation; not yet verified in code.
- Working state model for the next implementation pass: `installed` means active model appears in `/api/tags`; `loaded_cpu` means active model appears in `/api/ps` and reports CPU/`size_vram: 0`; `loaded_gpu` means active model appears in `/api/ps` with GPU/`size_vram > 0`; `loaded_mixed` means CPU+GPU/offload is reported; `cold` means installed but absent from `/api/ps`; `missing` means absent from `/api/tags`; `unknown` means endpoint or response could not be classified.
- The loaded-model availability fix is implemented in `src-tauri/src/ai_fallback/provider.rs` and `src-tauri/src/audio.rs`: `/api/ps` active-model presence now means loaded even when `size_vram` is zero; `size_vram` remains GPU/offload telemetry.
- The normal dictation result event now carries a compact `refinement_gate` object, and the frontend Refinement Inspector shows the gate verdict in metadata. This exposes whether the result was set to refine, deferred, skipped, whether Ollama reported the active model loaded, and the reported VRAM bytes.
- In the current dev session, local settings had AI Refinement re-enabled with provider `ollama`, model `qwen3.5:4b`, execution mode `local_primary`, and prompt profile `wording`. This means the specific `LLM Prompt Engineer` English-output repro still requires switching the prompt profile back to `llm_prompt` in UI/settings before testing that symptom.
- The current managed runtime selected by the app is bundled Ollama `0.30.7` at `C:\Users\hendr\AppData\Local\Trispr Flow\ollama-runtime\0.30.7\ollama.exe`, not `0.20.2`.
- A runtime UI mismatch was found after the GPU diagnosis: persisted settings had `runtime_path` and `runtime_version` set to `0.30.7`, but `runtime_target_version` still set to the stale default `0.20.2`. The UI used `runtime_target_version` as the selected display target, so it could show `0.20.2` even when the configured/installed runtime was `0.30.7`.
- The runtime UI normalization now aligns the target version with the installed runtime version when the target is empty or still the stale default. This keeps the runtime picker/display from implying `0.20.2` while Trispr is configured for `0.30.7`.
- With Ollama `0.30.7`, an app-shaped `/api/chat` request for `qwen3.5:4b` returned English in about 6.2 seconds. `/api/ps` then reported `size_vram: 3145098853`, and `ollama ps` reported `PROCESSOR 100% GPU`.
- With isolated Ollama `0.20.2` on port `11435`, the same app-shaped request returned in about 6.8 seconds, but `/api/ps` reported `size_vram: 0`, and `ollama ps` reported `PROCESSOR 100% CPU`.
- The `0.20.2` debug startup config reported `OLLAMA_VULKAN:false` and `OLLAMA_LIBRARY_PATH` containing the `cuda_v12` runner path. During model load it logged `offloaded 0/33 layers to GPU`, CPU model weights/cache/graph, and runner VRAM `0 B`. That explains the CPU-only state for `0.20.2` on the AMD Radeon RX 6800 XT.

## Decisions

- No ADR was created. The session captured observations and a debug handoff only; no hard-to-reverse architecture decision was made.
- The checkpoint should live in the Trispr Flow repository, not the Obsidian vault repository. This was corrected before writing any checkpoint to the vault project spec.
- A separate branch, `docs/ai-refinement-debug-checkpoint`, was created for this observation checkpoint to avoid adding exploratory/debug documentation directly on the release branch.
- Work proceeded sequentially: checkpoint cleanup first, Trispr loaded-model availability semantics second, debug-surface patch third, Ollama CPU/GPU runtime diagnosis fourth.
- Availability and GPU usage are separate concepts. Model presence in `/api/ps` should mean loaded/available; `size_vram > 0` should be GPU/offload telemetry, not the only availability gate.
- Commit authority is not granted by this checkpoint. Future agents should fix but not commit unless the user explicitly authorizes a commit.

## Progress

- Confirmed the initial workspace was the Obsidian vault, not the Trispr Flow repository.
- Located the Trispr Flow repository at `e:\code\ingo\Trispr_Flow`.
- Checked Trispr repo status before writing. The base branch was `release/v0.8.3-vulkan-whisper-hotfix`, with an existing unrelated modification in `src/styles.css`.
- Created branch `docs/ai-refinement-debug-checkpoint` from the current Trispr Flow worktree.
- Read Trispr domain context, roadmap, status, and existing checkpoint index.
- Created this checkpoint to preserve the debug observations and next-agent kickoff.
- Investigated runtime and code path on branch `docs/ai-refinement-debug-checkpoint`; no source-code fix was applied and no commit was made.
- Verified local Ollama `/api/tags` reports `qwen3.5:4b` installed as a GGUF Q4_K_M `qwen35` 4.7B model.
- Verified local Ollama `/api/ps` reports `qwen3.5:4b` loaded with `context_length: 2048` but `size_vram: 0`, so the model is resident in Ollama but not reported as using VRAM.
- Verified the bundled Ollama CLI reports `qwen3.5:4b` as `PROCESSOR 100% CPU`, `SIZE 5.7 GB`, `CONTEXT 2048`, `UNTIL Forever`. This confirms the model is loaded, but currently running on CPU/system RAM rather than GPU VRAM.
- Verified the managed Ollama runtime is `C:\Users\hendr\AppData\Local\Trispr Flow\ollama-runtime\0.20.2\ollama.exe serve`. The server is launched by Trispr with `OLLAMA_HOST`, `OLLAMA_NO_CLOUD=1`, `OLLAMA_KEEP_ALIVE`, optional `OLLAMA_RUNNERS_DIR`, and runtime PATH additions; Trispr does not set `OLLAMA_NUM_GPU` globally.
- Verified Windows reports `AMD Radeon RX 6800 XT`, and the bundled runtime contains both `lib\ollama\vulkan` and CUDA runner assets, but the live Ollama runner still selected `100% CPU` for `qwen3.5:4b`.
- A direct `/api/generate` English rewrite request with no app-style output cap timed out after 120 seconds, showing the current runtime/model path can be too slow for normal refinement budgets.
- An app-shaped `/api/chat` request using `think: false`, `num_ctx: 2048`, `num_thread: 4`, and `num_predict: 192` returned English output in about 12.9 seconds, proving the model and LLM Prompt Engineer instructions can produce English when called directly.
- Traced normal dictation in `src-tauri/src/audio.rs`: before emitting `transcription:result`, the code calls `fetch_ollama_running_vram()` and sets `should_refine = refinement_enabled && model_resident`; for Ollama, `model_resident` is `bytes > 0`. With observed `size_vram: 0`, normal dictation emits raw/postprocessed text with `paste_deferred=false` and does not spawn AI refinement.
- Traced prompt-style propagation: frontend selection normalizes `active_prompt_preset_id` into `ai_fallback.prompt_profile`; backend `prepare_refinement()` copies `ai.prompt_profile` into `RefinementOptions`; `provider::prompt_for_profile()` selects the `llm_prompt` prompt and intentionally skips source-language lock for that profile.
- Traced deferred paste selection: frontend queues raw text immediately when `paste_deferred` is false, queues refined text only when a matching `transcription:refined` event arrives before the pending deferred paste settles, and falls back to raw on timeout or failure.
- Inspected model-card terminology after the execution path was understood. Installed model cards use `Activate`, `Uninstall`, and `Activate another model to uninstall this one`; the import action title says `Import a local GGUF or Modelfile into Ollama`.
- Cleaned this checkpoint and index for sequential handoff.
- Implemented loaded-model availability semantics: added deterministic `/api/ps` parser tests, replaced the normal dictation gate so the active model appearing in `/api/ps` is treated as loaded even when `size_vram: 0`, and logged loaded status separately from VRAM bytes.
- Added `refinement_gate` telemetry to normal transcription result events and surfaced it in the Refinement Inspector metadata.
- Re-enabled AI Refinement was observed in settings, but the active prompt profile was `wording`, not `llm_prompt` / `LLM Prompt Engineer`.
- Verified current managed Ollama `0.30.7` runs `qwen3.5:4b` on GPU: `/api/ps` reported `size_vram: 3145098853`, and bundled `ollama ps` reported `PROCESSOR 100% GPU`.
- Reproduced old bundled Ollama `0.20.2` CPU-only behavior on alternate port `11435`: `/api/ps` reported `size_vram: 0`, bundled `ollama ps` reported `PROCESSOR 100% CPU`, and debug logs showed Vulkan disabled plus zero GPU offload.
- Fixed the runtime version display mismatch in `src/settings/ai-refinement.settings.ts` and `src/wiring/ai-refinement.wire.ts` so an installed/configured `0.30.7` runtime is not masked by stale `runtime_target_version: 0.20.2`.

## Stale Or Conflicting Context

- [STATUS.md](../../../STATUS.md) reports AI Refinement re-enable autostart and deterministic runtime-not-ready fallback reasons, but the observed UI did not make the final refinement decision visible. This may be an instrumentation/debug-surface gap rather than a contradiction.
- [ROADMAP.md](../../../ROADMAP.md) already plans VRAM probing and AI Refinement model-picker polish under Block C. The current observations strengthen that need but do not change the roadmap order by themselves.
- [STATUS.md](../../../STATUS.md) may need a small update after broader verification, because runtime-ready/defer semantics now distinguish loaded CPU models from GPU-resident models.
- This checkpoint now explains why bundled Ollama `0.20.2` selected `100% CPU` on the AMD Radeon RX 6800 XT in the reproduced diagnostic run: `OLLAMA_VULKAN:false`, CUDA runner path selection, and `offloaded 0/33 layers to GPU`. Current `0.30.7` did not reproduce the CPU-only behavior and selected `100% GPU`.
- The existing local `src/styles.css` modification predates this checkpoint branch and was not inspected or modified for this checkpoint.

## Open Questions

- What does the red/white bottom icon represent: runtime health, model readiness, recording state, AI Refinement availability, or stale UI state?
- Should Trispr keep `0.30.7` as the pinned/recommended managed Ollama runtime for AMD GPU users, since it selected `100% GPU` where `0.20.2` selected CPU?
- Should the runtime UI warn when the installed runtime is old enough that Vulkan is disabled or unavailable for AMD GPUs?
- Should CPU-loaded AI Refinement be classified as `fallback_active`, `needs_setup`, or a local warning in module health after the loaded-model gate is fixed?
- Should `Import model` be hidden or clarified when model-card installation is the normal path?
- Should model-card copy distinguish `Activate`, `Deactivate`, `Remove local files`, and `Uninstall`?
- Does vocabulary learning track only immediate post-paste diffs, or can it safely attribute delayed edits after focus/window/cursor changes?
- Is toggle mode feeding intermediate or unstable text into post-processing/refinement differently from hold-to-record?

## Next Actions

1. Reproduce with a deterministic spoken or pasted test case where `LLM Prompt Engineer` should visibly produce English. First switch the prompt profile from `wording` to `llm_prompt`; then record whether final inserted output is refined or fallback after the loaded-model fix.
2. Decide whether `STATUS.md`, module health taxonomy, runtime version policy, or an ADR needs updating. Run the ADR gate before adding any new decision file.
3. Consider whether `0.30.7` should become the pinned/recommended managed runtime in the install catalog, with checked URL/hash, since it selected GPU where `0.20.2` selected CPU on this AMD machine.
4. Consider a runtime-health warning or model-card hint for CPU-loaded Ollama models: CPU-loaded is available but may be slow; GPU-loaded is the healthy performance path.
5. Later, inspect model-card UI strings/actions for `Import model`, active model handling, uninstall/remove/deactivate terminology, and missing/installed state.
6. Later, inspect vocabulary learning attribution and toggle-vs-hold behavior.

## Kickoff Prompt

Use checkpoint-kickoff. Continue from `project-spec/checkpoints/2026-06-10/ai-refinement-runtime-observations.md`: the loaded-model availability fix, gate-inspector patch, and Ollama CPU/GPU diagnostic are implemented/recorded but not committed. Verify the current diff and tests. Next, switch the prompt profile to `llm_prompt` / `LLM Prompt Engineer` and perform a normal dictation/UI repro to confirm the final inserted output is refined English rather than raw German. Use `/grill-with-docs` for checkpoint, ADR, domain, release-policy, stale-doc, or cross-file design uncertainty; use `/grill-me` for pure implementation/debug uncertainty; if unsure whether uncertainty matters, grill. Commit authority: fix but do not commit unless explicitly authorized.

## Verification

Passed:

- Confirmed Trispr Flow repo location and branch context before writing.
- Created a separate branch for this checkpoint.
- Read [CONTEXT.md](../../../CONTEXT.md), [ROADMAP.md](../../../ROADMAP.md), [STATUS.md](../../../STATUS.md), and the checkpoint index.
- Read [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md), [docs/MODEL_MANAGER_BACKEND_CARDS.md](../../../docs/MODEL_MANAGER_BACKEND_CARDS.md), and relevant project-spec decisions for module and settings context.
- ADR creation gate was evaluated. No ADR was created because this is an observation/debug handoff, not a durable architecture decision.
- Re-verified branch `docs/ai-refinement-debug-checkpoint` and dirty worktree state before investigation; pre-existing `src/styles.css` was not touched.
- Ran local Ollama endpoint checks against `http://127.0.0.1:11434`: `/api/tags`, `/api/ps`, direct `/api/generate`, and app-shaped `/api/chat`.
- Ran the bundled Ollama CLI directly for `--version` and `ps`; confirmed version `0.20.2` and `qwen3.5:4b` running on `100% CPU`.
- Traced prompt-style propagation, model selection, Ollama request construction, AI refinement spawning, deferred paste handling, and model-management wording in source.
- Checkpoint was cleaned for sequential work: loaded-model availability fix first, GPU runtime diagnosis second, UX/health terminology later.
- Implemented the loaded-model availability fix in `src-tauri/src/ai_fallback/provider.rs` and `src-tauri/src/audio.rs`.
- Ran `cargo test --manifest-path src-tauri/Cargo.toml ai_fallback::provider::tests::ollama_ps_ --lib`; passed 4 tests, 0 failed.
- Checked editor diagnostics for changed source/checkpoint files; no errors found.
- Added gate telemetry in `src-tauri/src/transcription.rs`, `src-tauri/src/audio.rs`, `src/types.ts`, and `src/refinement-inspector.ts`.
- Ran `npm run build`; passed. Vite reported the existing dynamic/static import chunking warning for `src/settings/vocabulary.settings.ts`.
- Observed AI Refinement re-enabled in settings with prompt profile `wording`.
- Verified current managed Ollama `0.30.7` uses GPU for `qwen3.5:4b`: `/api/ps size_vram=3145098853`, `ollama ps PROCESSOR 100% GPU`.
- Verified isolated bundled Ollama `0.20.2` uses CPU for `qwen3.5:4b`: `/api/ps size_vram=0`, `ollama ps PROCESSOR 100% CPU`, debug log `offloaded 0/33 layers to GPU`.
- Verified current settings had `runtime_path/runtime_version=0.30.7` and stale `runtime_target_version=0.20.2`; patched frontend normalization so installed runtime version wins over the stale default target.
- Ran `npm test -- --run src/__tests__/ai-refinement-settings.test.ts src/__tests__/ai-refinement-wire.test.ts`; passed 94 tests.

Not checked:

- No full Rust test suite was run.
- No app UI reproduction was performed with `LLM Prompt Engineer`; prompt profile was still `wording` when AI Refinement was re-enabled.
