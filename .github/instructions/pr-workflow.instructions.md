---
applyTo: .github/pull_request_template.md
---

# PR workflow — Trispr Flow

## Lifecycle

Open PRs as **drafts** while work is in progress. Mark "Ready for review" only when the branch is complete and CI (`Type-check + unit tests`) is green. This is the explicit signal for Ingo's agents to begin review. Do not open a ready PR for a half-finished slice.

## PR description

Always fill every section of the template in `.github/pull_request_template.md`.

- **What & Why** — one paragraph, intent + motivation, link to issue/ROADMAP if applicable
- **Decision record** — link to `project-spec/decisions/YYYY-MM-DD/slug.md`, or `n/a`
- **Test evidence** — paste last lines of `npm test` output or link to CI run; if no tests exist, say why
- **Files of note** — only files where intent is non-obvious to a reviewer

Do not omit sections. Do not write "see commit messages" as a substitute for *What & Why*.
