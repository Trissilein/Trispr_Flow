# Trispr Flow - Status

Last updated: 2026-02-18

## Snapshot

- **Current release**: `v0.6.0` (released 2026-02-16)
- **Current planning phase**: `v0.7.0` Block F complete; Block A complete; Block B in progress
- **Canonical next steps**: see `ROADMAP.md`

## Working State

- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Voice Analysis UI flow exists (file picker, progress dialog, result rendering, error surfacing).
- Sidecar runtime supports two modes:
  - bundled executable (if present)
  - Python fallback (`main.py`) with per-user venv preferred
- Installer bundles sidecar Python files plus `setup-vibevoice.ps1` for optional dependency setup.
- NSIS Voice Analysis setup flow is unified across CUDA/Vulkan and soft-fail by default.
- In-app Voice Analysis failure dialog exposes `Install Voice Analysis` and retries analysis after successful install.

## Known Gaps

- Guided first-use setup UX (size/storage/progress wizard on first Analyse click) is not fully shipped yet (D2).
- Model/runtime edge cases still need hardening (dependency pinning, prefetch guardrails, clearer fallback behavior).
- Release-vs-dev source discovery split (D3) is not complete yet.
- Some legacy docs still contain historical assumptions; `ROADMAP.md` is now the source of truth for priority and sequencing.
- Policy decisions D1-D4 are locked; Block A is complete, B/C remain.

## Privacy + Network Notes

- Hugging Face is used for model/tokenizer downloads and cache checks.
- User audio for Voice Analysis is processed locally by the sidecar and is **not uploaded to Hugging Face**.

## Next Focus

1. Block B: VibeVoice runtime/dependency hardening (incl. release-vs-dev source discovery policy).
2. Block C: Voice Analysis retry/reset and first-use guided setup UX.
3. Block D: v0.7.0 AI Fallback implementation.
