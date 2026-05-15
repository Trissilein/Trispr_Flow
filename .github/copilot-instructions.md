# GitHub Copilot Instructions — Trispr Flow

## Commit messages

Follow conventional commits (`type(scope): subject`). Body lines should be complete sentences.
The scope should name the domain area, not the file (`refactoring`, `overlay`, `audio`, `tts`, etc.).

## Repo conventions

- All files use **LF** line endings. Never commit CRLF.
- Do not run Git commands from WSL. Use a native Windows shell.
- Do not use `overlay.ts` — it was removed. Overlay is controlled via `window.eval()` from Rust.
- See `CLAUDE.md` and `CONTEXT.md` for architecture rules and domain language.
