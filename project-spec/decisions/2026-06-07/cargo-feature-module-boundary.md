# Cargo Feature Boundary for Feature Modules

Date: 2026-06-07
Status: **accepted for first slice**
Participants: Hendr, Copilot

---

## Claim

Trispr Flow will use Cargo feature flags as the first build-time boundary for optional Feature Modules. Tauri plugins remain a later option. The first implementation slice proves the boundary with the Confluence Integration only.

---

## Context

The Module System currently describes runtime lifecycle state, but it does not remove optional code from the Rust build. Module commands are still registered from `src-tauri/src/lib.rs`, and module manifests often treat bundled, installed, enabled, and core status as the same thing.

The current domain glossary separates two concepts:

- **Trispr Core**: the non-toggleable product baseline.
- **Feature Modules**: optional capabilities with their own lifecycle, consent, permissions, and build/install story.

Confluence is the right first slice because it is a clear optional integration surface, not part of transcription Core. It is also small enough to expose the real coupling points before the same pattern is applied to larger modules.

---

## Decision

Add Cargo feature flags named `module-*` for optional Feature Modules. The initial flag is `module-confluence`.

The default build keeps current full-product behavior by enabling `module-confluence`. A `--no-default-features` build must compile without registering Confluence Tauri commands and must describe the Confluence Integration as not bundled or installed.

For this slice, the Confluence settings shape remains ungated for serde compatibility. Confluence DTOs and internal source may still compile because GDD publish queue and Workflow Agent currently refer to Confluence request/result types directly. Removing those source-level references is a later decoupling step, not part of the first gate.

---

## Consequences

- Variant builds get a real compile-time switch before the module system is fully decomposed.
- `module-confluence` becomes the naming pattern for future module flags.
- Registry state must distinguish build-bundled from runtime-enabled.
- `generate_handler!` remains the command registration hub for now, with feature-gated command entries.
- Tauri plugin extraction is deferred until a module needs to own state, setup, and commands as a deeper unit.

---

## Rejected alternatives

### Gate the entire GDD module first

Rejected for the first slice. GDD is larger, is still tangled with Assistant workflows, and would mix two decisions: optional GDD and optional Confluence.

### Convert modules to Tauri plugins immediately

Rejected for now. Plugins may become the right seam later, but the feature-flag boundary gives a cheaper compile-time check without reversing the recent command extraction work.

### Gate `Settings` fields now

Rejected for this slice. Changing persisted settings shape at the same time would make the feature boundary harder to validate and could break existing user configuration files.

---

## Follow-up

- Move Confluence request/result DTOs behind a neutral GDD publishing boundary.
- Remove direct Confluence calls from Workflow Agent once GDD owns its publishing port.
- Apply the same `module-*` flag pattern to the next optional module only after the Confluence slice validates in default and `--no-default-features` builds.
