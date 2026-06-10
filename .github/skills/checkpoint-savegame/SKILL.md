---
name: checkpoint-savegame
description: 'Creates durable project checkpoints for agent handoff and resume by updating a dated checkpoint index, auditing relevant docs for stale or conflicting context, adding kickoff instructions for delegated agents, and delegating canonical ADR/spec edits to existing documentation skills. Use when the user asks for a savegame, checkpoint, handoff, resume point, session close summary, or to make project docs fresh enough for another agent to continue.'
argument-hint: '[topic or current work to checkpoint]'
---

# Checkpoint Savegame

Create a durable handoff point so another agent can resume work from the repository without relying on chat history or one agent's memory.

This skill owns checkpoint orchestration, completeness, and staleness checks. Canonical project docs, ADRs, and issue trackers remain the sources of truth; when they need edits, this skill routes that work through the owning workflow instead of duplicating their content in the checkpoint.

## Use When

Use this skill when the user asks to:

- create a savegame or checkpoint
- prepare a handoff or resume point
- close a session with durable context
- make project docs fresh enough for another agent to continue
- capture current progress, open questions, and next actions across multiple docs

Do not use this skill for a single ADR, one doc edit, or a normal status summary. Use `recording-decisions` for durable fact/ADR/doc edits and `verifying-against-spec` for checking one artifact against sources.

## Storage

Default checkpoint location:

- `project-spec/checkpoints/YYYY-MM-DD/<slug>.md`
- `project-spec/checkpoints/README.md` as the index

Use the current date for `YYYY-MM-DD`. Choose a short topic slug from the work being checkpointed.

Same slug means same logical work thread, not a Git branch. When creating a newer checkpoint with the same slug, mark older same-slug checkpoints as `superseded` in the index. Different slug means a parallel work thread and must not supersede another checkpoint. If the user explicitly requests that a different-slug checkpoint supersede an existing checkpoint, finish as `blocked` and ask the user to confirm slug alignment before proceeding, because the index schema does not support cross-slug supersession.

If `project-spec/` does not exist, ask where to store checkpoints or run the repository setup workflow first. Do not silently use agent memory as the primary store.

## Checkpoint Schema

Each checkpoint must include these sections:

```markdown
# <Topic> Checkpoint

Status: active | superseded | closed | blocked
Created: YYYY-MM-DD
Updated: YYYY-MM-DD
Slug: <slug>
Supersedes: <relative path or none>

## Scope

What work this checkpoint covers and what it does not cover.

## Sources Read

Repository files, ADRs, specs, issues, external docs, or memory sources consulted.

## Current Facts

Material facts needed to resume. Cite source files for material claims.

## Decisions

Decisions made, with ADR/spec links where they exist. Mark missing ADRs explicitly.

## Progress

What changed, what was verified, and what remains incomplete.

## Stale Or Conflicting Context

Docs, comments, specs, ADRs, issues, or memory entries that appear stale or conflict with the current facts.

## Open Questions

Unknowns that must not be guessed by the next agent.

## Next Actions

Concrete, ordered steps another agent can pick up.

## Kickoff Prompt

A compact prompt for a delegated agent to start from this checkpoint. It should invoke `checkpoint-kickoff`, link this checkpoint, state the task, name the grill rule, and state whether commits are authorized.

## Verification

What was checked, what passed, what failed, and what was not checked.
```

## Index Schema

Maintain `project-spec/checkpoints/README.md` with:

- a short purpose statement
- active checkpoints first
- superseded, closed, and blocked checkpoints grouped below active items
- each entry linking to the checkpoint file
- status, slug, date, one-line scope, and next action

Do not let the index become a second source of truth. It is a map, not the savegame itself.

## Workflow

1. Identify the topic and slug.
2. Read existing checkpoint index and same-slug checkpoints, if present.
3. Gather the relevant surface:
   - files touched or read during the current work
   - canonical project docs such as `CONTEXT.md`, `project-spec/`, and ADRs
   - roadmap, status, issue, or release docs named by the work
   - agent-accessible memory tools, such as persistent memory APIs or prior checkpoint files, only when the tool is explicitly available in the current execution environment and the stored entries are directly relevant to the topic being checkpointed
4. Separate facts, decisions, assumptions, conflicts, open questions, and next actions.
5. For each stale or missing canonical doc:
   - If the correction is unambiguous because the current fact is clearly documented in an authoritative source read during this session and the fix requires no new decisions, delegate the fix to `recording-decisions` and cite the result in the checkpoint.
   - If the fix would require a judgment call, new information not yet in sources, or user confirmation, add it to `Stale Or Conflicting Context` and plan to report `partial`.
   - Do not duplicate canonical doc content in the checkpoint in either case.
6. Create or update the dated checkpoint file.
7. Add a `Kickoff Prompt` that a delegated Copilot CLI or other coding agent can paste directly into a new session.
8. Update the checkpoint index.
9. Verify the checkpoint against the gathered sources.
10. Report one completion state.

## Kickoff Prompt Rules

The `Kickoff Prompt` must be short enough to paste into a delegated agent session and complete enough to prevent a cold start.

Include:

- `Use checkpoint-kickoff.`
- the checkpoint path or link
- the delegated task
- the grill rule: use `/grill-with-docs` for checkpoint, ADR, domain, release-policy, stale-doc, or cross-file design uncertainty; use `/grill-me` for pure plan or implementation uncertainty; if unsure whether uncertainty matters, grill
- commit authority, such as `fix and commit`, `fix but do not commit`, or `investigate only`

Do not include secrets, raw chat logs, or unrelated context in the kickoff prompt.

## Freshness Rules

Audit only the surface identified in Workflow step 3: files touched or read during the current work, canonical project docs, roadmap/status/issue/release docs named by the work, and explicitly available memory sources that are directly relevant. Do not expand the audit scope beyond what was gathered in that step, and do not claim the whole repository is fresh unless a whole-repo documentation audit was actually performed.

For material claims, include file-level evidence. If a claim is not supported by current sources, label it as an assumption or open question.

Use the stale-doc decision tree in Workflow step 5 for all canonical doc fixes.

## Delegation Boundaries

Use existing skills or their workflow rules when available:

- `recording-decisions` owns ADRs, durable facts, canonical doc edits, and stale-doc fixes.
- `verifying-against-spec` owns artifact-versus-source discrepancy checks.
- `grill-with-docs` owns design interrogation against domain docs while decisions are still being formed.
- `setup-spoke` owns creating a missing `project-spec/` structure.

This skill may summarize what those workflows changed, but must not turn the checkpoint into a duplicate ADR/spec/documentation archive.

## Sensitive Data

Never store secrets, credentials, tokens, private chat logs, or unnecessary personal data in repository checkpoints. Store sanitized operational context only.

If sensitive information is necessary to understand a decision, write a neutral placeholder and an access note such as `secret configured outside repo`.

## Completion Contract

End with exactly one state:

- `changed` — checkpoint and index were created or updated, and every stale or conflicting item discovered in the audited surface was either resolved through the owning workflow or explicitly determined to be non-material.
- `partial` — checkpoint and index were created or updated, but at least one material stale/conflicting item, unsafe canonical-doc fix, open decision, or missing verification remains listed.
- `no-op` — existing checkpoint and index already represent the current state.
- `blocked` — required sources, write access, storage path, or user decisions are missing.

Report changed files, superseded checkpoints, unresolved stale context, and the next action for the receiving agent.
