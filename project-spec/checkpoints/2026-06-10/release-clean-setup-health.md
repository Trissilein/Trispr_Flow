# Release-Clean Setup Health Checkpoint

Status: active
Created: 2026-06-10
Updated: 2026-06-10
Slug: release-clean-setup-health
Supersedes: none

## Scope

This checkpoint covers the documentation refresh and first implementation slice for making v0.8.3 release-clean by separating Trispr Core and Vulkan Whisper release blockers from optional Feature Module setup gaps.

It does not rewrite the module lifecycle state machine, redesign the Modules Hub, add deep AI Refinement/Ollama runtime readiness checks, add a dedicated release-gate script hook, or decide the durable source for the verified Vulkan runtime payload.

## Sources Read

- [CONTEXT.md](../../../CONTEXT.md)
- [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md)
- [project-spec/README.md](../../README.md)
- [project-spec/checkpoints/README.md](../README.md)
- [project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md](vulkan-whisper-fix.md)
- [project-spec/decisions/2026-06-08/installable-module-package-model.md](../../decisions/2026-06-08/installable-module-package-model.md)
- [project-spec/decisions/2026-06-07/cargo-feature-module-boundary.md](../../decisions/2026-06-07/cargo-feature-module-boundary.md)
- [project-spec/decisions/2026-05-25/trispr-flow-modularization.md](../../decisions/2026-05-25/trispr-flow-modularization.md)
- [src-tauri/src/modules/health.rs](../../../src-tauri/src/modules/health.rs)
- [src-tauri/src/modules/registry.rs](../../../src-tauri/src/modules/registry.rs)
- [src-tauri/src/modules/lifecycle.rs](../../../src-tauri/src/modules/lifecycle.rs)
- [src-tauri/src/modules/mod.rs](../../../src-tauri/src/modules/mod.rs)
- [src-tauri/src/runtime_commands.rs](../../../src-tauri/src/runtime_commands.rs)
- [src-tauri/src/state.rs](../../../src-tauri/src/state.rs)
- [src/modules-hub.ts](../../../src/modules-hub.ts)
- [scripts/validate-whisper-runtime.mjs](../../../scripts/validate-whisper-runtime.mjs)
- [scripts/latency-benchmark.ps1](../../../scripts/latency-benchmark.ps1)
- Session memory: `/memories/session/plan.md`

## Current Facts

- Trispr Core is the non-toggleable baseline and Assistant Core is an optional Feature Module. Trispr Core can transcribe without Assistant Core. Source: [CONTEXT.md](../../../CONTEXT.md).
- The v0.8.3 active release-gate closure block is the Vulkan Whisper hotfix. Source: [CONTEXT.md](../../../CONTEXT.md) and [project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md](vulkan-whisper-fix.md).
- Existing module lifecycle descriptors use states such as `not_installed`, `installed`, `enabled`, `active`, and `error`. Source: [src-tauri/src/modules/mod.rs](../../../src-tauri/src/modules/mod.rs).
- The module health adapter now classifies optional disabled or unused modules as `ok`, enabled missing setup as `needs_setup`, and Core errors as `release_blocker`. Source: [src-tauri/src/modules/health.rs](../../../src-tauri/src/modules/health.rs).
- Dependency preflight now uses the module health adapter for module issues instead of warning directly on descriptor `state == "error"`. Source: [src-tauri/src/runtime_commands.rs](../../../src-tauri/src/runtime_commands.rs).
- Module descriptors make `last_error` take precedence over enabled state after installation, so a stale optional-module error can surface as `error`. Source: [src-tauri/src/modules/registry.rs](../../../src-tauri/src/modules/registry.rs).
- Enabling a module records missing install, dependency, or consent failures into `last_error`; disabling or successful enable clears `last_error`. Source: [src-tauri/src/modules/lifecycle.rs](../../../src-tauri/src/modules/lifecycle.rs).
- Fresh defaults do not enable Feature Modules through `ModuleSettings::default()`, and `WorkflowAgentSettings::default()` starts with `enabled: false`. Source: [src-tauri/src/modules/mod.rs](../../../src-tauri/src/modules/mod.rs).
- Assistant Core migration can enable Assistant Core when legacy usage signals exist, including assistant product mode, consent history, overrides, customized wakewords, or workflow-agent flags. Source: [src-tauri/src/state.rs](../../../src-tauri/src/state.rs).
- Existing Modules Hub cards still derive labels and status classes directly from descriptor state. The health toast now maps `needs_setup`, `fallback_active`, and `local_warning` to warnings, and `release_blocker` to error. Source: [src/modules-hub.ts](../../../src/modules-hub.ts).
- The Vulkan runtime validator requires `whisper-cli.exe`, `whisper-server.exe`, `whisper.dll`, `ggml.dll`, `ggml-base.dll`, `ggml-cpu.dll`, and `ggml-vulkan.dll` for the Vulkan variant. Source: [scripts/validate-whisper-runtime.mjs](../../../scripts/validate-whisper-runtime.mjs).
- The latency benchmark script forces `TRISPR_LOCAL_BACKEND=vulkan` for Vulkan variants and restores the previous environment value afterward. Source: [scripts/latency-benchmark.ps1](../../../scripts/latency-benchmark.ps1).

## Decisions

- Use a release/setup health taxonomy with `release_blocker`, `needs_setup`, `fallback_active`, `local_warning`, and `ok` for release and doctor reporting. ADR: none; documented in [CONTEXT.md](../../../CONTEXT.md) and [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md).
- Keep the existing module lifecycle state machine mostly intact. Add a small interpretation layer over descriptors instead of rewriting `not_installed`, `installed`, `active`, and `error`. ADR: none; this is a scoped adapter decision over accepted module lifecycle ADRs, not a hard-to-reverse architecture decision.
- Disabled, uninstalled, or unused optional Feature Modules are not global app errors. Missing setup becomes user-facing `needs_setup` only when the user enabled the feature, selected a product mode that depends on it, or is actively using the feature surface. ADR: none; this follows the existing `Enabled` and `Available` definitions in [CONTEXT.md](../../../CONTEXT.md) and the accepted installable module decision.
- UI changes should be minimal. The Modules Hub should consume clearer health/setup semantics but should not be redesigned as part of this hotfix. ADR: none; implementation scope constraint.

## Progress

- Updated [CONTEXT.md](../../../CONTEXT.md) with the v0.8.3 release-clean rule and intent-scoped setup health language.
- Updated [docs/V0.8.x_BLOCK_U_RELEASE_GATE.md](../../../docs/V0.8.x_BLOCK_U_RELEASE_GATE.md) with setup health classes and fresh/default profile criteria.
- Created this checkpoint as the durable handoff for implementation.
- Implemented the first Rust health-classifier slice in [src-tauri/src/modules/health.rs](../../../src-tauri/src/modules/health.rs).
- Extended `ModuleHealthStatus` state typing in [src-tauri/src/modules/mod.rs](../../../src-tauri/src/modules/mod.rs) and [src/types.ts](../../../src/types.ts).
- Added minimal Modules Hub health-toast severity support in [src/modules-hub.ts](../../../src/modules-hub.ts), without changing layout, module cards, or consent flows.
- Updated dependency preflight in [src-tauri/src/runtime_commands.rs](../../../src-tauri/src/runtime_commands.rs) so stale disabled Assistant Core errors no longer produce startup Dependency Warnings.
- Added focused Rust tests for fresh default optional modules, stale disabled Assistant Core `last_error`, enabled Assistant Core without consent, enabled not-installed optional module setup, and Core error release blocking.
- Added dependency-preflight tests for ignoring stale disabled optional module errors and warning for enabled setup gaps.

## Stale Or Conflicting Context

- [project-spec/checkpoints/2026-06-10/vulkan-whisper-fix.md](vulkan-whisper-fix.md) remains active for the Vulkan runtime hotfix. It does not supersede this checkpoint, and this checkpoint does not supersede it. The two threads should be read together for v0.8.3 work.
- [project-spec/README.md](../../README.md) describes the ADR directory structure, but there is no `project-spec/decisions/README.md` file even though the editing instruction references one. This is non-material for this checkpoint because existing date-based ADR files provide the local format.
- No stale ADR content was found in the audited surface. Existing module ADRs already support the `Enabled` versus setup/availability distinction.

## Open Questions

- Should the current `get_module_health` command be enough for doctor/release tooling, or should a dedicated release-health command aggregate module, Core, hotkey, and Vulkan checks later?
- Should stale optional `last_error` values remain ignored only in health output, or should a narrow migration clear them when the module is disabled or unused?
- Which existing release script should own the final fresh/default setup health check?
- How should deep AI Refinement/Ollama runtime and model readiness be integrated without coupling module health to the full app settings/runtime state too early?
- Should the project add a `project-spec/decisions/README.md` file later to match the instruction reference, or is [project-spec/README.md](../../README.md) sufficient?

## Next Actions

1. Decide whether a dedicated release-health command or script hook is needed beyond `get_module_health` and dependency preflight.
2. If needed, add an aggregate release-health path that combines module health with Core/Vulkan runtime checks and local warnings such as hotkey conflicts.
3. Integrate AI Refinement/Ollama readiness only after choosing the aggregate health surface, because current module health only receives `ModuleSettings`.
4. Re-check the Modules Hub cards after runtime testing. Only change card labels/severity if false red states remain outside the health toast and dependency preflight.
5. Run the Vulkan benchmark path after any release-gate wiring changes.
6. Update this checkpoint with final release-gate evidence before handoff or closure.

## Kickoff Prompt

Use checkpoint-kickoff. Continue from `project-spec/checkpoints/2026-06-10/release-clean-setup-health.md`: decide whether v0.8.3 needs a dedicated aggregate release-health command/script beyond `get_module_health`, then wire Core/Vulkan/local-warning checks only if the current command is insufficient. Use `/grill-with-docs` for checkpoint, ADR, domain, release-policy, stale-doc, or cross-file design uncertainty; use `/grill-me` for pure implementation uncertainty. Fix but do not commit unless the kickoff owner explicitly authorizes commit.

## Verification

Passed:

- Worktree was clean before these doc/checkpoint edits.
- Sources above were read for the audited surface.
- ADR creation gate was evaluated. No new ADR was created because the decision is a scoped release-health interpretation over accepted module lifecycle decisions, not a separate hard-to-reverse architecture choice.
- `cargo test --manifest-path src-tauri/Cargo.toml modules::health`: 5 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml runtime_commands`: 2 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib`: 259 passed.
- `npm test`: 36 files passed, 637 tests passed.
- `node scripts/validate-whisper-runtime.mjs --variant vulkan`: passed.
- Editor diagnostics on changed Rust and TypeScript files: no errors.

Not checked:

- No Vulkan latency benchmark was run in this implementation slice.
- No full `npm run build` or strict assistant release gate was run.
- Deep AI Refinement/Ollama runtime/model readiness is not yet part of module health.