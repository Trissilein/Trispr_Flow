# Read Me First - Documentation Rules

Last updated: 2026-02-09

This file defines where information belongs.
Rule: write each topic once, in one canonical file.

## 1) Before you write

1. Check this file.
2. Pick the target file from the table below.
3. Update links if needed.
4. Do not duplicate the same content in another `.md`.

## 2) Canonical file map

| Topic | Canonical file | What belongs there |
|---|---|---|
| Project overview, quick setup, entry links | `README.md` | High-level intro only, no deep implementation details |
| Planning, priorities, done/open work | `ROADMAP.md` | Milestones, next steps, priority ordering, status by phase |
| Current health snapshot | `STATUS.md` | Short operational status and known gaps (if kept separate) |
| App behavior and user flows | `APP_FLOW.md` | UI flows, panel behavior, user journeys |
| Architecture and module boundaries | `docs/ARCHITECTURE.md` | Frontend/backend structure, data flow, runtime events |
| State semantics and UI states | `docs/STATE_MANAGEMENT.md` | State model, loading/error/empty state conventions |
| Dev setup and test commands | `docs/DEVELOPMENT.md` | Prereqs, local run, smoke/unit test workflow, platform deps |
| Technical decisions (ADR-lite) | `docs/DECISIONS.md` | Decision, status, rationale, open questions |
| Documentation contradiction log | `docs/DOC_SYNC_CONFLICTS.md` | Found conflicts, resolutions, and discussion-required items |
| Release history | `CHANGELOG.md` | User-visible changes per version/release |
| Contributor process | `CONTRIBUTING.md` | PR process, commit standards, workflow expectations |
| Licenses and third-party notices | `THIRD_PARTY_NOTICES.md` | License text, attribution, legal notices |
| Legacy/transitional plan docs | `TRANSCRIBE_PLAN.md` | Historical plan context; do not add new roadmap items here |
| Wiki navigation pages | `docs/wiki/*.md` | Link hub only; no canonical technical content |

## 3) Anti-duplication rules

- `ROADMAP.md` is the single source for "what is next".
- `docs/ARCHITECTURE.md` is the single source for "how it is built".
- `docs/DEVELOPMENT.md` is the single source for "how to run/test/build".
- Wiki pages must link to canonical files, not fork their content.
- If a section is copied from one file to another, replace one side with a link.

## 4) Update workflow (required)

1. Change the canonical file first.
2. Update dependent references (`README.md`, wiki links, cross-links).
3. Keep "Last updated" accurate in the changed canonical file.
4. In the commit message, include:
   - what changed
   - why it changed
   - verification result

## 5) Naming and format conventions

- Use clear, stable file names.
- Keep one responsibility per file.
- Keep checklists actionable and short.
- Prefer links over copy-paste.
- Use English for technical docs unless a file is explicitly localized.

## 6) If unsure where to put something

Default order:
1. `ROADMAP.md` (planning)
2. `docs/DECISIONS.md` (decision + rationale)
3. `docs/ARCHITECTURE.md` (implementation shape)
4. `docs/DEVELOPMENT.md` (execution/testing)

If it still does not fit, add a short note in `docs/DECISIONS.md` and link to the chosen target file.
