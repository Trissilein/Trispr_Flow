# Voxtral TTS Decision Memo + Lab-POC Plan (Trispr Flow)

Last updated: 2026-03-27
Owner: Voice Output / Runtime
Status: Conditional Go (Lab)

## 1. Executive Summary

Decision outcome: **Conditional Go (Lab)** for a strict-local Voxtral TTS evaluation lane.

Scope of this decision:
- TTS first
- strict local runtime
- internal/lab only usage
- primary success metric: naturalness (while preserving interactive latency)

Not in scope for this decision:
- production default switch away from Piper
- commercial rollout
- cloud/API dependency as runtime requirement

Why Conditional Go:
- Voxtral TTS has a strong quality signal and explicit low-latency target.
- Local serving path exists (vLLM Omni + OpenAI-compatible speech endpoint).
- Open-weights TTS model currently carries **CC BY-NC 4.0** inheritance from voice references, so commercial suitability is unresolved and must be gated later.

## 2. Model, License, and Deployment Facts

### 2.1 Source-of-truth facts

- Mistral announced Voxtral TTS as a 4B multilingual TTS model with low-latency focus.
- Mistral reports support for 9 languages including German and English.
- Mistral reports model latency around 70 ms (model-side benchmark context) and API availability.
- Mistral indicates open weights on Hugging Face under CC BY-NC 4.0.
- Hugging Face model card (`mistralai/Voxtral-4B-TTS-2603`) states:
  - license: `cc-by-nc-4.0`
  - recommended local serving path: `vLLM Omni`
  - single-GPU requirement guideline: `>= 16 GB` VRAM

### 2.2 Practical implication for Trispr

- For current internal/lab objective, license is acceptable for evaluation.
- For any future external/commercial distribution, this exact weights package is **not auto-approvable** and needs a separate license/commercial gate.

## 3. Fit vs Current Trispr TTS Stack

Current stack baseline:
- Runtime-stable providers: `windows_native`, `windows_natural`, `local_custom` (Piper)
- Experimental benchmark provider: `qwen3_tts`
- Fallback execution path is already centralized and deterministic.

Fit assessment:
- Voxtral local serving can reuse the existing OpenAI-compatible `/v1/audio/speech` request shape already used by `qwen3_tts`.
- Main engineering risk is not protocol mismatch; it is local runtime footprint (GPU memory, startup, stability).
- German/English coverage aligns with current Trispr needs.

## 4. POC Implementation Specification (Decision-Complete)

This section defines implementation behavior for a lab-only provider lane.

### 4.1 New provider lane

Add an opt-in provider id:
- `voxtral_local_experimental`

Provider visibility rules:
- only visible when `voice_output_settings.voxtral_tts_enabled == true`
- surface classification: `benchmark_experimental`
- label: `Voxtral TTS (local, experimental)`

Default/fallback policy:
- never default on fresh install
- never auto-promote to primary
- explicit user opt-in only
- fallback remains existing chain (e.g. `voxtral_local_experimental -> local_custom -> windows_native` depending on user settings)

### 4.2 Settings additions (optional, backward-compatible)

Extend `voice_output_settings` with optional fields:
- `voxtral_tts_enabled: bool` (default `false`)
- `voxtral_tts_endpoint: string` (default `http://127.0.0.1:8000/v1/audio/speech`)
- `voxtral_tts_model: string` (default `mistralai/Voxtral-4B-TTS-2603`)
- `voxtral_tts_voice: string` (default `casual_male`)
- `voxtral_tts_response_format: string` (default `wav`; allowed: `wav|pcm|opus|flac|mp3`)
- `voxtral_tts_timeout_sec: number` (default `90`, clamp `3..300`)

Compatibility constraints:
- existing settings files without these fields stay valid
- no schema break for current providers

### 4.3 Runtime behavior

Request path:
- generate speech via OpenAI-compatible `/v1/audio/speech` endpoint
- include `{ input, model, voice, response_format }`
- decode/playback through existing audio pipeline

Error behavior:
- any endpoint/network/runtime error returns provider error
- existing fallback executor handles retry path to configured fallback provider
- keep emitting existing `tts:speech-*` events with `provider_used` and fallback metadata

### 4.4 Scope guardrails

Do not change in this POC:
- Piper daemon lifecycle
- installer payload or setup flow
- public command signatures
- default provider recommendation logic

## 5. Evaluation Matrix and Go/No-Go Gates

### 5.1 Test scenarios

Minimum scenarios:
1. Smoke: short/long DE+EN utterances synthesize repeatedly.
2. Quality A/B: blind internal comparison Voxtral vs Piper.
3. Latency: first call vs warm calls.
4. Stability: 100 sequential requests.
5. Failure/fallback: forced endpoint failure, fallback must recover.
6. Hardware envelope: at least one representative team GPU profile.

### 5.2 Quantitative pass criteria (Lab)

Quality gate (primary):
- blind A/B preference for Voxtral >= 60% on German set and >= 55% on English set
- or MOS delta >= +0.4 vs Piper on combined set

Latency gate (interactive):
- warm short-response p95 <= 1.2 s end-to-audible
- cold first-call p95 <= 8 s

Stability gate:
- >= 98% success over 100 sequential requests
- no deadlock/crash in TTS worker path

Resource gate:
- stable local serving on target lab GPU without repeated OOM
- no persistent degradation of existing non-Voxtral providers

### 5.3 Final recommendation mapping

- **Go (Lab)**: all gates pass.
- **Conditional Go (Lab)**: quality pass + one non-critical gate misses but mitigatable.
- **No-Go**: quality fails, or runtime instability is material.

## 6. Risks and Mitigations

Primary risks:
- NC license blocks commercial usage path.
- Local GPU memory/throughput can be inconsistent across developer machines.
- Endpoint process management is external to Trispr app lifecycle.

Mitigations:
- keep lane explicitly experimental and opt-in.
- retain Piper as production baseline.
- isolate rollout to lab builds and benchmark workflows.
- define mandatory commercial-license gate before any production enablement.

## 7. Immediate Next Steps

1. Implement `voxtral_local_experimental` provider lane behind `voxtral_tts_enabled`.
2. Add benchmark profile and run DE/EN A/B pack against Piper.
3. Publish lab report with measured metrics and gate result.
4. If Lab-Go: prepare separate commercialization decision (license-safe alternative or approved terms).

## 8. References

- Mistral announcement: https://mistral.ai/news/voxtral-tts
- Mistral TTS API docs (`voxtral-mini-tts-2603`): https://docs.mistral.ai/capabilities/audio/text_to_speech/speech
- Voxtral 4B TTS model card and license: https://huggingface.co/mistralai/Voxtral-4B-TTS-2603
