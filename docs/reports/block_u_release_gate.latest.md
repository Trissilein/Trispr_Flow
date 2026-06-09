# Block U Release Gate Report

Generated: 2026-06-09T14:48:10.932Z
Commit: 7dc89650386c8d94765ad82ddc2248fb4f38e7b1
Overall gate pass: yes

## Gate Summary

- Automated checks: pass
- Benchmark linkage: fail
- Soak evidence attached: no
- Soak required: no

## Warnings

- Latency benchmark report (bench/results/latest.json) missing.
- TTS report indicates release_gate_pass=false.
- 8h soak evidence not attached.
- 24h soak evidence not attached.

## Steps

- build: pass (4017 ms)
- test: pass (11165 ms)
- cargo-test-lib: pass (12514 ms)

## Bench Reports

- Latency report present: no
- TTS report present: yes
- TTS release gate pass: no
- TTS provider consistency: yes
- TTS uncategorized failures: 0

## Soak Evidence

- 8h soak: missing
- 24h soak: missing

