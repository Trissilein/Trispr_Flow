# GitHub Copilot Instructions — Trispr Flow

## Commit messages

Follow conventional commits (`type(scope): subject`). Body lines should be complete sentences.
The scope should name the domain area, not the file (`refactoring`, `overlay`, `audio`, `tts`, etc.).

## Repo conventions

- All files use **LF** line endings. Never commit CRLF.
- Do not run Git commands from WSL. Use a native Windows shell.
- Do not use `overlay.ts` — it was removed. Overlay is controlled via `window.eval()` from Rust.
- See `CLAUDE.md` and `CONTEXT.md` for architecture rules and domain language.

## Delegated agent kickoff

Checkpoint handoffs must be self-contained enough for agents that cannot read user-level or Hub skills.

When creating or using a checkpoint under `project-spec/checkpoints/`:

- Include a `## Kickoff Prompt` section when another agent is expected to continue the work.
- The kickoff prompt must link the checkpoint, state the task, name acceptance criteria or verification, and state commit authority.
- Tell delegated agents to read the checkpoint fully, verify branch and working-tree state, isolate unrelated changes, and then decide whether to proceed or clarify.
- Use `/grill-with-docs` for checkpoint, ADR, `CONTEXT.md`, release-policy, stale-doc, domain-language, or cross-file design uncertainty.
- Use `/grill-me` for pure plan, implementation, or debugging uncertainty.
- If unsure whether uncertainty affects correctness, scope, release safety, docs, or implementation path, clarify before editing.
- Commit only when the kickoff prompt explicitly says `fix and commit`, `commit when done`, or equivalent.
