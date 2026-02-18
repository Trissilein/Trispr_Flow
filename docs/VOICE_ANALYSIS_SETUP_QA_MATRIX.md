# Voice Analysis Setup - QA Matrix

Last updated: 2026-02-18

This matrix validates Block A (D1 + D4): optional setup, soft-fail installer, and clear remediation.

| ID | Variant | Preconditions | Action | Expected Result |
| --- | --- | --- | --- | --- |
| QA-A1 | CUDA installer | Python missing | Opt in to Voice Analysis during install | Installer offers python.org link; install completes; no hard fail |
| QA-A2 | CUDA installer | Python 3.11+ available | Opt in to Voice Analysis during install | `setup-vibevoice.ps1` runs; dependencies install; app usable |
| QA-A3 | CUDA installer | Python available, force pip failure (offline/index fail) | Opt in to Voice Analysis during install | Installer shows soft-fail message with manual PowerShell command; install completes |
| QA-A4 | Vulkan installer | Python missing | Opt in to Voice Analysis during install | Same behavior as QA-A1 |
| QA-A5 | Vulkan installer | Python 3.11+ available | Opt in to Voice Analysis during install | Same behavior as QA-A2 |
| QA-A6 | Vulkan installer | Python available, force pip failure | Opt in to Voice Analysis during install | Same behavior as QA-A3 |
| QA-A7 | In-app analysis | Dependencies missing | Click `Analyse` -> error dialog -> `Install Voice Analysis` | Backend setup runs; on success analysis auto-retries |
| QA-A8 | Setup script rerun | Existing venv already present | Run `setup-vibevoice.ps1` again | Script remains successful (idempotent) and exits `0` |

## Exit-Code Smoke Checks (`setup-vibevoice.ps1`)

- Python missing: non-zero (class `E10`)
- Unsupported Python version: non-zero (class `E12`)
- Requirements missing: non-zero (class `E13`)
- Venv/Pip failure: non-zero (class `E20+`/`E30+`)
- Prefetch failure (if enabled): non-zero (class `E40`)
- Success path: `0`
