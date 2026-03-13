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

## Housekeeping (Required Before Push)
Housekeeping is the mandatory end-of-session workflow before every push. The goal is to keep
repository state clean, track progress in docs, and isolate maintenance work in its own commit.

1. Review current context:

```bash
git log --oneline -n 10
git status -sb
```

2. Sync progress documentation:
- Update `STATUS.md`.
- Update `CHANGELOG.md`.
- Update any affected task/planning docs (for example `ROADMAP.md`, `docs/TASK_SCHEDULE.md`).

3. Run a cleanup audit:
- Check for build/image/temp artifacts and keep generated files out of commits.
- Check for deprecated files, folders, or legacy paths.

4. Deprecated-file safety rule:
- Mark deprecated candidates first (docs/status note), do not delete immediately.
- Delete only after explicit confirmation.

5. Commit order:
- Commit feature/work changes first.
- Create a separate housekeeping commit with `chore(housekeeping): <scope>`.

6. Push gate:
- Run `git status -sb` again.
- Push only when the final state is clean and matches the intended commit set.
