# Analysis Module

Standalone analysis project for speaker-aware transcript enrichment.

## Intent
This module is intentionally decoupled from Trispr Flow mainline. Trispr Flow keeps a placeholder `Analyse` button, while all analysis runtime, UX, and release work moves here.

## Built With
- VibeVoice runtime (speaker-aware ASR)
- Python/Rust worker pipeline (implementation to be finalized)
- Web UI for upload, timeline, segment review, and export

## Target Capabilities
- Speaker-aware transcription from uploaded audio
- Segment timeline and speaker label editing
- Export as TXT, Markdown, and JSON
- Batch processing queue for multiple files

## Folder Layout
- `ANALYSIS_MODULE_SPEC.md`: consolidated implementation spec (single source for behavior/UI/API)
- `PRODUCT_SCOPE.md`: product boundaries and requirements
- `ARCHITECTURE.md`: runtime and integration architecture
- `ROADMAP.md`: incremental delivery plan
- `web-ui-template/`: baseline UI scaffold

## Integration Contract (Draft)
Trispr Flow integration is file-based and optional. No direct runtime coupling is required.

- Input: local audio file path or uploaded audio blob
- Output: normalized analysis JSON payload
- Ownership: analysis module lifecycle and release cadence are independent from Trispr Flow
