# N11 TTS Benchmark Guide

Last updated: 2026-03-21

> Planning note (2026-03-22): This document is retained unchanged as benchmark evidence guidance and is referenced by `Block S` stabilization work (`v0.7.3`) in `ROADMAP.md` / `docs/TASK_SCHEDULE.md`.

## Goal

Run a reproducible, gate-driven benchmark for TTS providers and derive a production default + deterministic fallback order from measured reliability and latency.

The harness is backend-only/headless and does not require `tauri dev` or any WebView window.

## Locked Scope and Matrix

- Providers in scope:
  - `windows_native` (runtime stable)
  - `windows_natural` (runtime stable; optional, requires SAPI natural voices)
  - `local_custom` (runtime stable, Piper)
  - `qwen3_tts` (benchmark-only experimental)
- Locked matrix (`lock_matrix=true` by default):
  - length: `short`, `long`
  - language: `de`, `en`
  - thermal: `cold`, `warm`
  - resulting scenario ids:
    - `short_de_cold`, `short_de_warm`
    - `short_en_cold`, `short_en_warm`
    - `long_de_cold`, `long_de_warm`
    - `long_en_cold`, `long_en_warm`
- Runs:
  - warmup: configurable (default `1`)
  - measured: configurable (default `3`, minimum `3`)

## Release Gates (Frozen)

- Reliability gate: `success_rate >= 95%`
- Latency targets:
  - `p50 <= 700ms`
  - `p95 <= 1500ms`
- Per-scenario success floor: minimum `2` successful measured runs in every scenario
- Runtime gate scope: only `runtime_stable` providers (`windows_native`, `local_custom`)
  - `windows_natural` participates only when explicitly included via `-Providers windows_natural,...`

`qwen3_tts` is benchmarked and reported, but explicitly marked experimental and excluded from production release gate evaluation.

## Preflight Checklist (Recorded in Report)

Before measured runs, each provider records pass/fail checks:

- `windows_native`:
  - platform check (`Windows required`)
  - synthesis probe
- `local_custom`:
  - binary check (`piper` resolvable)
  - model check (configured model or discoverable `.onnx`)
- `qwen3_tts`:
  - endpoint format check (`http/https`)
  - endpoint/auth probe check (small synthesis request)

Failure categories are normalized to:

- `missing_binary`
- `missing_model`
- `endpoint_unreachable`
- `auth_missing`
- `runtime_error`

## Execution

Standard run:

```powershell
npm run benchmark:tts
```

Strict run (fails on missing recommendation OR release-gate miss):

```powershell
npm run benchmark:tts:strict
```

Direct script usage:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tts-benchmark.ps1 -Runs 5 -Warmup 1 -Providers windows_native,local_custom,qwen3_tts
```

Optional overrides:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/tts-benchmark.ps1 `
  -Providers windows_native,local_custom,qwen3_tts `
  -Qwen3Endpoint http://127.0.0.1:8000/v1/audio/speech `
  -Qwen3Model Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice `
  -Qwen3Voice vivian `
  -Qwen3TimeoutSec 20 `
  -PiperBinaryPath D:\tools\piper\piper.exe `
  -PiperModelPath D:\tools\piper\voices\de_DE-thorsten-medium.onnx
```

Advanced flags:

- `-UnlockMatrix` currently only toggles metadata; custom scenario injection is not wired yet
- `-NoRuntimeSmoke` disables runtime speak smoke checks (default is enabled)
- `-FailOnGateMiss` fails script on `release_gate_pass=false`
- `-NoSaveExamples` disables WAV example export
- `-PlayExamples` plays generated examples after benchmark run
- `-PlayBlindExamples` plays blind samples (`sample_XX.wav`) after benchmark run

## Output Artifact

Report file:

- `bench/results/tts.latest.json`

Key fields:

- `artifact_version`, `generated_at`
- `gates`, `scenario_matrix_locked`
- `provider_profiles`
- `preflight_checks`
- `runtime_smoke_checks`
- `samples` with `failure_category`
- `provider_summaries`
- `provider_gate_evaluations`
- `fallback_order`
- `release_gate_pass`, `release_gate_reason`
- `recommended_default_provider`, `recommendation_reason`
- `uncategorized_failure_count` (must be `0`)
- `example_clips_dir`, `example_manifest_path`
- `blind_examples_dir`, `blind_mapping_path`

## Human Listening Evaluation

Every run (unless `-NoSaveExamples`) exports one WAV per `provider x scenario`:

- location: `bench/results/tts-samples/<timestamp>/`
- manifest: `examples.manifest.json`
- blind pack:
  - audio: `blind/sample_XX.wav`
  - mapping: `blind/blind-map.json`

Use the blind pack for subjective quality rating without provider bias.

## Decision and Rollout Rule

- Set default provider only from benchmark evidence in latest report.
- Use `fallback_order` as deterministic runtime fallback chain.
- Do not ship release if:
  - `release_gate_pass=false`, or
  - `uncategorized_failure_count > 0`, or
  - strict benchmark run fails.

## Notes

- Windows-first benchmark and release gate.
- Recommendation is machine-specific evidence; rerun on target hardware before changing defaults.
