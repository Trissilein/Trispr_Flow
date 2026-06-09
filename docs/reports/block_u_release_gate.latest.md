# Block U Release Gate Report

Generated: 2026-06-09T16:27:44.556Z
Commit: 94be0bcadc1dafaf714ee7fab5a69e95e9536aca
Overall gate pass: yes

## Gate Summary

- Automated checks: pass
- Benchmark linkage: pass
- Soak evidence attached: no
- Soak required: no

## Warnings

- TTS supported optional providers degraded: local_custom.
- TTS experimental providers unavailable: qwen3_tts.
- 8h soak evidence not attached.
- 24h soak evidence not attached.

## Steps

- build: pass (4060 ms)
- test: pass (11402 ms)
- cargo-test-lib: pass (12490 ms)

## Bench Reports

- Latency report present: yes
- TTS report present: yes
- TTS release gate pass: yes
- TTS provider consistency: yes
- TTS degraded optional providers: local_custom
- TTS unavailable experimental providers: qwen3_tts
- TTS uncategorized failures: 0

## Soak Evidence

- 8h soak: missing
- 24h soak: missing

