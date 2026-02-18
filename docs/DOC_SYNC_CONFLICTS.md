# Documentation Sync - Conflicts and Discussion Points

Last updated: 2026-02-18

This file tracks contradictions found during doc consolidation.

## Resolved Conflicts

| Topic | Previous conflict | Resolution |
| --- | --- | --- |
| Sidecar packaging | Some docs claimed "no Python dependencies for users" while status notes still required manual setup | Canonical docs now state: runtime supports bundled exe and Python fallback; current installer ships Python sidecar setup path |
| Sidecar request format | Sidecar docs mentioned `multipart/form-data`, code uses JSON path request | Sidecar docs and code comments now aligned to JSON `{ audio_path, precision, language }` |
| v0.7 migration references | v0.7 docs referenced non-existing `src-tauri/src/cloud_fallback.rs` and obsolete settings fields | Updated to current `transcription.rs` + `cloud_fallback: bool` baseline |
| Installer variants | `INSTALLER_VARIANTS.md` still used old v0.4 naming/sizing assumptions | Rewritten to current CUDA/Vulkan + optional Voice Analysis setup flow |

## Decisions Locked (2026-02-18)

| ID | Selected option | Implementation note |
| --- | --- | --- |
| D1 | Keep base installer slim + optional Voice Analysis pack/setup path | Treat as Block A distribution baseline |
| D2 | Prefetch default OFF + guided first-use setup on Analyse click | Add storage hint/check/progress UX in Block B/C |
| D3 | Local `VibeVoice` auto-discovery OFF in release, ON in dev/explicit override | Add build/runtime gating in Block B |
| D4 | Soft fail for optional Voice Analysis setup | Keep app install successful, surface remediation + retry path |

## Canonical Sources After Sync

- Priorities and dependencies: `ROADMAP.md`
- Operational snapshot: `STATUS.md`
- Detailed task table: `docs/TASK_SCHEDULE.md`
- Sidecar runtime/API: `sidecar/vibevoice-asr/README.md`
- Installer behavior: `docs/INSTALLER_VARIANTS.md`
