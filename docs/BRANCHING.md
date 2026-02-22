# Branching Model

Last updated: 2026-02-22

## Core Branches

- `main`
  - Trispr Flow MVP product mainline
  - Source of truth for production-ready baseline
- `vip-base`
  - Integration baseline branch
  - No feature work directly; refreshed from `main` and used as branch-off point
- `analysis-module-branch`
  - Standalone analysis module project line
  - Independent docs, architecture, and web UI template
- `spike/ollama-offline-fallback`
  - Offline-first AI fallback exploration line (Ollama)
  - Isolated from `main` until feature readiness

## Workflow

1. Product-ready work lands on `main`.
2. `vip-base` is fast-forwarded from `main` and kept in sync.
3. `analysis-module-branch` and `spike/ollama-offline-fallback` branch from `vip-base`.
4. `main` is merged regularly into both WIP branches to keep drift low.
5. No analysis runtime code is maintained inside Trispr Flow `main`.
6. Integration between lines is contract-based (file/API schema), not shared runtime.

## Housekeeping Rules

- Keep `main`, `analysis-module-branch`, and `spike/ollama-offline-fallback` buildable and documented.
- Keep `vip-base` aligned with `main` (no custom commits unless absolutely required).
- Avoid additional long-lived feature branches unless needed for risky refactors.
- If temporary branches are used, merge or close them quickly.
