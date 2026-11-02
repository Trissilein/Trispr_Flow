# Cloud fallback (Claude toggle)

The cloud fallback is optional and off by default. When enabled, Trispr Flow sends finalized audio chunks to a remote transcription endpoint instead of the local whisper.cpp backend. The endpoint returns plain text, which is then post-processed and pasted like local results.

## Why Claude is mentioned
Claude itself is not a speech-to-text engine. The cloud pipeline is expected to run an ASR model (e.g. Whisper) and may optionally use Claude for post-processing (punctuation, formatting, normalization, domain vocabulary). The desktop app stays agnostic by speaking to a single HTTP endpoint.

## Expected request
- **Method**: POST
- **Content-Type**: audio/wav (or multipart with metadata)
- **Body**: 16-bit PCM WAV, mono, 16 kHz (preferred)

## Expected response (JSON)
```
{
  "text": "Transcribed text here",
  "language": "de",
  "confidence": 0.92,
  "source": "cloud"
}
```

## Desktop behavior
- Toggle on/off is available in the UI and tray menu.
- When enabled, the history entries are tagged as `cloud`.
- The local backend remains the default, with CPU fallback available.

## Configuration (local dev)
- `TRISPR_CLOUD_ENDPOINT`: HTTP endpoint for the cloud pipeline.
- `TRISPR_CLOUD_TOKEN`: optional bearer token (Authorization header).

## Open questions
- Auth strategy (API key, OAuth, or local proxy).
- Data retention policy on the server.
- Streaming vs. batch upload.
