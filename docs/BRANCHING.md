# Branching Model

Last updated: 2026-02-22

## Core Branches

- `main`
  - Trispr Flow MVP product mainline
  - Source of truth for production-ready baseline
- `spike/ollama-offline-fallback`
  - Offline-first AI fallback exploration line (Ollama)
  - Isolated from `main` until feature readiness

## Workflow

1. Product-ready work lands on `main`.
2. `spike/ollama-offline-fallback` branches from `main`.
3. `main` is merged regularly into that WIP branch to keep drift low.

## Housekeeping Rules

- Keep `main` and `spike/ollama-offline-fallback` buildable and documented.
- Avoid additional long-lived feature branches unless needed for risky refactors.
- If temporary branches are used, merge or close them quickly.
