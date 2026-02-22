# Trispr Flow — Project Conventions

## Line Endings

All files use **LF** line endings (enforced via `.gitattributes`). Never commit CRLF.

## Architecture

- **Tauri 2** desktop app: Rust backend (`src-tauri/`) + TypeScript frontend (`src/`)
- **Overlay**: Standalone `overlay.html` with inline JS, controlled via `window.eval()` from Rust
  - Do NOT use `overlay.ts` — it was removed (dead code)
  - Primary settings path: Rust → `window.eval()` → overlay JS functions
  - Backup path: Tauri events (`settings-changed`, `overlay:settings`)
- **Main window**: `index.html` + `src/main.ts` + `src/event-listeners.ts`

## Key Rules

- Do not introduce retry loops with cloned state in Rust — they cause stale-data overwrites
- Tauri event listeners must be tracked and cleaned up (use `unlisten` pattern)
- Settings are persisted via `invoke("save_settings")` which calls `apply_overlay_settings()` and emits `settings-changed`

## Environment Constraint

- Do not run Git commands from WSL for this project.
- Use a native Windows shell/session for all Git operations (status/commit/merge/push/branch/delete).
