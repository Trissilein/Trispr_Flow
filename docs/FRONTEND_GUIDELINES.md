# Frontend Guidelines — Trispr Flow

Last updated: 2026-03-20

This document is the consolidated frontend guideline for Trispr Flow.
It replaces the former root docs `DESIGN_SYSTEM.md`, `FRONTEND_GUIDELINES.md`, and `SIMPLIFY.md`.

## 1) Scope

Use this file as canonical reference for:
- frontend architecture and module boundaries
- UI state and rendering conventions
- design tokens and component-level styling rules
- simplification/review rules for frontend changes

## 2) Stack and Philosophy

- Framework style: Vanilla TypeScript + Tauri 2 (no React/Vue/Svelte runtime)
- Build: Vite
- Tests: Vitest
- Principle: explicit state changes and explicit DOM updates (no hidden reactivity)

## 3) Architecture Rules

- Keep modules single-purpose (`state.ts`, `settings.ts`, `models.ts`, `history.ts`, etc.).
- Centralize DOM lookups in `src/dom-refs.ts`.
- Keep event wiring in `src/event-listeners.ts`.
- Keep persistence via `invoke("save_settings")` through the settings module.
- Keep cross-module UI refreshes batched (`schedule*Render`) where possible.

## 4) File Ownership (Frontend)

- `src/main.ts`: bootstrap + Tauri event listeners
- `src/state.ts`: frontend in-memory state
- `src/settings.ts`: settings rendering + persistence flow
- `src/ollama-models.ts`: local runtime + model manager UX
- `src/models.ts`: Whisper model manager + quantization UI
- `src/styles.css`: primary styles and component skinning

## 5) Design Tokens

### Colors
- `--ink`, `--ink-soft`, `--muted`: text hierarchy
- `--accent`: warm highlight
- `--accent-2`: primary action/active state
- `--accent-3`: transcribing/warning accent
- `--stroke`, `--stroke-strong`: borders and separators

### Typography
- Sans: `IBM Plex Sans` fallback stack
- Display/Mono accents: `Space Grotesk`
- Labels and badges: compact uppercase style with slight letter spacing

### Spacing + Radius
- Base spacing grid: 4px increments
- Primary radius: `--radius: 10px`
- Small radius: `--radius-sm: 6px`
- Pill/badge radius: `--radius-pill: 999px`

## 6) Component Conventions

### Buttons
- Primary action: accent color, clear hover feedback, no ambiguous ghost primary.
- Dangerous action: red-tinted destructive styling.
- Labels should be action-first (`Download model`, `Activate`, `Delete`).

### Form Controls
- Inputs/selects keep consistent padding, stroke, and focus ring.
- Runtime/version selections must expose status hints (selected, installed, recommended, installable).

### Status and Feedback
- Status chips must encode semantic state (`available`, `installed`, `active`, `warning`, `error`).
- Toasts should include follow-up CTA only when immediate user action is meaningful.

## 7) State and Render Rules

- Update state first, then call targeted render function(s).
- Avoid duplicate save/render sequences; prefer shared helpers.
- Use `requestAnimationFrame` scheduling for high-frequency updates.
- Avoid writing logic directly inside large template strings when helper functions can isolate behavior.

## 8) Simplify / Quality Gates

For every larger frontend change, check:
- Reuse: Is logic already implemented elsewhere?
- Quality: Is state duplicated or stringly-typed?
- Efficiency: Are we causing unnecessary renders/listeners or repeated expensive work?

Apply concrete fixes only; avoid speculative refactors without a measurable benefit.

## 9) UX Copy Standards (Local AI)

- Use consistent action wording:
  - `Download model` for pull operations
  - `Activate` for selecting the active local refinement model
  - `Activate now` for post-download CTA
- Prefer concise and deterministic status text over decorative text.

## 10) Accessibility and Reliability

- Keep keyboard paths available for primary settings/actions.
- Preserve `aria-*` semantics for toggles, expanders, progress, and selected states.
- Ensure listener cleanup for window-level handlers.
- Ensure failure states remain actionable (explicit hint + retry path).

## 11) Related Docs

- Architecture: `ARCHITECTURE.md`
- State semantics: `STATE_MANAGEMENT.md`
- Development workflow: `DEVELOPMENT.md`
- Decisions: `DECISIONS.md`
- Lessons and review outcomes: `LESSONS.md`
