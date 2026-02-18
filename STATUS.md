# Trispr Flow - Status

Last updated: 2026-02-18

## Snapshot

- **Current release**: `v0.6.0` (released 2026-02-16)
- **Current planning phase**: `v0.7.0` Block F complete; Block G/H pending
- **Canonical next steps**: see `ROADMAP.md`

## Working State

- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Voice Analysis UI flow exists (file picker, progress dialog, result rendering, error surfacing).
- Sidecar runtime supports two modes:
  - bundled executable (if present)
  - Python fallback (`main.py`) with per-user venv preferred
- Installer currently bundles sidecar Python files plus `setup-vibevoice.ps1` for optional dependency setup.

## Known Gaps

- Voice Analysis dependency path is not fully zero-touch in all setups yet (installer/setup hardening still ongoing).
- Model/runtime edge cases still need hardening (dependency pinning, prefetch guardrails, clearer fallback behavior).
- Retry/reset behavior after failed analysis needs additional UX hardening.
- Some legacy docs still contain historical assumptions; `ROADMAP.md` is now the source of truth for priority and sequencing.
- Policy decisions D1-D4 are locked; implementation is pending in Blocks A-C.

## Privacy + Network Notes

- Hugging Face is used for model/tokenizer downloads and cache checks.
- User audio for Voice Analysis is processed locally by the sidecar and is **not uploaded to Hugging Face**.

## Next Focus

1. Block A: installer and setup automation hardening.
2. Block B: VibeVoice runtime/dependency hardening (incl. release-vs-dev source discovery policy).
3. Block C: Voice Analysis retry/reset and first-use guided setup UX.
4. Block D: v0.7.0 AI Fallback implementation.
