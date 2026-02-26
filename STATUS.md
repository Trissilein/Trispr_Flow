# Trispr Flow - Status

Last updated: 2026-02-26

## Snapshot

- Current release: `v0.7.0`
- Current planning phase: `v0.7.0 stabilization only`
- Canonical next steps: `ROADMAP.md`

## Working State

- Core capture/transcription pipeline is stable (PTT/VAD + system audio + export).
- Adaptive continuous dump is unified across mic Toggle mode and system loopback.
- Continuous dump profiles and per-source overrides are available in Settings.
- Session chunk persistence is source-specific (`mic` and `output`) to avoid finalize collisions.
- AI fallback foundation is in place in Post-Processing settings.
- Installer variants are now:
  - CUDA (base)
  - Vulkan (base)

## Analysis De-Scope

- Analysis launcher flow has been removed from Trispr Flow mainline.
- The `Analyse` button remains as a placeholder and shows a "coming soon" notice.
- Analysis runtime/packaging artifacts were removed from mainline.
- Dedicated analysis work moved to `analysis-module-branch`.

## Known Gaps

- Hands-on desktop/mobile UX QA remains manual and ongoing.
- Some historical planning docs still reference previous analysis experiments.

## Privacy + Network Notes

- Mainline runtime remains local-first for transcription.
- No analysis installer download path exists in Trispr Flow mainline.

## Next Focus

1. Lock `v0.7.0` baseline (no scope expansion during stabilization).
2. Run architecture review and produce prioritized findings.
3. Performance hardening for transcription -> refinement turnaround.
