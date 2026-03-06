# GDD Module Workflow

Last updated: 2026-03-06

This guide describes the current end-to-end workflow in Trisper Flow for generating and publishing Game Design Documents.

## Prerequisites

1. GDD flow is a core capability and always available from the `Modules` tab.
2. If publishing to Confluence is needed:
   - Configure Confluence auth (`OAuth` or `API token`).
   - Ensure `site_base_url` and (for OAuth) `oauth_cloud_id` are set.
   - Set a default space key if possible.
3. Optional automation path:
   - Enable module `workflow_agent` for wakeword-driven command orchestration.

## Flow

### Manual core flow

1. Open `Modules` tab and click `Open GDD Flow`.
2. Select a GDD preset (or run `Auto-detect Preset`).
3. Optional: load template guidance from:
   - Confluence page URL
   - File upload (`.pdf`, `.docx`, `.txt`, `.md`)
4. Click `Generate Draft`.
5. Click `Validate`.
6. Optional: click `Suggest Target` for Confluence routing hints.
7. Click `Publish to Confluence`.
8. Use the generated link to open the published page.

### One-click policy and queue fallback

1. If one-click publish is preferred, routing confidence is validated against the configured threshold.
2. Low-confidence routes require explicit confirmation before publish continues.
3. If Confluence is transiently unreachable (network/timeout/429/5xx), publish is queued locally with a bundle:
   - `draft.json`
   - `draft.md`
   - `draft.confluence.html`
   - `publish-request.json`
   - `manifest.json`
4. Pending queue jobs can be retried or deleted directly from the GDD flow.

### Workflow-agent flow (optional module)

1. Enable `workflow_agent` in `Modules`.
2. Speak or type a wakeword command (e.g. “Hey Trispr, create and publish a GDD from yesterday”).
3. Agent parses intent and returns candidate sessions (1-3).
4. Confirm selected session.
5. Select target language (always asked).
6. Build execution plan and confirm.
7. Execute -> draft generation -> publish or queue fallback.

## Notes

1. Template ingestion is used as guidance context; it does not replace preset schema enforcement.
2. `.doc` legacy Word format is not supported yet; use `.docx`.
3. Long template text is truncated for safety before prompt usage.
4. Agent execution uses a separate command channel (`transcription:raw-result`) and does not reuse the activation-word drop filter.
5. Queue fallback is for transient publish failures only; hard auth/validation failures are not auto-queued.

## Troubleshooting

1. `Could not extract Confluence page id from URL`:
   - Ensure the URL includes either `pageId=` or `/pages/<id>/...`.
2. `Confluence OAuth site is not selected yet`:
   - Re-run OAuth exchange and persist selected site.
3. `Could not extract readable text from PDF`:
   - PDF may be image-only. Use OCR or export text first.
