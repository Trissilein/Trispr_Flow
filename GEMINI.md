# Trispr Flow — Instructional Context

This file provides foundational context and instructions for AI agents working on the Trispr Flow codebase.

## Project Overview

Trispr Flow is a professional-grade desktop dictation application for Windows (macOS planned) built with **Tauri 2**, **Rust**, and **TypeScript**. It specializes in GPU-accelerated offline transcription and system audio capture with local AI refinement.

### Architecture

- **Backend (Rust)**: Located in `src-tauri/`. Handles audio capture (WASAPI), transcription (whisper.cpp), model management, and system-level integrations.
- **Frontend (TypeScript)**: Located in `src/`. Built with Vite and modern TypeScript. Uses a modular approach for DOM manipulation, event handling, and state management.
- **Overlay**: A standalone `overlay.html` with inline JS, controlled from Rust via `window.eval()`.
- **Managed Modules**: A platform for optional features like GDD automation and workflow agents.
- **AI Refinement**: Local-first refinement using Ollama (Qwen models) and Piper (TTS).

## Core Technologies

- **Tauri 2**: Framework for building desktop apps with web technologies.
- **Rust**: Systems programming for high-performance audio and transcription.
- **Whisper.cpp**: High-performance Whisper inference (CUDA/Vulkan).
- **Ollama**: Local AI model runtime for refinement.
- **Piper**: Local neural voice engine for TTS.
- **FFmpeg**: Bundled for OPUS encoding of audio recordings.

## Building and Running

### Development
```powershell
npm install
npm run dev          # Starts Tauri in release mode for performance
npm run dev:web      # Starts Vite dev server only
```

### Testing
```powershell
npm run test         # Unit tests (Vitest)
npm run test:smoke   # Full build + Rust tests + Cargo build check
```

### Benchmarks
```powershell
npm run benchmark:latency        # Transcription latency (p50/p95)
npm run benchmark:latency:live   # Live transcription benchmark
```

### Building Installers
- `scripts/windows/build-installers.bat`: Builds all installer variants (`vulkan`, `cuda-lite`, `cuda-complete`).
- `scripts/windows/build_unified.bat`: Backward-compatible wrapper (now delegates to multi-variant build).
- `scripts/windows/rebuild-installer.bat`: Rebuild entrypoint + opens `installers/`.
- Root wrappers (`build_unified.bat`, `rebuild-installer.bat`, `build_installers.bat`) remain for compatibility.

## Development Conventions

### General Rules
- **Line Endings**: Use **LF** line endings (enforced via `.gitattributes`).
- **Git Operations**: **MUST** use a native Windows shell (PowerShell/CMD). Never use WSL for Git in this project.
- **State Management**: Avoid retry loops with cloned state in Rust to prevent stale-data overwrites.

### Frontend
- **Modularity**: Logic is split into focused TS modules (e.g., `history.ts`, `settings.ts`, `models.ts`).
- **DOM Access**: Centralized in `dom-refs.ts` using the `$` helper.
- **Events**: Tauri event listeners must be tracked and cleaned up using the `unlisten` pattern.
- **Styling**: Prefer **Vanilla CSS** in `styles.css` and `styles-modern.css`. Avoid TailwindCSS.

### Backend (Rust)
- **Settings Persistence**: Handled via `invoke("save_settings")` which triggers `save_settings_file` in `state.rs`.
- **Command Registration**: Commands must be registered in `src-tauri/src/lib.rs`.
- **Global State**: Managed in `AppState` (Mutex-protected) in `src-tauri/src/state.rs`.

### Multimodal I/O (Current Focus)
- **TTS**: Implementations in `multimodal_io.rs`. Supports Windows Native and Piper.
- **Vision**: Skeleton commands are present; implementation of real frame capture (Task N5) is the primary upcoming goal.

## Key Files

- `src-tauri/src/lib.rs`: Main Tauri command registration and application logic.
- `src-tauri/src/state.rs`: Backend state and settings data models.
- `src/main.ts`: Frontend entry point and initialization sequence.
- `src/dom-refs.ts`: Centralized DOM element references.
- `index.html`: Main application UI structure.
- `docs/APP_FLOW.md`: Detailed description of the application's logical flow.
- `ROADMAP.md`: Canonical source for project priorities and task status.
- `docs/TASK_SCHEDULE.md`: Detailed execution log and block-level task table.
