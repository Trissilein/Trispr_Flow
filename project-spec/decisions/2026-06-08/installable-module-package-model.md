# Installable Module Package Model

Date: 2026-06-08
Status: **accepted**
Participants: Hendr, Ingo, Copilot

---

## Claim

Trispr Flow modules should become installable by unpacking a module package into a module directory. The first target model is package-based installation for host-known capabilities, not arbitrary runtime code loading.

---

## Context

The earlier Module System described module lifecycle state inside the app, but most modules were still built into the host binary and marked installed by registry defaults. Ingo clarified on 2026-06-08 that modules should eventually be installable by unpacking a package into a module directory.

The clarification also changed the GDD boundary:

- GDD should be an installable Feature Module.
- Confluence is part of the GDD Module, not a separately installable module.
- GDD Copilot is no longer a product priority.
- The publish queue is not product language and should not drive the domain model.

---

## Decision

An installable module package may contain descriptive metadata and assets for capabilities the host app already knows how to execute. It may include:

- a module manifest
- presets, templates, schemas, and default configuration
- UI-surface metadata for known host surfaces
- sidecar binaries or resources for host-known execution paths
- internal capability metadata, such as Confluence publishing inside GDD

It must not introduce arbitrary executable code loaded into the host process in the first target model. Runtime plugin code loading needs a separate security and compatibility decision.

A module is `Installed` only after the host validates:

- manifest parse and schema validity
- known module ID
- known host capability
- compatible module and schema versions
- all manifest-declared required assets
- absence of unsupported executable hooks

`Enabled` remains user intent. Enabling a module may start consent or setup flows, but missing permissions, secrets, service configuration, or network reachability make a module or sub-capability degraded or unavailable. They do not make the module uninstalled.

---

## GDD Boundary

GDD is the first target module for this model. The GDD package boundary is descriptive and asset-oriented. It contains its manifest, presets, templates, validation/default schemas, UI-surface metadata, and Confluence configuration/template metadata.

The host app continues to own:

- Rust commands
- TypeScript UI
- permission enforcement
- settings normalization and migration
- feature-gated executable capability surfaces

GDD is one Feature Module with internal sub-capabilities. Expected sub-capabilities include draft generation, validation, file template import, Confluence template import, Confluence publishing, and routing suggestions. These sub-capabilities may report health separately while the GDD Module remains installed and enabled.

Confluence remains an internal GDD integration boundary for auth, secrets, network access, spaces/pages, target routing, and queue behavior. It is not a separately installable product module.

---

## Implementation Sequence

1. Add the GDD package structure and manifest scanner.
2. Introduce a `module-gdd` registry and command-surface gate.
3. Pursue deeper source removal only after the package boundary and surface gate validate.

The prior `module-confluence` feature remains an interim boundary probe. It should converge into the GDD module boundary rather than become the target product shape.

---

## Rejected Alternatives

### Runtime-loaded plugin code now

Rejected. It would require a plugin security model, compatibility contract, signing policy, crash isolation strategy, and update story. That is larger than Ingo's current install-by-unpack requirement.

### Confluence as its own installable module

Rejected by Ingo's clarification. Confluence belongs inside the GDD Module.

### One GDD health state only

Rejected. GDD can be useful when some sub-capabilities are unavailable. Draft generation can be healthy while Confluence publishing is degraded due to missing credentials or network reachability.
