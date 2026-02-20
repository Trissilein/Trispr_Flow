# Trispr Flow - Status (Archived)

> Archive note: this file moved out of repo root during docs housekeeping on 2026-02-20.
> The canonical operational snapshot now lives in `ROADMAP.md` (`Canonical Current State`).

Last updated: 2026-02-19 (post roadmap-sync)

## Snapshot

- **Current release**: `v0.6.0` (released 2026-02-16)
- **Current planning phase**: `v0.7.0` execution mode (Block F + G complete, Block H open)
- **Canonical next steps**: see `ROADMAP.md`

## Working State

- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Analyse now launches an external Analysis Tool (no in-app runtime dependency bootstrap).
- Runtime installer download path has been removed from app launcher flow.
- Missing Analysis Tool now resolves via local EXE selection (`trispr-analysis.exe`) with path persistence.
- Dev builds support Python fallback (`analysis-tool/main.py`) when EXE is missing.
- System-audio session merge path now drops overlap prefixes between transcribe chunks to avoid duplicated audio at boundaries.
- Adaptive continuous dump is now unified across mic Toggle mode and system loopback (silence-aware flush + soft interval + hard cut).
- Continuous dump profiles and per-source overrides are available in Settings, with legacy chunk settings mapped for compatibility.
- Session chunk persistence is now source-specific (`mic` and `output`) to avoid cross-source finalize collisions.
- AI Fallback foundation (v0.7 Block G) is in place: provider architecture, settings migration (`ai_fallback` + `providers`), key storage flow, and Post-Processing config UI.
- Installer variants now target:
  - CUDA (base)
  - Vulkan (base)
  - CUDA+Analysis (bundled optional chain-install)

## Known Gaps

- `analysis-tool/` still needs full packaging into standalone release artifacts.
- CUDA+Analysis pipeline depends on local availability of `installers/Trispr-Analysis-Setup.exe`.
- AI Fallback provider calls are scaffolded/passthrough until Block H provider API integrations are completed.
- Some legacy docs still contain historical assumptions; `ROADMAP.md` is now the source of truth for priority and sequencing.

## Privacy + Network Notes

- App launcher does not download analysis installers from the network.
- Analysis processing remains local; no user audio upload is performed by Trispr launcher flow.

## Next Focus

1. Validate launcher UX end-to-end (missing EXE, remembered path, dev fallback).
2. Build and test all three installer variants, including CUDA+Analysis chain-install.
3. Block H execution:
   - Task 32 OpenAI integration
   - Task 33 Claude integration
   - Task 34 Gemini integration
   - Task 35 prompt strategy polish
   - Task 38 E2E.

## v0.7 Task State Snapshot

- ✅ Done: 31, 36, 37
- 🔵 Open: 32, 33, 34, 35, 38
