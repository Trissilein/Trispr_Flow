# Voice Analysis Runtime - QA Matrix (Block B)

This matrix validates Block B runtime hardening: dependency/runtime validation, cache/prefetch guardrails, and runtime logging UX.

| ID | Area | Preconditions | Action | Expected |
| --- | --- | --- | --- | --- |
| QB-1 | HF warning UX | Sidecar started without `HF_TOKEN` | Start Voice Analysis twice | First run logs one informational HF note; repeated warnings are not spammed at warning/error level |
| QB-2 | D3 source discovery (dev) | Dev build (`TRISPR_DEV_BUILD=1`), local `VibeVoice` checkout present | Start sidecar + run analysis | Local source auto-discovery is allowed; analysis can use local override path |
| QB-3 | D3 source discovery (release) | Release build (`TRISPR_DEV_BUILD=0`), local `VibeVoice` checkout present, no override env vars | Start sidecar + run analysis | Local source auto-discovery is disabled by default |
| QB-4 | D3 explicit override | Release build, set `VIBEVOICE_ALLOW_LOCAL_SOURCE=1` or `VIBEVOICE_SOURCE_DIR` | Start sidecar + run analysis | Local source override is accepted |
| QB-5 | Prefetch guardrail | Low free disk on HF cache drive | Run `setup-vibevoice.ps1 -PrefetchModel` | Setup exits with `E40` and remediation text (no ambiguous failure) |
| QB-6 | Runtime dependency validation | Force incompatible runtime package version(s) | Start sidecar and trigger analysis | Error includes specific version/runtime mismatch details and manual setup command |
| QB-7 | Runtime fallback messaging | Break native VibeVoice runtime modules | Trigger analysis | Error clearly states native runtime missing/incompatible and that fallback for `microsoft/VibeVoice-ASR` is disabled until repaired |
