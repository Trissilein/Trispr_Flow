# Dependency Policy and Install-Check Matrix

Last updated: 2026-03-05

This document defines how runtime dependencies are handled in Trispr Flow and what is validated at app startup.

## Policy

1. Core transcription dependencies must be installer-bundled.
2. Large optional runtimes may be managed on demand (downloaded by the app).
3. Experimental modules can ship disabled by default, but must declare dependency behavior.
4. Startup preflight must surface blocking dependency failures clearly.

## Packaging Classes

- `Bundled`: included directly in NSIS installer resources.
- `Managed`: downloaded/installed by Trispr Flow at runtime.
- `External`: expected from OS/user environment.

## Install-Check Matrix

| Capability | Dependency | Class | Current Status | Startup Preflight Behavior |
| --- | --- | --- | --- | --- |
| Whisper transcription runtime | `whisper-cli` + `ggml*` + backend DLLs | Bundled | CUDA/Vulkan variant bundles are active | `error` if missing |
| Whisper model inference | selected `.bin` model file | Managed (user download/import) | Not bundled by design | `error` if selected model missing |
| Model optimization | `quantize.exe` | Bundled (best effort) | Bundled in current installers | `warning` if missing |
| OPUS encode/merge | `ffmpeg` | External/Bundled fallback | currently external unless user provides/bundles | `warning` if missing |
| Local AI runtime | Ollama binary + runtime tree | Managed | Installed per user on demand | `warning` if endpoint unreachable when local AI enabled |
| GDD publish | Confluence connectivity/auth | External service | Optional; user-configured | handled by publish flow and queue fallback |
| Workflow agent | command parsing/session search | Internal | Available; module gated | module health/dependency warnings |
| Vision input module | monitor metadata stream (no image persistence) | Internal (current state) | Available; module gated | module health/dependency warnings |
| Voice output module | Windows PowerShell/System.Speech | External (OS) | Windows-native path active; local custom placeholder | `error` if TTS enabled and PowerShell missing |
| Voice output `local_custom` | custom local TTS runtime | Planned | Placeholder in current build | `warning` when selected |

## Startup Preflight

Backend now performs a dependency preflight during app setup and emits:

- Event: `dependency:preflight`
- Command: `get_dependency_preflight_status`

The report includes:

- `overall_status`: `ok | warning | error`
- `blocking_count`
- `warning_count`
- itemized checks with `id`, `status`, `required`, `message`, `hint`

Frontend shows startup toasts for blocking and warning findings.

## Rules for New Modules (Vision/TTS/Proprietary)

For each new module dependency, define:

1. Packaging class (`Bundled`, `Managed`, `External`)
2. Install location and version pinning strategy
3. Healthcheck command (fast, local)
4. Failure mode (`error` vs `warning`)
5. User-facing recovery hint

If a proprietary runtime is introduced for screen vision input or custom TTS, it must be mapped in this matrix before release and covered by startup preflight checks.
