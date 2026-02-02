# Contributing

Thanks for helping with Trispr Flow. Keep changes focused and easy to review.

## Quick start
```bash
npm install
npm run tauri dev
```

## Repo structure
- `src/` frontend (Vite + TS)
- `src-tauri/` backend (Rust, Tauri v2)
- `docs/` architecture and decisions

## Guidelines
- Prefer small, isolated changes with a clear purpose.
- Keep UI changes consistent with the current design system.
- Update `STATUS.md` and `docs/DECISIONS.md` when behavior changes.
- Avoid committing build artifacts (`dist/`, `node_modules/`, `src-tauri/target/`).

## Testing
- `npm run build` (frontend)
- `cargo check` (backend)
