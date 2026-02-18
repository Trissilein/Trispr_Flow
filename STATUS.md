# Trispr Flow - Status

Last updated: 2026-02-18

## Snapshot

- **Current release**: `v0.6.0` (released 2026-02-16)
- **Current planning phase**: `v0.7.0` Block F complete; Block A + B complete; Block C in progress
- **Canonical next steps**: see `ROADMAP.md`

## Working State

- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Voice Analysis UI flow exists (file picker, progress dialog, result rendering, error surfacing).
- Analyse click now runs guided setup preflight (runtime check + storage hint) and can launch setup with live progress updates.
- Sidecar runtime supports two modes:
  - bundled executable (if present)
  - Python fallback (`main.py`) with per-user venv preferred
- Installer bundles sidecar Python files plus `setup-vibevoice.ps1` for optional dependency setup.
- NSIS Voice Analysis setup flow is unified across CUDA/Vulkan and soft-fail by default.
- In-app Voice Analysis failure dialog exposes `Install Voice Analysis` and retries analysis after successful install.

## Known Gaps

- Guided first-use setup path is shipped for Windows (`Analyse` preflight + storage hint + setup progress); richer wizard polish can follow later.
- Model/runtime edge cases still need hardening, but key checks are now in place (runtime dependency/version validation for VibeVoice-ASR, prefetch disk guardrail, and clearer fallback messaging).
- Release-vs-dev source discovery split (D3) is implemented: local `VibeVoice` source auto-discovery defaults ON in dev, OFF in release, with explicit override support.
- Some legacy docs still contain historical assumptions; `ROADMAP.md` is now the source of truth for priority and sequencing.
- Policy decisions D1-D4 are locked; Block A/B are complete, C/D remain.

## Privacy + Network Notes

- Hugging Face is used for model/tokenizer downloads and cache checks.
- User audio for Voice Analysis is processed locally by the sidecar and is **not uploaded to Hugging Face**.
- Missing `HF_TOKEN` is now surfaced as a single informational runtime note (rate-limit hint), not a repeated warning.

## Next Focus

1. Block C: Voice Analysis retry/reset and first-use guided setup UX.
2. Block D: v0.7.0 AI Fallback implementation.
