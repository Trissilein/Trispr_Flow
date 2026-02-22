# Branching Model

Last updated: 2026-02-22

## Core Branches

- `main`
  - Trispr Flow MVP product mainline
  - Source of truth for production-ready baseline
- `analysis-module-branch`
  - Standalone analysis module project line
  - Independent docs, architecture, and web UI template
- `spike/ollama-offline-fallback`
  - Offline-first AI fallback exploration line (Ollama)
  - Isolated from `main` until feature readiness

## Workflow

1. Product-ready work lands on `main`.
2. `analysis-module-branch` and `spike/ollama-offline-fallback` branch from `main`.
3. `main` is merged regularly into both WIP branches to keep drift low.
4. No analysis runtime code is maintained inside Trispr Flow `main`.
5. Integration between lines is contract-based (file/API schema), not shared runtime.

## Housekeeping Rules

- Keep `main`, `analysis-module-branch`, and `spike/ollama-offline-fallback` buildable and documented.
- Avoid additional long-lived feature branches unless needed for risky refactors.
- If temporary branches are used, merge or close them quickly.
