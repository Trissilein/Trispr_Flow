# GitHub Copilot Instructions — Trispr Flow

## PR descriptions

When drafting or reviewing a PR description, always follow the template in `.github/pull_request_template.md`.
The four required sections are:

- **What & Why** — one paragraph, intent + motivation, link to issue/ROADMAP if applicable
- **Decision record** — link to `project-spec/decisions/YYYY-MM-DD/slug.md`, or `n/a`
- **Test evidence** — paste last lines of `npm test` output or link to CI run; if no tests exist, say why
- **Files of note** — only files where intent is non-obvious to a reviewer

Do not omit sections. Do not write "see commit messages" as a substitute for *What & Why*.

## Commit messages

Follow conventional commits (`type(scope): subject`). Body lines should be complete sentences.
The scope should name the domain area, not the file (`refactoring`, `overlay`, `audio`, `tts`, etc.).

## Repo conventions

- All files use **LF** line endings. Never commit CRLF.
- Do not run Git commands from WSL. Use a native Windows shell.
- Do not use `overlay.ts` — it was removed. Overlay is controlled via `window.eval()` from Rust.
- See `CLAUDE.md` and `CONTEXT.md` for architecture rules and domain language.
