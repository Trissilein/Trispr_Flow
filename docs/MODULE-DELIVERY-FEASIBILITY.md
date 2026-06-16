# Module Delivery Feasibility — Lean Core + On-Demand Modules

Date: 2026-06-16
Status: **finding (decision pending)**
Author: Claude (Opus 4.8), commissioned by Ingo
Scope: answers the `critical` architectural question raised in [`AUDIT-BRIEF.md`](AUDIT-BRIEF.md) §1b
directive 3 — *can a lean-core installer with runtime-added modules be reached by evolving the
existing `src-tauri/src/modules/` infrastructure, or does it need a fundamentally different
mechanism?*

This document is self-contained. It does not assume access to the KG or prior chat context.

---

## 0. TL;DR

- **The architectural fork is already decided** (see [`project-spec/decisions/2026-06-08/installable-module-package-model.md`](../project-spec/decisions/2026-06-08/installable-module-package-model.md)):
  modules are **install-by-unpack of asset/metadata packages for host-known capabilities** — *not*
  arbitrary runtime code loading. That decision is `accepted`. It removes the hardest part (plugin
  signing, crash isolation, ABI/compat) from scope.
- **The install-by-unpack engine already exists and is solid** (`modules/package.rs`): manifest schema,
  validation, path-traversal safety, staged atomic install, idempotency, good test coverage.
- **What does not exist yet is the *delivery* half**: remote discovery, download from Git, uninstall,
  and a real update check are all **0% implemented**. Today "install a module" only copies a package
  that was *already shipped inside the installer*.
- **There is one unresolved contradiction in code** that must be fixed before "lean core" is real:
  module **code presence is compile-time** (`cfg!(feature = "module-gdd")`), while module **activation
  is runtime** (package scan). A lean build that drops a feature also drops the code, so dropping a
  package in later cannot activate anything. The two models are not yet reconciled.
- **Verdict: feasible by evolving the existing infra — no new mechanism needed.** Recommended path is
  **Option A (assets-modular, code-monolithic)**: keep all host-known capability *code* compiled into
  one binary, and make the *heavy payloads* (models, runtimes, templates, sidecars) the modular,
  on-demand-delivered part. This matches both the accepted decision and the existing installer-variant
  reality, where the real weight is payload (30 MB → 560 MB), not code.

---

## 1. The goal, restated

> "Install the core elements, then add modules afterward." — Ingo

Two readings are possible, and they have very different cost:

1. **Install-time selection** — one installer, checkboxes for optional modules; modules still compiled
   in, just optionally materialized. Cheap, but "add later" means re-run the installer.
2. **Runtime marketplace** — lean-core download; modules discovered and pulled from Git *inside the
   app*, after install. This is the audit §1b directive 3 target.

The accepted 2026-06-08 decision and the existing `modules/` code both aim at **(2)**, but with a hard
security boundary: packages carry **assets and metadata only**, never executable code loaded into the
host process. That boundary is what makes (2) tractable.

---

## 2. Current state inventory (ground truth)

### 2.1 What exists and is solid

| Capability | Where | State |
|---|---|---|
| Module manifest schema (`trispr-module.json`, schema v1) | `modules/package.rs` | ✅ complete |
| Package scan + validation (id known, host-capability known, version, required assets, **no executable hooks**, **path-traversal safe**) | `package.rs::scan_package_dir` / `validate_manifest` | ✅ complete, well-tested |
| **Install-by-unpack** — staged copy to `.{id}.installing`, re-validate, atomic `rename`, idempotent | `package.rs::install_package_from_dir` | ✅ complete |
| Installed-module scan of on-disk dir | `package.rs::scan_modules_dir` | ✅ complete |
| Module registry / descriptors (11 modules, dependencies, permissions, surface, assistant actions) | `modules/registry.rs` | ✅ complete |
| Runtime state derivation (`not_installed`/`installed`/`active`/`error`) merging defaults + package scan + settings + deps | `registry.rs::modules_as_descriptors_with_packages` | ✅ complete |
| Enable/disable lifecycle + side-effect reconcile (Piper, Ollama) | `modules/lifecycle.rs`, `lifecycle_coordinator.rs` | ✅ state-level (no code unload — by design) |
| Per-module permission model (consent, whitelist-enforced at enable) | `modules/permissions.rs` | ✅ complete |
| Health / dependency preflight | `modules/health.rs` | ✅ complete |
| Module data dir resolution: `%LOCALAPPDATA%\Trispr Flow\modules\{id}\` (override `TRISPR_DATA_DIR`) | `paths.rs::resolve_modules_dir` | ✅ complete |
| Tauri commands: `list_modules`, `scan_module_packages`, `install_bundled_module_package`, `enable_module`, `disable_module`, `get_module_health`, `check_module_updates` | `lib.rs` ~1620–1885 | ✅ wired to frontend |
| Frontend Modules Hub UI (cards, Install/Enable/Disable/Health) | `src/modules-hub.ts` | ✅ for *bundled* modules |
| GDD package shipped as installer resource | `tauri.conf.json` resources | ✅ (GDD only) |

### 2.2 What does NOT exist (the delivery gap)

| Missing piece | Evidence | Impact |
|---|---|---|
| **Remote fetch** of a package from Git / HTTP | no `download_module`/`fetch_module`; no `reqwest`; no registry/manifest URL anywhere in `modules/` | **the** core blocker for "add modules later" |
| **Discovery API** — "browse available modules" | no `list_available_modules`; Modules Hub only lists registry entries | user can't see what's installable |
| **Uninstall** — delete on disk + clear state | `disable_module` only edits `enabled_modules`; no `uninstall_module` | modules accumulate; no clean removal |
| **Real update check** | `check_module_updates` always returns `update_available: false` (`health.rs` ~121) | no update story |
| **Signature / integrity verification** of downloaded packages | none (SHA256 exists for *runtime payloads* like FFmpeg, not modules) | needed once packages come from the network |
| **Installer module selection** | NSIS wizard selects Piper voices / FFmpeg, not modules | no install-time module choice today |

### 2.3 Today's actual "install a module" flow

`install_bundled_module_package(id)` copies a package **from the installer's bundled resources**
(`tauri.conf.json` ships `module-packages/gdd/...`) into `modules_dir`, then the user enables it.
So "adding a module" currently means **activating something the installer already shipped** — not
fetching anything new. The plumbing is real; only the *source* is local.

---

## 3. The one contradiction that blocks "lean core"

The accepted decision says the host keeps all executable code ("feature-gated executable capability
surfaces"; packages are assets only). But the code today gates **bundling itself** at compile time:

```rust
// registry.rs
bundled: cfg!(feature = "module-gdd"),     // GDD code present ⇔ feature compiled in
// Cargo.toml
default = ["module-gdd", "module-confluence"]
```

Consequence: if you build a *lean* core with `--no-default-features` to shrink it, the GDD code is
**not in the binary**. Dropping a GDD package into `modules_dir` later then activates… nothing,
because the capability code is absent. **Runtime install cannot resurrect compile-time-removed code.**

This is exactly the tension the audit flagged as `critical`. It is resolved *on paper* by the decision
(keep code in the host) but **not yet in code** (features still gate code presence). Reconciling these
two is the precondition for a meaningful lean core.

Note also: of the 11 registry modules, only `gdd` and `integrations_confluence` are feature-gated;
the other 9 are `bundled: true` unconditionally. So "lean core" is currently **not achieved** —
almost everything ships in the binary regardless.

---

## 4. Options with trade-offs

### Option A — Assets-modular, code-monolithic  ✅ recommended

Compile **all host-known capabilities into one binary** (stop gating code presence on cargo features;
keep features only as build-time *test* conveniences if at all). "Lean" then comes from the **heavy
payloads** delivered on demand: Whisper models, CUDA/Vulkan runtimes, Piper voices, GDD templates,
the video sidecar. The package + download engine governs *assets and activation*, never code.

- **Why it fits:** matches the accepted decision verbatim; reuses `package.rs` as-is; the installer
  variants already prove the real weight is payload, not code (vulkan-only ~30–40 MB vs cuda-complete
  ~510–560 MB — that delta is DLLs/models, not Rust).
- **Lean-core installer** = ship the small binary + minimal runtime (vulkan-only-class) + **no** heavy
  payloads or optional module assets; everything else is fetched from GitHub Releases / a Git-hosted
  module index on demand.
- **Cost:** medium. Build the delivery half (§5). No plugin security model needed.
- **Limit:** binary size is fixed (all capability code always present). For Trispr that's small
  relative to models/runtimes, so this is an acceptable trade.

### Option B — Runtime code modules (dynamic libs / WASM / sidecar processes)

True code removal from core; modules carry executable logic loaded at runtime.

- **Pro:** smallest possible core; genuinely optional code.
- **Con:** **explicitly rejected** by the 2026-06-08 decision. Requires plugin security model, signing
  policy, ABI/compat contract, crash isolation, and an update story. Large, cross-cutting, high risk.
- **Verdict:** defer. Revisit only if binary size ever becomes the dominant constraint (it isn't).

### Option C — Build-time variant matrix

Ship several installers with different feature sets (core / core+gdd / full).

- **Pro:** cheapest; no runtime delivery code.
- **Con:** "add a module later" = reinstall a different variant. Does not meet the stated UX. Also
  multiplies the release matrix (already 3 GPU variants × N feature sets).
- **Verdict:** not sufficient alone; acceptable only as an interim.

---

## 5. Recommended path (Option A) — concrete next steps

Ordered by dependency. Each is independently shippable.

1. **Reconcile the code/feature contradiction** *(precondition).*
   Decide: keep optional capabilities always-compiled (drop `cfg!`-gated `bundled`, set `bundled: true`),
   OR keep features but define "lean core" as a *payload* profile, not a *code* profile. Recommended:
   the former. Make `bundled` reflect "code is in this binary" honestly; let the package scan own
   install-state. — *small, mostly `registry.rs` + `Cargo.toml`.*

2. **Define the module source + remote index.**
   A Git repo (or this repo's GitHub Releases) hosting one zip per module package + an `index.json`
   listing `{id, name, version, url, sha256, size}`. — *infra/decision, no app code.*

3. **`download_module_package(id)` backend command.**
   Fetch zip (reuse existing HTTP path — `ureq` is already a dependency) → verify SHA256 → unzip to a
   temp dir → hand to the existing `install_package_from_dir`. Reuses all current validation. — *medium.*

4. **`list_available_modules()` discovery command.**
   Fetch + cache `index.json`; diff against installed scan to mark installed/update-available. — *small.*

5. **`uninstall_module(id)`** — remove `modules_dir/{id}` + clear `{id}.installed`/enabled state +
   reconcile side-effects. Mirror of install. — *small.*

6. **Wire real `check_module_updates`** against the index (installed version vs index version). — *small.*

7. **Frontend: Modules Hub "Browse / Download" section** — render available modules, Download button →
   `download_module_package`, progress, then the existing Enable flow. — *medium.*

8. **Lean-core installer profile** — a variant that ships the binary + minimal runtime and **omits**
   bundled module assets + heavy payloads, relying on §3/§4 for on-demand fetch. Keep one "offline /
   complete" variant that still bundles everything for air-gapped installs. — *medium, build scripts +
   `tauri.conf.json`.*

9. **Integrity/trust** — since packages now come from the network, document the trust model (host-known
   IDs + SHA256 from a signed index is the minimum; the decision already forbids executable hooks, which
   caps the blast radius). — *small, mostly policy.*

**Suggested first slice (pilot):** make **GDD** downloadable from GitHub Releases instead of bundled —
steps 2, 3, 4, 7 scoped to one module. It exercises the whole delivery path end-to-end on the module
that already has a real package, with the smallest surface.

---

## 6. Answer to the audit's `critical` question

> Can the target be reached by evolving the existing `modules/` infra, or does it need a fundamentally
> different mechanism?

**Evolve — no new mechanism.** The install-by-unpack engine, manifest model, registry, lifecycle,
permissions, and health reporting are already the right shape and are well-tested. The missing work is
a **delivery layer** (download + discovery + uninstall + update), plus a one-time **reconciliation** of
the compile-time-vs-runtime contradiction in §3. Runtime *code* loading (Option B) is neither required
nor desired under the accepted decision and should stay out of scope.

---

## 7. Open decisions for the maintainer

1. **§5.1** — make optional capabilities always-compiled (recommended), or define lean-core as a
   payload-only profile?
2. **§5.2** — module source: dedicated Git repo, or this repo's GitHub Releases + `index.json`?
3. **Offline story** — keep one "complete/offline" installer that still bundles everything for
   air-gapped use? (recommended: yes.)
4. **Pilot scope** — start with GDD-as-download (§5 first slice), or build the full delivery layer
   before migrating any module?
