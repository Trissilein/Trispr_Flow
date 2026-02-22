# Branching Model

Last updated: 2026-02-20

## Active Branches

- `main`
  - Trispr Flow product mainline
  - Capture/transcription/runtime UX and release prep
- `analysis-module-branch`
  - Standalone analysis module project
  - Independent docs, architecture, and web UI template

## Workflow

1. Product work lands on `main`.
2. Analysis module work lands on `analysis-module-branch`.
3. No analysis runtime code is maintained inside Trispr Flow mainline.
4. Integration between both lines is contract-based (file/API schema), not shared runtime.

## Housekeeping Rules

- Keep both branches buildable and documented.
- Avoid long-lived extra feature branches unless needed for risky refactors.
- If temporary branches are used, merge or close them quickly.
