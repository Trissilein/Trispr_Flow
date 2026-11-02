# Frontend Guidelines — Trispr Flow

Last updated: 2026-11-02

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

The theme is OLED dark (palette from SchedOpti). All tokens live in `:root` at the top of `src/styles.css`.

**Rule: no color literals (`#hex`, `rgb()`, `rgba()` with numbers) outside `:root`.** Rules use tokens only. For a translucent tint use `rgba(var(--NAME-rgb), alpha)` with a channel token, or one of the `--tint-*` steps. A color that is needed three or more times gets a token; a one-off does not justify a literal either, pick the closest token. Exception: `overlay.html` is a standalone window without `styles.css` and keeps its own small `:root`.

### Surfaces and borders

| Token | Value | Use |
|---|---|---|
| `--app-bg` | `#08090b` | Page background (true OLED black) |
| `--app-input` | `#0c0e10` | Inset wells, inputs, pipeline board |
| `--panel` | `#101214` | Panels, sections |
| `--app-raised` | `#151719` | Raised rows, nodes, banners |
| `--card` | `#1c1f22` | Cards, history items |
| `--card-hover` | `#23272b` | Hover surface (only `history-load-more` today) |
| `--stroke` | `#2a2e32` | Default border and separator |
| `--stroke-strong` | `#3a4046` | Emphasized border, pipeline wires |

### Text

| Token | Value | Use | Contrast on `--panel` / `--card` |
|---|---|---|---|
| `--ink` | `#e8ebee` | Primary text | 15.69 / 13.83 |
| `--ink-soft` | `#a0a7ae` | Secondary text | 7.72 / 6.81 |
| `--muted` | `#7f878f` | Hints, meta, disabled labels | 5.15 / 4.54 |
| `--on-accent` | `#08090b` | Text on `--accent-2` / brand fills (white gave 2.99:1) | 6.65 on `--accent-2` |

Never put `--muted` on `--card-hover` (4.13, below AA); use `--ink-soft` there.

### Accent

| Token | Value | Use |
|---|---|---|
| `--accent-2` (`--accent-2-rgb`) | `#1da6a0` | Primary action, active state, selection |
| `--accent-2-hover` | `#1bb8b1` | Hover of primary fills |
| `--accent-2-bright` (`--accent-2-bright-rgb`) | `#4be0d4` | Teal text and focus ring on dark (10.18 on `--card`) |
| `--accent` | `#ff6b3d` | Warm highlight (legacy) |
| `--accent-3` | `#f5b342` | Legacy amber, prefer `--status-warning` |
| `--rec-rgb` | `255, 107, 61` | Recording indicators (threshold markers, pulse) |
| `--violet`, `--violet-rgb` | `#d8b4fe`, `168, 85, 247` | Curated / category chips |

### Status

Each status has a text color, a fill, a line and a channel token. Text on its own fill is 8.07 to 9.30:1.

| Status | Text | Fill | Line | Channel |
|---|---|---|---|---|
| success | `--status-success` `#72d59e` | `--status-success-bg` `#13271d` | `--status-success-line` `#2a513b` | `--status-success-rgb` |
| warning | `--status-warning` `#f0c36a` | `--status-warning-bg` `#2b2417` | `--status-warning-line` `#6b5124` | `--status-warning-rgb` |
| error | `--status-error` `#ff9c9c` | `--status-error-bg` `#2b1d20` | `--status-error-line` `#654147` | `--status-error-rgb` |
| info | `--status-info` `#9bcdf7` | `--status-info-bg` `#182838` | `--status-info-line` `#355a79` | `--status-info-rgb` |

Chips built as `rgba(var(--status-x-rgb), 0.12..0.18)` over `--panel` / `--card` with `--status-x` text stay at 5.2 to 9.0:1.

### Tints and helpers

| Token | Value | Use |
|---|---|---|
| `--tint-1` to `--tint-7` | white at 0.02, 0.04, 0.06, 0.08, 0.12, 0.22, 0.30 | Hover, zebra rows, hairlines, scrollbars (pick the nearest step) |
| `--black-rgb` | `0, 0, 0` | Shadows: `rgba(var(--black-rgb), 0.3)` |
| `--brand-nvidia`, `--brand-amd` (+ `-rgb`) | `#76b900`, `#ed1c24` | CUDA / Vulkan backend toggle only |
| `--shadow` | `0 4px 16px` black at 0.3 | Default elevation |
| `--radius`, `--radius-sm` | `10px`, `6px` | Cards and controls, small controls |
| `--font-mono` | Consolas stack | Monospace |

Aliases kept for older rules: `--text`, `--text-bright`, `--text-muted`, `--ink-muted`, `--ink-softer`, `--bg-base`, `--bg-surface`, `--panel-soft`, `--surface-hover`, `--accent-1`, `--accent-rgb`, `--color-warning`, `--radius-md`. New rules use the primary names.

Every `var(--x)` used in `src/**/*.ts`, `index.html` and the stylesheets must be defined in `:root` (checked with a script on 2026-11-02: none missing). Do not add fallbacks (`var(--x, #abc)`).

### Typography
- Sans: `IBM Plex Sans` (bundled in `public/fonts`), fallback stack
- Display accents: `Space Grotesk`
- Labels: sentence case, no uppercase tracking. Only tiny status chips (`.model-status`, `.qwen-card-badge`, `.status-pill`) stay uppercase.

### Spacing, radius, motion
- Base spacing grid: 4px increments
- Primary radius `--radius: 10px`, small radius `--radius-sm: 6px`, pills `999px`
- Transitions name their properties (no `transition: all`); hover moves at most 1px
- Hue gradients, colored glows and looping decoration are not used; `prefers-reduced-motion` switches off the remaining pulses
- Global `:focus-visible` ring in `--accent-2-bright`; `color-scheme: dark` keeps native controls dark

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
