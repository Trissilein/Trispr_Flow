---
name: checkpoint-kickoff
description: 'Starts delegated agent work from a project checkpoint by reading the checkpoint, verifying repo state, deciding whether uncertainty warrants /grill-me or /grill-with-docs, and then proceeding with implementation only when scope and acceptance criteria are clear. Use when handing a checkpoint and task instructions to a Copilot CLI or other delegated coding agent.'
argument-hint: '[checkpoint path or link plus task instructions]'
---

# Checkpoint Kickoff

Use this skill when you are the delegated agent receiving a checkpoint and task instructions. The checkpoint is the durable source of context; this skill defines how to start safely, decide whether to grill, and proceed only when the work is clear enough.

This skill consumes checkpoints. It does not create savegames; use `checkpoint-savegame` for that.

## Inputs

You need:

- a checkpoint path or link
- the delegated task
- any explicit authority boundary, such as `fix`, `fix and commit`, `investigate only`, or `do not edit`

If the checkpoint is missing or unreadable, finish as `blocked` and ask for the checkpoint.

## Preflight

Before editing files:

1. Read the checkpoint in full.
2. Read any kickoff prompt or task text provided with it.
3. Inspect repository state using the environment's normal tools:
   - current branch and upstream
   - clean or dirty working tree
   - unrelated local changes
   - recently deleted or stale upstream branch warnings
4. Identify the likely source files, docs, commands, and evidence named by the checkpoint.
5. Restate the task in operational terms:
   - goal
   - non-goals
   - acceptance criteria
   - likely implementation path
   - verification plan
   - commit authority
6. Run the grill gate before making changes.

Do not inherit unrelated local changes unless the task explicitly says to use them.

## Grill Gate

Grill only for uncertainty that can affect correctness, scope, release safety, docs, user-visible behavior, data migration, architecture, or the implementation path.

Do not grill for irrelevant details, style preferences, or facts that can be answered by reading code or running a cheap command. Explore first when exploration can answer the question.

If you are unsure whether the uncertainty matters, grill. Safer to clarify than to implement the wrong thing.

Choose the grill mode:

- Use `/grill-with-docs` when uncertainty touches checkpoint facts, `CONTEXT.md`, ADRs, project-spec docs, release policy, domain terms, stale/conflicting docs, or cross-file design decisions.
- Use `/grill-me` when uncertainty is a pure plan, implementation, or debugging decision that does not need doc/domain grounding.

When grilling, ask one question at a time and include your recommended answer. Do not ask a bundle of speculative questions.

## Proceed Gate

Proceed without grilling only when all are true:

- the checkpoint and task agree on the goal
- acceptance criteria are clear enough to test
- no material open question blocks the implementation path
- repo state is understood and unrelated changes are isolated
- the chosen verification commands are known or can be discovered locally
- commit authority is clear

If any item is false and cannot be resolved by codebase exploration, grill or finish as `blocked`.

## Implementation

When proceeding:

1. Make the smallest focused change that satisfies the checkpoint task.
2. Preserve unrelated user changes.
3. Follow repo conventions and local instructions.
4. Prefer root-cause fixes over surface patches.
5. Keep docs in sync when the checkpoint or task explicitly requires it.

If new material decisions appear, pause and run the grill gate again before locking them in.

## Verification

Choose verification in this order:

1. Commands and evidence requested by the checkpoint.
2. Commands named by nearby docs, package scripts, or prior checkpoint verification.
3. The narrowest relevant test or reproduction command.
4. Full suite only when the task risk warrants it or the checkpoint asks for it.

If verification is blocked, too expensive, or unavailable, report the exact gap. Do not commit a fix that depends on unrun verification unless the kickoff explicitly allows it.

## Commit Rule

Commit only when the kickoff text explicitly says `fix and commit`, `commit when done`, or equivalent. Otherwise leave changes uncommitted and report them.

When committing, use the repository's commit message convention and include only task-related files.

## Checkpoint Update

For substantial work, update the checkpoint before finishing with:

- progress made
- changed files
- verification run and results
- remaining stale/conflicting context
- next actions

Skip checkpoint updates for tiny no-op checks, blocked starts, or investigations that produce no durable new context.

## Completion Contract

End with exactly one state:

- `grilling` - paused to ask `/grill-me` or `/grill-with-docs` questions.
- `changed` - implementation completed, verification acceptable, and changes are committed if explicitly authorized.
- `partial` - useful work completed, but verification, docs, checkpoint updates, or follow-up work remain.
- `no-op` - checkpoint and repo already satisfy the task.
- `blocked` - missing checkpoint, unsafe repo state, unclear authority, or unresolved ambiguity prevents progress.

Report the checkpoint used, the state, changed files, verification, commit hash if committed, and next action.
