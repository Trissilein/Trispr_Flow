# Documentation Guide

Last updated: 2026-02-20

This file is the canonical documentation map for Trispr Flow.
Rule: every topic has one canonical file.

## Root policy
Only these Markdown files are allowed in the repo root:

- `README.md`
- `ROADMAP.md`
- `CHANGELOG.md`
- `CONTRIBUTING.md`
- `CLAUDE.md`
- `THIRD_PARTY_NOTICES.md`

All other project docs belong in `docs/`.

## Canonical file map

| Topic | Canonical file | Notes |
| --- | --- | --- |
| Product overview + quick start | `README.md` | Keep high-level only |
| Priorities + status snapshot + next work | `ROADMAP.md` | Single source for planning |
| Release history | `CHANGELOG.md` | Keep-a-changelog style |
| Contributor workflow | `CONTRIBUTING.md` | PR/testing expectations |
| Architecture | `docs/ARCHITECTURE.md` | Runtime/module boundaries |
| App behavior and user flows | `docs/APP_FLOW.md` | UX and panel flow |
| Frontend implementation guidelines | `docs/frontend/FRONTEND_GUIDELINES.md` | TS/frontend conventions |
| Visual tokens and UI patterns | `docs/frontend/DESIGN_SYSTEM.md` | Design system |
| State model | `docs/STATE_MANAGEMENT.md` | UI/runtime states |
| Dev setup + build/test | `docs/DEVELOPMENT.md` | Local environment |
| Decisions (ADR-lite) | `docs/DECISIONS.md` | Why/tradeoffs |
| Installer variants | `docs/INSTALLER_VARIANTS.md` | Packaging matrix |
| Documentation contradiction log | `docs/DOC_SYNC_CONFLICTS.md` | Resolve doc conflicts |
| Historical docs | `docs/archive/*.md` | Archive only, not canonical |
| Wiki navigation | `docs/wiki/*.md` | Link hub only |

## Update workflow

1. Edit the canonical file first.
2. Update links/references (`README.md`, wiki pages, cross-links).
3. If behavior changed, update `ROADMAP.md` and `docs/DECISIONS.md`.
4. Run `npm run test:docs` before commit.

## Enforcement

`npm run test:docs` checks root Markdown governance (no new ad-hoc root `.md` files).
