# Cargo Feature Boundary for Feature Modules

Date: 2026-06-07
Status: **accepted for first slice; target module boundary clarified 2026-06-08**
Participants: Hendr, Copilot

Superseding target model: [`../2026-06-08/installable-module-package-model.md`](../2026-06-08/installable-module-package-model.md)

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

Clarification from Ingo, 2026-06-08: GDD should be installable as a Feature Module, and Confluence is part of the GDD Module. Modules should eventually be installable by unpacking a module package into a module directory. GDD Copilot is no longer a product priority.

Follow-up clarification, 2026-06-08: installable modules mean package-based installation for host-known capabilities. Module packages may provide manifests, assets, templates, presets, configuration defaults, and sidecar resources. They do not imply arbitrary runtime code loading in the first target model.

Confirmed GDD package boundary, 2026-06-08: the first GDD package should contain descriptive module metadata and GDD-owned assets such as presets, templates, validation/default schemas, UI-surface metadata, and Confluence configuration/template metadata. The host app continues to own Rust commands, TypeScript UI, permission enforcement, settings normalization, and the feature-gated executable capability surface.

Confirmed installation rule, 2026-06-08: a module is installed only after the host validates its manifest, recognizes the declared module ID and host capability, accepts the version/schema constraints, and verifies all required assets. A directory by itself is not installed.

Confirmed lifecycle rule, 2026-06-08: enabling a module records user intent and may start consent or setup flows. Permissions, secrets, and external service configuration do not define installation. Missing setup makes a module or sub-capability degraded or unavailable, not uninstalled.

Confirmed GDD capability rule, 2026-06-08: GDD is one Feature Module with internal sub-capabilities. Draft generation, validation, file template import, Confluence template import, Confluence publishing, and routing suggestions may each report health separately. Confluence remains an internal GDD integration boundary, not a separate product module.

---

## Decision

Add Cargo feature flags named `module-*` for optional Feature Modules. The initial flag is `module-confluence`.

After the 2026-06-08 clarification, `module-confluence` is treated as an interim boundary probe. The target product boundary is the GDD Module, with Confluence as an internal GDD integration surface rather than a separately installable module.

The default build keeps current full-product behavior by enabling `module-confluence`. A `--no-default-features` build must compile without registering Confluence Tauri commands and must describe the Confluence Integration as not bundled or installed.

For this slice, the Confluence settings shape remains ungated for serde compatibility. Confluence DTOs and internal source may still compile because GDD publish queue and Workflow Agent currently refer to Confluence request/result types directly. Removing those source-level references is a later decoupling step, not part of the first gate.

---

## Consequences

- Variant builds get a real compile-time switch before the module system is fully decomposed.
- `module-*` remains the naming pattern for future module flags.
- Future work should converge from the interim `module-confluence` probe toward a GDD module boundary.
- Registry state must distinguish build-bundled from runtime-enabled.
- `generate_handler!` remains the command registration hub for now, with feature-gated command entries.
- Tauri plugin extraction is deferred until a module needs to own state, setup, and commands as a deeper unit.
- Runtime plugin code loading is out of scope until a separate security and compatibility decision exists.

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

- Treat Confluence as an internal GDD integration boundary, not a separately installable module.
- Do not generalize the publish queue as a product concept until its purpose is clarified. Ingo did not recognize it as product language on 2026-06-08.
- Next implementation sequence, confirmed 2026-06-08: first add the GDD package structure and manifest scanner, then introduce a `module-gdd` registry and command-surface gate, then pursue deeper source removal after the package boundary and surface gate are validated.
