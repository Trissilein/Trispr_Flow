# Contributing

Thanks for helping with Trispr Flow. Keep changes focused and easy to review.

## Quick start
```bash
npm install
npm run tauri dev
```

## Project structure
- `src/` frontend (Vite + TypeScript)
- `src-tauri/` backend (Rust + Tauri v2)
- `docs/` architecture, development setup, design decisions

## Guidelines
- Prefer small, isolated changes with a clear purpose.
- Keep UI changes consistent with the current design system.
- Update `STATUS.md` and `docs/DECISIONS.md` when behavior changes.
- Avoid committing build artifacts (`dist/`, `node_modules/`, `src-tauri/target/`).

## Testing checklist
Run before opening a PR:

```bash
npm run test
npm run test:smoke
```

`test:smoke` executes frontend production build plus Rust tests.

## WSL/Linux notes
If smoke tests fail on linker/pkg-config dependencies, install required packages from:

- `docs/DEVELOPMENT.md`

## Parallel work safety
If multiple agents or sessions run in parallel:

- Do not edit files currently under active review in another session.
- Commit only the files owned by your current task.
- Sync frequently with `git status -sb` before staging/committing.
